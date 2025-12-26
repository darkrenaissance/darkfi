/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use miniquad::TextureFormat;
use parking_lot::Mutex as SyncMutex;
use rav1d::{
    Decoder as Rav1dDecoder, Picture as Rav1dPicture, PlanarImageComponent, Rav1dError,
    Settings as Rav1dSettings,
};
use std::sync::{
    mpsc::{Receiver, Sender},
    Arc,
};

use crate::{
    gfx::{gfxtag, RenderApi},
    util::spawn_thread,
};

use super::{ivf::IvfStreamingDemuxer, Av1VideoData};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui:video", $($arg)*); } }
macro_rules! w { ($($arg:tt)*) => { warn!(target: "ui:video", $($arg)*); } }

/// Spawn the loader-demuxer thread (Thread 1 of 2)
///
/// This thread loads video file chunks sequentially and demuxes AV1 frames.
///
/// # Thread Coordination
/// - Loads chunks from disk using format `path.{000, 001, 002, ...}` where `{frame}` is replaced
/// - Demuxes IVF container to extract raw AV1 frames
/// - Initializes vid_data with header info from first chunk
/// - Sends frames to decoder thread via `frame_tx` channel
/// - Signals completion by dropping `frame_tx`
///
/// # Arguments
/// * `path` - Base path with `{frame}` placeholder, e.g. `"assets/video.ivf.{frame}"`
/// * `frame_tx` - Channel sender for raw AV1 frames to decoder thread
/// * `vid_data` - Shared video data storage to initialize
/// * `render_api` - Render API for creating animation
///
/// # Returns
/// JoinHandle for the spawned thread
pub fn spawn_loader_demuxer_thread(
    path: String,
    frame_tx: Sender<Vec<u8>>,
    vid_data: Arc<SyncMutex<Option<Av1VideoData>>>,
    render_api: RenderApi,
) -> std::thread::JoinHandle<()> {
    spawn_thread("video-loader-demuxer", move || {
        let mut chunk_idx: usize = 0;
        let mut demuxer: Option<IvfStreamingDemuxer> = None;

        loop {
            // Replace {frame} placeholder with zero-padded chunk number
            let chunk_path = path.replace("{frame}", &format!("{chunk_idx:03}"));
            d!("Loading video chunk: {chunk_path}");

            // Load chunk asynchronously via miniquad callback
            let data = Arc::new(SyncMutex::new(None));
            let data2 = data.clone();
            miniquad::fs::load_file(&chunk_path, {
                let chunk_path = chunk_path.clone();
                move |res| match res {
                    Ok(chunk) => *data2.lock() = Some(chunk),
                    Err(err) => {
                        error!("Failed to load chunk {chunk_path}: {err}");
                    }
                }
            });
            let data = std::mem::take(&mut *data.lock());

            // Empty data means file not found - end of chunk sequence
            let Some(data) = data else {
                // Close channel to signal decoder thread
                drop(frame_tx);
                d!("Video demuxer finished");
                return
            };

            if let Some(demuxer) = demuxer.as_mut() {
                demuxer.feed_data(data);
            } else {
                // First chunk: initialize demuxer from IVF header
                let dem = IvfStreamingDemuxer::from_first_chunk(data).unwrap();
                let num_frames = dem.header.num_frames as usize;
                demuxer = Some(dem);

                // Initialize vid_data with header info
                *vid_data.lock() = Some(Av1VideoData::new(num_frames, &render_api));
            }

            let demuxer = demuxer.as_mut().unwrap();
            // Extract all complete frames from this chunk
            while let Some(frame) = demuxer.try_read_frame() {
                frame_tx.send(frame).unwrap();
                d!("Sent video chunk {chunk_idx}");
            }

            chunk_idx += 1;
        }
    })
}

/// Spawn the decoder thread (Thread 2 of 2)
///
/// This thread decodes AV1 frames and creates GPU textures directly.
///
/// # Thread Coordination
/// - Receives raw AV1 frames from loader-demuxer thread via `frame_rx` channel
/// - Uses optimistic decoding strategy:
///   1. Drain all pending frames with `try_recv()`
///   2. When queue is empty, block on `recv()` for next frame
///   3. Repeat - this minimizes latency by never waiting when data is available
/// - Creates textures directly via render_api and stores in vid_data
/// - Triggers draw updates when frames are ready
/// - On channel close, flushes decoder
///
/// # Arguments
/// * `frame_rx` - Channel receiver for raw AV1 frames from loader-demuxer thread
/// * `vid_data` - Shared video data storage to update with textures
/// * `render_api` - Render API for creating textures
/// * `dc_key` - Draw call key for triggering updates
///
/// # Returns
/// JoinHandle for the spawned thread
pub fn spawn_decoder_thread(
    frame_rx: Receiver<Vec<u8>>,
    vid_data: Arc<SyncMutex<Option<Av1VideoData>>>,
    render_api: RenderApi,
) -> std::thread::JoinHandle<()> {
    spawn_thread("video-decoder", move || {
        let mut settings = Rav1dSettings::new();
        // 0 is auto detect
        settings.set_n_threads(4);
        // 0 is auto
        settings.set_max_frame_delay(0);

        let mut decoder = Rav1dDecoder::with_settings(&settings).unwrap();
        //let mut decoder = Rav1dDecoder::new().unwrap();

        let mut frame_idx = 0;
        loop {
            // Blocking receive - returns Err when channel closes
            let Ok(av1_frame) = frame_rx.recv() else {
                // Channel closed - drain decoder (like dav1dplay)
                while let Ok(pic) = decoder.get_picture() {
                    process(&mut frame_idx, &pic, &vid_data, &render_api);
                }

                assert_eq!(frame_idx, vid_data.lock().as_ref().unwrap().textures.len());
                return;
            };

            let mut try_again = match decoder.send_data(av1_frame, None, None, None) {
                Ok(()) => false,
                Err(Rav1dError::TryAgain) => true,
                Err(_) => continue,
            };

            // Try to get decoded pictures
            loop {
                match decoder.get_picture() {
                    Ok(pic) => process(&mut frame_idx, &pic, &vid_data, &render_api),
                    Err(Rav1dError::TryAgain) => {
                        try_again = true;
                        break
                    }
                    Err(_) => break,
                }
            }

            // If we have pending data, retry sending it
            if try_again {
                while let Err(Rav1dError::TryAgain) = decoder.send_pending_data() {
                    let Ok(pic) = decoder.get_picture() else { continue };
                    process(&mut frame_idx, &pic, &vid_data, &render_api);
                }
            }
        }
        d!("Video decode finished, total frames: {frame_idx}");
    })
}

fn process(
    frame_idx: &mut usize,
    pic: &Rav1dPicture,
    vid_data: &SyncMutex<Option<Av1VideoData>>,
    render_api: &RenderApi,
) {
    // rav1d stores data as planar GBR (Y=G, U=B, V=R)
    let g_plane = pic.plane(PlanarImageComponent::Y);
    let b_plane = pic.plane(PlanarImageComponent::U);
    let r_plane = pic.plane(PlanarImageComponent::V);

    let g_stride = pic.stride(PlanarImageComponent::Y) as usize;
    let b_stride = pic.stride(PlanarImageComponent::U) as usize;
    let r_stride = pic.stride(PlanarImageComponent::V) as usize;

    let width = pic.width() as usize;
    let height = pic.height() as usize;

    let mut buf = Vec::with_capacity(width * height * 3);
    // Pack planar RGB into RGB format
    for y in 0..height {
        for x in 0..width {
            let g_idx = y * g_stride + x;
            let b_idx = y * b_stride + x;
            let r_idx = y * r_stride + x;

            buf.push(r_plane[r_idx]);
            buf.push(g_plane[g_idx]);
            buf.push(b_plane[b_idx]);
        }
    }

    // Create texture with RGB data
    let tex = render_api.new_texture(
        width as u16,
        height as u16,
        buf,
        TextureFormat::RGB8,
        gfxtag!("video"),
    );

    {
        // Store in vid_data
        let mut vd_guard = vid_data.lock();
        let vd = vd_guard.as_mut().unwrap();
        vd.textures[*frame_idx] = Some(tex.clone());
        let _ = vd.textures_pub.try_broadcast((*frame_idx, tex));
    }
    //d!("Loaded video frame {frame_idx}");
    *frame_idx += 1;
}
