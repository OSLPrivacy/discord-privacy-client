//! View-once link lane — the path for a recipient who has no OSL and no
//! account.
//!
//! The recipient gets a link in Discord, opens it, sees the content
//! once, and it is gone. This module owns everything about that lane
//! that is cryptographic or textual: sealing the payload, minting the
//! capabilities, assembling the URL, and the exact wording shown to the
//! sender and the viewer.
//!
//! # Key custody
//!
//! The AES-256-GCM key is generated here and placed in the URL
//! **fragment**. Browsers do not transmit fragments — not in the request
//! line, not in `Referer`, not across redirects. The cipher-store
//! receives ciphertext and a SHA-256 digest of a bearer token, and has
//! no column, parameter or code path that could hold a key. It is
//! structurally incapable of decrypting what it stores.
//!
//! AES-256-GCM specifically, because it lets the recipient's browser
//! decrypt with native WebCrypto. The landing page ships **zero JS
//! crypto**.
//!
//! # This lane is structurally weaker than OSL-to-OSL
//!
//! It always will be. The strong lane can require a native protected
//! surface that OSL controls; a web browser has no such surface and no
//! API that could provide one. Everything the landing page does about
//! screenshots is deterrence, not prevention, and the wording in this
//! module says so in those words. Do not add a claim here that the
//! browser cannot keep.
//!
//! # Vocabulary rules enforced by tests
//!
//! - Banned anywhere in this lane's copy: `screenshot-protected`,
//!   `secure view`, `cannot be saved`, `disappears forever`.
//! - Sender-facing status may say `Link created`, `Retrieved at HH:MM`,
//!   `Expired without being retrieved`. It may **never** say read, seen
//!   or viewed — retrieval is not viewing.

use crate::aes_gcm::{self, Key, Nonce, KEY_SIZE, NONCE_SIZE, TAG_SIZE};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;
use zeroize::Zeroize;

/// Associated data bound into every view-once ciphertext. The landing
/// page passes the identical bytes as `additionalData`, so a blob lifted
/// from another OSL lane cannot be replayed into this one.
pub const LINK_AAD: &[u8] = b"OSL-VIEW-ONCE-LINK-v1";

/// Frame magic. Present in the plaintext, never on the wire in clear.
pub const FRAME_MAGIC: &[u8; 5] = b"OSLV1";

const KIND_TEXT: u8 = 0;
const KIND_IMAGE: u8 = 1;

/// Server-side ceiling on the sealed body (`nonce || ciphertext`).
/// Mirrors `MAX_LINK_BYTES` in `cipher-store-cf/src/endpoints/link.ts`.
/// Larger images must be downscaled before sealing.
pub const MAX_SEALED_BYTES: usize = 256 * 1024;

/// The only TTL the store accepts for this lane, so the sender warning's
/// "after 1 hour" is literally true.
pub const LINK_TTL_SECONDS: u64 = 3600;

/// The reservation window opened by the first retrieval. One view, or
/// this, whichever is first.
pub const RESERVATION_SECONDS: u64 = 60;

const TOKEN_BYTES: usize = 16;

/// Strings that must never appear in any copy this lane emits. Each one
/// promises something a browser cannot deliver.
pub const BANNED_CLAIMS: &[&str] = &[
    "screenshot-protected",
    "secure view",
    "cannot be saved",
    "disappears forever",
];

/// Words that must never appear in sender-facing status copy. Retrieval
/// is not viewing: the bytes left the server, which says nothing about
/// whether a human looked at them.
pub const BANNED_STATUS_WORDS: &[&str] = &["read", "seen", "viewed"];

/// Shown to the **sender** before the link is created. Verbatim, with
/// Markdown emphasis, as specified.
pub const SENDER_WARNING_MARKDOWN: &str = "**One-time link.** The link stops working after it's opened once, or after 1 hour \u{2014} whichever comes first. Anyone who has the link can open it, so treat it like a key. Whoever opens it can screenshot, record or photograph what they see; OSL can't prevent that in a web browser. **This link shows that you used a privacy tool.** For full protection, send to someone using OSL.";

/// The same warning without Markdown markers, for surfaces that apply
/// their own emphasis.
pub const SENDER_WARNING: &str = "One-time link. The link stops working after it's opened once, or after 1 hour \u{2014} whichever comes first. Anyone who has the link can open it, so treat it like a key. Whoever opens it can screenshot, record or photograph what they see; OSL can't prevent that in a web browser. This link shows that you used a privacy tool. For full protection, send to someone using OSL.";

/// Shown to the **viewer** before they reveal. Mirrors the landing page.
pub const VIEWER_PRE_REVEAL_NOTICE: &str =
    "This opens once. When you close it, the link stops working. Your screen isn't protected.";

/// The only claim this lane may make about lifetime.
pub const ENFORCEABLE_CLAIM: &str =
    "The link dies after one view, or 60 seconds, whichever is first.";

#[derive(Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Payload would exceed the store's per-link ceiling once sealed.
    TooLarge { max: usize, got: usize },
    /// MIME type is absent, over-long, or not printable ASCII.
    BadMime,
    /// Frame did not parse (truncated, bad magic, unknown kind).
    BadFrame,
    /// AEAD failure, or the sealed body was too short to contain a tag.
    AeadFailure,
    /// Base host was not an absolute `https://` origin.
    BadHost,
}

impl core::fmt::Display for LinkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            LinkError::TooLarge { max, got } => {
                write!(
                    f,
                    "view-once payload too large (max {max} bytes, got {got})"
                )
            }
            LinkError::BadMime => write!(f, "view-once MIME type is malformed"),
            LinkError::BadFrame => write!(f, "view-once frame is malformed"),
            LinkError::AeadFailure => write!(f, "view-once AEAD failed"),
            LinkError::BadHost => write!(f, "view-once host must be an https:// origin"),
        }
    }
}

impl std::error::Error for LinkError {}

pub type Result<T> = core::result::Result<T, LinkError>;

/// What the recipient's browser will render. Text is drawn to a canvas;
/// an image is decoded via `createImageBitmap` on an in-memory `Blob`
/// (no `<img>`, no object URL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkPayload {
    Text(String),
    Image { mime: String, bytes: Vec<u8> },
}

/// A 16-byte bearer capability. Only its SHA-256 digest reaches the
/// server, so a database leak yields nothing fetchable.
#[derive(Clone)]
pub struct Capability([u8; TOKEN_BYTES]);

impl Capability {
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_BYTES];
        OsRng.fill_bytes(&mut bytes);
        Capability(bytes)
    }

    pub fn from_bytes(bytes: [u8; TOKEN_BYTES]) -> Self {
        Capability(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; TOKEN_BYTES] {
        &self.0
    }

    /// Lowercase hex, the form the wire uses.
    pub fn to_hex(&self) -> String {
        to_hex(&self.0)
    }
}

impl Drop for Capability {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl core::fmt::Debug for Capability {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print a bearer capability, not even in a panic message.
        f.write_str("Capability(redacted)")
    }
}

/// Everything produced by sealing one payload.
///
/// `sealed_body` is the only part that goes to the server. `key` and
/// `fetch_token` go into the URL fragment and never leave the client's
/// process except inside that fragment. `manage_token` never leaves the
/// sender at all — it is not in the URL.
pub struct NewLink {
    /// `nonce (12) || AES-256-GCM(ciphertext || tag)`. Upload body.
    pub sealed_body: Vec<u8>,
    /// Fragment `k=`. Never transmitted to the server by any browser.
    pub key: Key,
    /// Fragment `t=`. Sent by the page in a POST **body**, never a URL.
    pub fetch_token: Capability,
    /// Sender-only: status and early revoke. Not in the URL.
    pub manage_token: Capability,
}

impl core::fmt::Debug for NewLink {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NewLink")
            .field("sealed_body_len", &self.sealed_body.len())
            .finish()
    }
}

/// Encode a payload into the frame the landing page parses.
///
/// `magic(5) | kind(1) | mime_len(u16 BE) | mime | body_len(u32 BE) | body`
pub fn encode_frame(payload: &LinkPayload) -> Result<Vec<u8>> {
    let (kind, mime, body): (u8, &str, &[u8]) = match payload {
        LinkPayload::Text(text) => (KIND_TEXT, "text/plain; charset=utf-8", text.as_bytes()),
        LinkPayload::Image { mime, bytes } => (KIND_IMAGE, mime.as_str(), bytes.as_slice()),
    };
    if mime.is_empty() || mime.len() > u16::MAX as usize {
        return Err(LinkError::BadMime);
    }
    if !mime
        .bytes()
        .all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\')
    {
        return Err(LinkError::BadMime);
    }
    if body.len() > u32::MAX as usize {
        return Err(LinkError::TooLarge {
            max: MAX_SEALED_BYTES,
            got: body.len(),
        });
    }
    let mut out = Vec::with_capacity(12 + mime.len() + body.len());
    out.extend_from_slice(FRAME_MAGIC);
    out.push(kind);
    out.extend_from_slice(&(mime.len() as u16).to_be_bytes());
    out.extend_from_slice(mime.as_bytes());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

/// Inverse of [`encode_frame`]. Used by tests and by any OSL-side
/// preview of what the recipient will see.
pub fn decode_frame(frame: &[u8]) -> Result<LinkPayload> {
    if frame.len() < 12 || &frame[0..5] != FRAME_MAGIC {
        return Err(LinkError::BadFrame);
    }
    let kind = frame[5];
    let mime_len = u16::from_be_bytes([frame[6], frame[7]]) as usize;
    let mime_end = 8 + mime_len;
    if frame.len() < mime_end + 4 {
        return Err(LinkError::BadFrame);
    }
    let mime = core::str::from_utf8(&frame[8..mime_end]).map_err(|_| LinkError::BadFrame)?;
    let body_len = u32::from_be_bytes([
        frame[mime_end],
        frame[mime_end + 1],
        frame[mime_end + 2],
        frame[mime_end + 3],
    ]) as usize;
    let body_start = mime_end + 4;
    let body_end = body_start
        .checked_add(body_len)
        .ok_or(LinkError::BadFrame)?;
    if frame.len() != body_end {
        return Err(LinkError::BadFrame);
    }
    let body = &frame[body_start..body_end];
    match kind {
        KIND_TEXT => Ok(LinkPayload::Text(
            String::from_utf8(body.to_vec()).map_err(|_| LinkError::BadFrame)?,
        )),
        KIND_IMAGE => Ok(LinkPayload::Image {
            mime: mime.to_string(),
            bytes: body.to_vec(),
        }),
        _ => Err(LinkError::BadFrame),
    }
}

/// Seal a payload for the view-once lane.
///
/// Generates a fresh AES-256 key and a fresh 12-byte nonce per call
/// (AES-GCM is catastrophically broken under nonce reuse; the key is
/// single-use here, so a repeat is impossible by construction).
pub fn seal(payload: &LinkPayload) -> Result<NewLink> {
    let frame = encode_frame(payload)?;
    let projected = NONCE_SIZE + frame.len() + TAG_SIZE;
    if projected > MAX_SEALED_BYTES {
        return Err(LinkError::TooLarge {
            max: MAX_SEALED_BYTES,
            got: projected,
        });
    }
    let mut key_bytes = [0u8; KEY_SIZE];
    OsRng.fill_bytes(&mut key_bytes);
    let key = Key::from_bytes(key_bytes);
    key_bytes.zeroize();

    let (nonce, ciphertext) =
        aes_gcm::seal(&key, LINK_AAD, &frame).map_err(|_| LinkError::AeadFailure)?;
    let mut sealed_body = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    sealed_body.extend_from_slice(nonce.as_bytes());
    sealed_body.extend_from_slice(&ciphertext);

    Ok(NewLink {
        sealed_body,
        key,
        fetch_token: Capability::generate(),
        manage_token: Capability::generate(),
    })
}

/// Inverse of [`seal`], for round-trip tests and OSL-side preview. The
/// production decrypt happens in the recipient's browser via WebCrypto.
pub fn open(key: &Key, sealed_body: &[u8]) -> Result<LinkPayload> {
    if sealed_body.len() <= NONCE_SIZE + TAG_SIZE {
        return Err(LinkError::AeadFailure);
    }
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    nonce_bytes.copy_from_slice(&sealed_body[..NONCE_SIZE]);
    let nonce = Nonce::from_bytes(nonce_bytes);
    let frame = aes_gcm::open(key, &nonce, LINK_AAD, &sealed_body[NONCE_SIZE..])
        .map_err(|_| LinkError::AeadFailure)?;
    decode_frame(&frame)
}

/// Build the recipient URL.
///
/// `https://<host>/v/<id>#k=<base64url key>&t=<hex fetch token>`
///
/// Everything secret is after the `#`. The path carries only the random
/// 16-byte id, which by itself yields the same content-free landing page
/// as any other id.
pub fn link_url(host: &str, id_hex: &str, key: &Key, fetch_token: &Capability) -> Result<String> {
    let host = host.trim_end_matches('/');
    let origin = if let Some(rest) = host.strip_prefix("https://") {
        if rest.is_empty() || rest.contains('/') {
            return Err(LinkError::BadHost);
        }
        host
    } else {
        return Err(LinkError::BadHost);
    };
    if id_hex.len() != 32 || !id_hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(LinkError::BadHost);
    }
    Ok(format!(
        "{origin}/v/{id_hex}#k={}&t={}",
        URL_SAFE_NO_PAD.encode(key.as_bytes()),
        fetch_token.to_hex()
    ))
}

/// The form OSL posts into Discord.
///
/// Angle brackets are Discord's documented suppression for embed
/// generation. Belt and braces alongside the content-free landing page:
/// even if the unfurler does fetch, it gets the same static bytes as
/// everybody else and cannot consume the view.
pub fn discord_link_text(url: &str) -> String {
    format!("<{url}>")
}

/// Sender-facing lifecycle state. Deliberately has no "read", "seen" or
/// "viewed" case, because the server cannot know any of those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    /// The link exists and nothing has retrieved it.
    Created,
    /// The bytes were released, at this unix-second timestamp. This says
    /// nothing about whether a human looked at them.
    Retrieved { at_unix_seconds: i64 },
    /// The hour elapsed with no retrieval.
    ExpiredWithoutRetrieval,
}

impl LinkStatus {
    /// The exact string to show the sender.
    ///
    /// `utc_offset_seconds` localises the clock — pass the sender's
    /// offset, or 0 for UTC.
    pub fn sender_label(&self, utc_offset_seconds: i64) -> String {
        match self {
            LinkStatus::Created => "Link created".to_string(),
            LinkStatus::Retrieved { at_unix_seconds } => {
                format!(
                    "Retrieved at {}",
                    hhmm(at_unix_seconds.saturating_add(utc_offset_seconds))
                )
            }
            LinkStatus::ExpiredWithoutRetrieval => "Expired without being retrieved".to_string(),
        }
    }
}

/// `HH:MM` from unix seconds, no date, no seconds — the coarsest form
/// that still answers the sender's question.
fn hhmm(unix_seconds: i64) -> String {
    let day_seconds = unix_seconds.rem_euclid(86_400);
    let hours = day_seconds / 3600;
    let minutes = (day_seconds % 3600) / 60;
    format!("{hours:02}:{minutes:02}")
}

// ---------------------------------------------------------------------
// Link-creation grants
// ---------------------------------------------------------------------
//
// Creation is restricted to authenticated OSL clients. Without that, the
// lane is an open, logless, self-deleting file host — a malware
// distribution service — which is what gets the domain Safe-Browsing
// flagged, gets the URL filtered by Discord, and gets enforcement taken
// against the *account* that also hosts the cipher-store and keyserver
// the strong lane depends on.
//
// A grant is anonymous by design: it proves "a vouched OSL client", not
// "this user". The cipher-store therefore never learns who made a link.

/// Domain separator for grant signatures. A `0x00` follows it, which
/// cannot occur inside the ASCII domain, so the signed message is
/// unambiguous.
pub const GRANT_DOMAIN: &str = "OSL-LINK-GRANT-v1";

/// Audience claim the cipher-store requires.
pub const GRANT_AUDIENCE: &str = "osl-link-create";

/// Maximum grant lifetime the cipher-store will accept.
pub const MAX_GRANT_LIFETIME_SECONDS: u64 = 600;

/// The exact bytes an issuer signs for a grant payload.
pub fn grant_signing_bytes(payload_json: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(GRANT_DOMAIN.len() + 1 + payload_json.len());
    out.extend_from_slice(GRANT_DOMAIN.as_bytes());
    out.push(0x00);
    out.extend_from_slice(payload_json);
    out
}

/// Canonical grant payload. Field order is fixed so the issuer and any
/// test agree byte-for-byte.
pub fn grant_payload_json(exp_unix_seconds: i64, jti_hex: &str) -> Vec<u8> {
    format!("{{\"aud\":\"{GRANT_AUDIENCE}\",\"exp\":{exp_unix_seconds},\"jti\":\"{jti_hex}\"}}")
        .into_bytes()
}

/// Assemble the `Authorization` header value from an already-signed
/// grant. The 64-byte signature is Ed25519 over
/// [`grant_signing_bytes`]`(payload)`.
pub fn grant_authorization_header(payload_json: &[u8], signature: &[u8; 64]) -> String {
    format!(
        "OSL-Link-Grant {}.{}",
        URL_SAFE_NO_PAD.encode(payload_json),
        URL_SAFE_NO_PAD.encode(signature)
    )
}

/// Fresh 16-byte `jti`, lowercase hex.
pub fn new_grant_jti() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    to_hex(&bytes)
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_copy() -> Vec<&'static str> {
        vec![
            SENDER_WARNING,
            SENDER_WARNING_MARKDOWN,
            VIEWER_PRE_REVEAL_NOTICE,
            ENFORCEABLE_CLAIM,
        ]
    }

    #[test]
    fn text_round_trips_through_seal_and_open() {
        let payload = LinkPayload::Text("hello, one-time world".to_string());
        let link = seal(&payload).expect("seal");
        assert_eq!(open(&link.key, &link.sealed_body).unwrap(), payload);
    }

    #[test]
    fn image_round_trips_and_preserves_mime() {
        let payload = LinkPayload::Image {
            mime: "image/png".to_string(),
            bytes: vec![0x89, 0x50, 0x4e, 0x47, 1, 2, 3],
        };
        let link = seal(&payload).expect("seal");
        assert_eq!(open(&link.key, &link.sealed_body).unwrap(), payload);
    }

    #[test]
    fn sealed_body_starts_with_the_nonce_and_hides_the_plaintext() {
        let text = "sentinel-plaintext-value";
        let link = seal(&LinkPayload::Text(text.to_string())).expect("seal");
        assert!(link.sealed_body.len() > NONCE_SIZE + TAG_SIZE);
        let haystack = link.sealed_body.clone();
        assert!(
            haystack.windows(text.len()).all(|w| w != text.as_bytes()),
            "plaintext must not survive in the sealed body"
        );
        // The frame magic must not be visible either.
        assert!(haystack.windows(5).all(|w| w != FRAME_MAGIC));
    }

    #[test]
    fn a_different_key_cannot_open_the_body() {
        let link = seal(&LinkPayload::Text("x".into())).expect("seal");
        let wrong = Key::from_bytes([7u8; KEY_SIZE]);
        assert_eq!(open(&wrong, &link.sealed_body), Err(LinkError::AeadFailure));
    }

    #[test]
    fn ciphertext_from_another_aad_does_not_open_in_this_lane() {
        // Bind check: the AAD is what stops a blob from another OSL lane
        // being replayed into the view-once landing page.
        let key = Key::from_bytes([9u8; KEY_SIZE]);
        let frame = encode_frame(&LinkPayload::Text("y".into())).unwrap();
        let (nonce, ct) = aes_gcm::seal(&key, b"some-other-lane", &frame).unwrap();
        let mut body = nonce.as_bytes().to_vec();
        body.extend_from_slice(&ct);
        assert_eq!(open(&key, &body), Err(LinkError::AeadFailure));
    }

    #[test]
    fn every_seal_uses_a_fresh_key_and_nonce() {
        let a = seal(&LinkPayload::Text("same".into())).unwrap();
        let b = seal(&LinkPayload::Text("same".into())).unwrap();
        assert_ne!(a.key.as_bytes(), b.key.as_bytes());
        assert_ne!(a.sealed_body[..NONCE_SIZE], b.sealed_body[..NONCE_SIZE]);
        assert_ne!(a.sealed_body, b.sealed_body);
    }

    #[test]
    fn fetch_and_manage_capabilities_are_distinct() {
        let link = seal(&LinkPayload::Text("z".into())).unwrap();
        assert_ne!(link.fetch_token.as_bytes(), link.manage_token.as_bytes());
        assert_eq!(link.fetch_token.to_hex().len(), 32);
        assert_eq!(link.manage_token.to_hex().len(), 32);
    }

    #[test]
    fn oversized_payload_is_refused_before_sealing() {
        let payload = LinkPayload::Image {
            mime: "image/png".into(),
            bytes: vec![0u8; MAX_SEALED_BYTES],
        };
        assert!(matches!(seal(&payload), Err(LinkError::TooLarge { .. })));
    }

    #[test]
    fn truncated_or_corrupt_frames_are_rejected() {
        let frame = encode_frame(&LinkPayload::Text("abc".into())).unwrap();
        assert!(decode_frame(&frame[..frame.len() - 1]).is_err());
        let mut bad_magic = frame.clone();
        bad_magic[0] = b'X';
        assert_eq!(decode_frame(&bad_magic), Err(LinkError::BadFrame));
        let mut bad_kind = frame.clone();
        bad_kind[5] = 9;
        assert_eq!(decode_frame(&bad_kind), Err(LinkError::BadFrame));
    }

    #[test]
    fn malformed_mime_is_refused() {
        assert_eq!(
            encode_frame(&LinkPayload::Image {
                mime: String::new(),
                bytes: vec![1]
            }),
            Err(LinkError::BadMime)
        );
        assert_eq!(
            encode_frame(&LinkPayload::Image {
                mime: "image/\u{0000}png".into(),
                bytes: vec![1]
            }),
            Err(LinkError::BadMime)
        );
    }

    #[test]
    fn the_key_and_token_live_only_after_the_fragment_marker() {
        let link = seal(&LinkPayload::Text("q".into())).unwrap();
        let id = "0123456789abcdef0123456789abcdef";
        let url = link_url("https://example.com", id, &link.key, &link.fetch_token).unwrap();
        let (before_hash, after_hash) = url.split_once('#').expect("url must carry a fragment");
        assert!(before_hash.ends_with(id));
        assert!(!before_hash.contains("k="));
        assert!(!before_hash.contains("t="));
        assert!(
            !before_hash.contains('?'),
            "no query string may carry a secret"
        );
        assert!(after_hash.contains(&link.fetch_token.to_hex()));
        assert!(after_hash.contains(&URL_SAFE_NO_PAD.encode(link.key.as_bytes())));
        // The manage capability is sender-only and must never be in the URL.
        assert!(!url.contains(&link.manage_token.to_hex()));
    }

    #[test]
    fn link_url_refuses_a_non_https_or_pathful_host() {
        let link = seal(&LinkPayload::Text("q".into())).unwrap();
        let id = "0123456789abcdef0123456789abcdef";
        for host in ["http://example.com", "example.com", "https://example.com/x"] {
            assert_eq!(
                link_url(host, id, &link.key, &link.fetch_token),
                Err(LinkError::BadHost)
            );
        }
        assert_eq!(
            link_url(
                "https://example.com",
                "nothex",
                &link.key,
                &link.fetch_token
            ),
            Err(LinkError::BadHost)
        );
    }

    #[test]
    fn discord_text_wraps_the_url_in_angle_brackets() {
        // Documented suppression for Discord's embed generation.
        let text = discord_link_text("https://example.com/v/aa#k=b&t=c");
        assert!(text.starts_with('<') && text.ends_with('>'));
        assert_eq!(text, "<https://example.com/v/aa#k=b&t=c>");
    }

    #[test]
    fn sender_warning_is_the_specified_wording() {
        assert_eq!(SENDER_WARNING_MARKDOWN, "**One-time link.** The link stops working after it's opened once, or after 1 hour \u{2014} whichever comes first. Anyone who has the link can open it, so treat it like a key. Whoever opens it can screenshot, record or photograph what they see; OSL can't prevent that in a web browser. **This link shows that you used a privacy tool.** For full protection, send to someone using OSL.");
        assert_eq!(SENDER_WARNING, SENDER_WARNING_MARKDOWN.replace("**", ""));
        assert_eq!(
            VIEWER_PRE_REVEAL_NOTICE,
            "This opens once. When you close it, the link stops working. Your screen isn't protected."
        );
    }

    #[test]
    fn no_copy_in_this_lane_contains_a_banned_claim() {
        for copy in all_copy() {
            let lowered = copy.to_lowercase();
            for banned in BANNED_CLAIMS {
                assert!(
                    !lowered.contains(banned),
                    "banned claim {banned:?} found in copy"
                );
            }
        }
    }

    #[test]
    fn sender_status_never_says_read_seen_or_viewed() {
        let labels = [
            LinkStatus::Created.sender_label(0),
            LinkStatus::Retrieved {
                at_unix_seconds: 1_753_500_000,
            }
            .sender_label(0),
            LinkStatus::ExpiredWithoutRetrieval.sender_label(0),
        ];
        for label in &labels {
            let lowered = label.to_lowercase();
            for banned in BANNED_STATUS_WORDS {
                assert!(
                    !lowered
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|w| w == *banned),
                    "sender label {label:?} must not use the word {banned:?}"
                );
            }
        }
        assert_eq!(labels[0], "Link created");
        assert_eq!(labels[2], "Expired without being retrieved");
    }

    #[test]
    fn retrieved_label_is_hh_mm_and_localises() {
        // 1970-01-01T01:02:03Z
        let at = 3_723;
        assert_eq!(
            LinkStatus::Retrieved {
                at_unix_seconds: at
            }
            .sender_label(0),
            "Retrieved at 01:02"
        );
        assert_eq!(
            LinkStatus::Retrieved {
                at_unix_seconds: at
            }
            .sender_label(3600),
            "Retrieved at 02:02"
        );
        // Wrapping backwards across midnight must not panic or go negative.
        assert_eq!(
            LinkStatus::Retrieved {
                at_unix_seconds: at
            }
            .sender_label(-2 * 3600),
            "Retrieved at 23:02"
        );
    }

    #[test]
    fn grant_payload_and_signing_bytes_are_canonical() {
        let payload = grant_payload_json(1_753_500_600, "0123456789abcdef0123456789abcdef");
        assert_eq!(
            core::str::from_utf8(&payload).unwrap(),
            "{\"aud\":\"osl-link-create\",\"exp\":1753500600,\"jti\":\"0123456789abcdef0123456789abcdef\"}"
        );
        let signing = grant_signing_bytes(&payload);
        assert!(signing.starts_with(GRANT_DOMAIN.as_bytes()));
        assert_eq!(signing[GRANT_DOMAIN.len()], 0x00);
        assert_eq!(&signing[GRANT_DOMAIN.len() + 1..], &payload[..]);
    }

    #[test]
    fn grant_header_matches_the_scheme_the_worker_parses() {
        let payload = grant_payload_json(1, &new_grant_jti());
        let header = grant_authorization_header(&payload, &[0u8; 64]);
        assert!(header.starts_with("OSL-Link-Grant "));
        let value = header.trim_start_matches("OSL-Link-Grant ");
        let parts: Vec<&str> = value.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(URL_SAFE_NO_PAD.decode(parts[0]).unwrap(), payload);
        assert_eq!(URL_SAFE_NO_PAD.decode(parts[1]).unwrap().len(), 64);
    }

    #[test]
    fn a_grant_jti_is_fresh_each_time() {
        let a = new_grant_jti();
        let b = new_grant_jti();
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn capability_debug_never_prints_the_bearer_value() {
        let cap = Capability::from_bytes([0xab; TOKEN_BYTES]);
        assert_eq!(format!("{cap:?}"), "Capability(redacted)");
        assert!(!format!("{cap:?}").contains("abab"));
    }

    #[test]
    fn new_link_debug_never_prints_key_or_capabilities() {
        let link = seal(&LinkPayload::Text("secret".into())).unwrap();
        let rendered = format!("{link:?}");
        assert!(!rendered.contains(&link.fetch_token.to_hex()));
        assert!(!rendered.contains(&link.manage_token.to_hex()));
        assert!(!rendered.contains(&URL_SAFE_NO_PAD.encode(link.key.as_bytes())));
    }

    #[test]
    fn lifetime_constants_match_the_enforceable_claim() {
        assert_eq!(LINK_TTL_SECONDS, 3600);
        assert_eq!(RESERVATION_SECONDS, 60);
        assert_eq!(
            ENFORCEABLE_CLAIM,
            "The link dies after one view, or 60 seconds, whichever is first."
        );
        assert!(SENDER_WARNING.contains("after 1 hour"));
    }
}
