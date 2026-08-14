//! Account-consent binding at the Scrub provider-action boundary.
//!
//! A provider tab or native profile can change accounts without ever becoming
//! signed out.  Consequently a selected row, a requested account id, visible
//! UI text, and a provider's self-declared display name are not authority.  The
//! only authority used here is the stable account id returned by an
//! independently authenticated read from the same shipping provider port.

use std::fmt;

/// Carrier sources with shipping account-bearing Scrub routes.
pub const SHIPPING_SCRUB_CARRIER_ROUTES: [&str; 7] = [
    "discord",
    "telegram",
    "whatsapp",
    "x",
    "instagram",
    "messenger",
    "signal",
];

/// Mail sources with shipping account-bearing Scrub routes.  Outlook web and
/// desktop are separate because they have separate live identity readers.
pub const SHIPPING_SCRUB_MAIL_ROUTES: [&str; 10] = [
    "gmail",
    "outlook-web",
    "outlook-desktop",
    "proton",
    "tuta",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
];

pub const SHIPPING_SCRUB_SOURCE_COUNT: usize =
    SHIPPING_SCRUB_CARRIER_ROUTES.len() + SHIPPING_SCRUB_MAIL_ROUTES.len();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrubRunMode {
    Discovery,
    ScheduledFindOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderAction {
    List,
    Open,
    Fetch,
    Search,
    Scroll,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderPortKind {
    ShippingRoute,
    Fixture,
}

impl ProviderAction {
    pub const ALL: [Self; 6] = [
        Self::List,
        Self::Open,
        Self::Fetch,
        Self::Search,
        Self::Scroll,
        Self::Delete,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Open => "open",
            Self::Fetch => "fetch",
            Self::Search => "search",
            Self::Scroll => "scroll",
            Self::Delete => "delete",
        }
    }
}

/// An identity observation made through provider authentication/network state.
///
/// The last three fields are retained for diagnostics only.  They are
/// intentionally never read by the consent comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedProviderAccount {
    pub provider_source: String,
    pub stable_account_id: String,
    pub authenticated_session_id: String,
    pub independent_auth_evidence: String,
    pub requested_account_id: Option<String>,
    pub ui_account_label: Option<String>,
    pub self_declared_account_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScrubConsentBinding {
    pub provider_source: String,
    pub stable_account_id: String,
}

/// The shipping seam.  Both the independent identity read and the action are
/// on the same mutable port, which makes it impossible for a caller to hand the
/// common guard an identity from one provider and act through another port.
pub trait ScrubShippingProviderPort {
    fn port_kind(&self) -> ProviderPortKind;

    fn independently_authenticated_account(
        &mut self,
    ) -> Result<AuthenticatedProviderAccount, String>;

    fn perform_checked_action(
        &mut self,
        authority: &CheckedProviderAction,
    ) -> Result<ProviderActionReceipt, String>;
}

/// Capability created only after the immediately preceding live identity read
/// matched the exact consent binding.  Fields are private so callers cannot
/// construct one around the guard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedProviderAction {
    provider_source: String,
    stable_account_id: String,
    authenticated_session_id: String,
    independent_auth_evidence: String,
    mode: ScrubRunMode,
    action: ProviderAction,
}

impl CheckedProviderAction {
    pub fn provider_source(&self) -> &str {
        &self.provider_source
    }

    pub fn stable_account_id(&self) -> &str {
        &self.stable_account_id
    }

    pub fn authenticated_session_id(&self) -> &str {
        &self.authenticated_session_id
    }

    pub fn independent_auth_evidence(&self) -> &str {
        &self.independent_auth_evidence
    }

    pub const fn mode(&self) -> ScrubRunMode {
        self.mode
    }

    pub const fn action(&self) -> ProviderAction {
        self.action
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderActionReceipt {
    pub provider_source: String,
    pub stable_account_id: String,
    pub action: ProviderAction,
    pub provider_response_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScrubAccountRebindingError {
    UnknownShippingSource(String),
    FixtureProviderRefused {
        provider_source: String,
    },
    InvalidAuthenticatedIdentity {
        provider_source: String,
        reason: &'static str,
    },
    IndependentIdentityReadFailed {
        provider_source: String,
        reason: String,
    },
    FreshApprovalRequired {
        provider_source: String,
    },
    AccountChanged {
        provider_source: String,
        approved_account_id: String,
        live_account_id: String,
    },
    ScheduledFindOnlyDeleteRefused {
        provider_source: String,
        stable_account_id: String,
    },
    ProviderActionFailed {
        provider_source: String,
        stable_account_id: String,
        action: ProviderAction,
        reason: String,
    },
    InvalidProviderReceipt {
        provider_source: String,
        stable_account_id: String,
        action: ProviderAction,
    },
    InvalidPageCount,
}

impl fmt::Display for ScrubAccountRebindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShippingSource(source) => {
                write!(formatter, "Scrub has no shipping provider source {source}")
            }
            Self::FixtureProviderRefused { provider_source } => write!(
                formatter,
                "{provider_source} fixture provider cannot authorize a shipping Scrub action"
            ),
            Self::InvalidAuthenticatedIdentity {
                provider_source,
                reason,
            } => write!(
                formatter,
                "{provider_source} independent account identity is invalid: {reason}; fresh approval required"
            ),
            Self::IndependentIdentityReadFailed {
                provider_source,
                reason,
            } => write!(
                formatter,
                "{provider_source} independent account identity could not be read: {reason}; fresh approval required"
            ),
            Self::FreshApprovalRequired { provider_source } => write!(
                formatter,
                "{provider_source} has no exact live-account consent; fresh account approval required"
            ),
            Self::AccountChanged {
                provider_source,
                approved_account_id,
                live_account_id,
            } => write!(
                formatter,
                "{provider_source} account changed from {approved_account_id} to {live_account_id}; fresh {live_account_id} approval required"
            ),
            Self::ScheduledFindOnlyDeleteRefused {
                provider_source,
                stable_account_id,
            } => write!(
                formatter,
                "Scheduled AutoScrub is Find only; delete refused for {provider_source} account {stable_account_id}"
            ),
            Self::ProviderActionFailed {
                provider_source,
                stable_account_id,
                action,
                reason,
            } => write!(
                formatter,
                "{} action failed for {provider_source} account {stable_account_id}: {reason}",
                action.as_str()
            ),
            Self::InvalidProviderReceipt {
                provider_source,
                stable_account_id,
                action,
            } => write!(
                formatter,
                "{} action for {provider_source} account {stable_account_id} returned a mismatched provider receipt",
                action.as_str()
            ),
            Self::InvalidPageCount => {
                formatter.write_str("Scrub multi-page run needs at least two provider pages")
            }
        }
    }
}

impl std::error::Error for ScrubAccountRebindingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiPageReadOutcome {
    pub pages_completed: usize,
    pub actions_completed: usize,
}

/// Live state for one source and mode.  Selection and approval deliberately
/// travel together and are both erased on an identity read failure or change.
pub struct ScrubAccountConsentGuard {
    provider_source: String,
    mode: ScrubRunMode,
    selected_account: Option<ScrubConsentBinding>,
    approved_account: Option<ScrubConsentBinding>,
}

impl ScrubAccountConsentGuard {
    pub fn new(
        provider_source: impl Into<String>,
        mode: ScrubRunMode,
    ) -> Result<Self, ScrubAccountRebindingError> {
        let provider_source = provider_source.into();
        if !shipping_source_is_known(&provider_source) {
            return Err(ScrubAccountRebindingError::UnknownShippingSource(
                provider_source,
            ));
        }
        Ok(Self {
            provider_source,
            mode,
            selected_account: None,
            approved_account: None,
        })
    }

    pub fn selected_account(&self) -> Option<&ScrubConsentBinding> {
        self.selected_account.as_ref()
    }

    pub fn approved_account(&self) -> Option<&ScrubConsentBinding> {
        self.approved_account.as_ref()
    }

    pub fn clear_account_state(&mut self) {
        self.selected_account = None;
        self.approved_account = None;
    }

    /// Approve the account the provider independently authenticates now.  No
    /// requested id or visible/self-declared label participates in the grant.
    pub fn approve_live_account(
        &mut self,
        port: &mut dyn ScrubShippingProviderPort,
    ) -> Result<ScrubConsentBinding, ScrubAccountRebindingError> {
        let live = self.read_live_account(port)?;
        let binding = ScrubConsentBinding {
            provider_source: live.provider_source,
            stable_account_id: live.stable_account_id,
        };
        self.selected_account = Some(binding.clone());
        self.approved_account = Some(binding.clone());
        Ok(binding)
    }

    /// The only production entry to a provider action.  It independently reads
    /// the live identity immediately before every action, compares exact source
    /// and stable id, clears stale state on any change, and only then creates a
    /// non-constructible checked capability for the shipping port.
    pub fn perform_provider_action(
        &mut self,
        port: &mut dyn ScrubShippingProviderPort,
        action: ProviderAction,
    ) -> Result<ProviderActionReceipt, ScrubAccountRebindingError> {
        let authority = self.authorize_next_action(port, action)?;
        let receipt = port.perform_checked_action(&authority).map_err(|reason| {
            ScrubAccountRebindingError::ProviderActionFailed {
                provider_source: authority.provider_source.clone(),
                stable_account_id: authority.stable_account_id.clone(),
                action,
                reason,
            }
        })?;
        if receipt.provider_source != authority.provider_source
            || receipt.stable_account_id != authority.stable_account_id
            || receipt.action != authority.action
            || invalid_identity_part(&receipt.provider_response_id)
        {
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::InvalidProviderReceipt {
                provider_source: authority.provider_source,
                stable_account_id: authority.stable_account_id,
                action,
            });
        }
        Ok(receipt)
    }

    fn authorize_next_action(
        &mut self,
        port: &mut dyn ScrubShippingProviderPort,
        action: ProviderAction,
    ) -> Result<CheckedProviderAction, ScrubAccountRebindingError> {
        let live = self.read_live_account(port)?;
        let Some(approved) = self.approved_account.clone() else {
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::FreshApprovalRequired {
                provider_source: self.provider_source.clone(),
            });
        };

        if self.selected_account.as_ref() != Some(&approved)
            || approved.provider_source != live.provider_source
            || approved.stable_account_id != live.stable_account_id
        {
            let approved_account_id = approved.stable_account_id;
            let live_account_id = live.stable_account_id;
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::AccountChanged {
                provider_source: self.provider_source.clone(),
                approved_account_id,
                live_account_id,
            });
        }

        if self.mode == ScrubRunMode::ScheduledFindOnly && action == ProviderAction::Delete {
            return Err(ScrubAccountRebindingError::ScheduledFindOnlyDeleteRefused {
                provider_source: live.provider_source,
                stable_account_id: live.stable_account_id,
            });
        }

        Ok(CheckedProviderAction {
            provider_source: live.provider_source,
            stable_account_id: live.stable_account_id,
            authenticated_session_id: live.authenticated_session_id,
            independent_auth_evidence: live.independent_auth_evidence,
            mode: self.mode,
            action,
        })
    }

    fn read_live_account(
        &mut self,
        port: &mut dyn ScrubShippingProviderPort,
    ) -> Result<AuthenticatedProviderAccount, ScrubAccountRebindingError> {
        if port.port_kind() != ProviderPortKind::ShippingRoute {
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::FixtureProviderRefused {
                provider_source: self.provider_source.clone(),
            });
        }
        let live = match port.independently_authenticated_account() {
            Ok(live) => live,
            Err(reason) => {
                self.clear_account_state();
                return Err(ScrubAccountRebindingError::IndependentIdentityReadFailed {
                    provider_source: self.provider_source.clone(),
                    reason,
                });
            }
        };
        if live.provider_source != self.provider_source {
            let approved_account_id = self
                .approved_account
                .as_ref()
                .map(|binding| binding.stable_account_id.clone())
                .unwrap_or_else(|| "unapproved".to_owned());
            let live_account_id = live.stable_account_id;
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::AccountChanged {
                provider_source: self.provider_source.clone(),
                approved_account_id,
                live_account_id,
            });
        }
        if invalid_identity_part(&live.stable_account_id) {
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::InvalidAuthenticatedIdentity {
                provider_source: self.provider_source.clone(),
                reason:
                    "stable account id is empty, untrimmed, oversized, or contains control text",
            });
        }
        if invalid_identity_part(&live.authenticated_session_id)
            || invalid_identity_part(&live.independent_auth_evidence)
        {
            self.clear_account_state();
            return Err(ScrubAccountRebindingError::InvalidAuthenticatedIdentity {
                provider_source: self.provider_source.clone(),
                reason: "authenticated session or independent evidence is missing",
            });
        }
        Ok(live)
    }
}

/// A shipping multi-page read: one provider search followed by list/open/fetch
/// and scroll on every page.  Each call crosses the common live-account guard.
pub fn run_multi_page_read(
    guard: &mut ScrubAccountConsentGuard,
    port: &mut dyn ScrubShippingProviderPort,
    page_count: usize,
) -> Result<MultiPageReadOutcome, ScrubAccountRebindingError> {
    if page_count < 2 {
        return Err(ScrubAccountRebindingError::InvalidPageCount);
    }
    let mut actions_completed = 0;
    guard.perform_provider_action(port, ProviderAction::Search)?;
    actions_completed += 1;
    for page in 0..page_count {
        for action in [
            ProviderAction::List,
            ProviderAction::Open,
            ProviderAction::Fetch,
            ProviderAction::Scroll,
        ] {
            guard.perform_provider_action(port, action)?;
            actions_completed += 1;
        }
        debug_assert!(page < page_count);
    }
    Ok(MultiPageReadOutcome {
        pages_completed: page_count,
        actions_completed,
    })
}

pub fn verify_shipping_scrub_source_inventory(
    carriers: &[&str],
    mail: &[&str],
) -> Result<(), String> {
    verify_exact_inventory("carrier", carriers, &SHIPPING_SCRUB_CARRIER_ROUTES)?;
    verify_exact_inventory("mail", mail, &SHIPPING_SCRUB_MAIL_ROUTES)
}

fn verify_exact_inventory(kind: &str, actual: &[&str], expected: &[&str]) -> Result<(), String> {
    for source in expected {
        match actual
            .iter()
            .filter(|candidate| *candidate == source)
            .count()
        {
            0 => {
                return Err(format!(
                    "shipping Scrub {kind} inventory is missing {source}"
                ))
            }
            1 => {}
            _ => {
                return Err(format!(
                    "shipping Scrub {kind} inventory duplicates {source}"
                ))
            }
        }
    }
    if actual.len() != expected.len() {
        let unexpected = actual
            .iter()
            .find(|source| !expected.contains(source))
            .copied()
            .unwrap_or("route count");
        return Err(format!(
            "shipping Scrub {kind} inventory has unexpected {unexpected}"
        ));
    }
    if actual != expected {
        return Err(format!(
            "shipping Scrub {kind} inventory no longer matches shipping route order"
        ));
    }
    Ok(())
}

fn shipping_source_is_known(source: &str) -> bool {
    SHIPPING_SCRUB_CARRIER_ROUTES.contains(&source) || SHIPPING_SCRUB_MAIL_ROUTES.contains(&source)
}

fn invalid_identity_part(value: &str) -> bool {
    value.is_empty()
        || value.trim() != value
        || value.len() > 512
        || value.chars().any(char::is_control)
}
