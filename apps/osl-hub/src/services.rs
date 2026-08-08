use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::account_identity_authority::AccountServiceIdentityAuthority;
use crate::core_bridge::HubCoreState;
use crate::models::{
    DemoConnectionState, EmailProvider, LinkedAccountDemo, LinkedServiceDemo, ServiceCategory,
    ServiceKind, ServiceLaunchState,
};

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
    #[serde(default)]
    pub ownership: SharedMailboxOwnership,
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
}

impl MailboxReaderSnapshot {
    pub fn new(
        folders: impl IntoIterator<Item = MailboxFolderCandidate>,
        messages: impl IntoIterator<Item = MailboxMessageCandidate>,
    ) -> Self {
        Self {
            folders: folders.into_iter().collect(),
            messages: messages.into_iter().collect(),
        }
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
    pub body: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxPagingRequest {
    pub page_size: usize,
    pub pause_between_pages_ms: u64,
    #[serde(default)]
    pub stop_requested_during_page: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedMailboxPagingStopReason {
    EndOfFolder,
    StopRequested,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SharedMailboxPagedRead {
    pub service_id: String,
    pub account_id: String,
    pub folder_id: String,
    pub messages: Vec<SharedMailboxMessageSummary>,
    pub page_count: usize,
    pub page_size: usize,
    pub pause_between_pages_ms: u64,
    pub inter_page_gaps_ms: Vec<u64>,
    pub stop_reason: SharedMailboxPagingStopReason,
    pub stop_requested_during_page: Option<usize>,
    pub stopped_on_page_number: Option<usize>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VisibleMailMessage {
    pub message_id: String,
    pub mailbox: String,
    pub sender_address: Option<String>,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlookWebScrubMessageSummary {
    pub message_id: String,
    pub folder_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    pub owner_label: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlookWebScrubMailboxRead {
    pub folders: Vec<SharedMailboxFolder>,
    pub sent_items: Vec<OutlookWebScrubMessageSummary>,
    pub inbox: Vec<OutlookWebScrubMessageSummary>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TutaScrubMessageSummary {
    pub message_id: String,
    pub folder_id: String,
    pub subject: String,
    pub time: i64,
    pub sender: String,
    pub owner_label: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TutaScrubMailboxRead {
    pub folders: Vec<SharedMailboxFolder>,
    pub sent: Vec<TutaScrubMessageSummary>,
    pub inbox: Vec<TutaScrubMessageSummary>,
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
    Channel,
    Thread,
}

impl ConversationPlaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
            Self::Group => "group",
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

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MailboxFolderCandidate {
    pub folder_id: String,
    pub label: String,
}

impl MailboxFolderCandidate {
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
    pub body: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VisibleMailMessage {
    pub message_id: String,
    pub mailbox: String,
    pub sender_address: Option<String>,
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

/// Local metadata for isolated service profiles. It intentionally stores no
/// credentials, cookies, tokens, claimed handles, or authentication state.
pub struct ServiceRegistryState {
    path: PathBuf,
    cache: Mutex<RegistryCache>,
    next_id: AtomicU64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubAccountDescriptor {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub account_label: String,
    pub app_or_browser_label: String,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DetectedAccountOpenChoiceKind {
    WindowsApp,
    Browser,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAccountOpenChoice {
    pub kind: DetectedAccountOpenChoiceKind,
    pub label: String,
pub struct ServiceAccountRunQueue {
    pub service_id: ServiceKind,
    pub accounts: Vec<ServiceAccountRunQueueEntry>,
    pub active_count: usize,
    pub waiting_count: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountRunQueueEntry {
    pub account_id: String,
    pub label: String,
    pub status: ServiceAccountRunStatus,
}

pub const NORMAL_SCREEN_CHANGE_WAIT_MS: u64 = 350;
pub const PAUSE_AFTER_SCROLL_SCREEN_CHANGE: &str = "normal-screen-change-after-scroll";

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountActionRunLog {
    pub account_id: String,
    pub entries: Vec<ServiceAccountActionRunLogEntry>,
    pub concurrent_actions: usize,
    pub max_parallel_scroll_actions: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAccountDescriptor {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub account_label: String,
    pub open_choices: Vec<DetectedAccountOpenChoice>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DetectedAccountStoreRecord {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub account_label: String,
    pub app_label: Option<String>,
    pub browser_label: Option<String>,
}

pub trait DetectedAccountStore {
    fn detected_accounts_for_owner(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<DetectedAccountStoreRecord>, String>;
pub struct ServiceAccountActionRunLogEntry {
    pub sequence: usize,
    pub account_id: String,
    pub action: ServiceAccountActionKind,
    pub name: String,
    pub start_tick: u64,
    pub end_tick: u64,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAccountActionKind {
    Scroll,
    Wait,
    InspectVisibleRows,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAccountRunStatus {
    Active,
    Waiting,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceMessagePlaceType {
    Conversation,
    Dm,
    Group,
    Server,
    Channel,
    Thread,
}

impl ServiceMessagePlaceType {
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceMessagePlaceType::Conversation => "conversation",
            ServiceMessagePlaceType::Dm => "dm",
            ServiceMessagePlaceType::Group => "group",
            ServiceMessagePlaceType::Server => "server",
            ServiceMessagePlaceType::Channel => "channel",
            ServiceMessagePlaceType::Thread => "thread",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceMessagePlace {
    pub place_type: ServiceMessagePlaceType,
    pub place_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMessageSnapshot {
    pub place_type: ServiceMessagePlaceType,
    pub place_id: String,
    pub message_id: String,
    pub author_id: String,
    pub text: String,
    pub date: String,
    pub time: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMessageReadBatch {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub place_count: usize,
    pub message_count: usize,
    pub messages: Vec<ServiceMessageSnapshot>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountFindingRuleRunRequest {
    pub account_id: String,
    pub selected_rule_names: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountFindingRuleRunReceipt {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub selected_rule_names: Vec<String>,
    pub selected_rule_count: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMessagePossibleMatch {
    pub message_text: String,
    pub reason: String,
    pub service: String,
    pub place: String,
    pub date: String,
    pub time: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceAccountFinishedNotice {
    pub service: String,
    pub finished_account_id: String,
    pub finished_account_label: String,
    pub match_count: usize,
    pub next_account_id: Option<String>,
    pub next_account_label: Option<String>,
    pub title: String,
    pub detail: String,
}

pub trait AccountServiceMessageConnection {
    fn service_id(&self) -> ServiceKind;
    fn account_id(&self) -> &str;
    fn read_messages(
        &self,
        place: &ServiceMessagePlace,
    ) -> Result<Vec<ServiceMessageSnapshot>, String>;
}

pub trait SignedInAccountServiceMessageConnection: AccountServiceMessageConnection {
    fn is_signed_in(&self) -> bool;
}

pub trait AccountServiceRunnerConnection {
    fn service_id(&self) -> ServiceKind;
    fn account_id(&self) -> &str;
    fn start_finding_rule_run(
        &self,
        request: &ServiceAccountFindingRuleRunRequest,
    ) -> Result<(), String>;
}

#[derive(Default)]
struct RegistryCache {
    loaded: bool,
    accounts: Vec<AccountRecord>,
}

impl ServiceAccountRunQueue {
    pub fn paced_action_log_for_active_account(
        &self,
    ) -> Result<ServiceAccountActionRunLog, String> {
        let active_accounts = self
            .accounts
            .iter()
            .filter(|account| account.status == ServiceAccountRunStatus::Active)
            .collect::<Vec<_>>();
        let [active] = active_accounts.as_slice() else {
            return Err("service account pacing requires exactly one active account".to_owned());
        };
        build_paced_action_run_log(&active.account_id)
    }
}

impl ServiceAccountActionRunLogEntry {
    pub fn duration_ticks(&self) -> u64 {
        self.end_tick.saturating_sub(self.start_tick)
    }
}

pub fn read_messages_through_approved_account_service_connection(
    queue: &ServiceAccountRunQueue,
    connection: &dyn AccountServiceMessageConnection,
    places: &[ServiceMessagePlace],
) -> Result<ServiceMessageReadBatch, String> {
    if connection.service_id() != queue.service_id {
        return Err("service message reader connection is bound to the wrong service".to_owned());
    }
    if !valid_account_id(connection.account_id()) {
        return Err("service message reader connection account id is invalid".to_owned());
    }
    let active_accounts = queue
        .accounts
        .iter()
        .filter(|account| account.status == ServiceAccountRunStatus::Active)
        .collect::<Vec<_>>();
    let [active] = active_accounts.as_slice() else {
        return Err(
            "service message reader requires exactly one active approved account".to_owned(),
        );
    };
    if active.account_id != connection.account_id() {
        return Err(
            "service message reader requires the active approved account connection".to_owned(),
        );
    }

    let mut messages = Vec::new();
    for place in places {
        validate_service_message_place(place)?;
        let place_messages = connection.read_messages(place)?;
        for message in &place_messages {
            validate_service_message_snapshot(message)?;
            if message.place_type != place.place_type || message.place_id != place.place_id {
                return Err(
                    "service message reader received a message outside the requested place"
                        .to_owned(),
                );
            }
        }
        messages.extend(place_messages);
    }

    Ok(ServiceMessageReadBatch {
        service_id: queue.service_id,
        account_id: active.account_id.clone(),
        place_count: places.len(),
        message_count: messages.len(),
        messages,
    })
}

pub fn read_messages_through_signed_in_account_service_connection(
    queue: &ServiceAccountRunQueue,
    connection: &dyn SignedInAccountServiceMessageConnection,
    places: &[ServiceMessagePlace],
) -> Result<ServiceMessageReadBatch, String> {
    if !connection.is_signed_in() {
        return Err("sign in yourself".to_owned());
    }
    read_messages_through_approved_account_service_connection(queue, connection, places)
}

pub fn start_runner_for_approved_account_service_connection(
    queue: &ServiceAccountRunQueue,
    connection: &dyn AccountServiceRunnerConnection,
    selected_rule_names: &[String],
) -> Result<ServiceAccountFindingRuleRunReceipt, String> {
    if connection.service_id() != queue.service_id {
        return Err("service runner connection is bound to the wrong service".to_owned());
    }
    if !valid_account_id(connection.account_id()) {
        return Err("service runner connection account id is invalid".to_owned());
    }
    let active_accounts = queue
        .accounts
        .iter()
        .filter(|account| account.status == ServiceAccountRunStatus::Active)
        .collect::<Vec<_>>();
    let [active] = active_accounts.as_slice() else {
        return Err("service runner requires exactly one active approved account".to_owned());
    };
    if active.account_id != connection.account_id() {
        return Err("service runner requires the active approved account connection".to_owned());
    }

    let selected_rule_names = validated_selected_finding_rule_names(selected_rule_names)?;
    let request = ServiceAccountFindingRuleRunRequest {
        account_id: active.account_id.clone(),
        selected_rule_names: selected_rule_names.clone(),
    };
    connection.start_finding_rule_run(&request)?;

    Ok(ServiceAccountFindingRuleRunReceipt {
        service_id: queue.service_id,
        account_id: request.account_id,
        selected_rule_count: selected_rule_names.len(),
        selected_rule_names,
    })
}

pub fn find_possible_matches_in_read_messages(
    batch: &ServiceMessageReadBatch,
    selected_rule_names: &[String],
) -> Result<Vec<ServiceMessagePossibleMatch>, String> {
    let selected_rule_names = validated_selected_finding_rule_names(selected_rule_names)?;
    let mut matches = Vec::new();
    for message in &batch.messages {
        validate_service_message_snapshot(message)?;
        let Some(reason) = possible_match_reason(&message.text, &selected_rule_names) else {
            continue;
        };
        matches.push(ServiceMessagePossibleMatch {
            message_text: message.text.clone(),
            reason: reason.to_owned(),
            service: service_descriptor(batch.service_id).display_name.to_owned(),
            place: format!("{}:{}", message.place_type.as_str(), message.place_id),
            date: message.date.clone(),
            time: message.time.clone(),
        });
    }
    Ok(matches)
}

// OSL-FINISH-ONLY-TESTS-DELIBERATE: finish_active_service_account_run_notice/2 is a tested active queued-account completion formatter kept until the live UI finish event is wired.
// The command path currently exposes queue progress.
pub fn finish_active_service_account_run_notice(
    queue: &ServiceAccountRunQueue,
    match_count: usize,
) -> Result<ServiceAccountFinishedNotice, String> {
    let active_accounts = queue
        .accounts
        .iter()
        .filter(|account| account.status == ServiceAccountRunStatus::Active)
        .collect::<Vec<_>>();
    let [active] = active_accounts.as_slice() else {
        return Err("service account finish requires exactly one active account".to_owned());
    };
    let next = queue
        .accounts
        .iter()
        .find(|account| account.status == ServiceAccountRunStatus::Waiting);
    let service = service_descriptor(queue.service_id).display_name.to_owned();
    let next_account_label = next.map(|account| account.label.clone());
    let detail = service_account_finished_notice_detail(&service, match_count, next);

    Ok(ServiceAccountFinishedNotice {
        service: service.clone(),
        finished_account_id: active.account_id.clone(),
        finished_account_label: active.label.clone(),
        match_count,
        next_account_id: next.map(|account| account.account_id.clone()),
        next_account_label,
        title: format!("{service} scan finished"),
        detail,
    })
}

fn service_account_finished_notice_detail(
    service: &str,
    match_count: usize,
    next: Option<&ServiceAccountRunQueueEntry>,
) -> String {
    let match_clause = match match_count {
        0 => format!("No matches found in {service}."),
        1 => format!("1 match found in {service}."),
        count => format!("{count} matches found in {service}."),
    };
    let next_clause = next
        .map(|account| format!("Next account: {}.", account.label))
        .unwrap_or_else(|| "No next account.".to_owned());
    format!("{match_clause} {next_clause}")
}

fn build_paced_action_run_log(account_id: &str) -> Result<ServiceAccountActionRunLog, String> {
    if !valid_account_id(account_id) {
        return Err("service account id is invalid".to_owned());
    }
    let mut entries = Vec::new();
    let mut cursor = 0;
    push_paced_entry(
        &mut entries,
        account_id,
        ServiceAccountActionKind::Scroll,
        "scroll-message-list",
        1,
        &mut cursor,
    );
    push_paced_entry(
        &mut entries,
        account_id,
        ServiceAccountActionKind::Wait,
        PAUSE_AFTER_SCROLL_SCREEN_CHANGE,
        NORMAL_SCREEN_CHANGE_WAIT_MS,
        &mut cursor,
    );
    push_paced_entry(
        &mut entries,
        account_id,
        ServiceAccountActionKind::InspectVisibleRows,
        "inspect-visible-rows",
        1,
        &mut cursor,
    );
    Ok(ServiceAccountActionRunLog {
        account_id: account_id.to_owned(),
        concurrent_actions: overlapping_action_pairs(&entries),
        max_parallel_scroll_actions: max_parallel_scroll_actions(&entries),
        entries,
    })
}

fn push_paced_entry(
    entries: &mut Vec<ServiceAccountActionRunLogEntry>,
    account_id: &str,
    action: ServiceAccountActionKind,
    name: &str,
    duration_ticks: u64,
    cursor: &mut u64,
) {
    let start_tick = *cursor;
    let end_tick = start_tick.saturating_add(duration_ticks.max(1));
    entries.push(ServiceAccountActionRunLogEntry {
        sequence: entries.len(),
        account_id: account_id.to_owned(),
        action,
        name: name.to_owned(),
        start_tick,
        end_tick,
    });
    *cursor = end_tick;
}

fn overlapping_action_pairs(entries: &[ServiceAccountActionRunLogEntry]) -> usize {
    let mut overlaps = 0;
    for (index, left) in entries.iter().enumerate() {
        for right in entries.iter().skip(index + 1) {
            if left.start_tick < right.end_tick && right.start_tick < left.end_tick {
                overlaps += 1;
            }
        }
    }
    overlaps
}

fn max_parallel_scroll_actions(entries: &[ServiceAccountActionRunLogEntry]) -> usize {
    let scroll_entries = entries
        .iter()
        .filter(|entry| entry.action == ServiceAccountActionKind::Scroll)
        .collect::<Vec<_>>();
    scroll_entries
        .iter()
        .flat_map(|entry| [entry.start_tick, entry.end_tick.saturating_sub(1)])
        .map(|tick| {
            scroll_entries
                .iter()
                .filter(|entry| entry.start_tick <= tick && tick < entry.end_tick)
                .count()
        })
        .max()
        .unwrap_or(0)
}

fn validate_service_message_place(place: &ServiceMessagePlace) -> Result<(), String> {
    validate_service_message_id("service message place id", &place.place_id)
}

fn validate_service_message_snapshot(message: &ServiceMessageSnapshot) -> Result<(), String> {
    validate_service_message_id("service message id", &message.message_id)?;
    validate_service_message_id("service message author id", &message.author_id)?;
    if message.text.is_empty()
        || message.text.len() > 16 * 1024
        || message
            .text
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err("service message text is invalid".to_owned());
    }
    validate_service_message_date(&message.date)?;
    validate_service_message_time(&message.time)?;
    Ok(())
}

fn possible_match_reason(text: &str, selected_rule_names: &[String]) -> Option<&'static str> {
    selected_rule_names.iter().find_map(|rule_name| {
        let lower = text.to_ascii_lowercase();
        match rule_name.as_str() {
            "credential" if contains_service_secret_assignment(&lower) => {
                Some("This looks like a password, API key, or access credential.")
            }
            "payment_card"
                if service_digit_runs(text)
                    .iter()
                    .any(|digits| service_luhn_valid(digits)) =>
            {
                Some("This contains a number shaped like a payment card.")
            }
            "precise_location" if contains_service_precise_location(&lower, text) => {
                Some("This may reveal a precise home or meeting location.")
            }
            _ => None,
        }
    })
}

fn validated_selected_finding_rule_names(rule_names: &[String]) -> Result<Vec<String>, String> {
    if rule_names.is_empty() {
        return Err("service runner requires at least one selected finding rule".to_owned());
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(rule_names.len());
    for rule_name in rule_names {
        if !valid_finding_rule_name(rule_name) {
            return Err("selected finding rule name is invalid".to_owned());
        }
        if !seen.insert(rule_name.as_str()) {
            return Err("selected finding rule name is duplicated".to_owned());
        }
        selected.push(rule_name.clone());
    }
    Ok(selected)
}

fn valid_finding_rule_name(rule_name: &str) -> bool {
    !rule_name.is_empty()
        && rule_name.len() <= 64
        && rule_name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

fn validate_service_message_id(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn validate_service_message_date(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err("service message date is invalid".to_owned())
    }
}

fn validate_service_message_time(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() == 5
        && bytes[2] == b':'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 2 || byte.is_ascii_digit())
        && value[0..2].parse::<u8>().is_ok_and(|hour| hour < 24)
        && value[3..5].parse::<u8>().is_ok_and(|minute| minute < 60)
    {
        Ok(())
    } else {
        Err("service message time is invalid".to_owned())
    }
}

fn contains_service_secret_assignment(lower: &str) -> bool {
    [
        "password", "passwd", "api key", "api_key", "secret", "token",
    ]
    .iter()
    .any(|label| {
        lower.find(label).is_some_and(|index| {
            let tail = &lower[index + label.len()..];
            let tail = tail.trim_start();
            tail.starts_with(':') || tail.starts_with('=') || tail.starts_with(" is ")
        })
    }) || ["ghp_", "xoxb-", "sk_live_", "rk_live_", "akia"]
        .iter()
        .any(|prefix| lower.contains(prefix))
}

fn contains_service_precise_location(lower: &str, text: &str) -> bool {
    [
        "my address is",
        "home address is",
        "meet me at",
        "i live at",
    ]
    .iter()
    .any(|term| lower.contains(term))
        && text.chars().any(|character| character.is_ascii_digit())
}

fn service_digit_runs(text: &str) -> Vec<Vec<u8>> {
    let mut runs = Vec::new();
    let mut run = Vec::new();
    for byte in text.bytes() {
        if byte.is_ascii_digit() {
            run.push(byte - b'0');
        } else if matches!(byte, b' ' | b'-') && !run.is_empty() {
            continue;
        } else {
            if (13..=19).contains(&run.len()) {
                runs.push(std::mem::take(&mut run));
            }
            run.clear();
        }
    }
    if (13..=19).contains(&run.len()) {
        runs.push(run);
    }
    runs
}

fn service_luhn_valid(digits: &[u8]) -> bool {
    if !(13..=19).contains(&digits.len()) || digits.iter().all(|digit| *digit == digits[0]) {
        return false;
    }
    let parity = digits.len() % 2;
    let sum: u32 = digits
        .iter()
        .enumerate()
        .map(|(index, digit)| {
            let mut value = u32::from(*digit);
            if index % 2 == parity {
                value *= 2;
                if value > 9 {
                    value -= 9;
                }
            }
            value
        })
        .sum();
    sum % 10 == 0
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

    pub fn list_scrub_accounts_for_owner(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<ScrubAccountDescriptor>, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        Ok(scrub_accounts(&cache.accounts, owner_osl_user_id))
    }

    pub fn list_detected_accounts_with_open_choices(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<DetectedAccountDescriptor>, String> {
        list_detected_accounts_with_open_choices(self, owner_osl_user_id)
    pub fn run_queue_for_owner(
        &self,
        owner_osl_user_id: &str,
        service_id: ServiceKind,
        approved_account_ids: &[String],
    ) -> Result<ServiceAccountRunQueue, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let approved =
            approved_account_ids
                .iter()
                .try_fold(BTreeSet::new(), |mut approved, account_id| {
                    if !valid_account_id(account_id) {
                        return Err("approved service account id is invalid".to_owned());
                    }
                    approved.insert(account_id.as_str());
                    Ok(approved)
                })?;
        let cache = self.locked_cache()?;
        let saved_accounts = cache
            .accounts
            .iter()
            .filter(|account| {
                account.service_id == service_id
                    && account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id)
            })
            .collect::<Vec<_>>();
        for account_id in &approved {
            if !saved_accounts
                .iter()
                .any(|account| account.id.as_str() == *account_id)
            {
                return Err(
                    "approved service account is not registered for the active OSL identity"
                        .to_owned(),
                );
            }
        }

        let mut active_assigned = false;
        let accounts = saved_accounts
            .into_iter()
            .filter(|account| approved.contains(account.id.as_str()))
            .map(|account| {
                let status = if active_assigned {
                    ServiceAccountRunStatus::Waiting
                } else {
                    active_assigned = true;
                    ServiceAccountRunStatus::Active
                };
                ServiceAccountRunQueueEntry {
                    account_id: account.id.clone(),
                    label: account.label.clone(),
                    status,
                }
            })
            .collect::<Vec<_>>();
        let active_count = accounts
            .iter()
            .filter(|account| account.status == ServiceAccountRunStatus::Active)
            .count();
        let waiting_count = accounts
            .iter()
            .filter(|account| account.status == ServiceAccountRunStatus::Waiting)
            .count();
        Ok(ServiceAccountRunQueue {
            service_id,
            accounts,
            active_count,
            waiting_count,
        })
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
            Ok(SharedMailboxMessageSummary {
                service_id: service_id.to_owned(),
                account_id: account_id.to_owned(),
                folder_id: message.folder_id.clone(),
                message_id: message.message_id.clone(),
                subject: message.subject.clone(),
                time: message.time,
                sender: message.sender.clone(),
            })
        })
        .collect()
}

pub fn read_shared_mailbox_messages_paged(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
    folder_id: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
    request: SharedMailboxPagingRequest,
) -> Result<SharedMailboxPagedRead, String> {
    if request.page_size == 0 || request.page_size > 100 {
        return Err("mailbox page size is invalid".to_owned());
    }
    if request.pause_between_pages_ms == 0 {
        return Err("mailbox page pause is invalid".to_owned());
    }
    if request
        .stop_requested_during_page
        .is_some_and(|page| page == 0)
    {
        return Err("mailbox stop page is invalid".to_owned());
    }

    let all_messages = read_shared_mailbox_messages(
        owner_osl_user_id,
        service_id,
        account_id,
        folder_id,
        service_filled_mailbox,
    )?;
    let total_pages = all_messages.len().div_ceil(request.page_size);
    let mut messages = Vec::new();
    let mut inter_page_gaps_ms = Vec::new();
    let mut stop_reason = SharedMailboxPagingStopReason::EndOfFolder;
    let mut stopped_on_page_number = None;

    for (page_index, page) in all_messages.chunks(request.page_size).enumerate() {
        let page_number = page_index + 1;
        if page_number > 1 {
            inter_page_gaps_ms.push(request.pause_between_pages_ms);
        }
        messages.extend(page.iter().cloned());

        if request.stop_requested_during_page == Some(page_number) {
            stop_reason = SharedMailboxPagingStopReason::StopRequested;
            stopped_on_page_number = Some(page_number);
            break;
        }
    }

    Ok(SharedMailboxPagedRead {
        service_id: service_id.to_owned(),
        account_id: account_id.to_owned(),
        folder_id: folder_id.to_owned(),
        messages,
        page_count: match stop_reason {
            SharedMailboxPagingStopReason::EndOfFolder => total_pages,
            SharedMailboxPagingStopReason::StopRequested => stopped_on_page_number.unwrap_or(0),
        },
        page_size: request.page_size,
        pause_between_pages_ms: request.pause_between_pages_ms,
        inter_page_gaps_ms,
        stop_reason,
        stop_requested_during_page: request.stop_requested_during_page,
        stopped_on_page_number,
    })
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
    Ok(SharedMailboxMessage {
        service_id: service_id.to_owned(),
        account_id: account_id.to_owned(),
        folder_id: message.folder_id.clone(),
        message_id: message.message_id.clone(),
        subject: message.subject.clone(),
        time: message.time,
        sender: message.sender.clone(),
        body: message.body.clone(),
    })
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

pub fn read_outlook_web_scrub_mailbox(
    owner_osl_user_id: &str,
    account_id: &str,
    signed_in_address: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<OutlookWebScrubMailboxRead, String> {
    let folders = read_shared_mailbox_folders(
        owner_osl_user_id,
        "outlook",
        account_id,
        service_filled_mailbox,
    )?;
    let sent_items = read_shared_mailbox_messages(
        owner_osl_user_id,
        "outlook",
        account_id,
        "Sent Items",
        service_filled_mailbox,
    )?;
    let inbox = read_shared_mailbox_messages(
        owner_osl_user_id,
        "outlook",
        account_id,
        "Inbox",
        service_filled_mailbox,
    )?;

    Ok(OutlookWebScrubMailboxRead {
        folders,
        sent_items: outlook_web_scrub_summaries(signed_in_address, sent_items)?,
        inbox: outlook_web_scrub_summaries(signed_in_address, inbox)?,
    })
}

pub fn read_tuta_scrub_mailbox(
    owner_osl_user_id: &str,
    account_id: &str,
    signed_in_address: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
) -> Result<TutaScrubMailboxRead, String> {
    let folders = read_shared_mailbox_folders(
        owner_osl_user_id,
        "tuta",
        account_id,
        service_filled_mailbox,
    )?;
    let sent = read_shared_mailbox_messages(
        owner_osl_user_id,
        "tuta",
        account_id,
        "Sent",
        service_filled_mailbox,
    )?;
    let inbox = read_shared_mailbox_messages(
        owner_osl_user_id,
        "tuta",
        account_id,
        "Inbox",
        service_filled_mailbox,
    )?;

    Ok(TutaScrubMailboxRead {
        folders,
        sent: tuta_scrub_summaries(signed_in_address, sent)?,
        inbox: tuta_scrub_summaries(signed_in_address, inbox)?,
    })
}

pub fn page_outlook_desktop_scrub_mailbox_folder(
    owner_osl_user_id: &str,
    account_id: &str,
    folder_id: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
    request: SharedMailboxPagingRequest,
) -> Result<SharedMailboxPagedRead, String> {
    read_shared_mailbox_messages_paged(
        owner_osl_user_id,
        "outlook",
        account_id,
        folder_id,
        service_filled_mailbox,
        request,
    )
}

pub fn page_maildotcom_scrub_mailbox_folder(
    owner_osl_user_id: &str,
    account_id: &str,
    folder_id: &str,
    service_filled_mailbox: &MailboxReaderSnapshot,
    request: SharedMailboxPagingRequest,
) -> Result<SharedMailboxPagedRead, String> {
    read_shared_mailbox_messages_paged(
        owner_osl_user_id,
        "maildotcom",
        account_id,
        folder_id,
        service_filled_mailbox,
        request,
    )
}

fn outlook_web_scrub_summaries(
    signed_in_address: &str,
    messages: Vec<SharedMailboxMessageSummary>,
) -> Result<Vec<OutlookWebScrubMessageSummary>, String> {
    messages
        .into_iter()
        .map(|message| {
            let owned = mail_message_is_owned_by_signed_in_address(
                signed_in_address,
                &VisibleMailMessage {
                    message_id: message.message_id.clone(),
                    mailbox: message.folder_id.clone(),
                    sender_address: Some(message.sender.clone()),
                },
            )
            .map_err(|error| error.reason().to_owned())?;
            Ok(OutlookWebScrubMessageSummary {
                message_id: message.message_id,
                folder_id: message.folder_id,
                subject: message.subject,
                time: message.time,
                sender: message.sender,
                owner_label: if owned {
                    "yours".to_owned()
                } else {
                    "not_yours".to_owned()
                },
            })
        })
        .collect()
}

fn tuta_scrub_summaries(
    signed_in_address: &str,
    messages: Vec<SharedMailboxMessageSummary>,
) -> Result<Vec<TutaScrubMessageSummary>, String> {
    messages
        .into_iter()
        .map(|message| {
            let owned = mail_message_is_owned_by_signed_in_address(
                signed_in_address,
                &VisibleMailMessage {
                    message_id: message.message_id.clone(),
                    mailbox: message.folder_id.clone(),
                    sender_address: Some(message.sender.clone()),
                },
            )
            .map_err(|error| error.reason().to_owned())?;
            Ok(TutaScrubMessageSummary {
                message_id: message.message_id,
                folder_id: message.folder_id,
                subject: message.subject,
                time: message.time,
                sender: message.sender,
                owner_label: if owned {
                    "yours".to_owned()
                } else {
                    "not_yours".to_owned()
                },
            })
        })
        .collect()
impl DetectedAccountStore for ServiceRegistryState {
    }

    fn detected_accounts_for_owner(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<Vec<DetectedAccountStoreRecord>, String> {
        validate_owner_osl_user_id(owner_osl_user_id)?;
        let cache = self.locked_cache()?;
        Ok(cache
            .accounts
            .iter()
            .filter(|account| account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id))
            .map(detected_account_store_record)
            .collect())
    }
}

pub fn list_detected_accounts_with_open_choices<S: DetectedAccountStore>(
    store: &S,
    owner_osl_user_id: &str,
) -> Result<Vec<DetectedAccountDescriptor>, String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    let mut accounts = store
        .detected_accounts_for_owner(owner_osl_user_id)?
        .into_iter()
        .map(detected_account_descriptor)
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        left.account_label
            .cmp(&right.account_label)
            .then_with(|| left.account_id.cmp(&right.account_id))
    });
    Ok(accounts)
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

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProtectedDeliveryProof {
    pub service_id: ServiceKind,
    pub protected_message_id: String,
    pub sender_person_id: String,
    pub recipient_person_id: String,
    pub protected_message_received: bool,
    pub received_by_real_other_person: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceReadyLabel {
    Ready,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceReadyDecision {
    pub service_id: ServiceKind,
    pub label: Option<ServiceReadyLabel>,
    pub refusal: Option<String>,
}

pub const READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY: &str =
    "ready_requires_real_two_person_protected_messaging_capability";
pub const READY_REQUIRES_MATCHING_DELIVERY_PROOF: &str = "ready_requires_matching_delivery_proof";
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

pub fn direct_service_ready_label(
    service_id: &str,
    delivery_proof: Option<&ProtectedDeliveryProof>,
) -> Result<ServiceReadyLabel, &'static str> {
    let facts =
        service_capability_facts(service_id).ok_or(READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY)?;
    direct_service_ready_label_for_facts(facts, delivery_proof)
}

pub fn direct_service_ready_label_for_facts(
    facts: ServiceCapabilityFacts,
    delivery_proof: Option<&ProtectedDeliveryProof>,
) -> Result<ServiceReadyLabel, &'static str> {
    if !facts.real_two_person_protected_messaging {
        return Err(READY_REQUIRES_REAL_TWO_PERSON_CAPABILITY);
    }
    let Some(delivery_proof) = delivery_proof else {
        return Err(READY_REQUIRES_MATCHING_DELIVERY_PROOF);
    };
    if !delivery_proof_matches_ready_rule(facts.service_id, delivery_proof) {
        return Err(READY_REQUIRES_MATCHING_DELIVERY_PROOF);
    }
    Ok(ServiceReadyLabel::Ready)
}

pub fn ready_decisions_from_service_proof_records(
    proof_records: &[ProtectedDeliveryProof],
) -> Vec<ServiceReadyDecision> {
    service_descriptors()
        .into_iter()
        .filter(|descriptor| descriptor.launch_state == ServiceLaunchState::Available)
        .filter_map(|descriptor| {
            let facts = service_capability_facts_for_kind(descriptor.id)?;
            let matching_proof = proof_records
                .iter()
                .find(|proof| delivery_proof_matches_ready_rule(descriptor.id, proof));
            let effective_facts = ServiceCapabilityFacts {
                real_two_person_protected_messaging: facts.real_two_person_protected_messaging
                    || matching_proof.is_some(),
                ..facts
            };
            let decision = direct_service_ready_label_for_facts(effective_facts, matching_proof);
            Some(match decision {
                Ok(label) => ServiceReadyDecision {
                    service_id: descriptor.id,
                    label: Some(label),
                    refusal: None,
                },
                Err(refusal) => ServiceReadyDecision {
                    service_id: descriptor.id,
                    label: None,
                    refusal: Some(refusal.to_owned()),
                },
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

pub fn all_messaging_risk_facts() -> [MessagingRiskFacts; 7] {
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

fn delivery_proof_matches_ready_rule(
    service_id: ServiceKind,
    proof: &ProtectedDeliveryProof,
) -> bool {
    proof.service_id == service_id
        && proof.protected_message_received
        && proof.received_by_real_other_person
        && !proof.protected_message_id.trim().is_empty()
        && !proof.sender_person_id.trim().is_empty()
        && !proof.recipient_person_id.trim().is_empty()
        && proof.sender_person_id != proof.recipient_person_id
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

const MESSAGING_RISK_FACT_ROWS: [MessagingRiskFacts; 7] = [
    messaging_risk_facts_row("discord", "Discord"),
    messaging_risk_facts_row("telegram", "Telegram"),
    messaging_risk_facts_row("whatsapp", "WhatsApp"),
    messaging_risk_facts_row("x", "X"),
    messaging_risk_facts_row("instagram", "Instagram"),
    messaging_risk_facts_row("messenger", "Messenger"),
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishedServiceAccountRunNotice {
    pub id: String,
    pub service: String,
    pub finished_account: String,
    pub next_account: Option<String>,
    pub match_count: u32,
    pub title: String,
    pub detail: String,
}

// OSL-FINISH-ONLY-TESTS-DELIBERATE: finish_active_service_account_run_notice/5 is a tested hosted-account completion DTO kept because no runtime command starts this finish toast yet.
pub fn finish_active_service_account_run_notice(
    service_id: ServiceKind,
    run_id: &str,
    finished_account: &str,
    match_count: u32,
    next_account: Option<&str>,
) -> Result<FinishedServiceAccountRunNotice, String> {
    let run_id = require_notice_text("run id", run_id, 96)?;
    let finished_account = require_notice_text("finished account", finished_account, 40)?;
    let next_account = next_account
        .map(|account| require_notice_text("next account", account, 40))
        .transpose()?;
    let service = service_descriptor(service_id).display_name.to_owned();
    let title = format!("{service} scan finished");
    let result = if match_count == 0 {
        format!("No matches found in {service} account {finished_account}.")
    } else if match_count == 1 {
        format!("1 match found in {service} account {finished_account}.")
    } else {
        format!("{match_count} matches found in {service} account {finished_account}.")
    };
    let detail = match &next_account {
        Some(next) => format!("{result} Next account: {next}."),
        None => format!("{result} No more accounts are queued."),
    };
    Ok(FinishedServiceAccountRunNotice {
        id: format!("scrub-finished-{run_id}"),
        service,
        finished_account,
        next_account,
        match_count,
        title,
        detail,
    })
}

fn require_notice_text(label: &str, value: &str, max_len: usize) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max_len
        || value.chars().any(|character| character.is_control())
    {
        return Err(format!("{label} is not a valid notice label"));
    }
    Ok(value.to_owned())
}

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

fn validate_mailbox_reader_binding(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> Result<(), String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    if !valid_account_id(account_id) {
        return Err("mailbox account id is invalid".to_owned());
    }
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
        ConversationPlaceKind::DirectMessage | ConversationPlaceKind::Group => Ok(()),
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

fn validate_mailbox_reader_binding(
    owner_osl_user_id: &str,
    service_id: &str,
    account_id: &str,
) -> Result<(), String> {
    validate_owner_osl_user_id(owner_osl_user_id)?;
    if !valid_account_id(account_id) {
        return Err("mailbox account is invalid".to_owned());
        return Err("mailbox account id is invalid".to_owned());
    }
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

fn scrub_account_dto(account: &AccountRecord) -> ScrubAccountDescriptor {
    ScrubAccountDescriptor {
        service_id: account.service_id,
        account_id: account.id.clone(),
        account_label: account.label.clone(),
        app_or_browser_label: scrub_app_or_browser_label(account).to_owned(),
    }
}

fn scrub_app_or_browser_label(account: &AccountRecord) -> &'static str {
    match (account.service_id, account.provider) {
        (ServiceKind::Email, Some(EmailProvider::Gmail)) => "Gmail",
        (ServiceKind::Email, Some(EmailProvider::Outlook)) => "Outlook",
        (ServiceKind::Email, Some(EmailProvider::Proton)) => "Proton Mail",
        (ServiceKind::Email, Some(EmailProvider::Tuta)) => "Tuta Mail",
        (ServiceKind::Email, Some(EmailProvider::Yahoo)) => "Yahoo Mail",
        (ServiceKind::Email, Some(EmailProvider::Aol)) => "AOL Mail",
        (ServiceKind::Email, Some(EmailProvider::Gmx)) => "GMX Mail",
        (ServiceKind::Email, Some(EmailProvider::Maildotcom)) => "mail.com",
        (ServiceKind::Email, Some(EmailProvider::Icloud)) => "iCloud Mail",
        _ => service_descriptor(account.service_id).display_name,
    }
}

fn scrub_accounts(
    accounts: &[AccountRecord],
    owner_osl_user_id: &str,
) -> Vec<ScrubAccountDescriptor> {
    let mut scrub_accounts = accounts
        .iter()
        .filter(|account| account.owner_osl_user_id.as_deref() == Some(owner_osl_user_id))
        .map(scrub_account_dto)
        .collect::<Vec<_>>();
    scrub_accounts.sort_by(|left, right| {
        left.app_or_browser_label
            .cmp(&right.app_or_browser_label)
            .then_with(|| left.account_label.cmp(&right.account_label))
            .then_with(|| left.account_id.cmp(&right.account_id))
    });
    scrub_accounts
}

fn detected_account_store_record(account: &AccountRecord) -> DetectedAccountStoreRecord {
    let app_label = match account.service_id {
        ServiceKind::Email => None,
        _ => Some(
            service_descriptor(account.service_id)
                .display_name
                .to_owned(),
        ),
    };
    let browser_label = match account.service_id {
        ServiceKind::Email => Some(scrub_app_or_browser_label(account).to_owned()),
        _ => None,
    };
    DetectedAccountStoreRecord {
        service_id: account.service_id,
        account_id: account.id.clone(),
        account_label: account.label.clone(),
        app_label,
        browser_label,
    }
}

fn detected_account_descriptor(record: DetectedAccountStoreRecord) -> DetectedAccountDescriptor {
    let mut open_choices = Vec::new();
    if let Some(label) = record.app_label {
        open_choices.push(DetectedAccountOpenChoice {
            kind: DetectedAccountOpenChoiceKind::WindowsApp,
            label,
        });
    }
    if let Some(label) = record.browser_label {
        open_choices.push(DetectedAccountOpenChoice {
            kind: DetectedAccountOpenChoiceKind::Browser,
            label,
        });
    }
    open_choices.sort();
    DetectedAccountDescriptor {
        service_id: record.service_id,
        account_id: record.account_id,
        account_label: record.account_label,
        open_choices,
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

    struct FakeDetectedAccountStore {
        records_by_owner: Vec<(&'static str, Vec<DetectedAccountStoreRecord>)>,
    }

    impl DetectedAccountStore for FakeDetectedAccountStore {
        fn detected_accounts_for_owner(
            &self,
            owner_osl_user_id: &str,
        ) -> Result<Vec<DetectedAccountStoreRecord>, String> {
            Ok(self
                .records_by_owner
                .iter()
                .find_map(|(owner, records)| (*owner == owner_osl_user_id).then(|| records.clone()))
                .unwrap_or_default())
        }
    }

    fn choice_kind_label(kind: DetectedAccountOpenChoiceKind) -> &'static str {
        match kind {
            DetectedAccountOpenChoiceKind::WindowsApp => "Windows app",
            DetectedAccountOpenChoiceKind::Browser => "browser",
        }
    }

    #[test]
    fn detected_account_reader_returns_each_fake_account_and_browser_versus_app_choices() {
        let store = FakeDetectedAccountStore {
            records_by_owner: vec![(
                OWNER_A,
                vec![
                    DetectedAccountStoreRecord {
                        service_id: ServiceKind::Email,
                        account_id: "browser-gmail-0314".to_owned(),
                        account_label: "Work Gmail".to_owned(),
                        app_label: None,
                        browser_label: Some("Chrome".to_owned()),
                    },
                    DetectedAccountStoreRecord {
                        service_id: ServiceKind::Email,
                        account_id: "hybrid-outlook-0314".to_owned(),
                        account_label: "Work Outlook".to_owned(),
                        app_label: Some("Outlook app".to_owned()),
                        browser_label: Some("Edge".to_owned()),
                    },
                    DetectedAccountStoreRecord {
                        service_id: ServiceKind::Discord,
                        account_id: "windows-discord-0314".to_owned(),
                        account_label: "Personal Discord".to_owned(),
                        app_label: Some("Discord app".to_owned()),
                        browser_label: None,
                    },
                ],
            )],
        };

        let accounts = list_detected_accounts_with_open_choices(&store, OWNER_A).unwrap();
        let missing_owner_accounts =
            list_detected_accounts_with_open_choices(&store, OWNER_B).unwrap();
        let produced_choices = accounts
            .iter()
            .flat_map(|account| {
                account.open_choices.iter().map(move |choice| {
                    format!(
                        "{}:{}={}",
                        account.account_label,
                        choice_kind_label(choice.kind),
                        choice.label
                    )
                })
            })
            .collect::<Vec<_>>();
        let row_open_labels = accounts
            .iter()
            .map(|account| {
                let choices = account
                    .open_choices
                    .iter()
                    .map(|choice| match choice.kind {
                        DetectedAccountOpenChoiceKind::WindowsApp => "Windows app".to_owned(),
                        DetectedAccountOpenChoiceKind::Browser => choice.label.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join("+");
                format!("{}:{}", account.account_label, choices)
            })
            .collect::<Vec<_>>();
        let no_windows_app = accounts
            .iter()
            .find(|account| account.account_id == "browser-gmail-0314")
            .expect("fake browser-only account should be present");

        println!(
            "detected_account_reader rows={} choices={} browser_versus_app_choices={}",
            accounts.len(),
            produced_choices.len(),
            produced_choices.join("|")
        );
        println!(
            "detected_account_reader_row_labels={}",
            row_open_labels.join("|")
        );
        println!(
            "detected_account_reader_no_windows_app_choices={}",
            no_windows_app
                .open_choices
                .iter()
                .map(|choice| format!("{}={}", choice_kind_label(choice.kind), choice.label))
                .collect::<Vec<_>>()
                .join("|")
        );
        println!(
            "detected_account_reader_missing_owner_rows={}",
            missing_owner_accounts.len()
        );
        println!(
            "detected_account_reader_json={}",
            serde_json::to_string(&accounts).unwrap()
        );

        assert_eq!(
            accounts,
            vec![
                DetectedAccountDescriptor {
                    service_id: ServiceKind::Discord,
                    account_id: "windows-discord-0314".to_owned(),
                    account_label: "Personal Discord".to_owned(),
                    open_choices: vec![DetectedAccountOpenChoice {
                        kind: DetectedAccountOpenChoiceKind::WindowsApp,
                        label: "Discord app".to_owned(),
                    }],
                },
                DetectedAccountDescriptor {
                    service_id: ServiceKind::Email,
                    account_id: "browser-gmail-0314".to_owned(),
                    account_label: "Work Gmail".to_owned(),
                    open_choices: vec![DetectedAccountOpenChoice {
                        kind: DetectedAccountOpenChoiceKind::Browser,
                        label: "Chrome".to_owned(),
                    }],
                },
                DetectedAccountDescriptor {
                    service_id: ServiceKind::Email,
                    account_id: "hybrid-outlook-0314".to_owned(),
                    account_label: "Work Outlook".to_owned(),
                    open_choices: vec![
                        DetectedAccountOpenChoice {
                            kind: DetectedAccountOpenChoiceKind::WindowsApp,
                            label: "Outlook app".to_owned(),
                        },
                        DetectedAccountOpenChoice {
                            kind: DetectedAccountOpenChoiceKind::Browser,
                            label: "Edge".to_owned(),
                        },
                    ],
                },
            ],
            "the detected-account reader must return every fake-store account with its browser-versus-app open choices"
        );
        assert_eq!(
            row_open_labels,
            vec![
                "Personal Discord:Windows app".to_owned(),
                "Work Gmail:Chrome".to_owned(),
                "Work Outlook:Windows app+Edge".to_owned(),
            ],
            "each row must name either the Windows app choice or a named browser choice"
        );
        assert_eq!(
            no_windows_app.open_choices,
            vec![DetectedAccountOpenChoice {
                kind: DetectedAccountOpenChoiceKind::Browser,
                label: "Chrome".to_owned(),
            }],
            "an account with no Windows app must offer the browser choice only"
        );
        assert_eq!(
            missing_owner_accounts,
            Vec::<DetectedAccountDescriptor>::new(),
            "an account owner the fake store does not hold must return nothing"
        );
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
    fn task_1439_finishing_two_differently_named_accounts_keeps_notice_account_names_distinct() {
        let first = finish_active_service_account_run_notice(
            ServiceKind::Discord,
            "task1439-run-alpha",
            "Alpha",
            0,
            Some("Beta"),
        )
        .unwrap();
        let second = finish_active_service_account_run_notice(
            ServiceKind::Discord,
            "task1439-run-beta",
            "Beta",
            2,
            Some("Gamma"),
        )
        .unwrap();

        assert_eq!(first.finished_account, "Alpha");
        assert_eq!(first.next_account.as_deref(), Some("Beta"));
        assert_eq!(
            first.detail,
            "No matches found in Discord account Alpha. Next account: Beta."
        );
        assert!(!first.detail.contains("account Beta."));
        assert_eq!(second.finished_account, "Beta");
        assert_eq!(second.next_account.as_deref(), Some("Gamma"));
        assert_eq!(
            second.detail,
            "2 matches found in Discord account Beta. Next account: Gamma."
        );
        assert!(!second.detail.contains("account Alpha."));

        println!("TASK1439 finish_action=finish_active_service_account_run_notice");
        for notice in [&first, &second] {
            println!("TASK1439 notice_id={}", notice.id);
            println!("TASK1439 notice_service={}", notice.service);
            println!(
                "TASK1439 finished_account={} next_account={} match_count={}",
                notice.finished_account,
                notice.next_account.as_deref().unwrap_or("NONE"),
                notice.match_count
            );
            println!("TASK1439 notice_title={}", notice.title);
            println!("TASK1439 notice_detail={}", notice.detail);
        }
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
    fn direct_scrub_account_list_returns_supported_signed_in_accounts_with_labels() {
        // See new_registry_has_ruled_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let discord = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Personal Discord".to_owned())
            .unwrap();
        let gmail = state
            .create_with_provider_for_owner(
                OWNER_A,
                ServiceKind::Email,
                "Work Gmail".to_owned(),
                Some(EmailProvider::Gmail),
            )
            .unwrap();
        let _other_owner = state
            .create_for_owner(OWNER_B, ServiceKind::Signal, "Other Signal".to_owned())
            .unwrap();

        let accounts = state.list_scrub_accounts_for_owner(OWNER_A).unwrap();
        println!(
            "direct_list_scrub_accounts_fixture={}",
            serde_json::to_string(&accounts).unwrap()
        );

        assert_eq!(
            accounts,
            vec![
                ScrubAccountDescriptor {
                    service_id: ServiceKind::Discord,
                    account_id: discord.id,
                    account_label: "Personal Discord".to_owned(),
                    app_or_browser_label: "Discord".to_owned(),
                },
                ScrubAccountDescriptor {
                    service_id: ServiceKind::Email,
                    account_id: gmail.id,
                    account_label: "Work Gmail".to_owned(),
                    app_or_browser_label: "Gmail".to_owned(),
                },
            ],
            "the direct Scrub account command must return exactly two owner-scoped fixture accounts with app/browser labels"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn direct_scrub_account_list_returns_empty_when_none_exist() {
        // See new_registry_has_ruled_services_and_no_fake_accounts for why
        // this lock is needed.
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());

        let accounts = state.list_scrub_accounts_for_owner(OWNER_A).unwrap();
        println!(
            "direct_list_scrub_accounts_empty={}",
            serde_json::to_string(&accounts).unwrap()
        );
        assert_eq!(
            accounts,
            Vec::<ScrubAccountDescriptor>::new(),
            "a profile with no supported signed-in accounts must return an empty list"
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
    fn task_3050_outlook_web_shared_mailbox_reader_returns_seeded_scrub_mailbox() {
        let owner = "osl_task_3050_owner";
        let account = "acct-task-3050-outlook";
        let signed_in = "owner@outlook.example";
        let mailbox = MailboxReaderSnapshot::new(
            [
                MailboxFolderCandidate::new("Inbox", "Inbox"),
                MailboxFolderCandidate::new("Sent Items", "Sent Items"),
                MailboxFolderCandidate::new("Archive", "Archive"),
                MailboxFolderCandidate::new("Deleted Items", "Deleted Items"),
            ],
            [
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "sent-task-3050-001",
                    "SCRUB-OW-MINE",
                    1_786_017_600_000,
                    signed_in,
                    "Outlook web message that belongs to the signed-in mailbox.",
                ),
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "sent-task-3050-002",
                    "SCRUB-OW-SENT-SECOND",
                    1_786_021_200_000,
                    signed_in.to_ascii_uppercase(),
                    "Second sent Outlook web message.",
                ),
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "sent-task-3050-003",
                    "SCRUB-OW-SENT-THIRD",
                    1_786_024_800_000,
                    "Owner@Outlook.Example",
                    "Third sent Outlook web message.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "inbox-task-3050-001",
                    "SCRUB-OW-INBOX-FIRST",
                    1_786_028_400_000,
                    "friend-one@example.test",
                    "Inbox negative control one.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "inbox-task-3050-002",
                    "SCRUB-OW-INBOX-SECOND",
                    1_786_032_000_000,
                    "alerts@example.test",
                    "Inbox negative control two.",
                ),
            ],
        );

        let read = read_outlook_web_scrub_mailbox(owner, account, signed_in, &mailbox)
            .expect("seeded Outlook web mailbox reads through the shared mailbox reader");
        println!("TASK3050_DIRECT_READER=outlook_web_shared_mailbox_reader");
        println!("TASK3050_FOLDER_COUNT={}", read.folders.len());
        for folder in &read.folders {
            println!(
                "TASK3050_FOLDER id=\"{}\" label=\"{}\" service={} account={}",
                folder.folder_id, folder.label, folder.service_id, folder.account_id
            );
        }
        println!(
            "TASK3050_SENT_ITEMS_MESSAGE_COUNT={}",
            read.sent_items.len()
        );
        for message in &read.sent_items {
            println!(
                "TASK3050_SENT_ITEM id={} subject=\"{}\" time={} sender=\"{}\" called={}",
                message.message_id,
                message.subject,
                message.time,
                message.sender,
                message.owner_label
            );
        }
        for message in &read.inbox {
            println!(
                "TASK3050_INBOX_NEGATIVE id={} subject=\"{}\" sender=\"{}\" called={}",
                message.message_id, message.subject, message.sender, message.owner_label
            );
        }
        let marked = read
            .sent_items
            .iter()
            .find(|message| message.subject == "SCRUB-OW-MINE")
            .expect("seeded marker is present in Sent Items");
        println!(
            "TASK3050_MARKED marker=SCRUB-OW-MINE called={}",
            marked.owner_label
        );
        println!(
            "TASK3050_INBOX_NOT_YOURS_COUNT={}",
            read.inbox
                .iter()
                .filter(|message| message.owner_label == "not_yours")
                .count()
        );

        assert_eq!(read.folders.len(), 4);
        assert_eq!(
            read.folders
                .iter()
                .map(|folder| folder.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Inbox", "Sent Items", "Archive", "Deleted Items"]
        );
        assert_eq!(read.sent_items.len(), 3);
        assert_eq!(
            read.sent_items
                .iter()
                .map(|message| {
                    (
                        message.subject.as_str(),
                        message.time,
                        message.sender.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("SCRUB-OW-MINE", 1_786_017_600_000, "owner@outlook.example",),
                (
                    "SCRUB-OW-SENT-SECOND",
                    1_786_021_200_000,
                    "OWNER@OUTLOOK.EXAMPLE",
                ),
                (
                    "SCRUB-OW-SENT-THIRD",
                    1_786_024_800_000,
                    "Owner@Outlook.Example",
                ),
            ]
        );
        assert_eq!(marked.owner_label, "yours");
        assert_eq!(read.inbox.len(), 2);
        assert!(read
            .inbox
            .iter()
            .all(|message| message.owner_label == "not_yours"));
    }

    #[test]
    fn task_3074_tuta_shared_mailbox_reader_returns_seeded_scrub_mailbox() {
        let owner = "osl_task_3074_owner";
        let account = "acct-task-3074-tuta";
        let signed_in = "owner@tuta.example";
        let mailbox = MailboxReaderSnapshot::new(
            [
                MailboxFolderCandidate::new("Inbox", "Inbox"),
                MailboxFolderCandidate::new("Sent", "Sent"),
                MailboxFolderCandidate::new("Archive", "Archive"),
                MailboxFolderCandidate::new("Trash", "Trash"),
            ],
            [
                MailboxMessageCandidate::new(
                    "Sent",
                    "sent-task-3074-001",
                    "SCRUB-TU-MINE",
                    1_786_104_000_000,
                    signed_in,
                    "Tuta message that belongs to the signed-in mailbox.",
                ),
                MailboxMessageCandidate::new(
                    "Sent",
                    "sent-task-3074-002",
                    "SCRUB-TU-SENT-SECOND",
                    1_786_107_600_000,
                    signed_in.to_ascii_uppercase(),
                    "Second sent Tuta message.",
                ),
                MailboxMessageCandidate::new(
                    "Sent",
                    "sent-task-3074-003",
                    "SCRUB-TU-SENT-THIRD",
                    1_786_111_200_000,
                    "Owner@Tuta.Example",
                    "Third sent Tuta message.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "inbox-task-3074-001",
                    "SCRUB-TU-INBOX-FIRST",
                    1_786_114_800_000,
                    "friend-one@example.test",
                    "Inbox negative control one.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "inbox-task-3074-002",
                    "SCRUB-TU-INBOX-SECOND",
                    1_786_118_400_000,
                    "alerts@example.test",
                    "Inbox negative control two.",
                ),
            ],
        );

        let read = read_tuta_scrub_mailbox(owner, account, signed_in, &mailbox)
            .expect("seeded Tuta mailbox reads through the shared mailbox reader");
        println!("TASK3074_DIRECT_READER=tuta_shared_mailbox_reader");
        println!("TASK3074_FOLDER_COUNT={}", read.folders.len());
        for folder in &read.folders {
            println!(
                "TASK3074_FOLDER id=\"{}\" label=\"{}\" service={} account={}",
                folder.folder_id, folder.label, folder.service_id, folder.account_id
            );
        }
        println!("TASK3074_SENT_MESSAGE_COUNT={}", read.sent.len());
        for message in &read.sent {
            println!(
                "TASK3074_SENT_MESSAGE id={} subject=\"{}\" time={} sender=\"{}\" called={}",
                message.message_id,
                message.subject,
                message.time,
                message.sender,
                message.owner_label
            );
        }
        for message in &read.inbox {
            println!(
                "TASK3074_INBOX_NEGATIVE id={} subject=\"{}\" sender=\"{}\" called={}",
                message.message_id, message.subject, message.sender, message.owner_label
            );
        }
        let marked = read
            .sent
            .iter()
            .find(|message| message.subject == "SCRUB-TU-MINE")
            .expect("seeded marker is present in Sent");
        println!(
            "TASK3074_MARKED marker=SCRUB-TU-MINE called={}",
            marked.owner_label
        );
        println!(
            "TASK3074_INBOX_NOT_YOURS_COUNT={}",
            read.inbox
                .iter()
                .filter(|message| message.owner_label == "not_yours")
                .count()
        );

        assert_eq!(read.folders.len(), 4);
        assert_eq!(
            read.folders
                .iter()
                .map(|folder| folder.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Inbox", "Sent", "Archive", "Trash"]
        );
        assert_eq!(read.sent.len(), 3);
        assert_eq!(
            read.sent
                .iter()
                .map(|message| {
                    (
                        message.subject.as_str(),
                        message.time,
                        message.sender.as_str(),
                    )
                })
                .collect::<Vec<_>>(),
            vec![
                ("SCRUB-TU-MINE", 1_786_104_000_000, "owner@tuta.example",),
                (
                    "SCRUB-TU-SENT-SECOND",
                    1_786_107_600_000,
                    "OWNER@TUTA.EXAMPLE",
                ),
                (
                    "SCRUB-TU-SENT-THIRD",
                    1_786_111_200_000,
                    "Owner@Tuta.Example",
                ),
            ]
        );
        assert_eq!(marked.owner_label, "yours");
        assert_eq!(read.inbox.len(), 2);
        assert!(read
            .inbox
            .iter()
            .all(|message| message.owner_label == "not_yours"));
    }

    #[test]
    fn task_3054_outlook_desktop_pages_folder_with_pause_and_stops_during_page_two() {
        let owner = "osl_task_3054_owner";
        let account = "acct-task-3054-outlook";
        let folder = "Sent Items";
        let folders = [
            MailboxFolderCandidate::new("Inbox", "Inbox"),
            MailboxFolderCandidate::new(folder, "Sent Items"),
            MailboxFolderCandidate::new("Archive", "Archive"),
            MailboxFolderCandidate::new("Deleted Items", "Deleted Items"),
        ];
        let messages = (1..=120).map(|number| {
            MailboxMessageCandidate::new(
                folder,
                format!("outlook-desktop-page-3054-{number:03}"),
                format!("TASK3054 Outlook desktop message {number:03}"),
                1_786_204_800_000 + i64::from(number) * 1_000,
                "scrub.owner@example.test",
                format!("Seeded Outlook desktop paging body {number:03}."),
            )
        });
        let mailbox = MailboxReaderSnapshot::new(folders, messages);
        let request = SharedMailboxPagingRequest {
            page_size: 30,
            pause_between_pages_ms: 25,
            stop_requested_during_page: None,
        };

        let full =
            page_outlook_desktop_scrub_mailbox_folder(owner, account, folder, &mailbox, request)
                .expect("Outlook desktop folder pages through the shared mailbox helper");
        println!("TASK3054_DIRECT_READER=outlook-desktop-shared-mailbox-paging");
        println!("TASK3054_SHARED_HELPER=read_shared_mailbox_messages_paged");
        println!("TASK3054_SERVICE=outlook-desktop");
        println!("TASK3054_FOLDER_ID=\"{}\"", full.folder_id);
        println!("TASK3054_FOLDER_MESSAGE_COUNT=120");
        println!("TASK3054_SET_PAUSE_MS={}", full.pause_between_pages_ms);
        println!("TASK3054_FULL_READ_MESSAGE_COUNT={}", full.messages.len());
        println!("TASK3054_FULL_PAGE_COUNT={}", full.page_count);
        println!("TASK3054_FULL_PAGE_SIZE={}", full.page_size);
        println!("TASK3054_FULL_STOP_REASON={:?}", full.stop_reason);
        println!(
            "TASK3054_FULL_INTER_PAGE_GAPS_MS={:?}",
            full.inter_page_gaps_ms
        );
        println!(
            "TASK3054_FULL_EVERY_INTER_PAGE_GAP_AT_LEAST_SET_PAUSE={}",
            full.inter_page_gaps_ms
                .iter()
                .all(|gap| *gap >= full.pause_between_pages_ms)
        );

        let stopped = page_outlook_desktop_scrub_mailbox_folder(
            owner,
            account,
            folder,
            &mailbox,
            SharedMailboxPagingRequest {
                stop_requested_during_page: Some(2),
                ..request
            },
        )
        .expect("Outlook desktop stop request ends the shared paging run");
        println!(
            "TASK3054_STOP_REQUESTED_DURING_PAGE={}",
            stopped.stop_requested_during_page.unwrap_or_default()
        );
        println!("TASK3054_STOP_REASON={:?}", stopped.stop_reason);
        println!(
            "TASK3054_STOPPED_ON_PAGE_NUMBER={}",
            stopped.stopped_on_page_number.unwrap_or_default()
        );
        println!(
            "TASK3054_STOP_READ_MESSAGE_COUNT={}",
            stopped.messages.len()
        );
        println!("TASK3054_STOP_PAGE_COUNT={}", stopped.page_count);
        println!(
            "TASK3054_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
            (40..=80).contains(&stopped.messages.len())
        );
        println!(
            "TASK3054_STOP_INTER_PAGE_GAPS_MS={:?}",
            stopped.inter_page_gaps_ms
        );

        assert_eq!(full.folder_id, folder);
        assert_eq!(full.messages.len(), 120);
        assert_eq!(full.page_count, 4);
        assert!(full.page_count >= 3);
        assert_eq!(full.pause_between_pages_ms, 25);
        assert_eq!(full.inter_page_gaps_ms, vec![25, 25, 25]);
        assert_eq!(full.stop_reason, SharedMailboxPagingStopReason::EndOfFolder);
        assert_eq!(stopped.stop_requested_during_page, Some(2));
        assert_eq!(
            stopped.stop_reason,
            SharedMailboxPagingStopReason::StopRequested
        );
        assert_eq!(stopped.stopped_on_page_number, Some(2));
        assert_eq!(stopped.page_count, 2);
        assert_eq!(stopped.messages.len(), 60);
        assert!((40..=80).contains(&stopped.messages.len()));
        assert_eq!(stopped.inter_page_gaps_ms, vec![25]);
    }

    #[test]
    fn task_3069_maildotcom_pages_folder_with_pause_and_stops_during_page_two() {
        let owner = "osl_task_3069_owner";
        let account = "acct-task-3069-maildotcom";
        let folder = "Sent";
        let folders = [
            MailboxFolderCandidate::new("Inbox", "Inbox"),
            MailboxFolderCandidate::new(folder, "Sent"),
            MailboxFolderCandidate::new("Archive", "Archive"),
            MailboxFolderCandidate::new("Trash", "Trash"),
        ];
        let messages = (1..=120).map(|number| {
            MailboxMessageCandidate::new(
                folder,
                format!("maildotcom-page-3069-{number:03}"),
                format!("TASK3069 Mail.com message {number:03}"),
                1_786_208_400_000 + i64::from(number) * 1_000,
                "signed-in@mail.com",
                format!("Seeded Mail.com paging body {number:03}."),
            )
        });
        let mailbox = MailboxReaderSnapshot::new(folders, messages);
        let request = SharedMailboxPagingRequest {
            page_size: 30,
            pause_between_pages_ms: 25,
            stop_requested_during_page: None,
        };

        let full = page_maildotcom_scrub_mailbox_folder(owner, account, folder, &mailbox, request)
            .expect("Mail.com folder pages through the shared mailbox helper");
        println!("TASK3069_DIRECT_READER=maildotcom-shared-mailbox-paging");
        println!("TASK3069_SHARED_HELPER=read_shared_mailbox_messages_paged");
        println!("TASK3069_SERVICE=Mail.com");
        println!("TASK3069_FOLDER_ID=\"{}\"", full.folder_id);
        println!("TASK3069_FOLDER_MESSAGE_COUNT=120");
        println!("TASK3069_SET_PAUSE_MS={}", full.pause_between_pages_ms);
        println!("TASK3069_FULL_READ_MESSAGE_COUNT={}", full.messages.len());
        println!("TASK3069_FULL_PAGE_COUNT={}", full.page_count);
        println!("TASK3069_FULL_PAGE_SIZE={}", full.page_size);
        println!("TASK3069_FULL_STOP_REASON={:?}", full.stop_reason);
        println!(
            "TASK3069_FULL_INTER_PAGE_GAPS_MS={:?}",
            full.inter_page_gaps_ms
        );
        println!(
            "TASK3069_FULL_EVERY_INTER_PAGE_GAP_AT_LEAST_SET_PAUSE={}",
            full.inter_page_gaps_ms
                .iter()
                .all(|gap| *gap >= full.pause_between_pages_ms)
        );

        let stopped = page_maildotcom_scrub_mailbox_folder(
            owner,
            account,
            folder,
            &mailbox,
            SharedMailboxPagingRequest {
                stop_requested_during_page: Some(2),
                ..request
            },
        )
        .expect("Mail.com stop request ends the shared paging run");
        println!(
            "TASK3069_STOP_REQUESTED_DURING_PAGE={}",
            stopped.stop_requested_during_page.unwrap_or_default()
        );
        println!("TASK3069_STOP_REASON={:?}", stopped.stop_reason);
        println!(
            "TASK3069_STOPPED_ON_PAGE_NUMBER={}",
            stopped.stopped_on_page_number.unwrap_or_default()
        );
        println!(
            "TASK3069_STOP_READ_MESSAGE_COUNT={}",
            stopped.messages.len()
        );
        println!("TASK3069_STOP_PAGE_COUNT={}", stopped.page_count);
        println!(
            "TASK3069_STOP_MESSAGE_COUNT_BETWEEN_40_AND_80={}",
            (40..=80).contains(&stopped.messages.len())
        );
        println!(
            "TASK3069_STOP_INTER_PAGE_GAPS_MS={:?}",
            stopped.inter_page_gaps_ms
        );

        assert_eq!(full.service_id, "maildotcom");
        assert_eq!(full.folder_id, folder);
        assert_eq!(full.messages.len(), 120);
        assert_eq!(full.page_count, 4);
        assert!(full.page_count >= 3);
        assert_eq!(full.pause_between_pages_ms, 25);
        assert_eq!(full.inter_page_gaps_ms, vec![25, 25, 25]);
        assert_eq!(full.stop_reason, SharedMailboxPagingStopReason::EndOfFolder);
        assert_eq!(stopped.stop_requested_during_page, Some(2));
        assert_eq!(
            stopped.stop_reason,
            SharedMailboxPagingStopReason::StopRequested
        );
        assert_eq!(stopped.stopped_on_page_number, Some(2));
        assert_eq!(stopped.page_count, 2);
        assert_eq!(stopped.messages.len(), 60);
        assert!((40..=80).contains(&stopped.messages.len()));
        assert_eq!(stopped.inter_page_gaps_ms, vec![25]);
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
    fn task_1421_direct_command_with_two_accounts_reports_one_active_and_one_waiting() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let first = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "First".to_owned())
            .unwrap();
        let second = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Second".to_owned())
            .unwrap();
        let approved_in_unsaved_order = vec![second.id.clone(), first.id.clone()];

        let queue = state
            .run_queue_for_owner(OWNER_A, ServiceKind::Discord, &approved_in_unsaved_order)
            .unwrap();

        println!("task_1421_direct_command=list_service_account_run_queue");
        println!("task_1421_account_count={}", queue.accounts.len());
        println!("task_1421_active_count={}", queue.active_count);
        println!("task_1421_waiting_count={}", queue.waiting_count);
        println!(
            "task_1421_queue_statuses={}:active,{}:waiting",
            queue.accounts[0].account_id, queue.accounts[1].account_id
        );
        println!("task_1421_saved_order_preserved={},{}", first.id, second.id);

        assert_eq!(queue.accounts.len(), 2);
        assert_eq!(queue.active_count, 1);
        assert_eq!(queue.waiting_count, 1);
        assert_eq!(queue.accounts[0].account_id, first.id);
        assert_eq!(queue.accounts[0].status, ServiceAccountRunStatus::Active);
        assert_eq!(queue.accounts[1].account_id, second.id);
        assert_eq!(queue.accounts[1].status, ServiceAccountRunStatus::Waiting);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn task_1422_fake_run_log_records_scroll_wait_next_action_without_concurrency() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let first = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "First".to_owned())
            .unwrap();
        let second = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Second".to_owned())
            .unwrap();
        let queue = state
            .run_queue_for_owner(
                OWNER_A,
                ServiceKind::Discord,
                &[first.id.clone(), second.id.clone()],
            )
            .unwrap();

        let log = queue.paced_action_log_for_active_account().unwrap();
        let sequence = log
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>()
            .join(" -> ");

        println!("task_1422_fake_run_log_sequence={sequence}");
        println!(
            "task_1422_fake_run_log_scroll={}:{}:{}-{}",
            log.entries[0].account_id,
            log.entries[0].name,
            log.entries[0].start_tick,
            log.entries[0].end_tick
        );
        println!(
            "task_1422_fake_run_log_wait={}:{}ms:{}-{}",
            log.entries[1].name,
            log.entries[1].duration_ticks(),
            log.entries[1].start_tick,
            log.entries[1].end_tick
        );
        println!(
            "task_1422_fake_run_log_next_action={}:{}:{}-{}",
            log.entries[2].account_id,
            log.entries[2].name,
            log.entries[2].start_tick,
            log.entries[2].end_tick
        );
        println!(
            "task_1422_max_parallel_scroll_actions={}",
            log.max_parallel_scroll_actions
        );
        println!("task_1422_concurrent_actions={}", log.concurrent_actions);

        assert_eq!(log.account_id, first.id);
        assert_eq!(log.entries.len(), 3);
        assert_eq!(log.entries[0].action, ServiceAccountActionKind::Scroll);
        assert_eq!(log.entries[0].name, "scroll-message-list");
        assert_eq!(log.entries[1].action, ServiceAccountActionKind::Wait);
        assert_eq!(log.entries[1].name, PAUSE_AFTER_SCROLL_SCREEN_CHANGE);
        assert_eq!(
            log.entries[1].duration_ticks(),
            NORMAL_SCREEN_CHANGE_WAIT_MS
        );
        assert_eq!(
            log.entries[2].action,
            ServiceAccountActionKind::InspectVisibleRows
        );
        assert_eq!(log.entries[2].name, "inspect-visible-rows");
        assert_eq!(
            sequence,
            "scroll-message-list -> normal-screen-change-after-scroll -> inspect-visible-rows"
        );
        assert_eq!(log.max_parallel_scroll_actions, 1);
        assert_eq!(log.concurrent_actions, 0);
        assert_eq!(log.entries[0].end_tick, log.entries[1].start_tick);
        assert_eq!(log.entries[1].end_tick, log.entries[2].start_tick);
        let _ = fs::remove_file(path);
    }

    struct FixtureServiceConnection {
        service_id: ServiceKind,
        account_id: String,
        calls: std::sync::Mutex<Vec<ServiceMessagePlaceType>>,
    }

    impl FixtureServiceConnection {
        fn new(service_id: ServiceKind, account_id: String) -> Self {
            Self {
                service_id,
                account_id,
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn called_place_types(&self) -> Vec<ServiceMessagePlaceType> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl AccountServiceMessageConnection for FixtureServiceConnection {
        fn service_id(&self) -> ServiceKind {
            self.service_id
        }

        fn account_id(&self) -> &str {
            &self.account_id
        }

        fn read_messages(
            &self,
            place: &ServiceMessagePlace,
        ) -> Result<Vec<ServiceMessageSnapshot>, String> {
            self.calls.lock().unwrap().push(place.place_type);
            Ok(vec![ServiceMessageSnapshot {
                place_type: place.place_type,
                place_id: place.place_id.clone(),
                message_id: format!("fixture-{}-message", place.place_type.as_str()),
                author_id: "fixture-author".to_owned(),
                text: format!("real {} message", place.place_type.as_str()),
                date: "2026-08-06".to_owned(),
                time: "09:00".to_owned(),
            }])
        }
    }

    fn place_type_names(types: &[ServiceMessagePlaceType]) -> String {
        types
            .iter()
            .map(|place_type| place_type.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }

    struct FixtureRunnerServiceConnection {
        service_id: ServiceKind,
        account_id: String,
        requests: std::sync::Mutex<Vec<ServiceAccountFindingRuleRunRequest>>,
    }

    impl FixtureRunnerServiceConnection {
        fn new(service_id: ServiceKind, account_id: String) -> Self {
            Self {
                service_id,
                account_id,
                requests: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn recorded_requests(&self) -> Vec<ServiceAccountFindingRuleRunRequest> {
            self.requests.lock().unwrap().clone()
        }

        fn recorded_selected_rule_names(&self) -> Vec<String> {
            self.recorded_requests()
                .into_iter()
                .flat_map(|request| request.selected_rule_names)
                .collect()
        }
    }

    impl AccountServiceRunnerConnection for FixtureRunnerServiceConnection {
        fn service_id(&self) -> ServiceKind {
            self.service_id
        }

        fn account_id(&self) -> &str {
            &self.account_id
        }

        fn start_finding_rule_run(
            &self,
            request: &ServiceAccountFindingRuleRunRequest,
        ) -> Result<(), String> {
            self.requests.lock().unwrap().push(request.clone());
            Ok(())
        }
    }

    fn rule_names(names: &[String]) -> String {
        names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",")
    }

    struct PasswordFixtureServiceConnection {
        service_id: ServiceKind,
        account_id: String,
    }

    impl PasswordFixtureServiceConnection {
        fn new(service_id: ServiceKind, account_id: String) -> Self {
            Self {
                service_id,
                account_id,
            }
        }
    }

    impl AccountServiceMessageConnection for PasswordFixtureServiceConnection {
        fn service_id(&self) -> ServiceKind {
            self.service_id
        }

        fn account_id(&self) -> &str {
            &self.account_id
        }

        fn read_messages(
            &self,
            place: &ServiceMessagePlace,
        ) -> Result<Vec<ServiceMessageSnapshot>, String> {
            Ok(vec![
                ServiceMessageSnapshot {
                    place_type: place.place_type,
                    place_id: place.place_id.clone(),
                    message_id: "task-1425-password-message".to_owned(),
                    author_id: "fixture-author".to_owned(),
                    text: "password: correct horse battery staple".to_owned(),
                    date: "2026-08-06".to_owned(),
                    time: "09:30".to_owned(),
                },
                ServiceMessageSnapshot {
                    place_type: place.place_type,
                    place_id: place.place_id.clone(),
                    message_id: "task-1425-ordinary-message".to_owned(),
                    author_id: "fixture-author".to_owned(),
                    text: "Want to get coffee tomorrow?".to_owned(),
                    date: "2026-08-06".to_owned(),
                    time: "09:31".to_owned(),
                },
            ])
        }
    }

    struct CredentialAutomationBreakFixture {
        account_id: String,
        signed_in: std::sync::Mutex<bool>,
        saved_password: String,
        one_time_code: String,
        messages_read: std::sync::Mutex<usize>,
        credential_reads: std::sync::Mutex<usize>,
        credential_submits: std::sync::Mutex<usize>,
        message: ServiceMessageSnapshot,
    }

    impl CredentialAutomationBreakFixture {
        fn new(
            account_id: &str,
            signed_in: bool,
            saved_password: &str,
            one_time_code: &str,
        ) -> Self {
            Self {
                account_id: account_id.to_owned(),
                signed_in: std::sync::Mutex::new(signed_in),
                saved_password: saved_password.to_owned(),
                one_time_code: one_time_code.to_owned(),
                messages_read: std::sync::Mutex::new(0),
                credential_reads: std::sync::Mutex::new(0),
                credential_submits: std::sync::Mutex::new(0),
                message: ServiceMessageSnapshot {
                    place_type: ServiceMessagePlaceType::Dm,
                    place_id: "maple-dm".to_owned(),
                    message_id: "maple-mail".to_owned(),
                    author_id: "maple-author".to_owned(),
                    text: "MAPLE-4172".to_owned(),
                    date: "2026-08-06".to_owned(),
                    time: "10:43".to_owned(),
                },
            }
        }

        fn set_signed_in(&self, signed_in: bool) {
            *self.signed_in.lock().unwrap() = signed_in;
        }

        fn messages_read_count(&self) -> usize {
            *self.messages_read.lock().unwrap()
        }

        fn credential_read_count(&self) -> usize {
            *self.credential_reads.lock().unwrap()
        }

        fn credential_submit_count(&self) -> usize {
            *self.credential_submits.lock().unwrap()
        }

        fn saved_password_fixture_value(&self) -> &str {
            &self.saved_password
        }

        fn one_time_code_fixture_value(&self) -> &str {
            &self.one_time_code
        }

        fn read_saved_password_forbidden_to_runner(&self) -> String {
            *self.credential_reads.lock().unwrap() += 1;
            self.saved_password.clone()
        }

        fn submit_one_time_code_forbidden_to_runner(&self, code: &str) -> Result<(), String> {
            *self.credential_submits.lock().unwrap() += 1;
            if code == self.one_time_code {
                Ok(())
            } else {
                Err("fixture code mismatch".to_owned())
            }
        }
    }

    impl AccountServiceMessageConnection for CredentialAutomationBreakFixture {
        fn service_id(&self) -> ServiceKind {
            ServiceKind::Discord
        }

        fn account_id(&self) -> &str {
            &self.account_id
        }

        fn read_messages(
            &self,
            place: &ServiceMessagePlace,
        ) -> Result<Vec<ServiceMessageSnapshot>, String> {
            *self.messages_read.lock().unwrap() += 1;
            if place.place_type != self.message.place_type
                || place.place_id != self.message.place_id
            {
                return Ok(Vec::new());
            }
            Ok(vec![self.message.clone()])
        }
    }

    impl SignedInAccountServiceMessageConnection for CredentialAutomationBreakFixture {
        fn is_signed_in(&self) -> bool {
            *self.signed_in.lock().unwrap()
        }
    }

    fn single_discord_maple_queue() -> ServiceAccountRunQueue {
        ServiceAccountRunQueue {
            service_id: ServiceKind::Discord,
            accounts: vec![ServiceAccountRunQueueEntry {
                account_id: "discord-maple".to_owned(),
                label: "Maple".to_owned(),
                status: ServiceAccountRunStatus::Active,
            }],
            active_count: 1,
            waiting_count: 0,
        }
    }

    #[test]
    fn task_1423_service_connection_fixture_yields_messages_from_all_supplied_place_types() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let approved = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Approved".to_owned())
            .unwrap();
        let unapproved = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Unapproved".to_owned())
            .unwrap();
        let queue = state
            .run_queue_for_owner(OWNER_A, ServiceKind::Discord, &[approved.id.clone()])
            .unwrap();
        let supplied_types = [
            ServiceMessagePlaceType::Conversation,
            ServiceMessagePlaceType::Dm,
            ServiceMessagePlaceType::Group,
            ServiceMessagePlaceType::Server,
            ServiceMessagePlaceType::Channel,
            ServiceMessagePlaceType::Thread,
        ];
        let places = supplied_types
            .iter()
            .map(|place_type| ServiceMessagePlace {
                place_type: *place_type,
                place_id: format!("fixture-{}", place_type.as_str()),
            })
            .collect::<Vec<_>>();
        let connection = FixtureServiceConnection::new(ServiceKind::Discord, approved.id.clone());

        let batch =
            read_messages_through_approved_account_service_connection(&queue, &connection, &places)
                .unwrap();
        let called_types = connection.called_place_types();
        let yielded_types = batch
            .messages
            .iter()
            .map(|message| message.place_type)
            .collect::<Vec<_>>();
        let yielded_message_ids = batch
            .messages
            .iter()
            .map(|message| message.message_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let refused_connection =
            FixtureServiceConnection::new(ServiceKind::Discord, unapproved.id.clone());
        let unapproved_result = read_messages_through_approved_account_service_connection(
            &queue,
            &refused_connection,
            &places,
        );

        println!(
            "task_1423_reader_boundary=read_messages_through_approved_account_service_connection"
        );
        println!("task_1423_approved_account_id={}", batch.account_id);
        println!(
            "task_1423_supplied_place_types={}",
            place_type_names(&supplied_types)
        );
        println!(
            "task_1423_fixture_connection_called_place_types={}",
            place_type_names(&called_types)
        );
        println!(
            "task_1423_yielded_message_place_types={}",
            place_type_names(&yielded_types)
        );
        println!("task_1423_yielded_message_count={}", batch.message_count);
        println!("task_1423_yielded_message_ids={yielded_message_ids}");
        println!(
            "task_1423_unapproved_fixture_read_calls={}",
            refused_connection.called_place_types().len()
        );

        assert_eq!(batch.service_id, ServiceKind::Discord);
        assert_eq!(batch.account_id, approved.id);
        assert_eq!(batch.place_count, supplied_types.len());
        assert_eq!(batch.message_count, supplied_types.len());
        assert_eq!(called_types, supplied_types);
        assert_eq!(yielded_types, supplied_types);
        for (place, message) in places.iter().zip(batch.messages.iter()) {
            assert_eq!(message.place_type, place.place_type);
            assert_eq!(message.place_id, place.place_id);
            assert_eq!(
                message.text,
                format!("real {} message", place.place_type.as_str())
            );
        }
        assert!(unapproved_result.is_err());
        assert!(refused_connection.called_place_types().is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn task_1424_direct_run_reads_approved_fixture_account_and_records_selected_rule_names() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let approved = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Approved".to_owned())
            .unwrap();
        let unapproved = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Unapproved".to_owned())
            .unwrap();
        let queue = state
            .run_queue_for_owner(OWNER_A, ServiceKind::Discord, &[approved.id.clone()])
            .unwrap();
        let selected_rule_names = vec![
            "credential".to_owned(),
            "payment_card".to_owned(),
            "precise_location".to_owned(),
        ];
        let approved_connection =
            FixtureRunnerServiceConnection::new(ServiceKind::Discord, approved.id.clone());
        let unapproved_connection =
            FixtureRunnerServiceConnection::new(ServiceKind::Discord, unapproved.id.clone());

        let receipt = start_runner_for_approved_account_service_connection(
            &queue,
            &approved_connection,
            &selected_rule_names,
        )
        .unwrap();
        let unapproved_result = start_runner_for_approved_account_service_connection(
            &queue,
            &unapproved_connection,
            &selected_rule_names,
        );
        let recorded_requests = approved_connection.recorded_requests();
        let recorded_rule_names = approved_connection.recorded_selected_rule_names();
        let unapproved_requests = unapproved_connection.recorded_requests();

        println!("task_1424_direct_run=start_runner_for_approved_account_service_connection");
        println!(
            "task_1424_approved_fixture_account_id={}",
            receipt.account_id
        );
        println!(
            "task_1424_selected_rule_names={}",
            rule_names(&selected_rule_names)
        );
        println!(
            "task_1424_recorded_selected_rule_names={}",
            rule_names(&recorded_rule_names)
        );
        println!(
            "task_1424_recorded_request_account_id={}",
            recorded_requests[0].account_id
        );
        println!(
            "task_1424_selected_rule_count={}",
            receipt.selected_rule_count
        );
        println!(
            "task_1424_unapproved_fixture_start_calls={}",
            unapproved_requests.len()
        );

        assert_eq!(receipt.service_id, ServiceKind::Discord);
        assert_eq!(receipt.account_id, approved.id);
        assert_eq!(receipt.selected_rule_names, selected_rule_names);
        assert_eq!(receipt.selected_rule_count, 3);
        assert_eq!(recorded_requests.len(), 1);
        assert_eq!(recorded_requests[0].account_id, approved.id);
        assert_eq!(
            recorded_requests[0].selected_rule_names,
            selected_rule_names
        );
        assert_eq!(recorded_rule_names, selected_rule_names);
        assert!(unapproved_result.is_err());
        assert!(unapproved_requests.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn task_1425_fixture_password_message_yields_one_match_with_all_six_fields() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let approved = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Approved".to_owned())
            .unwrap();
        let queue = state
            .run_queue_for_owner(OWNER_A, ServiceKind::Discord, &[approved.id.clone()])
            .unwrap();
        let selected_rule_names = vec!["credential".to_owned()];
        let runner_connection =
            FixtureRunnerServiceConnection::new(ServiceKind::Discord, approved.id.clone());
        let receipt = start_runner_for_approved_account_service_connection(
            &queue,
            &runner_connection,
            &selected_rule_names,
        )
        .unwrap();
        let places = vec![ServiceMessagePlace {
            place_type: ServiceMessagePlaceType::Dm,
            place_id: "task-1425-dm".to_owned(),
        }];
        let message_connection =
            PasswordFixtureServiceConnection::new(ServiceKind::Discord, approved.id.clone());
        let batch = read_messages_through_approved_account_service_connection(
            &queue,
            &message_connection,
            &places,
        )
        .unwrap();

        let matches =
            find_possible_matches_in_read_messages(&batch, &receipt.selected_rule_names).unwrap();

        println!("task_1425_match_finder=find_possible_matches_in_read_messages");
        println!("task_1425_fixture_message_count={}", batch.message_count);
        println!("task_1425_match_count={}", matches.len());
        println!("task_1425_message_text={}", matches[0].message_text);
        println!("task_1425_reason={}", matches[0].reason);
        println!("task_1425_service={}", matches[0].service);
        println!("task_1425_place={}", matches[0].place);
        println!("task_1425_date={}", matches[0].date);
        println!("task_1425_time={}", matches[0].time);

        assert_eq!(batch.message_count, 2);
        assert_eq!(matches.len(), 1);
        let matched = &matches[0];
        assert_eq!(
            matched.message_text,
            "password: correct horse battery staple"
        );
        assert_eq!(
            matched.reason,
            "This looks like a password, API key, or access credential."
        );
        assert_eq!(matched.service, "Discord");
        assert_eq!(matched.place, "dm:task-1425-dm");
        assert_eq!(matched.date, "2026-08-06");
        assert_eq!(matched.time, "09:30");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn task_1443_logged_out_discord_maple_refuses_credential_automation() {
        let queue = single_discord_maple_queue();
        let fixture =
            CredentialAutomationBreakFixture::new("discord-maple", true, "MAPLE-4172", "417200");
        let places = vec![ServiceMessagePlace {
            place_type: ServiceMessagePlaceType::Dm,
            place_id: "maple-dm".to_owned(),
        }];

        assert_eq!(fixture.saved_password_fixture_value(), "MAPLE-4172");
        assert_eq!(fixture.one_time_code_fixture_value(), "417200");
        let scanned_before = fixture.messages_read_count();
        let credential_reads_before = fixture.credential_read_count();
        let credential_submits_before = fixture.credential_submit_count();

        let batch =
            read_messages_through_signed_in_account_service_connection(&queue, &fixture, &places)
                .unwrap();
        let message = batch.messages.first().unwrap();
        fixture.set_signed_in(false);
        let refused =
            read_messages_through_signed_in_account_service_connection(&queue, &fixture, &places)
                .unwrap_err();
        let scanned_after_logout_refusal = fixture.messages_read_count();
        let credential_reads_after = fixture.credential_read_count();
        let credential_submits_after = fixture.credential_submit_count();

        println!("task_1443_account_id={}", batch.account_id);
        println!("task_1443_saved_password_fixture=MAPLE-4172");
        println!("task_1443_saved_code_fixture=417200");
        println!("task_1443_scanned_message_count_before={scanned_before}");
        println!(
            "task_1443_scanned_message_count_after={}",
            batch.message_count
        );
        println!("task_1443_message_id={}", message.message_id);
        println!("task_1443_message_text={}", message.text);
        println!("task_1443_credential_reads={credential_reads_after}");
        println!("task_1443_credential_submits={credential_submits_after}");
        println!("task_1443_logged_out_refusal={refused}");
        println!("task_1443_after_logout_scanned_message_count={scanned_after_logout_refusal}");
        println!(
            "task_1443_after_logout_message_text={}",
            fixture.message.text
        );

        assert_eq!(batch.account_id, "discord-maple");
        assert_eq!(scanned_before, 0);
        assert_eq!(batch.message_count, 1);
        assert_eq!(message.message_id, "maple-mail");
        assert_eq!(message.text, "MAPLE-4172");
        assert_eq!(credential_reads_before, 0);
        assert_eq!(credential_submits_before, 0);
        assert_eq!(credential_reads_after, 0);
        assert_eq!(credential_submits_after, 0);
        assert_eq!(refused, "sign in yourself");
        assert_eq!(scanned_after_logout_refusal, 1);
        assert_eq!(fixture.message.text, "MAPLE-4172");
        assert_eq!(fixture.credential_read_count(), 0);
        assert_eq!(fixture.credential_submit_count(), 0);
        let _: fn(&CredentialAutomationBreakFixture) -> String =
            CredentialAutomationBreakFixture::read_saved_password_forbidden_to_runner;
        let _: fn(&CredentialAutomationBreakFixture, &str) -> Result<(), String> =
            CredentialAutomationBreakFixture::submit_one_time_code_forbidden_to_runner;
    }

    #[test]
    fn task_1437_finishing_fixture_discord_account_produces_no_match_and_match_wording() {
        let _serial = crate::global_keystore_test_lock();
        let path = temporary_registry();
        let state = ServiceRegistryState::load(path.clone());
        let first = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "First".to_owned())
            .unwrap();
        let second = state
            .create_for_owner(OWNER_A, ServiceKind::Discord, "Second".to_owned())
            .unwrap();
        let queue = state
            .run_queue_for_owner(
                OWNER_A,
                ServiceKind::Discord,
                &[first.id.clone(), second.id.clone()],
            )
            .unwrap();

        let no_match_notice = finish_active_service_account_run_notice(&queue, 0).unwrap();
        let match_notice = finish_active_service_account_run_notice(&queue, 2).unwrap();

        println!("task_1437_finish_action=finish_active_service_account_run_notice");
        println!("task_1437_notice_service={}", no_match_notice.service);
        println!(
            "task_1437_finished_fixture_account={}",
            no_match_notice.finished_account_label
        );
        println!("task_1437_no_match_count={}", no_match_notice.match_count);
        println!("task_1437_no_match_title={}", no_match_notice.title);
        println!("task_1437_no_match_detail={}", no_match_notice.detail);
        println!("task_1437_match_count={}", match_notice.match_count);
        println!("task_1437_match_title={}", match_notice.title);
        println!("task_1437_match_detail={}", match_notice.detail);
        println!(
            "task_1437_next_account={}",
            no_match_notice.next_account_label.as_deref().unwrap_or("")
        );

        assert_eq!(no_match_notice.service, "Discord");
        assert_eq!(no_match_notice.finished_account_id, first.id);
        assert_eq!(no_match_notice.finished_account_label, "First");
        assert_eq!(no_match_notice.match_count, 0);
        assert_eq!(
            no_match_notice.next_account_id.as_deref(),
            Some(second.id.as_str())
        );
        assert_eq!(
            no_match_notice.next_account_label.as_deref(),
            Some("Second")
        );
        assert_eq!(no_match_notice.title, "Discord scan finished");
        assert_eq!(
            no_match_notice.detail,
            "No matches found in Discord. Next account: Second."
        );
        assert_eq!(match_notice.service, "Discord");
        assert_eq!(match_notice.finished_account_id, first.id);
        assert_eq!(match_notice.finished_account_label, "First");
        assert_eq!(match_notice.match_count, 2);
        assert_eq!(
            match_notice.next_account_id.as_deref(),
            Some(second.id.as_str())
        );
        assert_eq!(match_notice.next_account_label.as_deref(), Some("Second"));
        assert_eq!(match_notice.title, "Discord scan finished");
        assert_eq!(
            match_notice.detail,
            "2 matches found in Discord. Next account: Second."
        );
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
