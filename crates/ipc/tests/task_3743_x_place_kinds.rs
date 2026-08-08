use ipc::auto_whitelist_rules::{
    normalize_place_kind_for_app, X_DIRECT_MESSAGE_PLACE_KIND, X_GROUP_DIRECT_MESSAGE_PLACE_KIND,
    X_PLACE_KINDS, X_PUBLIC_POST_PLACE_KIND, X_REPLY_PLACE_KIND,
};
use ipc::commands::{cmd_osl_direct_new_place, cmd_osl_save_auto_whitelist_rule_for_place};
use ipc::AppState;
use tempfile::tempdir;

#[test]
fn task_3743_x_kind_list_has_four_resolving_kinds_and_allows_1125_1127() {
    let expected = [
        X_DIRECT_MESSAGE_PLACE_KIND,
        X_PUBLIC_POST_PLACE_KIND,
        X_GROUP_DIRECT_MESSAGE_PLACE_KIND,
        X_REPLY_PLACE_KIND,
    ];
    println!("TASK3743_X_KIND_COUNT={}", X_PLACE_KINDS.len());
    println!("TASK3743_X_KIND_IDS={}", X_PLACE_KINDS.join(","));
    assert_eq!(X_PLACE_KINDS, expected);

    for kind in expected {
        let resolved = normalize_place_kind_for_app("x", kind).expect("known X kind resolves");
        println!("TASK3743_X_RESOLVED id={resolved} result=allowed");
        assert_eq!(resolved, kind);
    }

    let invented = run_task_3743_x_kind_probe("invented_kind");
    let invented_stderr = String::from_utf8_lossy(&invented.stderr);
    println!(
        "TASK3743_X_INVENTED_KIND_EXIT={}",
        invented.status.code().unwrap_or(-1)
    );
    println!(
        "TASK3743_X_INVENTED_KIND_REFUSAL={}",
        invented_stderr.trim()
    );
    assert_eq!(invented.status.code(), Some(1));
    assert!(invented_stderr.contains("invented_kind"));

    let state = AppState::new();
    let dirs = tempdir().expect("temp dirs");
    let app_data_dir = dirs.path().join("app-data");
    let prefs_dir = dirs.path().join("prefs");

    for (task_id, kind) in [
        ("1125", X_GROUP_DIRECT_MESSAGE_PLACE_KIND),
        ("1127", X_REPLY_PLACE_KIND),
    ] {
        cmd_osl_save_auto_whitelist_rule_for_place(
            &state,
            "x".to_owned(),
            kind.to_owned(),
            "always".to_owned(),
            Some(prefs_dir.clone()),
        )
        .expect("save X kind rule");

        let decision = cmd_osl_direct_new_place(
            &state,
            app_data_dir.clone(),
            "x".to_owned(),
            kind.to_owned(),
            format!("x-task-{task_id}-3743"),
            Some(format!("task {task_id} X place")),
        )
        .expect("X place decision resolves");

        println!(
            "TASK3743_TASK_{task_id}_KIND={kind} RESULT={}",
            decision.result
        );
        assert_eq!(decision.result, "allowed");
    }
}

fn run_task_3743_x_kind_probe(kind: &str) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current test binary"))
        .arg("task_3743_x_kind_probe_child")
        .arg("--ignored")
        .arg("--nocapture")
        .env("TASK3743_X_KIND_PROBE", kind)
        .output()
        .expect("run task 3743 child probe")
}

#[test]
#[ignore]
fn task_3743_x_kind_probe_child() {
    let kind = std::env::var("TASK3743_X_KIND_PROBE").expect("probe kind");
    match normalize_place_kind_for_app("x", &kind) {
        Ok(resolved) => println!("TASK3743_X_KIND_PROBE_ALLOWED={resolved}"),
        Err(refusal) => {
            eprintln!("{refusal}");
            std::process::exit(1);
        }
    }
}
