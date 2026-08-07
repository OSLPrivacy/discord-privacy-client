//! Image carrier for the fixed OSL pointer payload.
//!
//! Normal photo-sized images use a redundant luminance stripe that survives
//! provider resize and PNG re-save. Tiny images fall back to the original
//! lossless low-bit carrier so existing small fixtures continue to work.

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
