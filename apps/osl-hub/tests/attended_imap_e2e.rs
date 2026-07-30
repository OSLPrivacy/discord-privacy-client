use osl_privacy_hub::attended_imap::{
    delete_prepared, prepare_delete, ImapMailbox, ImapMessageSnapshot, ImapPolicyError,
};
use sha2::{Digest, Sha256};

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
fn attended_imap_rejects_wrong_sender_duplicate_message_id_and_changed_fingerprint() {
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
        Err(ImapPolicyError::AccountBindingMismatch)
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
