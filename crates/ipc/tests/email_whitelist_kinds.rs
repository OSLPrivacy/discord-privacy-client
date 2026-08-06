use ipc::commands::cmd_osl_list_email_whitelist_kinds;
use ipc::email_whitelist_kinds::{parse_email_whitelist_kind, EmailWhitelistKind};
use std::process::Command;

#[test]
fn email_kinds_command_returns_exactly_two_named_kinds() {
    let kinds = cmd_osl_list_email_whitelist_kinds();
    let names: Vec<&str> = kinds.iter().map(|kind| kind.name.as_str()).collect();

    println!("email whitelist kinds: {}", names.join(", "));
    println!("TASK0165 email_kind_count={}", names.len());

    assert_eq!(names, vec!["email address", "email domain"]);
}

#[test]
fn task_0167_email_address_and_domain_fixtures_resolve_and_thread_exits_1() {
    let fixtures = [
        ("address", EmailWhitelistKind::EmailAddress),
        ("domain", EmailWhitelistKind::EmailDomain),
    ];

    let resolved: Vec<_> = fixtures
        .iter()
        .map(|(fixture, expected)| {
            let kind = parse_email_whitelist_kind(fixture).expect("fixture email kind resolves");
            assert_eq!(kind, *expected);
            (*fixture, kind.id(), kind.name())
        })
        .collect();

    println!(
        "TASK0167_EMAIL_FIXTURES created={} resolved={} kinds={} {}",
        fixtures.len(),
        resolved.len(),
        resolved
            .iter()
            .map(|(_, id, _)| *id)
            .collect::<Vec<_>>()
            .join(","),
        resolved
            .iter()
            .map(|(fixture, _, name)| format!("{fixture}={name}"))
            .collect::<Vec<_>>()
            .join(" | ")
    );

    assert_eq!(resolved.len(), 2);

    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("task_0167_email_thread_probe")
        .arg("--nocapture")
        .env("TASK0167_EMAIL_THREAD_PROBE", "1")
        .output()
        .expect("run email-thread rejection helper");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");

    assert_eq!(output.status.code(), Some(1));
    println!("TASK0167_THIRD_KIND kind=email_thread exit_code=1");
}

#[test]
#[ignore = "task 0167 helper: exits 1 only when email thread is refused"]
fn task_0167_email_thread_probe() {
    if std::env::var_os("TASK0167_EMAIL_THREAD_PROBE").is_none() {
        return;
    }
    match parse_email_whitelist_kind("email thread") {
        Ok(kind) => {
            println!(
                "TASK0167_THIRD_KIND_UNEXPECTEDLY_RESOLVED kind=email_thread resolved={}",
                kind.name()
            );
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("TASK0167_THIRD_KIND_REFUSED kind=email_thread error={error}");
            std::process::exit(1);
        }
    }
}
