fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = source.find(start).expect("source section start exists");
    let end_index = source[start_index..]
        .find(end)
        .map(|offset| start_index + offset)
        .expect("source section end exists");
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
