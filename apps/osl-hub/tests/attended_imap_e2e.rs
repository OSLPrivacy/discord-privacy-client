use osl_privacy_hub::attended_imap::{
    delete_prepared, prepare_delete, ImapMailbox, ImapMessageSnapshot, ImapPolicyError,
    SeededLocalImapFixture,
};
use sha2::{Digest, Sha256};

fn self_authored_fixture_message(index: usize) -> ImapMessageSnapshot {
    let fixture = SeededLocalImapFixture::scaffold(17);
    let mut message = fixture.messages()[index].clone();
    message.authored_by_self = true;
    message
}

fn fingerprint(account_id: &str, mailbox: &str, message_id: &str, uid: u32) -> [u8; 32] {
    let mut bytes = Vec::new();
    for part in [
        account_id.as_bytes(),
        mailbox.as_bytes(),
        message_id.as_bytes(),
    ] {
        bytes.extend_from_slice(&(part.len() as u32).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    bytes.extend_from_slice(&uid.to_be_bytes());
    Sha256::digest(bytes).into()
}

fn message(
    owner_osl_user_id: &str,
    account_id: &str,
    mailbox: &str,
    message_id: &str,
    uid: u32,
    authored_by_self: bool,
) -> ImapMessageSnapshot {
    ImapMessageSnapshot {
        owner_osl_user_id: owner_osl_user_id.to_owned(),
        account_id: account_id.to_owned(),
        mailbox: mailbox.to_owned(),
        message_id: message_id.to_owned(),
        uid,
        fingerprint: fingerprint(account_id, mailbox, message_id, uid),
        authored_by_self,
    }
}

#[test]
fn attended_imap_fixture_rejects_wrong_sender_duplicate_message_id_and_changed_fingerprint() {
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

#[test]
fn attended_imap_manual_snapshots_reject_wrong_sender_duplicate_and_uid_change() {
    let owner = "owner-local-1";
    let account = "account-local-1";
    let mailbox_name = "INBOX";
    let message_id = "<msg-1@local.test>";

    let wrong_sender = ImapMailbox::from_messages(vec![message(
        owner,
        account,
        mailbox_name,
        message_id,
        10,
        false,
    )]);
    assert_eq!(
        prepare_delete(&wrong_sender, owner, account, mailbox_name, message_id),
        Err(ImapPolicyError::OwnershipRequired)
    );

    let duplicate = ImapMailbox::from_messages(vec![
        message(owner, account, mailbox_name, message_id, 10, true),
        message(owner, account, mailbox_name, message_id, 11, true),
    ]);
    assert_eq!(
        prepare_delete(&duplicate, owner, account, mailbox_name, message_id),
        Err(ImapPolicyError::DuplicateMessageId)
    );

    let mut changed = ImapMailbox::from_messages(vec![message(
        owner,
        account,
        mailbox_name,
        message_id,
        10,
        true,
    )]);
    let prepared = prepare_delete(&changed, owner, account, mailbox_name, message_id)
        .expect("initial self-authored message prepares");
    changed.mutate_message(message(owner, account, mailbox_name, message_id, 12, true));
    assert_eq!(
        delete_prepared(&mut changed, &prepared),
        Err(ImapPolicyError::FingerprintMismatch)
    );
    assert_eq!(changed.deleted_count(), 0);
}
