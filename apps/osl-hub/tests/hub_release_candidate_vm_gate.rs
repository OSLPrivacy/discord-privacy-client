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
