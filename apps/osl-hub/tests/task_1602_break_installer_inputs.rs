use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!("{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("scratch directory can be created");
        Self { path }
    }

    fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("apps/osl-hub has a repository root")
        .to_path_buf()
}

fn write_recipe(input_dir: &Path, body: &str) {
    fs::create_dir_all(input_dir).expect("input directory can be created");
    fs::write(input_dir.join("recipe.json"), body).expect("recipe input can be written");
}

fn clean_copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("copy destination can be created");
    for entry in fs::read_dir(from).expect("copy source can be read") {
        let entry = entry.expect("directory entry can be read");
        let target = to.join(entry.file_name());
        fs::copy(entry.path(), target).expect("recipe copy can be written");
    }
}

fn run_recipe(script: &Path, input_dir: &Path, output_dir: &Path) -> Output {
    Command::new("python3")
        .arg(script)
        .arg("--input-dir")
        .arg(input_dir)
        .arg("--output-dir")
        .arg(output_dir)
        .output()
        .expect("installer recipe can be launched")
}

fn installer_count(output_dir: &Path) -> usize {
    fs::read_dir(output_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.path().is_file()
                        && entry
                            .path()
                            .extension()
                            .and_then(|extension| extension.to_str())
                            == Some("exe")
                })
                .count()
        })
        .unwrap_or(0)
}

fn installer_fingerprint(installer: &Path) -> String {
    let text = fs::read_to_string(installer).expect("installer fixture can be read");
    text.lines()
        .find_map(|line| line.strip_prefix("build_fingerprint="))
        .expect("installer records build fingerprint")
        .to_owned()
}

#[test]
fn task_1602_break_installer_inputs() {
    let root = repo_root();
    let script = root.join("scripts/installer_recipe.py");
    let scratch = Scratch::new("task-1602-installer-recipe");
    let output_dir = scratch.join("installers");
    let good = scratch.join("good-input");
    let missing_version = scratch.join("missing-version");
    let missing_build = scratch.join("missing-build");

    write_recipe(
        &good,
        r#"{"version":"2.0.0","build":{"fingerprint":"MAPLE-4172"}}"#,
    );
    clean_copy(&good, &missing_version);
    clean_copy(&good, &missing_build);
    fs::write(
        missing_version.join("recipe.json"),
        r#"{"build":{"fingerprint":"MAPLE-4172"}}"#,
    )
    .expect("missing-version copy can be changed");
    fs::write(missing_build.join("recipe.json"), r#"{"version":"2.0.0"}"#)
        .expect("missing-build copy can be changed");

    let before = installer_count(&output_dir);
    println!("installer_count_before={before}");
    assert_eq!(before, 0);

    let good_output = run_recipe(&script, &good, &output_dir);
    assert!(
        good_output.status.success(),
        "good recipe must pass; stdout={}, stderr={}",
        String::from_utf8_lossy(&good_output.stdout),
        String::from_utf8_lossy(&good_output.stderr)
    );
    let after_good = installer_count(&output_dir);
    let installer = output_dir.join("OSL-2.0.0.exe");
    let fingerprint_after_good = installer_fingerprint(&installer);
    println!("installer_count_after_good={after_good}");
    println!("OSL-2.0.0.exe_fingerprint_after_good={fingerprint_after_good}");
    assert_eq!(after_good, 1);
    assert_eq!(fingerprint_after_good, "MAPLE-4172");

    let missing_version_output = run_recipe(&script, &missing_version, &output_dir);
    let missing_version_stderr = String::from_utf8_lossy(&missing_version_output.stderr);
    println!(
        "missing_version_refusal_status={:?}",
        missing_version_output.status.code()
    );
    println!("missing_version_refusal={missing_version_stderr}");
    assert!(!missing_version_output.status.success());
    assert!(missing_version_stderr.contains("missing version"));

    let missing_build_output = run_recipe(&script, &missing_build, &output_dir);
    let missing_build_stderr = String::from_utf8_lossy(&missing_build_output.stderr);
    println!(
        "missing_build_refusal_status={:?}",
        missing_build_output.status.code()
    );
    println!("missing_build_refusal={missing_build_stderr}");
    assert!(!missing_build_output.status.success());
    assert!(missing_build_stderr.contains("missing build input"));

    let final_count = installer_count(&output_dir);
    let final_fingerprint = installer_fingerprint(&installer);
    println!("installer_count_final={final_count}");
    println!("OSL-2.0.0.exe_fingerprint_final={final_fingerprint}");
    assert_eq!(final_count, 1);
    assert_eq!(final_fingerprint, "MAPLE-4172");
}
