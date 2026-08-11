//! Shipping email sender discovery and rule enforcement.
//!
//! Stable identities originate only from messages delivered by an
//! independently verified connector retained in the shipping registry.

use crate::allowed_places::AllowedPlaceRecord;
use crate::auto_whitelist_rules::AutoWhitelistChoice;
use crate::email_whitelist_kinds::EmailWhitelistKind;
use crate::state::AppState;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippingEmailConnector {
    pub provider_id: &'static str,
    pub supports_sender_discovery: bool,
    pub supports_provider_action: bool,
}

pub const SHIPPING_EMAIL_CONNECTORS: [ShippingEmailConnector; 1] = [ShippingEmailConnector {
    provider_id: "gmail",
    supports_sender_discovery: true,
    supports_provider_action: true,
}];

#[derive(Debug, Clone, Copy, Default)]
pub struct ShippingEmailProviderRegistry;

impl ShippingEmailProviderRegistry {
    pub const fn authoritative() -> Self {
        Self
    }

    pub fn supported_connector(&self, provider_id: &str) -> Option<ShippingEmailConnector> {
        SHIPPING_EMAIL_CONNECTORS.into_iter().find(|connector| {
            connector.provider_id == provider_id
                && connector.supports_sender_discovery
                && connector.supports_provider_action
        })
    }

    pub fn verify_connected_account(
        &self,
        provider_id: &str,
        account_id: &str,
        connector_revision: &str,
    ) -> Result<VerifiedShippingEmailAccount, String> {
        let connector = self.supported_connector(provider_id).ok_or_else(|| {
            format!("OSL: email provider '{provider_id}' is not a supported shipping connector")
        })?;
        if account_id.is_empty() || account_id != account_id.trim() {
            return Err("OSL: verified email account id is invalid".to_owned());
        }
        if connector_revision.is_empty() || connector_revision != connector_revision.trim() {
            return Err("OSL: verified email connector revision is invalid".to_owned());
        }
        Ok(VerifiedShippingEmailAccount {
            connector,
            account_id: account_id.to_owned(),
            connector_revision: connector_revision.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedShippingEmailAccount {
    connector: ShippingEmailConnector,
    account_id: String,
    connector_revision: String,
}

impl VerifiedShippingEmailAccount {
    pub fn provider_id(&self) -> &'static str {
        self.connector.provider_id
    }

    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    pub fn connector_revision(&self) -> &str {
        &self.connector_revision
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedProviderEmail {
    pub provider_message_id: String,
    pub sender_header: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredEmailSender {
    pub provider_message_id: String,
    pub sender_address: String,
    pub sender_domain: String,
    pub address_stable_id: String,
    pub domain_stable_id: String,
}

pub fn discover_received_sender(
    account: &VerifiedShippingEmailAccount,
    received: &ReceivedProviderEmail,
) -> Result<DiscoveredEmailSender, String> {
    if received.provider_message_id.trim().is_empty() {
        return Err("OSL: provider message id is missing".to_owned());
    }
    let address = parse_sender_address(&received.sender_header)?;
    let (_, domain) = address
        .rsplit_once('@')
        .ok_or_else(|| "OSL: provider sender address is invalid".to_owned())?;
    let sender_domain = domain.to_ascii_lowercase();
    Ok(DiscoveredEmailSender {
        provider_message_id: received.provider_message_id.clone(),
        address_stable_id: discovered_stable_id(account, "sender_address", &address),
        domain_stable_id: discovered_stable_id(account, "sender_domain", &sender_domain),
        sender_address: address,
        sender_domain,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailRuleTarget {
    SenderAddress,
    SenderDomain,
}

impl EmailRuleTarget {
    pub const fn whitelist_kind(self) -> EmailWhitelistKind {
        match self {
            Self::SenderAddress => EmailWhitelistKind::EmailAddress,
            Self::SenderDomain => EmailWhitelistKind::EmailDomain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShippingEmailActionReceipt {
    pub provider_id: String,
    pub provider_message_id: String,
    pub target_kind: String,
    pub stable_id: String,
    pub rule_choice: String,
    pub outcome: String,
}

pub fn run_shipping_email_action(
    state: &AppState,
    app_data_dir: impl AsRef<Path>,
    account: &VerifiedShippingEmailAccount,
    received: &ReceivedProviderEmail,
    target: EmailRuleTarget,
) -> Result<ShippingEmailActionReceipt, String> {
    let discovered = discover_received_sender(account, received)?;
    let kind = target.whitelist_kind();
    let choice = state
        .app_preferences
        .lock()
        .expect("app_preferences mutex poisoned")
        .auto_whitelist_rules
        .get(kind.rule_key())
        .copied()
        .unwrap_or_default();
    let (stable_id, identity) = match target {
        EmailRuleTarget::SenderAddress => {
            (&discovered.address_stable_id, &discovered.sender_address)
        }
        EmailRuleTarget::SenderDomain => (&discovered.domain_stable_id, &discovered.sender_domain),
    };
    let allowed = match choice {
        AutoWhitelistChoice::Always => true,
        AutoWhitelistChoice::OnlyIfAFriend if matches!(target, EmailRuleTarget::SenderAddress) => {
            state
                .friend_ids
                .lock()
                .expect("friend_ids mutex poisoned")
                .iter()
                .any(|friend| friend.eq_ignore_ascii_case(identity))
        }
        _ => false,
    };
    let outcome = if allowed {
        let place = AllowedPlaceRecord::from_parts(
            "email",
            account.account_id(),
            kind.allowed_place_kind(),
            stable_id,
        );
        if !crate::allowed_places::is_allowed_place_record(&app_data_dir, &place)
            .map_err(|error| format!("OSL: shipping email allowed-place read refused: {error}"))?
        {
            crate::allowed_places::add_allowed_place_record(app_data_dir.as_ref(), place).map_err(
                |error| format!("OSL: shipping email allowed-place write refused: {error}"),
            )?;
        }
        "provider_allowed"
    } else {
        match target {
            EmailRuleTarget::SenderAddress => "provider_refused_address_rule",
            EmailRuleTarget::SenderDomain => "provider_refused_domain_rule",
        }
    };
    Ok(ShippingEmailActionReceipt {
        provider_id: account.provider_id().to_owned(),
        provider_message_id: discovered.provider_message_id,
        target_kind: kind.id().to_owned(),
        stable_id: stable_id.clone(),
        rule_choice: choice.label().to_owned(),
        outcome: outcome.to_owned(),
    })
}

fn parse_sender_address(header: &str) -> Result<String, String> {
    let trimmed = header.trim();
    let candidate = match (trimmed.rfind('<'), trimmed.rfind('>')) {
        (Some(start), Some(end)) if start < end && end == trimmed.len() - 1 => {
            &trimmed[start + 1..end]
        }
        (None, None) => trimmed,
        _ => return Err("OSL: provider sender header is invalid".to_owned()),
    }
    .trim();
    if candidate.is_empty()
        || candidate.contains(char::is_whitespace)
        || candidate.matches('@').count() != 1
    {
        return Err("OSL: provider sender address is invalid".to_owned());
    }
    let (local, domain) = candidate
        .split_once('@')
        .ok_or_else(|| "OSL: provider sender address is invalid".to_owned())?;
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || domain.split('.').any(|label| {
            label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    {
        return Err("OSL: provider sender address is invalid".to_owned());
    }
    Ok(format!(
        "{}@{}",
        local.to_ascii_lowercase(),
        domain.to_ascii_lowercase()
    ))
}

fn discovered_stable_id(
    account: &VerifiedShippingEmailAccount,
    kind: &str,
    identity: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"osl.shipping-email.sender.v1\0");
    hasher.update(account.provider_id().as_bytes());
    hasher.update(b"\0");
    hasher.update(account.account_id().as_bytes());
    hasher.update(b"\0");
    hasher.update(kind.as_bytes());
    hasher.update(b"\0");
    hasher.update(identity.as_bytes());
    let digest = hasher.finalize();
    let encoded: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("email:{}:{kind}:{encoded}", account.account_id())
}
