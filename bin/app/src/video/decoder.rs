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

pub type DecoderResult<T> = Result<T, Rav1dError>;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "ui:video", $($arg)*); } }

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
/// This wraps the rav1d decoder and provides automatic planar GBR to RGBA conversion.
pub struct Rav1dDecoder {
    /// Inner decoder from rav1d
    decoder: Rav1dDecoderInner,
}

impl Rav1dDecoder {
    pub fn new() -> Self {
        Self { decoder: Rav1dDecoderInner::new().unwrap() }
    }

    /// Send AV1 bitstream data to the decoder without getting a frame
    pub fn send_data(&mut self, data: &[u8]) -> DecoderResult<()> {
        let data = data.to_vec();
        match self.decoder.send_data(data, None, None, None) {
            Ok(_) => {}
            Err(Rav1dError::TryAgain) => {
                while let Err(Rav1dError::TryAgain) = self.decoder.send_pending_data() {
                    // Continue sending pending data
                }
            }
            Err(err) => return Err(err),
        }
        Ok(())
    }

    /// Get the next decoded frame from the decoder
    pub fn get_pic(&mut self) -> DecoderResult<DecodedFrame> {
        let now = std::time::Instant::now();
        let pix = self.decoder.get_picture();
        t!("decoder get pix: {:?}", now.elapsed());

        let now = std::time::Instant::now();
        let res = pix.map(|pic| Self::conv(pic));
        t!("decoder conv: {:?}", now.elapsed());
        res
    }

    /// Decode AV1 bitstream data and get all available frames
    /// Returns a vector of frames (may be empty if decoder needs more data)
    pub fn decode(&mut self, data: &[u8]) -> DecoderResult<Vec<DecodedFrame>> {
        self.send_data(data)?;

        let mut frames = Vec::new();
        loop {
            match self.get_pic() {
                Ok(frame) => frames.push(frame),
                Err(Rav1dError::TryAgain) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(frames)
    }

    /// Convert a rav1d Picture from planar GBR to RGBA
    fn conv(pic: Picture) -> DecodedFrame {
        let g_plane = pic.plane(PlanarImageComponent::Y);
        let b_plane = pic.plane(PlanarImageComponent::U);
        let r_plane = pic.plane(PlanarImageComponent::V);

        let g_stride = pic.stride(PlanarImageComponent::Y) as usize;
        let b_stride = pic.stride(PlanarImageComponent::U) as usize;
        let r_stride = pic.stride(PlanarImageComponent::V) as usize;

        let width = pic.width() as usize;
        let height = pic.height() as usize;

        let mut rgba = vec![0u8; width * height * 4];

        for y in 0..height {
            for x in 0..width {
                let g_idx = y * g_stride + x;
                let b_idx = y * b_stride + x;
                let r_idx = y * r_stride + x;

                let r = r_plane[r_idx];
                let g = g_plane[g_idx];
                let b = b_plane[b_idx];

                let out_idx = (y * width + x) * 4;
                rgba[out_idx] = r;
                rgba[out_idx + 1] = g;
                rgba[out_idx + 2] = b;
                rgba[out_idx + 3] = 255;
            }
        }

        DecodedFrame { width: width as u32, height: height as u32, data: rgba }
    }

    /// Flush the decoder to get any remaining frames
    pub fn flush(&mut self) -> DecoderResult<Vec<DecodedFrame>> {
        self.decoder.flush();
        let mut frames = Vec::new();
        loop {
            match self.get_pic() {
                Ok(frame) => frames.push(frame),
                Err(Rav1dError::TryAgain) => break,
                Err(e) => return Err(e),
            }
        }
        Ok(frames)
    }
}
