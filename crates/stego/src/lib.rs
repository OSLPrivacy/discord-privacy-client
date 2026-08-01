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
pub mod line_shape;
mod mode0;
mod mode1;
mod mode1_chunking;
mod mode1_reassembly;
mod mode1_templates;
mod mode1_wordlists;

pub use line_shape::{
    encode_mode1_shaped, encode_token_shaped, rendered_rows, rows_for_hard_lines, shape_cover,
    RowBudget, RowMatch, ShapedCover, MAX_SHAPED_ROWS,
};
pub use mode0::{
    decode_mode0, encode_mode0, is_mode0, MODE0_MAX_RAW_LEN, MODE0_PREFIX, MODE0_PREFIX_BYTES,
};
pub use mode1::{
    decode_mode1, decode_token, encode_mode1, encode_token, is_mode1, ConversationCipher,
    MODE1_MAX_RAW_LEN, MODE1_PREFIX, PERMUTATION_DOMAIN, TOKEN_ID_BYTES, TOKEN_MAC_BYTES,
    TOKEN_MAC_DOMAIN, TOKEN_PAYLOAD_BITS,
};
pub use mode1_chunking::{
    chunk_payload, chunk_payload_with_cipher, parse_chunk, ChunkError, ParsedChunk,
    SerializedChunk, CHUNK_HEADER_BYTES, CHUNK_HMAC_DOMAIN, CHUNK_MAX_TOTAL, CHUNK_PAYLOAD_BYTES,
};
pub use mode1_reassembly::{
    PushOutcome, ReassemblyBuffer, ReassemblyComplete, MAX_CONCURRENT_SESSIONS,
    SESSION_TIMEOUT_SECS,
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
}

pub type Result<T> = core::result::Result<T, Error>;
