//! Fail-closed native ledger for optional AutoScrub runs.
//!
//! This module is deliberately process-local. It records the reviewed scope of
//! a run, authorizes at most one already-reviewed deletion at a time, and clears
//! all authority when a run exits or is stopped.

use crate::models::ServiceKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use ipc::AppState;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const AUTOSCRUB_TIER_ID: &str = "autoscrub-own-session-v1";
pub const MAX_RUN_ID_LEN: usize = 64;
pub const MAX_ID_LEN: usize = 128;
pub const MAX_FINGERPRINT_LEN: usize = 128;
pub const MAX_ITEMS_PER_RUN: usize = 500;
pub const MAX_BATCHES_PER_RUN: usize = 64;
pub const MAX_ATTENDED_BATCH_ITEMS: usize = 100;
pub const MAX_CONCURRENT_RUNS: usize = 8;
pub const MANIFEST_LIFETIME: Duration = Duration::from_secs(1_800);
pub const RUN_CONSENT_LIFETIME: Duration = Duration::from_secs(300);
pub const MAX_RUN_DURATION: Duration = Duration::from_secs(900);
pub const PENDING_STEP_LIFETIME: Duration = Duration::from_secs(60);

const DOCUMENTED_PROVIDER_DELETE_API: &str = "documented_provider_delete_api";
const IMAP_PROVIDER_ID: &str = "imap";
const IMAP_SERVICE_ID: &str = "email";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeEntitlement {
    Confirmed,
    Absent,
}

impl NativeEntitlement {
    fn require(self) -> Result<(), AutoScrubRunError> {
        match self {
            Self::Confirmed => Ok(()),
            Self::Absent => Err(AutoScrubRunError::EntitlementRequired),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunPhaseDto {
    Active,
    Halted,
    Completed,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStatus {
    pub run_id: String,
    pub provider_id: String,
    pub account_id: String,
    pub phase: RunPhaseDto,
    pub halted_reason: Option<&'static str>,
    pub items_authorized: usize,
    pub items_remaining: usize,
    pub batches_touched: usize,
    pub consent_age_ms: u64,
    pub resumable: bool,
    pub detail: String,
}

impl RunStatus {
    pub fn validate_bounds(&self) -> Result<(), AutoScrubRunError> {
        validate_field(&self.run_id, MAX_RUN_ID_LEN)?;
        validate_field(&self.provider_id, MAX_ID_LEN)?;
        validate_field(&self.account_id, MAX_ID_LEN)?;
        if self.items_authorized > MAX_ITEMS_PER_RUN
            || self.items_remaining > MAX_ITEMS_PER_RUN
            || self.batches_touched > MAX_BATCHES_PER_RUN
        {
            return Err(AutoScrubRunError::BoundsExceeded);
        }
        validate_field(&self.detail, 256)?;
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFleetStatus {
    pub runs: Vec<RunStatus>,
    pub working: usize,
    pub total_runs: usize,
    pub run_capacity: usize,
    pub blocking_dry_run: Option<String>,
    pub detail: String,
}

impl RunFleetStatus {
    pub fn validate_bounds(&self) -> Result<(), AutoScrubRunError> {
        if self.runs.len() > self.run_capacity || self.total_runs > self.run_capacity {
            return Err(AutoScrubRunError::BoundsExceeded);
        }
        for run in &self.runs {
            run.validate_bounds()?;
        }
        if let Some(run_id) = &self.blocking_dry_run {
            validate_field(run_id, MAX_RUN_ID_LEN)?;
        }
        validate_field(&self.detail, 256)?;
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttendedImapReviewedItem {
    pub mailbox: String,
    pub message_id: String,
    pub content_fingerprint: String,
}

impl AttendedImapReviewedItem {
    pub fn key(&self) -> Result<String, AutoScrubRunError> {
        validate_field(&self.mailbox, MAX_ID_LEN)?;
        validate_field(&self.message_id, MAX_ID_LEN)?;
        validate_field(&self.content_fingerprint, MAX_FINGERPRINT_LEN)?;
        Ok(length_key([
            &self.mailbox,
            &self.message_id,
            &self.content_fingerprint,
        ]))
    }
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttendedImapBatchRequest {
    pub account_id: String,
    pub expected_auth_epoch: String,
    pub items: Vec<AttendedImapReviewedItem>,
}

impl AttendedImapBatchRequest {
    pub fn validate_bounds(&self) -> Result<(), AutoScrubRunError> {
        validate_field(&self.account_id, MAX_ID_LEN)?;
        validate_field(&self.expected_auth_epoch, MAX_ID_LEN)?;
        if self.items.is_empty() || self.items.len() > MAX_ATTENDED_BATCH_ITEMS {
            return Err(AutoScrubRunError::BoundsExceeded);
        }
        let mut seen = BTreeSet::new();
        for item in &self.items {
            if !seen.insert(item.key()?) {
                return Err(AutoScrubRunError::BoundsExceeded);
            }
        }
        Ok(())
    }
}

pub trait OwnedAccountRegistry {
    fn confirms_owned(&self, owner: &str, service: ServiceKind, account_id: &str) -> bool;
}

#[cfg(feature = "core")]
impl OwnedAccountRegistry for crate::services::ServiceRegistryState {
    fn confirms_owned(&self, owner: &str, service: ServiceKind, account_id: &str) -> bool {
        self.require_owned(owner, service, account_id).is_ok()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceObservation {
    OwnerOnSurface,
    Free,
    Unreadable,
}

impl SurfaceObservation {
    pub fn contends(self) -> bool {
        !matches!(self, Self::Free)
    }
}

pub trait RunSurfacePresence {
    fn observe_run_surface(&self, provider_id: &str, account_id: &str) -> SurfaceObservation;
}

impl RunSurfacePresence for crate::service_host::ServiceHostState {
    fn observe_run_surface(&self, provider_id: &str, account_id: &str) -> SurfaceObservation {
        use crate::service_host::ServiceHostPhase;

        let Some(service_id) = host_service_for_provider(provider_id) else {
            return SurfaceObservation::Unreadable;
        };
        let Ok(status) = self.status() else {
            return SurfaceObservation::Unreadable;
        };
        let visible = matches!(
            status.phase,
            ServiceHostPhase::Opening
                | ServiceHostPhase::Navigating
                | ServiceHostPhase::DocumentReady
        );
        if !visible {
            return SurfaceObservation::Free;
        }
        match status.active {
            Some(active)
                if active.generation != 0
                    && active.service_id == service_id
                    && active.account_id == account_id =>
            {
                SurfaceObservation::OwnerOnSurface
            }
            _ => SurfaceObservation::Free,
        }
    }
}

pub fn registry_service_for_provider(provider_id: &str) -> Option<ServiceKind> {
    match provider_id {
        IMAP_PROVIDER_ID | "gmail-web" => Some(ServiceKind::Email),
        "discord" => Some(ServiceKind::Discord),
        "telegram-web" => Some(ServiceKind::Telegram),
        _ => None,
    }
}

pub fn host_service_for_provider(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "gmail-web" => Some(IMAP_SERVICE_ID),
        "discord" => Some("discord"),
        "telegram-web" => Some("telegram"),
        _ => None,
    }
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedBatchInput {
    pub plan_fingerprint: String,
    pub findings_fingerprint: String,
    pub item_keys: Vec<String>,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunManifestInput {
    pub run_id: String,
    pub provider_id: String,
    pub account_id: String,
    pub reviewed_at: i64,
    pub expires_at: i64,
    pub tos_caveat_acknowledged: bool,
    pub unattended: bool,
    pub max_items: usize,
    pub batches: Vec<ReviewedBatchInput>,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnSessionProofInput {
    pub provider_id: String,
    pub account_id: String,
    pub session_kind: String,
    pub session_epoch: String,
    #[serde(default)]
    pub discovered_by_owner_detection: bool,
    #[serde(default)]
    pub credential_bound_to_osl_identity: Option<bool>,
    #[serde(default)]
    pub reused_osl_owned_profile: Option<bool>,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunConsentInput {
    pub run_id: String,
    pub provider_id: String,
    pub account_id: String,
    pub acknowledged_at: i64,
    pub tos_caveat_acknowledged: bool,
    pub unattended_acknowledged: bool,
    pub estimated_item_count: usize,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteCapabilityInput {
    pub mechanism: String,
    pub stop_on: Vec<String>,
}

#[derive(Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunStepInput {
    pub run_id: String,
    pub manifest_digest: String,
    pub consent_digest: String,
    pub plan_fingerprint: String,
    pub findings_fingerprint: String,
    pub channel_id: String,
    pub item_id: String,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOpened {
    pub run_id: String,
    pub manifest_digest: String,
    pub consent_digest: String,
    pub tier_id: &'static str,
    pub reviewed_items: usize,
    pub reviewed_batches: usize,
    pub max_items: usize,
    pub expires_in_ms: u64,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepAuthorization {
    pub run_id: String,
    pub channel_id: String,
    pub item_id: String,
    pub items_authorized: usize,
    pub items_remaining: usize,
    pub batches_touched: usize,
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapPrepareAuthorization {
    pub run_id: String,
    pub mailbox: String,
    pub message_id: String,
    pub source: &'static str,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AutoScrubRunError {
    EntitlementRequired,
    MechanismNotPermitted,
    StopConditionsNarrowed,
    SessionScopeInvalid,
    AccountNotOwned,
    ConsentRequired,
    ConsentStale,
    ConsentBindingDrifted,
    ManifestWindow,
    ManifestTampered,
    BoundsExceeded,
    RunAlreadyOpen,
    NoRun,
    WrongRun,
    RunAmbiguous,
    RunHalted,
    RunFinished,
    ItemNotReviewed,
    AuthorityRevoked,
    SnapshotCleanupFailed,
    StateUnavailable,
}

impl fmt::Debug for AutoScrubRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EntitlementRequired => "AutoScrubRunError::EntitlementRequired",
            Self::MechanismNotPermitted => "AutoScrubRunError::MechanismNotPermitted",
            Self::StopConditionsNarrowed => "AutoScrubRunError::StopConditionsNarrowed",
            Self::SessionScopeInvalid => "AutoScrubRunError::SessionScopeInvalid",
            Self::AccountNotOwned => "AutoScrubRunError::AccountNotOwned",
            Self::ConsentRequired => "AutoScrubRunError::ConsentRequired",
            Self::ConsentStale => "AutoScrubRunError::ConsentStale",
            Self::ConsentBindingDrifted => "AutoScrubRunError::ConsentBindingDrifted",
            Self::ManifestWindow => "AutoScrubRunError::ManifestWindow",
            Self::ManifestTampered => "AutoScrubRunError::ManifestTampered",
            Self::BoundsExceeded => "AutoScrubRunError::BoundsExceeded",
            Self::RunAlreadyOpen => "AutoScrubRunError::RunAlreadyOpen",
            Self::NoRun => "AutoScrubRunError::NoRun",
            Self::WrongRun => "AutoScrubRunError::WrongRun",
            Self::RunAmbiguous => "AutoScrubRunError::RunAmbiguous",
            Self::RunHalted => "AutoScrubRunError::RunHalted",
            Self::RunFinished => "AutoScrubRunError::RunFinished",
            Self::ItemNotReviewed => "AutoScrubRunError::ItemNotReviewed",
            Self::AuthorityRevoked => "AutoScrubRunError::AuthorityRevoked",
            Self::SnapshotCleanupFailed => "AutoScrubRunError::SnapshotCleanupFailed",
            Self::StateUnavailable => "AutoScrubRunError::StateUnavailable",
        })
    }
}

impl fmt::Display for AutoScrubRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EntitlementRequired => "AutoScrub entitlement is required",
            Self::MechanismNotPermitted => "delete mechanism is not permitted",
            Self::StopConditionsNarrowed => "stop conditions are narrower than required",
            Self::SessionScopeInvalid => "session scope is invalid",
            Self::AccountNotOwned => "account ownership is not confirmed",
            Self::ConsentRequired => "explicit consent is required",
            Self::ConsentStale => "consent is stale",
            Self::ConsentBindingDrifted => "consent no longer describes this run",
            Self::ManifestWindow => "manifest time window is invalid",
            Self::ManifestTampered => "manifest digest does not match",
            Self::BoundsExceeded => "AutoScrub bounds were exceeded",
            Self::RunAlreadyOpen => "run is already open",
            Self::NoRun => "no AutoScrub run is open",
            Self::WrongRun => "AutoScrub run is not owned by this identity",
            Self::RunAmbiguous => "AutoScrub status must name one run",
            Self::RunHalted => "AutoScrub run is halted",
            Self::RunFinished => "AutoScrub run is finished",
            Self::ItemNotReviewed => "item was not reviewed",
            Self::AuthorityRevoked => "delete authority was revoked",
            Self::SnapshotCleanupFailed => "snapshot cleanup failed",
            Self::StateUnavailable => "AutoScrub state is unavailable",
        })
    }
}

impl std::error::Error for AutoScrubRunError {}

#[derive(Default)]
pub struct AutoScrubRunState {
    ledger: Mutex<Ledger>,
}

#[derive(Default)]
struct Ledger {
    runs: BTreeMap<RunSlot, ActiveRun>,
    attended: BTreeMap<AttendedSlot, AttendedAuthorization>,
    snapshot_root: Option<PathBuf>,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct RunSlot {
    owner_scope: String,
    provider_id: String,
    account_id: String,
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
struct AttendedSlot {
    owner_scope: String,
    account_id: String,
}

struct ActiveRun {
    run_id: String,
    provider_id: String,
    account_id: String,
    manifest_digest: String,
    consent_digest: String,
    consent_recorded_at: Instant,
    deadline: Instant,
    batches: Vec<PinnedBatch>,
    reviewed_items: BTreeSet<String>,
    phase: RunPhase,
    authorized_items: BTreeSet<String>,
    touched_batches: BTreeSet<usize>,
    pending: Option<PendingStep>,
}

#[derive(Clone, Eq, PartialEq)]
struct PinnedBatch {
    plan_fingerprint: String,
    findings_fingerprint: String,
    item_keys: BTreeSet<String>,
}

struct PendingStep {
    mailbox: String,
    message_id: String,
    authorized_at: Instant,
}

struct AttendedAuthorization {
    remaining: usize,
    reviewed_items: BTreeSet<String>,
    auth_epoch: String,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RunPhase {
    Active,
    Halted(StopReason),
    Completed,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum StopReason {
    Owner,
    GlobalStop,
    Drift,
    Deadline,
}

impl StopReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner_stop",
            Self::GlobalStop => "global_stop",
            Self::Drift => "drift",
            Self::Deadline => "deadline",
        }
    }
}

impl AutoScrubRunState {
    pub fn open(
        &self,
        owner: &str,
        entitlement: NativeEntitlement,
        manifest: &RunManifestInput,
        session: &OwnSessionProofInput,
        consent: &RunConsentInput,
        capability: &DeleteCapabilityInput,
        registry: &dyn OwnedAccountRegistry,
    ) -> Result<RunOpened, AutoScrubRunError> {
        entitlement.require()?;
        validate_open_contract(owner, manifest, session, consent, capability, registry)?;
        let (batches, reviewed_items) = pin_batches(manifest)?;
        let manifest_digest = manifest_digest(manifest, &batches);
        let consent_digest = consent_digest(&manifest_digest, consent);
        let now_ms = now_unix_ms()?;
        let expires_in_ms = manifest
            .expires_at
            .saturating_sub(now_ms)
            .max(0)
            .min(MAX_RUN_DURATION.as_millis() as i64) as u64;
        let deadline = Instant::now() + Duration::from_millis(expires_in_ms);
        let scope = owner_scope(owner)?;
        let slot = RunSlot {
            owner_scope: scope,
            provider_id: manifest.provider_id.clone(),
            account_id: manifest.account_id.clone(),
        };
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        if ledger
            .runs
            .get(&slot)
            .is_some_and(|run| matches!(run.phase, RunPhase::Active))
        {
            return Err(AutoScrubRunError::RunAlreadyOpen);
        }
        if !ledger.runs.contains_key(&slot) && ledger.runs.len() >= MAX_CONCURRENT_RUNS {
            return Err(AutoScrubRunError::BoundsExceeded);
        }
        let opened = RunOpened {
            run_id: manifest.run_id.clone(),
            manifest_digest: manifest_digest.clone(),
            consent_digest: consent_digest.clone(),
            tier_id: AUTOSCRUB_TIER_ID,
            reviewed_items: reviewed_items.len(),
            reviewed_batches: batches.len(),
            max_items: manifest.max_items,
            expires_in_ms,
        };
        ledger.runs.insert(
            slot,
            ActiveRun {
                run_id: manifest.run_id.clone(),
                provider_id: manifest.provider_id.clone(),
                account_id: manifest.account_id.clone(),
                manifest_digest,
                consent_digest,
                consent_recorded_at: Instant::now(),
                deadline,
                batches,
                reviewed_items,
                phase: RunPhase::Active,
                authorized_items: BTreeSet::new(),
                touched_batches: BTreeSet::new(),
                pending: None,
            },
        );
        Ok(opened)
    }

    pub fn step(
        &self,
        owner: &str,
        entitlement: NativeEntitlement,
        request: &RunStepInput,
    ) -> Result<RunStepAuthorization, AutoScrubRunError> {
        entitlement.require()?;
        let scope = owner_scope(owner)?;
        let now = Instant::now();
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        Self::step_locked(&mut ledger, &scope, request, now)
    }

    fn step_locked(
        ledger: &mut Ledger,
        owner_scope: &str,
        request: &RunStepInput,
        now: Instant,
    ) -> Result<RunStepAuthorization, AutoScrubRunError> {
        let run = find_owned_run_mut(&mut ledger.runs, owner_scope, &request.run_id)?;
        match run.phase {
            RunPhase::Active => {}
            RunPhase::Halted(_) => return Err(AutoScrubRunError::RunHalted),
            RunPhase::Completed => return Err(AutoScrubRunError::RunFinished),
        }
        if run.manifest_digest != request.manifest_digest {
            run.finish(RunPhase::Halted(StopReason::Drift));
            return Err(AutoScrubRunError::ManifestTampered);
        }
        if run.consent_digest != request.consent_digest {
            run.finish(RunPhase::Halted(StopReason::Drift));
            return Err(AutoScrubRunError::ConsentBindingDrifted);
        }
        if now >= run.deadline {
            run.finish(RunPhase::Completed);
            return Err(AutoScrubRunError::RunFinished);
        }
        let key = item_key(&request.channel_id, &request.item_id);
        let Some(batch_index) = run.batches.iter().position(|batch| {
            batch.plan_fingerprint == request.plan_fingerprint
                && batch.findings_fingerprint == request.findings_fingerprint
                && batch.item_keys.contains(&key)
        }) else {
            return Err(AutoScrubRunError::ItemNotReviewed);
        };
        if !run.reviewed_items.contains(&key) {
            return Err(AutoScrubRunError::ItemNotReviewed);
        }
        run.authorized_items.insert(key);
        run.touched_batches.insert(batch_index);
        run.pending = Some(PendingStep {
            mailbox: request.channel_id.clone(),
            message_id: request.item_id.clone(),
            authorized_at: now,
        });
        Ok(RunStepAuthorization {
            run_id: run.run_id.clone(),
            channel_id: request.channel_id.clone(),
            item_id: request.item_id.clone(),
            items_authorized: run.authorized_items.len(),
            items_remaining: run
                .reviewed_items
                .len()
                .saturating_sub(run.authorized_items.len()),
            batches_touched: run.touched_batches.len(),
        })
    }

    pub fn halt(
        &self,
        owner: &str,
        run_id: &str,
        reason: &str,
    ) -> Result<RunStatus, AutoScrubRunError> {
        validate_field(run_id, MAX_RUN_ID_LEN)?;
        let scope = owner_scope(owner)?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        if !ledger.runs.keys().any(|slot| slot.owner_scope == scope) {
            return Err(AutoScrubRunError::NoRun);
        }
        let run = find_owned_run_mut(&mut ledger.runs, &scope, run_id)?;
        if matches!(run.phase, RunPhase::Active) {
            let stop = if reason == "global_stop" {
                StopReason::GlobalStop
            } else {
                StopReason::Owner
            };
            run.finish(RunPhase::Halted(stop));
        }
        Ok(run.status())
    }

    pub fn status(&self, owner: &str) -> Result<RunStatus, AutoScrubRunError> {
        let scope = owner_scope(owner)?;
        let ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        let mut owned = ledger
            .runs
            .iter()
            .filter(|(slot, _)| slot.owner_scope == scope);
        let first = owned.next().ok_or(AutoScrubRunError::NoRun)?;
        if owned.next().is_some() {
            return Err(AutoScrubRunError::RunAmbiguous);
        }
        Ok(first.1.status())
    }

    pub fn fleet_status(&self, owner: &str) -> Result<RunFleetStatus, AutoScrubRunError> {
        let scope = owner_scope(owner)?;
        let ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        let runs: Vec<RunStatus> = ledger
            .runs
            .iter()
            .filter(|(slot, _)| slot.owner_scope == scope)
            .map(|(_, run)| run.status())
            .collect();
        let working = runs
            .iter()
            .filter(|run| run.phase == RunPhaseDto::Active)
            .count();
        let blocking_dry_run = runs
            .iter()
            .rev()
            .find(|run| run.phase != RunPhaseDto::Active)
            .map(|run| run.run_id.clone());
        let detail = if runs.is_empty() {
            "No AutoScrub run is open."
        } else {
            "AutoScrub fleet status is bounded to this owner."
        };
        let total_runs = runs.len();
        let status = RunFleetStatus {
            runs,
            working,
            total_runs,
            run_capacity: MAX_CONCURRENT_RUNS,
            blocking_dry_run,
            detail: detail.to_owned(),
        };
        status.validate_bounds()?;
        Ok(status)
    }

    pub fn global_stop(&self, owner: &str) -> Result<RunFleetStatus, AutoScrubRunError> {
        let scope = owner_scope(owner)?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        let mut saw_owner_run = false;
        for (slot, run) in ledger.runs.iter_mut() {
            if slot.owner_scope == scope {
                saw_owner_run = true;
                run.finish(RunPhase::Halted(StopReason::GlobalStop));
            }
        }
        ledger.attended.retain(|slot, _| slot.owner_scope != scope);
        if !saw_owner_run {
            return Err(AutoScrubRunError::NoRun);
        }
        drop(ledger);
        self.fleet_status(owner)
    }

    pub fn authorize_imap_prepare(
        &self,
        owner: &str,
        entitlement: NativeEntitlement,
        account_id: &str,
        mailbox: &str,
        message_id: &str,
    ) -> Result<ImapPrepareAuthorization, AutoScrubRunError> {
        entitlement.require()?;
        validate_field(account_id, MAX_ID_LEN)?;
        let scope = owner_scope(owner)?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        let run = ledger
            .runs
            .iter_mut()
            .filter(|(slot, _)| slot.owner_scope == scope)
            .map(|(_, run)| run)
            .find(|run| run.provider_id == IMAP_PROVIDER_ID && run.account_id == account_id)
            .ok_or(AutoScrubRunError::NoRun)?;
        if !matches!(run.phase, RunPhase::Active) {
            return Err(AutoScrubRunError::RunHalted);
        }
        let Some(pending) = run.pending.take() else {
            return Err(AutoScrubRunError::AuthorityRevoked);
        };
        if pending.authorized_at.elapsed() > PENDING_STEP_LIFETIME {
            return Err(AutoScrubRunError::AuthorityRevoked);
        }
        if pending.mailbox != mailbox || pending.message_id != message_id {
            return Err(AutoScrubRunError::ItemNotReviewed);
        }
        Ok(ImapPrepareAuthorization {
            run_id: run.run_id.clone(),
            mailbox: mailbox.to_owned(),
            message_id: message_id.to_owned(),
            source: "autoscrub_run",
        })
    }

    pub fn authorize_attended_imap_batch_reviewed(
        &self,
        owner: &str,
        request: &AttendedImapBatchRequest,
    ) -> Result<(), AutoScrubRunError> {
        request.validate_bounds()?;
        let scope = owner_scope(owner)?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        if ledger.runs.keys().any(|slot| slot.owner_scope == scope) {
            return Err(AutoScrubRunError::RunAlreadyOpen);
        }
        let reviewed_items = request
            .items
            .iter()
            .map(AttendedImapReviewedItem::key)
            .collect::<Result<BTreeSet<_>, _>>()?;
        ledger.attended.insert(
            AttendedSlot {
                owner_scope: scope,
                account_id: request.account_id.clone(),
            },
            AttendedAuthorization {
                remaining: request.items.len(),
                reviewed_items,
                auth_epoch: request.expected_auth_epoch.clone(),
            },
        );
        Ok(())
    }

    pub fn authorize_reviewed_imap_prepare(
        &self,
        owner: &str,
        entitlement: NativeEntitlement,
        account_id: &str,
        mailbox: &str,
        message_id: &str,
        content_fingerprint: &str,
        expected_auth_epoch: &str,
    ) -> Result<ImapPrepareAuthorization, AutoScrubRunError> {
        entitlement.require()?;
        let scope = owner_scope(owner)?;
        let key = attended_imap_item_key(mailbox, message_id, content_fingerprint)?;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        if ledger.runs.keys().any(|slot| slot.owner_scope == scope) {
            return Err(AutoScrubRunError::RunAlreadyOpen);
        }
        let auth = ledger
            .attended
            .get_mut(&AttendedSlot {
                owner_scope: scope,
                account_id: account_id.to_owned(),
            })
            .ok_or(AutoScrubRunError::NoRun)?;
        if auth.auth_epoch != expected_auth_epoch || !auth.reviewed_items.remove(&key) {
            return Err(AutoScrubRunError::ItemNotReviewed);
        }
        if auth.remaining == 0 {
            return Err(AutoScrubRunError::AuthorityRevoked);
        }
        auth.remaining -= 1;
        Ok(ImapPrepareAuthorization {
            run_id: "attended-imap".to_owned(),
            mailbox: mailbox.to_owned(),
            message_id: message_id.to_owned(),
            source: "attended_review",
        })
    }

    pub fn set_snapshot_root(&self, root: PathBuf) -> Result<(), AutoScrubRunError> {
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        ledger.snapshot_root = Some(root);
        Ok(())
    }

    pub fn startup_cleanup(&self) -> Result<(), AutoScrubRunError> {
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| AutoScrubRunError::StateUnavailable)?;
        if let Some(root) = ledger.snapshot_root.take() {
            remove_snapshot_root(&root)?;
        }
        Ok(())
    }
}

impl ActiveRun {
    fn finish(&mut self, phase: RunPhase) {
        self.phase = phase;
        self.pending = None;
    }

    fn status(&self) -> RunStatus {
        let (phase, halted_reason, resumable) = match self.phase {
            RunPhase::Active => (RunPhaseDto::Active, None, true),
            RunPhase::Halted(reason) => (RunPhaseDto::Halted, Some(reason.as_str()), false),
            RunPhase::Completed => (RunPhaseDto::Completed, None, false),
        };
        RunStatus {
            run_id: self.run_id.clone(),
            provider_id: self.provider_id.clone(),
            account_id: self.account_id.clone(),
            phase,
            halted_reason,
            items_authorized: self.authorized_items.len(),
            items_remaining: self
                .reviewed_items
                .len()
                .saturating_sub(self.authorized_items.len()),
            batches_touched: self.touched_batches.len(),
            consent_age_ms: self.consent_recorded_at.elapsed().as_millis() as u64,
            resumable,
            detail: "AutoScrub run status is scoped to this owner.".to_owned(),
        }
    }
}

pub struct IsolatedRunEnvironment {
    root: PathBuf,
    memory: Vec<u8>,
    exited: bool,
}

impl IsolatedRunEnvironment {
    pub fn new(root: PathBuf, memory: Vec<u8>) -> Result<Self, AutoScrubRunError> {
        fs::create_dir_all(&root).map_err(|_| AutoScrubRunError::SnapshotCleanupFailed)?;
        Ok(Self {
            root,
            memory,
            exited: false,
        })
    }

    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    pub fn exit(&mut self) -> Result<(), AutoScrubRunError> {
        self.memory.fill(0);
        remove_snapshot_root(&self.root)?;
        self.exited = true;
        Ok(())
    }
}

impl Drop for IsolatedRunEnvironment {
    fn drop(&mut self) {
        if !self.exited {
            self.memory.fill(0);
            let _ = remove_snapshot_root(&self.root);
        }
    }
}

pub fn sync_snapshot_directory(root: &Path) -> Result<(), AutoScrubRunError> {
    if root.exists() {
        remove_snapshot_root(root)?;
    }
    fs::create_dir_all(root).map_err(|_| AutoScrubRunError::SnapshotCleanupFailed)
}

pub fn remove_snapshot_root(root: &Path) -> Result<(), AutoScrubRunError> {
    if !root.exists() {
        return Ok(());
    }
    let metadata =
        fs::symlink_metadata(root).map_err(|_| AutoScrubRunError::SnapshotCleanupFailed)?;
    if metadata.file_type().is_symlink() {
        return Err(AutoScrubRunError::SnapshotCleanupFailed);
    }
    fs::remove_dir_all(root).map_err(|_| AutoScrubRunError::SnapshotCleanupFailed)
}

fn validate_open_contract(
    owner: &str,
    manifest: &RunManifestInput,
    session: &OwnSessionProofInput,
    consent: &RunConsentInput,
    capability: &DeleteCapabilityInput,
    registry: &dyn OwnedAccountRegistry,
) -> Result<(), AutoScrubRunError> {
    validate_field(owner, MAX_ID_LEN)?;
    validate_field(&manifest.run_id, MAX_RUN_ID_LEN)?;
    validate_field(&manifest.provider_id, MAX_ID_LEN)?;
    validate_field(&manifest.account_id, MAX_ID_LEN)?;
    if capability.mechanism != DOCUMENTED_PROVIDER_DELETE_API {
        return Err(AutoScrubRunError::MechanismNotPermitted);
    }
    if !capability
        .stop_on
        .iter()
        .any(|condition| condition == "owner_stop")
        || !capability
            .stop_on
            .iter()
            .any(|condition| condition == "manifest_drift")
        || !capability
            .stop_on
            .iter()
            .any(|condition| condition == "deadline")
        || !capability
            .stop_on
            .iter()
            .any(|condition| condition == "global_stop")
    {
        return Err(AutoScrubRunError::StopConditionsNarrowed);
    }
    if !manifest.unattended || !manifest.tos_caveat_acknowledged {
        return Err(AutoScrubRunError::ConsentRequired);
    }
    if consent.run_id != manifest.run_id
        || consent.provider_id != manifest.provider_id
        || consent.account_id != manifest.account_id
        || !consent.tos_caveat_acknowledged
        || !consent.unattended_acknowledged
    {
        return Err(AutoScrubRunError::ConsentRequired);
    }
    let now_ms = now_unix_ms()?;
    if consent.acknowledged_at > now_ms
        || now_ms.saturating_sub(consent.acknowledged_at) > RUN_CONSENT_LIFETIME.as_millis() as i64
    {
        return Err(AutoScrubRunError::ConsentStale);
    }
    if manifest.reviewed_at < 0
        || manifest.expires_at <= now_ms
        || manifest.expires_at <= manifest.reviewed_at
        || manifest.expires_at.saturating_sub(manifest.reviewed_at)
            > MANIFEST_LIFETIME.as_millis() as i64
    {
        return Err(AutoScrubRunError::ManifestWindow);
    }
    if manifest.max_items == 0 || manifest.max_items > MAX_ITEMS_PER_RUN {
        return Err(AutoScrubRunError::BoundsExceeded);
    }
    if consent.estimated_item_count
        != manifest
            .batches
            .iter()
            .map(|batch| batch.item_keys.len())
            .sum::<usize>()
    {
        return Err(AutoScrubRunError::ConsentRequired);
    }
    if session.provider_id != manifest.provider_id
        || session.account_id != manifest.account_id
        || session.session_kind != "owner_provisioned_credential"
        || !session.discovered_by_owner_detection
        || session.credential_bound_to_osl_identity != Some(true)
        || session.reused_osl_owned_profile.is_some()
    {
        return Err(AutoScrubRunError::SessionScopeInvalid);
    }
    let service = registry_service_for_provider(&manifest.provider_id)
        .ok_or(AutoScrubRunError::SessionScopeInvalid)?;
    if !registry.confirms_owned(owner, service, &manifest.account_id) {
        return Err(AutoScrubRunError::AccountNotOwned);
    }
    Ok(())
}

fn validate_field(value: &str, max_len: usize) -> Result<(), AutoScrubRunError> {
    if value.is_empty() || value.len() > max_len {
        return Err(AutoScrubRunError::BoundsExceeded);
    }
    if value.chars().any(|character| {
        character.is_control()
            || ('\u{202a}'..='\u{202e}').contains(&character)
            || ('\u{2066}'..='\u{2069}').contains(&character)
    }) {
        return Err(AutoScrubRunError::BoundsExceeded);
    }
    Ok(())
}

fn pin_batches(
    manifest: &RunManifestInput,
) -> Result<(Vec<PinnedBatch>, BTreeSet<String>), AutoScrubRunError> {
    if manifest.batches.is_empty() || manifest.batches.len() > MAX_BATCHES_PER_RUN {
        return Err(AutoScrubRunError::BoundsExceeded);
    }
    let mut batches = Vec::with_capacity(manifest.batches.len());
    let mut all_items = BTreeSet::new();
    for batch in &manifest.batches {
        validate_field(&batch.plan_fingerprint, MAX_FINGERPRINT_LEN)?;
        validate_field(&batch.findings_fingerprint, MAX_FINGERPRINT_LEN)?;
        if batch.item_keys.is_empty() || batch.item_keys.len() > MAX_ITEMS_PER_RUN {
            return Err(AutoScrubRunError::BoundsExceeded);
        }
        let mut item_keys = BTreeSet::new();
        for key in &batch.item_keys {
            validate_field(key, MAX_ID_LEN * 2)?;
            if !item_keys.insert(key.clone()) {
                return Err(AutoScrubRunError::BoundsExceeded);
            }
            all_items.insert(key.clone());
        }
        batches.push(PinnedBatch {
            plan_fingerprint: batch.plan_fingerprint.clone(),
            findings_fingerprint: batch.findings_fingerprint.clone(),
            item_keys,
        });
    }
    if all_items.len() > manifest.max_items {
        return Err(AutoScrubRunError::BoundsExceeded);
    }
    Ok((batches, all_items))
}

fn manifest_digest(manifest: &RunManifestInput, batches: &[PinnedBatch]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"osl-autoscrub-manifest-v1");
    absorb(&mut digest, &manifest.run_id);
    absorb(&mut digest, &manifest.provider_id);
    absorb(&mut digest, &manifest.account_id);
    digest.update(manifest.reviewed_at.to_le_bytes());
    digest.update(manifest.expires_at.to_le_bytes());
    for batch in batches {
        absorb(&mut digest, &batch.plan_fingerprint);
        absorb(&mut digest, &batch.findings_fingerprint);
        for key in &batch.item_keys {
            absorb(&mut digest, key);
        }
    }
    format!("autoscrub-{:x}", digest.finalize())
}

fn consent_digest(manifest_digest: &str, consent: &RunConsentInput) -> String {
    let mut digest = Sha256::new();
    digest.update(b"osl-autoscrub-consent-v1");
    absorb(&mut digest, manifest_digest);
    absorb(&mut digest, &consent.run_id);
    absorb(&mut digest, &consent.provider_id);
    absorb(&mut digest, &consent.account_id);
    digest.update(consent.acknowledged_at.to_le_bytes());
    digest.update((consent.estimated_item_count as u64).to_le_bytes());
    format!("consent-{:x}", digest.finalize())
}

fn absorb(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_le_bytes());
    digest.update(value.as_bytes());
}

fn item_key(channel_id: &str, item_id: &str) -> String {
    length_key([channel_id, item_id])
}

fn attended_imap_item_key(
    mailbox: &str,
    message_id: &str,
    content_fingerprint: &str,
) -> Result<String, AutoScrubRunError> {
    AttendedImapReviewedItem {
        mailbox: mailbox.to_owned(),
        message_id: message_id.to_owned(),
        content_fingerprint: content_fingerprint.to_owned(),
    }
    .key()
}

fn length_key<const N: usize>(parts: [&str; N]) -> String {
    let mut key = String::new();
    for part in parts {
        key.push_str(&part.len().to_string());
        key.push(':');
        key.push_str(part);
    }
    key
}

fn owner_scope(owner: &str) -> Result<String, AutoScrubRunError> {
    validate_field(owner, MAX_ID_LEN)?;
    let mut digest = Sha256::new();
    digest.update(b"osl-autoscrub-owner-v1");
    digest.update(owner.as_bytes());
    Ok(format!("owner-{:x}", digest.finalize()))
}

fn now_unix_ms() -> Result<i64, AutoScrubRunError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AutoScrubRunError::StateUnavailable)?
        .as_millis();
    i64::try_from(millis).map_err(|_| AutoScrubRunError::StateUnavailable)
}

fn find_owned_run_mut<'a>(
    runs: &'a mut BTreeMap<RunSlot, ActiveRun>,
    owner_scope: &str,
    run_id: &str,
) -> Result<&'a mut ActiveRun, AutoScrubRunError> {
    let mut saw_owner_run = false;
    for (slot, run) in runs {
        if slot.owner_scope == owner_scope {
            saw_owner_run = true;
            if run.run_id == run_id {
                return Ok(run);
            }
        }
    }
    if saw_owner_run {
        Err(AutoScrubRunError::WrongRun)
    } else {
        Err(AutoScrubRunError::NoRun)
    }
}

const CONTRACT: &str = "autoscrubRunFleet.v1";
const PRO_REQUIRED: &str = "AutoScrub requires an active Pro license";
const MAX_OPEN_RUNS: usize = 2;
const MAX_ACCOUNT_ID_BYTES: usize = 64;
const MAX_REVIEW_TOKEN_BYTES: usize = 96;
const MAX_REVIEWED_ITEMS: u32 = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubRunPhase {
    ReviewRequired,
    Running,
    Stopping,
    Blocked,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubRunOutcome {
    None,
    Prepared,
    Confirmed,
    Held,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubQuitGuardState {
    NotRequested,
    Checking,
    Estimated,
    Stopped,
    Unknown,
    Refused,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubRunSummary {
    pub run_id: String,
    pub service_id: ServiceKind,
    pub phase: AutoScrubRunPhase,
    pub reviewed_item_count: u32,
    pub remaining_item_count: u32,
    pub stop_requested: bool,
    pub mutation_allowed: bool,
    pub last_outcome: AutoScrubRunOutcome,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubQuitGuardEstimate {
    pub state: AutoScrubQuitGuardState,
    pub honest_remaining_seconds_estimate: Option<u32>,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoScrubFleetStatus {
    pub contract: &'static str,
    pub open_run_count: usize,
    pub global_stop_requested: bool,
    pub unattended_execution_allowed: bool,
    pub quit_guard: AutoScrubQuitGuardEstimate,
    pub runs: Vec<AutoScrubRunSummary>,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum AutoScrubRunConsent {
    ReviewedBatchOnly,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoScrubReviewedRunRequest {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub review_token: String,
    pub plan_digest: String,
    pub reviewed_item_count: u32,
    pub consent: AutoScrubRunConsent,
}

#[derive(Default)]
struct AutoScrubRunStore {
    next_sequence: u64,
    runs: Vec<AutoScrubRunSummary>,
    global_stop_requested: bool,
}

static RUN_STORE: OnceLock<Mutex<AutoScrubRunStore>> = OnceLock::new();

fn run_store() -> &'static Mutex<AutoScrubRunStore> {
    RUN_STORE.get_or_init(|| Mutex::new(AutoScrubRunStore::default()))
}

#[cfg(test)]
fn reset_run_store_for_test() {
    *run_store().lock().expect("AutoScrub test run store lock") = AutoScrubRunStore::default();
}

pub fn fleet_status(state: &AppState) -> Result<AutoScrubFleetStatus, String> {
    require_pro(state)?;
    let store = run_store()
        .lock()
        .map_err(|_| "AutoScrub run store is unavailable".to_owned())?;
    Ok(store.fleet())
}

pub fn start_reviewed_run(
    state: &AppState,
    request: AutoScrubReviewedRunRequest,
) -> Result<AutoScrubFleetStatus, String> {
    require_pro(state)?;
    validate_reviewed_run_request(&request)?;
    let mut store = run_store()
        .lock()
        .map_err(|_| "AutoScrub run store is unavailable".to_owned())?;
    store.start_reviewed_run(request)
}

pub fn request_global_stop(state: &AppState) -> Result<AutoScrubFleetStatus, String> {
    require_pro(state)?;
    let mut store = run_store()
        .lock()
        .map_err(|_| "AutoScrub run store is unavailable".to_owned())?;
    Ok(store.request_global_stop())
}

fn require_pro(state: &AppState) -> Result<(), String> {
    if ipc::tier_gate::is_paid_equivalent(state) {
        Ok(())
    } else {
        Err(PRO_REQUIRED.to_owned())
    }
}

impl AutoScrubRunStore {
    fn start_reviewed_run(
        &mut self,
        request: AutoScrubReviewedRunRequest,
    ) -> Result<AutoScrubFleetStatus, String> {
        if self.open_runs() >= MAX_OPEN_RUNS {
            return Err("AutoScrub cannot open more than two reviewed runs".to_owned());
        }
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.runs.push(AutoScrubRunSummary {
            run_id: format!("autoscrub-run-{:04}", self.next_sequence),
            service_id: request.service_id,
            phase: AutoScrubRunPhase::Running,
            reviewed_item_count: request.reviewed_item_count,
            remaining_item_count: request.reviewed_item_count,
            stop_requested: false,
            mutation_allowed: false,
            last_outcome: AutoScrubRunOutcome::Held,
        });
        Ok(self.fleet())
    }

    fn request_global_stop(&mut self) -> AutoScrubFleetStatus {
        self.global_stop_requested = true;
        for run in &mut self.runs {
            if matches!(
                run.phase,
                AutoScrubRunPhase::Running | AutoScrubRunPhase::ReviewRequired
            ) {
                run.phase = AutoScrubRunPhase::Stopping;
            }
            run.stop_requested = true;
        }
        self.fleet()
    }

    fn fleet(&self) -> AutoScrubFleetStatus {
        let open_run_count = self.open_runs();
        AutoScrubFleetStatus {
            contract: CONTRACT,
            open_run_count,
            global_stop_requested: self.global_stop_requested,
            unattended_execution_allowed: false,
            quit_guard: self.quit_guard(open_run_count),
            runs: self.runs.clone(),
        }
    }

    fn open_runs(&self) -> usize {
        self.runs
            .iter()
            .filter(|run| {
                matches!(
                    run.phase,
                    AutoScrubRunPhase::ReviewRequired
                        | AutoScrubRunPhase::Running
                        | AutoScrubRunPhase::Stopping
                        | AutoScrubRunPhase::Blocked
                )
            })
            .count()
    }

    fn quit_guard(&self, open_run_count: usize) -> AutoScrubQuitGuardEstimate {
        if !self.global_stop_requested {
            return AutoScrubQuitGuardEstimate {
                state: AutoScrubQuitGuardState::NotRequested,
                honest_remaining_seconds_estimate: None,
                reason: "No stop request is active.",
            };
        }
        if open_run_count == 0 {
            return AutoScrubQuitGuardEstimate {
                state: AutoScrubQuitGuardState::Stopped,
                honest_remaining_seconds_estimate: None,
                reason: "No open AutoScrub run remains.",
            };
        }
        AutoScrubQuitGuardEstimate {
            state: AutoScrubQuitGuardState::Estimated,
            honest_remaining_seconds_estimate: Some(honest_stop_estimate_seconds(&self.runs)),
            reason: "OSL is stopping after the checked local items already in review.",
        }
    }
}

fn honest_stop_estimate_seconds(runs: &[AutoScrubRunSummary]) -> u32 {
    runs.iter()
        .filter(|run| run.stop_requested)
        .map(|run| run.remaining_item_count.max(1).saturating_mul(30))
        .max()
        .unwrap_or(30)
}

fn validate_reviewed_run_request(request: &AutoScrubReviewedRunRequest) -> Result<(), String> {
    if !valid_opaque(&request.account_id, MAX_ACCOUNT_ID_BYTES)
        || !valid_opaque(&request.review_token, MAX_REVIEW_TOKEN_BYTES)
        || !valid_digest(&request.plan_digest)
        || request.reviewed_item_count == 0
        || request.reviewed_item_count > MAX_REVIEWED_ITEMS
        || request.consent != AutoScrubRunConsent::ReviewedBatchOnly
    {
        return Err("AutoScrub reviewed run request is invalid".to_owned());
    }
    Ok(())
}

fn valid_opaque(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_host::ServiceHostState;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    const OWNER: &str = "owner-local";
    const OTHER_OWNER: &str = "owner-other";
    const ACCOUNT: &str = "acct-primary";
    const SECOND_ACCOUNT: &str = "acct-second";

    struct FixtureRegistry {
        allow: bool,
    }

    impl OwnedAccountRegistry for FixtureRegistry {
        fn confirms_owned(&self, _owner: &str, _service: ServiceKind, _account_id: &str) -> bool {
            self.allow
        }
    }

    fn registry() -> FixtureRegistry {
        FixtureRegistry { allow: true }
    }

    fn now_ms() -> i64 {
        now_unix_ms().unwrap()
    }

    fn manifest(run_id: &str, account_id: &str) -> RunManifestInput {
        let now = now_ms();
        RunManifestInput {
            run_id: run_id.to_owned(),
            provider_id: IMAP_PROVIDER_ID.to_owned(),
            account_id: account_id.to_owned(),
            reviewed_at: now - 1_000,
            expires_at: now + 120_000,
            tos_caveat_acknowledged: true,
            unattended: true,
            max_items: 4,
            batches: vec![ReviewedBatchInput {
                plan_fingerprint: "plan-a".to_owned(),
                findings_fingerprint: "findings-a".to_owned(),
                item_keys: vec![item_key("INBOX", "<a@x>")],
            }],
        }
    }

    fn session(account_id: &str) -> OwnSessionProofInput {
        OwnSessionProofInput {
            provider_id: IMAP_PROVIDER_ID.to_owned(),
            account_id: account_id.to_owned(),
            session_kind: "owner_provisioned_credential".to_owned(),
            session_epoch: "epoch-1".to_owned(),
            discovered_by_owner_detection: true,
            credential_bound_to_osl_identity: Some(true),
            reused_osl_owned_profile: None,
        }
    }

    fn consent(run_id: &str, account_id: &str) -> RunConsentInput {
        RunConsentInput {
            run_id: run_id.to_owned(),
            provider_id: IMAP_PROVIDER_ID.to_owned(),
            account_id: account_id.to_owned(),
            acknowledged_at: now_ms() - 1_000,
            tos_caveat_acknowledged: true,
            unattended_acknowledged: true,
            estimated_item_count: 1,
        }
    }

    fn capability() -> DeleteCapabilityInput {
        DeleteCapabilityInput {
            mechanism: DOCUMENTED_PROVIDER_DELETE_API.to_owned(),
            stop_on: vec![
                "owner_stop".to_owned(),
                "manifest_drift".to_owned(),
                "deadline".to_owned(),
                "global_stop".to_owned(),
            ],
        }
    }

    fn open_run(ledger: &AutoScrubRunState, run_id: &str, account_id: &str) -> RunOpened {
        open_run_for(ledger, OWNER, run_id, account_id)
    }

    fn open_run_for(
        ledger: &AutoScrubRunState,
        owner: &str,
        run_id: &str,
        account_id: &str,
    ) -> RunOpened {
        ledger
            .open(
                owner,
                NativeEntitlement::Confirmed,
                &manifest(run_id, account_id),
                &session(account_id),
                &consent(run_id, account_id),
                &capability(),
                &registry(),
            )
            .unwrap()
    }

    fn step_for(opened: &RunOpened) -> RunStepInput {
        RunStepInput {
            run_id: opened.run_id.clone(),
            manifest_digest: opened.manifest_digest.clone(),
            consent_digest: opened.consent_digest.clone(),
            plan_fingerprint: "plan-a".to_owned(),
            findings_fingerprint: "findings-a".to_owned(),
            channel_id: "INBOX".to_owned(),
            item_id: "<a@x>".to_owned(),
        }
    }

    fn temp_root(prefix: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn autoscrub_run_status_dtos_are_bounded_and_exact() {
        let status = RunStatus {
            run_id: "run-a".to_owned(),
            provider_id: IMAP_PROVIDER_ID.to_owned(),
            account_id: ACCOUNT.to_owned(),
            phase: RunPhaseDto::Active,
            halted_reason: None,
            items_authorized: 1,
            items_remaining: 2,
            batches_touched: 1,
            consent_age_ms: 42,
            resumable: true,
            detail: "scoped".to_owned(),
        };
        status.validate_bounds().unwrap();
        let encoded = serde_json::to_value(&status).unwrap();
        assert_eq!(
            encoded,
            json!({
                "runId": "run-a",
                "providerId": "imap",
                "accountId": "acct-primary",
                "phase": "active",
                "haltedReason": null,
                "itemsAuthorized": 1,
                "itemsRemaining": 2,
                "batchesTouched": 1,
                "consentAgeMs": 42,
                "resumable": true,
                "detail": "scoped"
            })
        );

        let fleet = RunFleetStatus {
            runs: vec![status],
            working: 1,
            total_runs: 1,
            run_capacity: 2,
            blocking_dry_run: None,
            detail: "fleet".to_owned(),
        };
        fleet.validate_bounds().unwrap();
        assert_eq!(
            serde_json::to_value(&fleet).unwrap(),
            json!({
                "runs": [{
                    "runId": "run-a",
                    "providerId": "imap",
                    "accountId": "acct-primary",
                    "phase": "active",
                    "haltedReason": null,
                    "itemsAuthorized": 1,
                    "itemsRemaining": 2,
                    "batchesTouched": 1,
                    "consentAgeMs": 42,
                    "resumable": true,
                    "detail": "scoped"
                }],
                "working": 1,
                "totalRuns": 1,
                "runCapacity": 2,
                "blockingDryRun": null,
                "detail": "fleet"
            })
        );

        let oversized = AttendedImapBatchRequest {
            account_id: ACCOUNT.to_owned(),
            expected_auth_epoch: "epoch-1".to_owned(),
            items: (0..=MAX_ATTENDED_BATCH_ITEMS)
                .map(|index| AttendedImapReviewedItem {
                    mailbox: "INBOX".to_owned(),
                    message_id: format!("<{index}@x>"),
                    content_fingerprint: format!("sha256:{index}"),
                })
                .collect(),
        };
        assert!(matches!(
            oversized.validate_bounds(),
            Err(AutoScrubRunError::BoundsExceeded)
        ));
        let exact = AttendedImapBatchRequest {
            account_id: ACCOUNT.to_owned(),
            expected_auth_epoch: "epoch-1".to_owned(),
            items: vec![AttendedImapReviewedItem {
                mailbox: "INBOX".to_owned(),
                message_id: "<a@x>".to_owned(),
                content_fingerprint: "sha256:a".to_owned(),
            }],
        };
        exact.validate_bounds().unwrap();
    }

    #[test]
    fn owned_account_registry_and_run_surface_presence_are_implemented() {
        assert!(registry().confirms_owned(OWNER, ServiceKind::Email, ACCOUNT));
        assert_eq!(
            registry_service_for_provider(IMAP_PROVIDER_ID),
            Some(ServiceKind::Email)
        );

        let host = ServiceHostState::default();
        let opened = host
            .begin_open("owner-scope", IMAP_SERVICE_ID, ACCOUNT, "mail.google.com")
            .unwrap();
        host.activate(IMAP_SERVICE_ID, ACCOUNT, opened.generation)
            .unwrap();
        assert_eq!(
            host.observe_run_surface("gmail-web", ACCOUNT),
            SurfaceObservation::OwnerOnSurface
        );
        assert_eq!(
            host.observe_run_surface("gmail-web", SECOND_ACCOUNT),
            SurfaceObservation::Free
        );
        assert_eq!(
            host.observe_run_surface(IMAP_PROVIDER_ID, ACCOUNT),
            SurfaceObservation::Unreadable
        );
        host.suspend().unwrap();
        assert_eq!(
            host.observe_run_surface("gmail-web", ACCOUNT),
            SurfaceObservation::Free
        );
    }

    #[test]
    fn autoscrub_run_state_open_validates_entitlement_contract_and_session_scope() {
        let ledger = AutoScrubRunState::default();
        let m = manifest("run-a", ACCOUNT);
        let s = session(ACCOUNT);
        let c = consent("run-a", ACCOUNT);
        let cap = capability();

        assert!(matches!(
            ledger.open(
                OWNER,
                NativeEntitlement::Absent,
                &m,
                &s,
                &c,
                &cap,
                &registry()
            ),
            Err(AutoScrubRunError::EntitlementRequired)
        ));
        let mut bad_cap = cap.clone();
        bad_cap.mechanism = "browser_ui_automation".to_owned();
        assert!(matches!(
            ledger.open(
                OWNER,
                NativeEntitlement::Confirmed,
                &m,
                &s,
                &c,
                &bad_cap,
                &registry()
            ),
            Err(AutoScrubRunError::MechanismNotPermitted)
        ));
        let mut missing_global_stop = cap.clone();
        missing_global_stop
            .stop_on
            .retain(|condition| condition != "global_stop");
        assert!(matches!(
            ledger.open(
                OWNER,
                NativeEntitlement::Confirmed,
                &m,
                &s,
                &c,
                &missing_global_stop,
                &registry()
            ),
            Err(AutoScrubRunError::StopConditionsNarrowed)
        ));
        let mut bad_session = s.clone();
        bad_session.account_id = SECOND_ACCOUNT.to_owned();
        assert!(matches!(
            ledger.open(
                OWNER,
                NativeEntitlement::Confirmed,
                &m,
                &bad_session,
                &c,
                &cap,
                &registry()
            ),
            Err(AutoScrubRunError::SessionScopeInvalid)
        ));
        assert!(matches!(
            ledger.open(
                OWNER,
                NativeEntitlement::Confirmed,
                &m,
                &s,
                &c,
                &cap,
                &FixtureRegistry { allow: false }
            ),
            Err(AutoScrubRunError::AccountNotOwned)
        ));
        ledger
            .open(
                OWNER,
                NativeEntitlement::Confirmed,
                &m,
                &s,
                &c,
                &cap,
                &registry(),
            )
            .unwrap();
    }

    #[test]
    fn authorize_imap_prepare_refuses_no_run_bypass() {
        let ledger = AutoScrubRunState::default();
        assert!(matches!(
            ledger.authorize_imap_prepare(
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                "INBOX",
                "<a@x>"
            ),
            Err(AutoScrubRunError::NoRun)
        ));
        assert!(matches!(
            ledger.authorize_reviewed_imap_prepare(
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                "INBOX",
                "<a@x>",
                "sha256:a",
                "epoch-1"
            ),
            Err(AutoScrubRunError::NoRun)
        ));

        let request = AttendedImapBatchRequest {
            account_id: ACCOUNT.to_owned(),
            expected_auth_epoch: "epoch-1".to_owned(),
            items: vec![AttendedImapReviewedItem {
                mailbox: "INBOX".to_owned(),
                message_id: "<a@x>".to_owned(),
                content_fingerprint: "sha256:a".to_owned(),
            }],
        };
        ledger
            .authorize_attended_imap_batch_reviewed(OWNER, &request)
            .unwrap();
        ledger
            .authorize_reviewed_imap_prepare(
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                "INBOX",
                "<a@x>",
                "sha256:a",
                "epoch-1",
            )
            .unwrap();
    }

    #[test]
    fn autoscrub_run_wipes_isolated_environment_after_each_exit() {
        let root = temp_root("autoscrub-env");
        let secret_file = root.join("session.bin");
        let mut env =
            IsolatedRunEnvironment::new(root.clone(), b"session-secret".to_vec()).unwrap();
        fs::write(&secret_file, b"disk-secret").unwrap();
        assert!(secret_file.exists());
        assert!(env.memory().iter().any(|byte| *byte != 0));

        env.exit().unwrap();

        assert!(!root.exists());
        assert!(env.memory().iter().all(|byte| *byte == 0));
    }

    #[test]
    fn halt_fails_closed_when_owner_has_zero_open_runs() {
        let ledger = AutoScrubRunState::default();
        let opened = open_run(&ledger, "run-a", ACCOUNT);
        assert!(matches!(
            ledger.halt(OTHER_OWNER, &opened.run_id, "owner_stop"),
            Err(AutoScrubRunError::NoRun)
        ));
        assert_eq!(
            ledger.status(OWNER).unwrap().phase,
            RunPhaseDto::Active,
            "another owner's no-run halt must not affect the existing run"
        );
    }

    #[test]
    fn autoscrub_status_refuses_ambiguous_runs_and_fleet_status_lists_latest() {
        let ledger = AutoScrubRunState::default();
        open_run(&ledger, "run-a", ACCOUNT);
        open_run(&ledger, "run-b", SECOND_ACCOUNT);

        assert!(matches!(
            ledger.status(OWNER),
            Err(AutoScrubRunError::RunAmbiguous)
        ));
        ledger.halt(OWNER, "run-b", "owner_stop").unwrap();
        let fleet = ledger.fleet_status(OWNER).unwrap();
        assert_eq!(
            fleet
                .runs
                .iter()
                .map(|run| run.run_id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-a", "run-b"]
        );
        assert_eq!(fleet.blocking_dry_run.as_deref(), Some("run-b"));
        assert_eq!(fleet.working, 1);
    }

    #[test]
    fn global_stop_halts_every_open_run_and_revokes_authority() {
        let ledger = AutoScrubRunState::default();
        let opened_a = open_run(&ledger, "run-a", ACCOUNT);
        let opened_b = open_run(&ledger, "run-b", SECOND_ACCOUNT);
        ledger
            .step(OWNER, NativeEntitlement::Confirmed, &step_for(&opened_a))
            .unwrap();

        let fleet = ledger.global_stop(OWNER).unwrap();
        assert_eq!(fleet.working, 0);
        assert!(fleet.runs.iter().all(
            |run| run.phase == RunPhaseDto::Halted && run.halted_reason == Some("global_stop")
        ));
        assert!(matches!(
            ledger.authorize_imap_prepare(
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                "INBOX",
                "<a@x>"
            ),
            Err(AutoScrubRunError::RunHalted)
        ));
        assert!(matches!(
            ledger.step(OWNER, NativeEntitlement::Confirmed, &step_for(&opened_b)),
            Err(AutoScrubRunError::RunHalted)
        ));
    }

    #[test]
    fn full_autoscrub_native_authority_acceptance() {
        let ledger = AutoScrubRunState::default();
        let opened_a = open_run(&ledger, "run-a", ACCOUNT);
        let opened_b = open_run(&ledger, "run-b", SECOND_ACCOUNT);
        open_run_for(&ledger, OTHER_OWNER, "run-c", ACCOUNT);

        assert!(matches!(
            ledger.open(
                OWNER,
                NativeEntitlement::Confirmed,
                &manifest("run-a-duplicate", ACCOUNT),
                &session(ACCOUNT),
                &consent("run-a-duplicate", ACCOUNT),
                &capability(),
                &registry()
            ),
            Err(AutoScrubRunError::RunAlreadyOpen)
        ));
        assert!(matches!(
            ledger.step(OWNER, NativeEntitlement::Absent, &step_for(&opened_a)),
            Err(AutoScrubRunError::EntitlementRequired)
        ));

        let first_step = ledger
            .step(OWNER, NativeEntitlement::Confirmed, &step_for(&opened_a))
            .unwrap();
        assert_eq!(first_step.run_id, "run-a");
        assert_eq!(first_step.items_authorized, 1);
        assert!(matches!(
            ledger.authorize_imap_prepare(
                OWNER,
                NativeEntitlement::Absent,
                ACCOUNT,
                "INBOX",
                "<a@x>"
            ),
            Err(AutoScrubRunError::EntitlementRequired)
        ));
        let prepare = ledger
            .authorize_imap_prepare(
                OWNER,
                NativeEntitlement::Confirmed,
                ACCOUNT,
                "INBOX",
                "<a@x>",
            )
            .unwrap();
        assert_eq!(prepare.run_id, "run-a");
        assert_eq!(prepare.mailbox, "INBOX");
        assert_eq!(prepare.message_id, "<a@x>");
        assert_eq!(prepare.source, "autoscrub_run");
        ledger
            .step(OWNER, NativeEntitlement::Confirmed, &step_for(&opened_b))
            .unwrap();

        assert!(matches!(
            ledger.status(OWNER),
            Err(AutoScrubRunError::RunAmbiguous)
        ));
        let owner_fleet = ledger.fleet_status(OWNER).unwrap();
        assert_eq!(owner_fleet.working, 2);
        assert_eq!(owner_fleet.total_runs, 2);
        assert_eq!(
            owner_fleet
                .runs
                .iter()
                .map(|run| run.run_id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-a", "run-b"]
        );

        let stopped = ledger.global_stop(OWNER).unwrap();
        assert_eq!(stopped.working, 0);
        assert_eq!(stopped.total_runs, 2);
        assert!(stopped.runs.iter().all(
            |run| run.phase == RunPhaseDto::Halted && run.halted_reason == Some("global_stop")
        ));
        assert!(matches!(
            ledger.authorize_imap_prepare(
                OWNER,
                NativeEntitlement::Confirmed,
                SECOND_ACCOUNT,
                "INBOX",
                "<a@x>"
            ),
            Err(AutoScrubRunError::RunHalted)
        ));
        assert!(matches!(
            ledger.step(OWNER, NativeEntitlement::Confirmed, &step_for(&opened_a)),
            Err(AutoScrubRunError::RunHalted)
        ));

        let other_fleet = ledger.fleet_status(OTHER_OWNER).unwrap();
        assert_eq!(other_fleet.working, 1);
        assert_eq!(other_fleet.total_runs, 1);
        assert_eq!(other_fleet.runs[0].run_id, "run-c");
        assert_eq!(other_fleet.runs[0].phase, RunPhaseDto::Active);
    }

    #[test]
    fn step_locked_halts_on_manifest_digest_consent_drift_or_deadline() {
        let ledger = AutoScrubRunState::default();
        let opened = open_run(&ledger, "run-a", ACCOUNT);
        let mut bad_manifest = step_for(&opened);
        bad_manifest.manifest_digest.push_str("-changed");
        assert!(matches!(
            ledger.step(OWNER, NativeEntitlement::Confirmed, &bad_manifest),
            Err(AutoScrubRunError::ManifestTampered)
        ));
        assert_eq!(ledger.status(OWNER).unwrap().halted_reason, Some("drift"));

        let consent_ledger = AutoScrubRunState::default();
        let opened = open_run(&consent_ledger, "run-b", ACCOUNT);
        let mut bad_consent = step_for(&opened);
        bad_consent.consent_digest.push_str("-changed");
        assert!(matches!(
            consent_ledger.step(OWNER, NativeEntitlement::Confirmed, &bad_consent),
            Err(AutoScrubRunError::ConsentBindingDrifted)
        ));
        assert_eq!(
            consent_ledger.status(OWNER).unwrap().halted_reason,
            Some("drift")
        );

        let expired = AutoScrubRunState::default();
        let mut m = manifest("run-c", ACCOUNT);
        m.expires_at = now_ms() + 250;
        let opened = expired
            .open(
                OWNER,
                NativeEntitlement::Confirmed,
                &m,
                &session(ACCOUNT),
                &consent("run-c", ACCOUNT),
                &capability(),
                &registry(),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(matches!(
            expired.step(OWNER, NativeEntitlement::Confirmed, &step_for(&opened)),
            Err(AutoScrubRunError::RunFinished)
        ));
        assert_eq!(expired.status(OWNER).unwrap().phase, RunPhaseDto::Completed);
    }

    #[test]
    fn snapshot_root_is_removed_on_exit_and_startup() {
        let exit_root = temp_root("autoscrub-snapshot-exit");
        sync_snapshot_directory(&exit_root).unwrap();
        fs::write(exit_root.join("snapshot.bin"), b"old").unwrap();
        remove_snapshot_root(&exit_root).unwrap();
        assert!(!exit_root.exists());

        let startup_root = temp_root("autoscrub-snapshot-startup");
        sync_snapshot_directory(&startup_root).unwrap();
        fs::write(startup_root.join("snapshot.bin"), b"old").unwrap();
        let ledger = AutoScrubRunState::default();
        ledger.set_snapshot_root(startup_root.clone()).unwrap();
        ledger.startup_cleanup().unwrap();
        assert!(!startup_root.exists());

        sync_snapshot_directory(&startup_root).unwrap();
        assert!(startup_root.exists());
        remove_snapshot_root(&startup_root).unwrap();
    }
}

#[cfg(test)]
mod production_fleet_tests {
    use super::*;
    use keystore::{LicenseState, LicenseStateDto};

    fn state_with_license(state: LicenseState, raw_status: &str) -> AppState {
        let state_holder = AppState::new();
        *state_holder
            .license_state
            .lock()
            .expect("license state lock") = LicenseStateDto {
            state,
            raw_status: raw_status.to_owned(),
            current_period_end: None,
            last_validated_at: None,
        };
        state_holder
    }

    fn reviewed_request(
        service_id: ServiceKind,
        reviewed_item_count: u32,
    ) -> AutoScrubReviewedRunRequest {
        AutoScrubReviewedRunRequest {
            service_id,
            account_id: "acct-discord-1".to_owned(),
            review_token: format!("review-token-{reviewed_item_count}"),
            plan_digest: "a".repeat(64),
            reviewed_item_count,
            consent: AutoScrubRunConsent::ReviewedBatchOnly,
        }
    }

    #[test]
    fn production_fleet_full_autoscrub_native_authority_acceptance() {
        let _guard = crate::GLOBAL_KEYSTORE_TEST_LOCK
            .lock()
            .expect("global keystore test lock");
        reset_run_store_for_test();
        let state = state_with_license(LicenseState::Paid, "ACTIVE");

        let first = start_reviewed_run(&state, reviewed_request(ServiceKind::Discord, 3))
            .expect("first reviewed AutoScrub run");
        assert_eq!(first.open_run_count, 1);
        assert!(!first.global_stop_requested);
        assert!(!first.unattended_execution_allowed);
        assert_eq!(first.runs[0].phase, AutoScrubRunPhase::Running);
        assert!(!first.runs[0].mutation_allowed);

        let second = start_reviewed_run(&state, reviewed_request(ServiceKind::Telegram, 5))
            .expect("second reviewed AutoScrub run");
        assert_eq!(second.open_run_count, 2);
        assert_eq!(second.runs.len(), 2);
        assert!(start_reviewed_run(&state, reviewed_request(ServiceKind::Signal, 1)).is_err());

        let stopped = request_global_stop(&state).expect("global AutoScrub stop request");
        assert_eq!(stopped.contract, CONTRACT);
        assert_eq!(stopped.open_run_count, 2);
        assert!(stopped.global_stop_requested);
        assert_eq!(stopped.quit_guard.state, AutoScrubQuitGuardState::Estimated);
        assert_eq!(
            stopped.quit_guard.honest_remaining_seconds_estimate,
            Some(150)
        );
        assert!(stopped.runs.iter().all(|run| run.stop_requested
            && run.phase == AutoScrubRunPhase::Stopping
            && !run.mutation_allowed));

        assert_eq!(fleet_status(&state).unwrap().contract, CONTRACT);
        let free = state_with_license(LicenseState::Free, "Unconfigured");
        assert_eq!(fleet_status(&free).unwrap_err(), PRO_REQUIRED);

        assert!(serde_json::from_str::<AutoScrubReviewedRunRequest>(
            r#"{"serviceId":"discord","accountId":"acct-1","reviewToken":"review-1","planDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reviewedItemCount":1,"consent":"reviewedBatchOnly","unattended":true}"#,
        )
        .is_err());
        assert!(serde_json::from_str::<AutoScrubReviewedRunRequest>(
            r#"{"serviceId":"discord","accountId":"acct-1","reviewToken":"review-1","planDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reviewedItemCount":1}"#,
        )
        .is_err());
    }

    #[test]
    fn autoscrub_fleet_debug_excludes_review_request_secrets() {
        let _guard = crate::GLOBAL_KEYSTORE_TEST_LOCK
            .lock()
            .expect("global keystore test lock");
        reset_run_store_for_test();
        let state = state_with_license(LicenseState::Paid, "ACTIVE");
        let request = AutoScrubReviewedRunRequest {
            service_id: ServiceKind::Discord,
            account_id: "acct-secret-debug-regression".to_owned(),
            review_token: "review-token-secret-debug-regression".to_owned(),
            plan_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_owned(),
            reviewed_item_count: 3,
            consent: AutoScrubRunConsent::ReviewedBatchOnly,
        };

        let debug = format!(
            "{:?}",
            start_reviewed_run(&state, request).expect("reviewed AutoScrub run")
        );

        assert!(!debug.contains("acct-secret-debug-regression"));
        assert!(!debug.contains("review-token-secret-debug-regression"));
        assert!(!debug.contains("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    }
}
