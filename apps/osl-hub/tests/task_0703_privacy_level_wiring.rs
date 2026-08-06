use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[test]
fn privacy_level_check_names_the_missing_changed_rule() {
    let repo = repo_root();
    let checker = repo.join("scripts/qa/check-privacy-level-wiring.mjs");
    let source_plan = repo.join("docs/design/osl-gui-final-plan.md");

    let green = run_checker(&checker, &source_plan);
    assert!(
        green.status.success(),
        "privacy-level check must pass on the source plan\nstdout:\n{}\nstderr:\n{}",
        text(&green.stdout),
        text(&green.stderr)
    );

    let scratch = tempfile::tempdir().expect("create privacy-level fixture directory");
    let broken_plan = scratch.path().join("osl-gui-final-plan.md");
    let original = fs::read_to_string(&source_plan).expect("read GUI plan");
    let broken = original.replacen(
        "| Discord | Installed Discord for Windows | Local protection and user-assisted handoff |",
        "| Discord | Installed Discord for Windows | Local protection |",
        1,
    );
    assert_ne!(broken, original, "negative fixture must remove one level-to-rule link");
    fs::write(&broken_plan, broken).expect("write broken GUI plan copy");

    let red = run_checker(&checker, &broken_plan);
    assert_eq!(
        red.status.code(),
        Some(1),
        "privacy-level check must exit 1 on the broken copy\nstdout:\n{}\nstderr:\n{}",
        text(&red.stdout),
        text(&red.stderr)
    );
    let combined = format!("{}\n{}", text(&red.stdout), text(&red.stderr));
    assert!(
        combined.contains(
            "PRIVACY_LEVEL_RULE_MISSING: Discord missing changed rule \"user-assisted handoff\" for User-assisted action"
        ),
        "privacy-level check must name the missing changed rule\n{combined}"
    );
}

fn run_checker(checker: &Path, gui_plan: &Path) -> Output {
    Command::new("node")
        .arg(checker)
        .arg("--gui-plan")
        .arg(gui_plan)
        .output()
        .expect("run privacy-level checker")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root is reachable from apps/osl-hub")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
