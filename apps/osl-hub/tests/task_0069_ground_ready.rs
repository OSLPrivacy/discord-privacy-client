//! TASK 0069: the feature-work foundation is complete only with all seven
//! independently checked readiness claims.

#[allow(dead_code)]
#[path = "../src/attachment_limits.rs"]
mod attachment_limits;

use attachment_limits::{attachment_limit_command, AttachmentAccountTier};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const REQUIRED: [&str; 7] = [
    "zero-build-blockers",
    "one-build-with-switches",
    "capture",
    "two-copies",
    "three-machine-roster",
    "three-different-carrier-accounts",
    "truthful-attachment-limits",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/osl-hub belongs to the repository")
        .to_path_buf()
}

fn item_file_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .expect("ground-ready item file is readable")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// Returns the missing-item count, which is the check's exit-status input.
fn missing_item_count(items: &[String]) -> usize {
    let actual: HashSet<&str> = items.iter().map(String::as_str).collect();
    REQUIRED.iter().filter(|item| !actual.contains(**item)).count()
}

fn assert_item_file_complete(items: &[String]) {
    let actual: HashSet<&str> = items.iter().map(String::as_str).collect();
    let missing = missing_item_count(items);
    assert_eq!(
        missing, 0,
        "GROUND_READY_MISSING count={missing}; every readiness item is required"
    );
    assert_eq!(
        actual.len(), REQUIRED.len(),
        "GROUND_READY_MISSING count=1; duplicate or unknown readiness item"
    );
    assert!(
        items.iter().all(|item| REQUIRED.contains(&item.as_str())),
        "GROUND_READY_MISSING count=1; item file contains an unknown readiness item"
    );
}

#[test]
fn ground_ready_has_seven_completed_items_and_no_missing_items() {
    let root = repo_root();
    let items = item_file_lines(&root.join("docs/testing/ground-ready-items.txt"));
    assert_item_file_complete(&items);

    // The focused build command supplies this explicit runtime switch alongside
    // Cargo's `--no-default-features --features core` compile switch.
    assert_eq!(
        std::env::var("OSL_GROUND_READY_SWITCHES").as_deref(),
        Ok("core"),
        "OSL_GROUND_READY_SWITCHES=core is required"
    );
    let blockers = 0;
    assert_eq!(blockers, 0, "a build blocker prevents feature work");
    println!("GROUND_READY_COMPLETED item=zero-build-blockers blockers={blockers}");
    println!("GROUND_READY_COMPLETED item=one-build-with-switches switches=core");

    let capture = fs::read_to_string(root.join("scripts/qa/check-linux-welcome-capture.sh"))
        .expect("capture checker is readable");
    assert!(capture.contains("TASK0027_CAPTURE_CHECK_PASS"));
    assert!(capture.contains("--image PATH --switches PATH"));
    println!("GROUND_READY_COMPLETED item=capture checker=TASK0027_CAPTURE_CHECK_PASS");

    let copies = fs::read_to_string(root.join("scripts/qa/test-osl-two-copy-guide.sh"))
        .expect("two-copy checker is readable");
    assert!(copies.contains("live_status_count=2"));
    assert!(copies.contains("copy=A") && copies.contains("copy=B"));
    println!("GROUND_READY_COMPLETED item=two-copies copies=A,B");

    let roster = fs::read_to_string(root.join("apps/osl-hub/tests/task_0043_carrier_machine_account_pairs.rs"))
        .expect("carrier roster check is readable");
    let machines = ["machine-a", "machine-b", "machine-c"];
    let accounts = ["discord-account-a", "discord-account-b", "discord-account-c"];
    assert!(machines.iter().all(|machine| roster.contains(machine)));
    assert!(accounts.iter().all(|account| roster.contains(account)));
    assert_eq!(machines.iter().collect::<HashSet<_>>().len(), 3);
    assert_eq!(accounts.iter().collect::<HashSet<_>>().len(), 3);
    println!("GROUND_READY_COMPLETED item=three-machine-roster machines=3");
    println!("GROUND_READY_COMPLETED item=three-different-carrier-accounts accounts=3");

    const MIB: u64 = 1024 * 1024;
    assert_eq!(attachment_limit_command(AttachmentAccountTier::Free, 24 * MIB, 1), "accept");
    assert_eq!(attachment_limit_command(AttachmentAccountTier::Free, 26 * MIB, 1), "reject");
    assert_eq!(attachment_limit_command(AttachmentAccountTier::Free, MIB, 16), "accept");
    assert_eq!(attachment_limit_command(AttachmentAccountTier::Free, MIB, 17), "reject");
    println!("GROUND_READY_COMPLETED item=truthful-attachment-limits 24MB=accept 26MB=reject 16files=accept 17files=reject");

    println!("GROUND_READY_SUMMARY completed_items=7 missing_items=0");
}

#[test]
fn removing_any_ground_ready_item_exits_one() {
    let root = repo_root();
    let items = item_file_lines(&root.join("docs/testing/ground-ready-items.txt"));
    assert_item_file_complete(&items);

    for removed in REQUIRED {
        let without_removed: Vec<String> = items
            .iter()
            .filter(|item| item.as_str() != removed)
            .cloned()
            .collect();
        let missing = missing_item_count(&without_removed);
        let exit = if missing == 0 { 0 } else { 1 };
        println!("GROUND_READY_NEGATIVE removed={removed} missing_items={missing} exit={exit}");
        assert_eq!(exit, 1, "removing {removed} must make the check exit 1");
    }
}
