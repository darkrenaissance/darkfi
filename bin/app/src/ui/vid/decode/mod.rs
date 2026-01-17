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

//! Platform-specific video decoding

use parking_lot::Mutex as SyncMutex;
use std::sync::Arc;

use crate::gfx::{RenderApi, Renderer};

use super::Av1VideoData;

#[cfg(target_os = "android")]
mod android;

#[cfg(not(target_os = "android"))]
mod rav1d;

/// Spawn the decoder thread
///
/// Platform-specific implementation:
/// - Android: Uses MediaCodec for H.264 hardware decoding
/// - Desktop: Uses rav1d for AV1 software decoding
pub fn spawn_decoder_thread(
    path: String,
    vid_data: Arc<SyncMutex<Option<Av1VideoData>>>,
    renderer: Renderer,
) -> std::thread::JoinHandle<()> {
    #[cfg(target_os = "android")]
    return android::spawn_decoder_thread(path, vid_data, renderer);

    #[cfg(not(target_os = "android"))]
    return rav1d::spawn_decoder_thread(path, vid_data, renderer);
}
