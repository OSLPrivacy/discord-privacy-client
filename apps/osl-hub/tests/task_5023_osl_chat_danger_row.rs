//! TASK 5023 - the OSL Chats chat-settings DANGER block.
//!
//! Finish line:
//!   * clear history on a 20-message fixture leaves 0 local messages while the
//!     other device still holds 20,
//!   * block makes 0 of 3 later fixture messages appear, with each refusal
//!     receipted,
//!   * burn without the confirm step changes 0 messages, and with it leaves 0
//!     on this side,
//!   * all 3 receipts name every item touched and every item refused.

#![cfg(feature = "core")]

use std::collections::BTreeSet;

use osl_privacy_hub::osl_chat_danger_row::{
    BlockPersonRequest, BurnThisChatRequest, ClearChatHistoryRequest, DangerRowReceipt,
    OslChatDangerRowState, BLOCKED_MESSAGE_REFUSAL_REASON, BURN_NEEDS_CONFIRMING_REASON,
    NEEDS_CONFIRMING, OTHER_COPY_REFUSAL_REASON,
};
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const CHAT_ID: &str = "task-5023-chat";
const THEM: &str = "task-5023-them";
const YOU: &str = "task-5023-you";
const FIXTURE_SIZE: usize = 20;
const LATER_MESSAGES: usize = 3;

fn open_store(dir: &TempDir, secret: u8) -> MessageStore {
    MessageStore::open(dir.path(), &[secret; 32]).expect("task 5023 store opens")
}

fn fixture_message(index: usize) -> StoredMessage {
    let sender = if index % 2 == 0 { THEM } else { YOU };
    StoredMessage {
        discord_message_id: format!("task-5023-msg-{index:02}"),
        channel_id: CHAT_ID.to_owned(),
        sender_discord_id: sender.to_owned(),
        sender_osl_user_id: sender.to_owned(),
        plaintext: format!("TASK5023 fixture message {index:02}"),
        decrypted_at: 1_800_000_000 + index as i64,
        reply_parent_id: None,
        edit_revision: 1,
        burned: false,
    }
}

fn later_message(index: usize) -> StoredMessage {
    StoredMessage {
        discord_message_id: format!("task-5023-later-{index}"),
        channel_id: CHAT_ID.to_owned(),
        sender_discord_id: THEM.to_owned(),
        sender_osl_user_id: THEM.to_owned(),
        plaintext: format!("TASK5023 later message {index} from a blocked person"),
        decrypted_at: 1_900_000_000 + index as i64,
        reply_parent_id: None,
        edit_revision: 1,
        burned: false,
    }
}

fn seed_twenty(store: &MessageStore) -> Vec<String> {
    let mut ids = Vec::with_capacity(FIXTURE_SIZE);
    for index in 0..FIXTURE_SIZE {
        let message = fixture_message(index);
        store.put(&message).expect("seed fixture message");
        ids.push(message.discord_message_id);
    }
    ids.sort();
    ids
}

fn live_count(store: &MessageStore) -> usize {
    store
        .count_live_by_channel(CHAT_ID, None)
        .expect("count local messages")
}

fn live_ids(store: &MessageStore) -> BTreeSet<String> {
    store
        .list_by_channel(CHAT_ID, 1_000)
        .expect("list local messages")
        .into_iter()
        .map(|msg| msg.discord_message_id)
        .collect()
}

/// Every item named in the receipt has to carry a non-empty id and a non-empty
/// reason, and the union of touched + refused has to cover every fixture id.
fn assert_receipt_names_everything(receipt: &DangerRowReceipt, expected: &[String], tag: &str) {
    for item in receipt.touched.iter().chain(receipt.refused.iter()) {
        assert!(!item.item_id.trim().is_empty(), "{tag}: unnamed receipt item");
        assert!(
            !item.detail.trim().is_empty(),
            "{tag}: receipt item {} has no reason",
            item.item_id
        );
    }
    assert!(
        receipt.names_every(expected),
        "{tag}: receipt does not name every fixture item"
    );
    let named: BTreeSet<String> = receipt
        .touched_message_ids()
        .union(&receipt.refused_message_ids())
        .cloned()
        .collect();
    let wanted: BTreeSet<String> = expected.iter().cloned().collect();
    assert_eq!(named, wanted, "{tag}: named ids differ from the fixture ids");
    println!(
        "TASK5023 {tag} receipt_touched={} receipt_refused={} receipt_lines={}",
        receipt.touched.len(),
        receipt.refused.len(),
        receipt.receipt_lines().len()
    );
}

#[test]
fn task_5023_each_danger_row_says_what_it_does() {
    use osl_privacy_hub::osl_chat_danger_row::DangerRowAction;
    println!(
        "TASK5023 says clear={:?}",
        DangerRowAction::ClearHistoryThisDevice.says()
    );
    println!("TASK5023 says block={:?}", DangerRowAction::Block.says());
    println!("TASK5023 says burn={:?}", DangerRowAction::BurnThisChat.says());
    assert_eq!(
        DangerRowAction::ClearHistoryThisDevice.label(),
        "Clear history (this device)"
    );
    assert_eq!(DangerRowAction::Block.label(), "Block");
    assert_eq!(DangerRowAction::BurnThisChat.label(), "Burn this chat");
    assert!(DangerRowAction::ClearHistoryThisDevice
        .says()
        .contains("this device only"));
    assert!(DangerRowAction::Block.says().contains("can still see"));
    assert!(DangerRowAction::BurnThisChat
        .says()
        .contains("until you confirm"));
}

#[test]
fn task_5023_a_blank_chat_id_is_refused_by_name() {
    let state = OslChatDangerRowState::new();
    let error = state
        .block_person(BlockPersonRequest {
            chat_id: "  ".to_owned(),
            person_id: "someone".to_owned(),
        })
        .unwrap_err();
    println!("TASK5023 blank_chat_id_error={error}");
    assert_eq!(error, "The chat id is missing");
}

#[test]
fn task_5023_clear_history_is_local_only() {
    let this_dir = TempDir::new().unwrap();
    let other_dir = TempDir::new().unwrap();
    let this_device = open_store(&this_dir, 0x50);
    let other_device = open_store(&other_dir, 0x51);

    let ids = seed_twenty(&this_device);
    let other_ids = seed_twenty(&other_device);
    assert_eq!(ids, other_ids);

    let before_here = live_count(&this_device);
    let before_there = live_count(&other_device);
    println!("TASK5023 clear_history local_before={before_here} other_device_before={before_there}");
    assert_eq!(before_here, FIXTURE_SIZE);
    assert_eq!(before_there, FIXTURE_SIZE);

    let state = OslChatDangerRowState::new();
    let receipt = state
        .clear_chat_history_this_device(
            &this_device,
            ClearChatHistoryRequest {
                chat_id: CHAT_ID.to_owned(),
                other_copy_holders: vec!["other-device".to_owned()],
            },
        )
        .expect("clear history runs");

    let after_here = live_count(&this_device);
    let after_there = live_count(&other_device);
    println!(
        "TASK5023 clear_history local_after={after_here} other_device_after={after_there} says={:?}",
        receipt.says
    );

    assert_eq!(after_here, 0, "clear history left local messages behind");
    assert_eq!(
        after_there, FIXTURE_SIZE,
        "clear history reached the other device"
    );
    assert_eq!(live_ids(&other_device).len(), FIXTURE_SIZE);

    // It says it is local-only.
    assert!(receipt.says.contains("this device only"));
    assert!(receipt.carried_out);
    assert_eq!(receipt.local_count_before, FIXTURE_SIZE);
    assert_eq!(receipt.local_count_after, 0);

    // Touched: all 20 local copies. Refused: all 20 copies on the other device.
    assert_eq!(receipt.touched.len(), FIXTURE_SIZE);
    assert_eq!(receipt.refused.len(), FIXTURE_SIZE);
    assert_eq!(receipt.touched_message_ids(), ids.iter().cloned().collect());
    assert_eq!(receipt.refused_message_ids(), ids.iter().cloned().collect());
    for item in &receipt.refused {
        assert_eq!(item.holder.as_deref(), Some("other-device"));
        assert_eq!(item.detail, OTHER_COPY_REFUSAL_REASON);
    }
    assert_receipt_names_everything(&receipt, &ids, "clear_history");

    println!(
        "TASK5023 clear_history touched_first={} refused_first={}",
        receipt.touched_ids()[0],
        receipt.refused_ids()[0]
    );
}

#[test]
fn task_5023_block_stops_their_later_messages() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir, 0x52);
    seed_twenty(&store);

    let state = OslChatDangerRowState::new();
    let block_receipt = state
        .block_person(BlockPersonRequest {
            chat_id: CHAT_ID.to_owned(),
            person_id: THEM.to_owned(),
        })
        .expect("block runs");

    assert!(block_receipt.carried_out);
    assert_eq!(block_receipt.touched.len(), 1);
    assert_eq!(block_receipt.touched[0].item_id, THEM);
    // It says what they can still see.
    assert_eq!(block_receipt.still_visible_to_them.len(), 3);
    assert!(block_receipt.says.contains("can still see"));
    println!(
        "TASK5023 block still_visible={:?}",
        block_receipt.still_visible_to_them
    );

    let before = live_count(&store);
    let mut later_ids = Vec::with_capacity(LATER_MESSAGES);
    let mut appeared = 0usize;
    let mut receipted_refusals = 0usize;
    for index in 0..LATER_MESSAGES {
        let message = later_message(index);
        later_ids.push(message.discord_message_id.clone());
        let outcome = state
            .deliver_incoming_message(&store, &message)
            .expect("inbound gate runs");
        if outcome.stored {
            appeared += 1;
        }
        match &outcome.refusal_receipt {
            Some(receipt) => {
                assert_eq!(receipt.refused.len(), 1);
                assert_eq!(receipt.refused[0].item_id, message.discord_message_id);
                assert_eq!(receipt.refused[0].detail, BLOCKED_MESSAGE_REFUSAL_REASON);
                receipted_refusals += 1;
                println!(
                    "TASK5023 block refusal_receipt message={} reason={}",
                    receipt.refused[0].item_id, receipt.refused[0].detail
                );
            }
            None => panic!("a blocked message produced no refusal receipt"),
        }
        assert!(
            !live_ids(&store).contains(&message.discord_message_id),
            "a blocked message reached the local store"
        );
    }
    let after = live_count(&store);
    println!(
        "TASK5023 block later_messages={LATER_MESSAGES} appeared={appeared} \
receipted_refusals={receipted_refusals} local_before={before} local_after={after}"
    );
    assert_eq!(appeared, 0, "a blocked message appeared");
    assert_eq!(receipted_refusals, LATER_MESSAGES);
    assert_eq!(before, after);

    // An unblocked sender still gets through, so the gate is a block and not a
    // wall.
    let mut allowed = later_message(9);
    allowed.sender_discord_id = YOU.to_owned();
    allowed.sender_osl_user_id = YOU.to_owned();
    let allowed_outcome = state
        .deliver_incoming_message(&store, &allowed)
        .expect("inbound gate runs for an unblocked sender");
    println!(
        "TASK5023 block unblocked_sender_stored={}",
        allowed_outcome.stored
    );
    assert!(allowed_outcome.stored);

    // The standing Block receipt names the person it touched and all 3
    // messages it refused.
    let standing = state
        .block_receipt(CHAT_ID)
        .expect("standing block receipt reads")
        .expect("a block exists for this chat");
    assert_eq!(standing.touched.len(), 1);
    assert_eq!(standing.refused.len(), LATER_MESSAGES);
    let mut expected = later_ids.clone();
    expected.sort();
    assert_receipt_names_everything(&standing, &expected, "block");
    println!(
        "TASK5023 block standing_touched={:?} standing_refused={:?}",
        standing.touched_ids(),
        standing.refused_ids()
    );
}

#[test]
fn task_5023_burn_this_chat_waits_for_the_ask_step() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir, 0x53);
    let ids = seed_twenty(&store);

    let state = OslChatDangerRowState::new();
    let before = live_count(&store);
    let before_ids = live_ids(&store);
    assert_eq!(before, FIXTURE_SIZE);

    // Ask step on, not confirmed: the burn changes nothing.
    let unconfirmed = state
        .burn_this_chat(
            &store,
            BurnThisChatRequest {
                chat_id: CHAT_ID.to_owned(),
                ask_step_on: true,
                confirmed: false,
                other_copy_holders: vec!["their-side".to_owned()],
            },
        )
        .expect("unconfirmed burn returns an answer");

    let after_unconfirmed = live_count(&store);
    let after_unconfirmed_ids = live_ids(&store);
    let changed = FIXTURE_SIZE - after_unconfirmed_ids.intersection(&before_ids).count()
        + after_unconfirmed_ids.difference(&before_ids).count();
    println!(
        "TASK5023 burn unconfirmed local_before={before} local_after={after_unconfirmed} \
messages_changed={changed} needs_confirming={:?} carried_out={}",
        unconfirmed.needs_confirming, unconfirmed.carried_out
    );

    assert_eq!(changed, 0, "the unconfirmed burn changed messages");
    assert_eq!(after_unconfirmed, FIXTURE_SIZE);
    assert!(!unconfirmed.carried_out);
    assert_eq!(unconfirmed.needs_confirming.as_deref(), Some(NEEDS_CONFIRMING));
    assert!(unconfirmed.touched.is_empty());
    // 20 local messages refused for needing confirmation, plus 20 their-side
    // copies this row cannot reach.
    assert_eq!(unconfirmed.refused.len(), FIXTURE_SIZE * 2);
    assert_eq!(
        unconfirmed
            .refused
            .iter()
            .filter(|item| item.detail == BURN_NEEDS_CONFIRMING_REASON)
            .count(),
        FIXTURE_SIZE
    );
    assert_receipt_names_everything(&unconfirmed, &ids, "burn_unconfirmed");

    // Confirmed: 0 left on this side.
    let confirmed = state
        .burn_this_chat(
            &store,
            BurnThisChatRequest {
                chat_id: CHAT_ID.to_owned(),
                ask_step_on: true,
                confirmed: true,
                other_copy_holders: vec!["their-side".to_owned()],
            },
        )
        .expect("confirmed burn runs");

    let after_confirmed = live_count(&store);
    println!(
        "TASK5023 burn confirmed local_after={after_confirmed} carried_out={} touched={} refused={}",
        confirmed.carried_out,
        confirmed.touched.len(),
        confirmed.refused.len()
    );

    assert_eq!(after_confirmed, 0, "the confirmed burn left messages behind");
    assert!(confirmed.carried_out);
    assert!(confirmed.needs_confirming.is_none());
    assert_eq!(confirmed.touched.len(), FIXTURE_SIZE);
    assert_eq!(confirmed.refused.len(), FIXTURE_SIZE);
    assert_eq!(confirmed.touched_message_ids(), ids.iter().cloned().collect());
    for item in &confirmed.refused {
        assert_eq!(item.holder.as_deref(), Some("their-side"));
        assert_eq!(item.detail, OTHER_COPY_REFUSAL_REASON);
    }
    assert_receipt_names_everything(&confirmed, &ids, "burn_confirmed");

    // The burned bodies are gone, not just hidden.
    for id in &ids {
        assert!(
            store.get(id).expect("read a burned message").is_none(),
            "burned message {id} is still readable"
        );
    }
}

#[test]
fn task_5023_burn_with_the_ask_step_off_runs_straight_away() {
    let dir = TempDir::new().unwrap();
    let store = open_store(&dir, 0x54);
    let ids = seed_twenty(&store);

    let state = OslChatDangerRowState::new();
    let receipt = state
        .burn_this_chat(
            &store,
            BurnThisChatRequest {
                chat_id: CHAT_ID.to_owned(),
                ask_step_on: false,
                confirmed: false,
                other_copy_holders: vec!["their-side".to_owned()],
            },
        )
        .expect("burn with the ask step off runs");

    let after = live_count(&store);
    println!("TASK5023 burn ask_step_off local_after={after} touched={}", receipt.touched.len());
    assert_eq!(after, 0);
    assert!(receipt.carried_out);
    assert_eq!(receipt.touched.len(), FIXTURE_SIZE);
    assert_receipt_names_everything(&receipt, &ids, "burn_ask_step_off");
}
