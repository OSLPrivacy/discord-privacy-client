use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("AEAD operation failed (wrong key, nonce, AD, or tampered ciphertext)")]
    AeadFailure,

    #[error("plaintext too large for any padding bucket (max {max} bytes, got {got})")]
    PaddingOverflow { max: usize, got: usize },

    #[error("padded ciphertext shorter than length prefix")]
    PaddingTruncated,

    #[error("padded length {claimed} exceeds bucket payload capacity {capacity}")]
    PaddingMalformed { claimed: usize, capacity: usize },

    #[error("padded buffer length {got} is not a recognised bucket size")]
    PaddingUnknownBucket { got: usize },

    #[error("HKDF expand failed: {0}")]
    HkdfExpand(String),

    #[error("internal crypto error: {0}")]
    Internal(String),
}

pub type Result<T> = core::result::Result<T, Error>;
