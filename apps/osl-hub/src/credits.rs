//! Honest presentation state for a credit balance.
//!
//! The ledger owns the balance; this module only makes it safe to present after
//! a refresh attempt. A refresh failure can never make a cached balance look
//! current, and the absence of a cached balance stays unknown.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditBalance(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BalanceDisplay {
    Current(CreditBalance),
    Stale(CreditBalance),
    Unknown,
}

impl BalanceDisplay {
    /// Only a successfully refreshed balance may be presented as current.
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Current(_))
    }

    /// A numeric balance is available only when it is known, whether current
    /// or explicitly marked stale by the caller's renderer.
    pub const fn balance(self) -> Option<CreditBalance> {
        match self {
            Self::Current(balance) | Self::Stale(balance) => Some(balance),
            Self::Unknown => None,
        }
    }
}

/// Projects a ledger refresh into the state that the UI must render.
pub const fn display_after_refresh(
    cached: Option<CreditBalance>,
    refreshed: Result<CreditBalance, ()>,
) -> BalanceDisplay {
    match refreshed {
        Ok(balance) => BalanceDisplay::Current(balance),
        Err(()) => match cached {
            Some(balance) => BalanceDisplay::Stale(balance),
            None => BalanceDisplay::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{display_after_refresh, BalanceDisplay, CreditBalance};

    #[test]
    fn stale_balance_is_labelled_stale_after_a_failed_refresh() {
        let display = display_after_refresh(Some(CreditBalance(7)), Err(()));

        assert_eq!(display, BalanceDisplay::Stale(CreditBalance(7)));
        assert!(!display.is_current());
        assert_eq!(display.balance(), Some(CreditBalance(7)));
    }

    #[test]
    fn missing_balance_after_a_failed_refresh_is_unknown_not_success() {
        let display = display_after_refresh(None, Err(()));

        assert_eq!(display, BalanceDisplay::Unknown);
        assert!(!display.is_current());
        assert_eq!(display.balance(), None);
    }

    #[test]
    fn successful_refresh_replaces_the_cached_balance() {
        let display = display_after_refresh(Some(CreditBalance(7)), Ok(CreditBalance(3)));

        assert_eq!(display, BalanceDisplay::Current(CreditBalance(3)));
        assert!(display.is_current());
    }
}
