//! Image carrier for the fixed OSL pointer payload.
//!
//! Normal photo-sized images use a redundant luminance stripe that survives
//! provider resize and PNG re-save. Tiny images fall back to the original
//! lossless low-bit carrier so existing small fixtures continue to work.
//! Lossless image carrier for the fixed OSL pointer payload.
//!
//! The carrier writes a small OSL marker plus the full protected pointer
//! (`20-byte message name || 4-byte check mark`) into the low bit of RGB
//! samples in a newly encoded PNG copy. The source image is only read; callers
//! choose a distinct output path for the post copy.

use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::Path;

use png::{BitDepth, ColorType};

use crate::{Error, Result, DETECT_TAG_BYTES, TOKEN_ID_BYTES};

pub const IMAGE_HIDDEN_POINTER_BYTES: usize = TOKEN_ID_BYTES;
pub const IMAGE_HIDDEN_CHECK_MARK_BYTES: usize = DETECT_TAG_BYTES;

const MAGIC: &[u8; 8] = b"OSLIH1\0\0";
const FRAME_BYTES: usize = MAGIC.len() + IMAGE_HIDDEN_POINTER_BYTES + IMAGE_HIDDEN_CHECK_MARK_BYTES;
const FRAME_BITS: usize = FRAME_BYTES * 8;
const RESAVE_MIN_WIDTH: u32 = FRAME_BITS as u32;
const RESAVE_STRIPE_DIVISOR: u32 = 8;
const RESAVE_LOW: u8 = 15;
const RESAVE_HIGH: u8 = 240;
const RESAVE_THRESHOLD: u64 = 128;

/// A pixel must differ from a horizontal or vertical neighbour by at least
/// this much luminance before it contributes robust image-hidden capacity.
/// Low-bit changes in flatter areas are easier to see and less likely to
/// survive a provider transform.
const QUALITY_MIN_LOCAL_LUMINANCE_DELTA: u64 = 12;

/// Liam's selected low-quality-image behavior from task 3134. There is no
/// warning/override path: callers must stop before posting this image.
pub const IMAGE_HIDDEN_LOW_QUALITY_REFUSAL: &str = "Choose a larger or more detailed image.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageHiddenQualityDecision {
    Accepted,
    Refused,
}

/// Capacity decision for one decoded source image.
///
/// `file_name` remains the caller-visible source name even when the decoder has
/// normalized JPEG, PNG, or another supported format to RGB samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageHiddenQualityResult {
    pub file_name: String,
    pub marked_bytes: usize,
    pub measured_capacity_bytes: usize,
    pub decision: ImageHiddenQualityDecision,
}

impl ImageHiddenQualityResult {
    pub const fn is_accepted(&self) -> bool {
        matches!(self.decision, ImageHiddenQualityDecision::Accepted)
    }

    /// Complete person-facing result. Both the accepted and refused forms name
    /// the file and the capacity actually measured from its decoded pixels.
    pub fn message(&self) -> String {
        match self.decision {
            ImageHiddenQualityDecision::Accepted => format!(
                "{}: accepted {} marked bytes; measured capacity {} bytes.",
                self.file_name, self.marked_bytes, self.measured_capacity_bytes
            ),
            ImageHiddenQualityDecision::Refused => format!(
                "{}: refused {} marked bytes; measured capacity {} bytes. {}",
                self.file_name,
                self.marked_bytes,
                self.measured_capacity_bytes,
                IMAGE_HIDDEN_LOW_QUALITY_REFUSAL
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHiddenPointer {
    pub pointer: [u8; IMAGE_HIDDEN_POINTER_BYTES],
    pub check_mark: [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
}

impl ImageHiddenPointer {
    pub fn new(
        pointer: [u8; IMAGE_HIDDEN_POINTER_BYTES],
        check_mark: [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
    ) -> Self {
        Self {
            pointer,
            check_mark,
        }
    }
}

struct DecodedPng {
    width: u32,
    height: u32,
    color_type: ColorType,
    bit_depth: BitDepth,
    pixels: Vec<u8>,
}

pub fn encode_png_hidden_pointer_copy(
    source_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    pointer: [u8; IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
) -> Result<()> {
    let source = File::open(source_path.as_ref())?;
    let mut decoded = decode_png_reader(BufReader::new(source))?;
    embed_frame(&mut decoded, ImageHiddenPointer::new(pointer, check_mark))?;

    let output = File::create(output_path.as_ref())?;
    write_png(BufWriter::new(output), &decoded)
}

pub fn encode_png_hidden_pointer_bytes(
    source_png: &[u8],
    pointer: [u8; IMAGE_HIDDEN_POINTER_BYTES],
    check_mark: [u8; IMAGE_HIDDEN_CHECK_MARK_BYTES],
) -> Result<Vec<u8>> {
    let mut decoded = decode_png_reader(Cursor::new(source_png))?;
    embed_frame(&mut decoded, ImageHiddenPointer::new(pointer, check_mark))?;

    let mut output = Vec::new();
    write_png(Cursor::new(&mut output), &decoded)?;
    Ok(output)
}

pub fn decode_png_hidden_pointer(path: impl AsRef<Path>) -> Result<Option<ImageHiddenPointer>> {
    let source = File::open(path.as_ref())?;
    let decoded = decode_png_reader(BufReader::new(source))?;
    extract_frame(&decoded)
}

pub fn decode_png_hidden_pointer_bytes(source_png: &[u8]) -> Result<Option<ImageHiddenPointer>> {
    let decoded = decode_png_reader(Cursor::new(source_png))?;
    extract_frame(&decoded)
}

/// Measure whether decoded RGB pixels have enough detailed area to carry the
/// requested number of marked bytes without relying on flat or tiny regions.
///
/// The caller supplies RGB samples after decoding, so the original can be a
/// JPEG, PNG, or another locally supported image type. One qualifying pixel is
/// counted as one conservative mark bit even though it has three colour
/// components. This reserves redundancy for provider resize/re-save behavior
/// and means geometric size alone cannot make a plain image look safe.
pub fn check_decoded_rgb_hidden_image_quality(
    file_name: impl Into<String>,
    width: u32,
    height: u32,
    rgb_pixels: &[u8],
    marked_bytes: usize,
) -> Result<ImageHiddenQualityResult> {
    let file_name = file_name.into();
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or(Error::ImageHiddenQualityDimensions { width, height })?;
    let expected_rgb_bytes = pixel_count
        .checked_mul(3)
        .ok_or(Error::ImageHiddenQualityDimensions { width, height })?;
    if rgb_pixels.len() != expected_rgb_bytes {
        return Err(Error::ImageHiddenQualityPixelLength {
            file_name,
            expected: expected_rgb_bytes,
            got: rgb_pixels.len(),
        });
    }

    let measured_capacity_bytes = detailed_pixel_count(width, height, rgb_pixels) / 8;
    let decision = if measured_capacity_bytes >= marked_bytes {
        ImageHiddenQualityDecision::Accepted
    } else {
        ImageHiddenQualityDecision::Refused
    };
    Ok(ImageHiddenQualityResult {
        file_name,
        marked_bytes,
        measured_capacity_bytes,
        decision,
    })
}

fn detailed_pixel_count(width: u32, height: u32, rgb_pixels: &[u8]) -> usize {
    let width = width as usize;
    let height = height as usize;
    let mut detailed = 0usize;

    for y in 0..height {
        for x in 0..width {
            let offset = (y * width + x) * 3;
            let current = luminance(
                rgb_pixels[offset],
                rgb_pixels[offset + 1],
                rgb_pixels[offset + 2],
            );
            let horizontal_delta = (x > 0).then(|| {
                let neighbour = offset - 3;
                current.abs_diff(luminance(
                    rgb_pixels[neighbour],
                    rgb_pixels[neighbour + 1],
                    rgb_pixels[neighbour + 2],
                ))
            });
            let vertical_delta = (y > 0).then(|| {
                let neighbour = offset - width * 3;
                current.abs_diff(luminance(
                    rgb_pixels[neighbour],
                    rgb_pixels[neighbour + 1],
                    rgb_pixels[neighbour + 2],
                ))
            });
            let local_delta = horizontal_delta
                .into_iter()
                .chain(vertical_delta)
                .max()
                .unwrap_or(0);
            if local_delta >= QUALITY_MIN_LOCAL_LUMINANCE_DELTA {
                detailed += 1;
            }
        }
    }

    detailed
}

fn decode_png_reader<R: std::io::Read>(reader: R) -> Result<DecodedPng> {
    let decoder = png::Decoder::new(reader);
    let mut reader = decoder
        .read_info()
        .map_err(|err| Error::ImageHiddenPng(err.to_string()))?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|err| Error::ImageHiddenPng(err.to_string()))?;
    pixels.truncate(info.buffer_size());

    if info.bit_depth != BitDepth::Eight
        || !matches!(info.color_type, ColorType::Rgb | ColorType::Rgba)
    {
        return Err(Error::ImageHiddenUnsupportedPng {
            color_type: format!("{:?}", info.color_type),
            bit_depth: format!("{:?}", info.bit_depth),
        });
    }

    Ok(DecodedPng {
        width: info.width,
        height: info.height,
        color_type: info.color_type,
        bit_depth: info.bit_depth,
        pixels,
    })
}

fn write_png<W: std::io::Write>(writer: W, decoded: &DecodedPng) -> Result<()> {
    let mut encoder = png::Encoder::new(writer, decoded.width, decoded.height);
    encoder.set_color(decoded.color_type);
    encoder.set_depth(decoded.bit_depth);
    let mut writer = encoder
        .write_header()
        .map_err(|err| Error::ImageHiddenPng(err.to_string()))?;
    writer
        .write_image_data(&decoded.pixels)
        .map_err(|err| Error::ImageHiddenPng(err.to_string()))
}

fn embed_frame(decoded: &mut DecodedPng, payload: ImageHiddenPointer) -> Result<()> {
    if can_embed_resave_survival(decoded) {
        embed_resave_survival_frame(decoded, payload);
        return Ok(());
    }

    let capacity_bits = carrier_capacity_bits(decoded);
    if capacity_bits < FRAME_BITS {
        return Err(Error::ImageHiddenTooSmall {
            required_bits: FRAME_BITS,
            capacity_bits,
        });
    }

    let frame = frame_bytes(payload);
    let carrier_indices: Vec<usize> = carrier_component_indices(decoded)
        .take(FRAME_BITS)
        .collect();
    for (bit_index, pixel_index) in carrier_indices.into_iter().enumerate() {
        let byte = frame[bit_index / 8];
        let bit = (byte >> (7 - (bit_index % 8))) & 1;
        decoded.pixels[pixel_index] = (decoded.pixels[pixel_index] & 0xfe) | bit;
    }

    Ok(())
}

fn extract_frame(decoded: &DecodedPng) -> Result<Option<ImageHiddenPointer>> {
    if let Some(payload) = extract_resave_survival_frame(decoded)? {
        return Ok(Some(payload));
    }

    if carrier_capacity_bits(decoded) < FRAME_BITS {
        return Ok(None);
    }

    let mut frame = [0u8; FRAME_BYTES];
    for (bit_index, pixel_index) in carrier_component_indices(decoded)
        .take(FRAME_BITS)
        .enumerate()
    {
        frame[bit_index / 8] <<= 1;
        frame[bit_index / 8] |= decoded.pixels[pixel_index] & 1;
    }

    if &frame[..MAGIC.len()] != MAGIC {
        return Ok(None);
    }

    let pointer_start = MAGIC.len();
    let check_start = pointer_start + IMAGE_HIDDEN_POINTER_BYTES;
    let mut pointer = [0u8; IMAGE_HIDDEN_POINTER_BYTES];
    pointer.copy_from_slice(&frame[pointer_start..check_start]);
    let mut check_mark = [0u8; IMAGE_HIDDEN_CHECK_MARK_BYTES];
    check_mark.copy_from_slice(&frame[check_start..]);

    Ok(Some(ImageHiddenPointer::new(pointer, check_mark)))
}

fn can_embed_resave_survival(decoded: &DecodedPng) -> bool {
    decoded.width >= RESAVE_MIN_WIDTH && decoded.height >= RESAVE_STRIPE_DIVISOR
}

fn embed_resave_survival_frame(decoded: &mut DecodedPng, payload: ImageHiddenPointer) {
    let frame = frame_bytes(payload);
    let samples = decoded.color_type.samples();
    let stripe_height = resave_stripe_height(decoded.height);

    for bit_index in 0..FRAME_BITS {
        let byte = frame[bit_index / 8];
        let bit = (byte >> (7 - (bit_index % 8))) & 1;
        let value = if bit == 1 { RESAVE_HIGH } else { RESAVE_LOW };
        let x_start = resave_column_start(decoded.width, bit_index);
        let x_end = resave_column_start(decoded.width, bit_index + 1).max(x_start + 1);

        for y in 0..stripe_height {
            for x in x_start..x_end.min(decoded.width) {
                let offset = ((y * decoded.width + x) as usize) * samples;
                decoded.pixels[offset] = value;
                decoded.pixels[offset + 1] = value;
                decoded.pixels[offset + 2] = value;
            }
        }
    }
}

fn extract_resave_survival_frame(decoded: &DecodedPng) -> Result<Option<ImageHiddenPointer>> {
    if decoded.width < RESAVE_MIN_WIDTH || decoded.height == 0 {
        return Ok(None);
    }

    let samples = decoded.color_type.samples();
    let stripe_height = resave_stripe_height(decoded.height);
    let mut frame = [0u8; FRAME_BYTES];

    for bit_index in 0..FRAME_BITS {
        let x_start = resave_column_start(decoded.width, bit_index);
        let x_end = resave_column_start(decoded.width, bit_index + 1).max(x_start + 1);
        let mut total = 0u64;
        let mut count = 0u64;

        for y in 0..stripe_height {
            for x in x_start..x_end.min(decoded.width) {
                let offset = ((y * decoded.width + x) as usize) * samples;
                total += luminance(
                    decoded.pixels[offset],
                    decoded.pixels[offset + 1],
                    decoded.pixels[offset + 2],
                );
                count += 1;
            }
        }

        frame[bit_index / 8] <<= 1;
        if count > 0 && total / count >= RESAVE_THRESHOLD {
            frame[bit_index / 8] |= 1;
        }
    }

    if &frame[..MAGIC.len()] != MAGIC {
        return Ok(None);
    }

    let pointer_start = MAGIC.len();
    let check_start = pointer_start + IMAGE_HIDDEN_POINTER_BYTES;
    let mut pointer = [0u8; IMAGE_HIDDEN_POINTER_BYTES];
    pointer.copy_from_slice(&frame[pointer_start..check_start]);
    let mut check_mark = [0u8; IMAGE_HIDDEN_CHECK_MARK_BYTES];
    check_mark.copy_from_slice(&frame[check_start..]);

    Ok(Some(ImageHiddenPointer::new(pointer, check_mark)))
}

fn resave_stripe_height(height: u32) -> u32 {
    (height / RESAVE_STRIPE_DIVISOR).max(1)
}

fn resave_column_start(width: u32, bit_index: usize) -> u32 {
    ((u64::from(width) * bit_index as u64) / FRAME_BITS as u64) as u32
}

fn luminance(r: u8, g: u8, b: u8) -> u64 {
    (u64::from(r) * 299 + u64::from(g) * 587 + u64::from(b) * 114) / 1000
}

fn frame_bytes(payload: ImageHiddenPointer) -> [u8; FRAME_BYTES] {
    let mut frame = [0u8; FRAME_BYTES];
    frame[..MAGIC.len()].copy_from_slice(MAGIC);
    let pointer_start = MAGIC.len();
    let check_start = pointer_start + IMAGE_HIDDEN_POINTER_BYTES;
    frame[pointer_start..check_start].copy_from_slice(&payload.pointer);
    frame[check_start..].copy_from_slice(&payload.check_mark);
    frame
}

fn carrier_capacity_bits(decoded: &DecodedPng) -> usize {
    carrier_component_indices(decoded).count()
}

fn carrier_component_indices(decoded: &DecodedPng) -> impl Iterator<Item = usize> + '_ {
    let samples = decoded.color_type.samples();
    decoded
        .pixels
        .iter()
        .enumerate()
        .filter_map(move |(index, _)| {
            if decoded.color_type == ColorType::Rgba && index % samples == 3 {
                None
            } else {
                Some(index)
            }
        })
}
