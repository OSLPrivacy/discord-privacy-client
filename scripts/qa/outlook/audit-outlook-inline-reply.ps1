[CmdletBinding()]
param(
    [ValidateSet('Run', 'ValidateEvidence')]
    [string]$Action = 'Run',

    [switch]$ConfirmLocalManualQaRun,

    [AllowNull()]$Evidence,

    [string]$EvidencePath,

    [string]$OutputPath,

    [long]$RunStartedAtUnixMs = -1,

    [long]$RunEndedAtUnixMs = -1,

    [long]$MaxArtifactByteLength = 1048576
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$script:OutlookInlineReplyBenchSchema = 'osl-a11y-bench-evidence-v1'
$script:OutlookInlineReplyBenchId = 'outlook-inline-reply-accessibility'
$script:OutlookInlineReplySurface = 'outlook-inline-reply'

function Get-OutlookInlineReplyUnixMs {
    return [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
}

function Get-OutlookInlineReplyTextSha256([string]$Value) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Value)
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Get-OutlookInlineReplyFileSha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
}

function Refuse-OutlookInlineReplyBench([string]$Reason) {
    throw "OSL_OUTLOOK_INLINE_REPLY_BENCH_REFUSED: $Reason"
}

function Get-OutlookInlineReplyField {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Name)
    if ($null -eq $Value) {
        return $null
    }
    if ($Value -is [System.Collections.IDictionary]) {
        if ($Value.Contains($Name)) {
            return $Value[$Name]
        }
        return $null
    }
    foreach ($property in $Value.PSObject.Properties) {
        if ($property.Name -ceq $Name) {
            return $property.Value
        }
    }
    return $null
}

function Get-OutlookInlineReplyFieldNames {
    param([AllowNull()]$Value)
    if ($null -eq $Value) {
        return @()
    }
    if ($Value -is [System.Collections.IDictionary]) {
        return @($Value.Keys | ForEach-Object { [string]$_ })
    }
    return @($Value.PSObject.Properties | ForEach-Object { $_.Name })
}

function Assert-OutlookInlineReplyExactFields {
    param(
        [AllowNull()]$Value,
        [Parameter(Mandatory)][string[]]$Names,
        [Parameter(Mandatory)][string]$Label
    )
    if ($null -eq $Value) {
        Refuse-OutlookInlineReplyBench $Label
    }
    $actual = @(Get-OutlookInlineReplyFieldNames -Value $Value | Sort-Object)
    $expected = @($Names | Sort-Object)
    if (($actual -join "`n") -cne ($expected -join "`n")) {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplyString {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value)) {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplyLabel {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    Assert-OutlookInlineReplyString -Value $Value -Label $Label
    if ($Value -cnotmatch '^[a-z0-9][a-z0-9.-]{0,127}$') {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplySha256 {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    if ($Value -isnot [string] -or $Value -cnotmatch '^[0-9a-f]{64}$') {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplyUnixMs {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    if (
        $Value -isnot [byte] -and
        $Value -isnot [int16] -and
        $Value -isnot [int32] -and
        $Value -isnot [int64] -and
        $Value -isnot [long]
    ) {
        Refuse-OutlookInlineReplyBench $Label
    }
    if ([long]$Value -lt 0) {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplyWithinRun {
    param(
        [long]$Value,
        [long]$StartedAt,
        [long]$EndedAt,
        [Parameter(Mandatory)][string]$Label
    )
    if ($StartedAt -ge 0 -and $Value -lt $StartedAt) {
        Refuse-OutlookInlineReplyBench $Label
    }
    if ($EndedAt -ge 0 -and $Value -gt $EndedAt) {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplyBooleanTrue {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    if ($Value -isnot [bool] -or $Value -ne $true) {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function Assert-OutlookInlineReplyBooleanFalse {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    if ($Value -isnot [bool] -or $Value -ne $false) {
        Refuse-OutlookInlineReplyBench $Label
    }
}

function ConvertTo-OutlookInlineReplyArray {
    param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
    if ($null -eq $Value) {
        return @()
    }
    if ($Value -is [string] -or $Value -is [System.Collections.IDictionary]) {
        Refuse-OutlookInlineReplyBench $Label
    }
    if ($Value -is [array]) {
        return @($Value)
    }
    Refuse-OutlookInlineReplyBench $Label
}

function Assert-OutlookInlineReplyFacts {
    param([AllowNull()]$Value)
    if ($null -eq $Value) {
        Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
    }
    $names = @(Get-OutlookInlineReplyFieldNames -Value $Value)
    if ($names.Count -lt 1) {
        Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
    }
    foreach ($name in $names) {
        Assert-OutlookInlineReplyString -Value $name -Label 'MEASUREMENTS'
        if ($name -cnotmatch '^[A-Za-z][A-Za-z0-9]{0,63}$') {
            Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
        }
        $fact = Get-OutlookInlineReplyField -Value $Value -Name $name
        if ($fact -is [bool]) {
            continue
        }
        if (
            $fact -is [byte] -or
            $fact -is [int16] -or
            $fact -is [int32] -or
            $fact -is [int64] -or
            $fact -is [single] -or
            $fact -is [double] -or
            $fact -is [decimal]
        ) {
            if ([double]::IsNaN([double]$fact) -or [double]::IsInfinity([double]$fact)) {
                Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
            }
            continue
        }
        Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
    }
}

function Assert-OutlookInlineReplyRelativeArtifactPath {
    param([AllowNull()]$Value)
    Assert-OutlookInlineReplyString -Value $Value -Label 'ARTIFACTS'
    if (
        $Value.StartsWith('/') -or
        $Value.StartsWith('\') -or
        $Value -match '^[A-Za-z]:' -or
        $Value.Contains('\') -or
        $Value.Contains('//')
    ) {
        Refuse-OutlookInlineReplyBench 'ARTIFACTS'
    }
    if ($Value -cnotmatch '^[a-z0-9][a-z0-9._/-]{0,255}$') {
        Refuse-OutlookInlineReplyBench 'ARTIFACTS'
    }
    foreach ($segment in @($Value.Split('/'))) {
        if ([string]::IsNullOrWhiteSpace($segment) -or $segment -ceq '.' -or $segment -ceq '..') {
            Refuse-OutlookInlineReplyBench 'ARTIFACTS'
        }
    }
}

function Test-OutlookInlineReplyVisibleRect {
    param([AllowNull()]$Element)
    if ($null -eq $Element -or $Element.Current.IsOffscreen) {
        return $false
    }
    $rect = $Element.Current.BoundingRectangle
    return (
        -not [double]::IsNaN($rect.Left) -and
        -not [double]::IsNaN($rect.Top) -and
        -not [double]::IsNaN($rect.Width) -and
        -not [double]::IsNaN($rect.Height) -and
        -not [double]::IsInfinity($rect.Left) -and
        -not [double]::IsInfinity($rect.Top) -and
        -not [double]::IsInfinity($rect.Width) -and
        -not [double]::IsInfinity($rect.Height) -and
        $rect.Width -ge 1 -and
        $rect.Height -ge 1
    )
}

function Test-OutlookInlineReplyPattern {
    param([AllowNull()]$Element, [Parameter(Mandatory)]$Pattern)
    if ($null -eq $Element) {
        return $false
    }
    $matched = $null
    return $Element.TryGetCurrentPattern($Pattern, [ref]$matched)
}

function Get-OutlookInlineReplyLiveSummary {
    param([Parameter(Mandatory)][long]$RunStartedAt)

    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        Refuse-OutlookInlineReplyBench 'WINDOWS'
    }

    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes

    $processes = @(Get-Process -Name olk -ErrorAction SilentlyContinue | Where-Object {
        try {
            $_.Path -and ([IO.Path]::GetFileName($_.Path) -ieq 'olk.exe')
        } catch {
            $false
        }
    })
    if ($processes.Count -ne 1) {
        Refuse-OutlookInlineReplyBench 'SUBJECT'
    }

    $process = $processes[0]
    $exeSha = Get-OutlookInlineReplyFileSha256 $process.Path
    $root = [Windows.Automation.AutomationElement]::RootElement
    $windows = @($root.FindAll(
        [Windows.Automation.TreeScope]::Children,
        [Windows.Automation.Condition]::TrueCondition
    ) | Where-Object {
        $_.Current.ProcessId -eq $process.Id -and (Test-OutlookInlineReplyVisibleRect $_)
    })
    if ($windows.Count -ne 1) {
        Refuse-OutlookInlineReplyBench 'BINDING'
    }
    $window = $windows[0]
    $bindingObservedAt = Get-OutlookInlineReplyUnixMs
    $bindingMaterial = '{0}|{1}|{2}|{3}' -f $exeSha, $process.Id, $process.StartTime.ToUniversalTime().Ticks, $script:OutlookInlineReplySurface
    $bindingSha = Get-OutlookInlineReplyTextSha256 $bindingMaterial

    $replyInputCount = 0
    $valuePatternInputs = 0
    $textPatternInputs = 0
    $commandButtonCount = 0
    $offscreenRejected = 0
    $descendantCount = 0

    $inputCondition = [Windows.Automation.OrCondition]::new([Windows.Automation.Condition[]]@(
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Edit
        ),
        [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ControlTypeProperty,
            [Windows.Automation.ControlType]::Document
        )
    ))
    $inputs = @($window.FindAll([Windows.Automation.TreeScope]::Descendants, $inputCondition))
    foreach ($input in $inputs) {
        $descendantCount += 1
        if (-not (Test-OutlookInlineReplyVisibleRect $input)) {
            $offscreenRejected += 1
            continue
        }
        $hasValue = Test-OutlookInlineReplyPattern $input ([Windows.Automation.ValuePattern]::Pattern)
        $hasText = Test-OutlookInlineReplyPattern $input ([Windows.Automation.TextPattern]::Pattern)
        if (-not $hasValue -and -not $hasText) {
            continue
        }
        $replyInputCount += 1
        if ($hasValue) {
            $valuePatternInputs += 1
        }
        if ($hasText) {
            $textPatternInputs += 1
        }
    }

    $buttonCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Button
    )
    $buttons = @($window.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition))
    foreach ($button in $buttons) {
        $descendantCount += 1
        if (-not (Test-OutlookInlineReplyVisibleRect $button)) {
            $offscreenRejected += 1
            continue
        }
        $name = [string]$button.Current.Name
        if ($name -cmatch '^(Send|Discard|Pop out)$') {
            $commandButtonCount += 1
        }
    }
    $measuredAt = Get-OutlookInlineReplyUnixMs

    return [pscustomobject][ordered]@{
        runId = [Guid]::NewGuid().ToString()
        createdAtUnixMs = $measuredAt
        consentObservedAtUnixMs = $RunStartedAt
        bindingObservedAtUnixMs = $bindingObservedAt
        authorityObservedAtUnixMs = $measuredAt
        appBuildSha256 = Get-OutlookInlineReplyFileSha256 $PSCommandPath
        executableSha256 = $exeSha
        subjectBindingSha256 = $bindingSha
        outlookWindowCount = $windows.Count
        newOutlookProcessCount = $processes.Count
        inlineReplyTextInputs = $replyInputCount
        inlineReplyCommandButtons = $commandButtonCount
        valuePatternInputs = $valuePatternInputs
        textPatternInputs = $textPatternInputs
        offscreenRejected = $offscreenRejected
        unsupportedProcessCount = 0
        inspectedAccessibleControls = $descendantCount
    }
}

function New-OutlookInlineReplyBenchEvidence {
    [CmdletBinding()]
    param([Parameter(Mandatory)]$Summary)

    $createdAt = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'createdAtUnixMs')
    $boundSurfacePass = (
        [long](Get-OutlookInlineReplyField -Value $Summary -Name 'outlookWindowCount') -eq 1 -and
        [long](Get-OutlookInlineReplyField -Value $Summary -Name 'newOutlookProcessCount') -eq 1
    )
    $inlineReplyPass = (
        [long](Get-OutlookInlineReplyField -Value $Summary -Name 'inlineReplyTextInputs') -ge 1 -and
        [long](Get-OutlookInlineReplyField -Value $Summary -Name 'inlineReplyCommandButtons') -ge 1 -and
        (
            [long](Get-OutlookInlineReplyField -Value $Summary -Name 'valuePatternInputs') +
            [long](Get-OutlookInlineReplyField -Value $Summary -Name 'textPatternInputs')
        ) -ge 1
    )
    $boundSurfaceStatus = if ($boundSurfacePass) { 'pass' } else { 'fail' }
    $inlineReplyStatus = if ($inlineReplyPass) { 'pass' } else { 'fail' }
    $declaredVerdict = if ($boundSurfacePass -and $inlineReplyPass) { 'pass' } else { 'fail' }

    $record = [pscustomobject][ordered]@{
        schema = $script:OutlookInlineReplyBenchSchema
        benchId = $script:OutlookInlineReplyBenchId
        runId = [string](Get-OutlookInlineReplyField -Value $Summary -Name 'runId')
        createdAtUnixMs = $createdAt
        subject = [pscustomobject][ordered]@{
            platform = 'windows'
            appBuildSha256 = [string](Get-OutlookInlineReplyField -Value $Summary -Name 'appBuildSha256')
            executableSha256 = [string](Get-OutlookInlineReplyField -Value $Summary -Name 'executableSha256')
            surface = $script:OutlookInlineReplySurface
        }
        consent = [pscustomobject][ordered]@{
            granted = $true
            source = 'local-manual-qa-run'
            observedAtUnixMs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'consentObservedAtUnixMs')
        }
        binding = [pscustomobject][ordered]@{
            method = 'existing-new-outlook-process'
            subjectBindingSha256 = [string](Get-OutlookInlineReplyField -Value $Summary -Name 'subjectBindingSha256')
            observedAtUnixMs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'bindingObservedAtUnixMs')
        }
        authority = [pscustomobject][ordered]@{
            collector = 'windows-uia-read-only'
            verifier = 'outlook-inline-reply-bench'
            observedAtUnixMs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'authorityObservedAtUnixMs')
        }
        measurements = @(
            [pscustomobject][ordered]@{
                name = 'outlook-bound-surface'
                status = $boundSurfaceStatus
                observedAtUnixMs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'bindingObservedAtUnixMs')
                facts = [pscustomobject][ordered]@{
                    outlookWindowCount = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'outlookWindowCount')
                    newOutlookProcessCount = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'newOutlookProcessCount')
                    unsupportedProcessCount = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'unsupportedProcessCount')
                }
            },
            [pscustomobject][ordered]@{
                name = 'inline-reply-a11y'
                status = $inlineReplyStatus
                observedAtUnixMs = $createdAt
                facts = [pscustomobject][ordered]@{
                    inlineReplyTextInputs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'inlineReplyTextInputs')
                    inlineReplyCommandButtons = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'inlineReplyCommandButtons')
                    valuePatternInputs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'valuePatternInputs')
                    textPatternInputs = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'textPatternInputs')
                    offscreenRejected = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'offscreenRejected')
                    inspectedAccessibleControls = [long](Get-OutlookInlineReplyField -Value $Summary -Name 'inspectedAccessibleControls')
                }
            }
        )
        artifacts = @()
        privacy = [pscustomobject][ordered]@{
            noRawText = $true
            noAccountIdentifiers = $true
            noSecrets = $true
            boundedArtifacts = $true
        }
        verdict = $declaredVerdict
    }
    return Test-OutlookInlineReplyBenchEvidence -Evidence $record
}

function Test-OutlookInlineReplyBenchEvidence {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Evidence,
        [long]$RunStartedAtUnixMs = -1,
        [long]$RunEndedAtUnixMs = -1,
        [long]$MaxArtifactByteLength = 1048576
    )

    Assert-OutlookInlineReplyExactFields -Value $Evidence -Names @(
        'schema',
        'benchId',
        'runId',
        'createdAtUnixMs',
        'subject',
        'consent',
        'binding',
        'authority',
        'measurements',
        'artifacts',
        'privacy',
        'verdict'
    ) -Label 'FIELDS'

    if ((Get-OutlookInlineReplyField -Value $Evidence -Name 'schema') -cne $script:OutlookInlineReplyBenchSchema) {
        Refuse-OutlookInlineReplyBench 'SCHEMA'
    }
    if ((Get-OutlookInlineReplyField -Value $Evidence -Name 'benchId') -cne $script:OutlookInlineReplyBenchId) {
        Refuse-OutlookInlineReplyBench 'BENCH'
    }
    $runId = Get-OutlookInlineReplyField -Value $Evidence -Name 'runId'
    Assert-OutlookInlineReplyString -Value $runId -Label 'RUN'
    if ($runId -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$') {
        Refuse-OutlookInlineReplyBench 'RUN'
    }
    $createdAt = Get-OutlookInlineReplyField -Value $Evidence -Name 'createdAtUnixMs'
    Assert-OutlookInlineReplyUnixMs -Value $createdAt -Label 'TIME'
    Assert-OutlookInlineReplyWithinRun -Value ([long]$createdAt) -StartedAt $RunStartedAtUnixMs -EndedAt $RunEndedAtUnixMs -Label 'TIME'

    $subject = Get-OutlookInlineReplyField -Value $Evidence -Name 'subject'
    Assert-OutlookInlineReplyExactFields -Value $subject -Names @(
        'platform',
        'appBuildSha256',
        'executableSha256',
        'surface'
    ) -Label 'SUBJECT'
    if ((Get-OutlookInlineReplyField -Value $subject -Name 'platform') -cne 'windows') {
        Refuse-OutlookInlineReplyBench 'SUBJECT'
    }
    Assert-OutlookInlineReplySha256 -Value (Get-OutlookInlineReplyField -Value $subject -Name 'appBuildSha256') -Label 'SUBJECT'
    Assert-OutlookInlineReplySha256 -Value (Get-OutlookInlineReplyField -Value $subject -Name 'executableSha256') -Label 'SUBJECT'
    if ((Get-OutlookInlineReplyField -Value $subject -Name 'surface') -cne $script:OutlookInlineReplySurface) {
        Refuse-OutlookInlineReplyBench 'SUBJECT'
    }

    $consent = Get-OutlookInlineReplyField -Value $Evidence -Name 'consent'
    Assert-OutlookInlineReplyExactFields -Value $consent -Names @(
        'granted',
        'source',
        'observedAtUnixMs'
    ) -Label 'CONSENT'
    Assert-OutlookInlineReplyBooleanTrue -Value (Get-OutlookInlineReplyField -Value $consent -Name 'granted') -Label 'CONSENT'
    Assert-OutlookInlineReplyLabel -Value (Get-OutlookInlineReplyField -Value $consent -Name 'source') -Label 'CONSENT'
    $consentAt = Get-OutlookInlineReplyField -Value $consent -Name 'observedAtUnixMs'
    Assert-OutlookInlineReplyUnixMs -Value $consentAt -Label 'CONSENT'
    Assert-OutlookInlineReplyWithinRun -Value ([long]$consentAt) -StartedAt $RunStartedAtUnixMs -EndedAt $RunEndedAtUnixMs -Label 'CONSENT'

    $binding = Get-OutlookInlineReplyField -Value $Evidence -Name 'binding'
    Assert-OutlookInlineReplyExactFields -Value $binding -Names @(
        'method',
        'subjectBindingSha256',
        'observedAtUnixMs'
    ) -Label 'BINDING'
    Assert-OutlookInlineReplyLabel -Value (Get-OutlookInlineReplyField -Value $binding -Name 'method') -Label 'BINDING'
    Assert-OutlookInlineReplySha256 -Value (Get-OutlookInlineReplyField -Value $binding -Name 'subjectBindingSha256') -Label 'BINDING'
    $bindingAt = Get-OutlookInlineReplyField -Value $binding -Name 'observedAtUnixMs'
    Assert-OutlookInlineReplyUnixMs -Value $bindingAt -Label 'BINDING'
    Assert-OutlookInlineReplyWithinRun -Value ([long]$bindingAt) -StartedAt $RunStartedAtUnixMs -EndedAt $RunEndedAtUnixMs -Label 'BINDING'

    $authority = Get-OutlookInlineReplyField -Value $Evidence -Name 'authority'
    Assert-OutlookInlineReplyExactFields -Value $authority -Names @(
        'collector',
        'verifier',
        'observedAtUnixMs'
    ) -Label 'AUTHORITY'
    Assert-OutlookInlineReplyLabel -Value (Get-OutlookInlineReplyField -Value $authority -Name 'collector') -Label 'AUTHORITY'
    Assert-OutlookInlineReplyLabel -Value (Get-OutlookInlineReplyField -Value $authority -Name 'verifier') -Label 'AUTHORITY'
    $authorityAt = Get-OutlookInlineReplyField -Value $authority -Name 'observedAtUnixMs'
    Assert-OutlookInlineReplyUnixMs -Value $authorityAt -Label 'AUTHORITY'
    Assert-OutlookInlineReplyWithinRun -Value ([long]$authorityAt) -StartedAt $RunStartedAtUnixMs -EndedAt $RunEndedAtUnixMs -Label 'AUTHORITY'

    $measurements = ConvertTo-OutlookInlineReplyArray -Value (Get-OutlookInlineReplyField -Value $Evidence -Name 'measurements') -Label 'MEASUREMENTS'
    if ($measurements.Count -lt 1) {
        Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
    }
    $hasFail = $false
    $firstMeasurementAt = $null
    foreach ($measurement in $measurements) {
        Assert-OutlookInlineReplyExactFields -Value $measurement -Names @(
            'name',
            'status',
            'observedAtUnixMs',
            'facts'
        ) -Label 'MEASUREMENTS'
        Assert-OutlookInlineReplyLabel -Value (Get-OutlookInlineReplyField -Value $measurement -Name 'name') -Label 'MEASUREMENTS'
        $status = Get-OutlookInlineReplyField -Value $measurement -Name 'status'
        if ($status -cne 'pass' -and $status -cne 'fail') {
            Refuse-OutlookInlineReplyBench 'MEASUREMENTS'
        }
        if ($status -ceq 'fail') {
            $hasFail = $true
        }
        $observedAt = Get-OutlookInlineReplyField -Value $measurement -Name 'observedAtUnixMs'
        Assert-OutlookInlineReplyUnixMs -Value $observedAt -Label 'MEASUREMENTS'
        Assert-OutlookInlineReplyWithinRun -Value ([long]$observedAt) -StartedAt $RunStartedAtUnixMs -EndedAt $RunEndedAtUnixMs -Label 'MEASUREMENTS'
        if ($null -eq $firstMeasurementAt -or [long]$observedAt -lt [long]$firstMeasurementAt) {
            $firstMeasurementAt = [long]$observedAt
        }
        Assert-OutlookInlineReplyFacts -Value (Get-OutlookInlineReplyField -Value $measurement -Name 'facts')
    }
    if ([long]$bindingAt -gt [long]$firstMeasurementAt) {
        Refuse-OutlookInlineReplyBench 'BINDING'
    }

    $artifacts = ConvertTo-OutlookInlineReplyArray -Value (Get-OutlookInlineReplyField -Value $Evidence -Name 'artifacts') -Label 'ARTIFACTS'
    foreach ($artifact in $artifacts) {
        Assert-OutlookInlineReplyExactFields -Value $artifact -Names @(
            'kind',
            'relativePath',
            'sha256',
            'byteLength',
            'containsUserContent'
        ) -Label 'ARTIFACTS'
        Assert-OutlookInlineReplyLabel -Value (Get-OutlookInlineReplyField -Value $artifact -Name 'kind') -Label 'ARTIFACTS'
        Assert-OutlookInlineReplyRelativeArtifactPath -Value (Get-OutlookInlineReplyField -Value $artifact -Name 'relativePath')
        Assert-OutlookInlineReplySha256 -Value (Get-OutlookInlineReplyField -Value $artifact -Name 'sha256') -Label 'ARTIFACTS'
        $byteLength = Get-OutlookInlineReplyField -Value $artifact -Name 'byteLength'
        Assert-OutlookInlineReplyUnixMs -Value $byteLength -Label 'ARTIFACTS'
        if ([long]$byteLength -gt $MaxArtifactByteLength) {
            Refuse-OutlookInlineReplyBench 'ARTIFACTS'
        }
        Assert-OutlookInlineReplyBooleanFalse -Value (Get-OutlookInlineReplyField -Value $artifact -Name 'containsUserContent') -Label 'ARTIFACTS'
    }

    $privacy = Get-OutlookInlineReplyField -Value $Evidence -Name 'privacy'
    Assert-OutlookInlineReplyExactFields -Value $privacy -Names @(
        'noRawText',
        'noAccountIdentifiers',
        'noSecrets',
        'boundedArtifacts'
    ) -Label 'PRIVACY'
    Assert-OutlookInlineReplyBooleanTrue -Value (Get-OutlookInlineReplyField -Value $privacy -Name 'noRawText') -Label 'PRIVACY'
    Assert-OutlookInlineReplyBooleanTrue -Value (Get-OutlookInlineReplyField -Value $privacy -Name 'noAccountIdentifiers') -Label 'PRIVACY'
    Assert-OutlookInlineReplyBooleanTrue -Value (Get-OutlookInlineReplyField -Value $privacy -Name 'noSecrets') -Label 'PRIVACY'
    Assert-OutlookInlineReplyBooleanTrue -Value (Get-OutlookInlineReplyField -Value $privacy -Name 'boundedArtifacts') -Label 'PRIVACY'

    $declaredVerdict = Get-OutlookInlineReplyField -Value $Evidence -Name 'verdict'
    if ($declaredVerdict -cne 'pass' -and $declaredVerdict -cne 'fail' -and $declaredVerdict -cne 'refused') {
        Refuse-OutlookInlineReplyBench 'VERDICT'
    }
    $computedVerdict = if ($hasFail) { 'fail' } else { 'pass' }
    if ($declaredVerdict -ceq 'refused') {
        $computedVerdict = 'refused'
    } elseif ($declaredVerdict -cne $computedVerdict) {
        Refuse-OutlookInlineReplyBench 'VERDICT'
    }

    if ($Evidence -is [System.Collections.IDictionary]) {
        $Evidence['verdict'] = $computedVerdict
    } else {
        $Evidence.verdict = $computedVerdict
    }
    return $Evidence
}

function Write-OutlookInlineReplyEvidence {
    param([Parameter(Mandatory)]$Record, [Parameter(Mandatory)][string]$Path)
    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        [void](New-Item -ItemType Directory -Force -Path $parent)
    }
    $json = ($Record | ConvertTo-Json -Depth 40) + [Environment]::NewLine
    [IO.File]::WriteAllText($Path, $json, [Text.UTF8Encoding]::new($false))
}

function OutlookInlineReplyBench {
    [CmdletBinding()]
    param(
        [ValidateSet('Run', 'ValidateEvidence')]
        [string]$Action = 'Run',

        [switch]$ConfirmLocalManualQaRun,

        [AllowNull()]$Evidence,

        [string]$EvidencePath,

        [string]$OutputPath,

        [long]$RunStartedAtUnixMs = -1,

        [long]$RunEndedAtUnixMs = -1,

        [long]$MaxArtifactByteLength = 1048576
    )

    if ($Action -ceq 'ValidateEvidence') {
        $record = $Evidence
        if ($null -eq $record) {
            if ([string]::IsNullOrWhiteSpace($EvidencePath)) {
                Refuse-OutlookInlineReplyBench 'EVIDENCE'
            }
            $record = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $EvidencePath).ProviderPath, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json
        }
        $validated = Test-OutlookInlineReplyBenchEvidence `
            -Evidence $record `
            -RunStartedAtUnixMs $RunStartedAtUnixMs `
            -RunEndedAtUnixMs $RunEndedAtUnixMs `
            -MaxArtifactByteLength $MaxArtifactByteLength
        if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
            Write-OutlookInlineReplyEvidence -Record $validated -Path $OutputPath
        }
        return $validated
    }

    if (-not $ConfirmLocalManualQaRun) {
        Refuse-OutlookInlineReplyBench 'CONSENT'
    }
    $startedAt = Get-OutlookInlineReplyUnixMs
    $summary = Get-OutlookInlineReplyLiveSummary -RunStartedAt $startedAt
    $record = New-OutlookInlineReplyBenchEvidence -Summary $summary
    $endedAt = Get-OutlookInlineReplyUnixMs
    $record = Test-OutlookInlineReplyBenchEvidence `
        -Evidence $record `
        -RunStartedAtUnixMs $startedAt `
        -RunEndedAtUnixMs $endedAt `
        -MaxArtifactByteLength $MaxArtifactByteLength
    if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
        Write-OutlookInlineReplyEvidence -Record $record -Path $OutputPath
    }
    return $record
}

if ($MyInvocation.InvocationName -cne '.') {
    OutlookInlineReplyBench @PSBoundParameters
}
