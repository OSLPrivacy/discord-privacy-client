const LAUNCHER: &str = include_str!("../../../scripts/qa/osl-launch-instance-b.ps1");

fn section<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start_index = source.find(start).expect("section start exists");
    let end_index = source[start_index..]
        .find(end)
        .map(|offset| start_index + offset)
        .expect("section end exists");
    &source[start_index..end_index]
}

fn offset(source: &str, marker: &str) -> usize {
    source.find(marker).unwrap_or_else(|| {
        panic!("expected launcher contract marker to exist:\n{marker}");
    })
}

fn assert_ordered(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let relative = source[cursor..].find(marker).unwrap_or_else(|| {
            panic!(
                "expected ordered launcher contract marker to exist after byte {cursor}:\n{marker}"
            );
        });
        cursor += relative + marker.len();
    }
}

fn assert_branch_finishes(source: &str, guard: &str, finish: &str) {
    let guarded = section(source, guard, "\n    }");
    offset(guarded, finish);
}

#[test]
fn instance_b_launcher_uses_private_temp_root_and_preserves_instance_a() {
    let fixture = section(
        LAUNCHER,
        "if ($ContractFixtureJson) {",
        "\n$here = Split-Path -Parent $MyInvocation.MyCommand.Path",
    );

    assert_branch_finishes(
        fixture,
        "if ($fixtureTempA -eq $fixtureTempB) {",
        "Finish-Fixture 'blocked' 'instance B must not share instance A temp root'",
    );
    assert_ordered(
        fixture,
        &[
            "if ($fixtureTempA -eq $fixtureTempB) {",
            "New-Item -ItemType Directory -Path $fixtureTempA -Force",
            "New-Item -ItemType Directory -Path $fixtureTempB -Force",
            "Add-FixtureStep 'temp-isolation' 'ok' 'A and B temp roots are separate'",
            "$startupTrace = Join-Path $fixtureTempB 'osl-startup-trace.txt'",
            "'fixture instance B startup trace' | Out-File -LiteralPath $startupTrace",
        ],
    );

    let fixture_launch = section(
        fixture,
        "$startupTrace = Join-Path $fixtureTempB 'osl-startup-trace.txt'",
        "$beforeIdentities = @($fixture.keyserverIdentitiesBefore)",
    );
    offset(fixture_launch, "inheritedTmp = $fixtureTempB");
    offset(fixture_launch, "inheritedTemp = $fixtureTempB");

    let fixture_a_assertion = section(
        fixture,
        "$aUntouched = (",
        "if (-not (Test-Path -LiteralPath $startupTrace))",
    );
    offset(
        fixture_a_assertion,
        "($fixture.instanceAMarkerAfter -eq $true)",
    );
    offset(
        fixture_a_assertion,
        "[string]$fixture.instanceAIdentityShaBefore -eq [string]$fixture.instanceAIdentityShaAfter",
    );
    assert_branch_finishes(
        fixture_a_assertion,
        "if (-not $aUntouched) {",
        "Finish-Fixture 'failed' 'instance A changed across the instance B launch'",
    );
    assert_ordered(
        fixture,
        &[
            "if (-not (Test-Path -LiteralPath $startupTrace)) {",
            "Finish-Fixture 'failed' 'instance B did not write QA artifacts under its private temp root'",
            "Add-FixtureStep 'assert/temp-redirect' 'ok' 'B wrote startup trace under its private temp root'",
            "Finish-Fixture 'ok' 'contract fixture launched distinguishable instance B without touching instance A'",
        ],
    );
    offset(fixture, "tempRoot = $fixtureTempA");
    offset(fixture, "identityUnchangedAcrossLaunch = $true");
    offset(fixture, "touchedByThisScript = $false");
    offset(fixture, "tempRoot = $fixtureTempB");
    offset(fixture, "tempRootHonouredByChild = $true");

    let normal_temp = section(
        LAUNCHER,
        "# --- 3. private temp root and executable-owned B6 preflight",
        "# --- 4. explicit consent after the no-side-effect preflight",
    );
    assert_ordered(
        normal_temp,
        &[
            "$tempA = $env:TEMP",
            "if (-not $TempRootB)",
            "if ($TempRootB -eq $tempA) {",
            "Write-Result 'blocked' `",
            "Add-Step 'temp-isolation' 'ok'",
        ],
    );

    let normal_launch = section(
        LAUNCHER,
        "# --- 7. launch",
        "# --- 7. read back WHICH BUNDLE actually started",
    );
    assert_ordered(
        normal_launch,
        &[
            "$savedTmp = $env:TMP",
            "$savedTemp = $env:TEMP",
            "$env:TMP = $TempRootB",
            "$env:TEMP = $TempRootB",
            "Start-Process -FilePath $ExeB -PassThru",
            "$env:TMP = $savedTmp",
            "$env:TEMP = $savedTemp",
            "Add-Step 'launch' 'ok'",
        ],
    );

    let normal_a_proof = section(
        LAUNCHER,
        "# --- 9. prove A was not disturbed",
        "# --- 10. profile separation, verified on disk",
    );
    offset(
        normal_a_proof,
        "$identityAAfter = Get-P2PFileStamp -Path $identityABefore.path -Label 'instance-a-identity-after'",
    );
    offset(
        normal_a_proof,
        "$identityAUntouched = ($identityABefore.sha256 -eq $identityAAfter.sha256)",
    );
    assert_branch_finishes(
        normal_a_proof,
        "if (-not $aOk) {",
        "Write-Result 'failed' `",
    );
    offset(
        normal_a_proof,
        "Add-Step 'assert/instance-a-untouched' 'ok'",
    );

    let normal_temp_assert = section(
        LAUNCHER,
        "# --- 11b. did the temp redirect actually take?",
        "# --- 11c. B created its own QA identity and public offer",
    );
    assert_ordered(
        normal_temp_assert,
        &[
            "$traceB = Join-Path $TempRootB 'osl-startup-trace.txt'",
            "$traceBStamp = Get-P2PFileStamp -Path $traceB -Label 'instance-b-startup-trace'",
            "$tempHonoured = ($traceBStamp.exists",
            "if (-not $tempHonoured) {",
            "Write-Result 'failed' `",
            "Add-Step 'assert/temp-redirect' 'ok'",
        ],
    );
}

#[test]
fn instance_b_confirm_creates_identity_registers_second_identity() {
    let fixture = section(
        LAUNCHER,
        "if ($ContractFixtureJson) {",
        "\n$here = Split-Path -Parent $MyInvocation.MyCommand.Path",
    );

    assert_ordered(
        fixture,
        &[
            "if (-not ($fixture.preflightStartupAllowed -eq $true)) {",
            "Add-FixtureStep 'gate/b6-preflight' 'ok' 'preflight allowed startup'",
            "if (-not $ConfirmCreatesIdentity) {",
            "Finish-Fixture 'blocked' 'identity creation requires explicit consent'",
            "Add-FixtureStep 'gate/consent' 'ok' 'identity creation consent was present'",
            "$registeredSecondIdentity = (",
        ],
    );
    offset(
        fixture,
        "$afterIdentities.Count -eq ($beforeIdentities.Count + 1)",
    );
    offset(
        fixture,
        "$beforeIdentities -notcontains $afterIdentities[-1]",
    );
    assert_branch_finishes(
        fixture,
        "if (-not $registeredSecondIdentity) {",
        "Finish-Fixture 'failed' 'consented instance B launch did not register exactly one additional identity'",
    );
    offset(fixture, "registeredSecondIdentity = $true");

    let consent_to_launch = section(
        LAUNCHER,
        "# --- 4. explicit consent after the no-side-effect preflight",
        "# --- 7. read back WHICH BUNDLE actually started",
    );
    assert_ordered(
        consent_to_launch,
        &[
            "if (-not $ConfirmCreatesIdentity) {",
            "Write-Result 'blocked' `",
            "Add-Step 'gate/consent' 'ok'",
            "# --- 5. snapshot the desktop BEFORE the launch",
            "# --- 7. launch",
            "Start-Process -FilePath $ExeB -PassThru",
        ],
    );

    let b_identity = section(
        LAUNCHER,
        "# --- 11c. B created its own QA identity and public offer",
        "# --- 11d. B's public identity resolves from the configured keyserver",
    );
    assert_ordered(
        b_identity,
        &[
            "$identityCandidate = Join-Path $coreRoot 'identity.json'",
            "$offerCandidate = Join-Path $coreRoot 'discord-qa-offer.v1.json'",
            "if (-not ($identityBAfter -and $identityBAfter.exists)) {",
            "Write-Result 'failed' `",
            "if ($identityABefore -and $identityABefore.exists -and $identityBAfter.sha256 -eq $identityABefore.sha256) {",
            "Write-Result 'failed' `",
            "if (-not ($offerBAfter -and $offerBAfter.exists)) {",
            "Write-Result 'failed' `",
            "Add-Step 'assert/instance-b-identity' 'ok'",
        ],
    );

    let keyserver = section(
        LAUNCHER,
        "# --- 11d. B's public identity resolves from the configured keyserver",
        "# --- 12. done",
    );
    assert_ordered(
        keyserver,
        &[
            "$offerJson = Get-Content -LiteralPath $offerBAfter.path",
            "$bOslUserId = [string]$offerJson.osl_user_id",
            "$keyserverBaseUrl = Get-KeyserverBaseUrl -CoreRoots $bCoreCandidates",
            "$pubkeysUrl = Join-KeyserverPath -BaseUrl $keyserverBaseUrl -Path ('/v1/pubkeys/{0}' -f [System.Uri]::EscapeDataString($bOslUserId))",
            "Invoke-RestMethod -Method Get -Uri $pubkeysUrl",
            "$registeredFields = @(",
            "$registeredSecondIdentity = (",
            "([string]$pubkeys.user_id -eq $bOslUserId)",
            "if (-not $registeredSecondIdentity) {",
            "Write-Result 'failed' `",
            "Add-Step 'identity/keyserver-registration' 'ok'",
        ],
    );
    for required_field in [
        "$pubkeys.user_id",
        "$pubkeys.registered_at",
        "$pubkeys.ik_x25519_pub",
        "$pubkeys.ik_ed25519_pub",
        "$pubkeys.ik_mlkem768_pub",
        "$pubkeys.registration_sig",
    ] {
        offset(keyserver, required_field);
    }
    offset(keyserver, "resolvedFromPublicOffer = $true");
    offset(keyserver, "requiredPublicFieldsPresent = $true");

    let final_payload = section(LAUNCHER, "# --- 12. done", "\n    }\n");
    offset(final_payload, "registeredSecondIdentity = $true");
}
