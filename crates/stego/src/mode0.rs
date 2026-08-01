//! Mode 0: internal base64 envelope.
//!
//! This is not a carrier transport. It re-wraps ciphertext retrieved through
//! the pointer-only transport so the established decrypt pipeline can consume
//! it as `DPC0::<base64>`.

use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Mode 0 magic prefix. Carried verbatim on the wire so receivers can
/// detect a stego'd message vs. cover plaintext without trial decode.
pub const MODE0_PREFIX: &str = "DPC0::";

/// Mode 0 magic prefix as bytes.
pub const MODE0_PREFIX_BYTES: &[u8] = MODE0_PREFIX.as_bytes();

/// Wrap raw ciphertext bytes in the internal Mode 0 envelope.
pub fn encode_mode0(ciphertext: &[u8]) -> Result<String> {
    let body = STANDARD.encode(ciphertext);
    let mut out = String::with_capacity(MODE0_PREFIX.len() + body.len());
    out.push_str(MODE0_PREFIX);
    out.push_str(&body);
    Ok(out)
}

/// Cheap detection: does this message carry a Mode 0 prefix?
pub fn is_mode0(msg: &str) -> bool {
    msg.starts_with(MODE0_PREFIX)
}

/// Decode a Mode 0 stego message back to raw ciphertext bytes.
///
/// Errors:
/// - [`Error::NotMode0`] — message does not start with the Mode 0 prefix.
/// - [`Error::Mode0Base64`] — body is not valid base64.
pub fn decode_mode0(msg: &str) -> Result<Vec<u8>> {
    let body = msg.strip_prefix(MODE0_PREFIX).ok_or(Error::NotMode0)?;
    STANDARD
        .decode(body.as_bytes())
        .map_err(|e| Error::Mode0Base64(e.to_string()))
}
