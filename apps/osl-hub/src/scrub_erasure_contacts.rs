//! Explicit provider contacts for locally composed erasure requests.
//!
//! This registry is deliberately closed. A provider missing from it is not
//! guessed from its domain: sending an erasure request to a guessed address
//! could disclose the user's personal data to an unrelated recipient.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureContactRoute {
    /// A provider-published privacy mailbox, with the page that documents it.
    PrivacyEmail {
        email: &'static str,
        documentation_url: &'static str,
    },
    /// A provider-published request form or account-settings route.
    RequestUrl(&'static str),
}

impl ErasureContactRoute {
    pub const fn is_complete(self) -> bool {
        match self {
            Self::PrivacyEmail {
                email,
                documentation_url,
            } => !email.is_empty() && !documentation_url.is_empty(),
            Self::RequestUrl(url) => !url.is_empty(),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProviderErasureContact {
    pub provider_id: &'static str,
    pub route: ErasureContactRoute,
}

const ERASURE_CONTACTS: &[ProviderErasureContact] = &[
    // Discord documents this mailbox for Data & Privacy request assistance.
    // Its data package must be requested before deleting the account, because
    // Discord cancels an outstanding request when the account is deleted.
    ProviderErasureContact {
        provider_id: "discord",
        route: ErasureContactRoute::PrivacyEmail {
            email: "privacy@discord.com",
            documentation_url:
                "https://support.discord.com/hc/en-us/articles/360004027692-Requesting-a-Copy-of-your-Data",
        },
    },
    ProviderErasureContact {
        provider_id: "x",
        route: ErasureContactRoute::RequestUrl("https://help.x.com/en/forms/privacy"),
    },
];

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ErasureContactRefusal {
    UnknownProvider,
}

/// Returns only an explicitly registered provider route.
///
/// Callers must present a manual route or decline composition when this
/// refuses; they must never derive an address from `provider_id`.
pub fn resolve_erasure_contact(
    provider_id: &str,
) -> Result<&'static ProviderErasureContact, ErasureContactRefusal> {
    ERASURE_CONTACTS
        .iter()
        .find(|contact| contact.provider_id == provider_id)
        .ok_or(ErasureContactRefusal::UnknownProvider)
}

pub fn registered_erasure_contacts() -> &'static [ProviderErasureContact] {
    ERASURE_CONTACTS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scr_g2_every_registered_provider_has_an_explicit_contact_or_request_url() {
        assert!(!registered_erasure_contacts().is_empty());
        for contact in registered_erasure_contacts() {
            assert!(!contact.provider_id.is_empty());
            assert!(contact.route.is_complete());
        }
    }

    #[test]
    fn scr_g2_missing_provider_refuses_instead_of_guessing_an_address() {
        assert_eq!(
            resolve_erasure_contact("unregistered.example"),
            Err(ErasureContactRefusal::UnknownProvider)
        );
    }

    #[test]
    fn scr_g2_known_provider_keeps_its_documented_route() {
        assert_eq!(
            resolve_erasure_contact("discord"),
            Ok(&ProviderErasureContact {
                provider_id: "discord",
                route: ErasureContactRoute::PrivacyEmail {
                    email: "privacy@discord.com",
                    documentation_url: "https://support.discord.com/hc/en-us/articles/360004027692-Requesting-a-Copy-of-your-Data",
                },
            })
        );
    }
}
