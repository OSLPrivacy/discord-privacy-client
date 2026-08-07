#![cfg(feature = "core")]

use osl_privacy_hub::row_who_wrote_it::{
    accept_published_row_batch, SharedRowWhoWroteIt, SharedRowWhoWroteItEvidence,
};
use osl_privacy_hub::service_connections::{mail_message_who_wrote_it, VisibleMailMessage};

fn row(name: impl Into<String>, who_wrote_it: SharedRowWhoWroteIt) -> SharedRowWhoWroteItEvidence {
    SharedRowWhoWroteItEvidence::new(name, Some(who_wrote_it))
}

fn source_occurrences(needle: &str) -> usize {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        "src/row_who_wrote_it.rs",
        "src/service_connections.rs",
        "src/broker.rs",
        "src/native_discord_adapter.rs",
    ]
    .into_iter()
    .map(|relative| {
        let text = std::fs::read_to_string(manifest_dir.join(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        text.matches(needle).count()
    })
    .sum()
}

fn source_declaration_lines(needle: &str) -> usize {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        "src/row_who_wrote_it.rs",
        "src/service_connections.rs",
        "src/broker.rs",
        "src/native_discord_adapter.rs",
    ]
    .into_iter()
    .map(|relative| {
        let text = std::fs::read_to_string(manifest_dir.join(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        text.lines().filter(|line| line.trim() == needle).count()
    })
    .sum()
}

#[test]
fn task_4070_shared_who_wrote_it_has_three_states_and_batch_refusals() {
    let states = SharedRowWhoWroteIt::STATES
        .iter()
        .map(|state| state.as_str())
        .collect::<Vec<_>>();
    let made_up_fourth =
        SharedRowWhoWroteIt::parse_name("ghost_writer").expect_err("fourth state is refused");

    let signed_in = "owner@example.test";
    let yours = VisibleMailMessage {
        message_id: "mail-4070-yours".to_owned(),
        mailbox: "Sent".to_owned(),
        sender_address: Some("owner@example.test".to_owned()),
    };
    let theirs = VisibleMailMessage {
        message_id: "mail-4070-theirs".to_owned(),
        mailbox: "Inbox".to_owned(),
        sender_address: Some("friend@example.test".to_owned()),
    };
    assert_eq!(
        mail_message_who_wrote_it(signed_in, &yours).unwrap(),
        SharedRowWhoWroteIt::Yours
    );
    assert_eq!(
        mail_message_who_wrote_it(signed_in, &theirs).unwrap(),
        SharedRowWhoWroteIt::Theirs
    );

    let mut not_published_batch = (0..10)
        .map(|index| {
            let answer = if index == 6 {
                SharedRowWhoWroteIt::NotPublishedByApp
            } else if index % 2 == 0 {
                SharedRowWhoWroteIt::Yours
            } else {
                SharedRowWhoWroteIt::Theirs
            };
            row(format!("row-4070-{index:02}"), answer)
        })
        .collect::<Vec<_>>();
    let not_published_result = accept_published_row_batch(&not_published_batch);

    let all_answered_batch = (0..10)
        .map(|index| {
            let answer = if index % 2 == 0 {
                SharedRowWhoWroteIt::Yours
            } else {
                SharedRowWhoWroteIt::Theirs
            };
            row(format!("row-4070-all-{index:02}"), answer)
        })
        .collect::<Vec<_>>();
    let all_answered_result = accept_published_row_batch(&all_answered_batch);

    not_published_batch[3].who_wrote_it = None;
    let missing_evidence_result = accept_published_row_batch(&not_published_batch);

    let answer_shape_count = source_declaration_lines("pub enum SharedRowWhoWroteIt {");
    let app_local_answer_shape_count = [
        "pub enum NativeDiscordRowPoster",
        "pub enum RehydratedRowPoster",
    ]
    .into_iter()
    .map(source_occurrences)
    .sum::<usize>();

    println!("TASK4070 state_count={}", states.len());
    println!("TASK4070 states={}", states.join(","));
    println!("TASK4070 made_up_fourth=ghost_writer");
    println!(
        "TASK4070 made_up_fourth_refusal={}",
        made_up_fourth.reason()
    );
    println!("TASK4070 not_published_batch.row_count=10");
    println!(
        "TASK4070 not_published_batch.accepted_rows={}",
        not_published_result.accepted_rows
    );
    println!(
        "TASK4070 not_published_batch.refusal={}",
        not_published_result.refusal.as_deref().unwrap_or("<none>")
    );
    println!(
        "TASK4070 all_answered_batch.accepted_rows={}",
        all_answered_result.accepted_rows
    );
    println!(
        "TASK4070 all_answered_batch.refusal={}",
        all_answered_result.refusal.as_deref().unwrap_or("<none>")
    );
    println!(
        "TASK4070 missing_evidence_batch.accepted_rows={}",
        missing_evidence_result.accepted_rows
    );
    println!(
        "TASK4070 missing_evidence_batch.refusal={}",
        missing_evidence_result
            .refusal
            .as_deref()
            .unwrap_or("<none>")
    );
    println!("TASK4070 search_answer_shape_count={answer_shape_count}");
    println!("TASK4070 search_app_local_answer_shape_count={app_local_answer_shape_count}");

    assert_eq!(states, vec!["yours", "theirs", "not_published_by_app"]);
    assert_eq!(
        made_up_fourth.reason(),
        "OSL: unknown who-wrote-it answer ghost_writer"
    );
    assert_eq!(not_published_result.accepted_rows, 0);
    assert_eq!(
        not_published_result.refusal.as_deref(),
        Some("OSL: row row-4070-06 was not published by the app")
    );
    assert_eq!(all_answered_result.accepted_rows, 10);
    assert_eq!(all_answered_result.refusal, None);
    assert_eq!(missing_evidence_result.accepted_rows, 0);
    assert_eq!(
        missing_evidence_result.refusal.as_deref(),
        Some("OSL: row row-4070-03 has no who-wrote-it evidence")
    );
    assert_eq!(answer_shape_count, 1);
    assert_eq!(app_local_answer_shape_count, 0);
}
