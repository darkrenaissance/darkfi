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

//! Android-specific video decoding using MediaCodec

use miniquad::TextureFormat;
use parking_lot::Mutex as SyncMutex;
use std::{
    sync::{mpsc, Arc},
    thread,
};

use crate::{
    android::vid::{self, DecodedFrame},
    gfx::{gfxtag, Renderer},
    ui::vid::{Av1VideoData, YuvTextures},
    util::spawn_thread,
};

macro_rules! d { ($($arg:tt)*) => { debug!(target: "ui:video::decode", $($arg)*); } }

pub fn spawn_decoder_thread(
    path: String,
    vid_data: Arc<SyncMutex<Option<Av1VideoData>>>,
    renderer: Renderer,
) -> thread::JoinHandle<()> {
    *vid_data.lock() = Some(Av1VideoData::new(150, &renderer));

    spawn_thread("video-decoder-android", move || {
        let now = std::time::Instant::now();
        d!("Decoding MP4 video file: {path}");

        let (frame_tx, frame_rx) = mpsc::channel::<DecodedFrame>();

        let decoder_id = vid::register(frame_tx);

        let Some(decoder_handle) = vid::videodecoder_init(&path) else {
            error!(target: "ui:video::decode", "Failed to initialize MediaCodec decoder for: {path}");
            return;
        };

        vid::videodecoder_set_id(decoder_handle.obj, decoder_id);

        let _decoded_count = vid::videodecoder_decode_all(decoder_handle.obj);

        drop(decoder_handle);

        let mut frame_idx = 0;
        while let Ok(frame) = frame_rx.recv() {
            if process_frame(frame_idx, frame, &vid_data, &renderer).is_err() {
                d!("Video stopped, exiting decoder thread");
                return;
            }
            frame_idx += 1;

            if (frame_idx % 10) == 0 {
                let pct_loaded = 100. * frame_idx as f32 / 150.0;
                d!("Decoded video {pct_loaded:.2}%%");
            }

            assert!(frame_idx <= 150);
            if frame_idx == 150 {
                break
            }
        }

        d!("Finished decoding video: {path} in {:?}", now.elapsed());

        vid::unregister(decoder_id);
    })
}

fn process_frame(
    frame_idx: usize,
    frame: DecodedFrame,
    vid_data: &SyncMutex<Option<Av1VideoData>>,
    renderer: &Renderer,
) -> Result<(), ()> {
    let uv_width = frame.width / 2;
    let uv_height = frame.height / 2;

    let tex_y = renderer.new_texture(
        frame.width as u16,
        frame.height as u16,
        frame.y_data,
        TextureFormat::Alpha,
        gfxtag!("video_y"),
    );

    let tex_u = renderer.new_texture(
        uv_width as u16,
        uv_height as u16,
        frame.u_data,
        TextureFormat::Alpha,
        gfxtag!("video_u"),
    );

    let tex_v = renderer.new_texture(
        uv_width as u16,
        uv_height as u16,
        frame.v_data,
        TextureFormat::Alpha,
        gfxtag!("video_v"),
    );

    let yuv_texs = YuvTextures { y: tex_y, u: tex_u, v: tex_v };

    let mut vd_guard = vid_data.lock();
    let vd = vd_guard.as_mut().ok_or(())?;
    vd.textures[frame_idx] = Some(yuv_texs.clone());
    let _ = vd.textures_pub.try_broadcast((frame_idx, yuv_texs));
    Ok(())
}
