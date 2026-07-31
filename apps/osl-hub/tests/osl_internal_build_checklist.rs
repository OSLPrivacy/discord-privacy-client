use std::path::{Path, PathBuf};
use std::process::Command;

const TEST_TO_CREATE: &str = "docs/design/osl-internal-build-checklist.md";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn keep_compact_reports_and_trap_ledger_rules_current_after_each_wave() {
    assert_eq!(
        TEST_TO_CREATE,
        "docs/design/osl-internal-build-checklist.md"
    );

    let root = repo_root();
    let script = root.join("scripts/qa/test_osl_internal_build_checklist.py");
    let output = Command::new("python3")
        .arg(&script)
        .current_dir(&root)
        .output()
        .expect("run internal build checklist behavior test");

    assert!(
        output.status.success(),
        "internal build checklist behavior test failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mutation_check = Command::new("python3")
        .arg("-c")
        .arg(
            r#"
import importlib.util
from pathlib import Path

script = Path("scripts/qa/test_osl_internal_build_checklist.py")
spec = importlib.util.spec_from_file_location("checklist_contract", script)
module = importlib.util.module_from_spec(spec)
assert spec and spec.loader
spec.loader.exec_module(module)

steps = module.update_protocol_steps(module.CHECKLIST.read_text(encoding="utf-8"))
assert module.has_current_compact_report_rule(steps)
assert not module.has_current_compact_report_rule([
    "close every wave with a compact report that records exact changed files, tests/evidence, and blockers",
])
assert not module.has_current_compact_report_rule([
    "when status changes, close every wave with exact changed files, tests/evidence, blockers, "
    "and trap-ledger disposition updated, unchanged-no-durable-trap-change, or pruned",
])
assert not module.has_current_compact_report_rule([
    "close every wave with trap-ledger disposition updated, unchanged-no-durable-trap-change, or pruned",
])
"#,
        )
        .current_dir(&root)
        .output()
        .expect("run internal build checklist mutation controls");

    assert!(
        mutation_check.status.success(),
        "internal build checklist mutation controls failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        mutation_check.status.code(),
        String::from_utf8_lossy(&mutation_check.stdout),
        String::from_utf8_lossy(&mutation_check.stderr)
    );
}
