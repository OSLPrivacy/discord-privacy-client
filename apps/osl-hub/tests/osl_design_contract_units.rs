use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GUI_PLAN: &str = include_str!("../../../docs/design/osl-gui-final-plan.md");
const SIMPLE_SPEC: &str = include_str!("../../../docs/design/osl-simple-spec.md");
const BRANCH_PROTECTION: &str = include_str!("../../../.github/BRANCH_PROTECTION.md");

fn section<'a>(markdown: &'a str, heading: &str) -> &'a str {
    let marker = format!("\n{heading}\n");
    let body = markdown
        .split_once(&marker)
        .unwrap_or_else(|| panic!("{heading} section is missing"))
        .1;
    let level = heading.chars().take_while(|ch| *ch == '#').count();
    let mut end = body.len();
    for candidate in 1..=level {
        if let Some(index) = body.find(&format!("\n{} ", "#".repeat(candidate))) {
            end = end.min(index);
        }
    }
    &body[..end]
}

fn table_rows(section: &str, expected_header: &str) -> Vec<BTreeMap<String, String>> {
    let table_lines: Vec<&str> = section
        .lines()
        .filter(|line| line.starts_with('|'))
        .collect();
    let header_index = table_lines
        .iter()
        .position(|line| line.split('|').any(|cell| cell.trim() == expected_header))
        .unwrap_or_else(|| panic!("{expected_header} table is missing"));
    let headers: Vec<String> = table_lines[header_index]
        .trim_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect();
    let mut rows = Vec::new();
    for line in table_lines.iter().skip(header_index + 2) {
        let cells: Vec<String> = line
            .trim_matches('|')
            .split('|')
            .map(|cell| cell.trim().to_string())
            .collect();
        if cells.len() != headers.len() {
            break;
        }
        rows.push(headers.iter().cloned().zip(cells).collect());
    }
    rows
}

fn words(source: &str) -> BTreeSet<String> {
    source
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn sentences(markdown: &str) -> Vec<String> {
    let normalized = markdown.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized
        .split_inclusive(['.', '!', '?'])
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn backtick_values(sentence: &str) -> Vec<String> {
    sentence
        .split('`')
        .skip(1)
        .step_by(2)
        .map(ToString::to_string)
        .collect()
}

fn information_architecture_errors(markdown: &str) -> Vec<String> {
    let architecture = section(markdown, "## Information architecture");
    let rows = table_rows(architecture, "Destination");
    let destinations: Vec<String> = rows
        .iter()
        .map(|row| row["Destination"].trim_matches('*').to_string())
        .collect();
    let expected = [
        "Home",
        "Inbox",
        "People",
        "Privacy",
        "Activity",
        "Connections",
    ];
    let mut errors = Vec::new();
    if destinations != expected {
        errors.push("primary destinations are not exactly the fixed six".to_string());
    }
    if destinations
        .iter()
        .any(|destination| destination == "Settings")
    {
        errors.push("Settings is a competing destination".to_string());
    }
    if rows.iter().any(|row| {
        ["User's question", "Main content", "Primary action"]
            .iter()
            .any(|column| row[*column].trim().is_empty())
    }) {
        errors.push("destination rows must encode question, content, and action".to_string());
    }
    let settings_is_outside_six = sentences(architecture).iter().any(|sentence| {
        sentence.starts_with("Settings ")
            && ["fixed", "bottom", "seventh"]
                .iter()
                .all(|word| words(sentence).contains(*word))
    });
    if !settings_is_outside_six {
        errors.push("Settings is not fixed at the bottom outside the six".to_string());
    }
    errors
}

fn ordered_double_enter_transitions(markdown: &str) -> Vec<String> {
    section(markdown, "### Double Enter state machine")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("-> "))
        .map(|transition| transition.to_ascii_lowercase())
        .collect()
}

fn send_contract_errors(simple_markdown: &str, gui_markdown: &str) -> Vec<String> {
    let sending = section(simple_markdown, "## Sending");
    let sending_sentences = sentences(sending);
    let mut errors = Vec::new();

    let outcome_sentence = sending_sentences
        .iter()
        .find(|sentence| words(sentence).contains("outcomes"));
    if let Some(sentence) = outcome_sentence {
        let outcome_words = words(sentence);
        for required in [
            ["sent"].as_slice(),
            ["not", "sent"].as_slice(),
            ["delivery", "uncertain"].as_slice(),
        ] {
            if !required.iter().all(|word| outcome_words.contains(*word)) {
                errors.push("sending outcomes are not the honest tri-state".to_string());
                break;
            }
        }
    } else {
        errors.push("sending outcomes are not the honest tri-state".to_string());
    }

    // The refusal sentence, not the outcome sentence. "## Sending" opens with
    // "Sending has three honest outcomes: sent, not sent, or delivery
    // uncertain.", which also contains "uncertain" -- matching on that word
    // alone reads the enumeration and reports every refusal below as missing
    // even when the document states all three. The refusals are the sentence
    // that says what OSL will never do with an uncertain outcome, so require
    // both words and let the enumeration fall through to its own check above.
    let uncertainty_sentence = sending_sentences.iter().find(|sentence| {
        let sentence_words = words(sentence);
        sentence_words.contains("uncertain") && sentence_words.contains("never")
    });
    let uncertainty_words = uncertainty_sentence
        .map(|sentence| words(sentence))
        .unwrap_or_default();
    for (required, message) in [
        (
            ["never", "treats", "delivery", "uncertain", "as", "sent"].as_slice(),
            "missing sending refusal: never treats delivery uncertain as sent",
        ),
        (
            ["never", "auto", "retries"].as_slice(),
            "missing sending refusal: never auto-retries",
        ),
        (
            ["never", "asks", "user", "resend", "certainly", "failed"].as_slice(),
            "missing sending refusal: never asks the user to resend as if the first attempt certainly failed",
        ),
    ] {
        if !required.iter().all(|word| uncertainty_words.contains(*word)) {
            errors.push(message.to_string());
        }
    }

    let simple_words = words(sending);
    for (required, message) in [
        (
            ["first", "distinct", "user", "enter"].as_slice(),
            "missing simple Double Enter rule: first distinct user enter",
        ),
        (
            ["second", "separate", "user", "enter", "key", "up"].as_slice(),
            "missing simple Double Enter rule: second separate user enter after key-up",
        ),
        (
            ["key", "repeat"].as_slice(),
            "missing simple Double Enter rule: key repeat",
        ),
        (
            ["synthetic", "event"].as_slice(),
            "missing simple Double Enter rule: synthetic event",
        ),
        (
            ["preserves", "local", "draft"].as_slice(),
            "missing simple Double Enter rule: preserves the local draft",
        ),
        (
            ["never", "retries", "automatically"].as_slice(),
            "missing simple Double Enter rule: never retries automatically",
        ),
    ] {
        if !required.iter().all(|word| simple_words.contains(*word)) {
            errors.push(message.to_string());
        }
    }

    let transitions = ordered_double_enter_transitions(gui_markdown);
    let required_transitions = [
        "first user enter",
        "verify focus + service + account + conversation + recipients + mode",
        "encrypt and place capsule",
        "awaiting second user enter",
        "reverify the same context",
        "second distinct user enter passes through to native send",
        "verify outcome, or report unknown",
    ];
    let mut position = 0;
    for required in required_transitions {
        let Some(found) = transitions[position..]
            .iter()
            .position(|transition| transition == required)
        else {
            errors.push(format!("missing Double Enter transition: {required}"));
            continue;
        };
        position += found + 1;
    }

    let double_enter_words = words(section(gui_markdown, "### Double Enter state machine"));
    for (required, message) in [
        (
            ["first", "enter", "consumed"].as_slice(),
            "missing GUI Double Enter refusal: first enter is consumed",
        ),
        (
            ["cannot", "send", "plaintext"].as_slice(),
            "missing GUI Double Enter refusal: cannot send the plaintext",
        ),
        (
            ["second", "enter", "separate", "trusted", "user", "key", "press", "after", "key", "up"].as_slice(),
            "missing GUI Double Enter refusal: second enter must be a separate, trusted user key press after key-up",
        ),
        (
            ["key", "repeat"].as_slice(),
            "missing GUI Double Enter refusal: key repeat",
        ),
        (
            ["synthetic", "event"].as_slice(),
            "missing GUI Double Enter refusal: synthetic event",
        ),
        (
            ["expiry", "safe", "draft", "state", "does", "not", "send"].as_slice(),
            "missing GUI Double Enter refusal: expiry returns to a safe draft state; it does not send",
        ),
        (
            ["mismatch", "cancels"].as_slice(),
            "missing GUI Double Enter refusal: mismatch cancels",
        ),
        (
            ["crash", "recovery", "restore", "draft", "never", "armed", "send", "state"].as_slice(),
            "missing GUI Double Enter refusal: crash recovery may restore the draft, never the armed-to-send state",
        ),
        (
            ["never", "retries", "automatically"].as_slice(),
            "missing GUI Double Enter refusal: never retries automatically",
        ),
    ] {
        if !required.iter().all(|word| double_enter_words.contains(*word)) {
            errors.push(message.to_string());
        }
    }

    errors
}

fn browser_and_monetization_errors(markdown: &str) -> Vec<String> {
    let connections = section(markdown, "## Connections");
    let connection_sentences = sentences(connections);
    let mut errors = Vec::new();

    let receipt_sentence = connection_sentences
        .iter()
        .find(|sentence| words(sentence).contains("receipt"));
    let receipt_choices = receipt_sentence
        .map(|sentence| backtick_values(sentence))
        .unwrap_or_default();
    if receipt_choices != ["Browser account", "New account"] {
        errors.push("browser import receipt path must expose exactly two choices".to_string());
    }

    // The import-receipt sentence, not merely the first sentence in
    // "## Connections" that happens to use the word "without" -- the
    // self-healing paragraph ("...disable one broken capability without
    // failing the whole service.") sits earlier in the same section and would
    // otherwise be read as the browser-import rule, reporting the fixed-origin
    // fallback as missing while the document states it. Requiring "receipt"
    // too pins the one sentence that owns both halves of this contract: the
    // two choices with a receipt, and the fixed official origin without one.
    let without_receipt = connection_sentences
        .iter()
        .find(|sentence| {
            let sentence_words = words(sentence);
            sentence_words.contains("without") && sentence_words.contains("receipt")
        })
        .map(|sentence| words(sentence))
        .unwrap_or_default();
    if !["fixed", "official", "sign", "in", "origin", "directly"]
        .iter()
        .all(|word| without_receipt.contains(*word))
    {
        errors.push("missing direct fixed-origin behavior without import receipt".to_string());
    }

    let new_account = connection_sentences
        .iter()
        .find(|sentence| sentence.trim_start().starts_with("`New account`"))
        .map(|sentence| words(sentence))
        .unwrap_or_default();
    if !["fixed", "owner", "scoped", "osl", "browser", "profile"]
        .iter()
        .all(|word| new_account.contains(*word))
    {
        errors.push("New account is not owner-scoped to an OSL browser profile".to_string());
    }

    let renderer_refusal = connection_sentences
        .iter()
        .find(|sentence| words(sentence).contains("renderer"))
        .map(|sentence| words(sentence))
        .unwrap_or_default();
    if ![
        "never",
        "accepts",
        "renderer",
        "provided",
        "executable",
        "url",
        "profile",
        "path",
        "browser",
        "argument",
    ]
    .iter()
    .all(|word| renderer_refusal.contains(*word))
    {
        errors.push("renderer-provided browser launch authority is not refused".to_string());
    }

    let monetization_sentence = sentences(markdown).into_iter().find(|sentence| {
        let sentence_words = words(sentence);
        sentence_words.contains("monetization") && sentence_words.contains("labels")
    });
    let monetization_words = monetization_sentence
        .map(|sentence| words(&sentence))
        .unwrap_or_default();
    if ![
        "must",
        "never",
        "interrupt",
        "safety",
        "warning",
        "destructive",
        "confirmation",
        "honest",
        "capability",
        "refusal",
    ]
    .iter()
    .all(|word| monetization_words.contains(*word))
    {
        errors.push("monetization can interrupt safety or capability refusal".to_string());
    }

    errors
}

fn branch_protection_errors(markdown: &str) -> Vec<String> {
    let contracts: Vec<Value> = markdown
        .split("```json")
        .skip(1)
        .filter_map(|tail| tail.split_once("```").map(|(json, _)| json))
        .filter_map(|json| serde_json::from_str(json).ok())
        .collect();
    let Some(contract) = contracts
        .iter()
        .find(|contract| contract.get("required_status_checks").is_some())
    else {
        return vec!["missing machine-readable branch-protection JSON contract".to_string()];
    };

    let mut errors = Vec::new();
    let status = &contract["required_status_checks"];
    if status["strict"] != Value::Bool(true) {
        errors.push("required status checks must require up-to-date branches".to_string());
    }

    let contexts: BTreeSet<String> = status["contexts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(ToString::to_string)
        .collect();
    let rust_contexts: BTreeSet<String> = status["checks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|check| check["workflow"] == Value::String("Rust Test".to_string()))
        .filter_map(|check| check["context"].as_str())
        .map(ToString::to_string)
        .collect();
    for context in ["test", "quality-checks"] {
        if !contexts.contains(context) {
            errors.push(format!("missing required status context {context}"));
        }
        if !rust_contexts.contains(context) {
            errors.push(format!(
                "{context} must be sourced from the Rust Test workflow"
            ));
        }
    }

    if contract["enforce_admins"] != Value::Bool(true) {
        errors.push("administrators must be included in enforcement".to_string());
    }
    if contract["admin_bypass"] != Value::String("forbidden".to_string()) {
        errors.push("admin bypass must be forbidden".to_string());
    }

    errors
}

#[test]
fn encode_the_six_fixed_information_architecture_destinations() {
    assert_eq!(
        information_architecture_errors(GUI_PLAN),
        Vec::<String>::new()
    );

    let settings_as_destination = GUI_PLAN.replace(
        "| **Activity** | What did OSL actually do?",
        "| **Settings** | What did OSL actually do?",
    );
    assert!(
        information_architecture_errors(&settings_as_destination)
            .iter()
            .any(|error| error == "primary destinations are not exactly the fixed six"),
        "test must reject Settings as one of the six primary destinations"
    );

    let missing_row_action = GUI_PLAN.replace(
        "| **People** | Who do I trust and where do I know them? | Verified OSL contacts, platform identities, groups, audiences and whitelist policy | Add or verify a person |",
        "| **People** | Who do I trust and where do I know them? | Verified OSL contacts, platform identities, groups, audiences and whitelist policy | |",
    );
    assert!(
        information_architecture_errors(&missing_row_action)
            .iter()
            .any(|error| error == "destination rows must encode question, content, and action"),
        "test must reject an IA row without its user-facing action"
    );
}

#[test]
fn encode_honest_tri_state_sending_and_double_enter_without_auto_retry() {
    assert_eq!(
        send_contract_errors(SIMPLE_SPEC, GUI_PLAN),
        Vec::<String>::new()
    );

    let optimistic_outcome = SIMPLE_SPEC.replace(
        "Sending has three honest outcomes: sent, not sent, or delivery uncertain.",
        "Sending has two outcomes: sent or failed.",
    );
    assert!(
        send_contract_errors(&optimistic_outcome, GUI_PLAN)
            .iter()
            .any(|error| error == "sending outcomes are not the honest tri-state"),
        "test must reject collapsing uncertain delivery into failure"
    );

    let auto_retry = SIMPLE_SPEC.replace("never auto-retries it, ", "");
    assert!(
        send_contract_errors(&auto_retry, GUI_PLAN)
            .iter()
            .any(|error| error == "missing sending refusal: never auto-retries"),
        "test must reject an uncertain send path that can retry automatically"
    );

    let synthetic_second_enter = GUI_PLAN.replace(
        "-> Second distinct user Enter passes through to native Send",
        "-> Synthetic Enter passes through to native Send",
    );
    assert!(
        send_contract_errors(SIMPLE_SPEC, &synthetic_second_enter)
            .iter()
            .any(|error| error
                == "missing Double Enter transition: second distinct user enter passes through to native send"),
        "test must reject a Double Enter state machine that allows synthetic Send"
    );
}

#[test]
fn encode_browser_import_choices_and_noninterrupting_monetization() {
    assert_eq!(
        browser_and_monetization_errors(GUI_PLAN),
        Vec::<String>::new()
    );

    let widened_import_choices = GUI_PLAN.replace(
        "a web app shows only `Browser account` and `New account`",
        "a web app shows `Browser account`, `Existing profile` and `New account`",
    );
    assert!(
        browser_and_monetization_errors(&widened_import_choices)
            .iter()
            .any(|error| error == "browser import receipt path must expose exactly two choices"),
        "test must reject a third browser-import choice"
    );

    let receiptless_path_without_fixed_origin = GUI_PLAN.replace(
        "without one, the tile opens the fixed official sign-in origin directly",
        "without one, the tile opens whatever page the connector used last",
    );
    assert!(
        browser_and_monetization_errors(&receiptless_path_without_fixed_origin)
            .iter()
            .any(|error| error == "missing direct fixed-origin behavior without import receipt"),
        "test must reject a receipt-less browser path that does not open the fixed official sign-in origin directly"
    );

    let interrupting_paid_state = GUI_PLAN.replace("must never interrupt", "may interrupt");
    assert!(
        browser_and_monetization_errors(&interrupting_paid_state)
            .iter()
            .any(|error| error == "monetization can interrupt safety or capability refusal"),
        "test must reject paid-state copy that can interrupt safety or refusal"
    );
}

#[test]
fn define_the_branch_protection_contract_that_requires_rust_and_forbids_admin_bypass() {
    assert_eq!(
        branch_protection_errors(BRANCH_PROTECTION),
        Vec::<String>::new()
    );

    let without_rust_source = BRANCH_PROTECTION.replace(
        r#""workflow": "Rust Test""#,
        r#""workflow": "TypeScript Test""#,
    );
    assert!(
        branch_protection_errors(&without_rust_source)
            .iter()
            .any(|error| error.contains("Rust Test")),
        "test must reject status contexts that are not sourced from Rust Test"
    );

    let with_admin_bypass = BRANCH_PROTECTION.replace(
        r#""admin_bypass": "forbidden""#,
        r#""admin_bypass": "allowed""#,
    );
    assert!(
        branch_protection_errors(&with_admin_bypass)
            .iter()
            .any(|error| error == "admin bypass must be forbidden"),
        "test must reject an admin bypass permission"
    );
}

#[test]
fn block_banned_burn_and_support_phrasings() {
    let repo = repo_root();
    let fixture_path = repo.join("apps/osl-hub-ui/src/__claim_gate_burn_support_fixture.ts");
    let _fixture = TemporaryFixture::write(
        fixture_path,
        [
            "export const claimGateBurnSupportFixture = [",
            "  'Burn deletes Discord messages.',",
            "  'OSL supports WhatsApp.',",
            "].join('\\n');",
            "",
        ]
        .join("\n"),
    );

    let output = run_claim_gate(&[]);
    assert!(
        !output.status.success(),
        "claim gate must refuse banned Burn and support phrasings"
    );
    let combined = format!("{}\n{}", as_text(&output.stdout), as_text(&output.stderr));
    for required in [
        "__claim_gate_burn_support_fixture.ts",
        "Burn deletes Discord messages",
        "OSL supports WhatsApp",
    ] {
        assert!(
            combined.contains(required),
            "claim gate did not report `{required}` for the negative fixture\n{combined}"
        );
    }

    let self_test = run_claim_gate(&["--self-test"]);
    assert_success(&self_test, "app-claim self-test");
    assert!(
        as_text(&self_test.stdout).contains("PASS Block banned Burn and support phrasings"),
        "claim-gate self-test must include the Burn/support ban proof\nstdout:\n{}\nstderr:\n{}",
        as_text(&self_test.stdout),
        as_text(&self_test.stderr)
    );
}

struct TemporaryFixture {
    path: PathBuf,
}

impl TemporaryFixture {
    fn write(path: PathBuf, contents: String) -> Self {
        assert!(
            !path.exists(),
            "temporary claim-gate fixture path already exists: {}",
            path.display()
        );
        fs::write(&path, contents).expect("write temporary claim-gate fixture");
        Self { path }
    }
}

impl Drop for TemporaryFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn run_claim_gate(args: &[&str]) -> Output {
    let repo = repo_root();
    Command::new("node")
        .arg("scripts/check-app-claims.mjs")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("run app-claim gate")
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        as_text(&output.stdout),
        as_text(&output.stderr)
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root is reachable from apps/osl-hub")
}

fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
