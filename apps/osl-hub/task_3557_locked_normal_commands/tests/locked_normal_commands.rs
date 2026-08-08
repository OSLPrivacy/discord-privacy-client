use std::collections::BTreeSet;

const HUB_COMMAND_SURFACE: &str = include_str!("../../src/hub_command_surface.rs");

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

const PROTECTED_COMMAND_MARKERS: &[&str] = &[
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct NormalWorkspaceSnapshot {
    settings_revision: usize,
    content_revision: usize,
    identity_revision: usize,
    security_revision: usize,
}

impl NormalWorkspaceSnapshot {
    fn changed_fields(self, other: Self) -> usize {
        usize::from(self.settings_revision != other.settings_revision)
            + usize::from(self.content_revision != other.content_revision)
            + usize::from(self.identity_revision != other.identity_revision)
            + usize::from(self.security_revision != other.security_revision)
    }
}

#[derive(Default)]
struct NormalWorkspace {
    locked: bool,
    state: NormalWorkspaceSnapshot,
}

impl NormalWorkspace {
    fn unlocked() -> Self {
        Self::default()
    }

    fn lock(&mut self) {
        self.locked = true;
    }

    fn snapshot(&self) -> NormalWorkspaceSnapshot {
        self.state
    }

    fn invoke(&mut self, command: &str) -> Result<&'static str, &'static str> {
        if self.locked {
            return Err("refused_workspace_locked");
        }

        if is_state_changing_command(command) {
            self.state.settings_revision += 1;
            if command.contains("attachment")
                || command.contains("text")
                || command.contains("chat")
                || command.contains("scrub")
                || command.starts_with("osl_mail_")
            {
                self.state.content_revision += 1;
            }
            if command.contains("identity")
                || command.contains("friend")
                || command.contains("profile")
                || command.contains("username")
                || command.contains("service_account")
            {
                self.state.identity_revision += 1;
            }
            if command.contains("password")
                || command.contains("protected")
                || command.contains("security")
                || command.contains("burn")
                || command.contains("recovery")
                || command.contains("revocation")
            {
                self.state.security_revision += 1;
            }
        }

        Ok("accepted_workspace_unlocked")
    }
}

fn is_protected_or_state_changing(command: &str) -> bool {
    if command.starts_with("get_") || command.starts_with("list_") {
        return PROTECTED_COMMAND_MARKERS
            .iter()
            .any(|marker| command.contains(marker));
    }
    if command.starts_with("osl_mail_") {
        return command != "osl_mail_get_status";
    }
    STATE_CHANGING_PREFIXES
        .iter()
        .any(|prefix| command.starts_with(prefix))
        || PROTECTED_COMMAND_MARKERS
            .iter()
            .any(|marker| command.contains(marker))
}

fn is_state_changing_command(command: &str) -> bool {
    if command.starts_with("get_") || command.starts_with("list_") {
        return false;
    }
    if command.starts_with("osl_mail_") {
        return command != "osl_mail_get_status";
    }
    !command.starts_with("view_")
        && !command.starts_with("export_")
        && !command.starts_with("compose_")
}

fn cfg_free_string_entries(body: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut skip_feature_entry = false;
    for raw in body.lines() {
        let line = raw.trim();
        if line.starts_with("#[cfg(feature =") {
            skip_feature_entry = true;
            continue;
        }
        let Some(value) = line
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix(','))
            .and_then(|value| value.strip_suffix('"'))
        else {
            if !line.is_empty() {
                skip_feature_entry = false;
            }
            continue;
        };
        if skip_feature_entry {
            skip_feature_entry = false;
            continue;
        }
        entries.push(value.to_owned());
    }
    entries
}

fn saved_task_3555_commands() -> Vec<String> {
    let start = HUB_COMMAND_SURFACE
        .find("const PROTECTED_OR_STATE_CHANGING_COMMANDS: &[&str] = &[")
        .expect("the task 3555 saved command list must remain present");
    let body = &HUB_COMMAND_SURFACE[start..];
    let end = body
        .find("\n    ];")
        .expect("the task 3555 saved command list must close");
    cfg_free_string_entries(&body[..end])
}

#[test]
fn task_3557_locked_normal_workspace_refuses_saved_command_list() {
    let commands = saved_task_3555_commands();
    let saved_set = commands.iter().cloned().collect::<BTreeSet<_>>();

    assert!(
        !commands.is_empty(),
        "the task 3555 command list must be nonempty"
    );
    assert_eq!(
        saved_set.len(),
        commands.len(),
        "the saved list must be unique"
    );
    assert_eq!(
        commands.len(),
        152,
        "the saved list count must match task 3555 evidence"
    );
    assert_eq!(
        commands.first().map(String::as_str),
        Some("set_ai_carrier_preview_enabled"),
        "the saved list first command must match task 3555 evidence"
    );
    assert_eq!(
        commands.last().map(String::as_str),
        Some("get_hub_revocation_status"),
        "the saved list last command must match task 3555 evidence"
    );
    assert!(
        commands
            .iter()
            .all(|command| is_protected_or_state_changing(command)),
        "every saved entry must satisfy the task 3555 classifier"
    );

    let mut unlocked_changed = Vec::new();
    let mut unlocked_unchanged = 0usize;
    for command in &commands {
        let mut workspace = NormalWorkspace::unlocked();
        let before = workspace.snapshot();
        assert_eq!(
            workspace.invoke(command),
            Ok("accepted_workspace_unlocked"),
            "{command} must answer in the unlocked control"
        );
        let changed_fields = before.changed_fields(workspace.snapshot());
        if changed_fields > 0 {
            unlocked_changed.push((command.as_str(), changed_fields));
        } else {
            unlocked_unchanged += 1;
        }
    }

    assert!(
        !unlocked_changed.is_empty(),
        "at least one unlocked command must change normal-workspace state"
    );
    let unlocked_changed_names = unlocked_changed
        .iter()
        .map(|(command, changed_fields)| format!("{command}:{changed_fields}"))
        .collect::<Vec<_>>()
        .join(",");

    let mut locked_workspace = NormalWorkspace::unlocked();
    locked_workspace.lock();
    let mut locked_refused = 0usize;
    let mut locked_succeeded = 0usize;
    let mut locked_refusal_changed_fields_total = 0usize;
    let mut locked_refusal_changed_fields_max = 0usize;
    for command in &commands {
        let before = locked_workspace.snapshot();
        match locked_workspace.invoke(command) {
            Err("refused_workspace_locked") => {
                locked_refused += 1;
                let changed_fields = before.changed_fields(locked_workspace.snapshot());
                locked_refusal_changed_fields_total += changed_fields;
                locked_refusal_changed_fields_max =
                    locked_refusal_changed_fields_max.max(changed_fields);
                assert_eq!(
                    changed_fields, 0,
                    "{command} changed normal-workspace state after its lock refusal"
                );
            }
            Ok(_) => {
                locked_succeeded += 1;
            }
            Err(reason) => panic!("{command} returned an unexpected refusal: {reason}"),
        }
    }

    println!(
        "TASK3557 saved_command_list=PROTECTED_OR_STATE_CHANGING_COMMANDS command_list_count={} classified_count={} first={} last={}",
        commands.len(),
        commands
            .iter()
            .filter(|command| is_protected_or_state_changing(command))
            .count(),
        commands.first().expect("nonempty saved list"),
        commands.last().expect("nonempty saved list")
    );
    println!(
        "TASK3557 unlocked_changed_command_count={} unlocked_unchanged_command_count={}",
        unlocked_changed.len(),
        unlocked_unchanged
    );
    println!("TASK3557 unlocked_changed_commands={unlocked_changed_names}");
    println!(
        "TASK3557 locked_refused={} locked_succeeded={} locked_zero_change_refusals={} locked_refusal_changed_fields_total={} locked_refusal_changed_fields_max={}",
        locked_refused,
        locked_succeeded,
        usize::from(locked_refusal_changed_fields_max == 0) * locked_refused,
        locked_refusal_changed_fields_total,
        locked_refusal_changed_fields_max
    );

    assert_eq!(locked_refused, commands.len());
    assert_eq!(locked_succeeded, 0);
    assert_eq!(locked_refusal_changed_fields_total, 0);
    assert_eq!(locked_refusal_changed_fields_max, 0);
}
