use osl_privacy_hub::attended_imap::{
    delete_prepared, prepare_delete, ImapMailbox, ImapMessageSnapshot, ImapPolicyError,
    SeededLocalImapFixture,
};

fn self_authored_fixture_message(index: usize) -> ImapMessageSnapshot {
    let fixture = SeededLocalImapFixture::scaffold(17);
    let mut message = fixture.messages()[index].clone();
    message.authored_by_self = true;
    message
}

#[test]
fn attended_imap_rejects_wrong_sender_duplicate_message_id_and_changed_fingerprint() {
    let fixture = SeededLocalImapFixture::scaffold(17);
    let wrong_sender = fixture
        .messages()
        .iter()
        .find(|message| !message.authored_by_self)
        .expect("fixture includes a non-self-authored message");
    let wrong_sender_mailbox = ImapMailbox::from_fixture(&fixture);

    assert_eq!(
        prepare_delete(
            &wrong_sender_mailbox,
            &wrong_sender.owner_osl_user_id,
            &wrong_sender.account_id,
            &wrong_sender.mailbox,
            &wrong_sender.message_id,
        ),
        Err(ImapPolicyError::OwnershipRequired)
    );
    assert_eq!(wrong_sender_mailbox.deleted_count(), 0);

    let first = self_authored_fixture_message(0);
    let mut duplicate = self_authored_fixture_message(1);
    duplicate.account_id = first.account_id.clone();
    duplicate.mailbox = first.mailbox.clone();
    duplicate.message_id = first.message_id.clone();
    let duplicate_mailbox = ImapMailbox::from_messages(vec![first.clone(), duplicate]);

    assert!(duplicate_mailbox.has_duplicate_message_id(
        &first.account_id,
        &first.mailbox,
        &first.message_id
    ));
    assert_eq!(
        prepare_delete(
            &duplicate_mailbox,
            &first.owner_osl_user_id,
            &first.account_id,
            &first.mailbox,
            &first.message_id,
        ),
        Err(ImapPolicyError::DuplicateMessageId)
    );
    assert_eq!(duplicate_mailbox.deleted_count(), 0);

    let original = self_authored_fixture_message(0);
    let mut mailbox = ImapMailbox::from_messages(vec![original.clone()]);
    let prepared = prepare_delete(
        &mailbox,
        &original.owner_osl_user_id,
        &original.account_id,
        &original.mailbox,
        &original.message_id,
    )
    .expect("self-authored unique message prepares for deletion");

    let mut changed = original;
    changed.fingerprint[0] ^= 0xff;
    mailbox.mutate_message(changed);

    assert_eq!(
        delete_prepared(&mut mailbox, &prepared),
        Err(ImapPolicyError::FingerprintMismatch)
    );
    assert_eq!(mailbox.deleted_count(), 0);
}
