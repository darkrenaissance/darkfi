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

//! IVF (Indeo Video File) container format demuxer for AV1 video.
//!
//! IVF is a simple container format with a 32-byte header followed by
//! frame headers (12 bytes each) and frame data.

use darkfi_serial::Decodable;
use std::io::{Cursor, Read};
use thiserror::Error;

/// Errors that can occur during IVF demuxing
#[derive(Debug, Error)]
pub enum IvfError {
    #[error("Invalid IVF signature: expected 'DKIF', got '{0:?}'")]
    InvalidSignature([u8; 4]),

    #[error("Unsupported codec: expected 'AV01', got '{0:?}'")]
    UnsupportedCodec([u8; 4]),

    #[error("Unexpected end of file")]
    UnexpectedEof,
}

impl From<std::io::Error> for IvfError {
    fn from(_: std::io::Error) -> Self {
        Self::UnexpectedEof
    }
}

pub type IvfResult<T> = Result<T, IvfError>;

/// IVF file header (32 bytes)
///
/// ```
/// DKIF             - signature (4 bytes)
/// version (u16)    - version (2 bytes)
/// header_len (u16) - header length (2 bytes)
/// codec_fourcc     - codec fourcc (4 bytes), e.g., "AV01" for AV1
/// width (u16)      - width (2 bytes)
/// height (u16)     - height (2 bytes)
/// timebase_den (u32) - timebase denominator (4 bytes)
/// timebase_num (u32) - timebase numerator (4 bytes)
/// num_frames (u32)  - number of frames (4 bytes)
/// unused (u32)      - unused (4 bytes)
/// ```
#[derive(Debug, Clone)]
pub struct IvfHeader {
    signature: [u8; 4],
    version: u16,
    header_len: u16,
    codec_fourcc: [u8; 4],
    width: u16,
    height: u16,
    timebase_den: u32,
    timebase_num: u32,
    pub num_frames: u32,
    unused: u32,
}

/// Streaming IVF demuxer for chunked video files
///
/// This demuxer is designed for videos split into multiple chunks
/// (e.g., forest_1920x1080.ivf.000, forest_1920x1080.ivf.001, ...).
/// It handles frames that may span across chunk boundaries.
pub struct IvfStreamingDemuxer {
    /// Cursor wrapping the data buffer
    cur: Cursor<Vec<u8>>,
    /// Parsed IVF header
    pub header: IvfHeader,
    /// Current frame counter
    current_frame: u32,
}

impl IvfStreamingDemuxer {
    /// Create a new streaming IVF demuxer from the first chunk
    ///
    /// The first chunk must contain the 32-byte IVF header followed by
    /// frame data. Remaining chunks should be fed via `feed_data`.
    pub fn from_first_chunk(data: Vec<u8>) -> IvfResult<Self> {
        let mut self_ = Self {
            cur: Cursor::new(data),
            header: unsafe { std::mem::zeroed() },
            current_frame: 0,
        };

        self_.parse_header()?;

        // Validate signature (bytes 0-3 should be "DKIF")
        if &self_.header.signature != b"DKIF" {
            return Err(IvfError::InvalidSignature(self_.header.signature));
        }

        // Validate codec (bytes 8-11 should be "AV01" for AV1)
        if &self_.header.codec_fourcc != b"AV01" {
            return Err(IvfError::UnsupportedCodec(self_.header.codec_fourcc));
        }

        Ok(self_)
    }

    /// Parse IVF header from bytes (shared with IvfDemuxer)
    fn parse_header(&mut self) -> Result<(), std::io::Error> {
        // Offset 0-3: Signature "DKIF" (raw bytes)
        let mut signature = [0u8; 4];
        self.cur.read_exact(&mut signature)?;

        // Offset 4-5: Version (usually 0)
        let version = u16::decode(&mut self.cur)?;
        // Offset 6-7: Header length (usually 32)
        let header_len = u16::decode(&mut self.cur)?;

        // Offset 8-11: Codec FourCC ("AV01" for AV1, "VP80" for VP8) (raw bytes)
        let mut codec_fourcc = [0u8; 4];
        self.cur.read_exact(&mut codec_fourcc)?;

        // Offset 12-13: Frame width
        let width = u16::decode(&mut self.cur)?;
        // Offset 14-15: Frame height
        let height = u16::decode(&mut self.cur)?;

        // Offset 16-19: Timebase denominator (FPS numerator)
        let timebase_den = u32::decode(&mut self.cur)?;
        // Offset 20-23: Timebase numerator (FPS denominator)
        let timebase_num = u32::decode(&mut self.cur)?;

        // Offset 24-27: Total number of frames
        let num_frames = u32::decode(&mut self.cur)?;
        // Offset 28-31: Unused/reserved
        let unused = u32::decode(&mut self.cur)?;

        self.header = IvfHeader {
            signature,
            version,
            header_len,
            codec_fourcc,
            width,
            height,
            timebase_den,
            timebase_num,
            num_frames,
            unused,
        };

        Ok(())
    }

    /// Feed additional chunk data to the internal buffer
    ///
    /// After feeding data, call `try_read_frame()` to extract complete frames.
    pub fn feed_data(&mut self, mut data: Vec<u8>) {
        let pos = self.cur.position() as usize;
        let buffer = self.cur.get_mut();

        // Append new data to buffer
        buffer.append(&mut data);

        // Reset cursor to continue reading
        self.cur.set_position(pos as u64);
    }

    /// Try to read the next complete frame
    ///
    /// Returns `Ok(Some(frame))` if a complete frame is available,
    /// `Ok(None)` if more data is needed, or `Err` on invalid data.
    pub fn try_read_frame(&mut self) -> Option<Vec<u8>> {
        let current_pos = self.cur.position() as usize;

        // Check if we have enough bytes for frame header (12 bytes)
        if self.buffer_len() < current_pos + 12 {
            return None;
        }

        // Save cursor position in case we need to roll back
        let saved_pos = self.cur.position();

        // Read frame header
        let frame_size = u32::decode(&mut self.cur).unwrap();
        let _timestamp = u64::decode(&mut self.cur).unwrap();

        let frame_end = self.cur.position() as usize + frame_size as usize;

        // Check if we have the complete frame
        if self.buffer_len() < frame_end {
            // Incomplete frame, reset cursor
            self.cur.set_position(saved_pos);
            return None;
        }

        // Read the frame data
        let mut frame_data = vec![0u8; frame_size as usize];
        self.cur.read_exact(&mut frame_data).unwrap();

        self.current_frame += 1;
        Some(frame_data)
    }

    /// Have we read all frames?
    pub fn is_finished(&self) -> bool {
        assert!(self.current_frame < self.header.num_frames);
        self.current_frame == self.header.num_frames - 1
    }

    fn buffer_len(&mut self) -> usize {
        self.cur.get_mut().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_parsing() {
        // Create a minimal valid IVF header
        let mut header_data = vec![0u8; 32];

        // Signature
        header_data[0..4].copy_from_slice(b"DKIF");

        // Version
        header_data[4..6].copy_from_slice(&0u16.to_le_bytes());

        // Header length
        header_data[6..8].copy_from_slice(&32u16.to_le_bytes());

        // Codec
        header_data[8..12].copy_from_slice(b"AV01");

        // Dimensions
        header_data[12..14].copy_from_slice(&1920u16.to_le_bytes());
        header_data[14..16].copy_from_slice(&1080u16.to_le_bytes());

        // Timebase: 25 FPS = 25/1
        header_data[16..20].copy_from_slice(&25u32.to_le_bytes()); // denominator
        header_data[20..24].copy_from_slice(&1u32.to_le_bytes()); // numerator

        // Frame count
        header_data[24..28].copy_from_slice(&100u32.to_le_bytes());

        let header = IvfDemuxer::parse_header(&header_data).unwrap();

        assert_eq!(&header.signature, b"DKIF");
        assert_eq!(&header.codec_fourcc, b"AV01");
        assert_eq!(header.width, 1920);
        assert_eq!(header.height, 1080);
        assert_eq!(header.timebase_den, 25);
        assert_eq!(header.timebase_num, 1);
        assert_eq!(header.num_frames, 100);
    }

    #[test]
    fn test_invalid_signature() {
        let mut header_data = vec![0u8; 32];
        header_data[0..4].copy_from_slice(b"BAD!");

        let result = IvfDemuxer::from_bytes(header_data);
        assert!(matches!(result, Err(IvfError::InvalidSignature(_))));
    }

    #[test]
    fn test_unsupported_codec() {
        let mut header_data = vec![0u8; 32];
        header_data[0..4].copy_from_slice(b"DKIF");
        // VP8 instead of AV1
        header_data[8..12].copy_from_slice(b"VP80");

        let result = IvfDemuxer::from_bytes(header_data);
        assert!(matches!(result, Err(IvfError::UnsupportedCodec(_))));
    }
}
