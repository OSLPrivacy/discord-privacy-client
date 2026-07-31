$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Invoke-OslA11yBenchCapture {
    [CmdletBinding()]
    param(
        [AllowNull()]$Evidence,
        [AllowNull()][string]$BenchId,
        [AllowNull()][string]$RunId,
        [long]$CreatedAtUnixMs = -1,
        [AllowNull()]$Subject,
        [AllowNull()]$Consent,
        [AllowNull()]$Binding,
        [AllowNull()]$Authority,
        [AllowNull()][object[]]$Measurements,
        [AllowNull()][object[]]$Artifacts = @(),
        [long]$MaxArtifactByteLength = 10485760,
        [long]$RunStartedAtUnixMs = -1,
        [long]$RunEndedAtUnixMs = -1,
        [AllowNull()][string]$OutputPath,
        [switch]$AsJson
    )

    $runStartedBound = $PSBoundParameters.ContainsKey('RunStartedAtUnixMs')
    $runEndedBound = $PSBoundParameters.ContainsKey('RunEndedAtUnixMs')
    $evidenceBound = $PSBoundParameters.ContainsKey('Evidence')

    function Refuse([string]$Reason) {
        throw "OSL_A11Y_BENCH_REFUSED: $Reason"
    }

    function Get-Field {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Name)
        # `return $x` UNROLLS a collection: a one-element array comes back as a
        # bare object, so a record with exactly one measurement was refused as
        # "not an array" while two or more passed. `,$x` wraps it so the
        # unrolling yields the original value unchanged -- for scalars too.
        if ($Value -is [System.Collections.IDictionary]) {
            if ($Value.Contains($Name)) {
                return ,$Value[$Name]
            }
            return $null
        }
        foreach ($property in $Value.PSObject.Properties) {
            if ($property.Name -ceq $Name) {
                return ,$property.Value
            }
        }
        return $null
    }

    function Get-FieldNames {
        param([AllowNull()]$Value)
        if ($null -eq $Value) {
            return @()
        }
        if ($Value -is [System.Collections.IDictionary]) {
            return @($Value.Keys | ForEach-Object { [string]$_ })
        }
        return @($Value.PSObject.Properties | ForEach-Object { $_.Name })
    }

    function Assert-ExactFields {
        param(
            [AllowNull()]$Value,
            [Parameter(Mandatory)][string[]]$Names,
            [Parameter(Mandatory)][string]$Label
        )
        if ($null -eq $Value) {
            Refuse $Label
        }
        $actual = @(Get-FieldNames -Value $Value | Sort-Object)
        $expected = @($Names | Sort-Object)
        if (($actual -join "`n") -cne ($expected -join "`n")) {
            Refuse $Label
        }
    }

    function Assert-RequiredString {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value)) {
            Refuse $Label
        }
    }

    function Assert-Label {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        Assert-RequiredString -Value $Value -Label $Label
        if ($Value -cnotmatch '^[a-z0-9][a-z0-9.-]{0,127}$') {
            Refuse $Label
        }
    }

    function Assert-FactName {
        param([AllowNull()]$Value)
        Assert-RequiredString -Value $Value -Label 'MEASUREMENTS'
        if ($Value -cnotmatch '^[A-Za-z][A-Za-z0-9]{0,63}$') {
            Refuse 'MEASUREMENTS'
        }
    }

    function Assert-Sha256 {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        if ($Value -isnot [string] -or $Value -cnotmatch '^[0-9a-f]{64}$') {
            Refuse $Label
        }
    }

    function Assert-UnixMs {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        if (
            $Value -isnot [byte] -and $Value -isnot [int16] -and
            $Value -isnot [int32] -and $Value -isnot [int64] -and
            $Value -isnot [long]
        ) {
            Refuse $Label
        }
        if ([long]$Value -lt 0) {
            Refuse $Label
        }
    }

    function Assert-WithinRunWindow {
        param([long]$Value, [Parameter(Mandatory)][string]$Label)
        if (
            $runStartedBound -and
            [long]$RunStartedAtUnixMs -ge 0 -and
            $Value -lt [long]$RunStartedAtUnixMs
        ) {
            Refuse $Label
        }
        if (
            $runEndedBound -and
            [long]$RunEndedAtUnixMs -ge 0 -and
            $Value -gt [long]$RunEndedAtUnixMs
        ) {
            Refuse $Label
        }
    }

    function Assert-Facts {
        param([AllowNull()]$Value)
        if ($null -eq $Value) {
            Refuse 'MEASUREMENTS'
        }
        $names = @(Get-FieldNames -Value $Value)
        if (@($names).Count -lt 1) {
            Refuse 'MEASUREMENTS'
        }
        foreach ($name in $names) {
            Assert-FactName -Value $name
            $fact = Get-Field -Value $Value -Name $name
            if ($fact -is [bool]) {
                continue
            }
            if (
                $fact -is [byte] -or $fact -is [int16] -or
                $fact -is [int32] -or $fact -is [int64] -or
                $fact -is [single] -or $fact -is [double] -or
                $fact -is [decimal]
            ) {
                if ([double]::IsNaN([double]$fact) -or [double]::IsInfinity([double]$fact)) {
                    Refuse 'MEASUREMENTS'
                }
                continue
            }
            Refuse 'MEASUREMENTS'
        }
    }

    function Assert-RelativeArtifactPath {
        param([AllowNull()]$Value)
        Assert-RequiredString -Value $Value -Label 'ARTIFACTS'
        if (
            $Value.StartsWith('/') -or
            $Value.StartsWith('\') -or
            $Value -match '^[A-Za-z]:' -or
            $Value.Contains('\') -or
            $Value.Contains('//')
        ) {
            Refuse 'ARTIFACTS'
        }
        if ($Value -cnotmatch '^[a-z0-9][a-z0-9._/-]{0,255}$') {
            Refuse 'ARTIFACTS'
        }
        $segments = @($Value.Split('/'))
        foreach ($segment in $segments) {
            if (
                [string]::IsNullOrWhiteSpace($segment) -or
                $segment -ceq '.' -or
                $segment -ceq '..'
            ) {
                Refuse 'ARTIFACTS'
            }
        }
    }

    function Assert-BooleanTrue {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        if ($Value -isnot [bool] -or $Value -ne $true) {
            Refuse $Label
        }
    }

    function Assert-BooleanFalse {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        if ($Value -isnot [bool] -or $Value -ne $false) {
            Refuse $Label
        }
    }

    function ConvertTo-ArrayValue {
        param([AllowNull()]$Value, [Parameter(Mandatory)][string]$Label)
        if ($null -eq $Value) {
            return ,@()
        }
        if ($Value -is [string] -or $Value -is [System.Collections.IDictionary]) {
            Refuse $Label
        }
        if ($Value -is [array]) {
            return ,@($Value)
        }
        Refuse $Label
    }

    function Get-ReducedVerdict {
        param([Parameter(Mandatory)]$Record)

        Assert-ExactFields -Value $Record -Names @(
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

        if ((Get-Field -Value $Record -Name 'schema') -cne 'osl-a11y-bench-evidence-v1') {
            Refuse 'SCHEMA'
        }
        Assert-Label -Value (Get-Field -Value $Record -Name 'benchId') -Label 'BENCH'
        $recordRunId = Get-Field -Value $Record -Name 'runId'
        Assert-RequiredString -Value $recordRunId -Label 'RUN'
        if ($recordRunId -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$') {
            Refuse 'RUN'
        }

        $createdAt = Get-Field -Value $Record -Name 'createdAtUnixMs'
        Assert-UnixMs -Value $createdAt -Label 'TIME'
        Assert-WithinRunWindow -Value ([long]$createdAt) -Label 'TIME'

        $subjectValue = Get-Field -Value $Record -Name 'subject'
        Assert-ExactFields -Value $subjectValue -Names @(
            'platform',
            'appBuildSha256',
            'executableSha256',
            'surface'
        ) -Label 'SUBJECT'
        if ((Get-Field -Value $subjectValue -Name 'platform') -cne 'windows') {
            Refuse 'SUBJECT'
        }
        Assert-Sha256 -Value (Get-Field -Value $subjectValue -Name 'appBuildSha256') -Label 'SUBJECT'
        Assert-Sha256 -Value (Get-Field -Value $subjectValue -Name 'executableSha256') -Label 'SUBJECT'
        Assert-Label -Value (Get-Field -Value $subjectValue -Name 'surface') -Label 'SUBJECT'

        $consentValue = Get-Field -Value $Record -Name 'consent'
        Assert-ExactFields -Value $consentValue -Names @(
            'granted',
            'source',
            'observedAtUnixMs'
        ) -Label 'CONSENT'
        Assert-BooleanTrue -Value (Get-Field -Value $consentValue -Name 'granted') -Label 'CONSENT'
        Assert-Label -Value (Get-Field -Value $consentValue -Name 'source') -Label 'CONSENT'
        $consentObservedAt = Get-Field -Value $consentValue -Name 'observedAtUnixMs'
        Assert-UnixMs -Value $consentObservedAt -Label 'CONSENT'
        Assert-WithinRunWindow -Value ([long]$consentObservedAt) -Label 'CONSENT'

        $bindingValue = Get-Field -Value $Record -Name 'binding'
        Assert-ExactFields -Value $bindingValue -Names @(
            'method',
            'subjectBindingSha256',
            'observedAtUnixMs'
        ) -Label 'BINDING'
        Assert-Label -Value (Get-Field -Value $bindingValue -Name 'method') -Label 'BINDING'
        Assert-Sha256 -Value (Get-Field -Value $bindingValue -Name 'subjectBindingSha256') -Label 'BINDING'
        $bindingObservedAt = Get-Field -Value $bindingValue -Name 'observedAtUnixMs'
        Assert-UnixMs -Value $bindingObservedAt -Label 'BINDING'
        Assert-WithinRunWindow -Value ([long]$bindingObservedAt) -Label 'BINDING'

        $authorityValue = Get-Field -Value $Record -Name 'authority'
        Assert-ExactFields -Value $authorityValue -Names @(
            'collector',
            'verifier',
            'observedAtUnixMs'
        ) -Label 'AUTHORITY'
        Assert-Label -Value (Get-Field -Value $authorityValue -Name 'collector') -Label 'AUTHORITY'
        Assert-Label -Value (Get-Field -Value $authorityValue -Name 'verifier') -Label 'AUTHORITY'
        $authorityObservedAt = Get-Field -Value $authorityValue -Name 'observedAtUnixMs'
        Assert-UnixMs -Value $authorityObservedAt -Label 'AUTHORITY'
        Assert-WithinRunWindow -Value ([long]$authorityObservedAt) -Label 'AUTHORITY'

        $measurementsValue = ConvertTo-ArrayValue `
            -Value (Get-Field -Value $Record -Name 'measurements') `
            -Label 'MEASUREMENTS'
        if (@($measurementsValue).Count -lt 1) {
            Refuse 'MEASUREMENTS'
        }

        $firstMeasurementAt = $null
        $hasFail = $false
        foreach ($measurement in $measurementsValue) {
            Assert-ExactFields -Value $measurement -Names @(
                'name',
                'status',
                'observedAtUnixMs',
                'facts'
            ) -Label 'MEASUREMENTS'
            Assert-Label -Value (Get-Field -Value $measurement -Name 'name') -Label 'MEASUREMENTS'
            $status = Get-Field -Value $measurement -Name 'status'
            if ($status -cne 'pass' -and $status -cne 'fail') {
                Refuse 'MEASUREMENTS'
            }
            if ($status -ceq 'fail') {
                $hasFail = $true
            }
            $observedAt = Get-Field -Value $measurement -Name 'observedAtUnixMs'
            Assert-UnixMs -Value $observedAt -Label 'MEASUREMENTS'
            Assert-WithinRunWindow -Value ([long]$observedAt) -Label 'MEASUREMENTS'
            if ($null -eq $firstMeasurementAt -or [long]$observedAt -lt [long]$firstMeasurementAt) {
                $firstMeasurementAt = [long]$observedAt
            }
            Assert-Facts -Value (Get-Field -Value $measurement -Name 'facts')
        }
        if ([long]$bindingObservedAt -gt [long]$firstMeasurementAt) {
            Refuse 'BINDING'
        }

        $artifactsValue = ConvertTo-ArrayValue `
            -Value (Get-Field -Value $Record -Name 'artifacts') `
            -Label 'ARTIFACTS'
        foreach ($artifact in $artifactsValue) {
            Assert-ExactFields -Value $artifact -Names @(
                'kind',
                'relativePath',
                'sha256',
                'byteLength',
                'containsUserContent'
            ) -Label 'ARTIFACTS'
            Assert-Label -Value (Get-Field -Value $artifact -Name 'kind') -Label 'ARTIFACTS'
            Assert-RelativeArtifactPath -Value (Get-Field -Value $artifact -Name 'relativePath')
            Assert-Sha256 -Value (Get-Field -Value $artifact -Name 'sha256') -Label 'ARTIFACTS'
            $byteLength = Get-Field -Value $artifact -Name 'byteLength'
            Assert-UnixMs -Value $byteLength -Label 'ARTIFACTS'
            if ([long]$byteLength -gt $MaxArtifactByteLength) {
                Refuse 'ARTIFACTS'
            }
            Assert-BooleanFalse `
                -Value (Get-Field -Value $artifact -Name 'containsUserContent') `
                -Label 'ARTIFACTS'
        }

        $privacyValue = Get-Field -Value $Record -Name 'privacy'
        Assert-ExactFields -Value $privacyValue -Names @(
            'noRawText',
            'noAccountIdentifiers',
            'noSecrets',
            'boundedArtifacts'
        ) -Label 'PRIVACY'
        Assert-BooleanTrue -Value (Get-Field -Value $privacyValue -Name 'noRawText') -Label 'PRIVACY'
        Assert-BooleanTrue -Value (Get-Field -Value $privacyValue -Name 'noAccountIdentifiers') -Label 'PRIVACY'
        Assert-BooleanTrue -Value (Get-Field -Value $privacyValue -Name 'noSecrets') -Label 'PRIVACY'
        Assert-BooleanTrue -Value (Get-Field -Value $privacyValue -Name 'boundedArtifacts') -Label 'PRIVACY'

        $declaredVerdict = Get-Field -Value $Record -Name 'verdict'
        if (
            $declaredVerdict -cne 'pass' -and
            $declaredVerdict -cne 'fail' -and
            $declaredVerdict -cne 'refused'
        ) {
            Refuse 'VERDICT'
        }

        $computedVerdict = 'pass'
        if ($hasFail) {
            $computedVerdict = 'fail'
        }

        if ($evidenceBound) {
            if ($declaredVerdict -ceq 'refused') {
                return 'refused'
            }
            if ($declaredVerdict -cne $computedVerdict) {
                Refuse 'VERDICT'
            }
        }
        return $computedVerdict
    }

    $record = $Evidence
    if (-not $evidenceBound) {
        $record = [pscustomobject][ordered]@{
            schema = 'osl-a11y-bench-evidence-v1'
            benchId = $BenchId
            runId = $RunId
            createdAtUnixMs = $CreatedAtUnixMs
            subject = $Subject
            consent = $Consent
            binding = $Binding
            authority = $Authority
            measurements = @($Measurements)
            artifacts = @($Artifacts)
            privacy = [pscustomobject][ordered]@{
                noRawText = $true
                noAccountIdentifiers = $true
                noSecrets = $true
                boundedArtifacts = $true
            }
            verdict = 'refused'
        }
    }

    $verdict = Get-ReducedVerdict -Record $record
    if ($record -is [System.Collections.IDictionary]) {
        $record['verdict'] = $verdict
    } else {
        $record.verdict = $verdict
    }

    if ($AsJson -or -not [string]::IsNullOrWhiteSpace($OutputPath)) {
        $json = $record | ConvertTo-Json -Depth 40
        if (-not [string]::IsNullOrWhiteSpace($OutputPath)) {
            $parent = Split-Path -Parent $OutputPath
            if (-not [string]::IsNullOrWhiteSpace($parent)) {
                [void](New-Item -ItemType Directory -Force -Path $parent)
            }
            [IO.File]::WriteAllText(
                $OutputPath,
                $json + "`n",
                [Text.UTF8Encoding]::new($false)
            )
        }
        if ($AsJson) {
            return $json
        }
    }

    return $record
}
