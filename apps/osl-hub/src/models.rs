use serde::{Deserialize, Serialize};

/// How an encrypted capsule would be handed to a service composer.
///
/// These values are preferences only in this isolated preview. This crate does
/// not implement keyboard control, clipboard writes, or platform automation.
#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum SendMode {
    #[default]
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "clipboard")]
    Clipboard,
    #[serde(rename = "double")]
    DoubleEnter,
    #[serde(rename = "single")]
    SingleEnter,
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

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OnboardingPreferences {
    pub onboarding_complete: bool,
    pub send_mode: SendMode,
    pub placement_mode: PlacementMode,
    pub show_plaintext_preview: bool,
    #[serde(default = "default_true")]
    pub window_capture_enabled: bool,
    pub acknowledge_experimental_send_risk: bool,
}

fn default_true() -> bool {
    true
}

impl Default for OnboardingPreferences {
    fn default() -> Self {
        Self {
            onboarding_complete: false,
            send_mode: SendMode::Manual,
            placement_mode: PlacementMode::Atomic,
            show_plaintext_preview: true,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: false,
        }
    }
}

impl OnboardingPreferences {
    /// Enforce the safety invariant at the native trust boundary.
    ///
    /// Experimental Enter modes cannot be treated as fully configured unless
    /// their exact risk acknowledgement is present. Non-experimental modes do
    /// not retain a stale acknowledgement from an earlier selection.
    pub fn fail_closed(mut self) -> Self {
        match self.send_mode {
            SendMode::DoubleEnter | SendMode::SingleEnter => {
                if !self.acknowledge_experimental_send_risk {
                    self.onboarding_complete = false;
                }
            }
            SendMode::Manual | SendMode::Clipboard => {
                self.acknowledge_experimental_send_risk = false;
            }
        }
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
        AndroidWorkspaceBoundary, AndroidWorkspaceExecution, AndroidWorkspacePolicy,
        AndroidWorkspaceRefusalReason, AndroidWorkspaceStartDecision,
        AndroidWorkspaceStartEvidence, OnboardingPreferences, PlacementMode, SendMode, ServiceKind,
        ServiceLaunchState,
    };

    #[test]
    fn send_modes_match_the_frontend_contract() {
        let cases = [
            (SendMode::Manual, "\"manual\""),
            (SendMode::Clipboard, "\"clipboard\""),
            (SendMode::DoubleEnter, "\"double\""),
            (SendMode::SingleEnter, "\"single\""),
        ];

        for (mode, expected) in cases {
            assert_eq!(serde_json::to_string(&mode).unwrap(), expected);
            assert_eq!(serde_json::from_str::<SendMode>(expected).unwrap(), mode);
        }
    }

    #[test]
    fn experimental_modes_cannot_skip_risk_setup() {
        let preferences = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::SingleEnter,
            placement_mode: PlacementMode::Atomic,
            show_plaintext_preview: true,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: false,
        }
        .fail_closed();

        assert!(!preferences.onboarding_complete);
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
    #[serde(rename = "whatsapp")]
    WhatsApp,
    Instagram,
    Snapchat,
    Email,
    X,
    Signal,
    Slack,
    Linkedin,
    Teams,
    Messenger,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EmailProvider {
    Gmail,
    Outlook,
    Proton,
    Tuta,
    Fastmail,
    Yahoo,
    Zoho,
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
    pub supports_native_preview: bool,
    pub supports_protected_preview: bool,
    pub accounts: Vec<LinkedAccountDemo>,
}
