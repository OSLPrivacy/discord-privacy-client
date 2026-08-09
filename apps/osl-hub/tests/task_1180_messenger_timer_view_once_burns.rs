use osl_privacy_hub::messenger_message_controls::{
    cmd_osl_messenger_burn_both_sides, cmd_osl_messenger_burn_their_side,
    cmd_osl_messenger_burn_your_side, cmd_osl_messenger_timer_expiry,
    cmd_osl_messenger_view_once_expiry, MessengerBurnRequest, MessengerProtectedMessageRecord,
};

const OWNER: &str = "osl-owner-1180";
const ACCOUNT: &str = "messenger-account-1180";
const CONVERSATION: &str = "messenger-conversation-1180";
const NOW: u64 = 1_900_001_180;

fn record(
    owner: &str,
    account: &str,
    conversation: &str,
    message_id: &str,
    authored_by_self: bool,
    created_at_unix_ms: i64,
) -> MessengerProtectedMessageRecord {
    MessengerProtectedMessageRecord {
        owner_osl_user_id: owner.to_owned(),
        messenger_account_id: account.to_owned(),
        conversation_id: conversation.to_owned(),
        messenger_message_id: message_id.to_owned(),
        authored_by_self,
        created_at_unix_ms,
    }
}

fn fixture() -> Vec<MessengerProtectedMessageRecord> {
    vec![
        record(
            OWNER,
            ACCOUNT,
            CONVERSATION,
            "messenger-own-1180-a",
            true,
            20,
        ),
        record(
            OWNER,
            ACCOUNT,
            CONVERSATION,
            "messenger-other-1180",
            false,
            25,
        ),
        record(
            OWNER,
            ACCOUNT,
            CONVERSATION,
            "messenger-own-1180-b",
            true,
            30,
        ),
        // Each of these looks tempting under one weak ownership predicate but
        // must remain outside all three returned target lists.
        record(
            "another-owner-1180",
            ACCOUNT,
            CONVERSATION,
            "wrong-owner-1180",
            true,
            1,
        ),
        record(
            OWNER,
            "another-account-1180",
            CONVERSATION,
            "wrong-account-1180",
            true,
            2,
        ),
        record(
            OWNER,
            ACCOUNT,
            "another-conversation-1180",
            "wrong-chat-1180",
            true,
            3,
        ),
    ]
}

fn all_owned_request() -> MessengerBurnRequest<'static> {
    MessengerBurnRequest {
        owner_osl_user_id: OWNER,
        messenger_account_id: ACCOUNT,
        conversation_id: CONVERSATION,
        requested_message_ids: &[],
    }
}

#[test]
fn task_1180_commands_return_exact_expiry_and_only_owned_targets() {
    let timer = cmd_osl_messenger_timer_expiry(NOW, 30 * 24 * 60 * 60)
        .expect("thirty-day Messenger timer is allowed");
    let view_once = cmd_osl_messenger_view_once_expiry(NOW, 60)
        .expect("sixty-second Messenger view once is allowed");
    assert_eq!(timer.mode, "timer");
    assert_eq!(timer.lifetime_seconds, 2_592_000);
    assert_eq!(timer.expires_at_unix_seconds, 1_902_593_180);
    assert_eq!(view_once.mode, "view-once");
    assert_eq!(view_once.lifetime_seconds, 60);
    assert_eq!(view_once.expires_at_unix_seconds, 1_900_001_240);

    let records = fixture();
    let your = cmd_osl_messenger_burn_your_side(&records, all_owned_request())
        .expect("select your-side targets");
    let their = cmd_osl_messenger_burn_their_side(&records, all_owned_request())
        .expect("select their-side targets");
    let both = cmd_osl_messenger_burn_both_sides(&records, all_owned_request())
        .expect("select both-sides targets");
    let expected = vec![
        "messenger-own-1180-a".to_owned(),
        "messenger-own-1180-b".to_owned(),
    ];

    for result in [&your, &their, &both] {
        assert_eq!(result.owned_target_ids, expected);
        assert!(!result.owned_target_ids.iter().any(|id| {
            matches!(
                id.as_str(),
                "messenger-other-1180"
                    | "wrong-owner-1180"
                    | "wrong-account-1180"
                    | "wrong-chat-1180"
            )
        }));
    }
    assert_eq!(your.local_target_ids, expected);
    assert_eq!(your.choice, "your-side");
    assert!(your.remote_target_ids.is_empty());
    assert_eq!(their.choice, "their-side");
    assert!(their.local_target_ids.is_empty());
    assert_eq!(their.remote_target_ids, expected);
    assert_eq!(both.choice, "both-sides");
    assert_eq!(both.local_target_ids, expected);
    assert_eq!(both.remote_target_ids, expected);

    // Prove the commands derive their answers from the prepared records: a
    // renamed owned fixture changes every returned list.
    let mut renamed = records.clone();
    renamed[0].messenger_message_id = "messenger-own-1180-renamed".to_owned();
    let renamed_both = cmd_osl_messenger_burn_both_sides(&renamed, all_owned_request())
        .expect("select renamed fixture targets");
    assert_eq!(
        renamed_both.owned_target_ids,
        vec![
            "messenger-own-1180-renamed".to_owned(),
            "messenger-own-1180-b".to_owned(),
        ]
    );

    // Exact-id review cannot be used to smuggle somebody else's row into a
    // burn. This refusal is shared by all three side commands.
    let other_person = MessengerBurnRequest {
        requested_message_ids: &["messenger-other-1180"],
        ..all_owned_request()
    };
    for refusal in [
        cmd_osl_messenger_burn_your_side(&records, other_person),
        cmd_osl_messenger_burn_their_side(&records, other_person),
        cmd_osl_messenger_burn_both_sides(&records, other_person),
    ] {
        assert_eq!(
            refusal.unwrap_err(),
            "OSL: Messenger request refused: another-person-delete"
        );
    }

    println!("TASK1180_TIMER_EXPIRY={}", timer.expires_at_unix_seconds);
    println!(
        "TASK1180_VIEW_ONCE_EXPIRY={}",
        view_once.expires_at_unix_seconds
    );
    println!(
        "TASK1180_YOUR_SIDE_OWNED_TARGETS={:?} local={:?} remote={:?}",
        your.owned_target_ids, your.local_target_ids, your.remote_target_ids
    );
    println!(
        "TASK1180_THEIR_SIDE_OWNED_TARGETS={:?} local={:?} remote={:?}",
        their.owned_target_ids, their.local_target_ids, their.remote_target_ids
    );
    println!(
        "TASK1180_BOTH_SIDES_OWNED_TARGETS={:?} local={:?} remote={:?}",
        both.owned_target_ids, both.local_target_ids, both.remote_target_ids
    );
    println!(
        "TASK1180_RENAMED_OWNED_TARGETS={:?}",
        renamed_both.owned_target_ids
    );
    println!("TASK1180_UNOWNED_TARGETS=0");
    println!("TASK1180_ANOTHER_PERSON_DELETE_REFUSALS=3");
}
