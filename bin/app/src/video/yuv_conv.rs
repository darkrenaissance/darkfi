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

//! YUV to RGBA color space conversion
//!
//! This module provides functions to convert YUV420P planar format
//! to RGBA format for GPU rendering.

macro_rules! t { ($($arg:tt)*) => { trace!(target: "video::yuv_conv", $($arg)*); } }

/// Convert YUV420P planar format to RGBA
///
/// # Arguments
/// * `y_plane` - Y (luminance) plane data
/// * `u_plane` - U (chrominance) plane data
/// * `v_plane` - V (chrominance) plane data
/// * `width` - Frame width in pixels
/// * `height` - Frame height in pixels
/// * `y_stride` - Y plane stride (bytes per row)
/// * `u_stride` - U plane stride (bytes per row)
/// * `v_stride` - V plane stride (bytes per row)
///
/// # Returns
/// A Vec<u8> containing RGBA data (width * height * 4 bytes)
///
/// # Color Conversion
/// Uses BT.601 standard for YUV to RGB conversion:
/// ```text
/// R = Y + 1.402 * (V - 128)
/// G = Y - 0.344 * (U - 128) - 0.714 * (V - 128)
/// B = Y + 1.772 * (U - 128)
/// ```
pub fn yuv420p_to_rgba(
    y_plane: &[u8],
    u_plane: &[u8],
    v_plane: &[u8],
    width: usize,
    height: usize,
    y_stride: usize,
    u_stride: usize,
    v_stride: usize,
) -> Vec<u8> {
    //t!("yuv420p_to_rgba() {}x{} strides: y={} u={} v={}", width, height, y_stride, u_stride, v_stride);

    let mut rgba = vec![0u8; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let y_idx = y * y_stride + x;
            let u_idx = (y / 2) * u_stride + (x / 2);
            let v_idx = (y / 2) * v_stride + (x / 2);

            let y_val = y_plane[y_idx] as i32;
            let u_val = u_plane[u_idx] as i32 - 128;
            let v_val = v_plane[v_idx] as i32 - 128;

            // BT.601 YUV to RGB conversion
            let r = clamp((y_val as f32) + 1.402 * (v_val as f32));
            let g = clamp((y_val as f32) - 0.344136 * (u_val as f32) - 0.714136 * (v_val as f32));
            let b = clamp((y_val as f32) + 1.772 * (u_val as f32));

            let out_idx = (y * width + x) * 4;
            rgba[out_idx] = r;
            rgba[out_idx + 1] = g;
            rgba[out_idx + 2] = b;
            // Alpha
            rgba[out_idx + 3] = 255;
        }
    }

    rgba
}

/// Clamp a floating point value to 0-255 range and convert to u8
#[inline]
fn clamp(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yuv420p_to_rgba_basic() {
        // Create a small test frame (4x4)
        let width = 4;
        let height = 4;

        // Y plane - full resolution
        let y_plane: Vec<u8> = (0..16).map(|i| (i * 16) as u8).collect();

        // U and V planes - half resolution (2x2)
        let u_plane: Vec<u8> = vec![128, 128, 128, 128];
        let v_plane: Vec<u8> = vec![128, 128, 128, 128];

        let y_stride = width;
        let u_stride = width / 2;
        let v_stride = width / 2;

        let rgba = yuv420p_to_rgba(
            &y_plane,
            &u_plane,
            &v_plane,
            width,
            height,
            y_stride,
            u_stride,
            v_stride,
        );

        // Check output size
        assert_eq!(rgba.len(), width * height * 4);

        // With U=V=128 (neutral chroma), RGB should equal Y
        for i in 0..16 {
            let expected_y = (i * 16) as u8;
            // R
            assert_eq!(rgba[i * 4], expected_y);
            // G
            assert_eq!(rgba[i * 4 + 1], expected_y);
            // B
            assert_eq!(rgba[i * 4 + 2], expected_y);
            // A
            assert_eq!(rgba[i * 4 + 3], 255);
        }
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(-10.0), 0);
        assert_eq!(clamp(0.0), 0);
        assert_eq!(clamp(127.5), 128);
        assert_eq!(clamp(255.0), 255);
        assert_eq!(clamp(300.0), 255);
    }

    #[test]
    fn test_yuv_to_rgb_conversion() {
        // Test specific color conversions
        // Black: Y=0, U=128, V=128
        let black = yuv_to_rgb(0, 128, 128);
        assert_eq!(black, [0, 0, 0]);

        // White: Y=255, U=128, V=128
        let white = yuv_to_rgb(255, 128, 128);
        assert_eq!(white, [255, 255, 255]);

        // Red: Y=76, U=85, V=255 (approximate pure red in BT.601)
        let red = yuv_to_rgb(76, 85, 255);
        // R should be high
        assert!(red[0] > 200);
        // G should be low
        assert!(red[1] < 50);
        // B should be low
        assert!(red[2] < 50);
    }

    /// Helper function to convert a single YUV pixel to RGB
    fn yuv_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
        let y_val = y as i32;
        let u_val = u as i32 - 128;
        let v_val = v as i32 - 128;

        let r = clamp((y_val as f32) + 1.402 * (v_val as f32));
        let g = clamp((y_val as f32) - 0.344136 * (u_val as f32) - 0.714136 * (v_val as f32));
        let b = clamp((y_val as f32) + 1.772 * (u_val as f32));

        [r, g, b]
    }
}
