#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailWhitelistKind {
    EmailAddress,
    EmailDomain,
}

impl EmailWhitelistKind {
    pub const ALL: [Self; 2] = [Self::EmailAddress, Self::EmailDomain];

    pub const fn name(self) -> &'static str {
        match self {
            Self::EmailAddress => "email address",
            Self::EmailDomain => "email domain",
        }
    }
}
