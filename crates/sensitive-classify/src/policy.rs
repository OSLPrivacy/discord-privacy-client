//! Content-free state for the sensitive-content consequence warning.
//!
//! This policy remembers only choices: whether warnings are enabled, which
//! categories are muted, and which active drafts have been dismissed. It does
//! not accept, retain, or derive any scanned message content.

use std::collections::BTreeSet;

/// Opaque identifier for the active compose draft.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DraftId(String);

impl DraftId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// The result of applying the user's saved warning choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarningDecision {
    Warn,
    SilentGlobalDisabled,
    SilentDraftDismissed,
    SilentCategoryMuted,
}

/// Content-free choices governing when a consequence warning may appear.
#[derive(Clone, Debug)]
pub struct WarningPolicy<Category> {
    enabled: bool,
    muted_categories: BTreeSet<Category>,
    dismissed_drafts: BTreeSet<DraftId>,
}

impl<Category: Ord> Default for WarningPolicy<Category> {
    fn default() -> Self {
        Self {
            enabled: true,
            muted_categories: BTreeSet::new(),
            dismissed_drafts: BTreeSet::new(),
        }
    }
}

impl<Category: Ord> WarningPolicy<Category> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn mute_category(&mut self, category: Category) {
        self.muted_categories.insert(category);
    }

    pub fn unmute_category(&mut self, category: &Category) {
        self.muted_categories.remove(category);
    }

    /// Dismiss all further warnings for this active draft only.
    pub fn dismiss_draft(&mut self, draft: DraftId) {
        self.dismissed_drafts.insert(draft);
    }

    /// Call when a draft is sent or discarded so its opaque identifier does
    /// not outlive the draft.
    pub fn forget_draft(&mut self, draft: &DraftId) {
        self.dismissed_drafts.remove(draft);
    }

    pub fn decision_for(&self, draft: &DraftId, category: &Category) -> WarningDecision {
        if !self.enabled {
            return WarningDecision::SilentGlobalDisabled;
        }
        if self.muted_categories.contains(category) {
            return WarningDecision::SilentCategoryMuted;
        }
        if self.dismissed_drafts.contains(draft) {
            return WarningDecision::SilentDraftDismissed;
        }
        WarningDecision::Warn
    }
}

#[cfg(test)]
mod tests {
    use super::{DraftId, WarningDecision, WarningPolicy};

    #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
    enum Category {
        Credential,
        PaymentCard,
    }

    #[test]
    fn dismissed_once_is_silent_for_that_draft_but_not_the_next_one() {
        let mut policy = WarningPolicy::new();
        let first = DraftId::new("draft-a");
        let next = DraftId::new("draft-b");

        assert_eq!(
            policy.decision_for(&first, &Category::Credential),
            WarningDecision::Warn
        );
        policy.dismiss_draft(first.clone());
        assert_eq!(
            policy.decision_for(&first, &Category::Credential),
            WarningDecision::SilentDraftDismissed
        );
        assert_eq!(
            policy.decision_for(&next, &Category::Credential),
            WarningDecision::Warn
        );
    }

    #[test]
    fn category_mute_and_global_switch_are_silent_without_recording_findings() {
        let mut policy = WarningPolicy::new();
        let draft = DraftId::new("draft-a");
        policy.mute_category(Category::Credential);
        assert_eq!(
            policy.decision_for(&draft, &Category::Credential),
            WarningDecision::SilentCategoryMuted
        );
        assert_eq!(
            policy.decision_for(&draft, &Category::PaymentCard),
            WarningDecision::Warn
        );

        policy.set_enabled(false);
        assert_eq!(
            policy.decision_for(&draft, &Category::PaymentCard),
            WarningDecision::SilentGlobalDisabled
        );
    }
}
