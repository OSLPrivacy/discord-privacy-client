//! Real GIF messages: provider search, direct-file pick, and playback.
//!
//! ## What this replaces
//!
//! Before TASK 6832 a GIF was an affordance and nothing else. `gif` sat in
//! [`crate::attachment_formats::CANDIDATE_EXTENSIONS`] and in
//! `ipc::attachment_wire`'s MIME table, so every surface *looked* like it could
//! carry one, and every surface then refused: the picker filter dropped `gif`
//! because the receive side's only image viewer is the Windows WIC single-frame
//! decoder, which rejects an animated GIF outright. A user could see the format
//! named and never send one. That is a synthetic affordance, and this module
//! exists so there is no longer one.
//!
//! ## What a GIF message actually is here
//!
//! A GIF message is an ordinary OSL attachment. It is not a link, not a
//! provider embed, and not a card that renders by asking a remote server for
//! pixels at display time. The bytes are fetched **once**, on the sender's
//! device, through the client privacy proxy; the trackers are cut out of them;
//! and what is encrypted, uploaded and delivered is the stripped GIF, under the
//! same per-attachment channel key
//! (`broker::begin_osl_chat_attachment` → `peer_attachment_io::encrypt_file`)
//! that every other OSL attachment uses. The recipient decrypts bytes, not a
//! URL, so nothing on the display path can phone home.
//!
//! ## The privacy proxy
//!
//! Neither the provider search nor the media download is allowed to leave this
//! device as a direct request. Both go out as a [`ProxyRequest`], and this
//! module — not the caller — decides every header on it:
//! [`outbound_headers`] emits a fixed, minimal set, and
//! [`reject_identifying_headers`] refuses the request outright if any header in
//! [`FORBIDDEN_OUTBOUND_HEADERS`] is present. A `Cookie`, a `Referer`, an
//! `Authorization`, a distinguishing `User-Agent` or an `X-Forwarded-For` is a
//! bug, not a configuration choice.
//!
//! The only caller-controlled value that reaches the provider is the search
//! text, which is what a search *is*. No recipient id, no enclave id, no
//! conversation id, no message body and no filename is ever placed on an
//! outbound request; the provider cannot learn who the GIF is for, because the
//! send has not happened yet and nothing in the request names it.
//!
//! ## Tracker stripping
//!
//! Two layers, because a GIF carries trackers in two places:
//!
//! * **URL** — [`sanitize_media_url`] pins the origin to the configured media
//!   origin, drops the entire query string and fragment (a media URL needs
//!   neither; `utm_*`, `client_key`, `session_id`, `gclid` and friends all live
//!   there), refuses userinfo and traversal, and requires a `.gif` path.
//! * **Bytes** — [`strip_remote_trackers`] walks the GIF block stream and
//!   rebuilds it keeping only the header, the logical screen descriptor, the
//!   global colour table, the NETSCAPE2.0 loop extension, graphic control
//!   extensions, image descriptors and their LZW data, and the trailer. Comment
//!   extensions, plain-text extensions, every other application extension
//!   (XMP packets carrying tracking URLs, ICC profiles, provider beacons) and
//!   anything appended after the trailer are dropped.
//!
//! ## Playback authorisation
//!
//! [`authorize_gif_playback`] is the gate the display surface must pass before
//! any decrypted GIF byte becomes playable. It re-checks the four facts the
//! broker already established — the viewer is the notice's recipient, the
//! sender is the bound peer, the scope matches and is approved, and decrypted
//! display is on for that scope — and refuses with a static string otherwise.
//! An unauthorised viewer gets no frames, no dimensions and no bytes.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use zeroize::Zeroizing;

/// The one MIME type a GIF message may carry. Matches
/// `ipc::attachment_wire::mime_for_filename("x.gif")`.
pub const GIF_MIME: &str = "image/gif";

/// Upper bound on a GIF OSL will carry. Far below the attachment ceiling on
/// purpose: a GIF is a reaction, and an unbounded one is a memory and bandwidth
/// hazard on both devices.
pub const MAX_GIF_BYTES: u64 = 8 * 1024 * 1024;

/// Most frames a GIF message may contain.
pub const MAX_GIF_FRAMES: usize = 4096;

/// Largest logical screen either dimension may declare.
pub const MAX_GIF_DIMENSION: u16 = 4096;

/// Longest search text that may leave the device.
pub const MAX_GIF_QUERY_CHARS: usize = 64;

/// Most results one search may return.
pub const MAX_GIF_RESULTS: usize = 24;

/// Longest provider-supplied description OSL will keep.
pub const MAX_GIF_DESCRIPTION_CHARS: usize = 120;

/// Headers that must never appear on a request leaving the privacy proxy.
///
/// Every one of these is a way for a provider to correlate one search with the
/// next, or with a browsing session elsewhere on the device.
pub const FORBIDDEN_OUTBOUND_HEADERS: &[&str] = &[
    "cookie",
    "set-cookie",
    "referer",
    "referrer",
    "authorization",
    "proxy-authorization",
    "user-agent",
    "accept-language",
    "dnt",
    "sec-ch-ua",
    "sec-ch-ua-platform",
    "x-forwarded-for",
    "x-real-ip",
    "x-client-ip",
    "x-request-id",
    "x-osl-recipient",
    "x-device-id",
    "etag",
    "if-none-match",
    "if-modified-since",
];

/// Refusal shown when a GIF message cannot be played, whoever asked.
pub const GIF_PLAYBACK_REFUSAL: &str =
    "OSL will not play this GIF: it was not sent to this device on this conversation.";

// ---------------------------------------------------------------------------
// Refusals and failures. A refusal is about the content; a failure is about the
// network. They are separate types because they read differently to the user
// and only one of them is worth retrying.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GifRefusal {
    NotAGif,
    Truncated,
    TooLarge,
    Empty,
    NoFrames,
    TooManyFrames,
    DimensionsOutOfRange,
    UnknownBlock,
    TrailingBytes,
    UrlNotProviderOrigin,
    UrlHasUserinfo,
    UrlNotAGifPath,
    UrlTraversal,
    QueryEmpty,
    QueryTooLong,
    QueryHasControlCharacters,
    IdentifyingHeader,
    ProviderAnsweredNonsense,
    LocalFileUnreadable,
    LocalFileNotAGifName,
}

impl GifRefusal {
    /// User-facing text. Names what OSL refused and why, and never contains a
    /// URL, a filename, a recipient or any provider-supplied string.
    pub fn message(self) -> &'static str {
        match self {
            GifRefusal::NotAGif => "That file is not a GIF, so OSL did not send it.",
            GifRefusal::Truncated => "That GIF is incomplete, so OSL did not send it.",
            GifRefusal::TooLarge => "That GIF is larger than 8 MB, so OSL did not send it.",
            GifRefusal::Empty => "That GIF is empty, so OSL did not send it.",
            GifRefusal::NoFrames => "That GIF has no pictures in it, so OSL did not send it.",
            GifRefusal::TooManyFrames => {
                "That GIF has too many frames to play safely, so OSL did not send it."
            }
            GifRefusal::DimensionsOutOfRange => {
                "That GIF is larger than 4096 pixels on a side, so OSL did not send it."
            }
            GifRefusal::UnknownBlock => {
                "OSL could not read that GIF from end to end, so it did not send it."
            }
            GifRefusal::TrailingBytes => {
                "That GIF has extra data hidden after the picture, so OSL did not send it."
            }
            GifRefusal::UrlNotProviderOrigin
            | GifRefusal::UrlHasUserinfo
            | GifRefusal::UrlNotAGifPath
            | GifRefusal::UrlTraversal => {
                "That GIF result points somewhere OSL will not fetch from, so OSL skipped it."
            }
            GifRefusal::QueryEmpty => "Type something to search for a GIF.",
            GifRefusal::QueryTooLong => "That GIF search is too long.",
            GifRefusal::QueryHasControlCharacters => "That GIF search cannot be sent as written.",
            GifRefusal::IdentifyingHeader => {
                "OSL blocked a GIF request that would have identified this device."
            }
            GifRefusal::ProviderAnsweredNonsense => {
                "The GIF provider sent an answer OSL could not read."
            }
            GifRefusal::LocalFileUnreadable => "OSL could not read that GIF file.",
            GifRefusal::LocalFileNotAGifName => "Pick a file whose name ends in .gif.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GifTransportFailure {
    /// No network at all.
    Offline,
    /// The privacy proxy itself could not be reached or refused to forward.
    ProxyUnavailable,
    /// The proxy reached the provider and the provider answered with a status
    /// that is not success.
    ProviderStatus(u16),
    /// The provider answered success with a body OSL could not use.
    ProviderUnusable,
}

impl GifTransportFailure {
    /// Whether trying the identical action again could succeed without the user
    /// changing anything. A 4xx that is not 408/429 is the provider saying no,
    /// and repeating it is not honest retrying.
    pub fn retryable(&self) -> bool {
        match self {
            GifTransportFailure::Offline | GifTransportFailure::ProxyUnavailable => true,
            GifTransportFailure::ProviderStatus(status) => {
                *status == 408 || *status == 429 || (500..600).contains(status)
            }
            GifTransportFailure::ProviderUnusable => false,
        }
    }

    /// User-facing text. Says what happened, says nothing was sent, and only
    /// offers a retry when [`Self::retryable`] is true.
    pub fn message(&self) -> String {
        let cause = match self {
            GifTransportFailure::Offline => {
                "OSL is offline, so it could not reach the GIF provider.".to_owned()
            }
            GifTransportFailure::ProxyUnavailable => {
                "OSL could not reach the GIF provider through its privacy proxy.".to_owned()
            }
            GifTransportFailure::ProviderStatus(status) => {
                format!("The GIF provider answered {status}.")
            }
            GifTransportFailure::ProviderUnusable => {
                "The GIF provider sent something OSL could not use.".to_owned()
            }
        };
        let tail = if self.retryable() {
            " Nothing was sent. Try again."
        } else {
            " Nothing was sent."
        };
        format!("{cause}{tail}")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GifIntakeError {
    Refused(GifRefusal),
    Transport(GifTransportFailure),
    /// The user cancelled. Distinct from every failure: there is nothing to
    /// report, nothing to retry and nothing was sent.
    Cancelled,
}

impl GifIntakeError {
    pub fn message(&self) -> String {
        match self {
            GifIntakeError::Refused(refusal) => refusal.message().to_owned(),
            GifIntakeError::Transport(failure) => failure.message(),
            GifIntakeError::Cancelled => "GIF cancelled. Nothing was sent.".to_owned(),
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, GifIntakeError::Transport(failure) if failure.retryable())
    }
}

impl From<GifRefusal> for GifIntakeError {
    fn from(value: GifRefusal) -> Self {
        GifIntakeError::Refused(value)
    }
}

impl From<GifTransportFailure> for GifIntakeError {
    fn from(value: GifTransportFailure) -> Self {
        GifIntakeError::Transport(value)
    }
}

// ---------------------------------------------------------------------------
// Cancellation.
// ---------------------------------------------------------------------------

/// A cancel flag shared with the surface that drew the picker.
///
/// Checked before the search leaves, before the media request leaves, and
/// between response chunks, so a cancel at any point costs zero uploaded bytes.
#[derive(Clone, Debug, Default)]
pub struct GifCancel(Arc<AtomicBool>);

impl GifCancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn guard(&self) -> Result<(), GifIntakeError> {
        if self.is_cancelled() {
            return Err(GifIntakeError::Cancelled);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// GIF block walk. One walker serves both the filmstrip read and the tracker
// strip, so a block the strip drops can never be a block the player counted.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GifFrame {
    /// Hundredths of a second the frame is held, as written in the GIF.
    pub delay_centiseconds: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifFilmstrip {
    pub width: u16,
    pub height: u16,
    pub frames: Vec<GifFrame>,
    /// A NETSCAPE2.0 loop count of zero means "forever".
    pub loop_forever: bool,
    pub loop_count: u16,
    /// Sum of the declared frame delays, in milliseconds.
    pub total_duration_ms: u64,
}

impl GifFilmstrip {
    /// A GIF with more than one frame is one the single-frame WIC viewer would
    /// have refused. That refusal is the whole reason this module exists, so
    /// the fact is named rather than inferred at call sites.
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
}

/// What [`strip_remote_trackers`] cut out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GifTrackerStrip {
    pub removed_comment_extensions: usize,
    /// Application extensions other than NETSCAPE2.0: XMP packets, ICC
    /// profiles, provider beacons.
    pub removed_application_extensions: usize,
    pub removed_plain_text_extensions: usize,
    pub removed_trailing_bytes: usize,
    pub bytes_before: usize,
    pub bytes_after: usize,
}

impl GifTrackerStrip {
    pub fn bytes_removed(&self) -> usize {
        self.bytes_before.saturating_sub(self.bytes_after)
    }

    pub fn blocks_removed(&self) -> usize {
        self.removed_comment_extensions
            + self.removed_application_extensions
            + self.removed_plain_text_extensions
    }
}

const BLOCK_EXTENSION: u8 = 0x21;
const BLOCK_IMAGE: u8 = 0x2c;
const BLOCK_TRAILER: u8 = 0x3b;
const EXTENSION_PLAIN_TEXT: u8 = 0x01;
const EXTENSION_GRAPHIC_CONTROL: u8 = 0xf9;
const EXTENSION_COMMENT: u8 = 0xfe;
const EXTENSION_APPLICATION: u8 = 0xff;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], GifRefusal> {
        let end = self
            .at
            .checked_add(count)
            .ok_or(GifRefusal::Truncated)?;
        if end > self.bytes.len() {
            return Err(GifRefusal::Truncated);
        }
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, GifRefusal> {
        Ok(self.take(1)?[0])
    }

    fn u16_le(&mut self) -> Result<u16, GifRefusal> {
        let raw = self.take(2)?;
        Ok(u16::from(raw[0]) | (u16::from(raw[1]) << 8))
    }

    /// Read one GIF sub-block chain and return the whole chain including its
    /// terminating zero, so a keeper can copy it verbatim.
    fn sub_blocks(&mut self) -> Result<&'a [u8], GifRefusal> {
        let start = self.at;
        loop {
            let len = self.byte()? as usize;
            if len == 0 {
                break;
            }
            self.take(len)?;
        }
        Ok(&self.bytes[start..self.at])
    }
}

fn colour_table_bytes(packed: u8) -> usize {
    if packed & 0b1000_0000 == 0 {
        0
    } else {
        3 * (1usize << ((packed & 0b0000_0111) + 1))
    }
}

/// One pass over the GIF stream, producing the filmstrip and the stripped copy
/// at the same time.
fn walk(bytes: &[u8]) -> Result<(GifFilmstrip, Vec<u8>, GifTrackerStrip), GifRefusal> {
    if bytes.is_empty() {
        return Err(GifRefusal::Empty);
    }
    if bytes.len() as u64 > MAX_GIF_BYTES {
        return Err(GifRefusal::TooLarge);
    }
    let mut reader = Reader::new(bytes);
    let header = reader.take(6)?;
    if header != b"GIF87a" && header != b"GIF89a" {
        return Err(GifRefusal::NotAGif);
    }
    let mut kept: Vec<u8> = Vec::with_capacity(bytes.len());
    kept.extend_from_slice(header);

    let descriptor_at = reader.at;
    let width = reader.u16_le()?;
    let height = reader.u16_le()?;
    let packed = reader.byte()?;
    let _background = reader.byte()?;
    let _aspect = reader.byte()?;
    if width == 0 || height == 0 || width > MAX_GIF_DIMENSION || height > MAX_GIF_DIMENSION {
        return Err(GifRefusal::DimensionsOutOfRange);
    }
    let global_table = reader.take(colour_table_bytes(packed))?;
    kept.extend_from_slice(&bytes[descriptor_at..descriptor_at + 7]);
    kept.extend_from_slice(global_table);

    let mut strip = GifTrackerStrip {
        bytes_before: bytes.len(),
        ..GifTrackerStrip::default()
    };
    let mut frames: Vec<GifFrame> = Vec::new();
    let mut pending_delay: Option<u16> = None;
    let mut loop_forever = false;
    let mut loop_count = 0u16;
    let mut saw_trailer = false;

    while !saw_trailer {
        let block_at = reader.at;
        let introducer = reader.byte()?;
        match introducer {
            BLOCK_TRAILER => {
                kept.push(BLOCK_TRAILER);
                saw_trailer = true;
            }
            BLOCK_EXTENSION => {
                let label = reader.byte()?;
                match label {
                    EXTENSION_GRAPHIC_CONTROL => {
                        let size = reader.byte()?;
                        if size != 4 {
                            return Err(GifRefusal::UnknownBlock);
                        }
                        let _flags = reader.byte()?;
                        let delay = reader.u16_le()?;
                        let _transparent = reader.byte()?;
                        if reader.byte()? != 0 {
                            return Err(GifRefusal::UnknownBlock);
                        }
                        pending_delay = Some(delay);
                        kept.extend_from_slice(&bytes[block_at..reader.at]);
                    }
                    EXTENSION_APPLICATION => {
                        let size = reader.byte()?;
                        if size != 11 {
                            return Err(GifRefusal::UnknownBlock);
                        }
                        let identifier = reader.take(11)?.to_vec();
                        let chain_at = reader.at;
                        let chain = reader.sub_blocks()?;
                        if identifier == b"NETSCAPE2.0" {
                            // The loop extension is playback, not tracking.
                            if chain.len() >= 5 && chain[0] == 3 && chain[1] == 1 {
                                loop_count = u16::from(chain[2]) | (u16::from(chain[3]) << 8);
                                loop_forever = loop_count == 0;
                            }
                            kept.extend_from_slice(&bytes[block_at..chain_at]);
                            kept.extend_from_slice(chain);
                        } else {
                            // XMP packets, ICC profiles, provider beacons.
                            strip.removed_application_extensions += 1;
                        }
                    }
                    EXTENSION_COMMENT => {
                        reader.sub_blocks()?;
                        strip.removed_comment_extensions += 1;
                    }
                    EXTENSION_PLAIN_TEXT => {
                        let size = reader.byte()?;
                        if size != 12 {
                            return Err(GifRefusal::UnknownBlock);
                        }
                        reader.take(12)?;
                        reader.sub_blocks()?;
                        strip.removed_plain_text_extensions += 1;
                    }
                    _ => return Err(GifRefusal::UnknownBlock),
                }
            }
            BLOCK_IMAGE => {
                let _left = reader.u16_le()?;
                let _top = reader.u16_le()?;
                let frame_width = reader.u16_le()?;
                let frame_height = reader.u16_le()?;
                let image_packed = reader.byte()?;
                reader.take(colour_table_bytes(image_packed))?;
                let _min_code_size = reader.byte()?;
                reader.sub_blocks()?;
                if frame_width == 0
                    || frame_height == 0
                    || frame_width > MAX_GIF_DIMENSION
                    || frame_height > MAX_GIF_DIMENSION
                {
                    return Err(GifRefusal::DimensionsOutOfRange);
                }
                if frames.len() == MAX_GIF_FRAMES {
                    return Err(GifRefusal::TooManyFrames);
                }
                frames.push(GifFrame {
                    delay_centiseconds: pending_delay.take().unwrap_or(0),
                    width: frame_width,
                    height: frame_height,
                });
                kept.extend_from_slice(&bytes[block_at..reader.at]);
            }
            _ => return Err(GifRefusal::UnknownBlock),
        }
    }

    // A GIF ends at its trailer. Anything after it is payload the decoder will
    // never show and a scanner will never look at, which is exactly where a
    // beacon or an exfiltration blob would be parked.
    strip.removed_trailing_bytes = bytes.len() - reader.at;
    if frames.is_empty() {
        return Err(GifRefusal::NoFrames);
    }
    strip.bytes_after = kept.len();

    let total_duration_ms = frames
        .iter()
        .map(|frame| u64::from(frame.delay_centiseconds) * 10)
        .sum();
    let filmstrip = GifFilmstrip {
        width,
        height,
        frames,
        loop_forever,
        loop_count,
        total_duration_ms,
    };
    Ok((filmstrip, kept, strip))
}

/// Read the frames, dimensions and loop behaviour of a GIF without decoding a
/// single pixel. Refuses anything that is not a complete, well-formed GIF.
pub fn read_filmstrip(bytes: &[u8]) -> Result<GifFilmstrip, GifRefusal> {
    walk(bytes).map(|(filmstrip, _, _)| filmstrip)
}

/// Rebuild a GIF from only the blocks a player needs, dropping every block a
/// tracker can hide in and everything appended after the trailer.
pub fn strip_remote_trackers(bytes: &[u8]) -> Result<(Vec<u8>, GifTrackerStrip), GifRefusal> {
    let (_, kept, strip) = walk(bytes)?;
    Ok((kept, strip))
}

// ---------------------------------------------------------------------------
// URL sanitising.
// ---------------------------------------------------------------------------

/// What [`sanitize_media_url`] cut out of a provider media URL.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UrlStrip {
    /// Names only. Values are provider tracking material and are not retained,
    /// not logged and not shown.
    pub stripped_parameters: Vec<String>,
    pub stripped_fragment: bool,
}

impl UrlStrip {
    pub fn stripped_anything(&self) -> bool {
        !self.stripped_parameters.is_empty() || self.stripped_fragment
    }
}

fn split_origin(raw: &str) -> Option<(&str, &str)> {
    for scheme in ["https://", "http://"] {
        if let Some(rest) = raw.strip_prefix(scheme) {
            let end = rest.find('/').unwrap_or(rest.len());
            return Some((&raw[..scheme.len() + end], &rest[end..]));
        }
    }
    None
}

/// Pin a provider media URL to the configured origin and strip its tracking
/// surface. Returns the fetchable URL and what was removed.
pub fn sanitize_media_url(raw: &str, expected_origin: &str) -> Result<(String, UrlStrip), GifRefusal> {
    let (origin, path_and_rest) = split_origin(raw).ok_or(GifRefusal::UrlNotProviderOrigin)?;
    if origin != expected_origin {
        return Err(GifRefusal::UrlNotProviderOrigin);
    }
    if origin
        .rsplit("//")
        .next()
        .is_some_and(|authority| authority.contains('@'))
    {
        return Err(GifRefusal::UrlHasUserinfo);
    }
    let mut strip = UrlStrip::default();
    let (without_fragment, fragment) = match path_and_rest.split_once('#') {
        Some((head, _)) => (head, true),
        None => (path_and_rest, false),
    };
    strip.stripped_fragment = fragment;
    let (path, query) = match without_fragment.split_once('?') {
        Some((head, tail)) => (head, Some(tail)),
        None => (without_fragment, None),
    };
    if let Some(query) = query {
        for pair in query.split('&').filter(|pair| !pair.is_empty()) {
            let name = pair.split('=').next().unwrap_or(pair);
            strip.stripped_parameters.push(name.to_owned());
        }
    }
    if path.is_empty() || !path.starts_with('/') {
        return Err(GifRefusal::UrlNotAGifPath);
    }
    if path.split('/').any(|segment| segment == ".." || segment == ".") {
        return Err(GifRefusal::UrlTraversal);
    }
    if !path.to_ascii_lowercase().ends_with(".gif") {
        return Err(GifRefusal::UrlNotAGifPath);
    }
    if path.chars().any(|character| {
        character.is_control() || character.is_whitespace() || character == '\\'
    }) {
        return Err(GifRefusal::UrlNotAGifPath);
    }
    Ok((format!("{origin}{path}"), strip))
}

/// Bound and clean the one caller-controlled value that leaves the device.
pub fn sanitize_query(raw: &str) -> Result<String, GifRefusal> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(GifRefusal::QueryEmpty);
    }
    if trimmed.chars().count() > MAX_GIF_QUERY_CHARS {
        return Err(GifRefusal::QueryTooLong);
    }
    if trimmed.chars().any(char::is_control) {
        return Err(GifRefusal::QueryHasControlCharacters);
    }
    Ok(trimmed.to_owned())
}

fn percent_encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        let character = *byte as char;
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~') {
            out.push(character);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Strip provider-supplied display text down to something safe to render.
pub fn sanitize_description(raw: &str) -> String {
    raw.chars()
        .filter(|character| !character.is_control())
        .take(MAX_GIF_DESCRIPTION_CHARS)
        .collect::<String>()
        .trim()
        .to_owned()
}

// ---------------------------------------------------------------------------
// The privacy proxy contract.
// ---------------------------------------------------------------------------

/// Where the GIF provider lives, as seen through the proxy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifProviderConfig {
    /// Origin the proxy forwards search requests to, e.g. `https://gifs.example`.
    pub search_origin: String,
    /// Path on that origin, e.g. `/v1/search`.
    pub search_path: String,
    /// Origin media URLs must be pinned to.
    pub media_origin: String,
}

impl GifProviderConfig {
    /// A configuration that would send OSL traffic in the clear is a bug. The
    /// only exception is a loopback origin, which never leaves the machine and
    /// is how the headless proof drives a real proxy.
    pub fn transport_is_private(&self) -> bool {
        [self.search_origin.as_str(), self.media_origin.as_str()]
            .iter()
            .all(|origin| {
                origin.starts_with("https://")
                    || origin.starts_with("http://127.0.0.1:")
                    || origin.starts_with("http://[::1]:")
                    || origin.starts_with("http://localhost:")
            })
    }
}

/// One request handed to the privacy proxy. Callers never build the headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyRequest {
    pub method: &'static str,
    pub origin: String,
    pub path_and_query: String,
    pub headers: Vec<(String, String)>,
}

impl ProxyRequest {
    pub fn url(&self) -> String {
        format!("{}{}", self.origin, self.path_and_query)
    }

    /// Every byte this request would put on the wire, for a leak scan.
    pub fn observable(&self) -> String {
        let mut out = format!("{} {}\n", self.method, self.url());
        for (name, value) in &self.headers {
            out.push_str(name);
            out.push_str(": ");
            out.push_str(value);
            out.push('\n');
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyResponse {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// The seam the real egress and the headless proof both implement.
pub trait GifProxyTransport {
    fn send(&self, request: &ProxyRequest) -> Result<ProxyResponse, GifTransportFailure>;
}

/// The fixed header set OSL puts on a proxied GIF request. Nothing here varies
/// with the device, the account, the conversation or the time.
pub fn outbound_headers(accept: &str) -> Vec<(String, String)> {
    vec![
        ("accept".to_owned(), accept.to_owned()),
        ("accept-encoding".to_owned(), "identity".to_owned()),
        ("connection".to_owned(), "close".to_owned()),
    ]
}

/// Refuse a request carrying anything a provider could correlate on.
pub fn reject_identifying_headers(request: &ProxyRequest) -> Result<(), GifRefusal> {
    for (name, _) in &request.headers {
        if FORBIDDEN_OUTBOUND_HEADERS.contains(&name.to_ascii_lowercase().as_str()) {
            return Err(GifRefusal::IdentifyingHeader);
        }
    }
    Ok(())
}

/// Build the proxied search request for `query`.
pub fn search_request(
    config: &GifProviderConfig,
    query: &str,
    limit: usize,
) -> Result<ProxyRequest, GifRefusal> {
    let query = sanitize_query(query)?;
    let limit = limit.clamp(1, MAX_GIF_RESULTS);
    let request = ProxyRequest {
        method: "GET",
        origin: config.search_origin.clone(),
        path_and_query: format!(
            "{}?q={}&limit={limit}",
            config.search_path,
            percent_encode_query(&query)
        ),
        headers: outbound_headers("application/json"),
    };
    reject_identifying_headers(&request)?;
    Ok(request)
}

/// Build the proxied media request for an already-sanitised media URL.
pub fn media_request(
    config: &GifProviderConfig,
    sanitized_url: &str,
) -> Result<ProxyRequest, GifRefusal> {
    let (origin, path) =
        split_origin(sanitized_url).ok_or(GifRefusal::UrlNotProviderOrigin)?;
    if origin != config.media_origin {
        return Err(GifRefusal::UrlNotProviderOrigin);
    }
    let request = ProxyRequest {
        method: "GET",
        origin: origin.to_owned(),
        path_and_query: path.to_owned(),
        headers: outbound_headers("image/gif"),
    };
    reject_identifying_headers(&request)?;
    Ok(request)
}

// ---------------------------------------------------------------------------
// Search and intake.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifSearchResult {
    pub id: String,
    pub description: String,
    /// Already pinned and stripped by [`sanitize_media_url`].
    pub media_url: String,
    pub url_strip: UrlStrip,
    pub width: u16,
    pub height: u16,
    pub byte_size: u64,
}

/// A search that ran, with what it had to drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifSearchOutcome {
    pub results: Vec<GifSearchResult>,
    /// Results whose media URL failed [`sanitize_media_url`] outright.
    pub rejected_results: usize,
    /// Tracking parameters removed across every accepted result.
    pub stripped_parameters: usize,
}

fn parse_search_body(body: &[u8], media_origin: &str) -> Result<GifSearchOutcome, GifRefusal> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| GifRefusal::ProviderAnsweredNonsense)?;
    let rows = value
        .get("results")
        .and_then(serde_json::Value::as_array)
        .ok_or(GifRefusal::ProviderAnsweredNonsense)?;
    let mut results = Vec::new();
    let mut rejected_results = 0usize;
    let mut stripped_parameters = 0usize;
    for row in rows.iter().take(MAX_GIF_RESULTS) {
        let id = row.get("id").and_then(serde_json::Value::as_str);
        let media = row.get("media_url").and_then(serde_json::Value::as_str);
        let (Some(id), Some(media)) = (id, media) else {
            rejected_results += 1;
            continue;
        };
        if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            rejected_results += 1;
            continue;
        }
        let Ok((media_url, url_strip)) = sanitize_media_url(media, media_origin) else {
            rejected_results += 1;
            continue;
        };
        stripped_parameters += url_strip.stripped_parameters.len();
        let byte_size = row
            .get("byte_size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if byte_size > MAX_GIF_BYTES {
            rejected_results += 1;
            continue;
        }
        results.push(GifSearchResult {
            id: id.to_owned(),
            description: sanitize_description(
                row.get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            ),
            media_url,
            url_strip,
            width: row
                .get("width")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .min(u64::from(u16::MAX)) as u16,
            height: row
                .get("height")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .min(u64::from(u16::MAX)) as u16,
            byte_size,
        });
    }
    Ok(GifSearchOutcome {
        results,
        rejected_results,
        stripped_parameters,
    })
}

/// Search the provider through the privacy proxy.
pub fn search_gifs<P: GifProxyTransport>(
    proxy: &P,
    config: &GifProviderConfig,
    query: &str,
    limit: usize,
    cancel: &GifCancel,
) -> Result<GifSearchOutcome, GifIntakeError> {
    cancel.guard()?;
    let request = search_request(config, query, limit)?;
    let response = proxy.send(&request)?;
    cancel.guard()?;
    if response.status != 200 {
        return Err(GifTransportFailure::ProviderStatus(response.status).into());
    }
    if !response.content_type.starts_with("application/json") {
        return Err(GifTransportFailure::ProviderUnusable.into());
    }
    Ok(parse_search_body(&response.body, &config.media_origin)?)
}

/// Where a GIF message's bytes came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GifSource {
    /// A result the user picked out of the provider search.
    Provider { result_id: String },
    /// A `.gif` the user picked off their own disk. Never touches the proxy.
    LocalFile,
}

/// A GIF that is ready to be encrypted and sent.
pub struct GifIntake {
    pub source: GifSource,
    /// Always ends in `.gif`, never carries a path, and for a provider GIF
    /// carries the provider's opaque result id rather than any provider
    /// filename.
    pub filename: String,
    pub mime: &'static str,
    pub bytes: Zeroizing<Vec<u8>>,
    pub filmstrip: GifFilmstrip,
    pub strip: GifTrackerStrip,
    pub url_strip: Option<UrlStrip>,
    /// Requests this intake put through the proxy: 1 for a provider GIF, 0 for
    /// a local file.
    pub proxy_requests: u32,
    /// Bytes the provider actually returned, before stripping.
    pub bytes_downloaded: u64,
}

impl GifIntake {
    pub fn plaintext_len(&self) -> u64 {
        self.bytes.len() as u64
    }
}

fn finish_intake(
    source: GifSource,
    filename: String,
    raw: Vec<u8>,
    url_strip: Option<UrlStrip>,
    proxy_requests: u32,
) -> Result<GifIntake, GifIntakeError> {
    let bytes_downloaded = raw.len() as u64;
    let (stripped, strip) = strip_remote_trackers(&raw)?;
    let filmstrip = read_filmstrip(&stripped)?;
    Ok(GifIntake {
        source,
        filename,
        mime: GIF_MIME,
        bytes: Zeroizing::new(stripped),
        filmstrip,
        strip,
        url_strip,
        proxy_requests,
        bytes_downloaded,
    })
}

/// Fetch a searched GIF through the privacy proxy and strip it.
pub fn intake_provider_gif<P: GifProxyTransport>(
    proxy: &P,
    config: &GifProviderConfig,
    result: &GifSearchResult,
    cancel: &GifCancel,
) -> Result<GifIntake, GifIntakeError> {
    cancel.guard()?;
    let request = media_request(config, &result.media_url)?;
    cancel.guard()?;
    let response = proxy.send(&request)?;
    cancel.guard()?;
    if response.status != 200 {
        return Err(GifTransportFailure::ProviderStatus(response.status).into());
    }
    if !response.content_type.starts_with("image/gif") {
        return Err(GifTransportFailure::ProviderUnusable.into());
    }
    if response.body.len() as u64 > MAX_GIF_BYTES {
        return Err(GifRefusal::TooLarge.into());
    }
    finish_intake(
        GifSource::Provider {
            result_id: result.id.clone(),
        },
        format!("{}.gif", result.id),
        response.body,
        Some(result.url_strip.clone()),
        1,
    )
}

/// Take a GIF straight off this device. No provider, no proxy, no network.
pub fn intake_local_gif(path: &Path, cancel: &GifCancel) -> Result<GifIntake, GifIntakeError> {
    cancel.guard()?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(GifRefusal::LocalFileNotAGifName)?
        .to_owned();
    if !filename.to_ascii_lowercase().ends_with(".gif")
        || filename.chars().any(char::is_control)
        || filename.contains(['/', '\\', ':'])
    {
        return Err(GifRefusal::LocalFileNotAGifName.into());
    }
    let metadata = std::fs::metadata(path).map_err(|_| GifRefusal::LocalFileUnreadable)?;
    if !metadata.is_file() {
        return Err(GifRefusal::LocalFileUnreadable.into());
    }
    if metadata.len() > MAX_GIF_BYTES {
        return Err(GifRefusal::TooLarge.into());
    }
    let raw = std::fs::read(path).map_err(|_| GifRefusal::LocalFileUnreadable)?;
    cancel.guard()?;
    finish_intake(GifSource::LocalFile, filename, raw, None, 0)
}

// ---------------------------------------------------------------------------
// Playback authorisation.
// ---------------------------------------------------------------------------

/// Who is asking to play a GIF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifViewer {
    /// This device's own OSL user id.
    pub self_osl_user_id: String,
    /// The peer this conversation is bound to.
    pub bound_peer_osl_user_id: String,
    pub scope: String,
    pub scope_approved: bool,
    pub decrypt_display_enabled: bool,
}

/// What the delivered notice claims about the GIF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GifPlaybackClaim {
    pub recipient_osl_user_id: String,
    pub sender_osl_user_id: String,
    pub scope: String,
    pub mime_type: String,
}

/// A GIF that this viewer is allowed to play, holding real frames.
pub struct GifPlayback {
    pub filmstrip: GifFilmstrip,
    pub bytes: Zeroizing<Vec<u8>>,
}

impl GifPlayback {
    pub fn frame_count(&self) -> usize {
        self.filmstrip.frame_count()
    }
}

/// The gate every GIF display surface must pass.
///
/// Returns a static refusal on every failure so an unauthorised viewer learns
/// nothing about who the GIF was for, how big it is or how many frames it has.
pub fn authorize_gif_playback(
    viewer: &GifViewer,
    claim: &GifPlaybackClaim,
    bytes: &[u8],
) -> Result<GifPlayback, String> {
    if claim.mime_type != GIF_MIME
        || claim.recipient_osl_user_id.is_empty()
        || claim.recipient_osl_user_id != viewer.self_osl_user_id
        || claim.sender_osl_user_id.is_empty()
        || claim.sender_osl_user_id != viewer.bound_peer_osl_user_id
        || claim.scope.is_empty()
        || claim.scope != viewer.scope
        || !viewer.scope_approved
        || !viewer.decrypt_display_enabled
    {
        return Err(GIF_PLAYBACK_REFUSAL.to_owned());
    }
    let filmstrip = read_filmstrip(bytes).map_err(|_| GIF_PLAYBACK_REFUSAL.to_owned())?;
    Ok(GifPlayback {
        filmstrip,
        bytes: Zeroizing::new(bytes.to_vec()),
    })
}

/// Whether the receive side has a real surface for this image MIME type.
///
/// `peer_attachment_io::supported_protected_image_mime` answers for the Windows
/// WIC single-frame viewer and deliberately still says no to GIF. This answers
/// for the GIF player, which is the surface that can hold an animation.
pub fn playable_gif_mime(mime: &str) -> bool {
    mime == GIF_MIME
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-pixel, one-frame GIF89a with a global colour table.
    fn minimal_gif() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0xff, 0xff, 0xff]);
        bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
        bytes.extend_from_slice(&[0x02, 0x02, 0x44, 0x01, 0x00]);
        bytes.push(0x3b);
        bytes
    }

    fn animated_gif(frames: usize, delay: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"GIF89a");
        bytes.extend_from_slice(&[2, 0, 2, 0, 0x80, 0, 0]);
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0xff, 0xff, 0xff]);
        bytes.extend_from_slice(&[0x21, 0xff, 11]);
        bytes.extend_from_slice(b"NETSCAPE2.0");
        bytes.extend_from_slice(&[3, 1, 0, 0, 0]);
        for _ in 0..frames {
            bytes.extend_from_slice(&[0x21, 0xf9, 4, 0x04]);
            bytes.extend_from_slice(&delay.to_le_bytes());
            bytes.extend_from_slice(&[0x00, 0x00]);
            bytes.extend_from_slice(&[0x2c, 0, 0, 0, 0, 2, 0, 2, 0, 0]);
            bytes.extend_from_slice(&[0x02, 0x03, 0x44, 0x08, 0x05, 0x00]);
        }
        bytes.push(0x3b);
        bytes
    }

    #[test]
    fn a_real_gif_reads_as_frames_and_a_fake_one_does_not() {
        let filmstrip = read_filmstrip(&minimal_gif()).expect("minimal GIF reads");
        assert_eq!(filmstrip.frame_count(), 1);
        assert!(!filmstrip.is_animated());
        let animated = read_filmstrip(&animated_gif(6, 8)).expect("animated GIF reads");
        assert_eq!(animated.frame_count(), 6);
        assert!(animated.is_animated());
        assert!(animated.loop_forever);
        assert_eq!(animated.total_duration_ms, 480);
        assert_eq!(read_filmstrip(b"GIF89a"), Err(GifRefusal::Truncated));
        assert_eq!(read_filmstrip(b"not a gif at all"), Err(GifRefusal::NotAGif));
        assert_eq!(read_filmstrip(&[]), Err(GifRefusal::Empty));
    }

    #[test]
    fn tracker_blocks_and_trailing_payload_are_cut_out() {
        let mut hostile = animated_gif(3, 5);
        let trailer = hostile.pop().expect("trailer");
        // An XMP application extension carrying a tracking URL.
        hostile.extend_from_slice(&[0x21, 0xff, 11]);
        hostile.extend_from_slice(b"XMP DataXMP");
        hostile.push(32);
        hostile.extend_from_slice(b"https://track.example/px?id=abcd");
        hostile.push(0x00);
        // A comment extension carrying another one.
        hostile.extend_from_slice(&[0x21, 0xfe, 30]);
        hostile.extend_from_slice(b"https://beacon.example/c?u=99");
        hostile.push(b'!');
        hostile.push(0x00);
        hostile.push(trailer);
        hostile.extend_from_slice(b"https://after-trailer.example/exfil");

        let (clean, strip) = strip_remote_trackers(&hostile).expect("hostile GIF strips");
        assert_eq!(strip.removed_application_extensions, 1);
        assert_eq!(strip.removed_comment_extensions, 1);
        assert_eq!(strip.removed_trailing_bytes, 35);
        assert!(strip.bytes_removed() > 0);
        for needle in [
            &b"track.example"[..],
            &b"beacon.example"[..],
            &b"after-trailer.example"[..],
            &b"https://"[..],
        ] {
            assert!(
                !clean.windows(needle.len()).any(|window| window == needle),
                "stripped GIF still carries a tracker"
            );
        }
        // The loop extension survives, because looping is playback.
        let filmstrip = read_filmstrip(&clean).expect("stripped GIF still reads");
        assert_eq!(filmstrip.frame_count(), 3);
        assert!(filmstrip.loop_forever);
        // Stripping is idempotent.
        let (again, second) = strip_remote_trackers(&clean).expect("second strip");
        assert_eq!(again, clean);
        assert_eq!(second.blocks_removed(), 0);
        assert_eq!(second.removed_trailing_bytes, 0);
    }

    #[test]
    fn media_urls_are_pinned_and_their_tracking_surface_is_removed() {
        let origin = "https://media.example";
        let (url, strip) = sanitize_media_url(
            "https://media.example/a/b/cat.gif?utm_source=x&client_key=99&session_id=7#frag",
            origin,
        )
        .expect("sanitises");
        assert_eq!(url, "https://media.example/a/b/cat.gif");
        assert_eq!(
            strip.stripped_parameters,
            vec![
                "utm_source".to_owned(),
                "client_key".to_owned(),
                "session_id".to_owned()
            ]
        );
        assert!(strip.stripped_fragment);
        for (raw, expected) in [
            ("https://other.example/cat.gif", GifRefusal::UrlNotProviderOrigin),
            ("https://media.example/cat.png", GifRefusal::UrlNotAGifPath),
            ("https://media.example/../cat.gif", GifRefusal::UrlTraversal),
            ("media.example/cat.gif", GifRefusal::UrlNotProviderOrigin),
        ] {
            assert_eq!(sanitize_media_url(raw, origin), Err(expected), "{raw}");
        }
    }

    #[test]
    fn outbound_requests_carry_no_identifying_header() {
        let config = GifProviderConfig {
            search_origin: "https://gifs.example".to_owned(),
            search_path: "/v1/search".to_owned(),
            media_origin: "https://media.example".to_owned(),
        };
        assert!(config.transport_is_private());
        let request = search_request(&config, "happy cat", 8).expect("search request");
        assert_eq!(
            request.url(),
            "https://gifs.example/v1/search?q=happy%20cat&limit=8"
        );
        for (name, _) in &request.headers {
            assert!(!FORBIDDEN_OUTBOUND_HEADERS.contains(&name.as_str()), "{name}");
        }
        let mut hostile = request.clone();
        hostile
            .headers
            .push(("Cookie".to_owned(), "sid=1".to_owned()));
        assert_eq!(
            reject_identifying_headers(&hostile),
            Err(GifRefusal::IdentifyingHeader)
        );
    }

    #[test]
    fn playback_is_refused_for_every_wrong_viewer() {
        let bytes = animated_gif(4, 6);
        let viewer = GifViewer {
            self_osl_user_id: "bob".to_owned(),
            bound_peer_osl_user_id: "alice".to_owned(),
            scope: "scope-1".to_owned(),
            scope_approved: true,
            decrypt_display_enabled: true,
        };
        let claim = GifPlaybackClaim {
            recipient_osl_user_id: "bob".to_owned(),
            sender_osl_user_id: "alice".to_owned(),
            scope: "scope-1".to_owned(),
            mime_type: GIF_MIME.to_owned(),
        };
        let playback = authorize_gif_playback(&viewer, &claim, &bytes).expect("authorised");
        assert_eq!(playback.frame_count(), 4);

        let wrong_recipient = GifViewer {
            self_osl_user_id: "carol".to_owned(),
            ..viewer.clone()
        };
        let wrong_sender = GifPlaybackClaim {
            sender_osl_user_id: "mallory".to_owned(),
            ..claim.clone()
        };
        let wrong_scope = GifPlaybackClaim {
            scope: "scope-2".to_owned(),
            ..claim.clone()
        };
        let unapproved = GifViewer {
            scope_approved: false,
            ..viewer.clone()
        };
        let display_off = GifViewer {
            decrypt_display_enabled: false,
            ..viewer.clone()
        };
        for (viewer, claim) in [
            (&wrong_recipient, &claim),
            (&viewer, &wrong_sender),
            (&viewer, &wrong_scope),
            (&unapproved, &claim),
            (&display_off, &claim),
        ] {
            assert_eq!(
                authorize_gif_playback(viewer, claim, &bytes).err().as_deref(),
                Some(GIF_PLAYBACK_REFUSAL)
            );
        }
    }

    #[test]
    fn failures_are_honest_about_retrying_and_cancellation_is_not_a_failure() {
        assert!(GifTransportFailure::Offline.retryable());
        assert!(GifTransportFailure::ProxyUnavailable.retryable());
        assert!(GifTransportFailure::ProviderStatus(503).retryable());
        assert!(GifTransportFailure::ProviderStatus(429).retryable());
        assert!(!GifTransportFailure::ProviderStatus(404).retryable());
        assert!(!GifTransportFailure::ProviderUnusable.retryable());
        assert!(GifTransportFailure::Offline.message().contains("Try again"));
        assert!(!GifTransportFailure::ProviderStatus(404)
            .message()
            .contains("Try again"));
        for failure in [
            GifTransportFailure::Offline,
            GifTransportFailure::ProxyUnavailable,
            GifTransportFailure::ProviderStatus(500),
            GifTransportFailure::ProviderUnusable,
        ] {
            assert!(failure.message().contains("Nothing was sent"));
        }
        assert!(!GifIntakeError::Cancelled.retryable());
        assert!(GifIntakeError::Cancelled.message().contains("Nothing was sent"));
    }

    #[test]
    fn a_cancelled_local_pick_reads_nothing() {
        let cancel = GifCancel::new();
        cancel.cancel();
        assert_eq!(
            intake_local_gif(Path::new("/definitely/not/here.gif"), &cancel),
            Err(GifIntakeError::Cancelled)
        );
    }
}

impl std::fmt::Debug for GifIntake {
    /// Never prints bytes.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GifIntake")
            .field("source", &self.source)
            .field("filename", &self.filename)
            .field("frames", &self.filmstrip.frame_count())
            .field("plaintext_len", &self.plaintext_len())
            .finish()
    }
}

impl PartialEq for GifIntake {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.filename == other.filename
            && self.bytes == other.bytes
            && self.filmstrip == other.filmstrip
    }
}

impl Eq for GifIntake {}
