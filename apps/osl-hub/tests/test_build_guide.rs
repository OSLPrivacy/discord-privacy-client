#![cfg(feature = "core")]

use std::path::{Path, PathBuf};

const GUIDE_RELATIVE_PATH: &str = "docs/qa/osl-hub-test-build-guide.md";
const ONE_BUILD_COMMAND: &str = "tauri build --features desktop";
const SWITCH_ENV: &str = "OSL_TEST_ONLY_RUNTIME_SWITCHES";
const PASSWORD_SWITCH: &str = "password_screen_access=skip-password-screen-for-test";
const SAFE_SENDING_SWITCH: &str = "safe_sending=dry-run-send-for-test";
const OLD_TEST_BUILD_SELECTORS: &[&str] = &[
    "--features desktop,discord-qa-shell",
    "--features desktop,whatsapp-qa-identity",
    "desktop,discord-qa-shell",
    "desktop,whatsapp-qa-identity",
    "old testing build",
    "WhatsApp lab identity build",
];

#[derive(Debug, Eq, PartialEq)]
struct GuideCheck {
    build_commands: Vec<String>,
    old_selectors: Vec<&'static str>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("apps/osl-hub has a repo root ancestor")
        .to_path_buf()
}

fn read_guide() -> String {
    std::fs::read_to_string(repo_root().join(GUIDE_RELATIVE_PATH))
        .unwrap_or_else(|error| panic!("read {GUIDE_RELATIVE_PATH}: {error}"))
}

fn check_guide(markdown: &str) -> Result<GuideCheck, String> {
    let build_commands: Vec<String> = markdown
        .lines()
        .map(str::trim)
        .filter(|line| line.contains(" build ") || line.ends_with(" build"))
        .filter(|line| line.starts_with("tauri ") || line.starts_with("cargo "))
        .map(str::to_owned)
        .collect();
    if build_commands != [ONE_BUILD_COMMAND] {
        return Err(format!(
            "expected one build command {ONE_BUILD_COMMAND:?}, got {:?}",
            build_commands
        ));
    }

    let old_selectors: Vec<&'static str> = OLD_TEST_BUILD_SELECTORS
        .iter()
        .copied()
        .filter(|selector| markdown.contains(selector))
        .collect();
    if !old_selectors.is_empty() {
        return Err(format!(
            "old test-build selector reintroduced: {}",
            old_selectors.join(", ")
        ));
    }

    for required in [SWITCH_ENV, PASSWORD_SWITCH, SAFE_SENDING_SWITCH] {
        if !markdown.contains(required) {
            return Err(format!("missing runtime switch guide text: {required}"));
        }
    }

    Ok(GuideCheck {
        build_commands,
        old_selectors,
    })
}

#[test]
fn test_guide_has_one_build_command_and_runtime_switches() {
    let guide = read_guide();
    let checked = check_guide(&guide).expect("test build guide stays on one build");

    println!("TEST GUIDE: {GUIDE_RELATIVE_PATH}");
    println!("BUILD COMMAND COUNT: {}", checked.build_commands.len());
    println!("BUILD COMMAND: {}", checked.build_commands[0]);
    println!(
        "OLD TEST-BUILD SELECTOR COUNT: {}",
        checked.old_selectors.len()
    );
    println!("RUNTIME SWITCH ENV: {SWITCH_ENV}");
    println!("RUNTIME SWITCH: {PASSWORD_SWITCH}");
    println!("RUNTIME SWITCH: {SAFE_SENDING_SWITCH}");
}

#[test]
fn reintroduced_old_test_build_selector_is_rejected() {
    let guide = read_guide();
    let mutated = format!(
        "{guide}\n{selector}\n",
        selector = OLD_TEST_BUILD_SELECTORS[0]
    );
    let error = check_guide(&mutated).expect_err("old selector must make the check fail");

    println!("OLD SELECTOR RED CONTROL: {}", OLD_TEST_BUILD_SELECTORS[0]);
    println!("OLD SELECTOR RED CONTROL ERROR: {error}");
    assert!(error.contains("old test-build selector reintroduced"));
}
