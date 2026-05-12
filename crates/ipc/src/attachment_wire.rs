//! Phase 8: attachment wire-format wrapper.
//!
//! Layered on top of `crypto::attachment` (streaming
//! XChaCha20-Poly1305, bucketed padding). This module adds:
//!
//! - A decoy PNG prefix so non-OSL viewers who fetch the file from
//!   Discord's CDN see a valid placeholder image (rather than
//!   binary garbage).
//! - A 16-byte ASCII magic so the receive side can locate the
//!   encrypted payload inside the concatenated file without
//!   trusting any external length metadata.
//! - A length-prefixed UTF-8 filename so the receiver can restore
//!   the original name and infer the MIME from its extension.
//!
//! Wire layout (everything past the decoy PNG is what gets emitted
//! into Discord's CDN as a single attachment file):
//!
//! ```text
//! [N bytes: decoy PNG ending in IEND chunk]
//! [16 bytes: magic "OSL-ATT1" + 8 null bytes]
//! [ 2 bytes BE: filename length L (<= MAX_FILENAME_LEN)]
//! [ L bytes:    original filename, UTF-8]
//! [variable:    crypto::attachment::encrypt_attachment output]
//! ```
//!
//! The decoy PNG is a self-contained solid-color PNG that Discord's
//! `attachments[].url` viewer renders without error. The receiver
//! finds the magic via a forward scan past the IEND chunk — we
//! never trust Discord's reported `size` or `width`/`height` since
//! that's attacker-controllable.

use crate::AppState;
use base64::Engine;
use crypto::aead;
use crypto::attachment as att;
use crypto::random;
use std::sync::OnceLock;

/// 16-byte magic that prefixes the encrypted-payload region of a
/// sealed attachment. Constant for the whole protocol lifetime;
/// changing it breaks compatibility with already-sent files.
pub const OSL_ATT_MAGIC: &[u8; 16] = b"OSL-ATT1\0\0\0\0\0\0\0\0";

/// Phase 8d: V2 magic. Distinguishes the new wire that embeds the
/// cover envelope inside the file (so message content can stay
/// empty and non-OSL viewers see no DPC0:: gibberish in the
/// channel). V1 magic is still recognised on the open path for
/// any legacy files in flight from Phase 8 / 8c.
pub const OSL_ATT_MAGIC_V2: &[u8; 16] = b"OSL-ATT2\0\0\0\0\0\0\0\0";

/// Max length of the embedded cover envelope. The cover is a v=2
/// wire — for a DM (1 recipient + self), the per-recipient header
/// is ~50 bytes; the CBOR-encoded `AttachmentEnvelope` is ~150 bytes
/// per attachment. 64 KB is enough for ~10 attachments × 10 recipients
/// with room to spare and bounds the open-path scan cost.
pub const MAX_COVER_BYTES: u32 = 64 * 1024;

/// Maximum original-filename length we'll seal or unseal. Keeps
/// the wire format bounded and the decode path inexpensive — Discord
/// itself caps filenames at ~128 chars in the UI, so 1024 is
/// generous.
pub const MAX_FILENAME_LEN: usize = 1024;

/// Maximum file size we'll seal. Discord stable's free-tier upload
/// cap is 25 MB; we leave 1 MB for the decoy PNG + AEAD framing
/// overhead.
pub const MAX_ATTACHMENT_BYTES: usize = 24 * 1024 * 1024;

/// MIME table for the supported attachment types. Receiver uses
/// this to construct the blob URL with the correct content-type so
/// the browser renders the result as an image or video element.
pub fn mime_for_filename(name: &str) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "mp4" => Some("video/mp4"),
        "webm" => Some("video/webm"),
        "mov" => Some("video/quicktime"),
        _ => None,
    }
}

/// Errors surfaced from `seal_attachment` / `open_attachment`.
#[derive(Debug, thiserror::Error)]
pub enum AttachmentWireError {
    #[error("attachment too large: {0} bytes (max {1})")]
    TooLarge(usize, usize),
    #[error("filename too long: {0} bytes (max {1})")]
    FilenameTooLong(usize, usize),
    #[error("unsupported file extension")]
    UnsupportedExtension,
    #[error("OSL-ATT1 / OSL-ATT2 magic not found in file")]
    MagicNotFound,
    #[error("truncated payload: missing cover-length header")]
    TruncatedCoverHeader,
    #[error("cover-length {0} exceeds max {1}")]
    CoverTooLarge(u32, u32),
    #[error("truncated payload: cover bytes {0} exceeds remaining {1}")]
    TruncatedCover(u32, usize),
    #[error("truncated payload: missing filename header")]
    TruncatedFilenameHeader,
    #[error("truncated payload: filename length {0} exceeds remaining {1} bytes")]
    TruncatedFilename(usize, usize),
    #[error("filename is not valid UTF-8")]
    FilenameNotUtf8,
    #[error("inner streaming AEAD failed: {0}")]
    InnerCrypto(String),
    #[error("decoy PNG generation failed: {0}")]
    DecoyGeneration(String),
}

/// Cached decoy PNG bytes. Built once on first call and reused for
/// every subsequent seal. Solid Discord-dark-mode background
/// (#2b2d31) at 800×450 — visually identifies as a non-rendering
/// attachment without hand-rendered text. Text overlay is a v1
/// polish item that can replace this asset later.
static DECOY_PNG: OnceLock<Vec<u8>> = OnceLock::new();

/// Build the decoy PNG on first call. Pure-Rust via the `png`
/// crate, no font/text rendering — solid #2b2d31 rectangle.
fn decoy_png_bytes() -> &'static [u8] {
    DECOY_PNG.get_or_init(|| {
        const WIDTH: u32 = 800;
        const HEIGHT: u32 = 450;
        // Discord dark-mode chat background. (R, G, B).
        const BG: [u8; 3] = [0x2b, 0x2d, 0x31];
        let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
        for _ in 0..(WIDTH * HEIGHT) {
            pixels.extend_from_slice(&BG);
        }
        let mut out: Vec<u8> = Vec::with_capacity(4096);
        {
            let mut encoder = png::Encoder::new(&mut out, WIDTH, HEIGHT);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder
                .write_header()
                .expect("png header write should never fail with valid params");
            writer
                .write_image_data(&pixels)
                .expect("png image-data write should never fail with valid params");
        }
        out
    })
}

/// Public accessor exposed for tests and for the seal/open pipeline.
pub fn decoy_png() -> &'static [u8] {
    decoy_png_bytes()
}

/// Seal `plaintext` for upload to Discord. Caller supplies the
/// per-attachment AEAD key (generated fresh by the send-gate and
/// shared to recipients via the message-text envelope).
///
/// Returns the full byte stream that should be uploaded as the
/// attachment file: decoy PNG prefix, then magic, then length-
/// prefixed filename, then the streaming-AEAD ciphertext.
pub fn seal_attachment(
    key: aead::Key,
    plaintext: &[u8],
    original_filename: &str,
) -> Result<Vec<u8>, AttachmentWireError> {
    if plaintext.len() > MAX_ATTACHMENT_BYTES {
        return Err(AttachmentWireError::TooLarge(
            plaintext.len(),
            MAX_ATTACHMENT_BYTES,
        ));
    }
    let fn_bytes = original_filename.as_bytes();
    if fn_bytes.len() > MAX_FILENAME_LEN {
        return Err(AttachmentWireError::FilenameTooLong(
            fn_bytes.len(),
            MAX_FILENAME_LEN,
        ));
    }
    if mime_for_filename(original_filename).is_none() {
        return Err(AttachmentWireError::UnsupportedExtension);
    }

    // The streaming AEAD takes a content_id (bound into the AEAD
    // associated-data alongside chunk indices). We don't have a
    // parent-message id at seal time, so use 16 random bytes —
    // freshly drawn per attachment. The receiver doesn't need to
    // reproduce the content_id; the AEAD header in the wire bytes
    // carries it.
    let content_id = random::random_bytes(16);
    let inner_wire = att::encrypt_attachment(key, plaintext, content_id, 0)
        .map_err(|e| AttachmentWireError::InnerCrypto(format!("{e:?}")))?;

    let decoy = decoy_png_bytes();
    let mut out = Vec::with_capacity(
        decoy.len() + OSL_ATT_MAGIC.len() + 2 + fn_bytes.len() + inner_wire.len(),
    );
    out.extend_from_slice(decoy);
    out.extend_from_slice(OSL_ATT_MAGIC);
    out.extend_from_slice(&(fn_bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(fn_bytes);
    out.extend_from_slice(&inner_wire);
    Ok(out)
}

/// Locate the OSL-ATT1 magic inside a Discord-served attachment
/// blob. Returns the byte offset of the first byte of magic. A
/// scan rather than a fixed offset because the decoy PNG can have
/// trailing chunks (text/tIME) appended by image-processing layers
/// in Discord's CDN, and we want the open path to survive that.
pub fn find_payload_offset(file_bytes: &[u8]) -> Option<usize> {
    file_bytes
        .windows(OSL_ATT_MAGIC.len())
        .position(|w| w == OSL_ATT_MAGIC)
}

/// Unseal a Discord-served attachment blob. Scans for the magic,
/// extracts the filename, and runs the streaming-AEAD decryptor
/// against the trailing ciphertext using `key`.
pub fn open_attachment(
    key: aead::Key,
    file_bytes: &[u8],
) -> Result<(Vec<u8>, String), AttachmentWireError> {
    let magic_off = find_payload_offset(file_bytes).ok_or(AttachmentWireError::MagicNotFound)?;
    let after_magic = magic_off + OSL_ATT_MAGIC.len();
    if after_magic + 2 > file_bytes.len() {
        return Err(AttachmentWireError::TruncatedFilenameHeader);
    }
    let fn_len =
        u16::from_be_bytes([file_bytes[after_magic], file_bytes[after_magic + 1]]) as usize;
    if fn_len > MAX_FILENAME_LEN {
        return Err(AttachmentWireError::FilenameTooLong(
            fn_len,
            MAX_FILENAME_LEN,
        ));
    }
    let fn_start = after_magic + 2;
    let fn_end = fn_start + fn_len;
    if fn_end > file_bytes.len() {
        return Err(AttachmentWireError::TruncatedFilename(
            fn_len,
            file_bytes.len() - fn_start,
        ));
    }
    let filename = std::str::from_utf8(&file_bytes[fn_start..fn_end])
        .map_err(|_| AttachmentWireError::FilenameNotUtf8)?
        .to_string();
    let inner_wire = &file_bytes[fn_end..];
    let plaintext = att::decrypt_attachment(key, inner_wire)
        .map_err(|e| AttachmentWireError::InnerCrypto(format!("{e:?}")))?;
    Ok((plaintext, filename))
}

/// Phase 8d: V2 seal — embeds the (already-built) cover envelope
/// bytes inside the file so the Discord message can ship with empty
/// content. Caller supplies the AEAD key for the file payload (the
/// same key whose value lives inside `cover_bytes`, encrypted to the
/// scope's recipients) and the cover bytes themselves. This
/// separation keeps `attachment_wire` ignorant of v=2 / wire_v2 —
/// the cover is opaque to it.
///
/// Layout (everything past the decoy PNG is what gets uploaded):
///
/// ```text
/// [N bytes:        decoy PNG]
/// [16 bytes:       OSL_ATT_MAGIC_V2]
/// [ 4 bytes BE:    cover_len C (<= MAX_COVER_BYTES)]
/// [ C bytes:       opaque cover (v=2 wire encoding the AttachmentEnvelope)]
/// [ 2 bytes BE:    filename length L (<= MAX_FILENAME_LEN)]
/// [ L bytes:       UTF-8 filename]
/// [variable:       crypto::attachment streaming-AEAD wire of file payload]
/// ```
pub fn seal_attachment_v2(
    file_key: aead::Key,
    plaintext: &[u8],
    original_filename: &str,
    cover_bytes: &[u8],
) -> Result<Vec<u8>, AttachmentWireError> {
    if plaintext.len() > MAX_ATTACHMENT_BYTES {
        return Err(AttachmentWireError::TooLarge(
            plaintext.len(),
            MAX_ATTACHMENT_BYTES,
        ));
    }
    let fn_bytes = original_filename.as_bytes();
    if fn_bytes.len() > MAX_FILENAME_LEN {
        return Err(AttachmentWireError::FilenameTooLong(
            fn_bytes.len(),
            MAX_FILENAME_LEN,
        ));
    }
    if mime_for_filename(original_filename).is_none() {
        return Err(AttachmentWireError::UnsupportedExtension);
    }
    if cover_bytes.len() > MAX_COVER_BYTES as usize {
        return Err(AttachmentWireError::CoverTooLarge(
            cover_bytes.len() as u32,
            MAX_COVER_BYTES,
        ));
    }

    let content_id = random::random_bytes(16);
    let inner_wire = att::encrypt_attachment(file_key, plaintext, content_id, 0)
        .map_err(|e| AttachmentWireError::InnerCrypto(format!("{e:?}")))?;

    let decoy = decoy_png_bytes();
    let mut out = Vec::with_capacity(
        decoy.len()
            + OSL_ATT_MAGIC_V2.len()
            + 4
            + cover_bytes.len()
            + 2
            + fn_bytes.len()
            + inner_wire.len(),
    );
    out.extend_from_slice(decoy);
    out.extend_from_slice(OSL_ATT_MAGIC_V2);
    out.extend_from_slice(&(cover_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(cover_bytes);
    out.extend_from_slice(&(fn_bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(fn_bytes);
    out.extend_from_slice(&inner_wire);
    Ok(out)
}

/// Phase 8d V2 open: split a file blob into its `(cover_bytes,
/// filename, file_payload_bytes)` triple WITHOUT performing any
/// decryption. The caller decrypts the cover via the existing
/// wire_v2 path (which knows about identity keys + scope rules)
/// and the payload via `crypto::attachment::decrypt_attachment`.
///
/// Falls back to the V1 magic on miss. For V1, returns
/// `(empty cover_bytes, filename, payload_bytes)` — the V1 caller
/// uses an out-of-band cover (the DPC0:: message text).
pub fn open_attachment_v2_split(
    file_bytes: &[u8],
) -> Result<(Vec<u8>, String, Vec<u8>), AttachmentWireError> {
    // Prefer V2 magic; fall back to V1 for legacy files.
    let v2_off = file_bytes
        .windows(OSL_ATT_MAGIC_V2.len())
        .position(|w| w == OSL_ATT_MAGIC_V2);
    if let Some(off) = v2_off {
        let mut p = off + OSL_ATT_MAGIC_V2.len();
        if p + 4 > file_bytes.len() {
            return Err(AttachmentWireError::TruncatedCoverHeader);
        }
        let cover_len = u32::from_be_bytes([
            file_bytes[p],
            file_bytes[p + 1],
            file_bytes[p + 2],
            file_bytes[p + 3],
        ]);
        p += 4;
        if cover_len > MAX_COVER_BYTES {
            return Err(AttachmentWireError::CoverTooLarge(
                cover_len,
                MAX_COVER_BYTES,
            ));
        }
        if p + cover_len as usize > file_bytes.len() {
            return Err(AttachmentWireError::TruncatedCover(
                cover_len,
                file_bytes.len() - p,
            ));
        }
        let cover = file_bytes[p..p + cover_len as usize].to_vec();
        p += cover_len as usize;
        if p + 2 > file_bytes.len() {
            return Err(AttachmentWireError::TruncatedFilenameHeader);
        }
        let fn_len = u16::from_be_bytes([file_bytes[p], file_bytes[p + 1]]) as usize;
        p += 2;
        if fn_len > MAX_FILENAME_LEN {
            return Err(AttachmentWireError::FilenameTooLong(
                fn_len,
                MAX_FILENAME_LEN,
            ));
        }
        if p + fn_len > file_bytes.len() {
            return Err(AttachmentWireError::TruncatedFilename(
                fn_len,
                file_bytes.len() - p,
            ));
        }
        let filename = std::str::from_utf8(&file_bytes[p..p + fn_len])
            .map_err(|_| AttachmentWireError::FilenameNotUtf8)?
            .to_string();
        p += fn_len;
        let payload = file_bytes[p..].to_vec();
        return Ok((cover, filename, payload));
    }
    // V1 fallback. Cover is empty — the caller has it from message text.
    let v1_off = find_payload_offset(file_bytes).ok_or(AttachmentWireError::MagicNotFound)?;
    let mut p = v1_off + OSL_ATT_MAGIC.len();
    if p + 2 > file_bytes.len() {
        return Err(AttachmentWireError::TruncatedFilenameHeader);
    }
    let fn_len = u16::from_be_bytes([file_bytes[p], file_bytes[p + 1]]) as usize;
    p += 2;
    if fn_len > MAX_FILENAME_LEN {
        return Err(AttachmentWireError::FilenameTooLong(
            fn_len,
            MAX_FILENAME_LEN,
        ));
    }
    if p + fn_len > file_bytes.len() {
        return Err(AttachmentWireError::TruncatedFilename(
            fn_len,
            file_bytes.len() - p,
        ));
    }
    let filename = std::str::from_utf8(&file_bytes[p..p + fn_len])
        .map_err(|_| AttachmentWireError::FilenameNotUtf8)?
        .to_string();
    p += fn_len;
    let payload = file_bytes[p..].to_vec();
    Ok((Vec::new(), filename, payload))
}

/// 8-hex-char random filename used as the upload name. Always
/// ".png" because the decoy prefix is a PNG; Discord's CDN
/// renderers will display the decoy when the URL is visited
/// directly.
pub fn random_upload_filename() -> String {
    let bytes = random::random_bytes(4);
    format!(
        "{:02x}{:02x}{:02x}{:02x}.png",
        bytes[0], bytes[1], bytes[2], bytes[3]
    )
}

// ---------- Tauri-facing DTOs / commands -----------------------------

/// Result returned to JS from `osl_seal_attachment`. JS will use
/// `file_blob_b64` as the upload body, `random_filename` as the
/// upload name, and pass `att_key_b64` + `original_filename` +
/// `mime_type` through the v=2 envelope to recipients.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SealedAttachment {
    pub file_blob_b64: String,
    pub random_filename: String,
    pub att_key_b64: String,
    pub mime_type: String,
}

/// Result returned to JS from `osl_open_attachment`. JS turns
/// `plaintext_b64` into a Blob with `mime_type`, builds a blob URL,
/// and swaps the rendered Discord attachment element's `src`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedAttachment {
    pub plaintext_b64: String,
    pub original_filename: String,
    pub mime_type: String,
}

/// Phase 8 send-side Tauri command (inner). Accepts the base64
/// encoding of the user-picked file so callers (main.rs Tauri
/// wrappers) don't need a direct base64 dep. Allocates a fresh
/// 32-byte AEAD key, seals the bytes with it, returns the
/// upload-ready bytes + the key so the v=2 envelope can carry it
/// to recipients.
pub fn cmd_osl_seal_attachment_b64(
    state: &AppState,
    original_bytes_b64: &str,
    original_filename: String,
) -> Result<SealedAttachment, String> {
    let original_bytes = base64::engine::general_purpose::STANDARD
        .decode(original_bytes_b64)
        .map_err(|e| format!("OSL: original_bytes b64 decode: {e}"))?;
    cmd_osl_seal_attachment(state, original_bytes, original_filename)
}

/// Phase 8 send-side Tauri command (inner). Allocates a fresh
/// 32-byte AEAD key, seals `original_bytes` with it, returns the
/// upload-ready bytes + the key so the v=2 envelope can carry it
/// to recipients.
pub fn cmd_osl_seal_attachment(
    _state: &AppState,
    original_bytes: Vec<u8>,
    original_filename: String,
) -> Result<SealedAttachment, String> {
    let mime = mime_for_filename(&original_filename)
        .ok_or_else(|| "OSL: unsupported file extension".to_string())?;
    let key_bytes = random::random_bytes(32);
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key_bytes);
    let key = aead::Key::from_bytes(key_arr);

    let sealed = seal_attachment(key, &original_bytes, &original_filename)
        .map_err(|e| format!("OSL: seal_attachment: {e}"))?;
    let file_blob_b64 = base64::engine::general_purpose::STANDARD.encode(&sealed);
    let att_key_b64 = base64::engine::general_purpose::STANDARD.encode(&key_arr);
    let random_filename = random_upload_filename();
    Ok(SealedAttachment {
        file_blob_b64,
        random_filename,
        att_key_b64,
        mime_type: mime.to_string(),
    })
}

/// Phase 8 receive-side Tauri command (inner). Same as
/// [`cmd_osl_open_attachment`] but takes base64 strings directly so
/// the Tauri wrapper doesn't need its own base64 dep.
pub fn cmd_osl_open_attachment_b64(
    state: &AppState,
    att_key_b64: String,
    file_bytes_b64: &str,
) -> Result<OpenedAttachment, String> {
    let file_bytes = base64::engine::general_purpose::STANDARD
        .decode(file_bytes_b64)
        .map_err(|e| format!("OSL: file_bytes b64 decode: {e}"))?;
    cmd_osl_open_attachment(state, att_key_b64, file_bytes)
}

/// Phase 8 receive-side Tauri command (inner). Decrypts a Discord-
/// CDN-served blob using the AEAD key from the v=2 envelope.
pub fn cmd_osl_open_attachment(
    _state: &AppState,
    att_key_b64: String,
    file_bytes: Vec<u8>,
) -> Result<OpenedAttachment, String> {
    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(&att_key_b64)
        .map_err(|e| format!("OSL: att_key b64 decode: {e}"))?;
    if key_bytes.len() != 32 {
        return Err(format!(
            "OSL: att_key must be 32 bytes, got {}",
            key_bytes.len()
        ));
    }
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key_bytes);
    let key = aead::Key::from_bytes(key_arr);
    let (plaintext, original_filename) =
        open_attachment(key, &file_bytes).map_err(|e| format!("OSL: open_attachment: {e}"))?;
    let mime = mime_for_filename(&original_filename)
        .ok_or_else(|| "OSL: unsupported file extension on decrypted filename".to_string())?;
    let plaintext_b64 = base64::engine::general_purpose::STANDARD.encode(&plaintext);
    Ok(OpenedAttachment {
        plaintext_b64,
        original_filename,
        mime_type: mime.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_key() -> aead::Key {
        let mut k = [0u8; 32];
        k.copy_from_slice(&random::random_bytes(32));
        aead::Key::from_bytes(k)
    }

    #[test]
    fn round_trip_small_image() {
        let key = fresh_key();
        // Borrow the decoy PNG bytes as fake "small image" plaintext —
        // it's a real PNG with a known shape so seal+open survives
        // any latent encoding fragility.
        let plain = decoy_png().to_vec();
        let wire = seal_attachment(key.clone(), &plain, "photo.png").unwrap();
        // The decoy + framing must precede any AEAD output, so the
        // first few bytes must match the PNG signature.
        assert_eq!(
            &wire[..8],
            &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']
        );
        let (recovered, fname) = open_attachment(key, &wire).unwrap();
        assert_eq!(recovered, plain);
        assert_eq!(fname, "photo.png");
    }

    #[test]
    fn round_trip_video_bytes() {
        // MP4 atom-style stub bytes — content is opaque to the
        // crypto layer; we just want the open path to recognise
        // the filename's extension.
        let key = fresh_key();
        let plain = vec![0x66u8; 200 * 1024]; // 200 KB
        let wire = seal_attachment(key.clone(), &plain, "clip.mp4").unwrap();
        let (recovered, fname) = open_attachment(key, &wire).unwrap();
        assert_eq!(recovered, plain);
        assert_eq!(fname, "clip.mp4");
    }

    #[test]
    fn special_char_filename_preserved() {
        let key = fresh_key();
        let plain = vec![1, 2, 3, 4];
        let weird = "hello world — déjà vu.jpg";
        let wire = seal_attachment(key.clone(), &plain, weird).unwrap();
        let (_, fname) = open_attachment(key, &wire).unwrap();
        assert_eq!(fname, weird);
    }

    #[test]
    fn unsupported_extension_rejected() {
        let key = fresh_key();
        let plain = vec![0u8; 10];
        let err = seal_attachment(key, &plain, "passwords.zip").unwrap_err();
        matches!(err, AttachmentWireError::UnsupportedExtension);
    }

    #[test]
    fn oversized_file_rejected() {
        let key = fresh_key();
        let plain = vec![0u8; MAX_ATTACHMENT_BYTES + 1];
        let err = seal_attachment(key, &plain, "huge.png").unwrap_err();
        matches!(err, AttachmentWireError::TooLarge(_, _));
    }

    #[test]
    fn tampered_ciphertext_fails_auth() {
        let key = fresh_key();
        let plain = vec![1u8; 32 * 1024];
        let mut wire = seal_attachment(key.clone(), &plain, "img.png").unwrap();
        // Flip one byte deep inside the AEAD body — beyond the
        // decoy + magic + filename — so the open path actually
        // hits the AEAD verification.
        let last_idx = wire.len() - 1;
        wire[last_idx] ^= 0x01;
        let err = open_attachment(key, &wire).unwrap_err();
        matches!(err, AttachmentWireError::InnerCrypto(_));
    }

    #[test]
    fn tampered_magic_fails_detection() {
        let key = fresh_key();
        let plain = vec![1u8; 1024];
        let mut wire = seal_attachment(key.clone(), &plain, "img.png").unwrap();
        let off = find_payload_offset(&wire).unwrap();
        // Corrupt the magic byte in place.
        wire[off] = b'X';
        let err = open_attachment(key, &wire).unwrap_err();
        matches!(err, AttachmentWireError::MagicNotFound);
    }

    #[test]
    fn random_filename_shape() {
        let f = random_upload_filename();
        assert!(f.ends_with(".png"));
        assert_eq!(f.len(), "abcd1234.png".len());
        for c in f.trim_end_matches(".png").chars() {
            assert!(c.is_ascii_hexdigit(), "non-hex char in random filename");
        }
    }

    #[test]
    fn mime_table_covers_all_supported_kinds() {
        assert_eq!(mime_for_filename("a.jpg"), Some("image/jpeg"));
        assert_eq!(mime_for_filename("a.JPG"), Some("image/jpeg"));
        assert_eq!(mime_for_filename("a.jpeg"), Some("image/jpeg"));
        assert_eq!(mime_for_filename("a.png"), Some("image/png"));
        assert_eq!(mime_for_filename("a.gif"), Some("image/gif"));
        assert_eq!(mime_for_filename("a.webp"), Some("image/webp"));
        assert_eq!(mime_for_filename("a.mp4"), Some("video/mp4"));
        assert_eq!(mime_for_filename("a.webm"), Some("video/webm"));
        assert_eq!(mime_for_filename("a.mov"), Some("video/quicktime"));
        assert_eq!(mime_for_filename("a.txt"), None);
        assert_eq!(mime_for_filename("no_extension"), None);
    }

    #[test]
    fn decoy_png_is_valid_signature() {
        let d = decoy_png();
        assert_eq!(
            &d[..8],
            &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']
        );
    }
}
