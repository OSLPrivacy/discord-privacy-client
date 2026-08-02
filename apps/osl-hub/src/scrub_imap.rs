//! Attended IMAP scrub primitives.
//!
//! This module is intentionally local and fail-closed. The UI can request a
//! dry-run preview and later submit an attended delete request, but the
//! executable path still has to pass a main-process ACL, spend a one-shot
//! consent grant, call the native IMAP adapter, and re-query after deletion.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

const MAX_BINDING_FIELD_BYTES: usize = 256;

fn owner_account_key(owner: &str, account_id: &str) -> Result<String, ScrubImapError> {
    validate_binding_part(owner)?;
    validate_binding_part(account_id)?;
    Ok(format!("{owner}:{account_id}"))
}

fn keyring_entry(owner: &str, account_id: &str) -> Result<String, ScrubImapError> {
    Ok(format!("imap:{}", owner_account_key(owner, account_id)?))
}

fn next_auth_epoch(owner: &str, account_id: &str) -> Result<u64, ScrubImapError> {
    let _ = keyring_entry(owner, account_id)?;
    Ok(1)
}

fn config_for_epoch(owner: &str, account_id: &str, epoch: u64) -> Result<String, ScrubImapError> {
    let key = owner_account_key(owner, account_id)?;
    Ok(format!("{key}:{epoch}"))
}

#[derive(Default)]
pub struct ScrubImapState;

impl ScrubImapState {
    pub fn revoke_all(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ConsentBinding {
    owner: String,
    account: String,
    scope: String,
}

impl ConsentBinding {
    pub fn new(
        owner: impl Into<String>,
        account: impl Into<String>,
        scope: impl Into<String>,
    ) -> Result<Self, ScrubImapError> {
        let binding = Self {
            owner: owner.into(),
            account: account.into(),
            scope: scope.into(),
        };
        validate_binding_part(&binding.owner)?;
        validate_binding_part(&binding.account)?;
        validate_binding_part(&binding.scope)?;
        Ok(binding)
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn account(&self) -> &str {
        &self.account
    }
}

impl fmt::Debug for ConsentBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentBinding")
            .field("owner", &"<redacted>")
            .field("account", &"<redacted>")
            .field("scope_present", &!self.scope.is_empty())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ConsentGrant {
    id: u64,
    binding: ConsentBinding,
    expires_at_unix: u64,
}

impl ConsentGrant {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl fmt::Debug for ConsentGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentGrant")
            .field("id", &self.id)
            .field("binding", &self.binding)
            .field("expires_at_unix", &self.expires_at_unix)
            .finish()
    }
}

#[derive(Debug, Default)]
pub struct ConsentLedger {
    next_id: u64,
    grants: BTreeMap<u64, ConsentGrant>,
}

impl ConsentLedger {
    pub fn mint(
        &mut self,
        binding: ConsentBinding,
        now_unix: u64,
        ttl_secs: u64,
    ) -> Result<ConsentGrant, ScrubImapError> {
        let id = self
            .next_id
            .checked_add(1)
            .ok_or(ScrubImapError::ConsentIdOverflow)?;
        self.next_id = id;
        let grant = ConsentGrant {
            id,
            binding,
            expires_at_unix: now_unix.saturating_add(ttl_secs),
        };
        self.grants.insert(id, grant.clone());
        Ok(grant)
    }

    pub fn consume(
        &mut self,
        grant_id: u64,
        binding: &ConsentBinding,
        now_unix: u64,
    ) -> Result<ConsentGrant, ScrubImapError> {
        let stored = self
            .grants
            .get(&grant_id)
            .ok_or(ScrubImapError::ConsentMissing)?;
        if &stored.binding != binding {
            return Err(ScrubImapError::ConsentBindingMismatch);
        }
        let grant = self
            .grants
            .remove(&grant_id)
            .expect("grant was checked present above");
        if now_unix > grant.expires_at_unix {
            return Err(ScrubImapError::ConsentExpired);
        }
        Ok(grant)
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ImapMessageSummary {
    pub uid: u64,
    pub subject: String,
    pub sender: String,
}

impl fmt::Debug for ImapMessageSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImapMessageSummary")
            .field("uid", &self.uid)
            .field("subject", &"<redacted>")
            .field("sender", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerFacingDryRunPreview {
    pub owner_account: String,
    pub dry_run: bool,
    pub total_messages: usize,
    pub deletable_uids: Vec<u64>,
    pub action_label: String,
}

impl fmt::Debug for OwnerFacingDryRunPreview {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnerFacingDryRunPreview")
            .field("owner_account", &"<redacted>")
            .field("dry_run", &self.dry_run)
            .field("total_messages", &self.total_messages)
            .field("deletable_uid_count", &self.deletable_uids.len())
            .field("action_label", &self.action_label)
            .finish()
    }
}

pub fn inspect(
    owner_account: impl Into<String>,
    messages: &[ImapMessageSummary],
) -> Result<OwnerFacingDryRunPreview, ScrubImapError> {
    let owner_account = owner_account.into();
    validate_binding_part(&owner_account)?;
    let mut deletable_uids = messages
        .iter()
        .map(|message| message.uid)
        .collect::<Vec<_>>();
    deletable_uids.sort_unstable();
    Ok(OwnerFacingDryRunPreview {
        owner_account,
        dry_run: true,
        total_messages: messages.len(),
        deletable_uids,
        action_label: "Review messages before deleting".to_string(),
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QueryAfterDelete {
    Gone,
    Present,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteVerification {
    VerifiedGone,
    StillPresent,
    Unknown,
}

/// Why deletion cannot be honestly verified.  These are intentionally not
/// collapsed: an operator needs to distinguish a provider outage from a
/// changed account authorization or a response that cannot be interpreted.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub enum VerificationAmbiguity {
    DroppedConnection,
    AuthEpochChanged,
    SchemaDrift,
    RateLimited,
    AmbiguousReadback,
    PriorUnknown,
}

#[derive(Debug, Clone, Eq, PartialEq, Default)]
pub struct AmbiguityCounts {
    counts: BTreeMap<VerificationAmbiguity, usize>,
}

impl AmbiguityCounts {
    pub fn record(&mut self, source: VerificationAmbiguity) -> DeleteVerification {
        *self.counts.entry(source).or_default() += 1;
        DeleteVerification::Unknown
    }

    pub fn count(&self, source: VerificationAmbiguity) -> usize {
        self.counts.get(&source).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrubReceiptStatus {
    VerifiedGone,
    StillPresent,
    Unknown,
}

impl From<DeleteVerification> for ScrubReceiptStatus {
    fn from(value: DeleteVerification) -> Self {
        match value {
            DeleteVerification::VerifiedGone => Self::VerifiedGone,
            DeleteVerification::StillPresent => Self::StillPresent,
            DeleteVerification::Unknown => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubReceiptStatusProjection {
    pub item_ordinal: u64,
    pub status: ScrubReceiptStatus,
}

pub fn project_scrub_receipt_statuses(
    receipts: &[(u64, DeleteVerification)],
) -> Vec<ScrubReceiptStatusProjection> {
    receipts
        .iter()
        .map(|(item_ordinal, status)| ScrubReceiptStatusProjection {
            item_ordinal: *item_ordinal,
            status: (*status).into(),
        })
        .collect()
}

pub trait NativeImapAdapter {
    fn delete_message(&mut self, uid: u64) -> Result<(), ScrubImapError>;
    fn query_message(&mut self, uid: u64) -> Result<QueryAfterDelete, ScrubImapError>;
}

pub fn delete_and_verify(
    adapter: &mut dyn NativeImapAdapter,
    uid: u64,
) -> Result<DeleteVerification, ScrubImapError> {
    adapter.delete_message(uid)?;
    match adapter.query_message(uid)? {
        QueryAfterDelete::Gone => Ok(DeleteVerification::VerifiedGone),
        QueryAfterDelete::Present => Ok(DeleteVerification::StillPresent),
        QueryAfterDelete::Unknown => Ok(DeleteVerification::Unknown),
    }
}

pub fn enumerate(
    owner: &str,
    account_id: &str,
    messages: &[ImapMessageSummary],
) -> Result<OwnerFacingDryRunPreview, ScrubImapError> {
    let epoch = next_auth_epoch(owner, account_id)?;
    let _config = config_for_epoch(owner, account_id, epoch)?;
    inspect(account_id, messages)
}

pub fn delete(
    owner: &str,
    account_id: &str,
    _uid: u64,
) -> Result<DeleteVerification, ScrubImapError> {
    let epoch = next_auth_epoch(owner, account_id)?;
    let _config = config_for_epoch(owner, account_id, epoch)?;
    let _refusal = "Native IMAP deletion is disabled";
    Err(ScrubImapError::NativeDeletionDisabled)
}

fn verify_with(
    adapter: &mut dyn NativeImapAdapter,
    uid: u64,
) -> Result<DeleteVerification, ScrubImapError> {
    match adapter.query_message(uid)? {
        QueryAfterDelete::Gone => Ok(DeleteVerification::VerifiedGone),
        QueryAfterDelete::Present => Ok(DeleteVerification::StillPresent),
        QueryAfterDelete::Unknown => Ok(DeleteVerification::Unknown),
    }
}

pub fn verify(
    adapter: &mut dyn NativeImapAdapter,
    owner: &str,
    account_id: &str,
    uid: u64,
) -> Result<DeleteVerification, ScrubImapError> {
    let epoch = next_auth_epoch(owner, account_id)?;
    let _config = config_for_epoch(owner, account_id, epoch)?;
    verify_with(adapter, uid)
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiScrubRequest {
    pub owner: String,
    pub account: String,
    pub grant_id: u64,
    pub approved_uids: Vec<u64>,
}

impl fmt::Debug for UiScrubRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UiScrubRequest")
            .field("owner", &"<redacted>")
            .field("account", &"<redacted>")
            .field("grant_id", &self.grant_id)
            .field("approved_uid_count", &self.approved_uids.len())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndToEndScrubReport {
    pub preview: OwnerFacingDryRunPreview,
    pub verified: Vec<(u64, DeleteVerification)>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct MainOnlyAcl {
    owner: String,
}

impl fmt::Debug for MainOnlyAcl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MainOnlyAcl")
            .field("owner", &"<redacted>")
            .finish()
    }
}

impl MainOnlyAcl {
    pub fn for_owner(owner: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
        }
    }

    fn authorize(&self, request: &UiScrubRequest) -> Result<(), ScrubImapError> {
        if self.owner == request.owner {
            Ok(())
        } else {
            Err(ScrubImapError::AclRefused)
        }
    }
}

pub fn run_attended_fixture(
    request: UiScrubRequest,
    acl: &MainOnlyAcl,
    ledger: &mut ConsentLedger,
    adapter: &mut dyn NativeImapAdapter,
    now_unix: u64,
    messages: &[ImapMessageSummary],
) -> Result<EndToEndScrubReport, ScrubImapError> {
    acl.authorize(&request)?;
    let binding = ConsentBinding::new(&request.owner, &request.account, "imap-attended-delete")?;
    ledger.consume(request.grant_id, &binding, now_unix)?;
    let preview = inspect(&request.account, messages)?;
    let approved = request.approved_uids.into_iter().collect::<BTreeSet<_>>();
    let mut verified = Vec::new();
    for uid in preview
        .deletable_uids
        .iter()
        .copied()
        .filter(|uid| approved.contains(uid))
    {
        verified.push((uid, delete_and_verify(adapter, uid)?));
    }
    Ok(EndToEndScrubReport { preview, verified })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ScrubImapError {
    InvalidBinding,
    ConsentIdOverflow,
    ConsentMissing,
    ConsentBindingMismatch,
    ConsentExpired,
    AclRefused,
    NativeDeletionDisabled,
    NativeDeleteFailed,
    NativeQueryFailed,
}

impl fmt::Display for ScrubImapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidBinding => "email cleanup binding is invalid",
            Self::ConsentIdOverflow => "email cleanup consent id overflow",
            Self::ConsentMissing => "email cleanup consent grant is missing",
            Self::ConsentBindingMismatch => "email cleanup consent binding mismatch",
            Self::ConsentExpired => "email cleanup consent grant expired",
            Self::AclRefused => "email cleanup request was refused by local authorization",
            Self::NativeDeletionDisabled => "Native IMAP deletion is disabled",
            Self::NativeDeleteFailed => "email cleanup delete failed",
            Self::NativeQueryFailed => "email cleanup verification failed",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ScrubImapError {}

fn validate_binding_part(value: &str) -> Result<(), ScrubImapError> {
    if value.is_empty() || value.len() > MAX_BINDING_FIELD_BYTES {
        Err(ScrubImapError::InvalidBinding)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ConsentBinding {
        ConsentBinding::new("owner-a", "account-a", "imap-attended-delete").unwrap()
    }

    fn messages() -> Vec<ImapMessageSummary> {
        vec![
            ImapMessageSummary {
                uid: 20,
                subject: "Receipt".to_string(),
                sender: "service@example.test".to_string(),
            },
            ImapMessageSummary {
                uid: 10,
                subject: "Alert".to_string(),
                sender: "alerts@example.test".to_string(),
            },
        ]
    }

    fn ui_tauri_request(grant_id: u64) -> UiScrubRequest {
        serde_json::from_str(&format!(
            r#"{{
                "owner": "owner-a",
                "account": "account-a",
                "grantId": {grant_id},
                "approvedUids": [20, 10]
            }}"#
        ))
        .unwrap()
    }

    #[derive(Default)]
    struct FixtureImap {
        outcomes: BTreeMap<u64, QueryAfterDelete>,
        calls: Vec<String>,
    }

    impl NativeImapAdapter for FixtureImap {
        fn delete_message(&mut self, uid: u64) -> Result<(), ScrubImapError> {
            self.calls.push(format!("delete:{uid}"));
            Ok(())
        }

        fn query_message(&mut self, uid: u64) -> Result<QueryAfterDelete, ScrubImapError> {
            self.calls.push(format!("query:{uid}"));
            Ok(*self
                .outcomes
                .get(&uid)
                .unwrap_or(&QueryAfterDelete::Unknown))
        }
    }

    #[test]
    fn inspect_produces_owner_facing_dry_run_preview() {
        let preview = inspect("owner@example.test", &messages()).unwrap();

        assert!(preview.dry_run);
        assert_eq!(preview.owner_account, "owner@example.test");
        assert_eq!(preview.total_messages, 2);
        assert_eq!(preview.deletable_uids, vec![10, 20]);
        assert_eq!(preview.action_label, "Review messages before deleting");
    }

    #[test]
    fn consent_ledger_mints_consumes_and_rejects_replay() {
        let mut ledger = ConsentLedger::default();
        let grant = ledger.mint(binding(), 100, 30).unwrap();

        let consumed = ledger.consume(grant.id(), &binding(), 110).unwrap();
        assert_eq!(consumed.id(), grant.id());
        assert_eq!(
            ledger.consume(grant.id(), &binding(), 111),
            Err(ScrubImapError::ConsentMissing),
            "a spent grant must not be replayable"
        );
    }

    #[test]
    fn consent_ledger_consume_spends_before_freshness_check() {
        let mut ledger = ConsentLedger::default();
        let grant = ledger.mint(binding(), 100, 5).unwrap();

        assert_eq!(
            ledger.consume(grant.id(), &binding(), 106),
            Err(ScrubImapError::ConsentExpired)
        );
        assert_eq!(
            ledger.consume(grant.id(), &binding(), 106),
            Err(ScrubImapError::ConsentMissing),
            "expired grants must be removed before the freshness check returns"
        );
    }

    #[test]
    fn delete_verification_requeries_and_distinguishes_verified_gone_still_present_unknown() {
        let mut adapter = FixtureImap::default();
        adapter.outcomes.insert(1, QueryAfterDelete::Gone);
        adapter.outcomes.insert(2, QueryAfterDelete::Present);
        adapter.outcomes.insert(3, QueryAfterDelete::Unknown);

        assert_eq!(
            delete_and_verify(&mut adapter, 1).unwrap(),
            DeleteVerification::VerifiedGone
        );
        assert_eq!(
            delete_and_verify(&mut adapter, 2).unwrap(),
            DeleteVerification::StillPresent
        );
        assert_eq!(
            delete_and_verify(&mut adapter, 3).unwrap(),
            DeleteVerification::Unknown
        );
        assert_eq!(
            adapter.calls,
            vec![
                "delete:1".to_string(),
                "query:1".to_string(),
                "delete:2".to_string(),
                "query:2".to_string(),
                "delete:3".to_string(),
                "query:3".to_string(),
            ]
        );
    }

    #[test]
    fn scrub_receipt_projects_verified_gone_still_present_and_unknown_statuses() {
        let projection = project_scrub_receipt_statuses(&[
            (3, DeleteVerification::VerifiedGone),
            (4, DeleteVerification::StillPresent),
            (5, DeleteVerification::Unknown),
        ]);

        assert_eq!(
            projection,
            vec![
                ScrubReceiptStatusProjection {
                    item_ordinal: 3,
                    status: ScrubReceiptStatus::VerifiedGone,
                },
                ScrubReceiptStatusProjection {
                    item_ordinal: 4,
                    status: ScrubReceiptStatus::StillPresent,
                },
                ScrubReceiptStatusProjection {
                    item_ordinal: 5,
                    status: ScrubReceiptStatus::Unknown,
                },
            ]
        );
        assert_eq!(
            serde_json::to_value(&projection).unwrap(),
            serde_json::json!([
                {"itemOrdinal": 3, "status": "verified_gone"},
                {"itemOrdinal": 4, "status": "still_present"},
                {"itemOrdinal": 5, "status": "unknown"}
            ])
        );
    }

    #[test]
    fn scrub_imap_end_to_end_fixture_ui_tauri_acl_native() {
        let mut ledger = ConsentLedger::default();
        let binding = binding();
        let grant = ledger.mint(binding, 100, 60).unwrap();

        assert!(
            serde_json::from_str::<UiScrubRequest>(&format!(
                r#"{{
                    "owner": "owner-a",
                    "account": "account-a",
                    "grantId": {},
                    "approvedUids": [10],
                    "deleteEverything": true
                }}"#,
                grant.id()
            ))
            .is_err(),
            "the Tauri command payload must reject renderer-supplied ambient authority"
        );

        let acl = MainOnlyAcl::for_owner("owner-a");
        let mut adapter = FixtureImap::default();

        let wrong_owner = UiScrubRequest {
            owner: "owner-b".to_string(),
            account: "account-a".to_string(),
            grant_id: grant.id(),
            approved_uids: vec![10],
        };
        assert_eq!(
            run_attended_fixture(
                wrong_owner,
                &acl,
                &mut ledger,
                &mut adapter,
                110,
                &messages()
            ),
            Err(ScrubImapError::AclRefused)
        );
        assert!(
            adapter.calls.is_empty(),
            "main-only ACL refusal must happen before native IMAP is called"
        );

        let wrong_binding = UiScrubRequest {
            owner: "owner-a".to_string(),
            account: "account-b".to_string(),
            grant_id: grant.id(),
            approved_uids: vec![10],
        };
        assert_eq!(
            run_attended_fixture(
                wrong_binding,
                &acl,
                &mut ledger,
                &mut adapter,
                111,
                &messages()
            ),
            Err(ScrubImapError::ConsentBindingMismatch)
        );
        assert!(
            adapter.calls.is_empty(),
            "consent binding refusal must happen before native IMAP is called"
        );

        adapter.outcomes.insert(10, QueryAfterDelete::Gone);
        adapter.outcomes.insert(20, QueryAfterDelete::Present);

        let report = run_attended_fixture(
            ui_tauri_request(grant.id()),
            &acl,
            &mut ledger,
            &mut adapter,
            120,
            &messages(),
        )
        .unwrap();

        assert!(report.preview.dry_run);
        assert_eq!(report.preview.deletable_uids, vec![10, 20]);
        assert_eq!(
            report.verified,
            vec![
                (10, DeleteVerification::VerifiedGone),
                (20, DeleteVerification::StillPresent)
            ]
        );
        assert_eq!(
            adapter.calls,
            vec![
                "delete:10".to_string(),
                "query:10".to_string(),
                "delete:20".to_string(),
                "query:20".to_string(),
            ],
            "native IMAP must be reached only after UI payload parsing, ACL, and consent"
        );
        let tauri_response = serde_json::to_value(&report).unwrap();
        assert_eq!(tauri_response["preview"]["dryRun"], true);
        assert_eq!(
            tauri_response["verified"],
            serde_json::json!([[10, "verified_gone"], [20, "still_present"]])
        );

        let replay = UiScrubRequest {
            owner: "owner-a".to_string(),
            account: "account-a".to_string(),
            grant_id: grant.id(),
            approved_uids: vec![20],
        };
        let native_calls_after_success = adapter.calls.clone();
        assert_eq!(
            run_attended_fixture(replay, &acl, &mut ledger, &mut adapter, 121, &messages()),
            Err(ScrubImapError::ConsentMissing),
            "the UI-to-native path must not be reusable after the consent grant is spent"
        );
        assert_eq!(
            adapter.calls, native_calls_after_success,
            "replayed UI authority must not reach native IMAP"
        );
    }
}
