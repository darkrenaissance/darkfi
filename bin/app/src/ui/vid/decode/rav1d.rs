/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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
    Decoder as Rav1dDecoder, InloopFilterType, Picture as Rav1dPicture, PlanarImageComponent,
    Rav1dError, Settings as Rav1dSettings,
};
use std::{sync::Arc, time::Instant};

use crate::{
    gfx::{gfxtag, RenderApi, Renderer},
    ui::vid::{ivf::IvfStreamingDemuxer, Av1VideoData, YuvTextures},
    util::spawn_thread,
};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui:video::decode", $($arg)*); } }

/// Spawn the decoder thread (Thread 2 of 2)
///
/// This thread decodes AV1 frames and creates GPU textures directly.
///
/// # Thread Coordination
/// - Uses optimistic decoding strategy:
///   1. Drain all pending frames with `try_recv()`
///   2. When queue is empty, block on `recv()` for next frame
///   3. Repeat - this minimizes latency by never waiting when data is available
/// - Creates textures directly via renderer and stores in vid_data
/// - Triggers draw updates when frames are ready
/// - On channel close, flushes decoder
///
/// # Arguments
/// * `vid_data` - Shared video data storage to update with textures
/// * `renderer` - Render API for creating textures
/// * `dc_key` - Draw call key for triggering updates
///
/// # Returns
/// JoinHandle for the spawned thread
pub fn spawn_decoder_thread(
    path: String,
    vid_data: Arc<SyncMutex<Option<Av1VideoData>>>,
    renderer: Renderer,
) -> std::thread::JoinHandle<()> {
    let mut settings = Rav1dSettings::new();
    // 0 is auto detect
    settings.set_n_threads(4);
    // 0 is auto
    settings.set_max_frame_delay(1);
    settings.set_apply_grain(false);
    settings.set_inloop_filters(InloopFilterType::empty());

    let mut decoder = Rav1dDecoder::with_settings(&settings).unwrap();
    //let mut decoder = Rav1dDecoder::new().unwrap();

    let data = Arc::new(SyncMutex::new(None));
    let data2 = data.clone();
    miniquad::fs::load_file(&path, {
        move |res| match res {
            Ok(chunk) => *data2.lock() = Some(chunk),
            Err(err) => {
                error!("Failed to load chunk: {err}");
            }
        }
    });
    let data = std::mem::take(&mut *data.lock()).unwrap();
    d!("Decoding video file: {path}");

    let mut demuxer = IvfStreamingDemuxer::from_first_chunk(data).unwrap();
    let num_frames = demuxer.header.num_frames as usize;

    *vid_data.lock() = Some(Av1VideoData::new(num_frames, &renderer));

    spawn_thread("video-decoder", move || {
        let now = Instant::now();
        let mut frame_idx = 0;
        loop {
            let Some(av1_frame) = demuxer.try_read_frame() else {
                // Channel closed - drain decoder (like dav1dplay)
                while let Ok(pic) = decoder.get_picture() {
                    if process(&mut frame_idx, &pic, &vid_data, &renderer).is_err() {
                        d!("Video stopped, exiting decoder thread");
                        return;
                    }
                }

                d!("Finished decoding video: {path} in {:?}", now.elapsed());
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
                    Ok(pic) => {
                        if process(&mut frame_idx, &pic, &vid_data, &renderer).is_err() {
                            d!("Video stopped, exiting decoder thread");
                            return;
                        }
                    }
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
                    if process(&mut frame_idx, &pic, &vid_data, &renderer).is_err() {
                        d!("Video stopped, exiting decoder thread");
                        return;
                    }
                }
            }
        }
    })
}

fn process(
    frame_idx: &mut usize,
    pic: &Rav1dPicture,
    vid_data: &SyncMutex<Option<Av1VideoData>>,
    renderer: &Renderer,
) -> Result<(), ()> {
    // rav1d stores data as planar GBR (Y=G, U=B, V=R)
    let y_plane = pic.plane_data(PlanarImageComponent::Y);
    let u_plane = pic.plane_data(PlanarImageComponent::U);
    let v_plane = pic.plane_data(PlanarImageComponent::V);

    let y_stride = pic.stride(PlanarImageComponent::Y) as usize;
    let u_stride = pic.stride(PlanarImageComponent::U) as usize;
    let v_stride = pic.stride(PlanarImageComponent::V) as usize;

    let width = pic.width() as usize;
    let height = pic.height() as usize;

    // Y plane is full resolution
    let y_data = copy_plane(&y_plane, y_stride, width, height);

    // U and V planes are half resolution (4:2:0 subsampling)
    let uv_width = width / 2;
    let uv_height = height / 2;
    let u_data = copy_plane(&u_plane, u_stride, uv_width, uv_height);
    let v_data = copy_plane(&v_plane, v_stride, uv_width, uv_height);

    // Create 3 separate textures with Alpha format (1 byte per pixel)
    let tex_y = renderer.new_texture(
        width as u16,
        height as u16,
        y_data,
        TextureFormat::Alpha,
        gfxtag!("video_y"),
    );

    let tex_u = renderer.new_texture(
        uv_width as u16,
        uv_height as u16,
        u_data,
        TextureFormat::Alpha,
        gfxtag!("video_u"),
    );

    let tex_v = renderer.new_texture(
        uv_width as u16,
        uv_height as u16,
        v_data,
        TextureFormat::Alpha,
        gfxtag!("video_v"),
    );

    let yuv_texs = YuvTextures { y: tex_y, u: tex_u, v: tex_v };

    let num_frames = {
        // Store in vid_data
        let mut vd_guard = vid_data.lock();
        let vd = vd_guard.as_mut().ok_or(())?;
        vd.textures[*frame_idx] = Some(yuv_texs.clone());
        let _ = vd.textures_pub.try_broadcast((*frame_idx, yuv_texs));
        vd.textures.len()
    };
    if (*frame_idx % 10) == 0 {
        let pct_loaded = 100. * *frame_idx as f32 / num_frames as f32;
        d!("Decoded video {pct_loaded:.2}%%");
    }
    *frame_idx += 1;
    Ok(())
}

/// Copy plane data row by row to handle stride padding.
/// When stride > width, the decoder adds padding bytes for alignment.
/// We need to copy only the actual pixel data, excluding the padding.
fn copy_plane(plane: &[u8], stride: usize, width: usize, height: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(width * height);
    for row in 0..height {
        let start = row * stride;
        let end = start + width;
        data.extend_from_slice(&plane[start..end]);
    }
    data
}
