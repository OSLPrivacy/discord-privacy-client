//! Story privacy settings, auto-burn, honest screenshot shielding and view
//! receipts.
//!
//! Four settings ship here, and each one is enforced rather than displayed:
//!
//! * **Default audience** — `EVERYONE`, `FRIENDS` or `VERIFIED`. A story with
//!   no per-story `SEND TO` inherits it; a story with `SEND TO` uses only the
//!   overriding audience. The audience is frozen into the story at publish as a
//!   set of wrapped content keys, so a later relationship change cannot widen
//!   or narrow a story that is already out.
//! * **Auto-burn** — `1H`, `12H` or `24H`. The deadline is persisted as an
//!   absolute instant, and every open of the store sweeps it before serving a
//!   single byte. A boundary crossed while the app was closed therefore burns
//!   on the next start, not on the next tick of a timer that was not running.
//!   Burn destroys the content key and the sealed payload: retained ciphertext
//!   recovers nothing afterwards.
//! * **Screenshot shield** — backed by the one primitive Windows actually
//!   supports (`SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE`, read
//!   through [`runtime::screenshot`]). It is always shown with the disclosure
//!   that a camera or an external capture device is outside its reach. Where
//!   the primitive does not exist the control is disabled, the setting cannot
//!   be turned on, and no protection is claimed at all.
//! * **View receipts** — off means *no per-view receipt and no viewer identity
//!   is created or retained, including retroactively*: turning the setting off
//!   destroys everything an earlier on-period recorded. On means exactly one
//!   authorized signal, [`ViewSignal`], carrying a story id and an aggregate
//!   count and nothing else.
//!
//! ## Why no identity is written anywhere
//!
//! The audience is stored as per-story salted slots, never as account ids, so a
//! reader of the sealed store cannot recover who a story went to and cannot
//! join two stories by their recipients. View de-duplication keys on the
//! viewer-generated random open-event token, which is not derived from the
//! viewer, so replay refusal costs no identity either.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

// ---------------------------------------------------------------------------
// Stable identifiers
// ---------------------------------------------------------------------------

pub const AUDIENCE_EVERYONE: &str = "story-audience-everyone";
pub const AUDIENCE_FRIENDS: &str = "story-audience-friends";
pub const AUDIENCE_VERIFIED: &str = "story-audience-verified";

pub const LIFETIME_1H: &str = "story-lifetime-1h";
pub const LIFETIME_12H: &str = "story-lifetime-12h";
pub const LIFETIME_24H: &str = "story-lifetime-24h";

pub const AUDIENCE_SOURCE_DEFAULT: &str = "inherited-default";
pub const AUDIENCE_SOURCE_OVERRIDE: &str = "send-to-override";

/// The only capture-protection primitive this product will stand behind.
pub const SHIELD_PRIMITIVE: &str = "SetWindowDisplayAffinity/WDA_EXCLUDEFROMCAPTURE";

/// Shown wherever the shield is on. It is a disclosure, not a boast: the
/// primitive excludes the window from OS capture and can do nothing at all
/// about a lens.
pub const SHIELD_DISCLOSURE: &str = "Blocks Windows screen capture of this story. It cannot block a camera pointed at your screen, an external capture device, or a photo of your display.";

/// Shown wherever the primitive does not exist. It claims nothing.
pub const SHIELD_UNAVAILABLE_COPY: &str =
    "Screenshot shield is unavailable on this system, so this build makes no capture-protection claim here.";

/// Shown to a viewer before a story opens while receipts are on. Frozen by the
/// counts-only ruling: a count is disclosed, a name is not, and the inference
/// risk in a small audience is disclosed rather than hidden.
pub const VIEW_RECEIPT_VIEWER_COPY: &str = "The poster sees a view count, not your name. In a small or individually targeted audience, they may still infer that you viewed. Repeat opens may increase the count.";

/// Shown to a viewer before a story opens while receipts are off.
pub const VIEW_RECEIPT_OFF_VIEWER_COPY: &str =
    "The poster records nothing when you open this story. No view is counted and no viewer is named.";

const SLOT_DOMAIN: &[u8] = b"osl-story-privacy/audience-slot/v1";
const WRAP_DOMAIN: &[u8] = b"osl-story-privacy/wrap/v1";
const EVENT_DOMAIN: &[u8] = b"osl-story-privacy/open-event/v1";
const STORY_AD: &[u8] = b"osl-story-privacy/story-body/v1";
const LEDGER_AD: &[u8] = b"osl-story-privacy/ledger/v1";

const SETTINGS_FILE: &str = "story_privacy_settings.osl";
const STORIES_FILE: &str = "story_privacy_stories.osl";
const RECEIPTS_FILE: &str = "story_privacy_receipts.osl";
const LOG_FILE: &str = "story_privacy.log";

// ---------------------------------------------------------------------------
// Settings vocabulary
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryAudience {
    Everyone,
    Friends,
    Verified,
}

impl StoryAudience {
    pub const ALL: [StoryAudience; 3] = [
        StoryAudience::Everyone,
        StoryAudience::Friends,
        StoryAudience::Verified,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            StoryAudience::Everyone => AUDIENCE_EVERYONE,
            StoryAudience::Friends => AUDIENCE_FRIENDS,
            StoryAudience::Verified => AUDIENCE_VERIFIED,
        }
    }

    pub fn from_stable_id(value: &str) -> Option<StoryAudience> {
        match value {
            AUDIENCE_EVERYONE => Some(StoryAudience::Everyone),
            AUDIENCE_FRIENDS => Some(StoryAudience::Friends),
            AUDIENCE_VERIFIED => Some(StoryAudience::Verified),
            _ => None,
        }
    }
}

/// Every surface reads the stable id, never a discriminant, so a reordered
/// enum cannot silently rename an audience on the wire.
impl Serialize for StoryAudience {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.stable_id())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoryLifetime {
    OneHour,
    TwelveHours,
    TwentyFourHours,
}

impl StoryLifetime {
    pub const ALL: [StoryLifetime; 3] = [
        StoryLifetime::OneHour,
        StoryLifetime::TwelveHours,
        StoryLifetime::TwentyFourHours,
    ];

    pub const fn stable_id(self) -> &'static str {
        match self {
            StoryLifetime::OneHour => LIFETIME_1H,
            StoryLifetime::TwelveHours => LIFETIME_12H,
            StoryLifetime::TwentyFourHours => LIFETIME_24H,
        }
    }

    /// The label the settings screen and the composer both print.
    pub const fn label(self) -> &'static str {
        match self {
            StoryLifetime::OneHour => "1H",
            StoryLifetime::TwelveHours => "12H",
            StoryLifetime::TwentyFourHours => "24H",
        }
    }

    pub const fn seconds(self) -> i64 {
        match self {
            StoryLifetime::OneHour => 3_600,
            StoryLifetime::TwelveHours => 43_200,
            StoryLifetime::TwentyFourHours => 86_400,
        }
    }

    pub fn from_stable_id(value: &str) -> Option<StoryLifetime> {
        match value {
            LIFETIME_1H => Some(StoryLifetime::OneHour),
            LIFETIME_12H => Some(StoryLifetime::TwelveHours),
            LIFETIME_24H => Some(StoryLifetime::TwentyFourHours),
            _ => None,
        }
    }
}

impl Serialize for StoryLifetime {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.stable_id())
    }
}

// ---------------------------------------------------------------------------
// Screenshot shield — availability decides whether anything may be claimed
// ---------------------------------------------------------------------------

/// What the shield row is allowed to say and do on this machine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ShieldState {
    /// Whether a supported OS capture-protection primitive exists here.
    pub supported: bool,
    /// Named only when one exists. `None` is the honest answer elsewhere.
    pub primitive: Option<&'static str>,
    /// Whether the user may operate the control at all.
    pub control_enabled: bool,
    /// The stored setting value after availability is applied.
    pub setting_on: bool,
    /// Whether any protection is being claimed to the user. Never true without
    /// a primitive.
    pub claims_protection: bool,
    /// The camera/external-capture disclosure, present whenever a claim is.
    pub disclosure: Option<&'static str>,
    /// The no-claim copy, present whenever the primitive is missing.
    pub unavailable_copy: Option<&'static str>,
}

/// The shield state for this build, asking the platform rather than assuming.
pub fn shield_state(requested_on: bool) -> ShieldState {
    shield_state_for(
        runtime::screenshot::capture_protection_is_enforced(),
        requested_on,
    )
}

/// The same decision with availability supplied, so both branches are testable
/// on one machine.
pub fn shield_state_for(supported: bool, requested_on: bool) -> ShieldState {
    if !supported {
        // Fail closed and stay quiet: no primitive, no setting, no claim.
        return ShieldState {
            supported: false,
            primitive: None,
            control_enabled: false,
            setting_on: false,
            claims_protection: false,
            disclosure: None,
            unavailable_copy: Some(SHIELD_UNAVAILABLE_COPY),
        };
    }
    ShieldState {
        supported: true,
        primitive: Some(SHIELD_PRIMITIVE),
        control_enabled: true,
        setting_on: requested_on,
        claims_protection: requested_on,
        disclosure: if requested_on {
            Some(SHIELD_DISCLOSURE)
        } else {
            None
        },
        unavailable_copy: None,
    }
}

/// The result of asking the OS to protect one real window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ShieldApplication {
    pub requested_on: bool,
    pub primitive: Option<&'static str>,
    /// True only when the OS accepted *and* the affinity read back as the
    /// exact exclude-from-capture value.
    pub enforced: bool,
    pub claims_protection: bool,
    pub disclosure: Option<&'static str>,
    pub unavailable_copy: Option<&'static str>,
    pub error: Option<String>,
}

/// Apply the shield to a real top-level window through the production
/// primitive. A refusal is reported as a refusal; it never becomes a claim.
pub fn apply_shield_to_window(hwnd: isize, on: bool) -> ShieldApplication {
    if !runtime::screenshot::capture_protection_is_enforced() {
        return ShieldApplication {
            requested_on: on,
            primitive: None,
            enforced: false,
            claims_protection: false,
            disclosure: None,
            unavailable_copy: Some(SHIELD_UNAVAILABLE_COPY),
            error: Some("no supported capture-protection primitive on this platform".to_owned()),
        };
    }
    let protection = if on {
        runtime::screenshot::ScreenshotProtection::On
    } else {
        runtime::screenshot::ScreenshotProtection::Off
    };
    match runtime::screenshot::apply_to_hwnd(hwnd, protection) {
        Ok(()) => ShieldApplication {
            requested_on: on,
            primitive: Some(SHIELD_PRIMITIVE),
            enforced: on,
            claims_protection: on,
            disclosure: if on { Some(SHIELD_DISCLOSURE) } else { None },
            unavailable_copy: None,
            error: None,
        },
        Err(error) => ShieldApplication {
            requested_on: on,
            primitive: Some(SHIELD_PRIMITIVE),
            enforced: false,
            claims_protection: false,
            disclosure: None,
            unavailable_copy: Some(SHIELD_UNAVAILABLE_COPY),
            error: Some(error.to_string()),
        },
    }
}

// ---------------------------------------------------------------------------
// People and audiences
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Relationship {
    pub friend: bool,
    pub verified: bool,
}

/// The author's own view of who exists, used only at publish time.
#[derive(Clone, Debug, Default)]
pub struct Directory {
    people: BTreeMap<String, Relationship>,
}

impl Directory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, person: &str, relationship: Relationship) {
        self.people.insert(person.to_owned(), relationship);
    }

    pub fn members_for(&self, audience: StoryAudience) -> BTreeSet<String> {
        self.people
            .iter()
            .filter(|(_, relationship)| match audience {
                StoryAudience::Everyone => true,
                StoryAudience::Friends => relationship.friend,
                StoryAudience::Verified => relationship.verified,
            })
            .map(|(person, _)| person.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.people.len()
    }

    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Persisted records
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WrappedKey {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoryRecord {
    id: String,
    author: String,
    created_at_ms: i64,
    expires_at_ms: i64,
    lifetime: String,
    audience: String,
    audience_source: String,
    /// Per-story random salt. Every recipient slot is salted with it, so the
    /// same person is a different slot in every story.
    salt: Vec<u8>,
    /// How many recipients the frozen audience had. A count, never a list.
    audience_size: usize,
    /// hex(slot commitment) -> the content key wrapped for that recipient.
    slots: BTreeMap<String, WrappedKey>,
    shield_on_at_publish: bool,
    receipts_on_at_publish: bool,
    body_nonce: Option<Vec<u8>>,
    body_ciphertext: Option<Vec<u8>>,
    body_digest: Option<String>,
    burned_at_ms: Option<i64>,
}

impl StoryRecord {
    fn is_burned(&self) -> bool {
        self.burned_at_ms.is_some()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoriesLedger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    stories: BTreeMap<String, StoryRecord>,
}

/// The only thing an on-period ever writes: an aggregate and the open-event
/// tokens already counted, so a replay cannot inflate it.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReceiptRow {
    view_count: u32,
    counted_events: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReceiptsLedger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    rows: BTreeMap<String, ReceiptRow>,
}

impl ReceiptsLedger {
    fn record_count(&self) -> usize {
        self.rows
            .values()
            .map(|row| 1 + row.counted_events.len())
            .sum()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SettingsRecord {
    #[serde(default)]
    version: u32,
    audience: String,
    lifetime: String,
    screenshot_shield: bool,
    view_receipts: bool,
}

impl Default for SettingsRecord {
    fn default() -> Self {
        SettingsRecord {
            version: 1,
            audience: AUDIENCE_FRIENDS.to_owned(),
            lifetime: LIFETIME_24H.to_owned(),
            screenshot_shield: false,
            view_receipts: false,
        }
    }
}

/// The four settings as the surfaces read them.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StoryDefaults {
    pub audience: StoryAudience,
    pub lifetime: StoryLifetime,
    pub shield: ShieldState,
    pub view_receipts: bool,
}

// ---------------------------------------------------------------------------
// Outcomes
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishedStory {
    pub id: String,
    pub audience: String,
    pub audience_source: String,
    pub lifetime: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
    pub audience_size: usize,
    pub receipts_on_at_publish: bool,
    pub shield_on_at_publish: bool,
    pub body_digest: String,
}

/// The whole authorized poster-side view signal. Two fields, by design.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ViewSignal {
    pub story_id: String,
    pub view_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum ViewOutcome {
    /// Receipts are off: nothing was created anywhere.
    NoSignalRecorded,
    /// Receipts are on and this open-event token was new.
    Counted(ViewSignal),
    /// Receipts are on and this exact open-event token was already counted.
    ReplayIgnored(ViewSignal),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct BurnReport {
    pub burned: Vec<String>,
    pub already_burned: usize,
    pub live: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ReceiptErasure {
    pub rows_destroyed: usize,
    pub log_lines_destroyed: usize,
    pub records_destroyed: usize,
}

// ---------------------------------------------------------------------------
// The client
// ---------------------------------------------------------------------------

/// One profile's story-privacy state, durable across restarts.
pub struct StoryPrivacyClient {
    dir: PathBuf,
    key: [u8; 32],
    settings: SettingsRecord,
    stories: StoriesLedger,
    receipts: ReceiptsLedger,
    shield_supported: bool,
}

impl StoryPrivacyClient {
    /// Open (or create) a profile's story state and sweep every burn boundary
    /// that passed while it was closed **before** anything can be read.
    pub fn open(dir: &Path, key: [u8; 32], now_ms: i64) -> Result<(Self, BurnReport), String> {
        Self::open_with_shield_support(
            dir,
            key,
            now_ms,
            runtime::screenshot::capture_protection_is_enforced(),
        )
    }

    /// The same, with capture-protection availability supplied so the
    /// unsupported branch is reachable on a machine that supports it and the
    /// supported branch is reachable on one that does not.
    pub fn open_with_shield_support(
        dir: &Path,
        key: [u8; 32],
        now_ms: i64,
        shield_supported: bool,
    ) -> Result<(Self, BurnReport), String> {
        fs::create_dir_all(dir).map_err(|error| format!("story store unavailable: {error}"))?;
        let settings: SettingsRecord = match read_sealed(&dir.join(SETTINGS_FILE), &key)? {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "story settings are malformed".to_owned())?,
            None => SettingsRecord::default(),
        };
        let stories: StoriesLedger = match read_sealed(&dir.join(STORIES_FILE), &key)? {
            Some(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| "story ledger is malformed".to_owned())?
            }
            None => StoriesLedger::default(),
        };
        let receipts: ReceiptsLedger = match read_sealed(&dir.join(RECEIPTS_FILE), &key)? {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "story receipt ledger is malformed".to_owned())?,
            None => ReceiptsLedger::default(),
        };
        let mut client = StoryPrivacyClient {
            dir: dir.to_path_buf(),
            key,
            settings,
            stories,
            receipts,
            shield_supported,
        };
        // An unsupported platform can never carry a stored "on": a profile
        // synced from Windows must not light up a claim on Linux.
        if !client.shield_supported && client.settings.screenshot_shield {
            client.settings.screenshot_shield = false;
            client.persist_settings()?;
        }
        // Receipts off must also be true of everything an earlier on-period
        // left behind, including across a restart.
        if !client.settings.view_receipts {
            if !client.receipts.rows.is_empty() {
                client.receipts.rows.clear();
                client.persist_receipts()?;
            }
            client.purge_view_signal_lines(None)?;
        }
        let report = client.sweep(now_ms)?;
        Ok((client, report))
    }

    pub fn defaults(&self) -> StoryDefaults {
        StoryDefaults {
            audience: StoryAudience::from_stable_id(&self.settings.audience)
                .unwrap_or(StoryAudience::Friends),
            lifetime: StoryLifetime::from_stable_id(&self.settings.lifetime)
                .unwrap_or(StoryLifetime::TwentyFourHours),
            shield: shield_state_for(self.shield_supported, self.settings.screenshot_shield),
            view_receipts: self.settings.view_receipts,
        }
    }

    pub fn set_default_audience(&mut self, audience: StoryAudience) -> Result<(), String> {
        self.settings.audience = audience.stable_id().to_owned();
        self.persist_settings()?;
        self.log(&format!(
            "{{\"event\":\"story-default-audience\",\"audience\":\"{}\"}}",
            audience.stable_id()
        ))
    }

    pub fn set_default_lifetime(&mut self, lifetime: StoryLifetime) -> Result<(), String> {
        self.settings.lifetime = lifetime.stable_id().to_owned();
        self.persist_settings()?;
        self.log(&format!(
            "{{\"event\":\"story-default-lifetime\",\"lifetime\":\"{}\"}}",
            lifetime.stable_id()
        ))
    }

    /// Turn the shield on or off. Refused outright where no primitive exists,
    /// so the stored value can never disagree with what the OS can do.
    pub fn set_screenshot_shield(&mut self, on: bool) -> Result<ShieldState, String> {
        if on && !self.shield_supported {
            return Err(SHIELD_UNAVAILABLE_COPY.to_owned());
        }
        self.settings.screenshot_shield = on && self.shield_supported;
        self.persist_settings()?;
        let state = shield_state_for(self.shield_supported, self.settings.screenshot_shield);
        self.log(&format!(
            "{{\"event\":\"story-screenshot-shield\",\"on\":{},\"claims_protection\":{}}}",
            state.setting_on, state.claims_protection
        ))?;
        Ok(state)
    }

    /// Turn view receipts on or off. Turning them off erases every per-view
    /// record an earlier on-period wrote — that is what "including
    /// retroactively" has to mean if it means anything.
    pub fn set_view_receipts(&mut self, on: bool) -> Result<ReceiptErasure, String> {
        self.settings.view_receipts = on;
        let mut erasure = ReceiptErasure::default();
        if !on {
            erasure.rows_destroyed = self.receipts.rows.len();
            erasure.records_destroyed = self.receipts.record_count();
            self.receipts.rows.clear();
            self.persist_receipts()?;
            // The log is a third observer, and a signal line left behind there
            // is still a retained per-view record.
            erasure.log_lines_destroyed = self.purge_view_signal_lines(None)?;
            erasure.records_destroyed += erasure.log_lines_destroyed;
        }
        self.persist_settings()?;
        self.log(&format!(
            "{{\"event\":\"story-view-receipts\",\"on\":{on},\"rows_destroyed\":{}}}",
            erasure.rows_destroyed
        ))?;
        Ok(erasure)
    }

    /// Publish a real story. `send_to` is the per-story `SEND TO` override;
    /// `None` inherits the settings default.
    pub fn publish_story(
        &mut self,
        id: &str,
        author: &str,
        body: &[u8],
        send_to: Option<StoryAudience>,
        directory: &Directory,
        pairwise: &BTreeMap<String, [u8; 32]>,
        now_ms: i64,
    ) -> Result<PublishedStory, String> {
        if self.stories.stories.contains_key(id) {
            return Err(format!("story {id} already exists"));
        }
        let defaults = self.defaults();
        let audience = send_to.unwrap_or(defaults.audience);
        let audience_source = if send_to.is_some() {
            AUDIENCE_SOURCE_OVERRIDE
        } else {
            AUDIENCE_SOURCE_DEFAULT
        };
        let lifetime = defaults.lifetime;
        let salt = crypto::random::random_bytes(32);
        let mut content_key = [0u8; 32];
        content_key.copy_from_slice(crypto::random::random_bytes(32).as_slice());

        let body_nonce = crypto::random::random_nonce();
        let body_ciphertext = crypto::aead::seal(
            &crypto::aead::Key::from_bytes(content_key),
            &body_nonce,
            STORY_AD,
            body,
        )
        .map_err(|_| "story body could not be sealed".to_owned())?;

        // Freeze the audience now. A relationship changed after this line
        // changes future stories only.
        let members = directory.members_for(audience);
        let mut slots = BTreeMap::new();
        for member in &members {
            let secret = pairwise
                .get(member)
                .ok_or_else(|| format!("no pairwise key for {member}"))?;
            let slot = slot_id(&salt, member);
            let wrap = wrap_key(&salt, secret);
            let nonce = crypto::random::random_nonce();
            let ciphertext = crypto::aead::seal(
                &crypto::aead::Key::from_bytes(wrap),
                &nonce,
                WRAP_DOMAIN,
                &content_key,
            )
            .map_err(|_| "story key could not be wrapped".to_owned())?;
            slots.insert(
                slot,
                WrappedKey {
                    nonce: nonce.as_bytes().to_vec(),
                    ciphertext,
                },
            );
        }
        content_key.zeroize();

        let expires_at_ms = now_ms
            .checked_add(lifetime.seconds() * 1_000)
            .ok_or_else(|| "story deadline overflows".to_owned())?;
        let digest = hex::encode(Sha256::digest(body));
        let record = StoryRecord {
            id: id.to_owned(),
            author: author.to_owned(),
            created_at_ms: now_ms,
            expires_at_ms,
            lifetime: lifetime.stable_id().to_owned(),
            audience: audience.stable_id().to_owned(),
            audience_source: audience_source.to_owned(),
            salt,
            audience_size: members.len(),
            slots,
            shield_on_at_publish: defaults.shield.setting_on,
            receipts_on_at_publish: defaults.view_receipts,
            body_nonce: Some(body_nonce.as_bytes().to_vec()),
            body_ciphertext: Some(body_ciphertext),
            body_digest: Some(digest.clone()),
            burned_at_ms: None,
        };
        let published = PublishedStory {
            id: record.id.clone(),
            audience: record.audience.clone(),
            audience_source: record.audience_source.clone(),
            lifetime: record.lifetime.clone(),
            created_at_ms: record.created_at_ms,
            expires_at_ms: record.expires_at_ms,
            audience_size: record.audience_size,
            receipts_on_at_publish: record.receipts_on_at_publish,
            shield_on_at_publish: record.shield_on_at_publish,
            body_digest: digest,
        };
        self.stories.stories.insert(id.to_owned(), record);
        self.persist_stories()?;
        self.log(&format!(
            "{{\"event\":\"story-published\",\"story\":\"{}\",\"audience\":\"{}\",\"audience_source\":\"{}\",\"lifetime\":\"{}\",\"expires_at_ms\":{},\"audience_size\":{},\"receipts\":\"{}\"}}",
            published.id,
            published.audience,
            published.audience_source,
            published.lifetime,
            published.expires_at_ms,
            published.audience_size,
            if published.receipts_on_at_publish { "on" } else { "off" }
        ))?;
        Ok(published)
    }

    /// The sealed bytes as they sit on disk, for a retained-ciphertext check.
    pub fn sealed_body(&self, story_id: &str) -> Option<Vec<u8>> {
        self.stories
            .stories
            .get(story_id)?
            .body_ciphertext
            .as_ref()
            .cloned()
    }

    pub fn story_ids(&self) -> Vec<String> {
        self.stories.stories.keys().cloned().collect()
    }

    pub fn is_burned(&self, story_id: &str) -> bool {
        self.stories
            .stories
            .get(story_id)
            .map(StoryRecord::is_burned)
            .unwrap_or(true)
    }

    pub fn published(&self, story_id: &str) -> Option<PublishedStory> {
        let record = self.stories.stories.get(story_id)?;
        Some(PublishedStory {
            id: record.id.clone(),
            audience: record.audience.clone(),
            audience_source: record.audience_source.clone(),
            lifetime: record.lifetime.clone(),
            created_at_ms: record.created_at_ms,
            expires_at_ms: record.expires_at_ms,
            audience_size: record.audience_size,
            receipts_on_at_publish: record.receipts_on_at_publish,
            shield_on_at_publish: record.shield_on_at_publish,
            body_digest: record.body_digest.clone().unwrap_or_default(),
        })
    }

    /// Open a story as `viewer`. Anyone without a slot recovers zero bytes,
    /// and so does everyone once the story has burned.
    pub fn open_story(
        &self,
        story_id: &str,
        viewer: &str,
        pairwise_secret: &[u8; 32],
        now_ms: i64,
    ) -> Result<Vec<u8>, String> {
        let record = self
            .stories
            .stories
            .get(story_id)
            .ok_or_else(|| "story is not available".to_owned())?;
        if record.is_burned() || record.expires_at_ms <= now_ms {
            return Err("story has burned".to_owned());
        }
        let slot = slot_id(&record.salt, viewer);
        let wrapped = record
            .slots
            .get(&slot)
            .ok_or_else(|| "story is not available".to_owned())?;
        let wrap = wrap_key(&record.salt, pairwise_secret);
        let nonce = nonce_from(&wrapped.nonce)?;
        let mut content_key_bytes = crypto::aead::open(
            &crypto::aead::Key::from_bytes(wrap),
            &nonce,
            WRAP_DOMAIN,
            &wrapped.ciphertext,
        )
        .map_err(|_| "story is not available".to_owned())?;
        if content_key_bytes.len() != 32 {
            return Err("story is not available".to_owned());
        }
        let mut content_key = [0u8; 32];
        content_key.copy_from_slice(&content_key_bytes);
        content_key_bytes.zeroize();
        let body_nonce = nonce_from(
            record
                .body_nonce
                .as_ref()
                .ok_or_else(|| "story has burned".to_owned())?,
        )?;
        let ciphertext = record
            .body_ciphertext
            .as_ref()
            .ok_or_else(|| "story has burned".to_owned())?;
        let plain = crypto::aead::open(
            &crypto::aead::Key::from_bytes(content_key),
            &body_nonce,
            STORY_AD,
            ciphertext,
        )
        .map_err(|_| "story is not available".to_owned())?;
        content_key.zeroize();
        Ok(plain)
    }

    /// Whether `viewer` is inside this story's frozen audience.
    pub fn viewer_is_addressed(&self, story_id: &str, viewer: &str) -> bool {
        match self.stories.stories.get(story_id) {
            Some(record) => record.slots.contains_key(&slot_id(&record.salt, viewer)),
            None => false,
        }
    }

    /// Record one open. `open_event` is a random token the viewer's client
    /// mints per open; it is never derived from the viewer.
    pub fn record_view(
        &mut self,
        story_id: &str,
        viewer: &str,
        open_event: &str,
        now_ms: i64,
    ) -> Result<ViewOutcome, String> {
        let record = self
            .stories
            .stories
            .get(story_id)
            .ok_or_else(|| "story is not available".to_owned())?;
        if record.is_burned() || record.expires_at_ms <= now_ms {
            return Err("story has burned".to_owned());
        }
        if !record.slots.contains_key(&slot_id(&record.salt, viewer)) {
            return Err("story is not available".to_owned());
        }
        if !self.settings.view_receipts {
            // Nothing is created: no row, no token, no count, no log line.
            return Ok(ViewOutcome::NoSignalRecorded);
        }
        let token = event_token(open_event);
        let row = self.receipts.rows.entry(story_id.to_owned()).or_default();
        let replay = !row.counted_events.insert(token);
        if !replay {
            row.view_count = row.view_count.saturating_add(1);
        }
        let signal = ViewSignal {
            story_id: story_id.to_owned(),
            view_count: row.view_count,
        };
        self.persist_receipts()?;
        self.log(&format!(
            "{{\"event\":\"story-view-signal\",\"story\":\"{}\",\"signal\":\"aggregate-count\",\"view_count\":{},\"replay\":{replay}}}",
            signal.story_id, signal.view_count
        ))?;
        Ok(if replay {
            ViewOutcome::ReplayIgnored(signal)
        } else {
            ViewOutcome::Counted(signal)
        })
    }

    /// The whole poster-side signal for a story, or `None` when receipts are
    /// off — the poster is told nothing, not told zero.
    pub fn view_signal(&self, story_id: &str) -> Option<ViewSignal> {
        if !self.settings.view_receipts {
            return None;
        }
        self.receipts.rows.get(story_id).map(|row| ViewSignal {
            story_id: story_id.to_owned(),
            view_count: row.view_count,
        })
    }

    /// The copy a viewer sees before opening, which has to match the mode.
    pub fn viewer_pre_open_copy(&self) -> &'static str {
        if self.settings.view_receipts {
            VIEW_RECEIPT_VIEWER_COPY
        } else {
            VIEW_RECEIPT_OFF_VIEWER_COPY
        }
    }

    /// Burn everything whose deadline has passed. Called on every open and
    /// safe to call again at any time.
    pub fn sweep(&mut self, now_ms: i64) -> Result<BurnReport, String> {
        let mut report = BurnReport::default();
        let mut changed = false;
        let mut burned_ids = Vec::new();
        for record in self.stories.stories.values_mut() {
            if record.is_burned() {
                report.already_burned += 1;
                continue;
            }
            if record.expires_at_ms <= now_ms {
                // Key destruction, not a flag: the wrapped keys and the sealed
                // body both go, so retained ciphertext opens to nothing.
                for wrapped in record.slots.values_mut() {
                    wrapped.ciphertext.zeroize();
                    wrapped.nonce.zeroize();
                }
                record.slots.clear();
                if let Some(mut nonce) = record.body_nonce.take() {
                    nonce.zeroize();
                }
                if let Some(mut ciphertext) = record.body_ciphertext.take() {
                    ciphertext.zeroize();
                }
                record.burned_at_ms = Some(now_ms);
                burned_ids.push(record.id.clone());
                changed = true;
            } else {
                report.live += 1;
            }
        }
        for id in &burned_ids {
            // A burned story keeps no view state either, in any observer.
            if self.receipts.rows.remove(id).is_some() {
                self.persist_receipts()?;
            }
            self.purge_view_signal_lines(Some(id))?;
        }
        report.burned = burned_ids;
        if changed {
            self.persist_stories()?;
            for id in &report.burned {
                self.log(&format!(
                    "{{\"event\":\"story-burned\",\"story\":\"{id}\",\"at_ms\":{now_ms}}}"
                ))?;
            }
        }
        Ok(report)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn log_path(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }

    /// Every in-memory per-view record this client holds.
    pub fn client_receipt_records(&self) -> usize {
        self.receipts.record_count()
    }

    /// A serialization of the whole live client state, for an observer that
    /// wants to look for an identity rather than trust an API.
    pub fn client_state_json(&self) -> String {
        serde_json::json!({
            "settings": {
                "audience": self.settings.audience,
                "lifetime": self.settings.lifetime,
                "screenshot_shield": self.settings.screenshot_shield,
                "view_receipts": self.settings.view_receipts,
            },
            "stories": serde_json::to_value(&self.stories).unwrap_or(serde_json::Value::Null),
            "receipts": serde_json::to_value(&self.receipts).unwrap_or(serde_json::Value::Null),
        })
        .to_string()
    }

    fn persist_settings(&self) -> Result<(), String> {
        let body = serde_json::to_vec(&self.settings)
            .map_err(|_| "story settings could not be encoded".to_owned())?;
        write_sealed(&self.dir.join(SETTINGS_FILE), &self.key, &body)
    }

    fn persist_stories(&self) -> Result<(), String> {
        let body = serde_json::to_vec(&self.stories)
            .map_err(|_| "story ledger could not be encoded".to_owned())?;
        write_sealed(&self.dir.join(STORIES_FILE), &self.key, &body)
    }

    fn persist_receipts(&self) -> Result<(), String> {
        let body = serde_json::to_vec(&self.receipts)
            .map_err(|_| "story receipt ledger could not be encoded".to_owned())?;
        write_sealed(&self.dir.join(RECEIPTS_FILE), &self.key, &body)
    }

    /// Rewrite the log without its view-signal lines, for one story or for all
    /// of them. Returns how many lines were destroyed.
    fn purge_view_signal_lines(&self, story_id: Option<&str>) -> Result<usize, String> {
        let path = self.dir.join(LOG_FILE);
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(0);
        };
        let marker = "\"event\":\"story-view-signal\"";
        let story_marker = story_id.map(|id| format!("\"story\":\"{id}\""));
        let mut kept = Vec::new();
        let mut destroyed = 0usize;
        for line in text.lines() {
            let is_signal = line.contains(marker)
                && story_marker
                    .as_deref()
                    .map(|needle| line.contains(needle))
                    .unwrap_or(true);
            if is_signal {
                destroyed += 1;
            } else {
                kept.push(line);
            }
        }
        if destroyed == 0 {
            return Ok(0);
        }
        let mut body = kept.join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        let temporary = path.with_extension("log.tmp");
        fs::write(&temporary, body.as_bytes())
            .map_err(|error| format!("story log unwritable: {error}"))?;
        fs::rename(&temporary, &path).map_err(|error| format!("story log unwritable: {error}"))?;
        Ok(destroyed)
    }

    fn log(&self, line: &str) -> Result<(), String> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join(LOG_FILE))
            .map_err(|error| format!("story log unavailable: {error}"))?;
        writeln!(file, "{line}").map_err(|error| format!("story log unavailable: {error}"))
    }
}

// ---------------------------------------------------------------------------
// Observers
// ---------------------------------------------------------------------------

/// What three independent observers found when they went looking for a viewer.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ViewerRecordObservation {
    /// Per-view records held by the live client.
    pub client_records: usize,
    /// Per-view records recovered from the sealed store *with the key*.
    pub store_records: usize,
    /// Log lines that carry a per-view record.
    pub log_records: usize,
    /// Probe identities found in the live client state.
    pub client_identity_hits: usize,
    /// Probe identities found in the store, raw and decrypted.
    pub store_identity_hits: usize,
    /// Probe identities found in the log.
    pub log_identity_hits: usize,
    pub files_scanned: usize,
    pub store_bytes_scanned: usize,
    pub log_bytes_scanned: usize,
    pub probes: usize,
}

impl ViewerRecordObservation {
    pub fn total_viewer_records(&self) -> usize {
        self.client_records + self.store_records + self.log_records
    }

    pub fn total_identity_hits(&self) -> usize {
        self.client_identity_hits + self.store_identity_hits + self.log_identity_hits
    }
}

/// Look for viewer records in the three places one could survive: the live
/// client, the sealed store on disk (opened with the profile key, because an
/// observer that cannot decrypt would report zero for a store full of them),
/// and the log.
pub fn observe_viewer_records(
    client: &StoryPrivacyClient,
    key: &[u8; 32],
    probe_identities: &[&str],
) -> Result<ViewerRecordObservation, String> {
    let mut observation = ViewerRecordObservation {
        probes: probe_identities.len(),
        ..Default::default()
    };
    let needles: Vec<Vec<u8>> = probe_identities
        .iter()
        .flat_map(|identity| {
            vec![
                identity.as_bytes().to_vec(),
                hex::encode(identity.as_bytes()).into_bytes(),
            ]
        })
        .collect();

    // 1. Client observer.
    observation.client_records = client.client_receipt_records();
    let client_state = client.client_state_json();
    observation.client_identity_hits = count_hits(client_state.as_bytes(), &needles);

    // 2. Store observer: raw bytes, then the decrypted ledger.
    let dir = client.dir();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|error| format!("story store unreadable: {error}"))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    entries.sort();
    for path in &entries {
        if path.file_name().and_then(|name| name.to_str()) == Some(LOG_FILE) {
            continue;
        }
        let bytes = fs::read(path).map_err(|error| format!("story store unreadable: {error}"))?;
        observation.files_scanned += 1;
        observation.store_bytes_scanned += bytes.len();
        observation.store_identity_hits += count_hits(&bytes, &needles);
        if let Some(plain) = read_sealed(path, key)? {
            observation.store_identity_hits += count_hits(&plain, &needles);
            if path.file_name().and_then(|name| name.to_str()) == Some(RECEIPTS_FILE) {
                let ledger: ReceiptsLedger = serde_json::from_slice(&plain)
                    .map_err(|_| "story receipt ledger is malformed".to_owned())?;
                observation.store_records += ledger.record_count();
            }
        }
    }

    // 3. Log observer.
    let log_path = client.log_path();
    if log_path.exists() {
        let bytes =
            fs::read(&log_path).map_err(|error| format!("story log unreadable: {error}"))?;
        observation.files_scanned += 1;
        observation.log_bytes_scanned += bytes.len();
        observation.log_identity_hits += count_hits(&bytes, &needles);
        let text = String::from_utf8_lossy(&bytes);
        observation.log_records = text
            .lines()
            .filter(|line| line.contains("\"event\":\"story-view-signal\""))
            .count();
    }
    Ok(observation)
}

fn count_hits(haystack: &[u8], needles: &[Vec<u8>]) -> usize {
    needles
        .iter()
        .filter(|needle| !needle.is_empty() && contains(haystack, needle))
        .count()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn slot_id(salt: &[u8], member: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(SLOT_DOMAIN);
    hash.update((salt.len() as u64).to_be_bytes());
    hash.update(salt);
    hash.update((member.len() as u64).to_be_bytes());
    hash.update(member.as_bytes());
    hex::encode(hash.finalize())
}

fn wrap_key(salt: &[u8], pairwise_secret: &[u8; 32]) -> [u8; 32] {
    crypto::hkdf::derive_32(salt, pairwise_secret, WRAP_DOMAIN)
        .unwrap_or_else(|_| *pairwise_secret)
}

fn event_token(open_event: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(EVENT_DOMAIN);
    hash.update((open_event.len() as u64).to_be_bytes());
    hash.update(open_event.as_bytes());
    hex::encode(hash.finalize())
}

fn nonce_from(bytes: &[u8]) -> Result<crypto::aead::Nonce, String> {
    if bytes.len() != crypto::aead::NONCE_SIZE {
        return Err("story is not available".to_owned());
    }
    let mut fixed = [0u8; crypto::aead::NONCE_SIZE];
    fixed.copy_from_slice(bytes);
    Ok(crypto::aead::Nonce::from_bytes(fixed))
}

fn read_sealed(path: &Path, key: &[u8; 32]) -> Result<Option<Vec<u8>>, String> {
    let Ok(sealed) = fs::read(path) else {
        return Ok(None);
    };
    if sealed.len() <= crypto::aead::NONCE_SIZE {
        return Ok(None);
    }
    let nonce = nonce_from(&sealed[..crypto::aead::NONCE_SIZE]).ok();
    let Some(nonce) = nonce else {
        return Ok(None);
    };
    match crypto::aead::open(
        &crypto::aead::Key::from_bytes(*key),
        &nonce,
        LEDGER_AD,
        &sealed[crypto::aead::NONCE_SIZE..],
    ) {
        Ok(plain) => Ok(Some(plain)),
        Err(_) => Ok(None),
    }
}

fn write_sealed(path: &Path, key: &[u8; 32], body: &[u8]) -> Result<(), String> {
    let nonce = crypto::random::random_nonce();
    let ciphertext = crypto::aead::seal(
        &crypto::aead::Key::from_bytes(*key),
        &nonce,
        LEDGER_AD,
        body,
    )
    .map_err(|_| "story store could not be sealed".to_owned())?;
    let mut sealed = nonce.as_bytes().to_vec();
    sealed.extend_from_slice(&ciphertext);
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, &sealed).map_err(|error| format!("story store unwritable: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("story store unwritable: {error}"))
}
