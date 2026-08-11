//! Production boundary for new places observed by a signed-in Windows provider.
//!
//! A provider may not manufacture a record or bypass the saved per-kind rule.
//! It supplies the account-bound observation; this connector derives the stable
//! identifier and sends that observation through `apply_discovered_place`.

use crate::allowed_places::{read_allowed_place_record, AllowedPlaceRecord};
use crate::auto_whitelist_rules::DiscordWhitelistKind;
use crate::commands::{apply_discovered_place, NewPlaceDecisionDto};
use crate::state::AppState;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShippingWindowsProvider {
    Discord,
}

impl ShippingWindowsProvider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Discord => "discord",
        }
    }

    pub fn supported_place_kinds(self) -> Vec<&'static str> {
        match self {
            Self::Discord => DiscordWhitelistKind::ALL
                .into_iter()
                .map(DiscordWhitelistKind::id)
                .collect(),
        }
    }
}

/// A session which the shipping Windows connector has already established.
/// The account is kept here, rather than accepted from the discovery event, so
/// a discovery cannot escape the signed-in provider account that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedInWindowsProviderSession {
    provider: ShippingWindowsProvider,
    account: String,
}

impl SignedInWindowsProviderSession {
    pub fn discord(account: impl Into<String>) -> Result<Self, String> {
        let account = account.into();
        if account.trim().is_empty() {
            return Err("OSL: signed-in Discord account is missing".to_string());
        }
        Ok(Self {
            provider: ShippingWindowsProvider::Discord,
            account,
        })
    }

    pub fn provider(&self) -> ShippingWindowsProvider {
        self.provider
    }

    pub fn account(&self) -> &str {
        &self.account
    }
}

/// The minimal provider event emitted after the live Windows provider finds a
/// new place.  Its stable ID is intentionally absent: the connector derives it
/// only after validating the provider session and supported kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDiscoveryEvent {
    pub kind: String,
    pub provider_place_id: String,
    pub place_name: String,
    pub person_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDiscoveryOutcome {
    pub provider: String,
    pub stable_id: String,
    pub added: bool,
    pub decision: NewPlaceDecisionDto,
}

/// Observes a place from the production Windows-provider path.
///
/// A missing session, an unsupported kind, and malformed provider IDs all
/// refuse before the allowed-place store is touched.  Repeated observations of
/// the exact derived stable ID are idempotent and never re-prompt.
pub fn observe_shipping_provider_discovery(
    state: &AppState,
    app_data_dir: &Path,
    session: Option<&SignedInWindowsProviderSession>,
    event: ProviderDiscoveryEvent,
) -> Result<ProviderDiscoveryOutcome, String> {
    let session =
        session.ok_or_else(|| "OSL: signed-in Windows provider session is required".to_string())?;
    let provider = session.provider();
    let kind = event
        .kind
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_");
    if !provider.supported_place_kinds().contains(&kind.as_str()) {
        return Err(format!(
            "OSL: unsupported {} discovery kind '{}'",
            provider.id(),
            event.kind
        ));
    }
    let place_id = event.provider_place_id.trim();
    if place_id.is_empty() || place_id.contains(':') || place_id.contains('\0') {
        return Err("OSL: provider discovery place ID is invalid".to_string());
    }

    let stable_id = format!(
        "{}:{}:{}:{}",
        provider.id(),
        session.account(),
        kind,
        place_id
    );
    let record = AllowedPlaceRecord {
        app: provider.id().to_string(),
        account: session.account().to_string(),
        kind,
        stable_id: stable_id.clone(),
        place_name: event.place_name,
        person_name: event.person_name,
    };
    record.validate().map_err(|error| format!("OSL: {error}"))?;

    if read_allowed_place_record(app_data_dir, &stable_id)
        .map_err(|error| format!("OSL: read discovered allowed place: {error}"))?
        .is_some()
    {
        let app_kind = format!("{}:{}", provider.id(), record.kind);
        return Ok(ProviderDiscoveryOutcome {
            provider: provider.id().to_string(),
            stable_id: stable_id.clone(),
            added: false,
            decision: NewPlaceDecisionDto {
                status: "already_allowed".to_string(),
                rule: "always".to_string(),
                prompt: false,
                allow_request: None,
                place: record,
                result: "already_allowed".to_string(),
                rule_choice: "always".to_string(),
                app_kind,
                stable_id: stable_id.clone(),
            },
        });
    }

    let decision = apply_discovered_place(state, record, Some(app_data_dir.to_path_buf()))?;
    Ok(ProviderDiscoveryOutcome {
        provider: provider.id().to_string(),
        stable_id,
        added: decision.status == "allowed",
        decision,
    })
}
