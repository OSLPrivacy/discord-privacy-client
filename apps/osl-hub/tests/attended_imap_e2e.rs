use osl_privacy_hub::attended_imap::{
    authorize_attended_imap_batch, authorize_attended_imap_batch_reviewed, delete_prepared,
    prepare_delete, still_authorizes_imap_delete, AttendedImapDeleteAuthorizer, ImapDeleteContext,
    ImapDeletePhase, ImapEntitlement, ImapMailbox, ImapMessageSnapshot, ImapPolicyError,
    SeededLocalImapFixture,
    delete_prepared, prepare_delete, still_authorizes_imap_delete, ImapDeleteContext,
    ImapDeleteGrant, ImapDeletePhase, ImapEntitlement, ImapGrantAuthority, ImapMailbox,
    ImapMessageSnapshot, ImapPolicyError, SeededLocalImapFixture,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

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

#[test]
fn task_0417_valid_delete_grant_reused_against_different_message_fails_and_both_records_remain() {
    let owner = "owner-task-0417";
    let account = "account-task-0417";
    let mailbox_name = "INBOX";
    let first_id = "<task-0417-first@local.test>";
    let second_id = "<task-0417-second@local.test>";
    let first = message(owner, account, mailbox_name, first_id, 41, true);
    let second = message(owner, account, mailbox_name, second_id, 42, true);
    let mut mailbox = ImapMailbox::from_messages(vec![first.clone(), second.clone()]);

    let prepared_first = prepare_delete(&mailbox, owner, account, mailbox_name, first_id)
        .expect("first same-conversation message prepares a valid delete grant source");
    let reviewed = authorize_attended_imap_batch_reviewed(
        std::slice::from_ref(&prepared_first),
        owner,
        account,
    )
    .expect("reviewed first message can mint a valid attended grant");
    let mut authorizer = AttendedImapDeleteAuthorizer::default();
    let mut grant = authorize_attended_imap_batch(&mut authorizer, reviewed, 1_000, 5_000).unwrap();
    grant.phase = ImapDeletePhase::Executing;
    grant
        .message_fingerprints
        .insert(prepared_first.fingerprint);

    let mut changed_message_request = prepared_first.clone();
    changed_message_request.message_id = second.message_id.clone();

    let context = ImapDeleteContext {
        entitlement: ImapEntitlement::Pro,
        phase: ImapDeletePhase::Executing,
        now_unix_ms: 1_100,
    };
    let authorization = still_authorizes_imap_delete(context, &grant, &changed_message_request);
    let mut attempted_deletes = 0usize;
    if authorization.is_ok() {
        attempted_deletes += 1;
        let _ = delete_prepared(&mut mailbox, &changed_message_request);
    }

    assert_eq!(authorization, Err(ImapPolicyError::FingerprintMismatch));
    assert_eq!(
        attempted_deletes, 0,
        "a changed-message request must fail before native delete is attempted"
    );
    let first_present = mailbox
        .search_message(account, mailbox_name, first_id)
        .is_some();
    let second_present = mailbox
        .search_message(account, mailbox_name, second_id)
        .is_some();
    let record_count = usize::from(first_present) + usize::from(second_present);
    assert_eq!(record_count, 2);
    assert_eq!(mailbox.deleted_count(), 0);
    println!(
        "TASK0417 authorization_error={:?} attempted_deletes={} same_conversation={} record_count={} first_present={} second_present={} deleted_count={}",
        authorization.unwrap_err(),
        attempted_deletes,
        first.mailbox == second.mailbox && first.account_id == second.account_id,
        record_count,
        first_present,
        second_present,
        mailbox.deleted_count()
    );
}

#[test]
fn task_0416_changed_delete_grant_owner_fails_and_stored_record_remains() {
    let owner = "owner-local-0416";
    let account = "account-local-0416";
    let mailbox_name = "INBOX";
    let message_id = "<task-0416@local.test>";
    let original = message(owner, account, mailbox_name, message_id, 41, true);
    let mailbox = ImapMailbox::from_messages(vec![original.clone()]);
    let prepared = prepare_delete(&mailbox, owner, account, mailbox_name, message_id)
        .expect("valid self-authored grant prepares before owner tamper");
    let mut grant_fingerprints = BTreeSet::new();
    grant_fingerprints.insert(prepared.fingerprint);
    let mut grant = ImapDeleteGrant {
        grant_id: "task-0416-grant".to_string(),
        authority: ImapGrantAuthority::Attended,
        owner_osl_user_id: prepared.owner_osl_user_id.clone(),
        account_id: prepared.account_id.clone(),
        batch_digest: prepared.batch_digest,
        message_fingerprints: grant_fingerprints,
        phase: ImapDeletePhase::Executing,
        entitlement: ImapEntitlement::Pro,
        deadline_unix_ms: 2_000,
        used: false,
        revoked: false,
    };
    let context = ImapDeleteContext {
        entitlement: ImapEntitlement::Pro,
        phase: ImapDeletePhase::Executing,
        now_unix_ms: 1_000,
    };

    assert_eq!(
        still_authorizes_imap_delete(context, &grant, &prepared),
        Ok(()),
        "precondition: the grant is valid before changing only its owner"
    );

    grant.owner_osl_user_id = "owner-local-0416-tampered".to_string();
    let result = still_authorizes_imap_delete(context, &grant, &prepared);

    assert_eq!(result, Err(ImapPolicyError::AccountBindingMismatch));
    assert_eq!(mailbox.deleted_count(), 0);
    assert_eq!(
        mailbox.search_message(account, mailbox_name, message_id),
        Some(&original)
    );
    println!("TASK0416_VALID_GRANT_BEFORE_OWNER_CHANGE=Ok(())");
    println!("TASK0416_CHANGED_FIELD=owner_osl_user_id");
    println!("TASK0416_REQUEST_RESULT=Err(AccountBindingMismatch)");
    println!("TASK0416_ORIGINAL_DELETED_COUNT={}", mailbox.deleted_count());
    println!(
        "TASK0416_ORIGINAL_STORED_RECORD_REMAINS={}",
        mailbox.search_message(account, mailbox_name, message_id) == Some(&original)
    );
}
