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
    fn write_candidate(root: &std::path::Path) {
        std::fs::write(
            root.join("osl-hub-0.1.0-x64-nsis.exe"),
            b"signed candidate fixture",
        )
        .expect("write candidate installer");
        std::fs::write(
            root.join("latest.json"),
            r#"{"version":"0.1.0","platforms":{"windows-x86_64":{"signature":"signed-update-fixture","url":"https://github.com/OSLPrivacy/discord-privacy-client/releases/download/hub-v0.1.0/osl-hub-0.1.0-x64-nsis.exe"}}}"#,
        )
        .expect("write updater manifest");
    }

    fn write_attestation(root: &std::path::Path, final_approver: &str, reproduced: bool) {
        std::fs::write(
            root.join("hub-vm-qa-attestation.json"),
            format!(
                r#"{{"schemaVersion":1,"candidateTag":"hub-v0.1.0","candidateSha256":"6f44e5bf164984cb7daf83fdcac411e4da8104af8ee7c8128d542def97507e47","completedAtUtc":"2026-07-17T23:00:00Z","operator":"qa-reviewer","finalApprover":"{final_approver}","packageReproducedBySecondSession":{reproduced},"captchaHandling":"paused_for_manual_completion","vms":[{{"name":"A","goldenSnapshotId":"signed-a","cleanRestore":true}},{{"name":"B","goldenSnapshotId":"signed-b","cleanRestore":true}}],"cases":{{"onboarding":true,"identityCreate":true,"identityRecover":true,"twoAccountLogin":true,"persistenceRestart":true,"signedUpdate":true,"oneSidedEncryption":true,"twoSidedEncryption":true,"fullCleanup":true}}}}"#,
            ),
        )
        .expect("write QA attestation");
    }

    fn run_verifier(root: &std::path::Path) -> std::process::Output {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        std::process::Command::new("python3")
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
        "osl-hub-final-owner-gate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create isolated candidate dir");
    write_candidate(&root);

    write_attestation(&root, "qa-final-approver-second-session", true);
    let accepted = run_verifier(&root);
    assert!(
        accepted.status.success(),
        "second-session package reproduction should pass\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&accepted.stdout),
        String::from_utf8_lossy(&accepted.stderr)
    );

    write_attestation(&root, "qa-final-approver-second-session", false);
    let missing_reproduction = run_verifier(&root);
    assert!(!missing_reproduction.status.success());
    assert!(String::from_utf8_lossy(&missing_reproduction.stderr)
        .contains("QA attestation must reproduce the package from a second session"));

    write_attestation(&root, "qa-reviewer", true);
    let same_session = run_verifier(&root);
    assert!(!same_session.status.success());
    assert!(String::from_utf8_lossy(&same_session.stderr)
        .contains("QA attestation final approver must be a different session"));

    std::fs::remove_dir_all(root).expect("remove isolated candidate dir");
}
