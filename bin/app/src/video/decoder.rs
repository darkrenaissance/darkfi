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

//! rav1d AV1 video decoder wrapper
//!
//! This module provides a Rust wrapper around the rav1d AV1 decoder.

use rav1d::{Decoder as Rav1dDecoderInner, Picture, PlanarImageComponent, Rav1dError};

use super::yuv_conv::yuv420p_to_rgba;

/// A decoded frame containing RGBA data
#[derive(Debug, Clone)]
pub struct DecodedFrame {
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// RGBA pixel data (width * height * 4 bytes)
    pub data: Vec<u8>,
}

/// rav1d AV1 video decoder wrapper
///
/// This wraps the rav1d decoder and provides automatic YUV to RGBA conversion.
pub struct Rav1dDecoder {
    /// Inner decoder from rav1d
    decoder: Rav1dDecoderInner,
}

impl Rav1dDecoder {
    pub fn new() -> Self {
        Self { decoder: Rav1dDecoderInner::new().unwrap() }
    }

    /// Decode AV1 bitstream data
    pub fn decode(&mut self, data: &[u8]) -> Result<DecodedFrame, Rav1dError> {
        // Send data to decoder
        // Need to copy data because send_data requires 'static ownership
        let data = data.to_vec();
        match self.decoder.send_data(data, None, None, None) {
            Ok(_) => {}
            Err(Rav1dError::TryAgain) => {
                // Pending data - try to send it again
                while let Err(Rav1dError::TryAgain) = self.decoder.send_pending_data() {
                    // Continue sending pending data
                }
            }
            Err(err) => return Err(err),
        }

        self.get_pic()
    }

    fn get_pic(&mut self) -> Result<DecodedFrame, Rav1dError> {
        self.decoder.get_picture().map(|pic| Self::conv(pic))
    }

    /// Convert a rav1d Picture to RGBA
    fn conv(pic: Picture) -> DecodedFrame {
        let y_plane = pic.plane(PlanarImageComponent::Y);
        let u_plane = pic.plane(PlanarImageComponent::U);
        let v_plane = pic.plane(PlanarImageComponent::V);

        let y_stride = pic.stride(PlanarImageComponent::Y) as usize;
        let u_stride = pic.stride(PlanarImageComponent::U) as usize;
        let v_stride = pic.stride(PlanarImageComponent::V) as usize;

        let width = pic.width() as usize;
        let height = pic.height() as usize;

        let data = yuv420p_to_rgba(
            &y_plane, &u_plane, &v_plane, width, height, y_stride, u_stride, v_stride,
        );

        DecodedFrame { width: width as u32, height: height as u32, data }
    }

    /// Flush the decoder to get any remaining frames
    pub fn flush(&mut self) -> Result<DecodedFrame, Rav1dError> {
        self.decoder.flush();
        self.get_pic()
    }
}
