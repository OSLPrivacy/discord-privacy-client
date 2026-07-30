$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'audit-outlook-inline-reply.ps1')

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
    param([string]$Name, [string]$Reason, [scriptblock]$Action)
    try {
        & $Action
        Fail $Name 'unexpected acceptance'
    } catch {
        if ($_.Exception.Message -match $Reason) {
            Pass $Name
        } else {
            Fail $Name $_.Exception.Message
        }
    }
}

function Copy-Value($Value) {
    return ($Value | ConvertTo-Json -Compress -Depth 40 | ConvertFrom-Json)
}

function New-PositiveOutlookSummary {
    return [pscustomobject][ordered]@{
        runId = '018f2c18-59e3-7cc6-98cf-6f3132439850'
        createdAtUnixMs = 1772110800000
        consentObservedAtUnixMs = 1772110799000
        bindingObservedAtUnixMs = 1772110799500
        authorityObservedAtUnixMs = 1772110799900
        appBuildSha256 = ('a' * 64)
        executableSha256 = ('b' * 64)
        subjectBindingSha256 = ('c' * 64)
        outlookWindowCount = 1
        newOutlookProcessCount = 1
        inlineReplyTextInputs = 1
        inlineReplyCommandButtons = 2
        valuePatternInputs = 1
        textPatternInputs = 0
        offscreenRejected = 3
        unsupportedProcessCount = 0
        inspectedAccessibleControls = 6
    }
}

function New-PositiveOutlookRecord {
    return New-OutlookInlineReplyBenchEvidence -Summary (New-PositiveOutlookSummary)
}

Expect-Pass 'defines OutlookInlineReplyBench entrypoint' {
    if ($null -eq (Get-Command OutlookInlineReplyBench -ErrorAction SilentlyContinue)) {
        throw 'entrypoint missing'
    }
}

Expect-Pass 'builds passing inline reply bench evidence' {
    $record = New-PositiveOutlookRecord
    if ($record.schema -cne 'osl-a11y-bench-evidence-v1') {
        throw 'wrong schema'
    }
    if ($record.benchId -cne 'outlook-inline-reply-accessibility') {
        throw 'wrong bench'
    }
    if ($record.verdict -cne 'pass') {
        throw 'positive record did not pass'
    }
    $json = $record | ConvertTo-Json -Depth 40
    if ($json -cmatch '(rawText|accountIdentifier|credential|secret|windowHandle|hwnd)') {
        throw 'record exposed a banned diagnostic field'
    }
}

Expect-Pass 'validates evidence through bench entrypoint' {
    $record = OutlookInlineReplyBench `
        -Action ValidateEvidence `
        -Evidence (Copy-Value (New-PositiveOutlookRecord)) `
        -RunStartedAtUnixMs 1772110798000 `
        -RunEndedAtUnixMs 1772110801000
    if ($record.verdict -cne 'pass') {
        throw 'entrypoint validation did not pass'
    }
}

Expect-Pass 'writes validated metadata json' {
    $path = Join-Path ([IO.Path]::GetTempPath()) (
        'osl-outlook-inline-reply-' + [guid]::NewGuid().ToString('N') + '.json'
    )
    try {
        [void](OutlookInlineReplyBench `
            -Action ValidateEvidence `
            -Evidence (Copy-Value (New-PositiveOutlookRecord)) `
            -OutputPath $path)
        $written = [IO.File]::ReadAllText($path, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json
        if ($written.verdict -cne 'pass') {
            throw 'written record did not pass'
        }
    } finally {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Force
        }
    }
}

Expect-Refusal 'live run refuses without explicit consent' '^OSL_OUTLOOK_INLINE_REPLY_BENCH_REFUSED: CONSENT$' {
    OutlookInlineReplyBench -Action Run
}

Expect-Refusal 'refuses missing consent' 'CONSENT|FIELDS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.PSObject.Properties.Remove('consent')
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses denied consent' 'CONSENT' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.consent.granted = $false
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses missing binding' 'BINDING|FIELDS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.PSObject.Properties.Remove('binding')
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses late binding' 'BINDING' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.binding.observedAtUnixMs = 1772110800100
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses missing authority' 'AUTHORITY|FIELDS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.authority.PSObject.Properties.Remove('collector')
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses unknown raw text field' 'FIELDS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record | Add-Member -NotePropertyName rawText -NotePropertyValue 'message body'
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses string measurement facts' 'MEASUREMENTS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $measurement = @($record.measurements)[1]
    $measurement.facts.inlineReplyTextInputs = '1'
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses scalar measurement object' 'MEASUREMENTS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.measurements = @($record.measurements)[0]
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses false privacy flag' 'PRIVACY' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.privacy.noAccountIdentifiers = $false
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses unbounded artifact' 'ARTIFACTS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.artifacts = @(
        [pscustomobject][ordered]@{
            kind = 'structured-json'
            relativePath = 'C:\Users\owner\facts.json'
            sha256 = ('d' * 64)
            byteLength = 200
            containsUserContent = $false
        }
    )
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Refusal 'refuses user content artifact' 'ARTIFACTS' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.artifacts = @(
        [pscustomobject][ordered]@{
            kind = 'structured-json'
            relativePath = 'outlook/facts.json'
            sha256 = ('d' * 64)
            byteLength = 200
            containsUserContent = $true
        }
    )
    OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
}

Expect-Pass 'failing inline reply measurement reduces to fail' {
    $summary = New-PositiveOutlookSummary
    $summary.inlineReplyTextInputs = 0
    $record = New-OutlookInlineReplyBenchEvidence -Summary $summary
    if ($record.verdict -cne 'fail') {
        throw 'inline reply violation did not reduce to fail'
    }
    $validated = OutlookInlineReplyBench -Action ValidateEvidence -Evidence (Copy-Value $record)
    if ($validated.verdict -cne 'fail') {
        throw 'validated failed measurement was not fail'
    }
}

Expect-Pass 'does not upgrade an existing refusal' {
    $record = Copy-Value (New-PositiveOutlookRecord)
    $record.verdict = 'refused'
    $validated = OutlookInlineReplyBench -Action ValidateEvidence -Evidence $record
    if ($validated.verdict -cne 'refused') {
        throw 'existing refusal was upgraded'
    }
}

Write-Output ("passed={0} failed={1}" -f $script:Passed, $script:Failed)
if ($script:Failed -ne 0) {
    exit 1
}
