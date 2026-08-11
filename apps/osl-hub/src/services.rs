use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::account_identity_authority::AccountServiceIdentityAuthority;
use crate::core_bridge::HubCoreState;
use crate::models::{
    DemoConnectionState, EmailProvider, LinkedAccountDemo, LinkedServiceDemo, ServiceCategory,
    ServiceKind, ServiceLaunchState,
};
use crate::shared_conversation_scroll::{SharedConversationScrollablePlace, SharedPlaceMessage};

const REGISTRY_VERSION: u8 = 3;
const MAX_REGISTRY_BYTES: u64 = 64 * 1024;
const MAX_ACCOUNTS_PER_SERVICE: usize = 10;
const MESSAGING_RISK_AGREEMENT_VERSION: u8 = 1;
const MAX_MESSAGING_RISK_AGREEMENT_BYTES: u64 = 32 * 1024;
const MESSAGING_RISK_AGREEMENT_FILE: &str = "messaging-risk-agreements.json.enc";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccountRecord {
    service_id: ServiceKind,
    id: String,
    label: String,
    /// Absent only on legacy v1/v2 rows. Those rows stay quarantined: silently
    /// assigning a browser profile to whichever OSL identity opens the new
    /// OSL Privacy first would expose that profile under the wrong cryptographic user.
    #[serde(default)]
    owner_osl_user_id: Option<String>,
    #[serde(default)]
    provider: Option<EmailProvider>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryDocument {
    version: u8,
    accounts: Vec<AccountRecord>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingRiskAgreementDocument {
    version: u8,
    agreements: Vec<MessagingRiskAgreementRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessagingRiskAgreementRecord {
    owner_osl_user_id: String,
    service_id: String,
    #[serde(default)]
    account_id: String,
    agreed_at: i64,
    #[serde(default)]
    wording: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessagingRiskAgreement {
    pub service_id: String,
    pub account_id: String,
    pub agreed_at: i64,
    pub wording: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationPlaceKind {
    DirectMessage,
    Group,
    NoteToSelf,
    Channel,
    Thread,
}

impl ConversationPlaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::Group => "group",
            Self::NoteToSelf => "note_to_self",
            Self::Channel => "channel",
            Self::Thread => "thread",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationPlaceParent {
    pub id: String,
    pub label: String,
}

impl ConversationPlaceParent {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConversationPlaceCandidate {
    pub place_id: String,
    pub label: String,
    pub place_kind: ConversationPlaceKind,
    #[serde(default)]
    pub server: Option<ConversationPlaceParent>,
    #[serde(default)]
    pub channel: Option<ConversationPlaceParent>,
}

impl ConversationPlaceCandidate {
    pub fn direct_message(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            place_id: id.into(),
            label: label.into(),
            place_kind: ConversationPlaceKind::DirectMessage,
            server: None,
            channel: None,
        }
    }

    pub fn group(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            place_id: id.into(),
            label: label.into(),
            place_kind: ConversationPlaceKind::Group,
            server: None,
            channel: None,
        }
    }

    pub fn channel(
        id: impl Into<String>,
        label: impl Into<String>,
        server: ConversationPlaceParent,
    ) -> Self {
        Self {
            place_id: id.into(),
            label: label.into(),
            place_kind: ConversationPlaceKind::Channel,
            server: Some(server),
            channel: None,
        }
    }

    pub fn thread(
        id: impl Into<String>,
        label: impl Into<String>,
        server: Option<ConversationPlaceParent>,
        channel: ConversationPlaceParent,
    ) -> Self {
        Self {
            place_id: id.into(),
            label: label.into(),
            place_kind: ConversationPlaceKind::Thread,
            server,
            channel: Some(channel),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedConversationPlace {
    pub service_id: String,
    pub account_id: String,
    pub place_id: String,
    pub label: String,
    pub place_kind: ConversationPlaceKind,
    #[serde(default)]
    pub server: Option<ConversationPlaceParent>,
    #[serde(default)]
    pub channel: Option<ConversationPlaceParent>,
}

/// The conversation kinds that the Telegram Desktop reader may expose.
/// Keeping a channel distinct prevents a later Scrub action from treating a
/// broadcast surface as a group conversation.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelegramDesktopPlaceKind {
    DirectChat,
    Group,
    Channel,
}

/// One openable Telegram Desktop conversation. This boundary intentionally
/// contains only display metadata, never session or credential material.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramDesktopPlace {
    pub place_id: String,
    pub label: String,
    pub kind: TelegramDesktopPlaceKind,
}

impl TelegramDesktopPlace {
    pub fn new(
        place_id: impl Into<String>,
        label: impl Into<String>,
        kind: TelegramDesktopPlaceKind,
    ) -> Self {
        Self {
            place_id: place_id.into(),
            label: label.into(),
            kind,
        }
    }
}

/// One Telegram message observed in an openable conversation. `yours` is the
/// provider-observed authorship bit; Scrub never infers it from message text.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramDesktopMessage {
    pub place_id: String,
    pub message_id: String,
    pub text: String,
    pub time: i64,
    pub yours: bool,
}

impl TelegramDesktopMessage {
    pub fn new(
        place_id: impl Into<String>,
        message_id: impl Into<String>,
        text: impl Into<String>,
        time: i64,
        yours: bool,
    ) -> Self {
        Self {
            place_id: place_id.into(),
            message_id: message_id.into(),
            text: text.into(),
            time,
            yours,
        }
    }
}

/// The read-only Telegram Desktop observations made available to Scrub.
#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramDesktopMachine {
    pub places: Vec<TelegramDesktopPlace>,
    pub messages: Vec<TelegramDesktopMessage>,
}

impl TelegramDesktopMachine {
    pub fn new(places: impl IntoIterator<Item = TelegramDesktopPlace>) -> Self {
        Self {
            places: places.into_iter().collect(),
            messages: Vec::new(),
        }
    }

    pub fn with_messages(
        mut self,
        messages: impl IntoIterator<Item = TelegramDesktopMessage>,
    ) -> Self {
        self.messages = messages.into_iter().collect();
        self
    }

    fn chat_page_place_from_messages(
        &self,
        place_id: &str,
        page_size: usize,
        messages: Vec<TelegramDesktopMessage>,
    ) -> Result<TelegramDesktopChatPagePlace, String> {
        let place = self
            .places
            .iter()
            .find(|place| place.place_id == place_id)
            .cloned()
            .ok_or_else(|| "Telegram conversation place not found".to_owned())?;
        Ok(TelegramDesktopChatPagePlace {
            place,
            messages,
            current_page: 0,
            page_size,
            one_screen_scrolls: 0,
            stop_when_reading_page: None,
            stop_request_callback: None,
        })
    }
}

/// Create a consent-gated, one-screen Telegram Scrub viewport. Provider rows
/// are read through the selected account's risk agreement before this function
/// makes its first page available; scrolling only advances that local copy.
pub fn telegram_desktop_chat_page_place_for_scrub(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    desktop_machine: &TelegramDesktopMachine,
    page_size: usize,
) -> Result<TelegramDesktopChatPagePlace, String> {
    validate_conversation_message_place_id(place_id)?;
    if page_size == 0 {
        return Err("Telegram message page size must be at least one".to_owned());
    }
    let messages = read_telegram_desktop_shared_messages(
        owner_osl_user_id,
        account_id,
        place_id,
        desktop_machine,
    )?;
    desktop_machine.chat_page_place_from_messages(place_id, page_size, messages)
}

/// A bounded read-only Telegram conversation viewport for the shared scroll
/// reader. It exposes exactly one screen until `scroll_one_screen` is called.
pub struct TelegramDesktopChatPagePlace {
    place: TelegramDesktopPlace,
    messages: Vec<TelegramDesktopMessage>,
    current_page: usize,
    page_size: usize,
    one_screen_scrolls: usize,
    stop_when_reading_page: Option<usize>,
    stop_request_callback: Option<Box<dyn Fn() -> Result<(), String>>>,
}

impl TelegramDesktopChatPagePlace {
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    pub fn one_screen_scroll_count(&self) -> usize {
        self.one_screen_scrolls
    }

    pub fn request_stop_when_reading_page(
        &mut self,
        page_number: usize,
        callback: impl Fn() -> Result<(), String> + 'static,
    ) {
        self.stop_when_reading_page = Some(page_number);
        self.stop_request_callback = Some(Box::new(callback));
    }

    fn current_page_number(&self) -> usize {
        self.current_page.saturating_add(1)
    }
}

impl std::fmt::Debug for TelegramDesktopChatPagePlace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramDesktopChatPagePlace")
            .field("place", &self.place)
            .field("message_count", &self.messages.len())
            .field("current_page", &self.current_page)
            .field("page_size", &self.page_size)
            .field("one_screen_scrolls", &self.one_screen_scrolls)
            .finish()
    }
}

impl SharedConversationScrollablePlace for TelegramDesktopChatPagePlace {
    fn read_current_screen(&self) -> Result<Vec<SharedPlaceMessage>, String> {
        if self.stop_when_reading_page == Some(self.current_page_number()) {
            if let Some(callback) = &self.stop_request_callback {
                callback()?;
            }
        }
        let start = self.current_page.saturating_mul(self.page_size);
        let end = start
            .saturating_add(self.page_size)
            .min(self.messages.len());
        Ok(self.messages[start..end]
            .iter()
            .map(|message| SharedPlaceMessage::new(&message.message_id, &message.text))
            .collect())
    }

    fn scroll_one_screen(&mut self) -> Result<bool, String> {
        let next_start = (self.current_page + 1).saturating_mul(self.page_size);
        if next_start >= self.messages.len() {
            return Ok(false);
        }
        self.current_page += 1;
        self.one_screen_scrolls += 1;
        Ok(true)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxFolderCandidate {
    pub folder_id: String,
    pub label: String,
}

impl MailboxFolderCandidate {
    pub fn new(folder_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            folder_id: folder_id.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxMessageCandidate {
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    pub body: String,
}

impl MailboxMessageCandidate {
    pub fn new(
        folder_id: impl Into<String>,
        message_id: impl Into<String>,
        subject: impl Into<String>,
        time: i64,
        sender: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            folder_id: folder_id.into(),
            message_id: message_id.into(),
            subject: subject.into(),
            time,
            sender: sender.into(),
            body: body.into(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxReaderSnapshot {
    pub folders: Vec<MailboxFolderCandidate>,
    pub messages: Vec<MailboxMessageCandidate>,
    #[serde(default)]
    pub signed_in_address: Option<String>,
}

/// Counts completed receiving reads without being part of the provider mailbox
/// snapshot. Keeping the meter separate makes the read boundary observational:
/// reading cannot delete, mark, move, or otherwise alter provider state.
#[derive(Debug, Default)]
pub struct SharedMailboxReceiveMeter {
    completed_reads: AtomicU64,
}

impl SharedMailboxReceiveMeter {
    pub fn completed_reads(&self) -> u64 {
        self.completed_reads.load(Ordering::Relaxed)
    }

    fn record_completed_read(&self) {
        self.completed_reads.fetch_add(1, Ordering::Relaxed);
    }
}

/// An arrived message made available to the receiving side of a shared mail
/// conversation. This is deliberately a copy of provider-observed fields, not
/// a mutable provider message handle.
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxReceivedMessage {
    pub message_id: String,
    pub sender: String,
    pub time: i64,
    pub text: String,
    pub thread_name: String,
}

/// The result of one receiving read. The sole state change associated with
/// this operation is recorded by the separate [`SharedMailboxReceiveMeter`].
#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxReceivingRead {
    pub inbox: Vec<SharedMailboxReceivedMessage>,
}

impl MailboxReaderSnapshot {
    pub fn new(
        folders: impl IntoIterator<Item = MailboxFolderCandidate>,
        messages: impl IntoIterator<Item = MailboxMessageCandidate>,
    ) -> Self {
        Self {
            folders: folders.into_iter().collect(),
            messages: messages.into_iter().collect(),
            signed_in_address: None,
        }
    }

    pub fn new_for_signed_in_address(
        signed_in_address: impl Into<String>,
        folders: impl IntoIterator<Item = MailboxFolderCandidate>,
        messages: impl IntoIterator<Item = MailboxMessageCandidate>,
    ) -> Self {
        Self {
            folders: folders.into_iter().collect(),
            messages: messages.into_iter().collect(),
            signed_in_address: Some(signed_in_address.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedMailboxOwnership {
    Yours,
    NotYours,
}

impl SharedMailboxOwnership {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::NotYours => "not yours",
        }
    }
}

impl Default for SharedMailboxOwnership {
    fn default() -> Self {
        Self::NotYours
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxFolder {
    pub service_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxMessageSummary {
    pub service_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    #[serde(default)]
    pub ownership: SharedMailboxOwnership,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxMessage {
    pub service_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub message_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    #[serde(default)]
    pub ownership: SharedMailboxOwnership,
    pub body: String,
}

/// Local metadata for isolated service profiles. It intentionally stores no
/// credentials, cookies, tokens, claimed handles, or authentication state.
pub struct ServiceRegistryState {
    path: PathBuf,
    cache: Mutex<RegistryCache>,
    next_id: AtomicU64,
}

#[derive(Default)]
struct RegistryCache {
    loaded: bool,
    accounts: Vec<AccountRecord>,
}

impl ServiceRegistryState {
    pub fn load(path: PathBuf) -> Self {
        Self {
            path,
            // The file-storage key does not exist until the trusted password
            // gate is unlocked. Never weaken locked startup by accepting a
            // plaintext ownership registry before then.
            cache: Mutex::new(RegistryCache::default()),
            next_id: AtomicU64::new(1),
        }
    }

    fn locked_cache(&self) -> Result<std::sync::MutexGuard<'_, RegistryCache>, String> {
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "service registry is unavailable".to_owned())?;
        if !cache.loaded {
            cache.accounts = load_protected_registry(&self.path)?;
            cache.loaded = true;
        }
        Ok(cache)
    }

    pub fn list_for_owner(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<LinkedServiceDemo>, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        Ok(service_registry(&cache.accounts, owner_osl_user_id))
    }

    pub fn create_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        label: String,
    ) -> Result<LinkedAccountDemo, String> {
        self.create_with_provider_for_owner(owner_osl_user_id, service_id, label, None)
    }

    pub fn create_with_provider_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        label: String,
        provider: Option<EmailProvider>,
    ) -> Result<LinkedAccountDemo, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let descriptor = service_descriptor(service_id);
        if descriptor.launch_state != ServiceLaunchState::Available {
            return Err("this service is coming soon and cannot create login profiles".to_owned());
        }
        let provider = match (service_id, provider) {
            (ServiceKind::Email, provider) => Some(provider.unwrap_or_default()),
            (_, None) => None,
            (_, Some(_)) => {
                return Err("email provider is valid only for Email profiles".to_owned())
            }
        };
        let label = label.trim();
        if label.is_empty()
            || label.len() > 40
            || label.chars().any(|character| character.is_control())
        {
            return Err("account label must be 1-40 printable characters".to_string());
        }

        let mut cache = self.locked_cache()?;
        if cache
            .accounts
            .iter()
            .filter(|account| {
                account.service_id == service_id
                    && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
            })
            .count()
            >= MAX_ACCOUNTS_PER_SERVICE
        {
            return Err("this service already has the maximum number of profiles".to_string());
        }

        let record = AccountRecord {
            service_id,
            id: new_account_id(&self.next_id),
            label: label.to_string(),
            owner_osl_user_id: Some(owner_osl_user_id.to_owned()),
            provider,
        };
        let mut updated = cache.accounts.clone();
        updated.push(record.clone());
        write_registry(&self.path, &updated)
            .map_err(|_| "could not save the isolated account profile".to_string())?;
        cache.accounts = updated;
        Ok(account_dto(&record))
    }

    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub fn remove_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<bool, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let mut cache = self.locked_cache()?;
        let mut updated = cache.accounts.clone();
        let before = updated.len();
        updated.retain(|account| {
            account.service_id != service_id
                || account.id.as_str() != account_id
                || account.owner_osl_user_id.as_deref() != Some(owner_osl_user_id)
        });
        let removed = updated.len() != before;
        if removed {
            write_registry(&self.path, &updated)
                .map_err(|_| "could not update the isolated account profile".to_string())?;
            cache.accounts = updated;
        }
        Ok(removed)
    }

    /// Authorize a command-boundary operation without revealing whether a
    /// profile exists for a different local OSL identity.
    pub fn require_owned(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<(), String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        if cache.accounts.iter().any(|account| {
            account.service_id == service_id
                && account.id == account_id
                && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
        }) {
            Ok(())
        } else {
            Err("service account is not registered for the active OSL identity".to_owned())
        }
    }

    /// Re-derive the exact local identity authority before issuing an opaque
    /// owner/service/account binding. The caller cannot nominate an owner
    /// string independently of the loaded identity's public keys.
    pub fn require_identity_authority(
        &self,
        core: &HubCoreState,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<AccountServiceIdentityAuthority, String> {
        let _account_switch = core
            .osl
            .account_switch_lock
            .lock()
            .map_err(|_| "OSL identity switch is unavailable".to_owned())?;
        let identity = core
            .osl
            .identity
            .lock()
            .map_err(|_| "OSL identity state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
        let canonical_owner = keystore::native_user_id(&identity);
        if identity.user_id != canonical_owner {
            return Err("active OSL identity authority is invalid".to_owned());
        }
        let cache = self.locked_cache()?;
        if !cache.accounts.iter().any(|account| {
            account.service_id == service_id
                && account.id == account_id
                && account.owner_osl_user_id.as_deref() == Some(canonical_owner.as_str())
        }) {
            return Err("service account is not registered for the active OSL identity".to_owned());
        }
        AccountServiceIdentityAuthority::issue(&identity, service_id, account_id)
    }

    #[cfg_attr(not(feature = "desktop"), allow(dead_code))]
    pub(crate) fn email_provider_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        account_id: &str,
    ) -> Result<Option<EmailProvider>, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        let account = cache
            .accounts
            .iter()
            .find(|account| {
                account.service_id == service_id
                    && account.id == account_id
                    && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
            })
            .ok_or_else(|| {
                "service account is not registered for the active OSL identity".to_owned()
            })?;
        Ok(account.provider)
    }
}

pub fn service_kind_from_id(service_id: &str) -> Option<ServiceKind> {
    Some(match service_id {
        "discord" => ServiceKind::Discord,
        "telegram" => ServiceKind::Telegram,
        "whatsapp" => ServiceKind::WhatsApp,
        "email" => ServiceKind::Email,
        "signal" => ServiceKind::Signal,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceCapabilityFacts {
    pub service_id: ServiceKind,
    pub placing: bool,
    pub reading: bool,
    pub opening: bool,
    pub real_two_person_protected_messaging: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceControlCapability {
    OpenApp,
    PlaceMessage,
    ReadMessages,
    ProtectedMessaging,
}

impl ServiceControlCapability {
    pub const fn id(self) -> &'static str {
        match self {
            Self::OpenApp => "open_app",
            Self::PlaceMessage => "place_message",
            Self::ReadMessages => "read_messages",
            Self::ProtectedMessaging => "protected_messaging",
        }
    }

    const fn is_built_by(self, facts: ServiceCapabilityFacts) -> bool {
        match self {
            Self::OpenApp => facts.opening,
            Self::PlaceMessage => facts.placing,
            Self::ReadMessages => facts.reading,
            Self::ProtectedMessaging => facts.real_two_person_protected_messaging,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceScreenControl {
    pub id: &'static str,
    pub service_id: ServiceKind,
    pub capability: ServiceControlCapability,
    pub label: &'static str,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceScreenTree {
    pub service_id: ServiceKind,
    pub controls: Vec<ServiceScreenControl>,
}

pub fn installed_service_count() -> usize {
    service_descriptors()
        .into_iter()
        .filter(|descriptor| descriptor.launch_state == ServiceLaunchState::Available)
        .count()
}

pub fn installed_service_capability_facts() -> Vec<ServiceCapabilityFacts> {
    service_descriptors()
        .into_iter()
        .filter(|descriptor| descriptor.launch_state == ServiceLaunchState::Available)
        .filter_map(|descriptor| service_capability_facts_for_kind(descriptor.id))
        .collect()
}

pub fn service_capability_facts(service_id: &str) -> Option<ServiceCapabilityFacts> {
    let service_id = service_kind_from_id(service_id)?;
    service_capability_facts_for_kind(service_id)
}

pub fn installed_service_screen_trees() -> Vec<ServiceScreenTree> {
    installed_service_capability_facts()
        .into_iter()
        .map(service_screen_tree_from_capability_facts)
        .collect()
}

pub fn service_screen_tree_from_capability_facts(
    facts: ServiceCapabilityFacts,
) -> ServiceScreenTree {
    let controls = SERVICE_CONTROL_DEFINITIONS
        .into_iter()
        .filter(|definition| definition.capability.is_built_by(facts))
        .map(|definition| ServiceScreenControl {
            id: definition.id,
            service_id: facts.service_id,
            capability: definition.capability,
            label: definition.label,
        })
        .collect();

    ServiceScreenTree {
        service_id: facts.service_id,
        controls,
    }
}

pub fn drawn_controls_without_capability_record(
    screen_trees: &[ServiceScreenTree],
    capability_records: &[ServiceCapabilityFacts],
) -> usize {
    screen_trees
        .iter()
        .flat_map(|tree| tree.controls.iter())
        .filter(|control| {
            !capability_records.iter().any(|facts| {
                facts.service_id == control.service_id && control.capability.is_built_by(*facts)
            })
        })
        .count()
}

pub fn generated_tile_label(facts: ServiceCapabilityFacts) -> &'static str {
    if facts.real_two_person_protected_messaging || (facts.placing && facts.reading) {
        "Ready"
    } else if facts.placing {
        "Placing only"
    } else if facts.reading {
        "Reading only"
    } else if facts.opening {
        "Opens the app"
    } else {
        "Not started"
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MessagingRiskFacts {
    pub service_id: &'static str,
    pub display_name: &'static str,
    pub facts: [&'static str; MESSAGING_RISK_FACT_COUNT],
}

pub const MESSAGING_RISK_FACT_COUNT: usize = 5;

pub const MESSAGING_RISK_FACTS: [&str; MESSAGING_RISK_FACT_COUNT] = [
    "OSL controls the app",
    "this may break that service's rules",
    "the account may be suspended",
    "OSL cannot remove that risk",
    "you can turn it off",
];

pub fn all_messaging_risk_facts() -> [MessagingRiskFacts; 8] {
    MESSAGING_RISK_FACT_ROWS
}

pub fn messaging_risk_facts(service_id: &str) -> Result<MessagingRiskFacts, String> {
    MESSAGING_RISK_FACT_ROWS
        .into_iter()
        .find(|facts| facts.service_id == service_id)
        .ok_or_else(|| "unknown messaging service".to_owned())
}

pub fn messaging_risk_refusal(service_id: &str) -> Option<String> {
    let facts = MESSAGING_RISK_FACT_ROWS
        .into_iter()
        .find(|facts| facts.service_id == service_id)?;
    Some(format!(
        "you have not agreed to the {} risk",
        facts.display_name
    ))
}

pub fn require_messaging_risk_agreed(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> Result<(), String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    let Some(refusal) = messaging_risk_refusal(service_id) else {
        return Ok(());
    };
    let document = load_messaging_risk_agreements()?;
    if document.agreements.iter().any(|agreement| {
        agreement.owner_osl_user_id == owner_osl_user_id
            && agreement.service_id == service_id
            && agreement.account_id == account_id
    }) {
        Ok(())
    } else {
        Err(refusal)
    }
}

pub fn save_messaging_risk_agreement(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> Result<(), String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    let facts = messaging_risk_facts(service_id)?;
    let mut document = load_messaging_risk_agreements()?;
    let now = ipc::main_password::now_unix_secs_pub();
    let wording = facts
        .facts
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if let Some(existing) = document.agreements.iter_mut().find(|agreement| {
        agreement.owner_osl_user_id == owner_osl_user_id
            && agreement.service_id == service_id
            && agreement.account_id == account_id
    }) {
        existing.agreed_at = now;
        existing.wording = wording;
    } else {
        document.agreements.push(MessagingRiskAgreementRecord {
            owner_osl_user_id: owner_osl_user_id.to_owned(),
            service_id: service_id.to_owned(),
            account_id: account_id.to_owned(),
            agreed_at: now,
            wording,
        });
    }
    write_messaging_risk_agreements(&document)
}

pub fn read_messaging_risk_agreement(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> Result<Option<MessagingRiskAgreement>, String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    messaging_risk_facts(service_id)?;
    let document = load_messaging_risk_agreements()?;
    Ok(document
        .agreements
        .into_iter()
        .find(|agreement| {
            agreement.owner_osl_user_id == owner_osl_user_id
                && agreement.service_id == service_id
                && agreement.account_id == account_id
        })
        .map(|agreement| MessagingRiskAgreement {
            service_id: agreement.service_id,
            account_id: agreement.account_id,
            agreed_at: agreement.agreed_at,
            wording: agreement.wording,
        }))
}

pub fn read_shared_conversation_places(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    service_filled_places: impl IntoIterator<Item = ConversationPlaceCandidate>,
) -> Result<Vec<SharedConversationPlace>, String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    messaging_risk_facts(service_id)?;
    if read_messaging_risk_agreement(owner_osl_user_id, service_id, account_id)?.is_none() {
        return Ok(Vec::new());
    }

    service_filled_places
        .into_iter()
        .map(|place| {
            validate_conversation_place(&place)?;
            Ok(SharedConversationPlace {
                service_id: service_id.to_owned(),
                account_id: account_id.to_owned(),
                place_id: place.place_id,
                label: place.label,
                place_kind: place.place_kind,
                server: place.server,
                channel: place.channel,
            })
        })
        .collect()
}

/// Read the messages of one owner-selected Telegram conversation. The account
/// risk agreement is checked before provider rows are filtered or released.
pub fn read_telegram_desktop_shared_messages(
    owner_osl_user_id: &str,
    account_id: &str,
    place_id: &str,
    desktop_machine: &TelegramDesktopMachine,
) -> Result<Vec<TelegramDesktopMessage>, String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    validate_conversation_message_place_id(place_id)?;
    if read_messaging_risk_agreement(owner_osl_user_id, "telegram", account_id)?.is_none() {
        return Ok(Vec::new());
    }

    let mut messages = desktop_machine
        .messages
        .iter()
        .filter(|message| message.place_id == place_id)
        .cloned()
        .collect::<Vec<_>>();
    for message in &messages {
        validate_telegram_desktop_message(message)?;
    }
    messages.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.message_id.cmp(&right.message_id))
    });
    Ok(messages)
}

pub fn read_shared_mailbox_folders(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<Vec<SharedMailboxFolder>, String> {
    validate_mailbox_reader_binding(owner_osl_user_id, service_id, account_id)?;
    validate_mailbox_folders(&service_filled_mailbox.folders)?;

    service_filled_mailbox
        .folders
        .iter()
        .map(|folder| {
            Ok(SharedMailboxFolder {
                service_id: service_id.to_owned(),
                account_id: account_id.to_owned(),
                folder_id: folder.folder_id.clone(),
                label: folder.label.clone(),
            })
        })
        .collect()
}

pub fn read_shared_mailbox_messages(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    folder_id: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<Vec<SharedMailboxMessageSummary>, String> {
    validate_mailbox_reader_binding(owner_osl_user_id, service_id, account_id)?;
    validate_mailbox_folders(&service_filled_mailbox.folders)?;
    ensure_mailbox_folder_exists(&service_filled_mailbox.folders, folder_id)?;

    service_filled_mailbox
        .messages
        .iter()
        .filter(|message| message.folder_id == folder_id)
        .map(|message| {
            validate_mailbox_message(message)?;
            let ownership = mailbox_message_ownership(service_filled_mailbox, message)?;
            Ok(SharedMailboxMessageSummary {
                service_id: service_id.to_owned(),
                account_id: account_id.to_owned(),
                folder_id: message.folder_id.clone(),
                message_id: message.message_id.clone(),
                subject: message.subject.clone(),
                time: message.time,
                sender: message.sender.clone(),
                ownership,
            })
        })
        .collect()
}

pub fn open_shared_mailbox_message(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    folder_id: &str,
    message_id: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<SharedMailboxMessage, String> {
    validate_mailbox_reader_binding(owner_osl_user_id, service_id, account_id)?;
    validate_mailbox_folders(&service_filled_mailbox.folders)?;
    ensure_mailbox_folder_exists(&service_filled_mailbox.folders, folder_id)?;
    validate_mailbox_text(message_id, "mailbox message id", 180)?;

    let mut matches = service_filled_mailbox
        .messages
        .iter()
        .filter(|message| message.folder_id == folder_id && message.message_id == message_id);
    let Some(message) = matches.next() else {
        return Err("mailbox message not found".to_owned());
    };
    if matches.next().is_some() {
        return Err("mailbox message is duplicated".to_owned());
    }
    validate_mailbox_message(message)?;
    let ownership = mailbox_message_ownership(service_filled_mailbox, message)?;
    Ok(SharedMailboxMessage {
        service_id: service_id.to_owned(),
        account_id: account_id.to_owned(),
        folder_id: message.folder_id.clone(),
        message_id: message.message_id.clone(),
        subject: message.subject.clone(),
        time: message.time,
        sender: message.sender.clone(),
        ownership,
        body: message.body.clone(),
    })
}

/// Read the received side of one shared-mail conversation.
///
/// The mailbox is accepted only by shared reference and this function never
/// calls a provider mutation operation. It selects Inbox rows sent by the
/// named other participant, copies their sender/time/body into the returned
/// projection, and records one completed shared-mailbox read only after the
/// whole projection has been validated and built.
pub fn receive_shared_mailbox_messages(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    own_address: &str,
    other_address: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
    meter: &SharedMailboxReceiveMeter,
) -> Result<SharedMailboxReceivingRead, String> {
    validate_mailbox_reader_binding(owner_osl_user_id, service_id, account_id)?;
    validate_mailbox_text(own_address, "mailbox own address", 254)?;
    validate_mailbox_text(other_address, "mailbox other address", 254)?;
    if own_address.eq_ignore_ascii_case(other_address) {
        return Err("mailbox conversation needs two different addresses".to_owned());
    }
    validate_mailbox_folders(&service_filled_mailbox.folders)?;
    ensure_mailbox_folder_exists(&service_filled_mailbox.folders, "Inbox")?;

    let mut inbox = service_filled_mailbox
        .messages
        .iter()
        .filter(|message| {
            message.folder_id == "Inbox" && message.sender.eq_ignore_ascii_case(other_address)
        })
        .map(|message| {
            validate_mailbox_message(message)?;
            Ok(SharedMailboxReceivedMessage {
                message_id: message.message_id.clone(),
                sender: message.sender.clone(),
                time: message.time,
                text: message.body.clone(),
                thread_name: shared_mailbox_thread_name(
                    own_address,
                    other_address,
                    &message.subject,
                )?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    inbox.sort_by(|left, right| {
        left.time
            .cmp(&right.time)
            .then_with(|| left.message_id.cmp(&right.message_id))
    });

    meter.record_completed_read();
    Ok(SharedMailboxReceivingRead { inbox })
}

/// Produce the conversation name from the two participants and the logical
/// subject. Address order is canonical, and mail reply prefixes are discarded,
/// so either participant computes the same name for `subject` and `Re:
/// subject`. A changed reply subject without a stable provider thread id cannot
/// be safely joined; this boundary intentionally does not guess one.
pub fn shared_mailbox_thread_name(
    first_address: &str,
    second_address: &str,
    subject: &str,
) -> Result<String, String> {
    validate_mailbox_text(first_address, "mailbox first address", 254)?;
    validate_mailbox_text(second_address, "mailbox second address", 254)?;
    let normalized_subject = normalize_shared_mailbox_thread_subject(subject)?;

    let mut addresses = [
        first_address.to_ascii_lowercase(),
        second_address.to_ascii_lowercase(),
    ];
    addresses.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"osl.shared-mailbox.thread-name.v1\\0");
    for value in [
        addresses[0].as_str(),
        addresses[1].as_str(),
        normalized_subject.as_str(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    Ok(format!("shared-mail-{:x}", hasher.finalize()))
}

fn normalize_shared_mailbox_thread_subject(subject: &str) -> Result<String, String> {
    validate_mailbox_text(subject, "mailbox message subject", 512)?;
    let mut normalized = subject.trim();
    while normalized
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("re:"))
    {
        normalized = normalized
            .get(3..)
            .expect("the checked ASCII reply prefix ends on a character boundary")
            .trim_start();
    }
    if normalized.is_empty() {
        Err("mailbox message subject is invalid".to_owned())
    } else {
        Ok(normalized.to_ascii_lowercase())
    }
}

fn service_capability_facts_for_kind(service_id: ServiceKind) -> Option<ServiceCapabilityFacts> {
    SERVICE_CAPABILITY_FACTS
        .iter()
        .copied()
        .find(|facts| facts.service_id == service_id)
}

#[derive(Debug, Clone, Copy)]
struct ServiceControlDefinition {
    id: &'static str,
    capability: ServiceControlCapability,
    label: &'static str,
}

const SERVICE_CONTROL_DEFINITIONS: [ServiceControlDefinition; 4] = [
    ServiceControlDefinition {
        id: "open-app",
        capability: ServiceControlCapability::OpenApp,
        label: "Open app",
    },
    ServiceControlDefinition {
        id: "place-message",
        capability: ServiceControlCapability::PlaceMessage,
        label: "Place message",
    },
    ServiceControlDefinition {
        id: "read-messages",
        capability: ServiceControlCapability::ReadMessages,
        label: "Read messages",
    },
    ServiceControlDefinition {
        id: "protected-messaging",
        capability: ServiceControlCapability::ProtectedMessaging,
        label: "Protected messaging",
    },
];

const MESSAGING_RISK_FACT_ROWS: [MessagingRiskFacts; 8] = [
    messaging_risk_facts_row("discord", "Discord"),
    messaging_risk_facts_row("telegram", "Telegram"),
    messaging_risk_facts_row("whatsapp", "WhatsApp"),
    messaging_risk_facts_row("x", "X"),
    messaging_risk_facts_row("instagram", "Instagram"),
    messaging_risk_facts_row("messenger", "Messenger"),
    messaging_risk_facts_row("signal", "Signal"),
    messaging_risk_facts_row("email", "email"),
];

const fn messaging_risk_facts_row(
    service_id: &'static str,
    display_name: &'static str,
) -> MessagingRiskFacts {
    MessagingRiskFacts {
        service_id,
        display_name,
        facts: MESSAGING_RISK_FACTS,
    }
}

const SERVICE_CAPABILITY_FACTS: [ServiceCapabilityFacts; 5] = [
    ServiceCapabilityFacts {
        service_id: ServiceKind::Discord,
        placing: true,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: false,
    },
    ServiceCapabilityFacts {
        service_id: ServiceKind::Telegram,
        placing: true,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: false,
    },
    ServiceCapabilityFacts {
        service_id: ServiceKind::WhatsApp,
        placing: false,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: false,
    },
    ServiceCapabilityFacts {
        service_id: ServiceKind::Email,
        placing: false,
        reading: false,
        opening: true,
        real_two_person_protected_messaging: false,
    },
    ServiceCapabilityFacts {
        service_id: ServiceKind::Signal,
        placing: false,
        reading: true,
        opening: true,
        real_two_person_protected_messaging: false,
    },
];

fn new_account_id(counter: &AtomicU64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = counter.fetch_add(1, Ordering::Relaxed);
    format!("acct-{now:x}-{sequence:x}")
}

fn sanitize_accounts(accounts: Vec<AccountRecord>) -> Vec<AccountRecord> {
    let mut clean = Vec::new();
    for mut account in accounts {
        account.provider = match account.service_id {
            ServiceKind::Email => Some(account.provider.unwrap_or_default()),
            _ => None,
        };
        if clean.len() >= 100
            || !valid_account_id(&account.id)
            || account.label.trim().is_empty()
            || account.label.len() > 40
            || account
                .label
                .chars()
                .any(|character| character.is_control())
            || account
                .owner_osl_user_id
                .as_deref()
                .is_some_and(|owner| validate_owner_osl_user_id(owner).is_err())
            || clean.iter().any(|existing: &AccountRecord| {
                existing.service_id == account.service_id && existing.id == account.id
            })
            || clean
                .iter()
                .filter(|existing| {
                    existing.service_id == account.service_id
                        && existing.owner_osl_user_id == account.owner_osl_user_id
                })
                .count()
                >= MAX_ACCOUNTS_PER_SERVICE
        {
            continue;
        }
        clean.push(account);
    }
    clean
}

fn validate_owner_osl_user_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err("active OSL identity is invalid".to_owned());
    }
    Ok(())
}

fn valid_account_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn validate_messaging_risk_account_id(account_id: &str) -> Result<(), String> {
    if valid_account_id(account_id) {
        Ok(())
    } else {
        Err("messaging risk agreement account is invalid".to_owned())
    }
}

fn validate_conversation_place(place: &ConversationPlaceCandidate) -> Result<(), String> {
    validate_conversation_place_text(&place.place_id, "conversation place id")?;
    validate_conversation_place_text(&place.label, "conversation place label")?;
    if let Some(server) = &place.server {
        validate_conversation_place_parent(server, "conversation place server")?;
    }
    if let Some(channel) = &place.channel {
        validate_conversation_place_parent(channel, "conversation place channel")?;
    }
    match place.place_kind {
        ConversationPlaceKind::DirectMessage
        | ConversationPlaceKind::Group
        | ConversationPlaceKind::NoteToSelf => Ok(()),
        ConversationPlaceKind::Channel => {
            if place.server.is_some() {
                Ok(())
            } else {
                Err("conversation channel place is missing its server".to_owned())
            }
        }
        ConversationPlaceKind::Thread => {
            if place.channel.is_some() {
                Ok(())
            } else {
                Err("conversation thread place is missing its channel".to_owned())
            }
        }
    }
}

fn validate_conversation_place_parent(
    parent: &ConversationPlaceParent,
    label: &str,
) -> Result<(), String> {
    validate_conversation_place_text(&parent.id, label)?;
    validate_conversation_place_text(&parent.label, label)
}

fn validate_conversation_place_text(value: &str, label: &str) -> Result<(), String> {
    if value.trim() == value
        && !value.is_empty()
        && value.len() <= 128
        && !value.chars().any(|character| character.is_control())
    {
        Ok(())
    } else {
        Err(format!("{label} is invalid"))
    }
}

fn validate_conversation_message_place_id(value: &str) -> Result<(), String> {
    validate_conversation_place_text(value, "conversation message place id")
}

fn validate_telegram_desktop_message(message: &TelegramDesktopMessage) -> Result<(), String> {
    validate_conversation_message_place_id(&message.place_id)?;
    validate_conversation_place_text(&message.message_id, "Telegram message id")?;
    if message.text.trim() != message.text
        || message.text.len() > 8_192
        || message
            .text
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err("Telegram message text is invalid".to_owned());
    }
    Ok(())
}

fn validate_mailbox_reader_binding(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> Result<(), String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    validate_messaging_risk_account_id(account_id)?;
    validate_mail_service_id(service_id)
}

fn validate_mail_service_id(service_id: &str) -> Result<(), String> {
    match service_id {
        "email" | "gmail" | "outlook" | "proton" | "tuta" | "yahoo" | "aol" | "gmx"
        | "maildotcom" | "icloud" => Ok(()),
        _ => Err("unknown mail service".to_owned()),
    }
}

fn validate_mailbox_folders(folders: &[MailboxFolderCandidate]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for folder in folders {
        validate_mailbox_text(&folder.folder_id, "mailbox folder id", 128)?;
        validate_mailbox_text(&folder.label, "mailbox folder label", 128)?;
        if !seen.insert(folder.folder_id.as_str()) {
            return Err("mailbox folder is duplicated".to_owned());
        }
    }
    Ok(())
}

fn ensure_mailbox_folder_exists(
    folders: &[MailboxFolderCandidate],
    folder_id: &str,
) -> Result<(), String> {
    validate_mailbox_text(folder_id, "mailbox folder id", 128)?;
    if folders.iter().any(|folder| folder.folder_id == folder_id) {
        Ok(())
    } else {
        Err("mailbox folder not found".to_owned())
    }
}

fn validate_mailbox_message(message: &MailboxMessageCandidate) -> Result<(), String> {
    validate_mailbox_text(&message.folder_id, "mailbox folder id", 128)?;
    validate_mailbox_text(&message.message_id, "mailbox message id", 180)?;
    validate_mailbox_text(&message.subject, "mailbox message subject", 512)?;
    validate_mailbox_text(&message.sender, "mailbox message sender", 254)?;
    validate_mailbox_body(&message.body)?;
    if message.time > 0 {
        Ok(())
    } else {
        Err("mailbox message time is invalid".to_owned())
    }
}

fn mailbox_message_ownership(
    mailbox: &MailboxReaderSnapshot,
    message: &MailboxMessageCandidate,
) -> Result<SharedMailboxOwnership, String> {
    let Some(signed_in_address) = mailbox.signed_in_address.as_deref() else {
        return Ok(SharedMailboxOwnership::NotYours);
    };
    validate_mailbox_text(signed_in_address, "mailbox signed-in address", 254)?;
    if message.sender.eq_ignore_ascii_case(signed_in_address) {
        Ok(SharedMailboxOwnership::Yours)
    } else {
        Ok(SharedMailboxOwnership::NotYours)
    }
}

fn validate_mailbox_text(value: &str, label: &str, max_bytes: usize) -> Result<(), String> {
    if value.trim() == value
        && !value.is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(|character| character.is_control())
    {
        Ok(())
    } else {
        Err(format!("{label} is invalid"))
    }
}

fn validate_mailbox_body(value: &str) -> Result<(), String> {
    if value.len() <= 256 * 1024
        && value
            .chars()
            .all(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
    {
        Ok(())
    } else {
        Err("mailbox message body is invalid".to_owned())
    }
}

fn messaging_risk_agreement_path() -> Result<PathBuf, String> {
    Ok(keystore::osl_config_dir()
        .map_err(|_| "OSL account storage is unavailable".to_owned())?
        .join(MESSAGING_RISK_AGREEMENT_FILE))
}

fn load_messaging_risk_agreements() -> Result<MessagingRiskAgreementDocument, String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock OSL before agreeing to messaging service risk".to_owned())?;
    let path = messaging_risk_agreement_path()?;
    let Some(bytes) = crate::atomic_file::read_recoverable_bounded(
        &path,
        MAX_MESSAGING_RISK_AGREEMENT_BYTES,
        "messaging risk agreement state",
    )?
    else {
        return Ok(MessagingRiskAgreementDocument {
            version: MESSAGING_RISK_AGREEMENT_VERSION,
            agreements: Vec::new(),
        });
    };
    let plain = ipc::main_password::decrypt_at_rest(&bytes, &key)
        .map_err(|_| "messaging risk agreement state is unavailable".to_owned())?;
    let document: MessagingRiskAgreementDocument = serde_json::from_slice(&plain)
        .map_err(|_| "messaging risk agreement state is malformed".to_owned())?;
    if document.version != MESSAGING_RISK_AGREEMENT_VERSION {
        return Err("messaging risk agreement state is unsupported".to_owned());
    }
    Ok(sanitize_messaging_risk_agreements(document))
}

fn write_messaging_risk_agreements(
    document: &MessagingRiskAgreementDocument,
) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock OSL before agreeing to messaging service risk".to_owned())?;
    let path = messaging_risk_agreement_path()?;
    let bytes = serde_json::to_vec(document)
        .map_err(|_| "messaging risk agreement state could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_MESSAGING_RISK_AGREEMENT_BYTES {
        return Err("messaging risk agreement state exceeds limit".to_owned());
    }
    let sealed = ipc::main_password::encrypt_at_rest(&bytes, &key)
        .map_err(|_| "messaging risk agreement state could not be encrypted".to_owned())?;
    crate::atomic_file::write_recoverable(&path, &sealed, "messaging risk agreement state")
}

fn sanitize_messaging_risk_agreements(
    mut document: MessagingRiskAgreementDocument,
) -> MessagingRiskAgreementDocument {
    document.agreements.retain(|agreement| {
        validate_owner_osl_user_id(&agreement.owner_osl_user_id).is_ok()
            && messaging_risk_refusal(&agreement.service_id).is_some()
            && validate_messaging_risk_account_id(&agreement.account_id).is_ok()
            && agreement.agreed_at > 0
            && agreement.wording == MESSAGING_RISK_FACTS
    });
    document.agreements.sort_by(|left, right| {
        left.owner_osl_user_id
            .cmp(&right.owner_osl_user_id)
            .then_with(|| left.service_id.cmp(&right.service_id))
            .then_with(|| left.account_id.cmp(&right.account_id))
            .then_with(|| left.agreed_at.cmp(&right.agreed_at))
    });
    document.agreements.dedup_by(|left, right| {
        left.owner_osl_user_id == right.owner_osl_user_id
            && left.service_id == right.service_id
            && left.account_id == right.account_id
    });
    document
}

fn load_protected_registry(path: &Path) -> Result<Vec<AccountRecord>, String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before accessing service profiles".to_owned())?;
    let backup = path.with_extension("bak");
    let primary = read_bounded(path)?;
    let fallback = read_bounded(&backup)?;

    for (bytes, recover) in [(primary.as_deref(), false), (fallback.as_deref(), true)] {
        let Some(bytes) = bytes else { continue };
        if !ipc::main_password::has_enc_magic(bytes) {
            continue;
        }
        if let Ok(document) = decode_registry(bytes, &key) {
            if recover {
                fs::copy(&backup, path)
                    .map_err(|_| "service registry backup could not be recovered".to_owned())?;
            }
            if document.version != REGISTRY_VERSION {
                return Err("service registry version is unsupported".to_owned());
            }
            return Ok(sanitize_accounts(document.accounts));
        }
    }

    let encrypted_present = primary
        .as_deref()
        .is_some_and(ipc::main_password::has_enc_magic)
        || fallback
            .as_deref()
            .is_some_and(ipc::main_password::has_enc_magic);
    if encrypted_present {
        return Err("service registry authentication failed".to_owned());
    }
    // Previous builds stored owner rows as unauthenticated JSON. Never leave
    // those account labels or owner identifiers on disk in plaintext. Keep a
    // sealed migration archive for explicit local recovery, but never
    // auto-claim or trust its contents.
    quarantine_plaintext(path, primary.as_deref(), &key)?;
    quarantine_plaintext(&backup, fallback.as_deref(), &key)?;
    Ok(Vec::new())
}

fn read_bounded(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_REGISTRY_BYTES => {
            fs::read(path)
                .map(Some)
                .map_err(|_| "service registry could not be read".to_owned())
        }
        Ok(_) => Err("service registry is not a bounded regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("service registry metadata could not be read".to_owned()),
    }
}

fn decode_registry(bytes: &[u8], key: &[u8; 32]) -> Result<RegistryDocument, String> {
    let plain = ipc::main_password::decrypt_at_rest(bytes, key)
        .map_err(|_| "service registry decrypt failed".to_owned())?;
    serde_json::from_slice(&plain).map_err(|_| "service registry is malformed".to_owned())
}

fn quarantine_plaintext(
    path: &Path,
    plaintext: Option<&[u8]>,
    key: &[u8; 32],
) -> Result<(), String> {
    let Some(plaintext) = plaintext else {
        return Ok(());
    };
    let mut quarantine_name = path.as_os_str().to_os_string();
    // Keep a final extension so `atomic_file`'s `.bak`/`.tmp` companions can
    // never collide with the live registry's own backup path.
    quarantine_name.push(".legacy-encrypted.bin");
    let quarantine = PathBuf::from(quarantine_name);
    let sealed = ipc::main_password::encrypt_at_rest(plaintext, key)
        .map_err(|_| "legacy service registry could not be sealed".to_owned())?;
    crate::atomic_file::write_recoverable(
        &quarantine,
        &sealed,
        "legacy encrypted service registry",
    )?;
    fs::remove_file(path)
        .map_err(|_| "legacy plaintext service registry could not be removed".to_owned())
}

fn write_registry(path: &Path, accounts: &[AccountRecord]) -> Result<(), String> {
    let key = ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "Unlock an OSL identity before accessing service profiles".to_owned())?;
    let bytes = serde_json::to_vec(&RegistryDocument {
        version: REGISTRY_VERSION,
        accounts: accounts.to_vec(),
    })
    .map_err(|_| "service registry could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_REGISTRY_BYTES {
        return Err("service registry exceeds limit".to_owned());
    }
    let sealed = ipc::main_password::encrypt_at_rest(&bytes, &key)
        .map_err(|_| "service registry could not be encrypted".to_owned())?;
    crate::atomic_file::write_recoverable(path, &sealed, "service registry")
}

fn account_dto(account: &AccountRecord) -> LinkedAccountDemo {
    LinkedAccountDemo {
        id: account.id.clone(),
        label: account.label.clone(),
        display_handle: "Sign in on the service".to_string(),
        state: DemoConnectionState::NotLinked,
        provider: account.provider,
    }
}

fn service_registry(accounts: &[AccountRecord], owner_osl_user_id: &str) -> Vec<LinkedServiceDemo> {
    service_descriptors()
        .into_iter()
        .map(|descriptor| LinkedServiceDemo {
            id: descriptor.id,
            display_name: descriptor.display_name.to_string(),
            sidebar_glyph: descriptor.sidebar_glyph.to_string(),
            sidebar_order: descriptor.sidebar_order,
            category: descriptor.category,
            launch_state: descriptor.launch_state,
            supports_native_preview: descriptor.launch_state == ServiceLaunchState::Available,
            supports_protected_preview: descriptor.launch_state == ServiceLaunchState::Available,
            accounts: accounts
                .iter()
                .filter(|account| {
                    account.service_id == descriptor.id
                        && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
                })
                .map(account_dto)
                .collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub struct ServiceDescriptor {
    pub id: ServiceKind,
    pub display_name: &'static str,
    pub sidebar_glyph: &'static str,
    pub sidebar_order: u8,
    pub category: ServiceCategory,
    pub launch_state: ServiceLaunchState,
}

pub fn service_descriptor(id: ServiceKind) -> ServiceDescriptor {
    service_descriptors()
        .into_iter()
        .find(|descriptor| descriptor.id == id)
        .expect("every ServiceKind has a descriptor")
}

fn service_descriptors() -> [ServiceDescriptor; 5] {
    use ServiceCategory::Consumer;
    use ServiceLaunchState::Available;
    [
        descriptor(
            ServiceKind::Discord,
            "Discord",
            "DC",
            10,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::Telegram,
            "Telegram",
            "TG",
            20,
            Consumer,
            Available,
        ),
        descriptor(
            ServiceKind::WhatsApp,
            "WhatsApp",
            "WA",
            25,
            Consumer,
            Available,
        ),
        descriptor(ServiceKind::Email, "Email", "EM", 70, Consumer, Available),
        // Signal has no first-party web messenger. The service is available
        // only through the separately spawned, OSL-owned native profile; the
        // web host remains disabled in `service_host`.
        descriptor(ServiceKind::Signal, "Signal", "SG", 80, Consumer, Available),
    ]
}

const fn descriptor(
    id: ServiceKind,
    display_name: &'static str,
    sidebar_glyph: &'static str,
    sidebar_order: u8,
    category: ServiceCategory,
    launch_state: ServiceLaunchState,
) -> ServiceDescriptor {
    ServiceDescriptor {
        id,
        display_name,
        sidebar_glyph,
        sidebar_order,
        category,
        launch_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNER_A: &str = "osl_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OWNER_B: &str = "osl_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const TEST_KEY: [u8; 32] = [0x5a; 32];

    fn temporary_registry() -> PathBuf {
        ipc::main_password::set_file_storage_key(Some(TEST_KEY));
        std::env::temp_dir().join(format!(
            "osl-hub-service-registry-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn new_registry_has_ruled_services_and_no_fake_accounts() {
        // temporary_registry() flips the process-wide main-password test key
        // (crates/ipc/src/main_password.rs), which other modules' tests also
        // mutate; hold the crate-wide lock so a sibling test can't swap the
        // key out from under this one mid-test.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let services = state.list_for_owner(OWNER_A).unwrap();
        assert_eq!(services.len(), 5);
        assert!(services.iter().all(|service| service.accounts.is_empty()));
        let signal = services
            .iter()
            .find(|service| service.id == ServiceKind::Signal)
            .unwrap();
        assert_eq!(signal.launch_state, ServiceLaunchState::Available);
        assert!(signal.supports_native_preview);
        assert!(services
            .windows(2)
            .all(|pair| pair[0].sidebar_order < pair[1].sidebar_order));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn created_profiles_persist_without_credentials_or_claimed_login() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
            .unwrap();
        assert_eq!(account.state, DemoConnectionState::NotLinked);
        assert_eq!(account.display_handle, "Sign in on the service");
        assert_eq!(account.provider, None);
        let bytes = fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&bytes));
        assert!(!bytes
            .windows("Personal".len())
            .any(|window| window == b"Personal"));
        assert_eq!(
            ServiceRegistryState::load(path.clone())
                .list_for_owner(OWNER_A)
                .unwrap()
                .into_iter()
                .find(|service| service.id == ServiceKind::Discord)
                .unwrap()
                .accounts
                .len(),
            1
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn remove_is_scoped_to_service_and_account() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
            .unwrap();
        assert!(!state
            .remove_for_owner(OWNER_A, ServiceKind::Telegram, &account.id)
            .unwrap());
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account.id)
            .is_ok());
        assert!(state
            .remove_for_owner(OWNER_A, ServiceKind::Discord, &account.id)
            .unwrap());
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account.id)
            .is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn string_service_ids_map_exactly_without_aliases() {
        assert_eq!(service_kind_from_id("discord"), Some(ServiceKind::Discord));
        assert_eq!(
            service_kind_from_id("whatsapp"),
            Some(ServiceKind::WhatsApp)
        );
        assert_eq!(service_kind_from_id("signal"), Some(ServiceKind::Signal));
        // Superseded by the owner ruling on 2026-08-05: social and enterprise
        // IDs are cut surfaces, not service aliases.
        for cut in [
            "instagram",
            "snapchat",
            "x",
            "messenger",
            "slack",
            "linkedin",
            "teams",
        ] {
            assert_eq!(service_kind_from_id(cut), None, "{cut}");
        }
        assert_eq!(service_kind_from_id("Discord"), None);
        assert_eq!(service_kind_from_id("discord.com"), None);
        assert_eq!(service_kind_from_id("../discord"), None);
    }

    #[test]
    fn failed_create_never_leaves_a_phantom_in_memory_account() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let parent_file = temporary_registry().with_extension("blocked-parent");
        fs::write(&parent_file, b"not a directory").unwrap();
        let state = ServiceRegistryState::load(parent_file.join("registry.json"));
        assert!(state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
            .is_err());

        // The invariant is that a create which failed to persist is never
        // served back, not that the failure surfaces as any particular error.
        // How "my parent directory is actually a file" surfaces is a platform
        // fact: Linux reports ENOTDIR, which `read_bounded` treats as an error,
        // while Windows reports ERROR_PATH_NOT_FOUND, which Rust maps to
        // `NotFound` and `read_bounded` deliberately reads as "no registry
        // yet". Asserting `is_err()` was asserting the Linux errno. Assert the
        // absence of the phantom instead, under either outcome.
        match state.list_for_owner(OWNER_A) {
            Err(_) => {}
            Ok(services) => assert!(
                services.iter().all(|service| service.accounts.is_empty()),
                "a create that failed to persist must never be listed afterwards"
            ),
        }
        let _ = fs::remove_file(&parent_file);
    }

    #[test]
    fn registry_path_that_is_not_a_regular_file_fails_closed() {
        // The portable half of the property the test above used to lean on:
        // when the registry path *does* resolve to something, and that
        // something is not a bounded regular file, the registry must refuse
        // rather than quietly present itself as empty -- an empty read is what
        // a later write would clobber a real registry from. Pointing the
        // registry straight at a directory reaches `read_bounded`'s
        // `metadata` -> "not a bounded regular file" arm identically on Linux
        // and Windows, with no errno spelling involved.
        let _serial = crate::global_keystore_test_lock();
        let directory = temporary_registry().with_extension("registry-is-a-directory");
        fs::create_dir_all(&directory).unwrap();
        let state = ServiceRegistryState::load(directory.clone());
        assert!(
            state.list_for_owner(OWNER_A).is_err(),
            "a registry path that is not a regular file must fail closed, not read as empty"
        );
        assert!(
            state
                .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_string())
                .is_err(),
            "and it must refuse writes rather than create a fresh registry over it"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn multiple_email_profiles_preserve_fixed_providers_across_restart() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let gmail = state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Email,
                "Personal Gmail".to_owned(),
                Some(EmailProvider::Gmail),
            )
            .unwrap();
        let proton = state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Email,
                "Private Proton".to_owned(),
                Some(EmailProvider::Proton),
            )
            .unwrap();
        assert_ne!(gmail.id, proton.id);
        assert_eq!(gmail.provider, Some(EmailProvider::Gmail));
        assert_eq!(proton.provider, Some(EmailProvider::Proton));

        let reloaded = ServiceRegistryState::load(path.clone());
        assert_eq!(
            reloaded
                .email_provider_for_owner(OWNER_A, ServiceKind::Email, &gmail.id)
                .unwrap(),
            Some(EmailProvider::Gmail)
        );
        assert_eq!(
            reloaded
                .email_provider_for_owner(OWNER_A, ServiceKind::Email, &proton.id)
                .unwrap(),
            Some(EmailProvider::Proton)
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn arbitrary_provider_binding_fails_closed() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        assert!(state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Discord,
                "Bad binding".to_owned(),
                Some(EmailProvider::Tuta),
            )
            .is_err());
        let signal = state
            .create_for_owner(OWNER_A, ServiceKind::Signal, "Signal".to_owned())
            .unwrap();
        assert_eq!(signal.label, "Signal");
        assert!(state
            .list_for_owner(OWNER_A)
            .unwrap()
            .iter()
            .filter(|service| service.id != ServiceKind::Signal)
            .all(|service| service.accounts.is_empty()));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_unowned_account_is_quarantined_during_version_three_migration() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        fs::write(
            &path,
            br#"{
              "version": 1,
              "accounts": [{
                "serviceId": "email",
                "id": "acct-existing",
                "label": "Existing email"
              }]
            }"#,
        )
        .unwrap();
        let state = ServiceRegistryState::load(path.clone());
        assert!(state
            .list_for_owner(OWNER_A)
            .unwrap()
            .into_iter()
            .all(|service| service.accounts.is_empty()));
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Email, "acct-existing")
            .is_err());
        state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Owned".to_owned())
            .unwrap();
        let archive = PathBuf::from(format!("{}.legacy-encrypted.bin", path.display()));
        let archived = fs::read(&archive).unwrap();
        assert!(ipc::main_password::has_enc_magic(&archived));
        assert!(String::from_utf8(
            ipc::main_password::decrypt_at_rest(&archived, &TEST_KEY).unwrap()
        )
        .unwrap()
        .contains("acct-existing"));
        assert!(!PathBuf::from(format!("{}.legacy-untrusted", path.display())).exists());
        let sealed = fs::read(&path).unwrap();
        assert!(ipc::main_password::has_enc_magic(&sealed));
        let migrated: serde_json::Value = serde_json::from_slice(
            &ipc::main_password::decrypt_at_rest(&sealed, &TEST_KEY).unwrap(),
        )
        .unwrap();
        assert_eq!(migrated["version"], REGISTRY_VERSION);
        assert!(migrated["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["id"] != "acct-existing"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn complete_backup_recovers_a_crash_between_registry_replacements() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal".to_owned())
            .unwrap();
        let backup = path.with_extension("bak");
        fs::rename(&path, &backup).unwrap();

        let recovered = ServiceRegistryState::load(path.clone());
        assert!(recovered
            .require_owned(OWNER_A, ServiceKind::Discord, &account.id)
            .is_ok());
        assert!(path.exists());
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(backup);
    }

    #[test]
    fn two_owners_cannot_cross_list_or_authorize_open() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let account_a = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Owner A".to_owned())
            .unwrap();
        let account_b = state
            .create_for_owner(OWNER_B, ServiceKind::Discord, "Owner B".to_owned())
            .unwrap();

        let listed_a = state
            .list_for_owner(OWNER_A)
            .unwrap()
            .into_iter()
            .find(|service| service.id == ServiceKind::Discord)
            .unwrap()
            .accounts;
        let listed_b = state
            .list_for_owner(OWNER_B)
            .unwrap()
            .into_iter()
            .find(|service| service.id == ServiceKind::Discord)
            .unwrap()
            .accounts;
        assert_eq!(
            listed_a.iter().map(|row| &row.id).collect::<Vec<_>>(),
            vec![&account_a.id]
        );
        assert_eq!(
            listed_b.iter().map(|row| &row.id).collect::<Vec<_>>(),
            vec![&account_b.id]
        );

        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account_a.id)
            .is_ok());
        assert!(state
            .require_owned(OWNER_B, ServiceKind::Discord, &account_b.id)
            .is_ok());
        assert!(state
            .require_owned(OWNER_A, ServiceKind::Discord, &account_b.id)
            .is_err());
        assert!(state
            .require_owned(OWNER_B, ServiceKind::Discord, &account_a.id)
            .is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn reload_preserves_the_per_owner_profile_limit() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        for index in 0..MAX_ACCOUNTS_PER_SERVICE {
            state
                .create_for_owner(OWNER_A, ServiceKind::Discord, format!("Owner A {index}"))
                .unwrap();
            state
                .create_for_owner(OWNER_B, ServiceKind::Discord, format!("Owner B {index}"))
                .unwrap();
        }

        let reloaded = ServiceRegistryState::load(path.clone());
        for owner in [OWNER_A, OWNER_B] {
            let profiles = reloaded
                .list_for_owner(owner)
                .unwrap()
                .into_iter()
                .find(|service| service.id == ServiceKind::Discord)
                .unwrap()
                .accounts;
            assert_eq!(profiles.len(), MAX_ACCOUNTS_PER_SERVICE);
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn tampered_encrypted_registry_fails_closed() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        ServiceRegistryState::load(path.clone())
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Owner A".to_owned())
            .unwrap();
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        fs::write(&path, bytes).unwrap();
        let reloaded = ServiceRegistryState::load(path.clone());
        assert!(reloaded.list_for_owner(OWNER_A).is_err());
        assert!(path.exists());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn plaintext_primary_and_backup_get_distinct_encrypted_archive_paths() {
        // See new_registry_has_twelve_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let backup = path.with_extension("bak");
        fs::write(&path, br#"{"version":2,"accounts":[]}"#).unwrap();
        fs::write(&backup, br#"{"version":2,"accounts":[]}"#).unwrap();
        let state = ServiceRegistryState::load(path.clone());
        assert!(state
            .list_for_owner(OWNER_A)
            .unwrap()
            .iter()
            .all(|row| row.accounts.is_empty()));
        for original in [&path, &backup] {
            assert!(!original.exists());
            let archive = PathBuf::from(format!("{}.legacy-encrypted.bin", original.display()));
            let bytes = fs::read(archive).unwrap();
            assert!(ipc::main_password::has_enc_magic(&bytes));
            assert!(ipc::main_password::decrypt_at_rest(&bytes, &TEST_KEY).is_ok());
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VisibleMailMessage {
    pub message_id: String,
    pub mailbox: String,
    pub sender_address: Option<String>,
}

pub fn mail_message_is_owned_by_signed_in_address(
    signed_in_address: &str,
    message: &VisibleMailMessage,
) -> Result<bool, MailOwnerCheckError> {
    let sender = message
        .sender_address
        .as_deref()
        .map(str::trim)
        .filter(|sender| !sender.is_empty())
        .ok_or(MailOwnerCheckError::SenderAddressUnreadable)?;
    Ok(sender.eq_ignore_ascii_case(signed_in_address.trim()))
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MailOwnerCheckError {
    SenderAddressUnreadable,
}

impl MailOwnerCheckError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::SenderAddressUnreadable => "OSL: sender address cannot be read",
        }
    }
}
