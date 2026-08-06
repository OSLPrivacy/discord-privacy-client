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

/// Local metadata for isolated service profiles. It intentionally stores no
/// credentials, cookies, tokens, claimed handles, or authentication state.
pub struct ServiceRegistryState {
    path: PathBuf,
    cache: Mutex<RegistryCache>,
    next_id: AtomicU64,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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
