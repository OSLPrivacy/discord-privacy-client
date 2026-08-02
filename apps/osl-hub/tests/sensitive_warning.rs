use osl_privacy_hub::sensitive_warning::{before_unencrypted_send, UnencryptedSendDecision};
use sensitive_classify::{
    categories::{WarningCategory},
    policy::{DraftId, WarningPolicy},
};

#[test]
fn t13_tg7_unencrypted_drafts_are_checked_and_policy_can_dismiss_without_blocking() {
    let draft = DraftId::new("draft-1");
    let mut policy = WarningPolicy::new();
    let decision = before_unencrypted_send(&draft, "password: correct horse battery staple", &policy);
    assert!(matches!(decision, UnencryptedSendDecision::Warn { ref categories }
        if categories.contains(&WarningCategory::Credential)));

    policy.dismiss_draft(draft.clone());
    assert_eq!(
        before_unencrypted_send(&draft, "password: correct horse battery staple", &policy),
        UnencryptedSendDecision::SendWithoutWarning,
    );
}
