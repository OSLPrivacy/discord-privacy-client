#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailWhitelistKind {
    EmailAddress,
    EmailDomain,
}

impl EmailWhitelistKind {
    pub const ALL: [Self; 2] = [Self::EmailAddress, Self::EmailDomain];

    pub const fn id(self) -> &'static str {
        match self {
            Self::EmailAddress => "address",
            Self::EmailDomain => "domain",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::EmailAddress => "email address",
            Self::EmailDomain => "email domain",
        }
    }
}

pub fn parse_email_whitelist_kind(input: &str) -> Result<EmailWhitelistKind, String> {
    let normalized = input.trim().to_ascii_lowercase().replace('-', "_");
    EmailWhitelistKind::ALL
        .into_iter()
        .find(|kind| normalized == kind.id() || normalized == kind.name())
        .ok_or_else(|| format!("OSL: unknown email whitelist kind '{input}'"))
}
