//! Fail-closed Pro capability boundary for future service-history cleanup.
//!
//! This module contains no service adapter, browser automation, network client,
//! filesystem access, or mutation path. It exposes only bounded capability
//! metadata. Discovery and execution entry points deliberately reject every
//! request until a reviewed local adapter is wired behind this boundary.

use crate::models::ServiceKind;
use ipc::AppState;
use serde::{Deserialize, Serialize};

const MAX_ACCOUNT_ID_BYTES: usize = 64;
const MAX_PLAN_ID_BYTES: usize = 80;
const MAX_CONFIRMATION_BYTES: usize = 96;
const PRO_REQUIRED: &str = "Mass Cleanup requires an active Pro license";
const DISCOVERY_UNAVAILABLE: &str =
    "Mass Cleanup discovery is unavailable because no reviewed local service adapter is installed";
const EXECUTION_UNAVAILABLE: &str =
    "Mass Cleanup execution is unavailable because no reviewed destructive adapter is installed";

/// Current adapter readiness. `Available` exists for forward-compatible DTOs,
/// but the compiled manifest below must not emit it until an adapter can both
/// act through normal service UI and verify the post-action state.
#[derive(Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MassCleanupAvailability {
    Unavailable,
    DiscoveryOnly,
    Available,
}

/// Semantic actions a future adapter may expose. Listing an action in
/// `planned_actions` is not authority to execute it.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MassCleanupAction {
    LeaveAndRemoveChat,
    ClearHistoryForSelf,
    LeaveServer,
    CloseConversation,
    ArchiveConversation,
    DeleteConversationForSelf,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceMassCleanupCapability {
    pub service_id: ServiceKind,
    pub availability: MassCleanupAvailability,
    pub discovery_supported: bool,
    pub mutation_supported: bool,
    /// Product direction only. These are never enabled while `availability`
    /// is `unavailable` or `discoveryOnly`.
    pub planned_actions: Vec<MassCleanupAction>,
    pub status: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MassCleanupCapabilityManifest {
    pub pro_required: bool,
    pub review_required_every_batch: bool,
    pub typed_confirmation_required_every_batch: bool,
    pub unattended_execution_allowed: bool,
    pub services: Vec<ServiceMassCleanupCapability>,
}

/// Account binding accepted by the future read-only discovery command.
/// The identifier is an OSL-owned opaque profile id, never a platform handle.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MassCleanupDiscoveryRequest {
    pub service_id: ServiceKind,
    pub account_id: String,
}

/// Immutable execution binding for a previously reviewed plan. There is no
/// plan store or executor in this build, so every valid request still fails.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MassCleanupExecutionRequest {
    pub service_id: ServiceKind,
    pub account_id: String,
    pub plan_id: String,
    pub plan_digest: String,
    pub typed_confirmation: String,
}

/// Enforce entitlement inside native code. UI visibility is never authority.
fn require_pro(state: &AppState) -> Result<(), String> {
    if ipc::tier_gate::is_paid_equivalent(state) {
        Ok(())
    } else {
        Err(PRO_REQUIRED.to_owned())
    }
}

/// Return bounded static metadata only after the native Pro check.
pub fn capability_manifest(state: &AppState) -> Result<MassCleanupCapabilityManifest, String> {
    require_pro(state)?;
    Ok(compiled_manifest())
}

/// Read-only discovery seam. It validates the complete request, but cannot
/// enumerate any service because no reviewed semantic adapter exists.
pub fn discover_targets(
    state: &AppState,
    request: MassCleanupDiscoveryRequest,
) -> Result<(), String> {
    require_pro(state)?;
    validate_account_binding(request.service_id, &request.account_id)?;
    Err(DISCOVERY_UNAVAILABLE.to_owned())
}

/// Destructive seam. There is intentionally no success DTO and no adapter
/// invocation: even a fully valid Pro request is rejected without mutation.
pub fn execute_batch(state: &AppState, request: MassCleanupExecutionRequest) -> Result<(), String> {
    require_pro(state)?;
    validate_account_binding(request.service_id, &request.account_id)?;
    if !valid_opaque(&request.plan_id, MAX_PLAN_ID_BYTES)
        || !valid_digest(&request.plan_digest)
        || !valid_confirmation(&request.typed_confirmation)
    {
        return Err("Mass Cleanup execution request is invalid".to_owned());
    }
    Err(EXECUTION_UNAVAILABLE.to_owned())
}

fn compiled_manifest() -> MassCleanupCapabilityManifest {
    use MassCleanupAction as Action;
    use ServiceKind as Service;

    let definitions: &[(ServiceKind, &[MassCleanupAction])] = &[
        (
            Service::Telegram,
            &[Action::LeaveAndRemoveChat, Action::ClearHistoryForSelf],
        ),
        (
            Service::Discord,
            &[Action::LeaveServer, Action::CloseConversation],
        ),
        (
            Service::WhatsApp,
            &[Action::LeaveAndRemoveChat, Action::ClearHistoryForSelf],
        ),
        (Service::Instagram, &[Action::DeleteConversationForSelf]),
        (Service::Snapchat, &[]),
        (
            Service::Email,
            &[
                Action::ArchiveConversation,
                Action::DeleteConversationForSelf,
            ],
        ),
        (Service::X, &[Action::DeleteConversationForSelf]),
        (Service::Signal, &[]),
        (Service::Slack, &[]),
        (Service::Linkedin, &[Action::DeleteConversationForSelf]),
        (Service::Teams, &[]),
        (Service::Messenger, &[Action::DeleteConversationForSelf]),
    ];

    MassCleanupCapabilityManifest {
        pro_required: true,
        review_required_every_batch: true,
        typed_confirmation_required_every_batch: true,
        unattended_execution_allowed: false,
        services: definitions
            .iter()
            .map(
                |(service_id, planned_actions)| ServiceMassCleanupCapability {
                    service_id: *service_id,
                    availability: MassCleanupAvailability::Unavailable,
                    discovery_supported: false,
                    mutation_supported: false,
                    planned_actions: planned_actions.to_vec(),
                    status: "No reviewed local adapter is installed in this build.",
                },
            )
            .collect(),
    }
}

fn validate_account_binding(service_id: ServiceKind, account_id: &str) -> Result<(), String> {
    let _ = service_id;
    if valid_opaque(account_id, MAX_ACCOUNT_ID_BYTES) {
        Ok(())
    } else {
        Err("Mass Cleanup account binding is invalid".to_owned())
    }
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

fn valid_confirmation(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CONFIRMATION_BYTES
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystore::{LicenseState, LicenseStateDto};
    use std::collections::HashSet;

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

    fn discovery_request() -> MassCleanupDiscoveryRequest {
        MassCleanupDiscoveryRequest {
            service_id: ServiceKind::Telegram,
            account_id: "acct-telegram-1".to_owned(),
        }
    }

    fn execution_request() -> MassCleanupExecutionRequest {
        MassCleanupExecutionRequest {
            service_id: ServiceKind::Telegram,
            account_id: "acct-telegram-1".to_owned(),
            plan_id: "plan-001".to_owned(),
            plan_digest: "a".repeat(64),
            typed_confirmation: "CLEAR 2 TELEGRAM HISTORIES".to_owned(),
        }
    }

    #[test]
    fn free_is_locked_at_every_native_entry_point() {
        let state = state_with_license(LicenseState::Free, "Unconfigured");
        let capability_error = match capability_manifest(&state) {
            Ok(_) => panic!("Free unexpectedly received Mass Cleanup capabilities"),
            Err(error) => error,
        };
        assert_eq!(capability_error, PRO_REQUIRED);
        assert_eq!(
            discover_targets(&state, discovery_request()).unwrap_err(),
            PRO_REQUIRED
        );
        assert_eq!(
            execute_batch(&state, execution_request()).unwrap_err(),
            PRO_REQUIRED
        );
    }

    #[test]
    fn pro_manifest_is_complete_but_every_adapter_remains_unavailable() {
        let state = state_with_license(LicenseState::Paid, "ACTIVE");
        let manifest = capability_manifest(&state).unwrap();
        assert!(manifest.pro_required);
        assert!(manifest.review_required_every_batch);
        assert!(manifest.typed_confirmation_required_every_batch);
        assert!(!manifest.unattended_execution_allowed);
        assert_eq!(manifest.services.len(), 12);
        assert!(manifest.services.iter().all(|service| {
            service.availability == MassCleanupAvailability::Unavailable
                && !service.discovery_supported
                && !service.mutation_supported
        }));
        let telegram = manifest
            .services
            .iter()
            .find(|service| service.service_id == ServiceKind::Telegram)
            .unwrap();
        assert!(
            telegram.planned_actions
                == vec![
                    MassCleanupAction::LeaveAndRemoveChat,
                    MassCleanupAction::ClearHistoryForSelf
                ]
        );
        let unique: HashSet<_> = manifest
            .services
            .iter()
            .map(|service| service.service_id)
            .collect();
        assert_eq!(unique.len(), manifest.services.len());
    }

    #[test]
    fn pro_and_offline_grace_still_cannot_discover_or_execute() {
        for license in [LicenseState::Paid, LicenseState::PaidOfflineGrace] {
            let state = state_with_license(license, "ACTIVE");
            assert_eq!(
                discover_targets(&state, discovery_request()).unwrap_err(),
                DISCOVERY_UNAVAILABLE
            );
            assert_eq!(
                execute_batch(&state, execution_request()).unwrap_err(),
                EXECUTION_UNAVAILABLE
            );
        }
    }

    #[test]
    fn request_dtos_reject_unknown_fields_and_invalid_bounds() {
        assert!(serde_json::from_str::<MassCleanupDiscoveryRequest>(
            r#"{"serviceId":"telegram","accountId":"acct-1","url":"https://example.test"}"#,
        )
        .is_err());
        assert!(serde_json::from_str::<MassCleanupExecutionRequest>(
            r#"{"serviceId":"telegram","accountId":"acct-1","planId":"plan-1","planDigest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","typedConfirmation":"CLEAR 1 TELEGRAM HISTORY","success":true}"#,
        )
        .is_err());

        let state = state_with_license(LicenseState::Paid, "ACTIVE");
        let mut invalid = discovery_request();
        invalid.account_id = "../other-profile".to_owned();
        assert_eq!(
            discover_targets(&state, invalid).unwrap_err(),
            "Mass Cleanup account binding is invalid"
        );
        let mut invalid = execution_request();
        invalid.typed_confirmation = "clear everything".to_owned();
        assert_eq!(
            execute_batch(&state, invalid).unwrap_err(),
            "Mass Cleanup execution request is invalid"
        );
    }
}
