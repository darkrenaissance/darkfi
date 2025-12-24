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

//! IVF (Indeo Video File) container format demuxer for AV1 video.
//!
//! IVF is a simple container format with a 32-byte header followed by
//! frame headers (12 bytes each) and frame data.

use darkfi_serial::Decodable;
use std::io::{Cursor, Read};
use thiserror::Error;

macro_rules! t { ($($arg:tt)*) => { trace!(target: "video::ivf", $($arg)*); } }
macro_rules! d { ($($arg:tt)*) => { debug!(target: "video::ivf", $($arg)*); } }

/// Errors that can occur during IVF demuxing
#[derive(Debug, Error)]
pub enum IvfError {
    #[error("Invalid IVF signature: expected 'DKIF', got '{0:?}'")]
    InvalidSignature([u8; 4]),

    #[error("Unsupported codec: expected 'AV01', got '{0:?}'")]
    UnsupportedCodec([u8; 4]),

    #[error("Unexpected end of file")]
    UnexpectedEof,

    #[error("Invalid frame size: {0}")]
    InvalidFrameSize(u32),
}

impl From<std::io::Error> for IvfError {
    fn from(_: std::io::Error) -> Self {
        IvfError::UnexpectedEof
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
struct IvfHeader {
    signature: [u8; 4],
    version: u16,
    header_len: u16,
    codec_fourcc: [u8; 4],
    pub width: u16,
    pub height: u16,
    timebase_den: u32,
    timebase_num: u32,
    num_frames: u32,
    unused: u32,
}

/// IVF demuxer for AV1 video files
pub struct IvfDemuxer {
    cur: Cursor<Vec<u8>>,
    pub header: IvfHeader,
    current_frame: u32,
}

impl IvfDemuxer {
    /// Create a new IVF demuxer from raw bytes
    pub fn from_bytes(data: Vec<u8>) -> IvfResult<Self> {
        if data.len() < 32 {
            return Err(IvfError::UnexpectedEof);
        }

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

        d!(
            "IVF header: {}x{} frames={}",
            self_.header.width,
            self_.header.height,
            self_.header.num_frames
        );

        Ok(self_)
    }

    /// Parse IVF header from bytes
    ///
    /// # IVF Header Structure (32 bytes, little-endian)
    ///
    /// | Offset | Size | Field           | Value                      |
    /// |--------|------|-----------------|----------------------------|
    /// | 0      | 4    | signature       | "DKIF"                     |
    /// | 4      | 2    | version         | 0                          |
    /// | 6      | 2    | header_len      | 32                         |
    /// | 8      | 4    | codec_fourcc    | "AV01" for AV1             |
    /// | 12     | 2    | width           | Frame width in pixels      |
    /// | 14     | 2    | height          | Frame height in pixels     |
    /// | 16     | 4    | timebase_den    | FPS denominator            |
    /// | 20     | 4    | timebase_num    | FPS numerator              |
    /// | 24     | 4    | num_frames      | Total frames               |
    /// | 28     | 4    | unused          | Reserved                   |
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

    /// Get the next frame's AV1 bitstream data
    ///
    /// # IVF Frame Header Structure (12 bytes, little-endian)
    ///
    /// | Offset | Size | Field       | Description                           |
    /// |--------|------|-------------|---------------------------------------|
    /// | 0      | 4    | frame_size  | Size of frame data in bytes           |
    /// | 4      | 8    | timestamp   | Presentation timestamp                |
    ///
    /// The frame data immediately follows the 12-byte header.
    pub fn next_frame(&mut self) -> Result<Vec<u8>, std::io::Error> {
        // Offset 0-3: Frame size in bytes
        let frame_size = u32::decode(&mut self.cur)?;
        // Offset 4-11: Timestamp (8 bytes) - not used for linear playback
        let _timestamp = u64::decode(&mut self.cur)?;

        let mut frame_data = vec![0u8; frame_size as usize];
        self.cur.read_exact(&mut frame_data)?;
        // Read the frame
        self.current_frame += 1;
        Ok(frame_data)
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
