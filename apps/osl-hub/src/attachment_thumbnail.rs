//! In-memory thumbnails for protected image attachments.
//!
//! The caller supplies the authenticated decryption operation.  A decoder is
//! deliberately a closure: this module has no filesystem, IPC, or network
//! capability, so an encoded image can only become a thumbnail after that
//! operation has returned authenticated plaintext.

/// The longest edge used by attachment-list thumbnails.
pub const THUMBNAIL_LONGEST_EDGE: u32 = 256;

/// A decoded, RGBA image kept entirely in process memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// Construct an image only when its dimensions exactly describe its pixels.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        let expected_len = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(4)?;
        (width > 0 && height > 0 && pixels.len() == expected_len).then_some(Self {
            width,
            height,
            pixels,
        })
    }
}

/// Authenticate, decode, and shrink an image without any persistence or upload
/// path in between.
///
/// `decode` is not called when `authenticate` rejects the ciphertext.  Keeping
/// that ordering in one small primitive prevents a caller from accidentally
/// deriving a preview from unauthenticated attachment bytes.
pub fn thumbnail_after_aead<Bytes, Error, Authenticate, Decode>(
    authenticate: Authenticate,
    decode: Decode,
) -> Result<RgbaImage, Error>
where
    Authenticate: FnOnce() -> Result<Bytes, Error>,
    Decode: FnOnce(Bytes) -> Result<RgbaImage, Error>,
{
    let authenticated_bytes = authenticate()?;
    let decoded = decode(authenticated_bytes)?;
    Ok(resize_to_thumbnail(decoded))
}

fn resize_to_thumbnail(image: RgbaImage) -> RgbaImage {
    let longest_edge = image.width.max(image.height);
    if longest_edge <= THUMBNAIL_LONGEST_EDGE {
        return image;
    }

    let (width, height) = if image.width >= image.height {
        (
            THUMBNAIL_LONGEST_EDGE,
            scaled_dimension(image.height, THUMBNAIL_LONGEST_EDGE, image.width),
        )
    } else {
        (
            scaled_dimension(image.width, THUMBNAIL_LONGEST_EDGE, image.height),
            THUMBNAIL_LONGEST_EDGE,
        )
    };
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for y in 0..height {
        let source_y = (u64::from(y) * u64::from(image.height) / u64::from(height)) as u32;
        for x in 0..width {
            let source_x = (u64::from(x) * u64::from(image.width) / u64::from(width)) as u32;
            let source_offset = ((source_y * image.width + source_x) * 4) as usize;
            let target_offset = ((y * width + x) * 4) as usize;
            pixels[target_offset..target_offset + 4]
                .copy_from_slice(&image.pixels[source_offset..source_offset + 4]);
        }
    }
    // The dimensions and allocation are constructed together above.
    RgbaImage::new(width, height, pixels).expect("thumbnail dimensions are valid")
}

fn scaled_dimension(dimension: u32, target_longest_edge: u32, original_longest_edge: u32) -> u32 {
    ((u64::from(dimension) * u64::from(target_longest_edge)) / u64::from(original_longest_edge))
        .max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn tampered_ciphertext_never_reaches_the_thumbnail_decoder() {
        let decode_called = Cell::new(false);
        let result = thumbnail_after_aead(
            || Err::<Vec<u8>, _>("authentication failed"),
            |_| {
                decode_called.set(true);
                Ok(RgbaImage::new(1, 1, vec![1, 2, 3, 4]).unwrap())
            },
        );

        assert_eq!(result, Err("authentication failed"));
        assert!(!decode_called.get());
    }

    #[test]
    fn authenticated_image_is_resized_in_memory() {
        let result = thumbnail_after_aead(
            || Ok::<_, ()>(vec![9, 8, 7]),
            |_| Ok(RgbaImage::new(512, 1, vec![4; 512 * 4]).unwrap()),
        )
        .unwrap();

        assert_eq!((result.width, result.height), (256, 1));
        assert_eq!(result.pixels, vec![4; 256 * 4]);
    }
}
