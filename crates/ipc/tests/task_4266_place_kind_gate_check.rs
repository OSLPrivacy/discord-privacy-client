//! TASK 4266 - check the allowed-places list takes the three kinds of place
//! (X, Instagram, Messenger) added by gates 3743, 3744, 3745, on top of the
//! Telegram/Signal/WhatsApp kinds added by gates 3740-3742.
//!
//! Before 3740-3745, each app's whitelist kind list only knew the shapes named
//! in its base task (0147 Telegram, 0150 Signal, 0153 WhatsApp, 0156 X,
//! 0159 Instagram, 0162 Messenger). Those pre-gate counts are historical facts
//! recorded in the plan text, not something still derivable from live code
//! (the gates replaced that state), so they are named constants here rather
//! than computed. Everything under AFTER is read live from the current enums.

use ipc::auto_whitelist_rules::{
    normalize_place_kind_for_app, parse_instagram_whitelist_kind, parse_messenger_whitelist_kind,
    parse_signal_whitelist_kind, parse_telegram_whitelist_kind, parse_whatsapp_whitelist_kind,
    InstagramWhitelistKind, MessengerWhitelistKind, SignalWhitelistKind, TelegramWhitelistKind,
    WhatsAppWhitelistKind, X_PLACE_KINDS,
};

// Pre-3740-3745 kind counts, taken from the base tasks that first defined
// each app's kind list (0147, 0150, 0153, 0156, 0159, 0162).
const TELEGRAM_BEFORE: usize = 4; // direct message, group chat, channel, public post
const SIGNAL_BEFORE: usize = 2; // direct message, group chat
const WHATSAPP_BEFORE: usize = 3; // direct message, group chat, channel
const X_BEFORE: usize = 2; // direct message, public post
const INSTAGRAM_BEFORE: usize = 3; // direct message, group chat, public post
const MESSENGER_BEFORE: usize = 2; // direct message, group chat

#[test]
fn task_4266_kind_count_grows_by_the_eleven_kinds_3740_to_3745_added() {
    let before = TELEGRAM_BEFORE
        + SIGNAL_BEFORE
        + WHATSAPP_BEFORE
        + X_BEFORE
        + INSTAGRAM_BEFORE
        + MESSENGER_BEFORE;

    let telegram_after = TelegramWhitelistKind::ALL.len();
    let signal_after = SignalWhitelistKind::ALL.len();
    let whatsapp_after = WhatsAppWhitelistKind::ALL.len();
    let x_after = X_PLACE_KINDS.len();
    let instagram_after = InstagramWhitelistKind::ALL.len();
    let messenger_after = MessengerWhitelistKind::ALL.len();
    let after = telegram_after
        + signal_after
        + whatsapp_after
        + x_after
        + instagram_after
        + messenger_after;

    println!(
        "TASK4266_KIND_COUNT_BEFORE total={before} telegram={TELEGRAM_BEFORE} signal={SIGNAL_BEFORE} whatsapp={WHATSAPP_BEFORE} x={X_BEFORE} instagram={INSTAGRAM_BEFORE} messenger={MESSENGER_BEFORE}"
    );
    println!(
        "TASK4266_KIND_COUNT_AFTER total={after} telegram={telegram_after} signal={signal_after} whatsapp={whatsapp_after} x={x_after} instagram={instagram_after} messenger={messenger_after}"
    );
    println!(
        "TASK4266_KIND_COUNT_DELTA added={}",
        after as i64 - before as i64
    );

    assert!(
        after > before,
        "after count {after} must be higher than before count {before}"
    );
    assert_eq!(
        after - before,
        11,
        "3740 to 3745 must add exactly 11 named kinds"
    );
}

#[test]
fn task_4266_all_eleven_named_kinds_added_by_3740_to_3745_succeed() {
    let checks: Vec<(&str, Result<(), String>)> = vec![
        (
            "telegram:supergroup",
            parse_telegram_whitelist_kind("supergroup").map(|_| ()),
        ),
        (
            "telegram:saved_messages",
            parse_telegram_whitelist_kind("saved_messages").map(|_| ()),
        ),
        (
            "signal:story",
            parse_signal_whitelist_kind("story").map(|_| ()),
        ),
        (
            "whatsapp:community",
            parse_whatsapp_whitelist_kind("community").map(|_| ()),
        ),
        (
            "whatsapp:community_group",
            parse_whatsapp_whitelist_kind("community_group").map(|_| ()),
        ),
        (
            "whatsapp:broadcast_list",
            parse_whatsapp_whitelist_kind("broadcast_list").map(|_| ()),
        ),
        (
            "x:group_direct_message",
            normalize_place_kind_for_app("x", "group_direct_message").map(|_| ()),
        ),
        (
            "x:reply",
            normalize_place_kind_for_app("x", "reply").map(|_| ()),
        ),
        (
            "instagram:comment",
            parse_instagram_whitelist_kind("comment").map(|_| ()),
        ),
        (
            "instagram:story",
            parse_instagram_whitelist_kind("story").map(|_| ()),
        ),
        (
            "messenger:community",
            parse_messenger_whitelist_kind("community").map(|_| ()),
        ),
    ];

    assert_eq!(checks.len(), 11, "must test exactly the 11 named kinds");

    let mut successes = 0usize;
    for (name, result) in &checks {
        let ok = result.is_ok();
        println!("TASK4266_NAMED_KIND name={name} success={ok}");
        if ok {
            successes += 1;
        } else {
            panic!("named kind {name} must succeed: {result:?}");
        }
    }

    println!("TASK4266_NAMED_KIND_SUCCESS_COUNT count={successes}");
    assert_eq!(successes, 11);
}

#[test]
fn task_4266_three_made_up_kinds_are_each_refused_by_name() {
    let made_up = [
        ("x", "invented_x_place_kind_4266"),
        ("instagram", "invented_instagram_place_kind_4266"),
        ("messenger", "invented_messenger_place_kind_4266"),
    ];

    assert_eq!(made_up.len(), 3);

    for (app, name) in made_up {
        let refusal = match app {
            "x" => normalize_place_kind_for_app("x", name).unwrap_err(),
            "instagram" => parse_instagram_whitelist_kind(name).unwrap_err(),
            "messenger" => parse_messenger_whitelist_kind(name).unwrap_err(),
            _ => unreachable!(),
        };
        println!("TASK4266_MADE_UP_KIND app={app} name={name} refusal={refusal}");
        assert!(
            refusal.contains(name),
            "refusal for {app}/{name} must name the made-up kind: {refusal}"
        );
    }
}

#[test]
fn task_4266_made_up_refusal_wording_differs_from_not_built_yet_refusal_wording() {
    // Telegram story and WhatsApp status are real, planned shapes the base
    // tasks (0147, 0153) explicitly held back until their own follow-on tasks
    // (1029a/4218 for Telegram story; 1089d/4220 for WhatsApp status) prove a
    // real place -- they are refused today, but for a different reason than a
    // name nobody ever planned to support.
    let telegram_made_up = parse_telegram_whitelist_kind("invented_telegram_place_kind_4266")
        .expect_err("made-up Telegram kind must be refused");
    let telegram_not_built_yet =
        parse_telegram_whitelist_kind("story").expect_err("Telegram story is not built yet");

    let whatsapp_made_up = parse_whatsapp_whitelist_kind("invented_whatsapp_place_kind_4266")
        .expect_err("made-up WhatsApp kind must be refused");
    let whatsapp_not_built_yet =
        parse_whatsapp_whitelist_kind("status").expect_err("WhatsApp status is not built yet");

    println!("TASK4266_MADE_UP_REFUSAL app=telegram wording=\"{telegram_made_up}\"");
    println!("TASK4266_NOT_BUILT_YET_REFUSAL app=telegram kind=story wording=\"{telegram_not_built_yet}\"");
    println!("TASK4266_MADE_UP_REFUSAL app=whatsapp wording=\"{whatsapp_made_up}\"");
    println!("TASK4266_NOT_BUILT_YET_REFUSAL app=whatsapp kind=status wording=\"{whatsapp_not_built_yet}\"");

    assert_ne!(
        telegram_made_up, telegram_not_built_yet,
        "made-up refusal wording must differ from the not-built-yet refusal wording"
    );
    assert_ne!(
        whatsapp_made_up, whatsapp_not_built_yet,
        "made-up refusal wording must differ from the not-built-yet refusal wording"
    );
}

#[test]
fn task_4266_zero_kinds_are_silently_accepted() {
    let should_be_refused: Vec<(&str, &str)> = vec![
        ("x", "invented_x_place_kind_4266"),
        ("instagram", "invented_instagram_place_kind_4266"),
        ("messenger", "invented_messenger_place_kind_4266"),
        ("telegram", "story"),  // not built yet, must still be refused
        ("whatsapp", "status"), // not built yet, must still be refused
    ];

    let mut silently_accepted = Vec::new();
    for (app, name) in &should_be_refused {
        let accepted = match *app {
            "x" => normalize_place_kind_for_app("x", name).is_ok(),
            "instagram" => parse_instagram_whitelist_kind(name).is_ok(),
            "messenger" => parse_messenger_whitelist_kind(name).is_ok(),
            "telegram" => parse_telegram_whitelist_kind(name).is_ok(),
            "whatsapp" => parse_whatsapp_whitelist_kind(name).is_ok(),
            _ => unreachable!(),
        };
        println!("TASK4266_SILENT_ACCEPT_CHECK app={app} name={name} accepted={accepted}");
        if accepted {
            silently_accepted.push(format!("{app}:{name}"));
        }
    }

    println!(
        "TASK4266_SILENTLY_ACCEPTED_COUNT count={} names={}",
        silently_accepted.len(),
        silently_accepted.join(",")
    );
    assert_eq!(silently_accepted.len(), 0);
}
