use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

const TS_TEST_WORKFLOW: &str = include_str!("../../../.github/workflows/ts-test.yml");
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
fn success() {
    let step = ts_test_success_step();

    let all_green =
        run_ts_test_success_script(&step.script, required_check_env(&step.env_names, None));
    assert_success(&all_green, "ts-test success aggregator");

    for failed_check in &step.required_checks {
        let output = run_ts_test_success_script(
            &step.script,
            required_check_env(&step.env_names, Some(failed_check.env_name.as_str())),
        );
        assert_failure(
            &output,
            "ts-test success aggregator",
            "must fail when a required upstream job is not green",
        );
        let combined = combined_output(&output);
        let expected = format!("{} failed with result: failure", failed_check.label);
        assert!(
            combined.contains(&expected),
            "ts-test success aggregator did not identify `{}` as failed\n{combined}",
            failed_check.label
        );
    }
}

#[derive(Debug)]
struct TsTestSuccessStep {
    script: String,
    env_names: Vec<String>,
    required_checks: Vec<RequiredCheck>,
}

#[derive(Debug)]
struct RequiredCheck {
    env_name: String,
    label: String,
}

fn ts_test_success_step() -> TsTestSuccessStep {
    let mut in_success_job = false;
    let mut in_success_step = false;
    let mut in_env_block = false;
    let mut in_run_block = false;
    let mut env_names = Vec::new();
    let mut script = String::new();

    for line in TS_TEST_WORKFLOW.lines() {
        if line.starts_with("  ") && !line.starts_with("    ") && line.ends_with(':') {
            in_success_job = line.trim() == "success:";
            in_success_step = false;
            in_env_block = false;
            in_run_block = false;
            continue;
        }
        if !in_success_job {
            continue;
        }
        if line.trim() == "- name: \"success'\"" {
            in_success_step = true;
            continue;
        }
        if !in_success_step {
            continue;
        }
        if in_env_block {
            if let Some(env_line) = line.strip_prefix("          ") {
                if let Some((name, _value)) = env_line.split_once(':') {
                    env_names.push(name.trim().to_string());
                    continue;
                }
            }
            in_env_block = false;
        }
        if line.trim() == "env:" {
            in_env_block = true;
            continue;
        }
        if in_success_step && line.trim() == "run: |" {
            in_run_block = true;
            continue;
        }
        if in_run_block {
            if line.starts_with("          ") || line.trim().is_empty() {
                if !script.is_empty() {
                    script.push('\n');
                }
                script.push_str(line.strip_prefix("          ").unwrap_or(""));
            } else {
                break;
            }
        }
    }

    assert!(
        !script.trim().is_empty(),
        "ts-test workflow must have one runnable success' step"
    );
    assert!(
        !env_names.is_empty(),
        "ts-test workflow success' step must declare the required-check result env"
    );
    let required_checks = required_checks_from_script(&script);
    assert!(
        !required_checks.is_empty(),
        "ts-test workflow success' step must run required checks from result env vars"
    );
    let declared_env = env_name_set(env_names.iter().map(String::as_str));
    let undeclared_refs: Vec<&str> = required_checks
        .iter()
        .map(|check| check.env_name.as_str())
        .filter(|env_name| !declared_env.contains(env_name))
        .collect();
    assert!(
        undeclared_refs.is_empty(),
        "ts-test workflow success' step must declare env var(s) referenced by run script: {}",
        undeclared_refs.join(", ")
    );

    TsTestSuccessStep {
        script,
        env_names,
        required_checks,
    }
}

fn required_checks_from_script(script: &str) -> Vec<RequiredCheck> {
    let mut checks = Vec::new();
    for line in script.lines() {
        let trimmed = line.trim().trim_end_matches('\\').trim();
        let Some(quoted) = trimmed
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        else {
            continue;
        };
        let Some((label, env_name)) = quoted.rsplit_once("=$") else {
            continue;
        };
        if env_name.ends_with("_RESULT")
            && env_name
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch == '_')
        {
            checks.push(RequiredCheck {
                env_name: env_name.to_string(),
                label: label.to_string(),
            });
        }
    }
    checks
}

fn required_check_env(env_names: &[String], failed_name: Option<&str>) -> Vec<(String, String)> {
    env_names
        .iter()
        .map(|name| {
            (
                name.clone(),
                if Some(name.as_str()) == failed_name {
                    "failure"
                } else {
                    "success"
                }
                .to_string(),
            )
        })
        .collect()
}

fn run_ts_test_success_script(script: &str, env: Vec<(String, String)>) -> Output {
    let repo = repo_root();
    let provided_env = env_name_set(env.iter().map(|(name, _value)| name.as_str()));
    let missing_env: Vec<&str> = shell_result_env_refs(script)
        .into_iter()
        .filter(|env_name| !provided_env.contains(env_name))
        .collect();
    assert!(
        missing_env.is_empty(),
        "ts-test success aggregator fixture must provide env var(s) referenced by extracted script: {}",
        missing_env.join(", ")
    );

    let script_path = repo.join("target").join(format!(
        "ts-test-success-{}-{}.sh",
        std::process::id(),
        monotonic_suffix()
    ));
    fs::create_dir_all(script_path.parent().expect("script has parent"))
        .expect("create target directory for workflow script test");
    fs::write(&script_path, script).expect("write workflow script fixture");
    let output = Command::new("bash")
        .arg("-e")
        .arg(&script_path)
        .envs(env)
        .current_dir(&repo)
        .output()
        .expect("run ts-test success workflow script");
    let _ = fs::remove_file(script_path);
    output
}

fn shell_result_env_refs(script: &str) -> BTreeSet<&str> {
    script
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'))
        .filter_map(|word| word.strip_prefix('$'))
        .filter(|word| {
            word.ends_with("_RESULT") && word.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_')
        })
        .collect()
}

fn env_name_set<'a>(env_names: impl IntoIterator<Item = &'a str>) -> BTreeSet<&'a str> {
    env_names.into_iter().collect()
}

fn monotonic_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
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
