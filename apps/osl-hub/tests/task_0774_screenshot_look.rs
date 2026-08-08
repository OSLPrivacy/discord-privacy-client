use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/osl-hub has a repository root")
        .to_path_buf()
}

fn capture(root: &Path, variant: &str, omit: Option<&str>) -> std::process::Output {
    let mut command = Command::new("bash");
    command
        .arg(root.join("scripts/qa/osl-look-fixed-screen-capture.sh"))
        .env("OSL_LOOK_SCREEN_VARIANT", variant)
        .env(
            "OSL_LOOK_SCREEN_OUT",
            root.join("evidence/task-0774-look-screen").join(variant),
        );
    if let Some(control) = omit {
        command.env("OSL_LOOK_SCREEN_OMIT_CONTROL", control);
    }
    command.output().expect("Look capture can launch")
}

#[test]
fn look_fixed_screens_name_all_controls_and_are_nonblank() {
    let root = repo_root();
    let controls = "OSL blue,High contrast,Custom accent,Save,Reset";
    for variant in ["osl-blue", "high-contrast", "custom-accent"] {
        let output = capture(&root, variant, None);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{variant} capture must pass; stdout={stdout}; stderr={stderr}"
        );
        print!("{stdout}");
        assert!(
            stdout.contains("TASK0774_SCREEN_TREE_TITLE=Look"),
            "{stdout}"
        );
        for control in controls.split(',') {
            assert!(
                stdout.contains(&format!("TASK0774_SCREEN_TREE_CONTROL={control}")),
                "{stdout}"
            );
        }
        assert!(
            stdout.contains(&format!(
                "TASK0774_DONE variant={variant} title=Look controls={controls} blank=false"
            )),
            "{stdout}"
        );
        let image = root
            .join("evidence/task-0774-look-screen")
            .join(variant)
            .join(format!("look-{variant}.png"));
        assert!(image.is_file(), "{}", image.display());
    }
}

#[test]
fn look_screen_tree_check_fails_for_throwaway_screen_missing_named_control() {
    let root = repo_root();
    let output = capture(&root, "osl-blue", Some("Reset"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "missing Reset must make checker fail; stdout={} stderr={stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("missing=['Reset']"),
        "red check must identify missing named control: {stderr}"
    );
    println!("TASK0774_RED_CHECK omitted=Reset status=failed");
}
