fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = source.find(start).expect("section start exists");
    let end_index = source[start_index..]
        .find(end)
        .map(|offset| start_index + offset)
        .expect("section end exists");
    &source[start_index..end_index]
}

fn offset(source: &str, needle: &str) -> usize {
    source
        .find(needle)
        .expect("expected contract marker exists")
}

fn assert_before(source: &str, earlier: &str, later: &str) {
    let earlier_index = offset(source, earlier);
    let later_index = offset(source, later);
    assert!(
        earlier_index < later_index,
        "expected `{earlier}` before `{later}`"
    );
}

fn count_occurrences(source: &str, needle: &str) -> usize {
    source.match_indices(needle).count()
}

fn assert_refusal_case(block: &str, result_var: &str, case_name: &str, blocked_by: &str) {
    let start = offset(
        block,
        &format!("${result_var} = Invoke-P2PLoopSelfTestChild"),
    );
    let tail = &block[start..];
    let end = tail["Invoke-P2PLoopSelfTestChild".len()..]
        .find("Invoke-P2PLoopSelfTestChild")
        .map(|offset| "Invoke-P2PLoopSelfTestChild".len() + offset)
        .unwrap_or(tail.len());
    let case = &tail[..end];

    assert_eq!(
        count_occurrences(case, "Invoke-P2PLoopSelfTestChild"),
        1,
        "{case_name} must be one child run, so its assertions bind to that run"
    );
    assert!(
        case.find(&format!(
            "${result_var}.overall.blockedBy -cne '{blocked_by}'"
        ))
        .is_some(),
        "{case_name} must refuse at the expected gate"
    );
    assert!(
        case.find(&format!("${result_var}.overall.stepsRun -ne $false"))
            .is_some()
            && case
                .find(&format!("@(${result_var}.steps).Count -ne 0"))
                .is_some(),
        "{case_name} must prove the six measurement steps did not run"
    );
    assert!(
        case.find("Assert-NoSelftestDriveFiles").is_some(),
        "{case_name} must prove refusal did not write a self-test drive request"
    );
}

#[test]
fn b6_controllers_read_the_retained_preflight_before_consent_or_drive() {
    let script = include_str!("../../../scripts/qa/osl-p2p-loop.ps1");

    let precondition_flow = section(
        script,
        "$b6A = Read-B6StartupReceipt -TempRoot $TempRootA -Side 'A'",
        "# G1 the two instances, distinguishable",
    );
    assert_before(
        precondition_flow,
        "$b6A = Read-B6StartupReceipt -TempRoot $TempRootA -Side 'A'",
        "Add-Gate 'b6-preflight'",
    );
    assert_before(
        precondition_flow,
        "$b6B = Read-B6StartupReceipt -TempRoot $TempRootB -Side 'B'",
        "Add-Gate 'b6-preflight'",
    );
    assert_before(
        precondition_flow,
        "Add-Gate 'b6-preflight'",
        "Write-Blocked 'b6-preflight'",
    );
    assert_before(
        precondition_flow,
        "Write-Blocked 'b6-preflight'",
        "Add-Gate 'consent'",
    );
    assert_before(
        precondition_flow,
        "Add-Gate 'consent'",
        "Write-Blocked 'consent'",
    );

    let first_live_drive_write = offset(
        script,
        "Set-Content -LiteralPath $reqA -Value 'osl-p2p-loop' -Encoding ascii -ErrorAction Stop",
    );
    let b6_gate = offset(script, "Add-Gate 'b6-preflight'");
    let consent_gate = offset(script, "Add-Gate 'consent'");
    assert!(
        b6_gate < consent_gate && consent_gate < first_live_drive_write,
        "the live drive request must be unreachable until retained B6 receipts and consent pass"
    );

    let binding = section(
        script,
        "function Test-B6StartupReceiptBinding",
        "function New-B6StartupReceiptForSelfTest",
    );
    for fact in [
        "distinctIdentityAndKeystoreRoots",
        "bidirectionalCiphertextAndPlaintext",
        "offlineEnqueueAndDelivery",
        "persistedRatchetRestart",
        "exactlyOnceDrain",
        "independentPeerAttribution",
        "negativeCrossPeerIsolation",
    ] {
        assert!(
            binding.find(fact).is_some(),
            "retained B6 receipt binding must require runtime fact `{fact}`"
        );
    }
    assert!(
        binding
            .find("$b6.runtime.PSObject.Properties[$fact].Value -ne $true")
            .is_some(),
        "retained B6 runtime facts must be true, not merely present"
    );
    for required_field in [
        "$Receipt.schemaVersion -eq 2",
        "$b6.schemaVersion -eq 2",
        "$b6.startupAllowed -eq $true",
        "$startupBlockers.Count -eq 0",
        "$b6.sourceCommit",
        "$b6.binarySha256",
        "$b6.serverDeploymentIdentity",
        "$null -ne $b6.identityPublicFingerprintsSha256",
        "$null -ne $b6.identityKeystoreRootFingerprintsSha256",
    ] {
        assert!(
            binding.find(required_field).is_some(),
            "retained B6 binding must require `{required_field}`"
        );
    }

    let selftest = section(
        script,
        "function b6_controllers_read_the_retained_preflight_before_consent_or_drive",
        "function b6_startup_receipt_binding_selftest",
    );
    let drive_refusal = section(
        selftest,
        "function Assert-NoSelftestDriveFiles",
        "$missingJson = Join-Path",
    );
    assert!(
        drive_refusal.find("osl-qa-selftest*.request").is_some()
            && drive_refusal.find("$driveFiles.Count -gt 0").is_some()
            && drive_refusal.find("throw").is_some(),
        "the self-test must fail if a refused run writes any self-test request file"
    );

    assert_refusal_case(selftest, "missing", "missing-retained-b6", "b6-preflight");
    assert_refusal_case(
        selftest,
        "oneSided",
        "one-sided-retained-b6",
        "b6-preflight",
    );
    assert_refusal_case(
        selftest,
        "incomplete",
        "incomplete-retained-b6",
        "b6-preflight",
    );
    assert_refusal_case(
        selftest,
        "falseFact",
        "false-fact-retained-b6",
        "b6-preflight",
    );
    assert_refusal_case(selftest, "valid", "valid-b6-no-consent", "consent");
}
