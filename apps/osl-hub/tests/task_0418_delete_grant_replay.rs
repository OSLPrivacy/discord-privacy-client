use osl_privacy_hub::attended_imap::{
    authorize_attended_imap_batch, authorize_attended_imap_batch_reviewed,
    delete_prepared_with_grant, prepare_delete, ImapDeleteContext, ImapDeletePhase,
    ImapEntitlement, ImapMailbox, ImapPolicyError, SeededLocalImapFixture,
};

#[test]
fn task_0418_exact_same_permitted_delete_command_replay_is_refused() {
    let fixture = SeededLocalImapFixture::scaffold(418);
    let first = fixture.messages()[0].clone();
    let mut mailbox = ImapMailbox::from_fixture(&fixture);
    let prepared = prepare_delete(
        &mailbox,
        &first.owner_osl_user_id,
        &first.account_id,
        &first.mailbox,
        &first.message_id,
    )
    .expect("fixture message is owned and deletable");
    let reviewed = authorize_attended_imap_batch_reviewed(
        std::slice::from_ref(&prepared),
        &prepared.owner_osl_user_id,
        &prepared.account_id,
    )
    .expect("single prepared delete has been reviewed");
    let mut authorizer = Default::default();
    let mut grant = authorize_attended_imap_batch(&mut authorizer, reviewed, 10_000, 5_000)
        .expect("reviewed batch authorizes one grant");
    grant.phase = ImapDeletePhase::Executing;
    grant.message_fingerprints.insert(prepared.fingerprint);
    let context = ImapDeleteContext {
        entitlement: ImapEntitlement::Pro,
        phase: ImapDeletePhase::Executing,
        now_unix_ms: 10_001,
    };

    let first_result = delete_prepared_with_grant(&mut mailbox, &mut grant, context, &prepared);
    assert!(first_result.is_ok());
    assert_eq!(mailbox.deleted_count(), 1);

    let replay_refusal =
        delete_prepared_with_grant(&mut mailbox, &mut grant, context, &prepared).unwrap_err();
    assert_eq!(replay_refusal, ImapPolicyError::SingleUseAuthorityRequired);
    assert_eq!(mailbox.deleted_count(), 1);

    println!(
        "TASK0418 first_request=ok deleted_count_after_first={} second_refusal={replay_refusal:?} second_refusal_text=\"{}\" deleted_count_after_second={}",
        mailbox.deleted_count(),
        replay_refusal,
        mailbox.deleted_count()
    );
}
