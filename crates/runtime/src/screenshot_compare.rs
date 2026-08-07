//! Exact comparison for fixed-screen screenshot captures.
//!
//! This module deliberately compares dimensions and raw RGBA pixels without
//! tolerances. It is for deterministic fixed-screen captures where any pixel
//! drift is evidence that the capture is not identical.

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixedScreenCapture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PixelMismatch {
    pub x: u32,
    pub y: u32,
    pub left_rgba: [u8; 4],
    pub right_rgba: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenshotComparison {
    Identical {
        width: u32,
        height: u32,
        pixels_compared: usize,
    },
    Different {
        width: u32,
        height: u32,
        mismatched_pixels: usize,
        first_mismatch: PixelMismatch,
    },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScreenshotCompareError {
    #[error("capture dimensions differ: left {left_width}x{left_height}, right {right_width}x{right_height}")]
    DimensionMismatch {
        left_width: u32,
        left_height: u32,
        right_width: u32,
        right_height: u32,
    },
    #[error("capture {width}x{height} requires {expected} RGBA bytes, observed {observed}")]
    InvalidRgbaLength {
        width: u32,
        height: u32,
        expected: usize,
        observed: usize,
    },
}

impl FixedScreenCapture {
    pub fn from_rgba(
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Self, ScreenshotCompareError> {
        let expected = expected_rgba_len(width, height)?;
        if rgba.len() != expected {
            return Err(ScreenshotCompareError::InvalidRgbaLength {
                width,
                height,
                expected,
                observed: rgba.len(),
            });
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

pub fn compare_fixed_screen_captures(
    left: &FixedScreenCapture,
    right: &FixedScreenCapture,
) -> Result<ScreenshotComparison, ScreenshotCompareError> {
    if left.width != right.width || left.height != right.height {
        return Err(ScreenshotCompareError::DimensionMismatch {
            left_width: left.width,
            left_height: left.height,
            right_width: right.width,
            right_height: right.height,
        });
    }

    let mut mismatched_pixels = 0usize;
    let mut first_mismatch = None;
    for pixel_index in 0..(left.rgba.len() / 4) {
        let offset = pixel_index * 4;
        let left_rgba = [
            left.rgba[offset],
            left.rgba[offset + 1],
            left.rgba[offset + 2],
            left.rgba[offset + 3],
        ];
        let right_rgba = [
            right.rgba[offset],
            right.rgba[offset + 1],
            right.rgba[offset + 2],
            right.rgba[offset + 3],
        ];
        if left_rgba != right_rgba {
            mismatched_pixels += 1;
            first_mismatch.get_or_insert_with(|| PixelMismatch {
                x: (pixel_index as u32) % left.width,
                y: (pixel_index as u32) / left.width,
                left_rgba,
                right_rgba,
            });
        }
    }

    if let Some(first_mismatch) = first_mismatch {
        Ok(ScreenshotComparison::Different {
            width: left.width,
            height: left.height,
            mismatched_pixels,
            first_mismatch,
        })
    } else {
        Ok(ScreenshotComparison::Identical {
            width: left.width,
            height: left.height,
            pixels_compared: left.rgba.len() / 4,
        })
    }
}

fn expected_rgba_len(width: u32, height: u32) -> Result<usize, ScreenshotCompareError> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(ScreenshotCompareError::InvalidRgbaLength {
            width,
            height,
            expected: usize::MAX,
            observed: 0,
        })
}
