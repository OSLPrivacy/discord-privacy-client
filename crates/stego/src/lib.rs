//! Stego encoders for pointer-only Discord carriers.
//!
//! Protected payloads never ride in a Discord carrier. A carrier holds only
//! the opaque capability for a blob in the cipher store; there is no inline
//! fallback. Mode 1 encodes that capability as cover text.
//!
//! Mode 0 is retained solely as an internal envelope after the pointed-to blob
//! has been fetched, so the existing decrypt pipeline can consume its wire.
//! It is not a shipping transport and has no Discord capacity limit.
//!
//! ## Internal envelope (Mode 0)
//!
//! ```text
//! DPC0::<base64-standard-padding(ciphertext)>
//! ```
//!
//! - Prefix `DPC0::` identifies the internal envelope.
//! - Body uses standard base64 alphabet (`A-Z a-z 0-9 + /`) with `=`
//!   padding.

pub mod bigram;
mod image_hidden;
pub mod line_shape;
mod mode0;
mod mode1;
mod mode1_templates;
mod mode1_wordlists;

pub use image_hidden::{
    decode_png_hidden_pointer, decode_png_hidden_pointer_bytes, encode_png_hidden_pointer_bytes,
    encode_png_hidden_pointer_copy, ImageHiddenPointer, IMAGE_HIDDEN_CHECK_MARK_BYTES,
    IMAGE_HIDDEN_POINTER_BYTES,
};
pub use line_shape::{
    encode_mode1_shaped, encode_token_shaped, rendered_rows, rows_for_hard_lines, shape_cover,
    RowBudget, RowMatch, ShapedCover, MAX_SHAPED_ROWS,
};
pub use mode0::{
    decode_mode0, encode_mode0, is_mode0, MODE0_MAX_RAW_LEN, MODE0_PREFIX, MODE0_PREFIX_BYTES,
};
pub use mode1::{
    compute_shrunk_token_tag, compute_token_tag, decode_cover_message_token, decode_mode1,
    decode_shrunk_token, decode_token, encode_mode1, encode_shrunk_token, encode_token, is_mode1,
    ConversationCipher, CoverMessageToken, DETECT_TAG_BYTES, MODE1_MAX_RAW_LEN, MODE1_PREFIX,
    NEW_COVER_MESSAGE_VERSION, OLD_COVER_MESSAGE_VERSION, PERMUTATION_DOMAIN,
    SHRUNK_TOKEN_ID_BYTES, SHRUNK_TOKEN_MAC_DOMAIN, SHRUNK_TOKEN_PAYLOAD_BITS,
    SHRUNK_TOKEN_TAG_BYTES, TOKEN_ID_BYTES, TOKEN_MAC_BYTES, TOKEN_MAC_DOMAIN, TOKEN_PAYLOAD_BITS,
};
pub use mode1_templates::{
    SlotKind, BITS_PER_SENTENCE, SLOT_BITS, TEMPLATES_LEN, TEMPLATE_BITS, TOTAL_SLOTS,
};

use thiserror::Error;

/// Errors returned by the stego layer.
#[derive(Debug, Error)]
pub enum Error {
    #[error("not a Mode 0 stego message (missing DPC0:: prefix)")]
    NotMode0,

    #[error("Mode 0 base64 decode failed: {0}")]
    Mode0Base64(String),

    #[error("not a Mode 1 stego message (missing DPC1:: prefix)")]
    NotMode1,

    #[error("Mode 1 message exceeded the {max}-byte raw length limit (got {got})")]
    Mode1TooLong { got: usize, max: usize },

    #[error("Mode 1 parse error: {0}")]
    Mode1ParseError(String),

    #[error("unknown cover message version {0}")]
    UnknownCoverMessageVersion(u8),

    #[error("cover message version {version} did not decode as {kind}")]
    CoverMessageDecode { version: u8, kind: &'static str },

    #[error("image-hidden carrier I/O failed: {0}")]
    ImageHiddenIo(#[from] std::io::Error),

    #[error("image-hidden PNG failed: {0}")]
    ImageHiddenPng(String),

    #[error(
        "image-hidden PNG must be 8-bit RGB or RGBA, got color={color_type} depth={bit_depth}"
    )]
    ImageHiddenUnsupportedPng {
        color_type: String,
        bit_depth: String,
    },

    #[error(
        "image-hidden carrier capacity is too small (requires {required_bits} bits, got {capacity_bits})"
    )]
    ImageHiddenTooSmall {
        required_bits: usize,
        capacity_bits: usize,
    },
}

pub type Result<T> = core::result::Result<T, Error>;
