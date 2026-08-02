//! Independent verification for hosted Scrub UI automation.
//!
//! A row disappearing from a virtualised, filtered, or paginated list is a UI
//! event, not proof of deletion.  This module makes the second, independent
//! provider surface mandatory before a receipt can say `VerifiedGone`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedVerification {
    VerifiedGone,
    StillPresent,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedVerifyResult {
    /// The provider confirmed that the independent surface covered this id.
    pub covered: bool,
    /// Whether that independent surface re-resolved the stable item id.
    pub present: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostedVerifyError {
    Navigation,
    FilteredOrVirtualised,
    SessionChanged,
    SchemaDrift,
    Transport,
}

/// The only interface adapters need for post-delete readback.  Its method is
/// intentionally id-based; DOM row absence never enters this boundary.
pub trait IndependentHostedSurface {
    fn resolve_item_id(&mut self, item_id: &str) -> Result<HostedVerifyResult, HostedVerifyError>;
}

pub fn verify_on_independent_surface(
    surface: &mut dyn IndependentHostedSurface,
    item_id: &str,
) -> HostedVerification {
    if item_id.trim().is_empty() {
        return HostedVerification::Unknown;
    }
    match surface.resolve_item_id(item_id) {
        Ok(HostedVerifyResult { covered: true, present: false }) => HostedVerification::VerifiedGone,
        Ok(HostedVerifyResult { present: true, .. }) => HostedVerification::StillPresent,
        // A filtered, virtualised or paginated UI cannot establish absence.
        Ok(HostedVerifyResult { covered: false, present: false }) | Err(_) => HostedVerification::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(Result<HostedVerifyResult, HostedVerifyError>);
    impl IndependentHostedSurface for Fixture {
        fn resolve_item_id(&mut self, _: &str) -> Result<HostedVerifyResult, HostedVerifyError> { self.0 }
    }

    #[test]
    fn scr_h7_virtualised_or_filtered_row_absence_is_unknown() {
        let mut virtualised = Fixture(Ok(HostedVerifyResult { covered: false, present: false }));
        assert_eq!(verify_on_independent_surface(&mut virtualised, "tweet-1"), HostedVerification::Unknown);

        let mut present = Fixture(Ok(HostedVerifyResult { covered: true, present: true }));
        assert_eq!(verify_on_independent_surface(&mut present, "tweet-1"), HostedVerification::StillPresent);

        let mut gone = Fixture(Ok(HostedVerifyResult { covered: true, present: false }));
        assert_eq!(verify_on_independent_surface(&mut gone, "tweet-1"), HostedVerification::VerifiedGone);
    }
}
