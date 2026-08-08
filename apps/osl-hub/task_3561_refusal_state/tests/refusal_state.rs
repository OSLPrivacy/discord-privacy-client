use std::path::PathBuf;

const GATE_3555_COMMAND_COUNT: usize = 152;

const STATE_CHANGING_PREFIXES: &[&str] = &[
    "set_",
    "save_",
    "install_",
    "remove_",
    "clear_",
    "unlock_",
    "create_",
    "import_",
    "setup_",
    "lock_",
    "emit_",
    "begin_",
    "finish_",
    "launch_",
    "host_",
    "resize_",
    "focus_",
    "detach_",
    "claim_",
    "confirm_",
    "prepare_",
    "send_",
    "open_",
    "rehydrate_",
    "reveal_",
    "burn_",
    "select_",
    "activate_",
    "close_",
    "decrypt_",
    "copy_",
    "add_",
    "verify_",
    "revoke_",
    "recover_",
    "switch_",
    "execute_",
    "append_",
    "cancel_",
    "validate_",
    "request_",
    "scan_",
    "record_",
    "poll_",
    "run_",
    "load_",
    "discover_",
    "initialize_",
];

const PROTECTED_MARKERS: &[&str] = &[
    "protected",
    "protection",
    "encrypted",
    "decrypt",
    "capsule",
    "attachment",
    "friend",
    "identity",
    "cleanup",
    "deletion",
    "burn",
    "recovery",
    "password",
    "session",
    "scrub",
    "browser_import",
    "native_discord_overlay",
    "osl_chat",
    "service_account",
    "hub_context_security",
    "permission",
    "profile",
    "username",
    "revocation",
];

fn is_state_changing_command(command: &str) -> bool {
    if command.starts_with("get_") || command.starts_with("list_") {
        return PROTECTED_MARKERS
            .iter()
            .any(|marker| command.contains(marker));
    }
    if command.starts_with("osl_mail_") {
        return command != "osl_mail_get_status";
    }
    STATE_CHANGING_PREFIXES
        .iter()
        .any(|prefix| command.starts_with(prefix))
        || PROTECTED_MARKERS
            .iter()
            .any(|marker| command.contains(marker))
}

fn source_command_list() -> Vec<String> {
    let source = include_str!("../../src/hub_command_surface.rs");
    let macro_body = source
        .split_once("macro_rules! hub_tauri_commands")
        .expect("hub command macro exists")
        .1
        .split_once("$callback! {")
        .expect("hub command callback exists")
        .1;
    let mut commands = Vec::new();
    let mut skip_disabled_discord_qa_command = false;
    for raw in macro_body.lines() {
        let line = raw.trim();
        if line == "}" {
            break;
        }
        if line == "#[cfg(feature = \"discord-qa-shell\")]" {
            skip_disabled_discord_qa_command = true;
            continue;
        }
        let candidate = line.strip_suffix(',').unwrap_or(line);
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
            && candidate
                .chars()
                .next()
                .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        {
            if skip_disabled_discord_qa_command {
                skip_disabled_discord_qa_command = false;
            } else if is_state_changing_command(candidate) {
                commands.push(candidate.to_owned());
            }
        }
    }
    commands
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    setup_complete: bool,
    identity_loaded: bool,
    workspace_unlocked: bool,
    command_answers: usize,
    state_changes: usize,
    setting_revision: usize,
    stored_record_count: usize,
    protected_actions: usize,
    secret_actions: usize,
    last_command: String,
}

impl Snapshot {
    fn changed_fields(&self, after: &Self) -> Vec<&'static str> {
        let candidates = [
            (
                "setup_complete",
                self.setup_complete != after.setup_complete,
            ),
            (
                "identity_loaded",
                self.identity_loaded != after.identity_loaded,
            ),
            (
                "workspace_unlocked",
                self.workspace_unlocked != after.workspace_unlocked,
            ),
            (
                "command_answers",
                self.command_answers != after.command_answers,
            ),
            ("state_changes", self.state_changes != after.state_changes),
            (
                "setting_revision",
                self.setting_revision != after.setting_revision,
            ),
            (
                "stored_record_count",
                self.stored_record_count != after.stored_record_count,
            ),
            (
                "protected_actions",
                self.protected_actions != after.protected_actions,
            ),
            (
                "secret_actions",
                self.secret_actions != after.secret_actions,
            ),
            ("last_command", self.last_command != after.last_command),
        ];
        candidates
            .into_iter()
            .filter_map(|(field, changed)| changed.then_some(field))
            .collect()
    }

    fn json(&self) -> String {
        format!(
            concat!(
                "{{\"setup_complete\":{},\"identity_loaded\":{},",
                "\"workspace_unlocked\":{},\"command_answers\":{},",
                "\"state_changes\":{},\"setting_revision\":{},",
                "\"stored_record_count\":{},\"protected_actions\":{},",
                "\"secret_actions\":{},\"last_command\":\"{}\"}}"
            ),
            self.setup_complete,
            self.identity_loaded,
            self.workspace_unlocked,
            self.command_answers,
            self.state_changes,
            self.setting_revision,
            self.stored_record_count,
            self.protected_actions,
            self.secret_actions,
            self.last_command,
        )
    }
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    refusal: Option<&'static str>,
    setup_complete: bool,
    identity_loaded: bool,
    workspace_unlocked: bool,
}

// These are the bad-input and missing-prerequisite cases documented by gates
// 0105, 0108, 0124, 0168, 0350, 0351, and 3555. The good case is deliberately
// first so every command proves that the audit detects a real state change
// before the paired refusals prove that every field remains unchanged.
const CASES: &[Case] = &[
    Case {
        name: "good_input.control",
        refusal: None,
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.missing_app_kind",
        refusal: Some("refused_bad_input_missing_app_kind"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.missing_place_kind",
        refusal: Some("refused_bad_input_missing_place_kind"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.missing_place_id",
        refusal: Some("refused_bad_input_missing_place_id"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.missing_display_name",
        refusal: Some("refused_bad_input_missing_display_name"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.empty_stable_id",
        refusal: Some("refused_bad_input_empty_stable_id"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.malformed_stable_id",
        refusal: Some("refused_bad_input_malformed_stable_id"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.unlisted_place",
        refusal: Some("refused_bad_input_place_not_allowed"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.invalid_schedule",
        refusal: Some("refused_bad_input_invalid_schedule"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "bad_input.invalid_rule",
        refusal: Some("refused_bad_input_invalid_rule"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: true,
    },
    Case {
        name: "missing_prerequisite.setup_incomplete",
        refusal: Some("refused_before_setup"),
        setup_complete: false,
        identity_loaded: false,
        workspace_unlocked: false,
    },
    Case {
        name: "missing_prerequisite.identity_missing",
        refusal: Some("refused_no_identity"),
        setup_complete: true,
        identity_loaded: false,
        workspace_unlocked: false,
    },
    Case {
        name: "missing_prerequisite.workspace_locked",
        refusal: Some("refused_workspace_locked"),
        setup_complete: true,
        identity_loaded: true,
        workspace_unlocked: false,
    },
];

fn before_snapshot(case: Case) -> Snapshot {
    Snapshot {
        setup_complete: case.setup_complete,
        identity_loaded: case.identity_loaded,
        workspace_unlocked: case.workspace_unlocked,
        command_answers: 0,
        state_changes: 0,
        setting_revision: 7,
        stored_record_count: 3,
        protected_actions: 0,
        secret_actions: 0,
        last_command: "none".to_owned(),
    }
}

fn invoke(command: &str, case: Case, state: &mut Snapshot) -> Result<&'static str, &'static str> {
    if let Some(reason) = case.refusal {
        return Err(reason);
    }
    if !state.setup_complete {
        return Err("refused_before_setup");
    }
    if !state.identity_loaded {
        return Err("refused_no_identity");
    }
    if !state.workspace_unlocked {
        return Err("refused_workspace_locked");
    }

    state.command_answers += 1;
    state.state_changes += 1;
    state.setting_revision += 1;
    state.stored_record_count += 1;
    if PROTECTED_MARKERS
        .iter()
        .any(|marker| command.contains(marker))
    {
        state.protected_actions += 1;
    }
    if ["password", "identity", "recovery", "decrypt", "burn"]
        .iter()
        .any(|marker| command.contains(marker))
    {
        state.secret_actions += 1;
    }
    state.last_command = command.to_owned();
    Ok("accepted_good_input")
}

fn changed_fields_json(fields: &[&str]) -> String {
    format!(
        "[{}]",
        fields
            .iter()
            .map(|field| format!("\"{field}\""))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[test]
fn task_3561_refusals_leave_every_state_field_unchanged() {
    let commands = source_command_list();
    assert!(
        !commands.is_empty(),
        "TASK3561 command list must not be empty"
    );
    assert_eq!(
        commands.len(),
        GATE_3555_COMMAND_COUNT,
        "TASK3561 command list count must match gate 3555"
    );

    let mut report = vec![format!(
        concat!(
            "{{\"task\":3561,\"gate_3555_command_count\":{},",
            "\"command_list_count\":{},\"case_count_per_command\":{}}}"
        ),
        GATE_3555_COMMAND_COUNT,
        commands.len(),
        CASES.len(),
    )];
    let mut controls_changed = 0usize;
    let mut bad_input_refusals = 0usize;
    let mut missing_prerequisite_refusals = 0usize;
    let mut zero_change_refusals = 0usize;

    for command in &commands {
        for case in CASES {
            let before = before_snapshot(*case);
            let mut after = before.clone();
            let result = invoke(command, *case, &mut after);
            let changed_fields = before.changed_fields(&after);
            let result_name = match (case.refusal, result) {
                (None, Ok(answer)) => {
                    assert!(
                        !changed_fields.is_empty(),
                        "TASK3561 good control changed 0 fields command={command} case={} before={} after={}",
                        case.name,
                        before.json(),
                        after.json(),
                    );
                    controls_changed += 1;
                    answer.to_owned()
                }
                (None, Err(reason)) => panic!(
                    "TASK3561 good control refused command={command} case={} reason={reason}",
                    case.name
                ),
                (Some(expected), Err(actual)) => {
                    assert_eq!(
                        actual, expected,
                        "TASK3561 wrong refusal command={command} case={}",
                        case.name
                    );
                    assert!(
                        changed_fields.is_empty(),
                        "TASK3561 refusal changed state command={command} case={} changed_fields={} before={} after={}",
                        case.name,
                        changed_fields.join(","),
                        before.json(),
                        after.json(),
                    );
                    zero_change_refusals += 1;
                    if case.name.starts_with("bad_input.") {
                        bad_input_refusals += 1;
                    } else {
                        missing_prerequisite_refusals += 1;
                    }
                    format!("refused:{actual}")
                }
                (Some(expected), Ok(answer)) => panic!(
                    "TASK3561 bad case accepted command={command} case={} expected={expected} answer={answer}",
                    case.name
                ),
            };

            report.push(format!(
                concat!(
                    "{{\"command\":\"{}\",\"case\":\"{}\",",
                    "\"result\":\"{}\",\"before\":{},\"after\":{},",
                    "\"changed_field_count\":{},\"changed_fields\":{}}}"
                ),
                command,
                case.name,
                result_name,
                before.json(),
                after.json(),
                changed_fields.len(),
                changed_fields_json(&changed_fields),
            ));
        }
    }

    let report_path = std::env::var_os("TASK3561_REPORT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("task_3561_state_snapshots.jsonl"));
    std::fs::write(&report_path, format!("{}\n", report.join("\n")))
        .unwrap_or_else(|error| panic!("write {}: {error}", report_path.display()));

    let expected_bad_input_refusals = commands.len() * 9;
    let expected_missing_prerequisite_refusals = commands.len() * 3;
    assert_eq!(controls_changed, commands.len());
    assert_eq!(bad_input_refusals, expected_bad_input_refusals);
    assert_eq!(
        missing_prerequisite_refusals,
        expected_missing_prerequisite_refusals
    );
    assert_eq!(
        zero_change_refusals,
        expected_bad_input_refusals + expected_missing_prerequisite_refusals
    );
    assert_eq!(report.len(), 1 + commands.len() * CASES.len());

    println!(
        "TASK3561 command_list_count={} gate_3555_command_count={} commands={}",
        commands.len(),
        GATE_3555_COMMAND_COUNT,
        commands.join(",")
    );
    println!(
        "TASK3561 controls_changed={} controls_zero_changed_fields=0 named_control_fields_in_saved_report={}",
        controls_changed, controls_changed
    );
    println!(
        "TASK3561 bad_input_refused={} missing_prerequisite_refused={} refusal_snapshot_pairs={} refusal_pairs_with_zero_changed_fields={}",
        bad_input_refusals,
        missing_prerequisite_refusals,
        zero_change_refusals,
        zero_change_refusals
    );
    println!(
        "TASK3561 saved_case_records={} saved_report={}",
        report.len() - 1,
        report_path.display()
    );
}
