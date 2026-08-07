use ipc::allowed_places::AllowedPlaceRecord;
use ipc::commands::{cmd_osl_add_allowed_place_record, cmd_osl_list_allowed_place_records};
use ipc::state::AppState;
use std::process::Command;

const ACCOUNT: &str = "telegram-account-0149";

fn telegram_fixture_place(kind: &str, place_id: &str) -> Result<AllowedPlaceRecord, String> {
    AllowedPlaceRecord::telegram(ACCOUNT, kind, place_id)
}

#[test]
fn task_0149_four_telegram_fixture_places_resolve_and_server_exits_one() {
    let state = AppState::new();
    let cases = [
        ("direct_message", "tg-dm-0149"),
        ("group_chat", "tg-group-0149"),
        ("channel", "tg-channel-0149"),
        ("public_post", "tg-post-0149"),
    ];

    let mut resolved_count = 0usize;
    for (index, (kind, place_id)) in cases.into_iter().enumerate() {
        let fixture = telegram_fixture_place(kind, place_id).expect("telegram fixture place");
        let stored = cmd_osl_add_allowed_place_record(
            &state,
            fixture.app.clone(),
            fixture.account.clone(),
            fixture.kind.clone(),
            fixture.stable_id.clone(),
            None,
        )
        .expect("store telegram fixture place");
        resolved_count += 1;

        println!("TASK0149 fixture_place.{index}.resolved=true");
        println!("TASK0149 fixture_place.{index}.app={}", stored.app);
        println!("TASK0149 fixture_place.{index}.account={}", stored.account);
        println!("TASK0149 fixture_place.{index}.kind={}", stored.kind);
        println!(
            "TASK0149 fixture_place.{index}.stable_id={}",
            stored.stable_id
        );

        assert_eq!(stored, fixture);
        assert_eq!(stored.app, "telegram");
        assert_eq!(stored.account, ACCOUNT);
        assert_eq!(stored.kind, kind);
        assert_eq!(
            stored.stable_id,
            format!("telegram:{ACCOUNT}:{kind}:{place_id}")
        );
    }

    let records = cmd_osl_list_allowed_place_records(&state).expect("list allowed places");
    println!("TASK0149 fixture_place.resolved_count={resolved_count}");
    println!(
        "TASK0149 fixture_place.stored_record_count={}",
        records.len()
    );
    assert_eq!(resolved_count, 4);
    assert_eq!(records.len(), 4);

    let output = Command::new(std::env::current_exe().expect("current test binary"))
        .arg("task_0149_server_child_process")
        .arg("--exact")
        .arg("--nocapture")
        .env("TASK0149_CHILD_KIND", "server")
        .output()
        .expect("run invalid server child");
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));

    let exit_code = output.status.code().unwrap_or(-1);
    println!("TASK0149 invalid_kind_exit_code={exit_code}");
    assert_eq!(exit_code, 1);
}

#[test]
fn task_0149_server_child_process() {
    let Ok(kind) = std::env::var("TASK0149_CHILD_KIND") else {
        return;
    };
    println!("TASK0149 invalid_kind_attempt.kind={kind}");
    match telegram_fixture_place(&kind, "tg-server-0149") {
        Ok(record) => {
            println!(
                "TASK0149 invalid_kind_attempt.unexpected_stable_id={}",
                record.stable_id
            );
            std::process::exit(0);
        }
        Err(error) => {
            println!("TASK0149 invalid_kind_attempt.error={error}");
            std::process::exit(1);
        }
    }
}
