use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const DEFAULT_TODO_DIR: &str = "/home/liamw/osl-plan/OSL-AUDITS/todo";

#[derive(Debug, Clone)]
struct TaskRecord {
    number: u16,
    title: String,
    body: String,
}

#[derive(Debug)]
struct CoverageReport {
    task_count: usize,
    used: BTreeMap<&'static str, BTreeSet<String>>,
    allowed: BTreeMap<&'static str, BTreeSet<String>>,
    missing: BTreeMap<&'static str, BTreeSet<String>>,
    missing_examples: BTreeMap<(&'static str, String), BTreeSet<String>>,
}

#[test]
fn task_3746_later_task_place_kinds_are_all_on_allowed_lists() {
    let report = build_report(None).expect("build task 3746 report");
    print_report(&report);
    assert!(
        report.missing.is_empty(),
        "TASK3746 missing place kinds: {:?}",
        report.missing_examples
    );
    println!("TASK3746_MISSING_PLACE_KIND_COUNT=0");
}

#[test]
fn task_3746_adding_invented_kind_to_task_exits_1() {
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("task_3746_invented_kind_probe")
        .arg("--nocapture")
        .env("TASK3746_INVENTED_KIND_PROBE", "1")
        .output()
        .expect("run task 3746 invented-kind probe");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout.contains("discord:moon_base") || stderr.contains("discord:moon_base"),
        "invented-kind probe did not name discord:moon_base\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    println!("TASK3746_INVENTED_KIND_EXIT_CODE=1 invented=discord:moon_base");
}

#[test]
fn task_3746_removed_allowed_kind_probe_exits_1() {
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("task_3746_removed_allowed_kind_probe")
        .arg("--nocapture")
        .env("TASK3746_REMOVED_ALLOWED_KIND_PROBE", "1")
        .env("TASK3746_REMOVED_ALLOWED_KIND", "messenger:community")
        .output()
        .expect("run task 3746 removed-allowed-kind probe");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout.contains("app=messenger kind=community") || stderr.contains("messenger:community"),
        "removed-kind probe did not name messenger:community\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    println!("TASK3746_REMOVED_ALLOWED_KIND_EXIT_CODE=1 removed=messenger:community");
}

#[test]
#[ignore = "task 3746 helper: exits 1 only when an invented task kind is missing"]
fn task_3746_invented_kind_probe() {
    if std::env::var_os("TASK3746_INVENTED_KIND_PROBE").is_none() {
        return;
    }
    let injected_task = "\nTASK 0901 - invented Discord place kind probe\n\
gates: 0900\n\
build: test\n\
who: agent\n\
run by: codex build\n\
do: Directly inspect an allowed Discord moon base.\n\
done when: it returns moon-base kind and OSL controls.\n";
    let report = build_report(Some(injected_task)).expect("build task 3746 injected report");
    print_report(&report);
    if report.missing.is_empty() {
        eprintln!("TASK3746_INVENTED_KIND_UNEXPECTEDLY_ALLOWED=discord:moon_base");
        std::process::exit(0);
    }
    eprintln!("TASK3746_INVENTED_KIND_REFUSED=discord:moon_base");
    std::process::exit(1);
}

#[test]
#[ignore = "task 3746 helper: exits 1 only when an allowed kind is removed"]
fn task_3746_removed_allowed_kind_probe() {
    if std::env::var_os("TASK3746_REMOVED_ALLOWED_KIND_PROBE").is_none() {
        return;
    }
    let report = build_report(None).expect("build task 3746 removed-kind report");
    print_report(&report);
    if report.missing.is_empty() {
        eprintln!("TASK3746_REMOVED_ALLOWED_KIND_UNEXPECTEDLY_ALLOWED=messenger:community");
        std::process::exit(0);
    }
    eprintln!("TASK3746_REMOVED_ALLOWED_KIND_REFUSED=messenger:community");
    std::process::exit(1);
}

fn build_report(extra_task_text: Option<&str>) -> Result<CoverageReport, String> {
    let tasks = read_tasks_0900_to_1399(extra_task_text)?;
    let mut allowed = allowed_place_kinds();
    remove_allowed_kind_from_env(&mut allowed);
    let mut used: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
    let mut examples: BTreeMap<(&'static str, String), BTreeSet<String>> = BTreeMap::new();

    for task in &tasks {
        let Some(app) = app_for_task(task.number) else {
            continue;
        };
        if task_is_research_only(task) {
            continue;
        }
        let kinds = kinds_named_by_task(app, task);
        for kind in kinds {
            used.entry(app).or_default().insert(kind.clone());
            examples
                .entry((app, kind))
                .or_default()
                .insert(format!("{:04} {}", task.number, task.title));
        }
    }

    let mut missing = BTreeMap::new();
    let mut missing_examples = BTreeMap::new();
    for (app, used_kinds) in &used {
        let allowed_kinds = allowed.get(app).cloned().unwrap_or_default();
        let app_missing: BTreeSet<String> =
            used_kinds.difference(&allowed_kinds).cloned().collect();
        if !app_missing.is_empty() {
            for kind in &app_missing {
                let key = (*app, kind.clone());
                if let Some(task_examples) = examples.get(&key) {
                    missing_examples.insert(key, task_examples.clone());
                }
            }
            missing.insert(*app, app_missing);
        }
    }

    Ok(CoverageReport {
        task_count: tasks.len(),
        used,
        allowed,
        missing,
        missing_examples,
    })
}

fn remove_allowed_kind_from_env(allowed: &mut BTreeMap<&'static str, BTreeSet<String>>) {
    let Ok(value) = std::env::var("TASK3746_REMOVED_ALLOWED_KIND") else {
        return;
    };
    let Some((app, kind)) = value.split_once(':') else {
        return;
    };
    if let Some((known_app, kinds)) = allowed.iter_mut().find(|(known_app, _)| **known_app == app) {
        kinds.remove(kind);
        println!("TASK3746_REMOVED_ALLOWED_KIND app={known_app} kind={kind}");
    }
}

fn read_tasks_0900_to_1399(extra_task_text: Option<&str>) -> Result<Vec<TaskRecord>, String> {
    let todo_dir = std::env::var("OSL_AUDIT_TODO_DIR").unwrap_or_else(|_| DEFAULT_TODO_DIR.into());
    let mut files = todo_files(Path::new(&todo_dir))?;
    let mut tasks = Vec::new();
    for file in files.drain(..) {
        let text = std::fs::read_to_string(&file)
            .map_err(|error| format!("read {}: {error}", file.display()))?;
        parse_tasks_from_text(&text, &mut tasks);
    }
    if let Some(extra) = extra_task_text {
        parse_tasks_from_text(extra, &mut tasks);
    }
    tasks.sort_by_key(|task| (task.number, task.title.clone()));
    Ok(tasks
        .into_iter()
        .filter(|task| (900..=1399).contains(&task.number))
        .collect())
}

fn todo_files(todo_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let entries = std::fs::read_dir(todo_dir)
        .map_err(|error| format!("read task todo dir {}: {error}", todo_dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("read task todo dir entry: {error}"))?
            .path();
        if path.extension().is_some_and(|ext| ext == "txt") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn parse_tasks_from_text(text: &str, tasks: &mut Vec<TaskRecord>) {
    let mut current: Option<TaskRecord> = None;
    for line in text.lines() {
        if let Some((number, title)) = parse_task_heading(line) {
            if let Some(task) = current.take() {
                tasks.push(task);
            }
            current = Some(TaskRecord {
                number,
                title,
                body: String::new(),
            });
        } else if let Some(task) = current.as_mut() {
            task.body.push_str(line);
            task.body.push('\n');
        }
    }
    if let Some(task) = current {
        tasks.push(task);
    }
}

fn parse_task_heading(line: &str) -> Option<(u16, String)> {
    let rest = line.strip_prefix("TASK ")?;
    let number_text = rest.get(0..4)?;
    if !number_text.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let number = number_text.parse().ok()?;
    let title = rest
        .split_once(" - ")
        .map(|(_, title)| title.to_string())
        .unwrap_or_else(|| rest.to_string());
    Some((number, title))
}

fn app_for_task(number: u16) -> Option<&'static str> {
    match number {
        900..=999 => Some("discord"),
        1000..=1029 => Some("telegram"),
        1030..=1059 => Some("signal"),
        1060..=1089 => Some("whatsapp"),
        1100..=1129 => Some("x"),
        1130..=1164 => Some("instagram"),
        1165..=1199 => Some("messenger"),
        1200..=1299 => Some("email"),
        _ => None,
    }
}

fn task_is_research_only(task: &TaskRecord) -> bool {
    let title = task.title.to_ascii_lowercase();
    let body = task.body.to_ascii_lowercase();
    title.contains("find out ")
        || title.contains("has no stories")
        || (body.contains("research") && body.contains("without posting"))
}

fn allowed_place_kinds() -> BTreeMap<&'static str, BTreeSet<String>> {
    let mut allowed = BTreeMap::new();
    allowed.insert(
        "discord",
        ipc::auto_whitelist_rules::DiscordWhitelistKind::ALL
            .into_iter()
            .map(|kind| kind.id().to_string())
            .collect(),
    );
    allowed.insert(
        "telegram",
        ipc::auto_whitelist_rules::TelegramWhitelistKind::ALL
            .into_iter()
            .map(|kind| kind.id().to_string())
            .collect(),
    );
    allowed.insert(
        "signal",
        ipc::auto_whitelist_rules::SignalWhitelistKind::ALL
            .into_iter()
            .map(|kind| kind.id().to_string())
            .collect(),
    );
    allowed.insert(
        "whatsapp",
        ipc::auto_whitelist_rules::WhatsAppWhitelistKind::ALL
            .into_iter()
            .map(|kind| kind.id().to_string())
            .collect(),
    );
    allowed.insert(
        "x",
        ipc::auto_whitelist_rules::X_PLACE_KINDS
            .into_iter()
            .map(str::to_string)
            .collect(),
    );
    allowed.insert(
        "instagram",
        ipc::auto_whitelist_rules::InstagramWhitelistKind::ALL
            .into_iter()
            .map(|kind| kind.id().to_string())
            .collect(),
    );
    allowed.insert(
        "messenger",
        ipc::auto_whitelist_rules::MessengerWhitelistKind::ALL
            .into_iter()
            .map(|kind| kind.id().to_string())
            .collect(),
    );
    allowed.insert(
        "email",
        ["email_address", "email_domain"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    );
    allowed
}

fn kinds_named_by_task(app: &'static str, task: &TaskRecord) -> BTreeSet<String> {
    let mut text = String::new();
    text.push_str(&task.title);
    text.push('\n');
    text.push_str(&task.body);
    let tokens = tokenize(&text);
    let mut kinds = known_kind_mentions(app, &tokens);
    kinds.extend(generic_kind_mentions(app, &tokens));
    kinds
}

fn known_kind_mentions(app: &'static str, tokens: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (phrases, kind) in phrase_table(app) {
        if phrases.iter().any(|phrase| has_phrase(tokens, phrase)) {
            found.insert(kind.to_string());
        }
    }
    found
}

fn generic_kind_mentions(app: &'static str, tokens: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for index in 0..tokens.len() {
        if tokens[index] != "kind" && tokens[index] != "kinds" {
            continue;
        }
        let mut parts = Vec::new();
        for token in tokens[..index].iter().rev() {
            if parts.len() == 3 || generic_stop_word(app, token) {
                break;
            }
            parts.push(token.as_str());
        }
        parts.reverse();
        if parts.is_empty() {
            continue;
        }
        let candidate = parts.join("_");
        if let Some(kind) = canonical_kind(app, &candidate) {
            found.insert(kind);
        }
    }
    found
}

fn phrase_table(app: &'static str) -> Vec<(Vec<Vec<&'static str>>, &'static str)> {
    match app {
        "discord" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (phrases(&[&["group", "chat"], &["groups"]]), "group_chat"),
            (phrases(&[&["server"]]), "server"),
            (
                phrases(&[&["server", "channel"], &["server", "channels"]]),
                "server_channel",
            ),
            (phrases(&[&["thread"], &["threads"]]), "thread"),
        ],
        "telegram" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (phrases(&[&["group"], &["group", "chat"]]), "group_chat"),
            (phrases(&[&["channel"]]), "channel"),
            (phrases(&[&["public", "post"]]), "public_post"),
            (phrases(&[&["supergroup"]]), "supergroup"),
            (
                phrases(&[&["saved", "message"], &["saved", "messages"]]),
                "saved_messages",
            ),
        ],
        "signal" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (phrases(&[&["group", "chat"], &["group"]]), "group_chat"),
            (phrases(&[&["story"], &["stories"]]), "story"),
        ],
        "whatsapp" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (phrases(&[&["group"], &["group", "chat"]]), "group_chat"),
            (phrases(&[&["channel"]]), "channel"),
            (phrases(&[&["community"]]), "community"),
            (phrases(&[&["community", "group"]]), "community_group"),
            (phrases(&[&["broadcast", "list"]]), "broadcast_list"),
        ],
        "x" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (
                phrases(&[
                    &["group", "direct", "message"],
                    &["group", "direct", "messages"],
                ]),
                "group_direct_message",
            ),
            (
                phrases(&[&["public", "post"], &["public", "posts"], &["own", "posts"]]),
                "public_post",
            ),
            (phrases(&[&["reply"], &["replies"]]), "reply"),
        ],
        "instagram" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (
                phrases(&[&["group", "chat"], &["group", "direct", "messages"]]),
                "group_chat",
            ),
            (
                phrases(&[&["public", "post"], &["public", "posts"], &["own", "posts"]]),
                "public_post",
            ),
            (phrases(&[&["comment"], &["comments"]]), "comment"),
            (phrases(&[&["story"], &["stories"]]), "story"),
        ],
        "messenger" => vec![
            (
                phrases(&[&["direct", "message"], &["direct", "messages"]]),
                "direct_message",
            ),
            (
                phrases(&[&["group", "chat"], &["group", "chats"]]),
                "group_chat",
            ),
            (phrases(&[&["community"], &["communities"]]), "community"),
        ],
        "email" => vec![
            (
                phrases(&[&["email", "address"], &["email", "addresses"]]),
                "email_address",
            ),
            (
                phrases(&[&["email", "domain"], &["email", "domains"]]),
                "email_domain",
            ),
        ],
        _ => Vec::new(),
    }
}

fn phrases(input: &[&[&'static str]]) -> Vec<Vec<&'static str>> {
    input.iter().map(|phrase| phrase.to_vec()).collect()
}

fn canonical_kind(app: &'static str, candidate: &str) -> Option<String> {
    let kind = match (app, candidate) {
        (_, "direct_message" | "direct_messages" | "direct_message_place") => "direct_message",
        (_, "group_chat" | "group_chats") => "group_chat",
        ("telegram", "group") | ("whatsapp", "group") => "group_chat",
        (_, "public_post" | "public_posts") => "public_post",
        ("telegram", "saved_message" | "saved_messages") => "saved_messages",
        ("x", "group_direct_message" | "group_direct_messages") => "group_direct_message",
        ("whatsapp", "community_group") => "community_group",
        ("whatsapp", "broadcast_list") => "broadcast_list",
        (_, "server" | "server_channel" | "thread" | "channel") => candidate,
        (_, "supergroup" | "story" | "comment" | "community" | "reply") => candidate,
        (_, other) if other.ends_with("_place") => other.trim_end_matches("_place"),
        (_, other) => other,
    };
    Some(kind.to_string())
}

fn generic_stop_word(app: &'static str, token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "as"
            | "both"
            | "correct"
            | "different"
            | "each"
            | "exactly"
            | "for"
            | "its"
            | "no"
            | "one"
            | "only"
            | "own"
            | "place"
            | "places"
            | "prepared"
            | "returns"
            | "return"
            | "returned"
            | "right"
            | "same"
            | "the"
            | "their"
            | "with"
            | "without"
            | "x"
    ) || token == app
}

fn tokenize(text: &str) -> Vec<String> {
    let mut normalized = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else {
            normalized.push(' ');
        }
    }
    normalized.split_whitespace().map(str::to_string).collect()
}

fn has_phrase(tokens: &[String], phrase: &[&str]) -> bool {
    if phrase.is_empty() || phrase.len() > tokens.len() {
        return false;
    }
    tokens
        .windows(phrase.len())
        .any(|window| window.iter().map(String::as_str).eq(phrase.iter().copied()))
}

fn print_report(report: &CoverageReport) {
    println!(
        "TASK3746_PLACE_KIND_COVERAGE task_range=0900..1399 tasks_read={}",
        report.task_count
    );
    for app in [
        "discord",
        "telegram",
        "signal",
        "whatsapp",
        "x",
        "instagram",
        "messenger",
        "email",
    ] {
        let used = report.used.get(app).cloned().unwrap_or_default();
        let allowed = report.allowed.get(app).cloned().unwrap_or_default();
        println!(
            "TASK3746_APP app={app} used_count={} used={} allowed_count={} allowed={}",
            used.len(),
            join_set(&used),
            allowed.len(),
            join_set(&allowed)
        );
    }
    let missing_count: usize = report.missing.values().map(BTreeSet::len).sum();
    println!("TASK3746_MISSING_PLACE_KIND_COUNT={missing_count}");
    for ((app, kind), tasks) in &report.missing_examples {
        println!(
            "TASK3746_MISSING app={app} kind={kind} tasks={}",
            join_set(tasks)
        );
    }
}

fn join_set(values: &BTreeSet<String>) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.iter().cloned().collect::<Vec<_>>().join(",")
    }
}
