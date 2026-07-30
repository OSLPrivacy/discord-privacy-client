//! Request vocabulary and verdict reports for the headless QA self-test
//! rendezvous.
//!
//! WHY THIS IS IN THE LIBRARY. The driver itself lives in
//! `apps/osl-hub/src/main.rs` (`mod qa_selftest`), because it needs the Tauri
//! app handle and the same native commands the protected renderer drives. The
//! `osl-privacy-hub` **binary** cannot be built on a Linux host (`rfd` has no
//! backend there), so every `#[cfg(test)]` inside `main.rs` is compiled but
//! never executed. Everything in this file is therefore the part of the driver
//! that is *pure* -- parsing one request, naming one refusal, tallying one
//! batch -- so that it can be unit-tested for real with
//! `cargo test --features core,discord-qa-shell --lib`.
//!
//! SCOPE. Gated on `discord-qa-shell` exactly like
//! [`crate::discord_qa_inbound_receipt`], so none of it can exist in a
//! production binary.
//!
//! PRIVACY. Nothing here may ever be handed a plaintext, a cover text, a key or
//! a conversation name, and nothing here returns anything but fixed
//! `&'static str` labels, booleans and counts. The one caller-supplied string
//! this module accepts at all is a message id, and it is validated by shape and
//! then handed straight back to the broker -- it is never serialised into a
//! report, because a report is written to disk.

use crate::broker::{
    NativeOverlayAcknowledgmentStatus, OpenedNativeOverlayText, OpenedNativeOverlayTextBatch,
};
use crate::native_apps::NativeAppId;
use crate::native_window_host::DiscordSessionMode;
use serde::Serialize;

/// Longest message id this module will pass through to the broker. The broker
/// applies the real predicate (`valid_peer_attachment_id`); this is only a
/// cheap shape gate so an absurd request is refused by name before it reaches
/// any OSL state.
const MAX_MESSAGE_ID_BYTES: usize = 128;

/// What one triggered invocation is being asked to drive.
///
/// Every variant maps to exactly one entry point the *protected renderer*
/// already drives. Nothing here is a QA-only code path into the broker: this
/// module names verbs, `main.rs` dispatches them onto the real commands, and
/// that is deliberate -- this file has produced QA/production divergences
/// before and a parallel implementation would be another one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verb {
    /// Read-only sweep. Drives nothing, changes nothing.
    Status,
    /// Read-only browser profile inventory. Listing grants nothing.
    ListBrowserProfiles,
    /// Grant one named browser profile to OSL import tooling.
    GrantBrowserProfile,
    /// Revoke one named browser profile grant.
    RevokeBrowserProfile,
    /// Run an import from the browser profiles already granted.
    RunBrowserImport,
    /// Adopt one native app window through the renderer's production command.
    Host,
    /// The original verb: one fixed probe plaintext through the atomic send.
    Send,
    /// Pull and decrypt inbound: `open_native_discord_overlay_text`.
    Drain,
    /// One transcript read/decode pass:
    /// `rehydrate_native_discord_overlay_history`.
    Rehydrate,
    /// The two-phase view-once reveal: drain to list, then reveal one.
    RevealViewOnce,
}

impl Verb {
    /// The fixed label written into the verdict. Never free text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::ListBrowserProfiles => "list-browser-profiles",
            Self::GrantBrowserProfile => "grant-browser-profile",
            Self::RevokeBrowserProfile => "revoke-browser-profile",
            Self::RunBrowserImport => "run-browser-import",
            Self::Host => "host",
            Self::Send => "send",
            Self::Drain => "drain",
            Self::Rehydrate => "rehydrate",
            Self::RevealViewOnce => "reveal-view-once",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "status" => Some(Self::Status),
            "list-browser-profiles" => Some(Self::ListBrowserProfiles),
            "grant-browser-profile" => Some(Self::GrantBrowserProfile),
            "revoke-browser-profile" => Some(Self::RevokeBrowserProfile),
            "run-browser-import" => Some(Self::RunBrowserImport),
            "host" => Some(Self::Host),
            "send" => Some(Self::Send),
            "drain" => Some(Self::Drain),
            "rehydrate" => Some(Self::Rehydrate),
            "reveal-view-once" => Some(Self::RevealViewOnce),
            _ => None,
        }
    }

    /// Whether this verb mutates any OSL state at all. `status` is the only
    /// historical read-only verb; profile listing is read-only for the same
    /// readiness-wait purpose because listing grants nothing.
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::Status | Self::ListBrowserProfiles)
    }
}

/// The resolved inputs for one browser profile grant or revoke request.
///
/// These are identifiers, not paths. The parser rejects traversal fragments,
/// separators and NULs before this value can exist.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserProfileRequest {
    pub browser_id: String,
    pub profile: String,
}

/// The resolved inputs for one `host` request.
///
/// There is deliberately no takeover field. The QA surface has no vocabulary
/// with which to express consent to quit an operator-owned app.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostRequest {
    pub app_id: NativeAppId,
    pub session_mode: DiscordSessionMode,
}

/// The only host action the pure request layer can authorize.
///
/// Keeping this as one payload-bearing variant makes the safety property
/// testable without importing or constructing `DiscordTakeover`: dispatch can
/// only borrow, and the production command receives a literal `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostAction {
    BorrowOnly {
        app_id: NativeAppId,
        session_mode: DiscordSessionMode,
    },
}

impl HostRequest {
    pub fn action(self) -> HostAction {
        HostAction::BorrowOnly {
            app_id: self.app_id,
            session_mode: self.session_mode,
        }
    }
}

/// The post-command facts one `host` verdict reports.
///
/// The verdict's ordinary readiness criteria are the pre-command observation;
/// these fields are the after observation, so one file shows the transition.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostReport {
    pub adopted: bool,
    pub app_id: NativeAppId,
    pub session_mode: DiscordSessionMode,
    pub discord_window_adopted: bool,
    pub overlay_context_valid: bool,
    pub protection_engaged: bool,
}

impl HostReport {
    pub fn after(
        action: HostAction,
        adopted: bool,
        discord_window_adopted: bool,
        overlay_context_valid: bool,
        protection_engaged: bool,
    ) -> Self {
        let HostAction::BorrowOnly {
            app_id,
            session_mode,
        } = action;
        Self {
            adopted,
            app_id,
            session_mode,
            discord_window_adopted,
            overlay_context_valid,
            protection_engaged,
        }
    }
}

/// How `reveal-view-once` chooses which message to reveal.
///
/// No variant requires the harness to have seen a message id, and only
/// `MessageId` lets one in from outside. `Last` exists so the *second* reveal
/// of the same message can be attempted -- the refusal that proves a view-once
/// message opens exactly once currently leaves no trace anywhere on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevealTarget {
    /// Take the n-th entry of the pending view-once list that phase one
    /// returned. The default, and the only one that needs no id anywhere.
    PendingIndex,
    /// Reveal the id the request named.
    MessageId,
    /// Re-attempt the id this process last revealed. Held in memory only and
    /// never written anywhere.
    Last,
}

impl RevealTarget {
    pub fn label(self) -> &'static str {
        match self {
            Self::PendingIndex => "pending-index",
            Self::MessageId => "message-id",
            Self::Last => "last",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        match label {
            "pending-index" => Some(Self::PendingIndex),
            "message-id" => Some(Self::MessageId),
            "last" => Some(Self::Last),
            _ => None,
        }
    }
}

/// One parsed, validated request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelftestRequest {
    pub verb: Verb,
    /// The bundle identifier this request declares it is for, if it declared
    /// one. `None` means "whoever finds it", which is only safe when the
    /// trigger path is already instance-addressed.
    pub instance: Option<String>,
    pub reveal_target: RevealTarget,
    pub pending_index: usize,
    /// Only ever `Some` for `RevealTarget::MessageId`. Passed to the broker and
    /// never serialised into a verdict.
    pub message_id: Option<String>,
    /// Present only for `host`; resolved to borrow-safe defaults by the parser.
    pub host: Option<HostRequest>,
    /// Present only for browser profile grants/revocations.
    pub browser_profile: Option<BrowserProfileRequest>,
    /// Which shape the request body was written in, for the verdict.
    pub format: &'static str,
}

impl SelftestRequest {
    /// The request an empty or free-text trigger body means: exactly what this
    /// rendezvous did before any of these verbs existed.
    pub fn legacy_send() -> Self {
        Self {
            verb: Verb::Send,
            instance: None,
            reveal_target: RevealTarget::PendingIndex,
            pending_index: 0,
            message_id: None,
            host: None,
            browser_profile: None,
            format: FORMAT_LEGACY,
        }
    }
}

pub const FORMAT_LEGACY: &str = "legacy";
pub const FORMAT_JSON: &str = "json";

/// Fixed refusal labels. A refusal is always one of these and never a sentence
/// derived from the request, so a malformed request cannot write its own bytes
/// into the verdict file.
pub const REFUSAL_MALFORMED_JSON: &str = "request-malformed-json";
pub const REFUSAL_NOT_AN_OBJECT: &str = "request-not-an-object";
pub const REFUSAL_VERB_MISSING: &str = "request-verb-missing";
pub const REFUSAL_VERB_NOT_A_STRING: &str = "request-verb-not-a-string";
pub const REFUSAL_VERB_UNKNOWN: &str = "request-verb-unknown";
pub const REFUSAL_INSTANCE_NOT_A_STRING: &str = "request-instance-not-a-string";
pub const REFUSAL_TARGET_NOT_A_STRING: &str = "request-target-not-a-string";
pub const REFUSAL_TARGET_UNKNOWN: &str = "request-target-unknown";
pub const REFUSAL_PENDING_INDEX_INVALID: &str = "request-pending-index-invalid";
pub const REFUSAL_MESSAGE_ID_MISSING: &str = "request-message-id-missing";
pub const REFUSAL_MESSAGE_ID_INVALID: &str = "request-message-id-invalid";
pub const REFUSAL_MESSAGE_ID_UNEXPECTED: &str = "request-message-id-unexpected";
pub const REFUSAL_APP_ID_NOT_A_STRING: &str = "request-app-id-not-a-string";
pub const REFUSAL_APP_ID_UNKNOWN: &str = "request-app-id-unknown";
pub const REFUSAL_APP_ID_UNEXPECTED: &str = "request-app-id-unexpected";
pub const REFUSAL_SESSION_MODE_NOT_A_STRING: &str = "request-session-mode-not-a-string";
pub const REFUSAL_SESSION_MODE_UNKNOWN: &str = "request-session-mode-unknown";
pub const REFUSAL_SESSION_MODE_UNEXPECTED: &str = "request-session-mode-unexpected";
pub const REFUSAL_TAKEOVER_NOT_PERMITTED: &str = "request-takeover-not-permitted";
pub const REFUSAL_BROWSER_ID_MISSING: &str = "request-browser-id-missing";
pub const REFUSAL_BROWSER_ID_NOT_A_STRING: &str = "request-browser-id-not-a-string";
pub const REFUSAL_BROWSER_ID_REJECTED: &str = "request-browser-id-rejected";
pub const REFUSAL_BROWSER_ID_UNEXPECTED: &str = "request-browser-id-unexpected";
pub const REFUSAL_BROWSER_PROFILE_MISSING: &str = "request-browser-profile-missing";
pub const REFUSAL_BROWSER_PROFILE_NOT_A_STRING: &str = "request-browser-profile-not-a-string";
pub const REFUSAL_BROWSER_PROFILE_REJECTED: &str = "request-browser-profile-rejected";
pub const REFUSAL_BROWSER_PROFILE_UNEXPECTED: &str = "request-browser-profile-unexpected";

/// The answer to one trigger body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParsedRequest {
    Accepted(SelftestRequest),
    /// Refused by name. Never a silent no-op: the caller writes this label into
    /// the verdict.
    Refused(&'static str),
}

/// Decide what one trigger file body asks for.
///
/// TWO SHAPES, AND THE BOUNDARY BETWEEN THEM IS THE FIRST NON-SPACE BYTE.
///
/// * A body whose first non-whitespace byte is `{` is a **JSON request** and
///   must parse and validate, or it is refused. It is never quietly downgraded.
/// * Anything else -- an empty file, or the free-text marker the existing
///   two-identity harness writes (`osl-p2p-loop`) -- is the **legacy send**,
///   byte-for-byte the behaviour this rendezvous has always had.
///
/// That split is the whole point: a truncated JSON body (`{"verb":"dra`) must
/// not fall through to "send a message into a live conversation". Failing open
/// into the one verb with an irreversible side effect is the exact mistake this
/// boundary exists to make impossible.
pub fn parse_request(body: &str) -> ParsedRequest {
    // A UTF-8 BOM is NOT whitespace, so `trim_start` leaves it in place. Windows
    // PowerShell's `Set-Content` writes one by default, so a perfectly
    // well-formed JSON request arrived as "\u{feff}{...}", failed the '{' test
    // below, and fell through to the LEGACY SEND -- the one verb with an
    // irreversible side effect. A request must never degrade into sending a
    // message, so the BOM is stripped before anything is decided.
    let trimmed = body.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with('{') {
        // Anything that OPENS like structured data is a request that failed to
        // parse, not a legacy plain-text trigger. Refuse it. Degrading a
        // truncated or malformed request into a send is exactly how a partial
        // write becomes a message nobody asked for; only genuine plain text
        // (the historical `osl-p2p-loop` marker) may still mean "send".
        if trimmed.starts_with('[') || trimmed.starts_with('"') {
            return ParsedRequest::Refused(REFUSAL_MALFORMED_JSON);
        }
        return ParsedRequest::Accepted(SelftestRequest::legacy_send());
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return ParsedRequest::Refused(REFUSAL_MALFORMED_JSON);
    };
    let Some(object) = value.as_object() else {
        return ParsedRequest::Refused(REFUSAL_NOT_AN_OBJECT);
    };

    // A takeover field is a consent receipt for quitting the operator's app.
    // This QA surface cannot collect that consent, so even takeover-shaped
    // spellings are refused rather than silently ignored.
    if object.keys().any(|key| takeoverish_key(key)) {
        return ParsedRequest::Refused(REFUSAL_TAKEOVER_NOT_PERMITTED);
    }

    let Some(verb_value) = object.get("verb") else {
        return ParsedRequest::Refused(REFUSAL_VERB_MISSING);
    };
    let Some(verb_label) = verb_value.as_str() else {
        return ParsedRequest::Refused(REFUSAL_VERB_NOT_A_STRING);
    };
    let Some(verb) = Verb::from_label(verb_label.trim()) else {
        return ParsedRequest::Refused(REFUSAL_VERB_UNKNOWN);
    };

    let instance = match object.get("instance") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => match value.as_str() {
            Some(instance) => Some(instance.trim().to_owned()),
            None => return ParsedRequest::Refused(REFUSAL_INSTANCE_NOT_A_STRING),
        },
    };

    let reveal_target = match object.get("target") {
        None | Some(serde_json::Value::Null) => RevealTarget::PendingIndex,
        Some(value) => match value.as_str() {
            Some(label) => match RevealTarget::from_label(label.trim()) {
                Some(target) => target,
                None => return ParsedRequest::Refused(REFUSAL_TARGET_UNKNOWN),
            },
            None => return ParsedRequest::Refused(REFUSAL_TARGET_NOT_A_STRING),
        },
    };

    let pending_index = match object.get("pendingIndex") {
        None | Some(serde_json::Value::Null) => 0usize,
        Some(value) => match value.as_u64().and_then(|index| usize::try_from(index).ok()) {
            Some(index) => index,
            None => return ParsedRequest::Refused(REFUSAL_PENDING_INDEX_INVALID),
        },
    };

    let message_id = match object.get("messageId") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => match value.as_str() {
            Some(id) if valid_message_id_shape(id) => Some(id.to_owned()),
            _ => return ParsedRequest::Refused(REFUSAL_MESSAGE_ID_INVALID),
        },
    };

    // Only `reveal-view-once` has a target or a message id, and it must be
    // internally consistent. A `messageId` on a `drain` is a harness bug that
    // would otherwise be silently ignored, and silently ignoring an instruction
    // is how a run gets graded against something it never did.
    if verb == Verb::RevealViewOnce {
        match reveal_target {
            RevealTarget::MessageId if message_id.is_none() => {
                return ParsedRequest::Refused(REFUSAL_MESSAGE_ID_MISSING);
            }
            RevealTarget::MessageId => {}
            _ if message_id.is_some() => {
                return ParsedRequest::Refused(REFUSAL_MESSAGE_ID_UNEXPECTED);
            }
            _ => {}
        }
    } else if message_id.is_some() {
        return ParsedRequest::Refused(REFUSAL_MESSAGE_ID_UNEXPECTED);
    }

    let host = if verb == Verb::Host {
        let app_id = match object.get("appId") {
            None | Some(serde_json::Value::Null) => NativeAppId::Discord,
            Some(value) => match value.as_str() {
                Some("discord") => NativeAppId::Discord,
                Some("telegram") => NativeAppId::Telegram,
                Some("signal") => NativeAppId::Signal,
                Some("whatsapp") => NativeAppId::Whatsapp,
                Some("outlook") => NativeAppId::Outlook,
                Some(_) => return ParsedRequest::Refused(REFUSAL_APP_ID_UNKNOWN),
                None => return ParsedRequest::Refused(REFUSAL_APP_ID_NOT_A_STRING),
            },
        };
        let session_mode = match object.get("sessionMode") {
            None | Some(serde_json::Value::Null) => DiscordSessionMode::ExistingSession,
            Some(value) => match value.as_str() {
                Some("dedicated") => DiscordSessionMode::Dedicated,
                Some("existingSession") => DiscordSessionMode::ExistingSession,
                Some(_) => return ParsedRequest::Refused(REFUSAL_SESSION_MODE_UNKNOWN),
                None => return ParsedRequest::Refused(REFUSAL_SESSION_MODE_NOT_A_STRING),
            },
        };
        Some(HostRequest {
            app_id,
            session_mode,
        })
    } else {
        if object.contains_key("appId") {
            return ParsedRequest::Refused(REFUSAL_APP_ID_UNEXPECTED);
        }
        if object.contains_key("sessionMode") {
            return ParsedRequest::Refused(REFUSAL_SESSION_MODE_UNEXPECTED);
        }
        None
    };

    let browser_profile = if matches!(verb, Verb::GrantBrowserProfile | Verb::RevokeBrowserProfile)
    {
        match parse_browser_profile_request(object) {
            Ok(request) => Some(request),
            Err(refusal) => return refusal,
        }
    } else {
        if object.contains_key("browserId") {
            return ParsedRequest::Refused(REFUSAL_BROWSER_ID_UNEXPECTED);
        }
        if object.contains_key("profile") {
            return ParsedRequest::Refused(REFUSAL_BROWSER_PROFILE_UNEXPECTED);
        }
        None
    };

    ParsedRequest::Accepted(SelftestRequest {
        verb,
        instance,
        reveal_target,
        pending_index,
        message_id,
        host,
        browser_profile,
        format: FORMAT_JSON,
    })
}

fn parse_browser_profile_request(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<BrowserProfileRequest, ParsedRequest> {
    let Some(browser_id_value) = object.get("browserId") else {
        return Err(ParsedRequest::Refused(REFUSAL_BROWSER_ID_MISSING));
    };
    let Some(browser_id) = browser_id_value.as_str() else {
        return Err(ParsedRequest::Refused(REFUSAL_BROWSER_ID_NOT_A_STRING));
    };
    if !valid_browser_id(browser_id) {
        return Err(ParsedRequest::Refused(REFUSAL_BROWSER_ID_REJECTED));
    }

    let Some(profile_value) = object.get("profile") else {
        return Err(ParsedRequest::Refused(REFUSAL_BROWSER_PROFILE_MISSING));
    };
    let Some(profile) = profile_value.as_str() else {
        return Err(ParsedRequest::Refused(REFUSAL_BROWSER_PROFILE_NOT_A_STRING));
    };
    if !valid_browser_profile(profile) {
        return Err(ParsedRequest::Refused(REFUSAL_BROWSER_PROFILE_REJECTED));
    }

    Ok(BrowserProfileRequest {
        browser_id: browser_id.to_owned(),
        profile: profile.to_owned(),
    })
}

fn contains_forbidden_browser_identifier_fragment(value: &str) -> bool {
    value.contains("..")
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.contains('\0')
}

fn valid_browser_id(browser_id: &str) -> bool {
    !contains_forbidden_browser_identifier_fragment(browser_id)
        && !browser_id.is_empty()
        && browser_id.len() <= 32
        && browser_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn valid_browser_profile(profile: &str) -> bool {
    !contains_forbidden_browser_identifier_fragment(profile)
        && !profile.is_empty()
        && profile.len() <= 64
        && profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b' ' | b'.' | b'_' | b'-'))
}

fn takeoverish_key(key: &str) -> bool {
    let normalized: String = key
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect();
    normalized.contains("takeover")
        || normalized.contains("quitandrelaunch")
        || matches!(normalized.as_str(), "quitdiscord" | "discordquit")
}

/// A cheap shape gate, not the authority. The broker's own
/// `valid_peer_attachment_id` decides; this only keeps something absurd from
/// travelling any further than the parser.
fn valid_message_id_shape(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_MESSAGE_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// The readiness criterion ids the driver reports for every verb, in the order
/// it inserts them. Named here so the per-verb grading table below can be
/// checked against the full set rather than against whatever the caller
/// happened to pass.
pub const READINESS_CRITERIA: [&str; 7] = [
    "identity_unlocked",
    "discord_window_adopted",
    "protection_engaged",
    "composer_window_exists",
    "composer_window_visible",
    "composer_above_discord",
    "overlay_context_valid",
];

/// Whether one readiness criterion is a claim `verb` actually makes.
///
/// `send` claims all of them: it hands the operator's own composer to Discord,
/// and a keystroke that lands in the wrong window is plaintext in a real
/// conversation.
///
/// The receive-side verbs claim an unlocked identity, an adopted Discord
/// window, an existing overlay window -- the caller identity all three commands
/// check -- and a valid overlay context, which is what binds a drain to one
/// conversation. They claim nothing about the lock being engaged or the
/// composer being on screen and stacked: `lockEngaged` is whether what the
/// operator *types* is encrypted, and none of these verbs types anything.
/// Grading those would make a receiving instance ungradeable for the entirely
/// correct reason that nobody was writing on it.
///
/// `status` claims none of them. It reports the world; it does not require one.
pub fn readiness_criterion_is_graded(verb: Verb, id: &str) -> bool {
    match verb {
        Verb::Status
        | Verb::ListBrowserProfiles
        | Verb::GrantBrowserProfile
        | Verb::RevokeBrowserProfile
        | Verb::RunBrowserImport
        | Verb::Host => false,
        Verb::Send => true,
        Verb::Drain | Verb::Rehydrate | Verb::RevealViewOnce => matches!(
            id,
            "identity_unlocked"
                | "discord_window_adopted"
                | "composer_window_exists"
                | "overlay_context_valid"
        ),
    }
}

/// Fixed refusal for a verb whose readiness wait expired.
///
/// The readiness criteria retain the detailed observations; this label ensures
/// the top-level `not-ready` outcome is never an unnamed no-op.
pub fn not_ready_refusal(verb: Verb) -> &'static str {
    match verb {
        Verb::Status => "status-readiness-precondition-missing",
        Verb::ListBrowserProfiles => "list-browser-profiles-readiness-precondition-missing",
        Verb::GrantBrowserProfile => "grant-browser-profile-readiness-precondition-missing",
        Verb::RevokeBrowserProfile => "revoke-browser-profile-readiness-precondition-missing",
        Verb::RunBrowserImport => "run-browser-import-readiness-precondition-missing",
        Verb::Host => "host-readiness-precondition-missing",
        Verb::Send => "send-readiness-precondition-missing",
        Verb::Drain => "drain-readiness-precondition-missing",
        Verb::Rehydrate => "rehydrate-readiness-precondition-missing",
        Verb::RevealViewOnce => "reveal-readiness-precondition-missing",
    }
}

/// Whether a request addressed to `declared` should be answered by the instance
/// whose bundle identifier is `mine`.
///
/// A request that declares nothing is for whoever is polling that path, which
/// is safe precisely because the addressed trigger path already names one
/// instance.
pub fn request_is_for_me(declared: Option<&str>, mine: &str) -> bool {
    match declared {
        None => true,
        Some(declared) => declared == mine,
    }
}

/// Make one bundle identifier safe to embed in a file name, without ever
/// letting two distinct identifiers collapse onto one path by truncation
/// alone -- the whole point of the addressed trigger is that two instances
/// never poll the same file.
pub fn instance_file_token(identifier: &str) -> String {
    const MAX_TOKEN_BYTES: usize = 96;
    let mut token: String = identifier
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if token.is_empty() {
        token.push_str("unnamed");
    }
    if token.len() > MAX_TOKEN_BYTES {
        // Truncating alone could alias two identifiers onto one path, so the
        // truncated form carries a digest of the whole identifier.
        let digest = sha256_hex(identifier.as_bytes());
        token.truncate(MAX_TOKEN_BYTES - 17);
        token.push('-');
        token.push_str(&digest[..16]);
    }
    token
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The fixed label for one acknowledgement kind.
///
/// *Received* and *Opened* are two different claims about a message and they
/// have been collapsed into a single `acknowledgmentCount` everywhere the QA
/// surface can see, which makes "the receipts arrived in the right order"
/// unprovable from outside. This is the vocabulary that separates them.
pub fn acknowledgment_kind_label(status: NativeOverlayAcknowledgmentStatus) -> &'static str {
    match status {
        NativeOverlayAcknowledgmentStatus::Received => "received",
        NativeOverlayAcknowledgmentStatus::Opened => "opened",
    }
}

/// Every acknowledgement kind in a batch, in the order the batch carries them.
/// Fixed labels only; the ids they belong to never leave the broker.
pub fn acknowledgment_kind_order(batch: &OpenedNativeOverlayTextBatch) -> Vec<&'static str> {
    batch
        .acknowledgments
        .iter()
        .map(|acknowledgment| acknowledgment_kind_label(acknowledgment.status))
        .collect()
}

pub fn acknowledgment_count_of(
    batch: &OpenedNativeOverlayTextBatch,
    status: NativeOverlayAcknowledgmentStatus,
) -> usize {
    batch
        .acknowledgments
        .iter()
        .filter(|acknowledgment| acknowledgment.status == status)
        .count()
}

fn sorted_deduplicated_strings<I, S>(values: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut values: Vec<String> = values.into_iter().map(Into::into).collect();
    values.sort();
    values.dedup();
    values
}

/// What one browser profile inventory observed.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfileListReport {
    pub profiles: Vec<String>,
}

impl BrowserProfileListReport {
    pub fn from_profiles<I, S>(profiles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            profiles: sorted_deduplicated_strings(profiles),
        }
    }
}

/// What one browser import run observed about its consent-scoped sources.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserImportReport {
    pub source_profiles: Vec<String>,
}

impl BrowserImportReport {
    pub fn from_source_profiles<I, S>(source_profiles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            source_profiles: sorted_deduplicated_strings(source_profiles),
        }
    }
}

/// Automation lanes covered by the challenge/stop/restart QA matrix.
///
/// The labels are fixed capability names, not account ids or provider-supplied
/// strings. F4 is the reviewed IMAP item path; F5 is the hosted scan-only path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AutomationQaLane {
    F4Imap,
    F5HostedScan,
}

impl AutomationQaLane {
    pub fn label(self) -> &'static str {
        match self {
            Self::F4Imap => "f4-imap",
            Self::F5HostedScan => "f5-hosted-scan",
        }
    }

    fn authority_scope(self) -> &'static str {
        match self {
            Self::F4Imap => "reviewed-imap-item",
            Self::F5HostedScan => "scan-only-hosted-port",
        }
    }
}

/// The interruption states the QA fixture must exercise for each lane.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AutomationQaInterruption {
    Challenge,
    Stop,
    Restart,
}

impl AutomationQaInterruption {
    pub fn label(self) -> &'static str {
        match self {
            Self::Challenge => "challenge",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }

    fn refusal(self) -> &'static str {
        match self {
            Self::Challenge => "operator-challenge-required",
            Self::Stop => "operator-stop-revoked-authority",
            Self::Restart => "restart-requires-fresh-authority",
        }
    }
}

/// One fixed-label decision in the F4/F5 challenge matrix.
///
/// `may_continue` and `may_delete` are deliberately false for every
/// interruption. A challenge, stop, or restart must never be interpreted as
/// permission to keep acting with stale authority.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationQaInterruptionDecision {
    pub lane: &'static str,
    pub interruption: &'static str,
    pub authority_scope: &'static str,
    pub may_continue: bool,
    pub may_delete: bool,
    pub refusal: &'static str,
}

pub fn challenge_stop_restart_decision(
    lane: AutomationQaLane,
    interruption: AutomationQaInterruption,
) -> AutomationQaInterruptionDecision {
    AutomationQaInterruptionDecision {
        lane: lane.label(),
        interruption: interruption.label(),
        authority_scope: lane.authority_scope(),
        may_continue: false,
        may_delete: false,
        refusal: interruption.refusal(),
    }
}

pub fn challenge_stop_restart_matrix() -> Vec<AutomationQaInterruptionDecision> {
    [AutomationQaLane::F4Imap, AutomationQaLane::F5HostedScan]
        .into_iter()
        .flat_map(|lane| {
            [
                AutomationQaInterruption::Challenge,
                AutomationQaInterruption::Stop,
                AutomationQaInterruption::Restart,
            ]
            .into_iter()
            .map(move |interruption| challenge_stop_restart_decision(lane, interruption))
        })
        .collect()
}

/// What one drive of the inbound drain observed.
///
/// Counts and booleans only. There is no field here that any plaintext, cover
/// text, message id or conversation name can reach: the tallies below read
/// exactly the structural flags of the batch and nothing else.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrainReport {
    pub opened_count: usize,
    pub pending_view_once_count: usize,
    pub acknowledgment_count: usize,
    /// The two halves of `acknowledgmentCount`, split. **P3b** is exactly this
    /// pair plus `acknowledgmentKindOrder` below.
    pub acknowledgment_received_count: usize,
    pub acknowledgment_opened_count: usize,
    /// Fixed `received` / `opened` labels in batch order, so "Received first,
    /// then Opened" is a readable fact rather than an inference from one total.
    pub acknowledgment_kind_order: Vec<&'static str>,
    pub fetched: u32,
    pub deferred_rows: u32,
    pub decrypt_display_enabled: bool,
    pub context_verified_count: usize,
    pub person_to_person_e2ee_count: usize,
    pub view_once_consumed_count: usize,
    /// Opened messages that carry a cover handle, i.e. rows the eye can paint
    /// **in place** over the Discord row they belong to rather than in the
    /// protected viewport. This is the structural half of **P6**.
    pub cover_pointer_count: usize,
}

impl DrainReport {
    pub fn from_batch(batch: &OpenedNativeOverlayTextBatch) -> Self {
        let count = |predicate: fn(&OpenedNativeOverlayText) -> bool| {
            batch
                .messages
                .iter()
                .filter(|message| predicate(message))
                .count()
        };
        Self {
            opened_count: batch.messages.len(),
            pending_view_once_count: batch.pending_view_once.len(),
            acknowledgment_count: batch.acknowledgments.len(),
            acknowledgment_received_count: acknowledgment_count_of(
                batch,
                NativeOverlayAcknowledgmentStatus::Received,
            ),
            acknowledgment_opened_count: acknowledgment_count_of(
                batch,
                NativeOverlayAcknowledgmentStatus::Opened,
            ),
            acknowledgment_kind_order: acknowledgment_kind_order(batch),
            fetched: batch.fetched,
            deferred_rows: batch.deferred_rows,
            decrypt_display_enabled: batch.decrypt_display_enabled,
            context_verified_count: count(|message| message.context_verified),
            person_to_person_e2ee_count: count(|message| message.person_to_person_e2ee),
            view_once_consumed_count: count(|message| message.view_once_consumed),
            cover_pointer_count: count(|message| message.cover_pointer.is_some()),
        }
    }
}

/// What one drive of the transcript rehydration observed.
///
/// `read` false with a non-zero `retryAfterMs` is the same-scope floor
/// refusing, which is a *rate limit*, not an empty conversation -- the two used
/// to be one empty vector.
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RehydrateReport {
    pub read: bool,
    pub retry_after_ms: u64,
    pub row_count: usize,
    /// Rows OSL could decrypt. **P6**: on an instance that sent nothing, every
    /// one of these is a row it did not send.
    pub decoded_row_count: usize,
    /// Decoded rows the eye can paint over their own Discord row.
    pub placed_row_count: usize,
    /// Decoded rows OSL cannot presently place, and therefore paints nowhere.
    pub unplaceable_row_count: usize,
    /// Rows that came back with no plaintext at all.
    pub opaque_row_count: usize,
}

impl RehydrateReport {
    /// `rows` is one `(decoded, placeable)` pair per row. It deliberately
    /// cannot be handed a row's text: the caller projects its rows down to two
    /// booleans before this is ever called.
    pub fn tally(read: bool, retry_after_ms: u64, rows: &[(bool, bool)]) -> Self {
        Self {
            read,
            retry_after_ms,
            row_count: rows.len(),
            decoded_row_count: rows.iter().filter(|(decoded, _)| *decoded).count(),
            placed_row_count: rows
                .iter()
                .filter(|(decoded, placeable)| *decoded && *placeable)
                .count(),
            unplaceable_row_count: rows
                .iter()
                .filter(|(decoded, placeable)| *decoded && !*placeable)
                .count(),
            opaque_row_count: rows.iter().filter(|(decoded, _)| !*decoded).count(),
        }
    }

    /// The honest report for a rehydration that never returned a transcript.
    pub fn refused() -> Self {
        Self::tally(false, 0, &[])
    }
}

/// What one drive of the two-phase view-once reveal observed.
///
/// Phase one is the drain that *lists* the message without opening it (**P4a**)
/// and phase two is the reveal that opens it exactly once (**P4b**). Driving
/// the same target twice makes the second refusal -- which today exists only as
/// a string handed to the renderer -- a written fact (**P4c**).
#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealReport {
    pub phase_one_driven: bool,
    pub phase_one_pending_count: usize,
    pub phase_one_opened_count: usize,
    pub target: &'static str,
    pub requested_index: usize,
    pub selected: bool,
    pub phase_two_driven: bool,
    pub revealed: bool,
    pub view_once_consumed: bool,
    pub context_verified: bool,
    pub person_to_person_e2ee: bool,
    pub has_cover_pointer: bool,
    /// True when phase two was driven and OSL refused it. On a second attempt
    /// against an already-consumed message this is the **expected** answer.
    pub refused: bool,
}

impl RevealReport {
    pub fn not_driven(target: RevealTarget, requested_index: usize) -> Self {
        Self {
            phase_one_driven: false,
            phase_one_pending_count: 0,
            phase_one_opened_count: 0,
            target: target.label(),
            requested_index,
            selected: false,
            phase_two_driven: false,
            revealed: false,
            view_once_consumed: false,
            context_verified: false,
            person_to_person_e2ee: false,
            has_cover_pointer: false,
            refused: false,
        }
    }

    /// Record what the reveal itself returned. The `Ok` value is the opened
    /// message, and only its structural flags are read -- `plaintext` is not
    /// touched here and `OpenedNativeOverlayText` does not implement `Debug`
    /// precisely so that it cannot be.
    pub fn record_phase_two(&mut self, result: Result<&OpenedNativeOverlayText, ()>) {
        self.phase_two_driven = true;
        match result {
            Ok(message) => {
                self.revealed = true;
                self.refused = false;
                self.view_once_consumed = message.view_once_consumed;
                self.context_verified = message.context_verified;
                self.person_to_person_e2ee = message.person_to_person_e2ee;
                self.has_cover_pointer = message.cover_pointer.is_some();
            }
            Err(()) => {
                self.revealed = false;
                self.refused = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{NativeOverlayAcknowledgment, PendingNativeOverlayText};

    fn accepted(body: &str) -> SelftestRequest {
        match parse_request(body) {
            ParsedRequest::Accepted(request) => request,
            ParsedRequest::Refused(label) => panic!("refused as {label}"),
        }
    }

    fn refusal(body: &str) -> &'static str {
        match parse_request(body) {
            ParsedRequest::Accepted(request) => panic!("accepted as {}", request.verb.label()),
            ParsedRequest::Refused(label) => label,
        }
    }

    #[test]
    fn an_empty_or_free_text_trigger_is_still_the_original_send() {
        // The two-identity harness writes the literal marker `osl-p2p-loop`
        // into the trigger file (scripts/qa/osl-p2p-loop.ps1). Every one of
        // these must keep meaning exactly what it meant before verbs existed.
        for body in ["", "   ", "\r\n", "osl-p2p-loop", "anything at all"] {
            let request = accepted(body);
            assert_eq!(request.verb, Verb::Send, "body {body:?}");
            assert_eq!(request.format, FORMAT_LEGACY, "body {body:?}");
            assert_eq!(request.instance, None, "body {body:?}");
        }
    }

    #[test]
    fn a_body_that_opens_a_json_object_is_never_downgraded_to_a_send() {
        // The failure this forbids: a truncated or mistyped JSON request
        // falling through to the one verb that puts a message into a live
        // conversation. Every one of these must refuse by name.
        for body in [
            "{",
            "{\"verb\":",
            "{\"verb\":\"dra",
            "  {\"verb\": \"drain\",}",
            "{ not json at all }",
        ] {
            assert_eq!(refusal(body), REFUSAL_MALFORMED_JSON, "body {body:?}");
        }
    }

    #[test]
    fn every_verb_round_trips_through_its_own_fixed_label() {
        for verb in [
            Verb::Status,
            Verb::Host,
            Verb::Send,
            Verb::Drain,
            Verb::Rehydrate,
            Verb::RevealViewOnce,
        ] {
            let body = format!("{{\"verb\":\"{}\"}}", verb.label());
            let request = accepted(&body);
            assert_eq!(request.verb, verb);
            assert_eq!(request.format, FORMAT_JSON);
        }
    }

    #[test]
    fn new_browser_verbs_round_trip_through_their_own_fixed_labels() {
        for verb in [
            Verb::ListBrowserProfiles,
            Verb::GrantBrowserProfile,
            Verb::RevokeBrowserProfile,
            Verb::RunBrowserImport,
        ] {
            assert_eq!(Verb::from_label(verb.label()), Some(verb));
        }
    }

    #[test]
    fn status_and_profile_listing_are_read_only() {
        assert!(Verb::Status.is_read_only());
        assert!(Verb::ListBrowserProfiles.is_read_only());
        for verb in [
            Verb::GrantBrowserProfile,
            Verb::RevokeBrowserProfile,
            Verb::RunBrowserImport,
            Verb::Host,
            Verb::Send,
            Verb::Drain,
            Verb::Rehydrate,
            Verb::RevealViewOnce,
        ] {
            assert!(!verb.is_read_only(), "{}", verb.label());
        }
    }

    fn browser_profile_json(verb: Verb, browser_id: &str, profile: &str) -> String {
        serde_json::json!({
            "verb": verb.label(),
            "browserId": browser_id,
            "profile": profile,
        })
        .to_string()
    }

    #[test]
    fn browser_profile_grant_and_revoke_accept_normal_identifiers() {
        for (verb, browser_id, profile) in [
            (Verb::GrantBrowserProfile, "firefox", "Default Profile_1"),
            (Verb::RevokeBrowserProfile, "chrome-beta", "Work.Profile-2"),
        ] {
            let request = accepted(&browser_profile_json(verb, browser_id, profile));
            assert_eq!(request.verb, verb);
            assert_eq!(
                request.browser_profile,
                Some(BrowserProfileRequest {
                    browser_id: browser_id.to_owned(),
                    profile: profile.to_owned(),
                })
            );
        }
    }

    #[test]
    fn browser_profile_grant_and_revoke_refuse_missing_or_mistyped_fields() {
        for verb in [Verb::GrantBrowserProfile, Verb::RevokeBrowserProfile] {
            let missing_browser = serde_json::json!({
                "verb": verb.label(),
                "profile": "Default",
            })
            .to_string();
            assert_eq!(
                refusal(&missing_browser),
                REFUSAL_BROWSER_ID_MISSING,
                "{}",
                verb.label()
            );

            let missing_profile = serde_json::json!({
                "verb": verb.label(),
                "browserId": "firefox",
            })
            .to_string();
            assert_eq!(
                refusal(&missing_profile),
                REFUSAL_BROWSER_PROFILE_MISSING,
                "{}",
                verb.label()
            );

            let browser_not_string = serde_json::json!({
                "verb": verb.label(),
                "browserId": 7,
                "profile": "Default",
            })
            .to_string();
            assert_eq!(
                refusal(&browser_not_string),
                REFUSAL_BROWSER_ID_NOT_A_STRING,
                "{}",
                verb.label()
            );

            let profile_not_string = serde_json::json!({
                "verb": verb.label(),
                "browserId": "firefox",
                "profile": false,
            })
            .to_string();
            assert_eq!(
                refusal(&profile_not_string),
                REFUSAL_BROWSER_PROFILE_NOT_A_STRING,
                "{}",
                verb.label()
            );
        }
    }

    #[test]
    fn browser_profile_grant_and_revoke_refuse_rejected_identifier_values() {
        for verb in [Verb::GrantBrowserProfile, Verb::RevokeBrowserProfile] {
            for profile in [
                "Profile..Two",
                "Profile/Two",
                "Profile\\Two",
                "Profile:Two",
                "",
            ] {
                assert_eq!(
                    refusal(&browser_profile_json(verb, "firefox", profile)),
                    REFUSAL_BROWSER_PROFILE_REJECTED,
                    "{} profile {profile:?}",
                    verb.label()
                );
            }

            let long_profile = "a".repeat(65);
            assert_eq!(
                refusal(&browser_profile_json(verb, "firefox", &long_profile)),
                REFUSAL_BROWSER_PROFILE_REJECTED,
                "{} over-long profile",
                verb.label()
            );

            for browser_id in ["", "firefox/qa", "firefox\\qa", "firefox:qa", "firefox..qa"] {
                assert_eq!(
                    refusal(&browser_profile_json(verb, browser_id, "Default")),
                    REFUSAL_BROWSER_ID_REJECTED,
                    "{} browser id {browser_id:?}",
                    verb.label()
                );
            }

            let long_browser_id = "a".repeat(33);
            assert_eq!(
                refusal(&browser_profile_json(verb, &long_browser_id, "Default")),
                REFUSAL_BROWSER_ID_REJECTED,
                "{} over-long browser id",
                verb.label()
            );
        }
    }

    #[test]
    fn new_browser_verbs_refuse_malformed_bodies_without_legacy_send_fallback() {
        for verb in [
            Verb::ListBrowserProfiles,
            Verb::GrantBrowserProfile,
            Verb::RevokeBrowserProfile,
            Verb::RunBrowserImport,
        ] {
            let malformed = format!("{{\"verb\":\"{}\"", verb.label());
            assert_eq!(refusal(&malformed), REFUSAL_MALFORMED_JSON);
        }
    }

    #[test]
    fn an_unknown_verb_is_refused_by_name_and_never_silently_ignored() {
        assert_eq!(refusal("{\"verb\":\"drian\"}"), REFUSAL_VERB_UNKNOWN);
        assert_eq!(refusal("{\"verb\":\"burn\"}"), REFUSAL_VERB_UNKNOWN);
        assert_eq!(refusal("{\"instance\":\"a\"}"), REFUSAL_VERB_MISSING);
        assert_eq!(refusal("{\"verb\":7}"), REFUSAL_VERB_NOT_A_STRING);
    }

    #[test]
    fn host_defaults_to_borrowing_the_existing_discord_session() {
        let request = accepted("{\"verb\":\"host\"}");
        assert_eq!(
            request.host,
            Some(HostRequest {
                app_id: NativeAppId::Discord,
                session_mode: DiscordSessionMode::ExistingSession,
            })
        );

        let explicit =
            accepted("{\"verb\":\"host\",\"appId\":\"signal\",\"sessionMode\":\"dedicated\"}");
        assert_eq!(
            explicit.host,
            Some(HostRequest {
                app_id: NativeAppId::Signal,
                session_mode: DiscordSessionMode::Dedicated,
            })
        );
    }

    #[test]
    fn host_refuses_unknown_or_mistyped_inputs_by_name() {
        assert_eq!(
            refusal("{\"verb\":\"host\",\"appId\":\"DiscordCanary\"}"),
            REFUSAL_APP_ID_UNKNOWN
        );
        assert_eq!(
            refusal("{\"verb\":\"host\",\"appId\":7}"),
            REFUSAL_APP_ID_NOT_A_STRING
        );
        assert_eq!(
            refusal("{\"verb\":\"host\",\"sessionMode\":\"borrow\"}"),
            REFUSAL_SESSION_MODE_UNKNOWN
        );
        assert_eq!(
            refusal("{\"verb\":\"host\",\"sessionMode\":false}"),
            REFUSAL_SESSION_MODE_NOT_A_STRING
        );
        assert_eq!(
            refusal("{\"verb\":\"drain\",\"appId\":\"discord\"}"),
            REFUSAL_APP_ID_UNEXPECTED
        );
        assert_eq!(
            refusal("{\"verb\":\"send\",\"sessionMode\":\"existingSession\"}"),
            REFUSAL_SESSION_MODE_UNEXPECTED
        );
    }

    #[test]
    fn host_takeover_is_structurally_impossible_and_takeover_keys_are_refused() {
        let action = accepted("{\"verb\":\"host\"}")
            .host
            .expect("host request")
            .action();
        assert_eq!(
            action,
            HostAction::BorrowOnly {
                app_id: NativeAppId::Discord,
                session_mode: DiscordSessionMode::ExistingSession,
            }
        );

        for key in [
            "takeover",
            "discordTakeover",
            "discord_takeover",
            "requestTakeoverReceipt",
            "quitAndRelaunch",
            "quitDiscord",
        ] {
            let body = format!("{{\"verb\":\"host\",\"{key}\":true}}");
            assert_eq!(refusal(&body), REFUSAL_TAKEOVER_NOT_PERMITTED, "key {key}");
        }
    }

    #[test]
    fn a_structured_body_that_is_not_an_object_is_refused_never_sent() {
        // CHANGED 2026-07-26. This previously asserted that `[1,2,3]` is a
        // legacy body and therefore SENDS. That is the wrong reading: a body
        // that opens like structured data is a request that failed to parse,
        // and answering a failed parse with the one irreversible verb is the
        // same defect class as the BOM fall-through below. Genuine plain text
        // still means legacy send; structured-but-wrong now refuses.
        assert_eq!(refusal("[1,2,3]"), REFUSAL_MALFORMED_JSON);
        assert_eq!(refusal("\"just a string\""), REFUSAL_MALFORMED_JSON);
        assert_eq!(accepted("osl-p2p-loop").verb, Verb::Send);
        assert_eq!(accepted("osl-p2p-loop").format, FORMAT_LEGACY);
    }

    /// A UTF-8 BOM is not whitespace. PowerShell's `Set-Content` writes one by
    /// default, so this is what a real harness actually put on disk -- and it
    /// used to be answered by SENDING A MESSAGE instead of running the verb.
    #[test]
    fn a_bom_prefixed_request_runs_its_verb_and_never_degrades_into_a_send() {
        let bom = "\u{feff}{\"verb\":\"status\"}";
        assert_eq!(
            accepted(bom).verb,
            Verb::Status,
            "BOM must not reach the legacy send"
        );
        assert_ne!(accepted(bom).format, FORMAT_LEGACY);

        // And a BOM in front of a MALFORMED object refuses, rather than falling
        // through to the send the way it did before.
        assert_eq!(refusal("\u{feff}{\"verb\":"), REFUSAL_MALFORMED_JSON);
    }

    #[test]
    fn a_declared_instance_is_carried_through_and_type_checked() {
        let request = accepted("{\"verb\":\"drain\",\"instance\":\"org.oslprivacy.hubqab\"}");
        assert_eq!(request.instance.as_deref(), Some("org.oslprivacy.hubqab"));
        assert_eq!(
            refusal("{\"verb\":\"drain\",\"instance\":5}"),
            REFUSAL_INSTANCE_NOT_A_STRING
        );
    }

    #[test]
    fn an_undeclared_request_is_for_whoever_polls_that_path() {
        assert!(request_is_for_me(None, "org.oslprivacy.hub"));
        assert!(request_is_for_me(
            Some("org.oslprivacy.hub"),
            "org.oslprivacy.hub"
        ));
        assert!(!request_is_for_me(
            Some("org.oslprivacy.hubqab"),
            "org.oslprivacy.hub"
        ));
        // Prefix-sharing identifiers must not be confused for one another.
        assert!(!request_is_for_me(
            Some("org.oslprivacy.hub"),
            "org.oslprivacy.hubqab"
        ));
    }

    #[test]
    fn reveal_targets_are_validated_against_their_message_id() {
        let default = accepted("{\"verb\":\"reveal-view-once\"}");
        assert_eq!(default.reveal_target, RevealTarget::PendingIndex);
        assert_eq!(default.pending_index, 0);
        assert_eq!(default.message_id, None);

        let indexed = accepted("{\"verb\":\"reveal-view-once\",\"pendingIndex\":2}");
        assert_eq!(indexed.pending_index, 2);

        let last = accepted("{\"verb\":\"reveal-view-once\",\"target\":\"last\"}");
        assert_eq!(last.reveal_target, RevealTarget::Last);

        let named = accepted(
            "{\"verb\":\"reveal-view-once\",\"target\":\"message-id\",\"messageId\":\"peer-0011\"}",
        );
        assert_eq!(named.reveal_target, RevealTarget::MessageId);
        assert_eq!(named.message_id.as_deref(), Some("peer-0011"));

        assert_eq!(
            refusal("{\"verb\":\"reveal-view-once\",\"target\":\"message-id\"}"),
            REFUSAL_MESSAGE_ID_MISSING
        );
        assert_eq!(
            refusal("{\"verb\":\"reveal-view-once\",\"target\":\"whatever\"}"),
            REFUSAL_TARGET_UNKNOWN
        );
        assert_eq!(
            refusal("{\"verb\":\"reveal-view-once\",\"target\":3}"),
            REFUSAL_TARGET_NOT_A_STRING
        );
        assert_eq!(
            refusal("{\"verb\":\"reveal-view-once\",\"pendingIndex\":-1}"),
            REFUSAL_PENDING_INDEX_INVALID
        );
        assert_eq!(
            refusal("{\"verb\":\"reveal-view-once\",\"pendingIndex\":\"0\"}"),
            REFUSAL_PENDING_INDEX_INVALID
        );
    }

    #[test]
    fn a_message_id_is_refused_where_it_would_be_silently_ignored() {
        // A `messageId` on a drain, or alongside a `pending-index` target, is a
        // harness instruction this driver would not honour. Ignoring it would
        // grade a run against something it never did.
        assert_eq!(
            refusal("{\"verb\":\"drain\",\"messageId\":\"peer-0011\"}"),
            REFUSAL_MESSAGE_ID_UNEXPECTED
        );
        assert_eq!(
            refusal("{\"verb\":\"reveal-view-once\",\"messageId\":\"peer-0011\"}"),
            REFUSAL_MESSAGE_ID_UNEXPECTED
        );
        assert_eq!(
            refusal(
                "{\"verb\":\"reveal-view-once\",\"target\":\"last\",\"messageId\":\"peer-0011\"}"
            ),
            REFUSAL_MESSAGE_ID_UNEXPECTED
        );
    }

    #[test]
    fn a_message_id_that_could_not_be_one_is_refused_by_shape() {
        for id in [
            "",
            "peer 0011",
            "peer/0011",
            "../../etc/passwd",
            "peer\u{0}0011",
        ] {
            let body = format!(
                "{{\"verb\":\"reveal-view-once\",\"target\":\"message-id\",\"messageId\":{}}}",
                serde_json::to_string(id).expect("encode")
            );
            assert_eq!(refusal(&body), REFUSAL_MESSAGE_ID_INVALID, "id {id:?}");
        }
        let long = "a".repeat(MAX_MESSAGE_ID_BYTES + 1);
        let body = format!(
            "{{\"verb\":\"reveal-view-once\",\"target\":\"message-id\",\"messageId\":\"{long}\"}}"
        );
        assert_eq!(refusal(&body), REFUSAL_MESSAGE_ID_INVALID);
    }

    #[test]
    fn send_grades_every_readiness_criterion_and_status_grades_none() {
        for id in READINESS_CRITERIA {
            assert!(readiness_criterion_is_graded(Verb::Send, id), "{id}");
            assert!(!readiness_criterion_is_graded(Verb::Status, id), "{id}");
            assert!(!readiness_criterion_is_graded(Verb::Host, id), "{id}");
        }
    }

    #[test]
    fn receive_side_verbs_are_never_graded_on_where_keystrokes_would_land() {
        // A drain sends no keystrokes, so the composer being on screen and
        // stacked above Discord is not a claim it makes. Grading these would
        // make a receiving instance -- which by definition nobody is typing
        // on -- permanently unable to pass a drain.
        for verb in [Verb::Drain, Verb::Rehydrate, Verb::RevealViewOnce] {
            for id in [
                "protection_engaged",
                "composer_window_visible",
                "composer_above_discord",
            ] {
                assert!(
                    !readiness_criterion_is_graded(verb, id),
                    "{} graded {id}",
                    verb.label()
                );
            }
            // But the four that decide whether a drain can name one
            // conversation at all are always graded.
            for id in [
                "identity_unlocked",
                "discord_window_adopted",
                "composer_window_exists",
                "overlay_context_valid",
            ] {
                assert!(
                    readiness_criterion_is_graded(verb, id),
                    "{} did not grade {id}",
                    verb.label()
                );
            }
        }
    }

    #[test]
    fn no_verb_grades_a_criterion_the_driver_does_not_report() {
        // A grading table that names an id the driver never inserts is a
        // criterion that silently never applies.
        for verb in [
            Verb::Status,
            Verb::ListBrowserProfiles,
            Verb::GrantBrowserProfile,
            Verb::RevokeBrowserProfile,
            Verb::RunBrowserImport,
            Verb::Host,
            Verb::Send,
            Verb::Drain,
            Verb::Rehydrate,
            Verb::RevealViewOnce,
        ] {
            for id in ["invented_criterion", "composer_above_discord_typo"] {
                assert!(
                    !readiness_criterion_is_graded(verb, id) || verb == Verb::Send,
                    "{} graded unknown {id}",
                    verb.label()
                );
            }
        }
    }

    #[test]
    fn every_non_ready_outcome_has_a_fixed_named_refusal() {
        for verb in [
            Verb::ListBrowserProfiles,
            Verb::GrantBrowserProfile,
            Verb::RevokeBrowserProfile,
            Verb::RunBrowserImport,
            Verb::Host,
            Verb::Send,
            Verb::Drain,
            Verb::Rehydrate,
            Verb::RevealViewOnce,
        ] {
            let refusal = not_ready_refusal(verb);
            assert!(!refusal.is_empty(), "{}", verb.label());
            assert!(
                refusal.contains(verb.label().split('-').next().expect("verb label")),
                "{} -> {refusal}",
                verb.label()
            );
        }
        assert_eq!(
            not_ready_refusal(Verb::Drain),
            "drain-readiness-precondition-missing"
        );
    }

    #[test]
    fn instance_file_tokens_stay_path_safe_and_stay_distinct() {
        assert_eq!(
            instance_file_token("org.oslprivacy.hubqab"),
            "org.oslprivacy.hubqab"
        );
        let hostile = instance_file_token("../../evil\\path:name");
        assert!(
            hostile
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')),
            "{hostile}"
        );
        assert!(!hostile.contains('/') && !hostile.contains('\\') && !hostile.contains(':'));
        assert_eq!(instance_file_token(""), "unnamed");

        // Two identifiers that agree for the first 96 bytes must still land on
        // two different files, or the addressed trigger stops addressing.
        let long_a = format!("org.oslprivacy.{}a", "x".repeat(200));
        let long_b = format!("org.oslprivacy.{}b", "x".repeat(200));
        assert_ne!(instance_file_token(&long_a), instance_file_token(&long_b));
        assert!(instance_file_token(&long_a).len() <= 96);
    }

    fn opened(cover: Option<&str>, view_once_consumed: bool) -> OpenedNativeOverlayText {
        OpenedNativeOverlayText {
            message_id: "peer-00001111222233334444555566667777".to_owned(),
            cover_pointer: cover.map(str::to_owned),
            plaintext: "qa secret".to_owned(),
            context_verified: true,
            person_to_person_e2ee: true,
            view_once_consumed,
            expires_at: 1_900_000_000,
        }
    }

    fn batch(
        messages: Vec<OpenedNativeOverlayText>,
        acknowledgments: Vec<NativeOverlayAcknowledgmentStatus>,
    ) -> OpenedNativeOverlayTextBatch {
        OpenedNativeOverlayTextBatch {
            fetched: u32::try_from(messages.len()).expect("fits"),
            messages,
            pending_view_once: Vec::new(),
            acknowledgments: acknowledgments
                .into_iter()
                .map(|status| NativeOverlayAcknowledgment {
                    message_id: "peer-00001111222233334444555566667777".to_owned(),
                    status,
                    acknowledged_at: 1_700_000_000,
                })
                .collect(),
            decrypt_display_enabled: true,
            deferred_rows: 0,
        }
    }

    #[test]
    fn a_drain_report_separates_received_from_opened_and_keeps_their_order() {
        // P3b. `acknowledgmentCount` alone answers 2 to both of these.
        let received_first = DrainReport::from_batch(&batch(
            Vec::new(),
            vec![
                NativeOverlayAcknowledgmentStatus::Received,
                NativeOverlayAcknowledgmentStatus::Opened,
            ],
        ));
        let opened_first = DrainReport::from_batch(&batch(
            Vec::new(),
            vec![
                NativeOverlayAcknowledgmentStatus::Opened,
                NativeOverlayAcknowledgmentStatus::Received,
            ],
        ));
        assert_eq!(received_first.acknowledgment_count, 2);
        assert_eq!(opened_first.acknowledgment_count, 2);
        assert_eq!(received_first.acknowledgment_received_count, 1);
        assert_eq!(received_first.acknowledgment_opened_count, 1);
        assert_eq!(
            received_first.acknowledgment_kind_order,
            ["received", "opened"]
        );
        assert_eq!(
            opened_first.acknowledgment_kind_order,
            ["opened", "received"]
        );
        assert_ne!(
            received_first.acknowledgment_kind_order,
            opened_first.acknowledgment_kind_order
        );
    }

    #[test]
    fn p3a_acknowledgement_roundtrip_a_from_b() {
        // P3a: after A sends a protected message, B's receive acknowledgement
        // returns to A through the same drain report path the QA driver writes.
        let request = accepted("{\"verb\":\"drain\",\"instance\":\"org.oslprivacy.hubqa-a\"}");
        assert_eq!(request.verb, Verb::Drain);
        assert!(readiness_criterion_is_graded(
            request.verb,
            "overlay_context_valid"
        ));

        let mut returned_to_a = batch(
            Vec::new(),
            vec![NativeOverlayAcknowledgmentStatus::Received],
        );
        returned_to_a.fetched = 1;

        let report = DrainReport::from_batch(&returned_to_a);
        assert_eq!(report.opened_count, 0);
        assert_eq!(report.pending_view_once_count, 0);
        assert_eq!(report.acknowledgment_count, 1);
        assert_eq!(report.acknowledgment_received_count, 1);
        assert_eq!(report.acknowledgment_opened_count, 0);
        assert_eq!(report.acknowledgment_kind_order, ["received"]);
        assert_eq!(report.fetched, 1);
        assert_eq!(report.deferred_rows, 0);
        assert!(report.decrypt_display_enabled);

        let encoded = serde_json::to_value(&report).expect("encode");
        assert_eq!(encoded["acknowledgmentCount"], 1);
        assert_eq!(encoded["acknowledgmentReceivedCount"], 1);
        assert_eq!(encoded["acknowledgmentOpenedCount"], 0);
        assert_eq!(
            encoded["acknowledgmentKindOrder"],
            serde_json::json!(["received"])
        );
        assert!(
            !encoded
                .to_string()
                .contains("peer-00001111222233334444555566667777"),
            "{encoded}"
        );
    }

    #[test]
    fn a_drain_report_counts_the_rows_the_eye_could_paint_in_place() {
        let report = DrainReport::from_batch(&batch(
            vec![
                opened(Some("qa cover prose"), false),
                opened(None, true),
                opened(Some("more cover prose"), false),
            ],
            Vec::new(),
        ));
        assert_eq!(report.opened_count, 3);
        assert_eq!(report.cover_pointer_count, 2);
        assert_eq!(report.view_once_consumed_count, 1);
        assert_eq!(report.context_verified_count, 3);
        assert_eq!(report.person_to_person_e2ee_count, 3);
    }

    #[test]
    fn no_drain_report_field_can_carry_content() {
        let encoded = serde_json::to_string(&DrainReport::from_batch(&batch(
            vec![opened(Some("qa cover prose"), false)],
            vec![NativeOverlayAcknowledgmentStatus::Received],
        )))
        .expect("encode");
        assert!(!encoded.contains("qa secret"), "{encoded}");
        assert!(!encoded.contains("qa cover prose"), "{encoded}");
        assert!(!encoded.contains("peer-0000"), "{encoded}");
    }

    #[test]
    fn a_host_report_serializes_only_the_resolved_borrow_and_after_facts() {
        let report = HostReport::after(
            HostAction::BorrowOnly {
                app_id: NativeAppId::Discord,
                session_mode: DiscordSessionMode::ExistingSession,
            },
            true,
            true,
            true,
            false,
        );
        let encoded = serde_json::to_value(report).expect("encode");
        assert_eq!(encoded["adopted"], true);
        assert_eq!(encoded["appId"], "discord");
        assert_eq!(encoded["sessionMode"], "existingSession");
        assert_eq!(encoded["discordWindowAdopted"], true);
        assert_eq!(encoded["overlayContextValid"], true);
        assert_eq!(encoded["protectionEngaged"], false);
        assert_eq!(encoded.as_object().expect("object").len(), 6);
    }

    #[test]
    fn a_browser_import_report_serializes_sorted_distinct_source_profiles() {
        let report = BrowserImportReport::from_source_profiles([
            "Work Profile",
            "Default",
            "Work Profile",
            "QA_Profile",
        ]);
        assert_eq!(
            report.source_profiles,
            ["Default", "QA_Profile", "Work Profile"]
        );
        let encoded = serde_json::to_value(report).expect("encode");
        assert_eq!(
            encoded["sourceProfiles"],
            serde_json::json!(["Default", "QA_Profile", "Work Profile"])
        );
        assert_eq!(encoded.as_object().expect("object").len(), 1);
    }

    #[test]
    fn challenge_stop_restart_matrix_across_imap_and_hosted_scan() {
        let matrix = challenge_stop_restart_matrix();
        assert_eq!(matrix.len(), 6);

        let expected = [
            (
                "f4-imap",
                "challenge",
                "reviewed-imap-item",
                "operator-challenge-required",
            ),
            (
                "f4-imap",
                "stop",
                "reviewed-imap-item",
                "operator-stop-revoked-authority",
            ),
            (
                "f4-imap",
                "restart",
                "reviewed-imap-item",
                "restart-requires-fresh-authority",
            ),
            (
                "f5-hosted-scan",
                "challenge",
                "scan-only-hosted-port",
                "operator-challenge-required",
            ),
            (
                "f5-hosted-scan",
                "stop",
                "scan-only-hosted-port",
                "operator-stop-revoked-authority",
            ),
            (
                "f5-hosted-scan",
                "restart",
                "scan-only-hosted-port",
                "restart-requires-fresh-authority",
            ),
        ];

        let actual_cells = matrix
            .iter()
            .map(|decision| {
                (
                    decision.lane,
                    decision.interruption,
                    decision.authority_scope,
                    decision.refusal,
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let expected_cells = expected
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(actual_cells, expected_cells);

        for (decision, (lane, interruption, authority_scope, refusal)) in
            matrix.iter().zip(expected)
        {
            assert_eq!(decision.lane, lane);
            assert_eq!(decision.interruption, interruption);
            assert_eq!(decision.authority_scope, authority_scope);
            assert_eq!(decision.refusal, refusal);
            assert!(!decision.may_continue, "{lane} {interruption}");
            assert!(!decision.may_delete, "{lane} {interruption}");
        }

        assert_eq!(
            challenge_stop_restart_decision(
                AutomationQaLane::F4Imap,
                AutomationQaInterruption::Restart
            ),
            AutomationQaInterruptionDecision {
                lane: "f4-imap",
                interruption: "restart",
                authority_scope: "reviewed-imap-item",
                may_continue: false,
                may_delete: false,
                refusal: "restart-requires-fresh-authority",
            }
        );

        let encoded = serde_json::to_string(&matrix).expect("encode");
        for forbidden in ["account-", "secret", "credential", "profile", "handle"] {
            assert!(!encoded.contains(forbidden), "{encoded}");
        }
    }

    #[test]
    fn a_rehydrate_report_separates_placed_unplaceable_and_opaque_rows() {
        let report = RehydrateReport::tally(
            true,
            0,
            &[(true, true), (true, false), (false, false), (true, true)],
        );
        assert_eq!(report.row_count, 4);
        assert_eq!(report.decoded_row_count, 3);
        assert_eq!(report.placed_row_count, 2);
        assert_eq!(report.unplaceable_row_count, 1);
        assert_eq!(report.opaque_row_count, 1);
        assert_eq!(
            report.placed_row_count + report.unplaceable_row_count,
            report.decoded_row_count
        );
    }

    #[test]
    fn a_refused_rehydration_is_not_an_empty_conversation() {
        let refused = RehydrateReport::tally(false, 1_200, &[]);
        assert!(!refused.read);
        assert_eq!(refused.retry_after_ms, 1_200);
        let empty = RehydrateReport::tally(true, 0, &[]);
        assert!(empty.read);
        assert_ne!(refused, empty);
    }

    #[test]
    fn a_reveal_report_records_a_refusal_as_a_written_fact() {
        // P4c: the second reveal of an already-consumed message. Today this
        // exists only as a string returned to the renderer.
        let mut report = RevealReport::not_driven(RevealTarget::Last, 0);
        report.phase_one_driven = true;
        report.selected = true;
        report.record_phase_two(Err(()));
        assert!(report.phase_two_driven);
        assert!(report.refused);
        assert!(!report.revealed);
        assert!(!report.view_once_consumed);
        assert_eq!(report.target, "last");
    }

    #[test]
    fn a_successful_reveal_reports_the_consumption_that_makes_it_view_once() {
        let mut report = RevealReport::not_driven(RevealTarget::PendingIndex, 0);
        report.phase_one_driven = true;
        report.phase_one_pending_count = 1;
        report.selected = true;
        report.record_phase_two(Ok(&opened(Some("qa cover prose"), true)));
        assert!(report.revealed);
        assert!(!report.refused);
        assert!(report.view_once_consumed);
        assert!(report.has_cover_pointer);
        let encoded = serde_json::to_string(&report).expect("encode");
        assert!(!encoded.contains("qa secret"), "{encoded}");
        assert!(!encoded.contains("qa cover prose"), "{encoded}");
    }

    #[test]
    fn a_reveal_that_was_never_driven_claims_nothing() {
        let report = RevealReport::not_driven(RevealTarget::PendingIndex, 3);
        assert!(!report.phase_one_driven);
        assert!(!report.phase_two_driven);
        assert!(!report.revealed);
        assert!(!report.refused);
        assert_eq!(report.requested_index, 3);
    }

    #[test]
    fn pending_view_once_entries_are_counted_but_never_quoted() {
        let mut listed = batch(Vec::new(), Vec::new());
        listed.pending_view_once = vec![PendingNativeOverlayText {
            message_id: "peer-00001111222233334444555566667777".to_owned(),
            expires_at: 1_900_000_000,
            person_to_person_e2ee: true,
        }];
        let report = DrainReport::from_batch(&listed);
        assert_eq!(report.pending_view_once_count, 1);
        assert_eq!(report.opened_count, 0);
        let encoded = serde_json::to_string(&report).expect("encode");
        assert!(!encoded.contains("peer-0000"), "{encoded}");
    }
}
