use serde::{Deserialize, Deserializer, Serialize};

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

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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
            show_plaintext_preview: bool,
            #[serde(default = "default_true")]
            window_capture_enabled: bool,
            acknowledge_experimental_send_risk: bool,
        }

        let preferences = WirePreferences::deserialize(deserializer)?;
        Ok(Self {
            onboarding_complete: preferences.onboarding_complete,
            send_mode: preferences.send_mode,
            placement_mode: preferences.placement_mode,
            show_plaintext_preview: preferences.show_plaintext_preview,
            window_capture_enabled: preferences.window_capture_enabled,
            acknowledge_experimental_send_risk: preferences.acknowledge_experimental_send_risk,
        }
        .fail_closed())
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

#[cfg(test)]
mod tests {
    use super::{OnboardingPreferences, PlacementMode, SendMode, ServiceKind};

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

        let default_contract = serde_json::json!({
            "onboardingComplete": false,
            "sendMode": "manual",
            "placementMode": "atomic",
            "showPlaintextPreview": true,
            "windowCaptureEnabled": true,
            "acknowledgeExperimentalSendRisk": false,
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
            "sendMode": "double",
            "placementMode": "atomic",
            "showPlaintextPreview": true,
            "windowCaptureEnabled": true,
            "acknowledgeExperimentalSendRisk": false,
        });
        assert!(
            !serde_json::from_value::<OnboardingPreferences>(incomplete_risk_acknowledgement)
                .unwrap()
                .onboarding_complete
        );
    }

    #[test]
    fn send_modes_refuse_absent_or_unknown_authority() {
        for raw in ["\"enter\"", "\"Double\"", "\"manual \"", "\"\"", "null"] {
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
