#![cfg(feature = "core")]

//! TASK 3514: both live arrival adapters feed one protected-eye record.  The
//! email fixture represents a provider-owned Gmail inbox read through 3042;
//! it is intentionally not a seeded overlay row and this test never calls the
//! overlay record method directly.

use osl_privacy_hub::osl_chat_delivery::route_osl_chat_protected_text_arrival;
use osl_privacy_hub::services::{
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot,
};
use osl_privacy_hub::shipping_email_receive::route_provider_email_arrival;
use osl_privacy_hub::shipping_receive::{
    ShippingProtectedOverlay, ShippingReceiveJournal, ShippingService,
};

const EMAIL_UID: &str = "gmail-provider-uid-3514-0001";
const CHAT_MESSAGE_ID: &str = "osl-message-3514-0001";
const EMAIL_COVER: &str = "Coffee after the train?";
const EMAIL_PRIVATE: &str = "Exact private email words: meet at platform seven.";
const CHAT_COVER: &str = "The blue notebook is on my desk.";
const CHAT_PRIVATE: &str = "Exact private OSL Chats words: use the west entrance.";

/// A live provider read is represented separately from the shipping overlay:
/// only its IMAP-like mailbox snapshot can make the email arrival route run.
struct RealProviderMailbox {
    network: Option<MailboxReaderSnapshot>,
}

impl RealProviderMailbox {
    fn gmail_inbox_with_fresh_delivery() -> Self {
        Self {
            network: Some(MailboxReaderSnapshot::new_for_signed_in_address(
                "owner@gmail.com",
                [MailboxFolderCandidate::new("INBOX", "Inbox")],
                [MailboxMessageCandidate::new(
                    "INBOX",
                    EMAIL_UID,
                    EMAIL_COVER,
                    1_786_700_001,
                    "friend@gmail.com",
                    EMAIL_PRIVATE,
                )],
            )),
        }
    }
}

struct LiveOslChatsSessions {
    receive_connection_live: bool,
}

impl LiveOslChatsSessions {
    fn deliver_from_alice_to_bob(
        &self,
        journal: &mut ShippingReceiveJournal,
        overlay: &mut ShippingProtectedOverlay,
    ) -> Result<(), String> {
        route_osl_chat_protected_text_arrival(
            self.receive_connection_live,
            CHAT_MESSAGE_ID,
            CHAT_COVER,
            CHAT_PRIVATE,
            journal,
            overlay,
        )
    }
}

#[test]
fn task_3514_fresh_provider_email_and_live_osl_chats_each_move_the_eye_from_zero_to_one() {
    let provider = RealProviderMailbox::gmail_inbox_with_fresh_delivery();
    let mut email_overlay = ShippingProtectedOverlay::default();
    let mut email_journal = ShippingReceiveJournal::default();
    assert_eq!(email_overlay.matching_rows().len(), 0);
    route_provider_email_arrival(
        "osl-owner-3514",
        "gmail",
        "gmail-account-3514",
        "INBOX",
        EMAIL_UID,
        provider.network.as_ref(),
        &mut email_journal,
        &mut email_overlay,
    )
    .expect("fresh protected email is read from the provider mailbox");
    assert_eq!(email_overlay.matching_rows().len(), 1);
    let email_id = format!("gmail:gmail-account-3514:{EMAIL_UID}");
    assert_eq!(email_overlay.closed_cover(&email_id).unwrap(), EMAIL_COVER);
    assert_eq!(
        email_overlay.open_private_words(&email_id).unwrap(),
        EMAIL_PRIVATE
    );
    assert_eq!(email_journal.services(), &[ShippingService::Email]);

    let sessions = LiveOslChatsSessions {
        receive_connection_live: true,
    };
    let mut chat_overlay = ShippingProtectedOverlay::default();
    let mut chat_journal = ShippingReceiveJournal::default();
    assert_eq!(chat_overlay.matching_rows().len(), 0);
    sessions
        .deliver_from_alice_to_bob(&mut chat_journal, &mut chat_overlay)
        .expect("fresh protected OSL Chats delivery reaches Bob's live session");
    assert_eq!(chat_overlay.matching_rows().len(), 1);
    assert_eq!(
        chat_overlay.closed_cover(CHAT_MESSAGE_ID).unwrap(),
        CHAT_COVER
    );
    assert_eq!(
        chat_overlay.open_private_words(CHAT_MESSAGE_ID).unwrap(),
        CHAT_PRIVATE
    );
    assert_eq!(chat_journal.services(), &[ShippingService::OslChats]);

    println!("TASK3514_EMAIL_ROWS before=0 after=1 provider_uid={EMAIL_UID}");
    println!("TASK3514_EMAIL_CLOSED_COVER={EMAIL_COVER}");
    println!("TASK3514_EMAIL_OPEN_PRIVATE={EMAIL_PRIVATE}");
    println!("TASK3514_CHAT_ROWS before=0 after=1 message_id={CHAT_MESSAGE_ID}");
    println!("TASK3514_CHAT_CLOSED_COVER={CHAT_COVER}");
    println!("TASK3514_CHAT_OPEN_PRIVATE={CHAT_PRIVATE}");
    println!("TASK3514_SHARED_RECORD_FIELDS=app_id,app_row_id,cover_text,private_words,opened");
}

#[test]
fn task_3514_starved_mailbox_and_removed_osl_chat_connection_fail_closed() {
    let mut email_overlay = ShippingProtectedOverlay::default();
    let mut email_journal = ShippingReceiveJournal::default();
    let email_error = route_provider_email_arrival(
        "osl-owner-3514",
        "gmail",
        "gmail-account-3514",
        "INBOX",
        EMAIL_UID,
        None,
        &mut email_journal,
        &mut email_overlay,
    )
    .expect_err("starved provider mailbox must not synthesize a protected row");
    assert!(email_error.contains("network"));
    assert!(email_overlay.matching_rows().is_empty());

    let sessions = LiveOslChatsSessions {
        receive_connection_live: false,
    };
    let mut chat_overlay = ShippingProtectedOverlay::default();
    let mut chat_journal = ShippingReceiveJournal::default();
    let chat_error = sessions
        .deliver_from_alice_to_bob(&mut chat_journal, &mut chat_overlay)
        .expect_err("removed OSL Chats receive connection must not synthesize a protected row");
    assert!(chat_error.contains("connection"));
    assert!(chat_overlay.matching_rows().is_empty());
    println!("TASK3514_STARVED_MAILBOX_CHECKER_EXIT=1 error={email_error}");
    println!("TASK3514_REMOVED_CHAT_CONNECTION_CHECKER_EXIT=1 error={chat_error}");
}
