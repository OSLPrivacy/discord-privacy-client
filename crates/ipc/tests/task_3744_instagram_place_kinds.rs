use ipc::allowed_places::AllowedPlaceRecord;
use ipc::auto_whitelist_rules::{
    instagram_auto_whitelist_rule_key, parse_instagram_whitelist_kind,
};
use ipc::commands::{
    cmd_osl_get_instagram_whitelist_kinds, cmd_osl_new_place, cmd_osl_save_auto_whitelist_rule,
};
use ipc::state::AppState;
use std::process::Command;

const INSTAGRAM_PLACE_KIND: &str = env!("CARGO_BIN_EXE_instagram-place-kind");
const EXPECTED_KINDS: [&str; 5] = [
    "direct_message",
    "group_chat",
    "public_post",
    "comment",
    "story",
];

#[test]
fn task_3744_instagram_has_exactly_five_kinds_and_all_resolve() {
    let kinds = cmd_osl_get_instagram_whitelist_kinds().expect("instagram whitelist kinds");
    let ids = kinds
        .iter()
        .map(|kind| kind.id.as_str())
        .collect::<Vec<_>>();
    let names = kinds
        .iter()
        .map(|kind| kind.name.as_str())
        .collect::<Vec<_>>();
    let mut resolved = Vec::new();

    for id in EXPECTED_KINDS {
        let parsed = parse_instagram_whitelist_kind(id).expect("known Instagram kind resolves");
        resolved.push(parsed.id());
    }

    println!(
        "TASK3744_INSTAGRAM_KIND_LIST count={} ids={} names={} resolved_count={} resolved={}",
        ids.len(),
        ids.join(","),
        names.join(","),
        resolved.len(),
        resolved.join(",")
    );

    assert_eq!(ids, EXPECTED_KINDS);
    assert_eq!(
        names,
        vec![
            "direct message",
            "group chat",
            "public post",
            "comment",
            "story"
        ]
    );
    assert_eq!(resolved, EXPECTED_KINDS);
}

#[test]
fn task_3744_five_instagram_kinds_allow_and_sixth_invented_kind_exits_1_by_name() {
    let mut allowed = Vec::new();

    for kind in EXPECTED_KINDS {
        let output = Command::new(INSTAGRAM_PLACE_KIND)
            .arg(kind)
            .output()
            .expect("run instagram-place-kind for known kind");
        let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
        assert_eq!(output.status.code(), Some(0), "{stderr}");
        assert!(
            stdout.contains(&format!("kind={kind} status=allowed")),
            "{stdout}"
        );
        allowed.push(kind);
    }

    let invented = Command::new(INSTAGRAM_PLACE_KIND)
        .arg("invented_instagram_place_kind_3744")
        .output()
        .expect("run instagram-place-kind for invented kind");
    let invented_stderr = String::from_utf8(invented.stderr).expect("stderr is UTF-8");
    assert_eq!(invented.status.code(), Some(1), "{invented_stderr}");
    assert!(
        invented_stderr.contains("invented_instagram_place_kind_3744"),
        "{invented_stderr}"
    );

    println!(
        "TASK3744_INSTAGRAM_PLACE_ALLOW allowed_count={} allowed={} invented_kind=invented_instagram_place_kind_3744 invented_exit=1 invented_error=\"{}\"",
        allowed.len(),
        allowed.join(","),
        invented_stderr.trim()
    );
}

#[test]
fn task_1154_and_1158_instagram_comment_and_story_against_list_return_allowed() {
    let comment = run_task_against_list("1154", "comment").expect("TASK1154 comment allowed");
    let story = run_task_against_list("1158", "story").expect("TASK1158 story allowed");

    println!(
        "TASK3744_INSTAGRAM_TASK_RUNS task1154={} task1158={}",
        comment, story
    );

    assert_eq!(comment, "allowed");
    assert_eq!(story, "allowed");
}

fn run_task_against_list(task: &str, kind_id: &str) -> Result<String, String> {
    let kinds = cmd_osl_get_instagram_whitelist_kinds()?;
    if !kinds.iter().any(|kind| kind.id == kind_id) {
        return Err(format!(
            "TASK{task}: refusal kind {kind_id} missing from list"
        ));
    }

    let kind = parse_instagram_whitelist_kind(kind_id)?;
    let state = AppState::new();
    cmd_osl_save_auto_whitelist_rule(
        &state,
        instagram_auto_whitelist_rule_key(kind),
        "always".to_owned(),
        None,
    )?;
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let decision = cmd_osl_new_place(
        &state,
        AllowedPlaceRecord {
            app: "instagram".to_owned(),
            account: format!("instagram-account-{task}"),
            kind: kind_id.to_owned(),
            stable_id: format!("instagram:instagram-account-{task}:{kind_id}:place-{task}"),
            place_name: format!("Instagram {kind_id} task {task}"),
            person_name: format!("Instagram Task {task}"),
        },
        Some(dir.path().to_path_buf()),
    )?;
    Ok(decision.status)
}
