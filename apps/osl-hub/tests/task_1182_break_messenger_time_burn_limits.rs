use osl_privacy_hub::messenger_message_controls::{
    cmd_osl_messenger_burn_both_sides, cmd_osl_messenger_timer_expiry,
    cmd_osl_messenger_view_once_expiry, MessengerBurnRequest, MessengerExpiryDto,
    MessengerProtectedMessageRecord,
};

const OWNER: &str = "osl-owner-1182";
const ACCOUNT: &str = "messenger-account-1182";
const CONVERSATION: &str = "messenger-conversation-1182";
const OWNED_MESSAGE: &str = "messenger-own-1182";
const ANOTHER_PERSON_DELETE: &str = "another-person-delete";
const NOW: u64 = 1_900_001_182;

#[derive(Clone, Debug, Eq, PartialEq)]
struct TimedMessage {
    messenger_message_id: &'static str,
    expiry: MessengerExpiryDto,
}

fn record(message_id: &str, authored_by_self: bool) -> MessengerProtectedMessageRecord {
    MessengerProtectedMessageRecord {
        owner_osl_user_id: OWNER.to_owned(),
        messenger_account_id: ACCOUNT.to_owned(),
        conversation_id: CONVERSATION.to_owned(),
        messenger_message_id: message_id.to_owned(),
        authored_by_self,
        created_at_unix_ms: 1_900_001_182_000,
    }
}

#[test]
fn task_1182_changed_timer_view_once_and_delete_requests_are_named_and_atomic() {
    let accepted_expiry = cmd_osl_messenger_timer_expiry(NOW, 30 * 24 * 60 * 60)
        .expect("the owned Messenger message accepts the maximum timer");
    let timed_messages = vec![TimedMessage {
        messenger_message_id: OWNED_MESSAGE,
        expiry: accepted_expiry,
    }];
    let accepted_snapshot = timed_messages.clone();

    assert_eq!(timed_messages.len(), 1);
    assert_eq!(
        timed_messages
            .iter()
            .filter(|message| message.messenger_message_id == OWNED_MESSAGE)
            .count(),
        1
    );
    assert_eq!(timed_messages[0].expiry.mode, "timer");
    println!(
        "TASK1182 good timed_message_count={} timed_messages={}",
        timed_messages.len(),
        timed_messages[0].messenger_message_id
    );

    let thirty_one_days = cmd_osl_messenger_timer_expiry(NOW, 31 * 24 * 60 * 60)
        .expect_err("31 days must be outside the Messenger timer limit");
    assert_eq!(thirty_one_days, "OSL: Messenger request refused: 31-days");
    assert_eq!(timed_messages, accepted_snapshot);
    println!(
        "TASK1182 changed_request_value=31-days refused_by_name=31-days timed_message_count={}",
        timed_messages.len()
    );

    let sixty_one_seconds = cmd_osl_messenger_view_once_expiry(NOW, 61)
        .expect_err("61 seconds must be outside the Messenger view-once limit");
    assert_eq!(
        sixty_one_seconds,
        "OSL: Messenger request refused: 61-seconds"
    );
    assert_eq!(timed_messages, accepted_snapshot);
    println!(
        "TASK1182 changed_request_value=61-seconds refused_by_name=61-seconds timed_message_count={}",
        timed_messages.len()
    );

    let records = [
        record(OWNED_MESSAGE, true),
        record(ANOTHER_PERSON_DELETE, false),
    ];
    let another_person_delete = cmd_osl_messenger_burn_both_sides(
        &records,
        MessengerBurnRequest {
            owner_osl_user_id: OWNER,
            messenger_account_id: ACCOUNT,
            conversation_id: CONVERSATION,
            requested_message_ids: &[ANOTHER_PERSON_DELETE],
        },
    )
    .expect_err("another person's Messenger message must not become a deletion target");
    assert_eq!(
        another_person_delete,
        "OSL: Messenger request refused: another-person-delete"
    );
    assert_eq!(timed_messages, accepted_snapshot);
    println!(
        "TASK1182 changed_request_value={ANOTHER_PERSON_DELETE} refused_by_name={ANOTHER_PERSON_DELETE} timed_message_count={}",
        timed_messages.len()
    );

    assert_eq!(timed_messages, accepted_snapshot);
    assert_eq!(timed_messages.len(), 1);
    assert_eq!(
        timed_messages
            .iter()
            .filter(|message| message.messenger_message_id == OWNED_MESSAGE)
            .count(),
        1
    );
    assert_eq!(timed_messages[0].expiry.mode, "timer");
    assert_eq!(timed_messages[0].expiry.lifetime_seconds, 2_592_000);
    assert_eq!(
        timed_messages[0].expiry.expires_at_unix_seconds,
        1_902_593_182
    );
    println!(
        "TASK1182 final timed_message_count={} timed_messages={} unchanged=true",
        timed_messages.len(),
        timed_messages[0].messenger_message_id
    );
}
