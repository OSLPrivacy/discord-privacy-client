use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SELF_TEST_COMMAND_NAME: &str = "--self-test";
const REPOSITORY_SCAN_TEST_NAME: &str = "scripts/check-app-claims.mjs";

#[test]
fn app_claim_gate_self_test_proves_release_workflow_and_parser_bindings() {
    let output = run_claim_gate(&[SELF_TEST_COMMAND_NAME]);
    assert_success(&output, "app-claim self-test");

    let stdout = as_text(&output.stdout);
    for required in [
        "PASS Gate public claims through the allowlist self-test before release.",
        "PASS Bind banned public phrases into the app-claim parser.",
        "PASS Scan repository copy without exposing implementation concepts to users.",
        "PASS Gate public claims against exact support evidence",
    ] {
        assert!(
            stdout.contains(required),
            "app-claim self-test did not prove `{required}`\nstdout:\n{stdout}\nstderr:\n{}",
            as_text(&output.stderr)
        );
    }
}

#[test]
fn scripts_check_app_claims_mjs() {
    let repo = repo_root();
    let fixture_path = repo.join("apps/osl-hub-ui/src/__claim_gate_negative_fixture.ts");
    let _fixture = TemporaryFixture::write(
        fixture_path,
        [
            "export const claimGateNegativeFixture = [",
            "  '<button data-public-claim=\"Open keyserver settings\">Keyserver settings</button>',",
            "  'Signal is supported for protected messaging.',",
            "].join('\\n');",
            "",
        ]
        .join("\n"),
    );

    let output = run_claim_gate(&[]);
    assert!(
        !output.status.success(),
        "{REPOSITORY_SCAN_TEST_NAME} must refuse public implementation concepts and unsupported exact-support claims"
    );

    let combined = format!("{}\n{}", as_text(&output.stdout), as_text(&output.stderr));
    for required in [
        "__claim_gate_negative_fixture.ts",
        "implementation concept in public copy",
        "validateSupportMatrixClaims: Signal support claim without exact matrix evidence",
    ] {
        assert!(
            combined.contains(required),
            "{REPOSITORY_SCAN_TEST_NAME} did not report `{required}` for the negative fixture\n{combined}"
        );
    }
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
    let mut command = Command::new("node");
    command
        .arg("scripts/check-app-claims.mjs")
        .args(args)
        .current_dir(repo);
    command.output().expect("run app-claim gate")
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
