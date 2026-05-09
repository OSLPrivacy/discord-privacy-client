//! Mode 0: base64 placeholder stego.
//!
//! See the crate-level docs for the wire format and the rationale.

use crate::{Error, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

/// Mode 0 magic prefix. Carried verbatim on the wire so receivers can
/// detect a stego'd message vs. cover plaintext without trial decode.
pub const MODE0_PREFIX: &str = "DPC0::";

/// Mode 0 magic prefix as bytes.
pub const MODE0_PREFIX_BYTES: &[u8] = MODE0_PREFIX.as_bytes();

/// Discord's per-message text limit is 2000 characters for normal
/// users and 4000 for Nitro. The prototype caps Mode 0 raw input
/// length so a caller can't accidentally produce a stego payload that
/// blows past the limit. The cap is on the *raw* ciphertext bytes;
/// base64 inflates by 4/3, and the prefix adds 6 chars.
///
/// 1400-byte cap = 1400 * 4 / 3 + ceiling-pad ≈ 1868 base64 chars +
/// 6 prefix chars = 1874 chars on the wire — comfortably under 2000.
pub const MODE0_MAX_RAW_LEN: usize = 1400;

/// Encode raw ciphertext bytes as a Mode 0 stego message.
///
/// Returns `Err(Error::Mode0TooLong)` if the ciphertext exceeds
/// [`MODE0_MAX_RAW_LEN`]. Larger payloads should split across multiple
/// stego'd messages (per-message-independence requirement is unaffected
/// — each split chunk is its own self-contained AEAD ciphertext).
pub fn encode_mode0(ciphertext: &[u8]) -> Result<String> {
    if ciphertext.len() > MODE0_MAX_RAW_LEN {
        return Err(Error::Mode0TooLong {
            got: ciphertext.len(),
            max: MODE0_MAX_RAW_LEN,
        });
    }
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
