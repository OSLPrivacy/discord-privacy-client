//! TASK 1337 - check dropped folders and auto-send fail.
//!
//! Drops `maple.txt` (fingerprint `MAPLE-4172`) into the OSL Chats attachment
//! tray, then changes *only* that dropped item's kind from file to folder --
//! same path, same name -- and drops it again without pressing Send. The folder
//! must be refused because folders are not allowed, and the refusal must leave
//! the already-staged card exactly as it was.

use osl_privacy_hub::osl_chat_drag_drop::{
    tray_fingerprint, OslChatAttachmentTray, OSL_CHAT_DROP_FOLDER_REFUSAL,
};

/// Fixture body for `maple.txt`. The intake folds the file's own bytes into the
/// four digits of the fingerprint; this body is the one that lands on `4172`,
/// which is what makes the dropped file read as `MAPLE-4172`.
const MAPLE_BODY: &[u8] = b"maple leaf 2176\n";
const MAPLE_FINGERPRINT: &str = "MAPLE-4172";

fn kind_of(path: &std::path::Path) -> &'static str {
    let metadata = std::fs::metadata(path).expect("dropped item metadata");
    if metadata.is_dir() {
        "folder"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
    }
}

#[test]
fn task_1337_dropped_folder_is_refused_and_nothing_auto_sends() {
    let temp = tempfile::tempdir().expect("tempdir");
    let maple = temp.path().join("maple.txt");
    std::fs::write(&maple, MAPLE_BODY).expect("write maple.txt fixture");

    let mut tray = OslChatAttachmentTray::default();
    println!("TASK1337 tray_count_before={}", tray.attachments().len());
    println!(
        "TASK1337 sent_message_count_before={}",
        tray.messages_created()
    );
    assert_eq!(tray.attachments().len(), 0);
    assert_eq!(tray.messages_created(), 0);

    // Drop the file. Nothing here is a Send: the tray API has no send path at
    // all, so `messages_created` can only stay where the send path left it.
    println!("TASK1337 dropped_item_kind_first_drop={}", kind_of(&maple));
    let receipt = tray
        .accept_dropped_files([maple.as_path()])
        .expect("maple.txt is accepted");

    println!(
        "TASK1337 accepted_file_count={} tray_count_after_file={} sent_message_count_after_file={}",
        receipt.accepted_file_count, receipt.tray_file_count, receipt.messages_created
    );
    println!(
        "TASK1337 dropped_filename={} dropped_fingerprint={}",
        receipt.accepted_filenames.join(","),
        receipt.accepted_fingerprints.join(",")
    );
    assert_eq!(receipt.accepted_file_count, 1);
    assert_eq!(receipt.tray_file_count, 1);
    assert_eq!(tray.attachments().len(), 1);
    assert_eq!(receipt.accepted_filenames, vec!["maple.txt".to_owned()]);
    assert_eq!(
        receipt.accepted_fingerprints,
        vec![MAPLE_FINGERPRINT.to_owned()]
    );
    assert_eq!(tray.attachments()[0].original_filename, "maple.txt");
    assert_eq!(tray.attachments()[0].fingerprint, MAPLE_FINGERPRINT);
    assert_eq!(receipt.messages_created, 0);
    assert_eq!(tray.messages_created(), 0);

    // Change ONLY the dropped item's kind: same directory, same name, file ->
    // folder. Send is still never pressed.
    std::fs::remove_file(&maple).expect("remove the dropped file");
    std::fs::create_dir(&maple).expect("re-create the dropped item as a folder");
    println!("TASK1337 dropped_item_kind_second_drop={}", kind_of(&maple));
    assert_eq!(kind_of(&maple), "folder");

    let refusal = tray
        .accept_dropped_files([maple.as_path()])
        .expect_err("a dropped folder is refused");
    println!("TASK1337 folder_refusal={refusal}");
    println!(
        "TASK1337 folder_refusal_says_folders_not_allowed={}",
        refusal.contains("folders are not allowed")
    );
    assert_eq!(refusal, OSL_CHAT_DROP_FOLDER_REFUSAL);
    assert!(refusal.contains("folders are not allowed"));

    // The refused drop must not have touched the staged card, the tray count, or
    // the sent-message count.
    let card = &tray.attachments()[0];
    println!(
        "TASK1337 maple_filename_after_folder={} maple_fingerprint_after_folder={}",
        card.original_filename, card.fingerprint
    );
    println!(
        "TASK1337 tray_count_after_folder={} sent_message_count_after_folder={}",
        tray.attachments().len(),
        tray.messages_created()
    );
    assert_eq!(card.original_filename, "maple.txt");
    assert_eq!(card.fingerprint, MAPLE_FINGERPRINT);
    assert_eq!(tray.attachments().len(), 1);
    assert_eq!(tray.messages_created(), 0);
}

/// The fingerprint has to be earned from the dropped bytes, otherwise "maple.txt
/// stays MAPLE-4172" would hold even if the intake never looked at the file.
#[test]
fn task_1337_fingerprint_is_derived_from_the_dropped_bytes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let maple = temp.path().join("maple.txt");
    std::fs::write(&maple, MAPLE_BODY).expect("write maple.txt fixture");
    let derived = tray_fingerprint("maple.txt", &maple).expect("fingerprint maple.txt");
    println!("TASK1337 derived_fingerprint={derived}");
    assert_eq!(derived, MAPLE_FINGERPRINT);

    let mut tampered = MAPLE_BODY.to_vec();
    tampered[0] = b'M';
    std::fs::write(&maple, &tampered).expect("rewrite maple.txt with one byte changed");
    let after_tamper = tray_fingerprint("maple.txt", &maple).expect("fingerprint tampered bytes");
    println!("TASK1337 fingerprint_after_one_byte_change={after_tamper}");
    assert_ne!(after_tamper, MAPLE_FINGERPRINT);
}

/// A folder anywhere in a drop refuses the whole drop; nothing is half-staged.
#[test]
fn task_1337_folder_in_a_mixed_drop_stages_nothing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let maple = temp.path().join("maple.txt");
    let folder = temp.path().join("maple-folder");
    std::fs::write(&maple, MAPLE_BODY).expect("write maple.txt fixture");
    std::fs::create_dir(&folder).expect("create folder fixture");

    let mut tray = OslChatAttachmentTray::default();
    let refusal = tray
        .accept_dropped_files([maple.as_path(), folder.as_path()])
        .expect_err("a drop containing a folder is refused");
    println!("TASK1337 mixed_drop_refusal={refusal}");
    println!(
        "TASK1337 mixed_drop_tray_count={} mixed_drop_sent_message_count={}",
        tray.attachments().len(),
        tray.messages_created()
    );
    assert_eq!(refusal, OSL_CHAT_DROP_FOLDER_REFUSAL);
    assert_eq!(tray.attachments().len(), 0);
    assert_eq!(tray.messages_created(), 0);
}
