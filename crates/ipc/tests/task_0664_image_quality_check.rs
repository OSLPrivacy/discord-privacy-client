use ipc::scope::{ScopeInput, ScopeKind};
use std::process::Command;

const CHILD_ENV: &str = "OSL_TASK_0664_UNRECOVERABLE_FIXTURE";
const UNRECOVERABLE_COPY: &str = "ordinary prose with no recoverable pointer";

fn scope() -> ScopeInput {
    ScopeInput {
        kind: ScopeKind::Dm,
        id: "task-0664-peer".to_owned(),
        server_id: None,
        channel_id: Some("task-0664-dm-channel".to_owned()),
    }
}

fn recover_prepared_pointer(copy: &str) -> bool {
    let detection_key = [0x66u8; 32];
    ipc::prose_token::prose_token_recover_pointer(&scope(), &detection_key, copy)
        .expect("local pointer recovery must not need network")
        .is_some()
}

#[test]
fn task_0664_unrecoverable_child() {
    if std::env::var_os(CHILD_ENV).is_none() {
        return;
    }

    let mut post_commands = 0usize;
    let pointer_recovered = recover_prepared_pointer(UNRECOVERABLE_COPY);
    println!("TASK0664_UNRECOVERABLE_POINTER_RECOVERED={pointer_recovered}");
    if !pointer_recovered {
        println!("TASK0664_POST_COMMANDS_BEFORE_EXIT={post_commands}");
        println!("TASK0664_REFUSED_BEFORE_POST_COMMAND=true");
        std::process::exit(1);
    }

    post_commands += 1;
    println!("TASK0664_POST_COMMANDS_BEFORE_EXIT={post_commands}");
    std::process::exit(0);
}

#[test]
fn unrecoverable_fixture_exits_1_before_any_post_command() {
    let output = Command::new(std::env::current_exe().expect("test executable path"))
        .env(CHILD_ENV, "1")
        .arg("--exact")
        .arg("task_0664_unrecoverable_child")
        .arg("--nocapture")
        .output()
        .expect("unrecoverable fixture child runs");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code();

    println!(
        "TASK0664_OBSERVED_UNRECOVERABLE_FIXTURE_EXIT_CODE={}",
        exit_code.unwrap_or(-1)
    );
    print!("{stdout}");
    eprint!("{stderr}");

    assert_eq!(exit_code, Some(1), "fixture must exit 1");
    assert!(
        stdout.contains("TASK0664_UNRECOVERABLE_POINTER_RECOVERED=false"),
        "fixture must prove the pointer was not recovered"
    );
    assert!(
        stdout.contains("TASK0664_POST_COMMANDS_BEFORE_EXIT=0"),
        "fixture must refuse before any post command"
    );
    assert!(
        stdout.contains("TASK0664_REFUSED_BEFORE_POST_COMMAND=true"),
        "fixture must name the ordering claim"
    );
}

#[test]
fn image_quality_check_is_before_provider_post_command() {
    let source = include_str!("../../../apps/osl-hub/src/hub_command_surface.rs");
    let send_start = source
        .find("pub fn direct_photo_post_after_image_quality_check<")
        .expect("image-hidden direct post command remains present");
    let send = &source[send_start..];
    let carrier_ready = send
        .find("image_hidden_photo_command_copies(")
        .expect("prepared image copies remain present");
    let quality_check = send
        .find("check_quality(&copies)?;")
        .expect("prepared image quality check is wired");
    let refusal = send
        .find("provider_copies = ImageCopyCommandResult")
        .expect("provider-boundary copy selection remains explicit");
    let post_command = send
        .find("provider_post(&provider_copies)?")
        .expect("provider post command remains present");

    println!("TASK0664_SOURCE_ORDER_CARRIER_READY={carrier_ready}");
    println!("TASK0664_SOURCE_ORDER_QUALITY_CHECK={quality_check}");
    println!("TASK0664_SOURCE_ORDER_REFUSAL={refusal}");
    println!("TASK0664_SOURCE_ORDER_POST_COMMAND={post_command}");

    assert!(carrier_ready < quality_check);
    assert!(quality_check < refusal);
    assert!(refusal < post_command);
}
