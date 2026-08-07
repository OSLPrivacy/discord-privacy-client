use std::process::Command;

const MESSENGER_PLACE_KIND: &str = env!("CARGO_BIN_EXE_messenger-place-kind");

#[test]
fn task_3745_messenger_kind_list_has_exactly_three_and_all_resolve() {
    let kinds = ipc::commands::cmd_osl_get_messenger_whitelist_kinds().unwrap();
    let ids: Vec<String> = kinds.iter().map(|kind| kind.id.clone()).collect();
    let names: Vec<String> = kinds.iter().map(|kind| kind.name.clone()).collect();

    println!("TASK3745_MESSENGER_KIND_COUNT={}", kinds.len());
    println!("TASK3745_MESSENGER_KINDS={}", ids.join(","));
    println!("TASK3745_MESSENGER_KIND_NAMES={}", names.join(","));

    assert_eq!(kinds.len(), 3);
    assert_eq!(ids, vec!["direct_message", "group_chat", "community"]);

    for id in &ids {
        let output = Command::new(MESSENGER_PLACE_KIND)
            .arg(id)
            .output()
            .expect("run Messenger fixture place");
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
        assert!(
            output.status.success(),
            "{id} should resolve, stdout={stdout}, stderr={stderr}"
        );
        assert!(
            stdout.contains(&format!("kind={id} status=allowed")),
            "{stdout}"
        );
        println!(
            "TASK3745_RESOLVE kind={id} exit={} status=allowed",
            output.status.code().unwrap_or(0)
        );
    }

    let invented_name = "invented_messenger_place_kind_3745";
    let invented = Command::new(MESSENGER_PLACE_KIND)
        .arg(invented_name)
        .output()
        .expect("run Messenger invented fixture place");
    let invented_stderr = String::from_utf8(invented.stderr).expect("stderr is UTF-8");
    assert_eq!(invented.status.code(), Some(1), "{invented_stderr}");
    assert!(
        invented_stderr
            .contains("OSL: unknown Messenger whitelist kind 'invented_messenger_place_kind_3745'"),
        "{invented_stderr}"
    );
    println!(
        "TASK3745_INVENTED_KIND_EXIT_BY_NAME name={invented_name} exit=1 refusal=\"{}\"",
        invented_stderr.trim()
    );

    let task1190 = Command::new(MESSENGER_PLACE_KIND)
        .arg("community")
        .output()
        .expect("run task 1190 Messenger community fixture place");
    let task1190_stdout = String::from_utf8(task1190.stdout).expect("stdout is UTF-8");
    let task1190_stderr = String::from_utf8(task1190.stderr).expect("stderr is UTF-8");
    assert!(
        task1190.status.success(),
        "TASK1190 should resolve, stdout={task1190_stdout}, stderr={task1190_stderr}"
    );
    assert!(
        task1190_stdout.contains("kind=community status=allowed"),
        "{task1190_stdout}"
    );
    println!(
        "TASK3745_TASK1190_AGAINST_LIST kind=community exit={} status=allowed",
        task1190.status.code().unwrap_or(0)
    );
}
