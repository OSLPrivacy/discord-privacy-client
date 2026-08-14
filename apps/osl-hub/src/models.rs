use serde::{Deserialize, Deserializer, Serialize};

pub const DEFAULT_HOME_TILE_ORDER: &[&str] = &[
    "discord",
    "telegram",
    "signal",
    "whatsapp",
    "messenger",
    "gmail",
    "outlook",
    "proton",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
    "tuta",
    "osl-chats",
    "osl-mail",
    "osl-notes",
    "scrub",
];

pub fn default_home_tile_order() -> Vec<String> {
    DEFAULT_HOME_TILE_ORDER
        .iter()
        .map(|tile| (*tile).to_owned())
        .collect()
}

pub fn is_default_home_tile(tile: &str) -> bool {
    DEFAULT_HOME_TILE_ORDER.contains(&tile)
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HomeTileArrangementInput {
    pub order: Vec<String>,
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeTileArrangementRead {
    pub visible_tiles: Vec<String>,
    pub hidden_tiles: Vec<String>,
    pub visible_tile_data: Vec<HomeTileData>,
    pub hidden_tile_data: Vec<HomeTileData>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "action", deny_unknown_fields)]
pub enum HomeTileArrangementAction {
    Move {
        tile_id: String,
        delta: i8,
    },
    Drag {
        tile_id: String,
        before_tile_id: String,
    },
    Hide {
        tile_id: String,
    },
    Show {
        tile_id: String,
    },
    Done,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeTileData {
    pub id: String,
    pub capability: Option<HomeTileCapabilityFacts>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeTileCapabilityFacts {
    pub surface: &'static str,
    pub public_claim: &'static str,
    pub carrier_evidence: &'static str,
    pub delivery_evidence: &'static str,
    pub claim_blockers: Vec<&'static str>,
    pub matrix_position: &'static str,
    pub first_party: bool,
    pub capability_claim: bool,
}

/// The explicit action that prepares a cover for a carrier composer.
///
/// These values are preferences only in this isolated preview. This crate does
/// not implement keyboard control, clipboard writes, or platform automation.
/// Cover insertion is deliberately a separate choice: it answers *how* the
/// prepared cover enters a composer, not *what triggers* preparation.
#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SendMode {
    #[default]
    #[serde(rename = "enter")]
    Enter,
    #[serde(rename = "clipboard")]
    Clipboard,
    #[serde(rename = "enter-x2")]
    EnterX2,
}

impl SendMode {
    pub const ALL: [Self; 3] = [Self::Enter, Self::EnterX2, Self::Clipboard];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::EnterX2 => "Enter x2",
            Self::Clipboard => "Clipboard",
        }
    }
}

/// How a future companion could place a user-approved capsule.
///
/// No placement behavior is present in this preview backend.
#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlacementMode {
    #[default]
    Atomic,
    Compatibility,
}

/// How generated cover text should be inserted into the carrier composer.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverInsertion {
    InsertOnSend,
    TypeNaturally,
}

/// The backend result of choosing a send trigger.
///
/// Every supported trigger prepares a cover. The independently selected
/// insertion choice is reported alongside it so callers cannot conflate the
/// trigger with insertion behavior.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedCoverSend {
    pub trigger: SendMode,
    pub trigger_name: &'static str,
    pub cover_prepared: bool,
    pub cover_insertion: CoverInsertion,
}

pub const fn prepare_cover_for_send(
    trigger: SendMode,
    cover_insertion: CoverInsertion,
) -> PreparedCoverSend {
    PreparedCoverSend {
        trigger,
        trigger_name: trigger.name(),
        cover_prepared: true,
        cover_insertion,
    }
}

/// The recovery/delivery policy explicitly selected during onboarding.
#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ForwardSecrecyMode {
    ProtectPast,
    #[default]
    KeepGroupDelivery,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingPreferences {
    pub onboarding_complete: bool,
    pub send_mode: SendMode,
    pub placement_mode: PlacementMode,
    pub cover_insertion: Option<CoverInsertion>,
    pub show_plaintext_preview: bool,
    #[serde(default = "default_true")]
    pub window_capture_enabled: bool,
    #[serde(default)]
    pub rn_wire_policy_requested: bool,
    pub acknowledge_experimental_send_risk: bool,
    #[serde(default)]
    pub forward_secrecy_mode: ForwardSecrecyMode,
}

fn default_true() -> bool {
    true
}

impl Default for OnboardingPreferences {
    fn default() -> Self {
        Self {
            onboarding_complete: false,
            send_mode: SendMode::Enter,
            placement_mode: PlacementMode::Atomic,
            cover_insertion: None,
            show_plaintext_preview: true,
            window_capture_enabled: true,
            rn_wire_policy_requested: false,
            acknowledge_experimental_send_risk: false,
            forward_secrecy_mode: ForwardSecrecyMode::default(),
        }
    }
}

impl<'de> Deserialize<'de> for OnboardingPreferences {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct WirePreferences {
            onboarding_complete: bool,
            send_mode: SendMode,
            placement_mode: PlacementMode,
            cover_insertion: Option<CoverInsertion>,
            show_plaintext_preview: bool,
            #[serde(default = "default_true")]
            window_capture_enabled: bool,
            #[serde(default)]
            rn_wire_policy_requested: bool,
            acknowledge_experimental_send_risk: bool,
            #[serde(default)]
            forward_secrecy_mode: ForwardSecrecyMode,
        }

        let preferences = WirePreferences::deserialize(deserializer)?;
        Ok(Self {
            onboarding_complete: preferences.onboarding_complete,
            send_mode: preferences.send_mode,
            placement_mode: preferences.placement_mode,
            cover_insertion: preferences.cover_insertion,
            show_plaintext_preview: preferences.show_plaintext_preview,
            window_capture_enabled: preferences.window_capture_enabled,
            rn_wire_policy_requested: preferences.rn_wire_policy_requested,
            acknowledge_experimental_send_risk: preferences.acknowledge_experimental_send_risk,
            forward_secrecy_mode: preferences.forward_secrecy_mode,
        }
        .fail_closed())
    }
}

impl OnboardingPreferences {
    /// Enforce the safety invariant at the native trust boundary.
    ///
    /// Send triggers do not carry a hidden risk mode. A stale acknowledgement
    /// from the retired five-mode UI is always discarded.
    pub fn fail_closed(mut self) -> Self {
        self.acknowledge_experimental_send_risk = false;
        self
    }
}

/// Product boundary for the planned Android Mobile Workspace.
///
/// This is a local-first model only. It does not launch Android, grant host
/// device access, or make compatibility/protection claims while the feature is
/// still staged as coming later.
#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AndroidWorkspaceExecution {
    Local,
    Hosted,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AndroidWorkspaceBoundary {
    DeniedUntilEnabled,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidWorkspaceSnapshotPolicy {
    pub encrypted_locally_before_backup: bool,
    pub backup_optional: bool,
    pub infrastructure_can_mount: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidWorkspacePolicy {
    pub launch_state: ServiceLaunchState,
    pub default_execution: AndroidWorkspaceExecution,
    pub hosted_requires_separate_threat_model_consent: bool,
    pub hosted_inherits_local_privacy_claims: bool,
    pub encrypted_disk_per_osl_identity: bool,
    pub android_identity_per_osl_identity: bool,
    pub app_permissions_individually_enabled: bool,
    pub clipboard: AndroidWorkspaceBoundary,
    pub files: AndroidWorkspaceBoundary,
    pub notifications: AndroidWorkspaceBoundary,
    pub camera: AndroidWorkspaceBoundary,
    pub microphone: AndroidWorkspaceBoundary,
    pub location: AndroidWorkspaceBoundary,
    pub snapshots: AndroidWorkspaceSnapshotPolicy,
    pub claims_android_app_compatibility: bool,
    pub claims_verified_isolation: bool,
    pub allows_prompt_bypass: bool,
    pub allows_device_fingerprint_spoofing: bool,
    pub allows_session_or_credential_copy: bool,
}

impl AndroidWorkspacePolicy {
    pub fn local_first() -> Self {
        Self {
            launch_state: ServiceLaunchState::ComingSoon,
            default_execution: AndroidWorkspaceExecution::Local,
            hosted_requires_separate_threat_model_consent: true,
            hosted_inherits_local_privacy_claims: false,
            encrypted_disk_per_osl_identity: true,
            android_identity_per_osl_identity: true,
            app_permissions_individually_enabled: true,
            clipboard: AndroidWorkspaceBoundary::DeniedUntilEnabled,
            files: AndroidWorkspaceBoundary::DeniedUntilEnabled,
            notifications: AndroidWorkspaceBoundary::DeniedUntilEnabled,
            camera: AndroidWorkspaceBoundary::DeniedUntilEnabled,
            microphone: AndroidWorkspaceBoundary::DeniedUntilEnabled,
            location: AndroidWorkspaceBoundary::DeniedUntilEnabled,
            snapshots: AndroidWorkspaceSnapshotPolicy {
                encrypted_locally_before_backup: true,
                backup_optional: true,
                infrastructure_can_mount: false,
            },
            claims_android_app_compatibility: false,
            claims_verified_isolation: false,
            allows_prompt_bypass: false,
            allows_device_fingerprint_spoofing: false,
            allows_session_or_credential_copy: false,
        }
    }

    pub fn authorize_start(
        &self,
        execution: AndroidWorkspaceExecution,
        evidence: AndroidWorkspaceStartEvidence,
    ) -> AndroidWorkspaceStartDecision {
        if self.launch_state != ServiceLaunchState::Available {
            return AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::ComingLater,
            );
        }
        if evidence.user_consented != Some(true) {
            return AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::ConsentRequired,
            );
        }
        if evidence.workspace_binding_present != Some(true) {
            return AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::BindingRequired,
            );
        }
        if evidence.runtime_authority_present != Some(true) {
            return AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::AuthorityRequired,
            );
        }
        if evidence.isolation_verified != Some(true) {
            return AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::IsolationNotVerified,
            );
        }
        if execution == AndroidWorkspaceExecution::Hosted {
            if !self.hosted_requires_separate_threat_model_consent
                || evidence.hosted_threat_model_consented != Some(true)
            {
                return AndroidWorkspaceStartDecision::Refused(
                    AndroidWorkspaceRefusalReason::HostedThreatModelConsentRequired,
                );
            }
        }
        AndroidWorkspaceStartDecision::Allowed
    }
}

impl Default for AndroidWorkspacePolicy {
    fn default() -> Self {
        Self::local_first()
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AndroidWorkspaceStartEvidence {
    pub user_consented: Option<bool>,
    pub workspace_binding_present: Option<bool>,
    pub runtime_authority_present: Option<bool>,
    pub isolation_verified: Option<bool>,
    pub hosted_threat_model_consented: Option<bool>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AndroidWorkspaceStartDecision {
    Allowed,
    Refused(AndroidWorkspaceRefusalReason),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AndroidWorkspaceRefusalReason {
    ComingLater,
    ConsentRequired,
    BindingRequired,
    AuthorityRequired,
    IsolationNotVerified,
    HostedThreatModelConsentRequired,
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_cover_for_send, AndroidWorkspaceBoundary, AndroidWorkspaceExecution,
        AndroidWorkspacePolicy, AndroidWorkspaceRefusalReason, AndroidWorkspaceStartDecision,
        AndroidWorkspaceStartEvidence, CoverInsertion, ForwardSecrecyMode, OnboardingPreferences,
        PlacementMode, SendMode, ServiceKind, ServiceLaunchState,
    };

    #[test]
    fn send_modes_match_the_frontend_contract() {
        let cases = [
            (SendMode::Enter, "\"enter\""),
            (SendMode::Clipboard, "\"clipboard\""),
            (SendMode::EnterX2, "\"enter-x2\""),
        ];

        for (mode, expected) in cases {
            assert_eq!(serde_json::to_string(&mode).unwrap(), expected);
            assert_eq!(serde_json::from_str::<SendMode>(expected).unwrap(), mode);
        }

        let default_contract = serde_json::json!({
            "onboardingComplete": false,
            "sendMode": "enter",
            "placementMode": "atomic",
            "coverInsertion": null,
            "showPlaintextPreview": true,
            "windowCaptureEnabled": true,
            "rnWirePolicyRequested": false,
            "acknowledgeExperimentalSendRisk": false,
            // Added when `forward_secrecy_mode` joined OnboardingPreferences;
            // this literal was never updated, and the name collision that kept
            // the lib test target from compiling meant nobody saw it fail. The
            // frontend agrees on both the key and this default:
            // `apps/osl-hub-ui/src/preferences.ts` seeds
            // `forwardSecrecyMode: "keepGroupDelivery"`.
            "forwardSecrecyMode": "keepGroupDelivery",
        });
        assert_eq!(
            serde_json::to_value(OnboardingPreferences::default()).unwrap(),
            default_contract
        );
        assert_eq!(
            serde_json::from_value::<OnboardingPreferences>(default_contract).unwrap(),
            OnboardingPreferences::default()
        );

        let incomplete_risk_acknowledgement = serde_json::json!({
            "onboardingComplete": true,
            "sendMode": "enter-x2",
            "placementMode": "atomic",
            "coverInsertion": null,
            "showPlaintextPreview": true,
            "windowCaptureEnabled": true,
            "rnWirePolicyRequested": false,
            "acknowledgeExperimentalSendRisk": false,
        });
        assert!(
            serde_json::from_value::<OnboardingPreferences>(incomplete_risk_acknowledgement)
                .unwrap()
                .onboarding_complete
        );
    }

    #[test]
    fn send_modes_refuse_absent_or_unknown_authority() {
        for raw in [
            "\"Manual\"",
            "\"manual\"",
            "\"Instant\"",
            "\"instant\"",
            "\"Match typing\"",
            "\"match_typing\"",
            "\"Double Enter\"",
            "\"single\"",
            "\"enter \"",
            "\"\"",
            "null",
        ] {
            assert!(
                serde_json::from_str::<SendMode>(raw).is_err(),
                "accepted invalid send mode {raw}"
            );
        }

        let missing_send_mode = r#"{
            "onboardingComplete": true,
            "placementMode": "atomic",
            "showPlaintextPreview": true,
            "windowCaptureEnabled": true,
            "acknowledgeExperimentalSendRisk": true
        }"#;

        assert!(serde_json::from_str::<OnboardingPreferences>(missing_send_mode).is_err());
    }

    #[test]
    fn all_three_send_triggers_prepare_covers_and_report_insertion_separately() {
        let expected = ["Enter", "Enter x2", "Clipboard"];
        let names: Vec<_> = SendMode::ALL.iter().map(|mode| mode.name()).collect();
        assert_eq!(names, expected);

        for trigger in SendMode::ALL {
            for insertion in [CoverInsertion::InsertOnSend, CoverInsertion::TypeNaturally] {
                let prepared = prepare_cover_for_send(trigger, insertion);
                println!(
                    "trigger={} cover_prepared={} cover_insertion={:?}",
                    prepared.trigger_name, prepared.cover_prepared, prepared.cover_insertion
                );
                assert_eq!(prepared.trigger, trigger);
                assert_eq!(prepared.trigger_name, trigger.name());
                assert!(prepared.cover_prepared);
                assert_eq!(prepared.cover_insertion, insertion);
            }
        }
    }

    #[test]
    fn retired_risk_acknowledgement_is_not_retained() {
        let preferences = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::EnterX2,
            placement_mode: PlacementMode::Atomic,
            cover_insertion: Some(CoverInsertion::InsertOnSend),
            show_plaintext_preview: true,
            window_capture_enabled: true,
            rn_wire_policy_requested: false,
            acknowledge_experimental_send_risk: false,
            forward_secrecy_mode: ForwardSecrecyMode::default(),
        }
        .fail_closed();

        assert!(preferences.onboarding_complete);
        assert!(!preferences.acknowledge_experimental_send_risk);
    }

    #[test]
    fn whatsapp_matches_the_frontend_service_id() {
        assert_eq!(
            serde_json::to_string(&ServiceKind::WhatsApp).unwrap(),
            "\"whatsapp\""
        );
        assert_eq!(
            serde_json::from_str::<ServiceKind>("\"whatsapp\"").unwrap(),
            ServiceKind::WhatsApp
        );
    }

    #[test]
    fn android_workspace_policy_is_local_first() {
        let policy = AndroidWorkspacePolicy::local_first();

        assert_eq!(policy.launch_state, ServiceLaunchState::ComingSoon);
        assert_eq!(policy.default_execution, AndroidWorkspaceExecution::Local);
        assert!(policy.hosted_requires_separate_threat_model_consent);
        assert!(!policy.hosted_inherits_local_privacy_claims);
        assert!(policy.encrypted_disk_per_osl_identity);
        assert!(policy.android_identity_per_osl_identity);
        assert!(policy.app_permissions_individually_enabled);

        for boundary in [
            policy.clipboard,
            policy.files,
            policy.notifications,
            policy.camera,
            policy.microphone,
            policy.location,
        ] {
            assert_eq!(boundary, AndroidWorkspaceBoundary::DeniedUntilEnabled);
        }

        assert!(policy.snapshots.encrypted_locally_before_backup);
        assert!(policy.snapshots.backup_optional);
        assert!(!policy.snapshots.infrastructure_can_mount);
        assert!(!policy.claims_android_app_compatibility);
        assert!(!policy.claims_verified_isolation);
        assert!(!policy.allows_prompt_bypass);
        assert!(!policy.allows_device_fingerprint_spoofing);
        assert!(!policy.allows_session_or_credential_copy);
        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    user_consented: Some(true),
                    workspace_binding_present: Some(true),
                    runtime_authority_present: Some(true),
                    isolation_verified: Some(true),
                    hosted_threat_model_consented: None,
                },
            ),
            AndroidWorkspaceStartDecision::Refused(AndroidWorkspaceRefusalReason::ComingLater)
        );
    }

    #[test]
    fn android_workspace_absent_consent_binding_or_authority_refuses() {
        let policy = AndroidWorkspacePolicy {
            launch_state: ServiceLaunchState::Available,
            ..AndroidWorkspacePolicy::local_first()
        };
        let complete = AndroidWorkspaceStartEvidence {
            user_consented: Some(true),
            workspace_binding_present: Some(true),
            runtime_authority_present: Some(true),
            isolation_verified: Some(true),
            hosted_threat_model_consented: None,
        };

        assert_eq!(
            policy.authorize_start(AndroidWorkspaceExecution::Local, complete),
            AndroidWorkspaceStartDecision::Allowed
        );

        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    user_consented: None,
                    ..complete
                },
            ),
            AndroidWorkspaceStartDecision::Refused(AndroidWorkspaceRefusalReason::ConsentRequired)
        );
        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    user_consented: Some(false),
                    ..complete
                },
            ),
            AndroidWorkspaceStartDecision::Refused(AndroidWorkspaceRefusalReason::ConsentRequired)
        );
        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    workspace_binding_present: None,
                    ..complete
                },
            ),
            AndroidWorkspaceStartDecision::Refused(AndroidWorkspaceRefusalReason::BindingRequired)
        );
        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    workspace_binding_present: Some(false),
                    ..complete
                },
            ),
            AndroidWorkspaceStartDecision::Refused(AndroidWorkspaceRefusalReason::BindingRequired)
        );
        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    runtime_authority_present: None,
                    ..complete
                },
            ),
            AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::AuthorityRequired
            )
        );
        assert_eq!(
            policy.authorize_start(
                AndroidWorkspaceExecution::Local,
                AndroidWorkspaceStartEvidence {
                    runtime_authority_present: Some(false),
                    ..complete
                },
            ),
            AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::AuthorityRequired
            )
        );
    }

    #[test]
    fn hosted_android_workspace_never_inherits_local_consent() {
        let policy = AndroidWorkspacePolicy {
            launch_state: ServiceLaunchState::Available,
            ..AndroidWorkspacePolicy::local_first()
        };
        let local_only_consent = AndroidWorkspaceStartEvidence {
            user_consented: Some(true),
            workspace_binding_present: Some(true),
            runtime_authority_present: Some(true),
            isolation_verified: Some(true),
            hosted_threat_model_consented: None,
        };

        assert_eq!(
            policy.authorize_start(AndroidWorkspaceExecution::Hosted, local_only_consent),
            AndroidWorkspaceStartDecision::Refused(
                AndroidWorkspaceRefusalReason::HostedThreatModelConsentRequired
            )
        );
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceKind {
    Discord,
    Telegram,
    Instagram,
    #[serde(rename = "whatsapp")]
    WhatsApp,
    Messenger,
    Email,
    Signal,
    X,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmailProvider {
    Gmail,
    Outlook,
    Proton,
    Tuta,
    Yahoo,
    Aol,
    Gmx,
    Maildotcom,
    Icloud,
}

impl Default for EmailProvider {
    fn default() -> Self {
        Self::Gmail
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceCategory {
    Consumer,
    Enterprise,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceLaunchState {
    Available,
    ComingSoon,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DemoConnectionState {
    DemoLinked,
    NotLinked,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedAccountDemo {
    pub id: String,
    pub label: String,
    pub display_handle: String,
    pub state: DemoConnectionState,
    /// Present only for Email. The value selects one fixed first-party
    /// webmail manifest; arbitrary user-provided URLs are never persisted.
    pub provider: Option<EmailProvider>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedServiceDemo {
    pub id: ServiceKind,
    pub display_name: String,
    pub sidebar_glyph: String,
    pub sidebar_order: u8,
    pub category: ServiceCategory,
    pub launch_state: ServiceLaunchState,
    /// Present-tense Home caption generated from this service's capability
    /// facts. The renderer must not infer a roadmap promise from launch state.
    pub generated_label: String,
    pub supports_native_preview: bool,
    pub supports_protected_preview: bool,
    pub accounts: Vec<LinkedAccountDemo>,
}
