$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'capture.ps1')

$script:Passed = 0
$script:Failed = 0

function Pass([string]$Name) {
    $script:Passed += 1
    Write-Output ("ok - " + $Name)
}

function Fail([string]$Name, [string]$Reason) {
    $script:Failed += 1
    Write-Output ("not ok - " + $Name + ": " + $Reason)
}

function Expect-Pass {
    param([string]$Name, [scriptblock]$Action)
    try {
        & $Action
        Pass $Name
    } catch {
        Fail $Name $_.Exception.Message
    }
}

function Expect-Refusal {
    param([string]$Name, [scriptblock]$Action)
    try {
        & $Action
        Fail $Name 'unexpected acceptance'
    } catch {
        if ($_.Exception.Message -match '^OSL_A11Y_BENCH_REFUSED: ') {
            Pass $Name
        } else {
            Fail $Name $_.Exception.Message
        }
    }
}

function Copy-Value($Value) {
    return ($Value | ConvertTo-Json -Compress -Depth 40 | ConvertFrom-Json)
}

function New-PositiveCaptureInput {
    return @{
        BenchId = 'native-visible-row-accessibility'
        RunId = '018f2c18-59e3-7cc6-98cf-6f3132439850'
        CreatedAtUnixMs = 1772110800000
        Subject = [pscustomobject][ordered]@{
            platform = 'windows'
            appBuildSha256 = ('a' * 64)
            executableSha256 = ('b' * 64)
            surface = 'conversation-view'
        }
        Consent = [pscustomobject][ordered]@{
            granted = $true
            source = 'local-manual-qa-run'
            observedAtUnixMs = 1772110799000
        }
        Binding = [pscustomobject][ordered]@{
            method = 'launched-subject-process'
            subjectBindingSha256 = ('c' * 64)
            observedAtUnixMs = 1772110799500
        }
        Authority = [pscustomobject][ordered]@{
            collector = 'windows-uia-read-only'
            verifier = 'independent-host-gate'
            observedAtUnixMs = 1772110800100
        }
        Measurements = @(
            [pscustomobject][ordered]@{
                name = 'visible-row-count'
                status = 'pass'
                observedAtUnixMs = 1772110800200
                facts = [pscustomobject][ordered]@{
                    rowsObserved = 3
                    ownRows = 1
                    peerRows = 1
                    unknownRows = 1
                }
            }
        )
        Artifacts = @(
            [pscustomobject][ordered]@{
                kind = 'structured-json'
                relativePath = 'a11ybench/facts.json'
                sha256 = ('d' * 64)
                byteLength = 2048
                containsUserContent = $false
            }
        )
    }
}

function New-PositiveRecord {
    $parameters = New-PositiveCaptureInput
    return Invoke-OslA11yBenchCapture @parameters
}

Expect-Pass 'accepts complete positive record' {
    $record = New-PositiveRecord
    if ($record.schema -cne 'osl-a11y-bench-evidence-v1') {
        throw 'wrong schema'
    }
    if ($record.verdict -cne 'pass') {
        throw 'positive record did not pass'
    }
    if (
        $record.privacy.noRawText -ne $true -or
        $record.privacy.noAccountIdentifiers -ne $true -or
        $record.privacy.noSecrets -ne $true -or
        $record.privacy.boundedArtifacts -ne $true
    ) {
        throw 'privacy flags were not hard true'
    }
}

Expect-Pass 'captures metadata only as exact v1 json' {
    $parameters = New-PositiveCaptureInput
    $json = Invoke-OslA11yBenchCapture @parameters -AsJson
    $record = $json | ConvertFrom-Json
    $topLevel = @($record.PSObject.Properties.Name | Sort-Object)
    $expected = @(
        'artifacts',
        'authority',
        'benchId',
        'binding',
        'consent',
        'createdAtUnixMs',
        'measurements',
        'privacy',
        'runId',
        'schema',
        'subject',
        'verdict'
    ) | Sort-Object
    if (($topLevel -join "`n") -cne ($expected -join "`n")) {
        throw 'top-level fields were not exact'
    }
    $measurement = @($record.measurements)[0]
    foreach ($fact in $measurement.facts.PSObject.Properties) {
        if ($fact.Value -is [string]) {
            throw 'measurement fact carried string content'
        }
    }
    if ($json -cmatch '"(rawText|accountIdentifier|credential|secret|windowHandle)"') {
        throw 'metadata output contained a banned diagnostic field'
    }
}

Expect-Pass 'writes metadata json output when requested' {
    $parameters = New-PositiveCaptureInput
    $path = Join-Path ([IO.Path]::GetTempPath()) (
        'osl-a11ybench-' + [guid]::NewGuid().ToString('N') + '.json'
    )
    try {
        [void](Invoke-OslA11yBenchCapture @parameters -OutputPath $path)
        $record = [IO.File]::ReadAllText($path) | ConvertFrom-Json
        if ($record.verdict -cne 'pass') {
            throw 'written record did not pass'
        }
    } finally {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force
        }
    }
}

Expect-Refusal 'refuses missing consent' {
    $record = Copy-Value (New-PositiveRecord)
    $record.PSObject.Properties.Remove('consent')
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses denied consent' {
    $record = Copy-Value (New-PositiveRecord)
    $record.consent.granted = $false
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses missing binding' {
    $record = Copy-Value (New-PositiveRecord)
    $record.PSObject.Properties.Remove('binding')
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses late binding' {
    $record = Copy-Value (New-PositiveRecord)
    $record.binding.observedAtUnixMs = 1772110800300
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses missing authority' {
    $record = Copy-Value (New-PositiveRecord)
    $record.authority.PSObject.Properties.Remove('collector')
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses unbounded artifact' {
    $record = Copy-Value (New-PositiveRecord)
    $artifact = @($record.artifacts)[0]
    $artifact.relativePath = 'C:\Users\owner\facts.json'
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses user content' {
    $record = Copy-Value (New-PositiveRecord)
    $artifact = @($record.artifacts)[0]
    $artifact.containsUserContent = $true
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses false privacy flag' {
    $record = Copy-Value (New-PositiveRecord)
    $record.privacy.noRawText = $false
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Refusal 'refuses unknown fields' {
    $record = Copy-Value (New-PositiveRecord)
    $record | Add-Member -NotePropertyName rawText -NotePropertyValue 'sample'
    Invoke-OslA11yBenchCapture -Evidence $record
}

Expect-Pass 'fails measured violation' {
    $record = Copy-Value (New-PositiveRecord)
    $measurement = @($record.measurements)[0]
    $measurement.status = 'fail'
    $record.verdict = 'fail'
    $result = Invoke-OslA11yBenchCapture -Evidence $record
    if ($result.verdict -cne 'fail') {
        throw 'failed measurement did not reduce to fail'
    }
}

Expect-Pass 'does not upgrade existing refusal' {
    $record = Copy-Value (New-PositiveRecord)
    $record.verdict = 'refused'
    $result = Invoke-OslA11yBenchCapture -Evidence $record
    if ($result.verdict -cne 'refused') {
        throw 'existing refusal was upgraded'
    }
}

if ($script:Failed -gt 0) {
    throw "$script:Failed a11ybench contract test(s) failed"
}

Write-Output "$script:Passed a11ybench contract test(s) passed"
