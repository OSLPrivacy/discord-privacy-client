//! Stego encoders for Discord-bound ciphertext.
//!
//! v1 alpha ships **Mode 0** only — a base64 placeholder with a
//! recognisable magic prefix. Mode 0 is **not** fluent stego: a human
//! scrolling a Discord channel will see "DPC0::<base64...>" and
//! immediately know an encrypted message lives there. That's
//! acceptable for prototype mode where both endpoints are dev devices
//! and the Discord channel is private; v1 stable replaces Mode 0 with
//! Mode 1 (template-based fluency).
//!
//! ## Hard architectural requirement: per-message independence
//!
//! Every stego'd message must be decodable from **itself plus the
//! shared secret**, with no reference to any other message. Discord
//! can reorder, edit, or delete messages on its CDN; context-dependent
//! stego (where decoding message N depends on N-1, N-2, ...) breaks
//! unrecoverably the moment any context message is lost. Making
//! context-dependent stego reliable would require storing messages on
//! our own server, converting the project from "privacy layer over
//! Discord" into "Discord-skinned messenger with separate storage" —
//! defeating the project thesis.
//!
//! Mode 0 trivially satisfies this (each message is a self-contained
//! base64 string); the constraint is documented here as a design
//! invariant for Modes 1, 2, and 3.
//!
//! ## Wire format (Mode 0)
//!
//! ```text
//! DPC0::<base64-standard-padding(ciphertext)>
//! ```
//!
//! - Prefix `DPC0::` is the encoder identifier — `DPC` for Discord
//!   Privacy Client, `0` for Mode 0, `::` as a delimiter that's
//!   trivially scannable in plain text and won't be stripped or
//!   "smart-quoted" by Discord's text rendering.
//! - Body uses standard base64 alphabet (`A-Z a-z 0-9 + /`) with `=`
//!   padding. Discord preserves these characters verbatim.
//!
//! Decoders that don't recognise the prefix MUST treat the message
//! as cover plaintext and skip stego processing entirely (the
//! prototype receiver only invokes the decoder when it has a reason
//! to expect a stego'd message — for the v1 alpha test loop this is
//! "every message in the configured private channel").

mod mode0;
mod mode1;
mod mode1_chunking;
mod mode1_reassembly;
mod mode1_templates;
mod mode1_wordlists;

pub use mode0::{
    decode_mode0, encode_mode0, is_mode0, MODE0_MAX_RAW_LEN, MODE0_PREFIX, MODE0_PREFIX_BYTES,
};
pub use mode1::{
    decode_mode1, encode_mode1, is_mode1, ConversationCipher, MODE1_MAX_RAW_LEN, MODE1_PREFIX,
    PERMUTATION_DOMAIN,
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

    #[error("Mode 0 message exceeded the {max}-byte raw length limit (got {got})")]
    Mode0TooLong { got: usize, max: usize },

    #[error("not a Mode 1 stego message (missing DPC1:: prefix)")]
    NotMode1,

    #[error("Mode 1 message exceeded the {max}-byte raw length limit (got {got})")]
    Mode1TooLong { got: usize, max: usize },

    #[error("Mode 1 parse error: {0}")]
    Mode1ParseError(String),
}

pub type Result<T> = core::result::Result<T, Error>;
