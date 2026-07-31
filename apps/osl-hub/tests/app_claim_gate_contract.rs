use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

const SELF_TEST_COMMAND_NAME: &str = "--self-test";
const REPOSITORY_SCAN_TEST_NAME: &str = "scripts/check-app-claims.mjs";
const SELF_TEST_RELEASE_WORKFLOW_NAME: &str =
    "Gate public claims through the allowlist self-test before release.";
const SELF_TEST_BANNED_PHRASES_NAME: &str = "Bind banned public phrases into the app-claim parser.";
const SELF_TEST_REPOSITORY_COPY_NAME: &str =
    "Scan repository copy without exposing implementation concepts to users.";
const SELF_TEST_SUPPORT_EVIDENCE_NAME: &str = "Gate public claims against exact support evidence";

static CLAIM_GATE_REPO_MUTATION_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn gate_public_claims_through_the_allowlist_self_test_before_release() {
    assert_self_test_passes_case(SELF_TEST_RELEASE_WORKFLOW_NAME);
}

#[test]
fn bind_banned_public_phrases_into_the_app_claim_parser() {
    assert_self_test_passes_case(SELF_TEST_BANNED_PHRASES_NAME);

    let _guard = CLAIM_GATE_REPO_MUTATION_LOCK
        .lock()
        .expect("claim-gate fixture lock is not poisoned");
    let repo = repo_root();
    let fixture_path = repo.join("apps/osl-hub-ui/src/__claim_gate_banned_phrase_fixture.ts");
    let _fixture = TemporaryFixture::write(
        fixture_path,
        [
            "export const claimGateBannedPhraseFixture = [",
            "  'This release offers cryptographic burn.',",
            "  'This release leaves permanent ciphertext.',",
            "].join('\\n');",
            "",
        ]
        .join("\n"),
    );

    let output = run_claim_gate(&[]);
    assert_failure(
        &output,
        SELF_TEST_BANNED_PHRASES_NAME,
        "must reject phrases parsed from allowlist section D",
    );
    let combined = combined_output(&output);
    for required in [
        "__claim_gate_banned_phrase_fixture.ts",
        "Cryptographic burn",
        "permanent ciphertext",
    ] {
        assert!(
            combined.contains(required),
            "{SELF_TEST_BANNED_PHRASES_NAME} did not report `{required}` for the banned-phrase fixture\n{combined}"
        );
    }
}

#[test]
fn scan_repository_copy_without_exposing_implementation_concepts_to_users() {
    assert_self_test_passes_case(SELF_TEST_REPOSITORY_COPY_NAME);

    let _guard = CLAIM_GATE_REPO_MUTATION_LOCK
        .lock()
        .expect("claim-gate fixture lock is not poisoned");
    let repo = repo_root();
    let fixture_path = repo.join("apps/osl-hub-ui/src/__claim_gate_public_concept_fixture.ts");
    let _fixture = TemporaryFixture::write(
        fixture_path,
        [
            "export const claimGatePublicConceptFixture = [",
            "  '<button data-public-claim=\"Open keyserver settings\">Keyserver settings</button>',",
            "].join('\\n');",
            "",
        ]
        .join("\n"),
    );

    let output = run_claim_gate(&[]);
    assert_failure(
        &output,
        SELF_TEST_REPOSITORY_COPY_NAME,
        "must reject implementation concepts in public app copy",
    );
    let combined = combined_output(&output);
    for required in [
        "__claim_gate_public_concept_fixture.ts",
        "implementation concept in public copy",
    ] {
        assert!(
            combined.contains(required),
            "{SELF_TEST_REPOSITORY_COPY_NAME} did not report `{required}` for the public-copy fixture\n{combined}"
        );
    }
}

#[test]
fn scripts_check_app_claims_mjs() {
    let _guard = CLAIM_GATE_REPO_MUTATION_LOCK
        .lock()
        .expect("claim-gate fixture lock is not poisoned");
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

#[test]
fn gate_public_claims_against_exact_support_evidence() {
    assert_self_test_passes_case(SELF_TEST_SUPPORT_EVIDENCE_NAME);

    let _guard = CLAIM_GATE_REPO_MUTATION_LOCK
        .lock()
        .expect("claim-gate fixture lock is not poisoned");
    let repo = repo_root();
    let fixture_path = repo.join("apps/osl-hub-ui/src/__claim_gate_support_claim_fixture.ts");
    let _fixture = TemporaryFixture::write(
        fixture_path,
        [
            "export const claimGateSupportClaimFixture = [",
            "  'Signal is supported for protected messaging.',",
            "].join('\\n');",
            "",
        ]
        .join("\n"),
    );

    let output = run_claim_gate(&[]);
    assert_failure(
        &output,
        SELF_TEST_SUPPORT_EVIDENCE_NAME,
        "must reject public support claims without exact support evidence",
    );
    let combined = combined_output(&output);
    for required in [
        "__claim_gate_support_claim_fixture.ts",
        "validateSupportMatrixClaims: Signal support claim without exact matrix evidence",
    ] {
        assert!(
            combined.contains(required),
            "{SELF_TEST_SUPPORT_EVIDENCE_NAME} did not report `{required}` for the support-claim fixture\n{combined}"
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

fn assert_failure(output: &Output, label: &str, reason: &str) {
    assert!(
        !output.status.success(),
        "{label} {reason}, but the claim gate passed\nstdout:\n{}\nstderr:\n{}",
        as_text(&output.stdout),
        as_text(&output.stderr)
    );
}

fn assert_self_test_passes_case(case_name: &str) {
    let output = run_claim_gate(&[SELF_TEST_COMMAND_NAME]);
    assert_success(&output, "app-claim self-test");

    let stdout = as_text(&output.stdout);
    let required = format!("PASS {case_name}");
    assert!(
        stdout.lines().any(|line| line == required),
        "app-claim self-test did not prove `{case_name}`\nstdout:\n{stdout}\nstderr:\n{}",
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

fn combined_output(output: &Output) -> String {
    format!("{}\n{}", as_text(&output.stdout), as_text(&output.stderr))
}
