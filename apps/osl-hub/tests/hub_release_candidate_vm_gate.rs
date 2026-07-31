/// Extract the block between two markers, matching each marker as a WHOLE LINE.
///
/// A plain `find` matched the wrong block: the script lists step arguments as
/// `            'launch' { @('exeSha256','timeoutSeconds') }` at a deeper indent,
/// and an 8-space marker matches inside that 12-space line (8 of its spaces
/// followed by the quote). The section therefore covered the argument list rather
/// than the implementation, and every assertion about the launch step was checking
/// a few characters that could not contain them.
fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    // Anchor to the START of a line. Markers are line prefixes, not whole lines, so
    // requiring a trailing newline breaks callers like "function Process-RunDirectory";
    // but an unanchored find matches a DEEPER-indented line (an 8-space marker matches
    // inside a 12-space one), which is what selected the wrong block.
    let at_line_start = |marker: &str, from: usize| -> Option<usize> {
        // Prefer a match at the start of a line: an unanchored find lets an
        // 8-space marker match INSIDE a 12-space line, which selected the step
        // argument list instead of the launch implementation and left every
        // assertion inspecting a few characters that could not contain them.
        // Some markers are written without their real indentation, so fall back
        // to a plain search when no line-anchored match exists.
        let needle = format!("\n{marker}");
        source[from..]
            .find(&needle)
            .map(|offset| from + offset + 1)
            .or_else(|| source[from..].find(marker).map(|offset| from + offset))
    };
    let start_index = at_line_start(start, 0).expect("source section start exists");
    let end_index = at_line_start(end, start_index).expect("source section end exists");
    &source[start_index..end_index]
}

fn assert_order(source: &str, earlier: &str, later: &str) {
    let earlier_index = source.find(earlier).expect("earlier marker exists");
    let later_index = source.find(later).expect("later marker exists");
    assert!(
        earlier_index < later_index,
        "expected `{earlier}` before `{later}`"
    );
}

#[test]
fn f1_live_windows_walkthrough_imports_nonempty_receipt() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = repo_root.join("scripts/vmqa/vmqa-run.sh");
    let output = std::process::Command::new("bash")
        .arg(&script)
        .arg("f1_live_windows_walkthrough_imports_nonempty_receipt")
        .current_dir(&repo_root)
        .output()
        .expect("run F1 live Windows walkthrough behavior harness");

    assert!(
        output.status.success(),
        "F1 walkthrough behavior harness failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn f2_real_vm_five_frame_walkthrough() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let script = repo_root.join("scripts/vmqa/vmqa-run.sh");
    let output = std::process::Command::new("bash")
        .arg(&script)
        .arg("f2_real_vm_five_frame_walkthrough")
        .current_dir(&repo_root)
        .output()
        .expect("run F2 real-VM five-frame walkthrough behavior harness");

    assert!(
        output.status.success(),
        "F2 walkthrough behavior harness failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn second_session_rebuild_reproduces_recorded_executable_hash() {
    let agent = include_str!("../../../scripts/vmqa/vmqa-agent.ps1");
    let win32 = include_str!("../../../scripts/vmqa/vmqa-win32.ps1");
    let operator = include_str!("../../../scripts/vmqa/vmqa-fast-cycle-operator.py");

    let process_run = section(agent, "function Process-RunDirectory", "# CLAIM THE RUN");
    for required in [
        "Assert-ExactJsonProperties -Object $request `\n            -Names @('schemaVersion','runId','identifier','runStartUtc','exeSha256',`",
        "$requestExeSha256 = ([string]$requestExeValue).ToLowerInvariant()",
        "if ($requestExeSha256 -notmatch '^[0-9a-f]{64}$')",
        "Assert-StrictBuildIdentity -Identity $buildIdentity -ExpectedExeSha256 $requestExeSha256",
    ] {
        assert!(
            process_run.contains(required),
            "request intake must bind the recorded executable hash: {required}"
        );
    }
    assert_order(
        process_run,
        "$requestExeSha256 = ([string]$requestExeValue).ToLowerInvariant()",
        "Assert-StrictBuildIdentity -Identity $buildIdentity -ExpectedExeSha256 $requestExeSha256",
    );

    let strict_identity = section(
        agent,
        "function Assert-StrictBuildIdentity",
        "function Assert-StrictRequestSteps",
    );
    assert!(
        strict_identity
            .contains("[string]$Identity.artifacts.executable.sha256 -cne $ExpectedExeSha256"),
        "the rebuilt build identity must reproduce the request's recorded executable hash"
    );

    let stage = section(
        agent,
        "function Copy-And-VerifyBuild",
        "function Invoke-SurfaceShot",
    );
    for required in [
        "$sourceExe = \"builds/$sha/osl-privacy-hub.exe\"",
        "$destExeHash = Get-Sha256 -Path $destExe",
        "if ($destExeHash -cne $sha -or $destExeSize -ne $ExeSizeBytes)",
        "staged exe identity mismatch expectedSha=$sha actualSha=$destExeHash",
    ] {
        assert!(
            stage.contains(required),
            "staging must be content-addressed by the recorded executable hash: {required}"
        );
    }
    assert_order(
        stage,
        "$sourceExe = \"builds/$sha/osl-privacy-hub.exe\"",
        "$destExeHash = Get-Sha256 -Path $destExe",
    );
    assert_order(
        stage,
        "$destExeHash = Get-Sha256 -Path $destExe",
        "staged exe identity mismatch expectedSha=$sha actualSha=$destExeHash",
    );

    let launch = section(agent, "        'launch' {", "        'shot' {");
    for required in [
        "$script:VmqaStartedExeSha256 = $exeSha",
        "if ($subject.ExeSha256 -cne $exeSha)",
        "marker image sha '$($subject.ExeSha256)' did not match requested exe sha '$exeSha'",
        "exeSha256 = $subject.ExeSha256",
    ] {
        assert!(
            launch.contains(required),
            "launch must measure the live executable before passing: {required}"
        );
    }
    assert_order(
        launch,
        "if ($subject.ExeSha256 -cne $exeSha)",
        "$script:VmqaPinnedPid = $subject.Pid",
    );
    assert_order(
        launch,
        "if ($subject.ExeSha256 -cne $exeSha)",
        "exeSha256 = $subject.ExeSha256",
    );

    let subject_resolution = section(
        win32,
        "function Resolve-VmqaSubject",
        "function Assert-VmqaSubject",
    );
    assert!(
        subject_resolution.contains(
            "(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()"
        ),
        "subject resolution must identify the live executable by content, not title or mtime"
    );

    assert!(
        operator.contains("sha256_bytes(exe_raw) != build[\"executableSha256\"]")
            && operator.contains("sha256_bytes(exe_raw) != build[\"stagedExecutableSha256\"]")
            && operator.contains("len(exe_raw) != build[\"stagedSizeBytes\"]"),
        "the second-session operator must re-read bundle bytes and compare them to the recorded executable identity"
    );
}

#[test]
fn final_owner_gated_signoff_reproduces_package_from_second_session() {
    use sha2::{Digest, Sha256};

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut digest = Sha256::new();
        digest.update(bytes);
        format!("{:x}", digest.finalize())
    }

    fn write_candidate(
        root: &std::path::Path,
        operator: &str,
        final_approver: &str,
        reproduced: bool,
    ) {
        let installer_name = "osl-hub-0.1.0-x64-nsis.exe";
        let installer_bytes = b"signed candidate fixture";
        std::fs::write(root.join(installer_name), installer_bytes).unwrap();
        std::fs::write(
            root.join("latest.json"),
            serde_json::json!({
                "version": "0.1.0",
                "platforms": {
                    "windows-x86_64": {
                        "signature": "signed-update-fixture",
                        "url": format!(
                            "https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-v0.1.0/{installer_name}"
                        )
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            root.join("hub-vm-qa-attestation.json"),
            serde_json::json!({
                "schemaVersion": 1,
                "candidateTag": "hub-v0.1.0",
                "candidateSha256": sha256_hex(installer_bytes),
                "completedAtUtc": "2026-07-17T23:00:00Z",
                "operator": operator,
                "finalApprover": final_approver,
                "packageReproducedBySecondSession": reproduced,
                "captchaHandling": "paused_for_manual_completion",
                "vms": [
                    {"name": "A", "goldenSnapshotId": "signed-a", "cleanRestore": true},
                    {"name": "B", "goldenSnapshotId": "signed-b", "cleanRestore": true}
                ],
                "cases": {
                    "onboarding": true,
                    "identityCreate": true,
                    "identityRecover": true,
                    "twoAccountLogin": true,
                    "persistenceRestart": true,
                    "signedUpdate": true,
                    "oneSidedEncryption": true,
                    "twoSidedEncryption": true,
                    "fullCleanup": true
                }
            })
            .to_string(),
        )
        .unwrap();
    }

    fn verify_candidate(root: &std::path::Path) -> std::process::Output {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::process::Command::new(std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_owned()))
            .arg(repo_root.join("scripts/verify_hub_vm_qa_attestation.py"))
            .arg("--tag")
            .arg("hub-v0.1.0")
            .arg("--candidate-dir")
            .arg(root)
            .arg("--attestation")
            .arg(root.join("hub-vm-qa-attestation.json"))
            .current_dir(repo_root)
            .output()
            .expect("run VM QA attestation verifier")
    }

    let root = std::env::temp_dir().join(format!(
        "osl-hub-final-signoff-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();

    write_candidate(
        &root,
        "qa-reviewer",
        "qa-final-approver-second-session",
        true,
    );
    let accepted = verify_candidate(&root);
    assert!(
        accepted.status.success(),
        "valid second-session signoff must pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&accepted.stdout),
        String::from_utf8_lossy(&accepted.stderr)
    );

    write_candidate(&root, "qa-reviewer", "qa-reviewer", true);
    let same_session = verify_candidate(&root);
    assert!(
        !same_session.status.success(),
        "same-session final approval must be refused\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&same_session.stdout),
        String::from_utf8_lossy(&same_session.stderr)
    );
    assert!(String::from_utf8_lossy(&same_session.stderr)
        .contains("QA attestation final approver must be a different session"));

    write_candidate(
        &root,
        "qa-reviewer",
        "qa-final-approver-second-session",
        false,
    );
    let unreproduced = verify_candidate(&root);
    assert!(
        !unreproduced.status.success(),
        "missing second-session package reproduction must be refused\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&unreproduced.stdout),
        String::from_utf8_lossy(&unreproduced.stderr)
    );
    assert!(String::from_utf8_lossy(&unreproduced.stderr)
        .contains("QA attestation must reproduce the package from a second session"));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn final_owner_gated_signoff_documents_second_session_requirements() {
    let guide = include_str!("../../../docs/testing/hub-release-candidate-vm-gate.md");
    let verifier = include_str!("../../../scripts/verify_hub_vm_qa_attestation.py");

    let section = section(
        guide,
        "## final_owner_gated_signoff_reproduces_package_from_second_session",
        "Validate locally before upload:",
    );
    for required in [
        "Final promotion is a two-person/session gate.",
        "final approver must use a second\nlogin/session to download the draft release assets",
        "second session computes the installer SHA-256 and it exactly matches\n  `candidateSha256`",
        "runs `scripts/verify_hub_vm_qa_attestation.py` against the\n  downloaded candidate directory, not a local build tree",
        "`packageReproducedBySecondSession: true`",
        "release owner and final approver are not the same session",
    ] {
        assert!(
            section.contains(required),
            "release gate guide must require second-session package reproduction: {required}"
        );
    }

    for required in [
        "operator = document.get(\"operator\")",
        "final_approver = document.get(\"finalApprover\")",
        "QA attestation needs a second-session final approver",
        "require(final_approver.strip() != operator.strip(),",
        "QA attestation final approver must be a different session",
        "require(document.get(\"packageReproducedBySecondSession\") is True,",
        "QA attestation must reproduce the package from a second session",
    ] {
        assert!(
            verifier.contains(required),
            "verifier must refuse final sign-off without independent package reproduction: {required}"
        );
    }

    let hash_check = verifier
        .find("require(file_sha256(installers[0]) == expected_hash,")
        .expect("verifier must hash the downloaded installer");
    let manifest_check = verifier
        .find("verify_updater_manifest(tag, candidate_dir / \"latest.json\", installers[0])")
        .expect("verifier must bind latest.json to the same downloaded installer");
    let approver_check = verifier
        .find("final_approver = document.get(\"finalApprover\")")
        .expect("verifier must read the final approver");
    let reproduction_check = verifier
        .find("require(document.get(\"packageReproducedBySecondSession\") is True,")
        .expect("verifier must require second-session reproduction");
    assert!(
        hash_check < manifest_check
            && manifest_check < approver_check
            && approver_check < reproduction_check,
        "final sign-off must happen only after the downloaded package hash and manifest match"
    );
}
