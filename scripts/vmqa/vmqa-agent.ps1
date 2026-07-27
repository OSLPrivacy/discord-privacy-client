<#
OSL VMQA interactive agent.

az vm run-command runs as SYSTEM in session 0, where Chromium/Tauri surfaces do not render and
synthetic input cannot prove an overlay over a real Discord window. This daemon is launched by an
interactive logon scheduled task so screenshots, focus, and input happen in a real rendering session.

Status vocabulary:
  pass         measured, and the measurement satisfies the step
  fail         measured, and the measurement violates the step
  unmeasurable the measurement was possible but did not happen this run (no stimulus, or a stale
               artifact). NEVER green.
  blocked      no run of this harness against this build could measure it. NEVER green.
#>

param(
    [int]$PollSeconds = 3,
    [switch]$RunOnce
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$StorageAccount = 'osltestartifactsa7d5'
$BlobContainer = 'vmqa'
$BlobContainerUri = "https://$StorageAccount.blob.core.windows.net/$BlobContainer"
$StorageToken = $null
$StorageTokenExpiresUtc = [datetime]::MinValue
$InteractiveUser = 'osltest'
$AgentRoot = 'C:\ProgramData\OSL-VMQA'
$TaskName = 'OSL-VMQA-Agent'
$AgentPath = Join-Path $AgentRoot 'vmqa-agent.ps1'
$Win32ModulePath = Join-Path $AgentRoot 'vmqa-win32.ps1'
$LogPath = Join-Path $AgentRoot 'agent.log'
$AgentStartedUtc = [datetime]::UtcNow

if ($env:COMPUTERNAME -notlike 'OSL-*') {
    throw "VMQA_WRONG_MACHINE: refusing to run the VMQA agent on '$env:COMPUTERNAME'. These scripts synthesize input and are only allowed on disposable OSL-* QA VMs."
}

. $Win32ModulePath

function Rotate-LogIfNeeded {
    if (Test-Path -LiteralPath $LogPath -PathType Leaf) {
        $item = Get-Item -LiteralPath $LogPath
        if ($item.Length -ge 8MB) {
            $backup = $LogPath + '.1'
            if (Test-Path -LiteralPath $backup) {
                Remove-Item -LiteralPath $backup -Force
            }
            Move-Item -LiteralPath $LogPath -Destination $backup -Force
        }
    }
}

function Write-Log {
    param([Parameter(Mandatory)][string]$Message)
    if (-not (Test-Path -LiteralPath $AgentRoot)) {
        New-Item -ItemType Directory -Force -Path $AgentRoot | Out-Null
    }
    Rotate-LogIfNeeded
    $line = '{0} {1}' -f [datetime]::UtcNow.ToString('o'), $Message
    Add-Content -LiteralPath $LogPath -Value $line -Encoding UTF8
}

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-Sha256Text {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($bytes)) -replace '-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Clear-StorageToken {
    $script:StorageToken = $null
    $script:StorageTokenExpiresUtc = [datetime]::MinValue
}

function Get-StorageToken {
    $refreshAfterUtc = $script:StorageTokenExpiresUtc.AddMinutes(-5)
    if (-not [string]::IsNullOrWhiteSpace($script:StorageToken) -and [datetime]::UtcNow -lt $refreshAfterUtc) {
        return $script:StorageToken
    }

    $uri = 'http://169.254.169.254/metadata/identity/oauth2/token' +
           '?api-version=2019-08-01&resource=https%3A%2F%2Fstorage.azure.com%2F'
    $t = Invoke-RestMethod -Method Get -Uri $uri -Headers @{ Metadata = 'true' } -TimeoutSec 10
    $script:StorageToken = [string]$t.access_token
    $script:StorageTokenExpiresUtc = [DateTimeOffset]::FromUnixTimeSeconds([int64]$t.expires_on).UtcDateTime
    return $script:StorageToken
}

function ConvertTo-BlobPath {
    param([Parameter(Mandatory)][string]$Name)
    return (($Name -split '/') | ForEach-Object { [uri]::EscapeDataString($_) }) -join '/'
}

function New-BlobUri {
    param(
        [string]$Name = '',
        [hashtable]$Query = @{}
    )
    $uri = $BlobContainerUri
    if (-not [string]::IsNullOrWhiteSpace($Name)) {
        $uri += '/' + (ConvertTo-BlobPath -Name $Name)
    }
    if ($Query.Count -gt 0) {
        $parts = @()
        foreach ($key in @($Query.Keys | Sort-Object)) {
            $value = $Query[$key]
            if ($null -ne $value -and [string]$value -ne '') {
                $parts += ('{0}={1}' -f [uri]::EscapeDataString([string]$key), [uri]::EscapeDataString([string]$value))
            }
        }
        if ($parts.Count -gt 0) {
            $uri += '?' + ($parts -join '&')
        }
    }
    return $uri
}

function Get-HttpStatusCode {
    param([Parameter(Mandatory)]$ErrorRecord)
    $response = $ErrorRecord.Exception.Response
    if ($null -eq $response) {
        return $null
    }
    if ($response.StatusCode -is [int]) {
        return [int]$response.StatusCode
    }
    return [int]$response.StatusCode.value__
}

function Invoke-BlobRequest {
    param(
        [Parameter(Mandatory)][ValidateSet('GET','HEAD','PUT')][string]$Method,
        [string]$Name = '',
        [hashtable]$Query = @{},
        [byte[]]$Body = $null,
        [string]$OutFile = '',
        [hashtable]$AdditionalHeaders = @{},
        [string]$ContentType = 'application/octet-stream'
    )
    $attempt = 0
    while ($true) {
        $attempt += 1
        try {
            $token = Get-StorageToken
            $headers = @{
                Authorization = "Bearer $token"
                # Entra-authenticated Blob requests fail without an explicit storage service version.
                'x-ms-version' = '2021-08-06'
            }
            foreach ($key in $AdditionalHeaders.Keys) {
                $headers[$key] = $AdditionalHeaders[$key]
            }
            $params = @{
                Method = $Method
                Uri = (New-BlobUri -Name $Name -Query $Query)
                Headers = $headers
                TimeoutSec = 60
                ErrorAction = 'Stop'
                UseBasicParsing = $true
            }
            if (-not [string]::IsNullOrWhiteSpace($OutFile)) {
                $directory = Split-Path -Parent $OutFile
                if ($directory -and -not (Test-Path -LiteralPath $directory)) {
                    New-Item -ItemType Directory -Force -Path $directory | Out-Null
                }
                $params['OutFile'] = $OutFile
            }
            if ($null -ne $Body) {
                $params['Body'] = $Body
                $params['ContentType'] = $ContentType
            }
            return Invoke-WebRequest @params
        } catch {
            $status = Get-HttpStatusCode -ErrorRecord $_
            Clear-StorageToken
            if ($status -eq 404 -or $status -eq 403 -or $status -eq 401 -or ($null -ne $status -and $status -ge 400 -and $status -lt 500)) {
                throw
            }
            if ($attempt -ge 3 -or ($null -ne $status -and $status -lt 500)) {
                throw
            }
            Start-Sleep -Seconds ([int][Math]::Pow(2, $attempt))
        }
    }
}

function Test-Blob {
    param([Parameter(Mandatory)][string]$Name)
    try {
        [void](Invoke-BlobRequest -Method HEAD -Name $Name)
        return $true
    } catch {
        $status = Get-HttpStatusCode -ErrorRecord $_
        if ($status -eq 404) {
            # Only 404 means absent. Other failures are broken apparatus and must surface.
            return $false
        }
        throw
    }
}

function Get-BlobText {
    param([Parameter(Mandatory)][string]$Name)
    try {
        $response = Invoke-BlobRequest -Method GET -Name $Name
        return [string]$response.Content
    } catch {
        if ((Get-HttpStatusCode -ErrorRecord $_) -eq 404) {
            return $null
        }
        throw
    }
}

function Get-BlobFile {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Path
    )
    try {
        [void](Invoke-BlobRequest -Method GET -Name $Name -OutFile $Path)
        return $true
    } catch {
        if ((Get-HttpStatusCode -ErrorRecord $_) -eq 404) {
            return $false
        }
        throw
    }
}

function Put-BlobText {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Text
    )
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
    [void](Invoke-BlobRequest -Method PUT -Name $Name -Body $bytes -ContentType 'text/plain; charset=utf-8' -AdditionalHeaders @{ 'x-ms-blob-type' = 'BlockBlob' })
}

function Put-BlobFile {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Path
    )
    $bytes = [IO.File]::ReadAllBytes($Path)
    [void](Invoke-BlobRequest -Method PUT -Name $Name -Body $bytes -ContentType 'application/octet-stream' -AdditionalHeaders @{ 'x-ms-blob-type' = 'BlockBlob' })
}

function Get-BlobList {
    param([Parameter(Mandatory)][string]$Prefix)
    $names = @()
    $marker = ''
    do {
        $query = @{
            restype = 'container'
            comp = 'list'
            prefix = $Prefix
        }
        if (-not [string]::IsNullOrWhiteSpace($marker)) {
            $query['marker'] = $marker
        }
        $response = Invoke-BlobRequest -Method GET -Query $query
        [xml]$xml = [string]$response.Content
        foreach ($blob in @($xml.EnumerationResults.Blobs.Blob)) {
            if ($null -ne $blob -and -not [string]::IsNullOrWhiteSpace($blob.Name)) {
                $names += [string]$blob.Name
            }
        }
        $marker = [string]$xml.EnumerationResults.NextMarker
    } while (-not [string]::IsNullOrWhiteSpace($marker))
    return $names
}

function Write-AtomicText {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Content
    )
    $directory = Split-Path -Parent $Path
    if ($directory -and -not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    $tmp = $Path + '.tmp'
    [IO.File]::WriteAllText($tmp, $Content, [Text.UTF8Encoding]::new($false))
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        [IO.File]::Replace($tmp, $Path, $null)
    } else {
        [IO.File]::Move($tmp, $Path)
    }
}

function Write-AtomicJson {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)]$Value
    )
    $json = $Value | ConvertTo-Json -Depth 12
    Write-AtomicText -Path $Path -Content ($json + [Environment]::NewLine)
}

function Get-AgentSha {
    if (Test-Path -LiteralPath $AgentPath -PathType Leaf) {
        return Get-Sha256 -Path $AgentPath
    }
    return ''
}

function Get-VmNameFromImds {
    $uri = 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text'
    return [string](Invoke-RestMethod -Method Get -Uri $uri -Headers @{ Metadata = 'true' } -TimeoutSec 10)
}

function Write-Heartbeat {
    param(
        [Parameter(Mandatory)][string]$VmName,
        [string]$BlobState = 'ok',
        [string]$Failure = ''
    )
    $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
    $heartbeat = [ordered]@{
        schemaVersion = 1
        vmName = $VmName
        utc = [datetime]::UtcNow.ToString('o')
        agentSha256 = Get-AgentSha
        sessionId = $sessionId
        interactiveUserName = [Security.Principal.WindowsIdentity]::GetCurrent().Name
        isInteractiveSession = ($sessionId -ne 0)
        blobState = $BlobState
        failure = $Failure
    }
    $json = ($heartbeat | ConvertTo-Json -Depth 12) + [Environment]::NewLine
    Put-BlobText -Name "agent/$VmName/heartbeat.json" -Text $json
}

function Get-RunNonce {
    $cmd = Get-Command -Name Start-VmqaRun -ErrorAction SilentlyContinue
    if ($cmd) {
        return [string](Start-VmqaRun)
    }
    return [guid]::NewGuid().ToString('n')
}

function Get-PropertyValue {
    param(
        [Parameter(Mandatory)]$Object,
        [Parameter(Mandatory)][string]$Name,
        $Default = $null,
        [switch]$Required
    )
    if ($null -ne $Object) {
        $property = $Object.PSObject.Properties[$Name]
        if ($null -ne $property) {
            return $property.Value
        }
    }
    if ($Required) {
        throw "VMQA_REQUEST_MISSING_FIELD: $Name"
    }
    return $Default
}

function New-StepResult {
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][string]$Verb,
        [Parameter(Mandatory)][ValidateSet('pass','fail','unmeasurable','blocked')][string]$Status,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Detail,
        [string[]]$Artifacts = @()
    )
    return [ordered]@{
        id = $Id
        verb = $Verb
        status = $Status
        detail = $Detail
        artifacts = @($Artifacts)
    }
}

function Resolve-StepSubject {
    param(
        [Parameter(Mandatory)][string]$Identifier,
        [Parameter(Mandatory)][string]$RunNonce
    )
    try {
        $subject = Resolve-VmqaSubject -Identifier $Identifier -RunNonce $RunNonce
        Assert-VmqaSubject -Subject $subject -RunNonce $RunNonce | Out-Null
        return [ordered]@{ ok = $true; subject = $subject; status = ''; detail = ''; markerWindowsTotal = $subject.MarkerWindowsTotal }
    } catch {
        $message = [string]$_.Exception.Message
        $markerWindowsTotal = 0
        if ($_.Exception.Data.Contains('MarkerWindowsTotal')) {
            $markerWindowsTotal = [int]$_.Exception.Data['MarkerWindowsTotal']
        }
        if ($message -like 'VMQA_NO_SUBJECT*') {
            # This distinction is the most important line in the agent: if the enumerator saw any
            # marker window, this identifier is absent and the run is blocked; if it saw zero marker
            # windows across all identifiers, the apparatus may be broken, so the result is
            # unmeasurable and must not be conflated with a harness-level block.
            if ($markerWindowsTotal -ge 1) {
                return [ordered]@{ ok = $false; subject = $null; status = 'blocked'; detail = "VMQA_NO_SUBJECT; markerWindowsTotal=$markerWindowsTotal"; markerWindowsTotal = $markerWindowsTotal }
            }
            return [ordered]@{ ok = $false; subject = $null; status = 'unmeasurable'; detail = 'VMQA_NO_SUBJECT; markerWindowsTotal=0; apparatus-not-proven'; markerWindowsTotal = 0 }
        }
        if ($message -like 'VMQA_AMBIGUOUS_SUBJECT*') {
            return [ordered]@{ ok = $false; subject = $null; status = 'blocked'; detail = "$message; markerWindowsTotal=$markerWindowsTotal"; markerWindowsTotal = $markerWindowsTotal }
        }
        return [ordered]@{ ok = $false; subject = $null; status = 'blocked'; detail = $message; markerWindowsTotal = $markerWindowsTotal }
    }
}

function Test-ArtifactFreshForRun {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][datetime]$RunStartUtc
    )
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [ordered]@{ fresh = $false; detail = "artifact missing: $Path" }
    }
    $lastWrite = (Get-Item -LiteralPath $Path).LastWriteTimeUtc
    if ($lastWrite -lt $RunStartUtc) {
        $age = [int]($RunStartUtc - $lastWrite).TotalSeconds
        return [ordered]@{ fresh = $false; detail = "artifact predates runStartUtc by ${age}s: $Path" }
    }
    return [ordered]@{ fresh = $true; detail = 'fresh' }
}

function Get-CanonicalFilePath {
    param([Parameter(Mandatory)][string]$Path)
    return (Resolve-Path -LiteralPath $Path -ErrorAction Stop).ProviderPath
}

function Copy-And-VerifyBuild {
    param(
        [Parameter(Mandatory)][string]$ExeSha256
    )
    $sha = $ExeSha256.ToLowerInvariant()
    $sourceExe = "builds/$sha/osl-privacy-hub.exe"
    $sourceDll = "builds/$sha/WebView2Loader.dll"
    $destDir = Join-Path 'C:\OSL-VMQA' $sha
    $destExe = Join-Path $destDir 'osl-privacy-hub.exe'
    $destDll = Join-Path $destDir 'WebView2Loader.dll'

    New-Item -ItemType Directory -Force -Path $destDir | Out-Null
    if (-not (Get-BlobFile -Name $sourceExe -Path $destExe)) {
        return New-StepResult -Id '' -Verb 'stage' -Status 'fail' -Detail "missing source exe: $sourceExe"
    }
    # WebView2Loader.dll must live beside the exe; without it the process hangs before main with no
    # trace file, which is indistinguishable from a corrupt build at the harness layer.
    if (-not (Get-BlobFile -Name $sourceDll -Path $destDll)) {
        return New-StepResult -Id '' -Verb 'stage' -Status 'fail' -Detail "missing source WebView2Loader.dll: $sourceDll"
    }

    $destExeHash = Get-Sha256 -Path $destExe
    $destDllHash = Get-Sha256 -Path $destDll
    if ($destExeHash -cne $sha) {
        return New-StepResult -Id '' -Verb 'stage' -Status 'fail' -Detail "staged exe sha mismatch expected=$sha actual=$destExeHash"
    }
    return New-StepResult -Id '' -Verb 'stage' -Status 'pass' -Detail "staged=$destDir exeSha=$destExeHash webview2Sha=$destDllHash"
}

function Invoke-FullDesktopShot {
    param(
        [Parameter(Mandatory)][string]$OutPath
    )
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing

    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
        } finally {
            $graphics.Dispose()
        }

        $directory = Split-Path -Parent $OutPath
        if ($directory -and -not (Test-Path -LiteralPath $directory)) {
            New-Item -ItemType Directory -Force -Path $directory | Out-Null
        }
        $bitmap.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)

        $colors = [System.Collections.Generic.HashSet[int]]::new()
        for ($y = 0; $y -lt $bounds.Height; $y += 4) {
            for ($x = 0; $x -lt $bounds.Width; $x += 4) {
                [void]$colors.Add($bitmap.GetPixel($x, $y).ToArgb())
            }
        }
        return [ordered]@{
            path = $OutPath
            width = $bounds.Width
            height = $bounds.Height
            distinctColors = $colors.Count
        }
    } finally {
        $bitmap.Dispose()
    }
}

function Invoke-Step {
    param(
        [Parameter(Mandatory)]$Step,
        [Parameter(Mandatory)]$Request,
        [Parameter(Mandatory)][string]$RunBlobPrefix,
        [Parameter(Mandatory)][string]$RunId,
        [Parameter(Mandatory)][string]$VmName,
        [Parameter(Mandatory)][string]$RunNonce,
        [Parameter(Mandatory)][datetime]$RunStartUtc
    )
    $stepId = [string](Get-PropertyValue -Object $Step -Name 'id' -Required)
    $verb = ([string](Get-PropertyValue -Object $Step -Name 'verb' -Required)).ToLowerInvariant()
    $stepArgs = Get-PropertyValue -Object $Step -Name 'args' -Default ([pscustomobject]@{})
    $identifier = [string](Get-PropertyValue -Object $Request -Name 'identifier' -Required)

    switch ($verb) {
        'ping' {
            $markerWindowsTotal = 0
            try {
                $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
                $markerWindowsTotal = [int]$resolved['markerWindowsTotal']
            } catch {
                if ($_.Exception.Data.Contains('MarkerWindowsTotal')) {
                    $markerWindowsTotal = [int]$_.Exception.Data['MarkerWindowsTotal']
                }
            }
            $sessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
            $detail = 'vmName={0}; sessionId={1}; isInteractiveSession={2}; markerWindowsTotal={3}' -f $VmName, $sessionId, ($sessionId -ne 0), $markerWindowsTotal
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail $detail
        }
        'stage' {
            $exeSha = [string](Get-PropertyValue -Object $stepArgs -Name 'exeSha256' -Required)
            $result = Copy-And-VerifyBuild -ExeSha256 $exeSha
            $result['id'] = $stepId
            return $result
        }
        'launch' {
            $exeSha = ([string](Get-PropertyValue -Object $stepArgs -Name 'exeSha256' -Required)).ToLowerInvariant()
            $timeoutSeconds = [int](Get-PropertyValue -Object $stepArgs -Name 'timeoutSeconds' -Default 60)
            $exePath = Join-Path (Join-Path 'C:\OSL-VMQA' $exeSha) 'osl-privacy-hub.exe'
            if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' -Detail "staged exe missing: $exePath"
            }
            $canonicalExePath = Get-CanonicalFilePath -Path $exePath
            $process = Start-Process -FilePath $exePath -PassThru
            # The marker is resolved once for this step. The blind wait gives Tauri time to publish
            # its single-instance marker without creating a second target-selection path.
            if ($timeoutSeconds -gt 0) {
                Start-Sleep -Seconds $timeoutSeconds
            }
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $subject = $resolved['subject']
            if ($subject.Pid -ne $process.Id) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' -Detail "marker pid $($subject.Pid) did not match started pid $($process.Id)"
            }
            if (-not [string]::Equals($subject.ExePath, $canonicalExePath, [StringComparison]::OrdinalIgnoreCase)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' -Detail "marker image path '$($subject.ExePath)' did not match staged exe '$canonicalExePath'"
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail "pid=$($subject.Pid); exe=$($subject.ExePath)"
        }
        'shot' {
            $name = [string](Get-PropertyValue -Object $stepArgs -Name 'name' -Required)
            $safeName = [IO.Path]::GetFileName($name)
            if ([string]::IsNullOrWhiteSpace($safeName)) {
                throw 'VMQA_BAD_SHOT_NAME'
            }
            $artifactDir = Join-Path (Join-Path $AgentRoot 'artifacts') $RunId
            $artifactPath = Join-Path $artifactDir ($safeName + '.png')
            $shot = Invoke-FullDesktopShot -OutPath $artifactPath
            $fresh = Test-ArtifactFreshForRun -Path $artifactPath -RunStartUtc $RunStartUtc
            $artifactRel = 'artifacts/' + ($safeName + '.png')
            if (-not $fresh['fresh']) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail $fresh['detail'] -Artifacts @($artifactRel)
            }
            Put-BlobFile -Name "$RunBlobPrefix/$artifactRel" -Path $artifactPath
            $detail = 'path={0}; width={1}; height={2}; distinctColors={3}' -f $artifactRel, $shot['width'], $shot['height'], $shot['distinctColors']
            if ([int]$shot['distinctColors'] -lt 16) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail $detail -Artifacts @($artifactRel)
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail $detail -Artifacts @($artifactRel)
        }
        'click' {
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $winX = [int](Get-PropertyValue -Object $stepArgs -Name 'winX' -Required)
            $winY = [int](Get-PropertyValue -Object $stepArgs -Name 'winY' -Required)
            $settleMs = [int](Get-PropertyValue -Object $stepArgs -Name 'settleMs' -Default 600)
            $click = Invoke-VmqaClick -Subject $resolved['subject'] -WinX $winX -WinY $winY -SettleMs $settleMs -RunNonce $RunNonce
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail ("screenX={0}; screenY={1}" -f $click.ScreenX, $click.ScreenY)
        }
        'type' {
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $text = [string](Get-PropertyValue -Object $stepArgs -Name 'text' -Default '')
            $settleMs = [int](Get-PropertyValue -Object $stepArgs -Name 'settleMs' -Default 400)
            Invoke-VmqaType -Subject $resolved['subject'] -Text $text -SettleMs $settleMs -RunNonce $RunNonce
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail ("chars={0}" -f $text.Length)
        }
        'key' {
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $key = [string](Get-PropertyValue -Object $stepArgs -Name 'key' -Required)
            $settleMs = [int](Get-PropertyValue -Object $stepArgs -Name 'settleMs' -Default 400)
            Invoke-VmqaKey -Subject $resolved['subject'] -Key $key -SettleMs $settleMs -RunNonce $RunNonce
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail "key=$key"
        }
        'wait' {
            $ms = [int](Get-PropertyValue -Object $stepArgs -Name 'ms' -Required)
            if ($ms -gt 0) {
                Start-Sleep -Milliseconds $ms
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail "ms=$ms"
        }
        'kill' {
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $subject = $resolved['subject']
            Assert-VmqaSubject -Subject $subject -RunNonce $RunNonce | Out-Null
            Stop-Process -Id $subject.Pid -Force
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail "stopped pid=$($subject.Pid)"
        }
        default {
            return New-StepResult -Id $stepId -Verb $verb -Status 'blocked' -Detail "unknown verb: $verb"
        }
    }
}

function Get-OverallStatus {
    param([Parameter(Mandatory)]$Steps)
    $statuses = @($Steps | ForEach-Object { $_['status'] })
    if (($statuses | Where-Object { $_ -ne 'pass' }).Count -eq 0) {
        return 'pass'
    }
    if (($statuses | Where-Object { $_ -eq 'blocked' }).Count -gt 0) {
        return 'blocked'
    }
    if (($statuses | Where-Object { $_ -eq 'fail' }).Count -gt 0) {
        return 'fail'
    }
    return 'unmeasurable'
}

function Write-Verdict {
    param(
        [Parameter(Mandatory)][string]$RunBlobPrefix,
        [Parameter(Mandatory)]$Verdict
    )
    $json = ($Verdict | ConvertTo-Json -Depth 12) + [Environment]::NewLine
    Put-BlobText -Name "$RunBlobPrefix/verdict.json" -Text $json
    $sha = Get-Sha256Text -Text $json
    Put-BlobText -Name "$RunBlobPrefix/verdict.json.ready" -Text ($sha + [Environment]::NewLine)
}

function New-BlockedVerdict {
    param(
        [Parameter(Mandatory)][string]$RunId,
        [Parameter(Mandatory)][string]$VmName,
        [Parameter(Mandatory)][string]$Diagnosis,
        [string]$RunStartUtc = ''
    )
    return [ordered]@{
        schemaVersion = 1
        runId = $RunId
        vmName = $VmName
        agentSha = Get-AgentSha
        runStartUtc = $RunStartUtc
        agentStartedUtc = $AgentStartedUtc.ToString('o')
        finishedUtc = [datetime]::UtcNow.ToString('o')
        overall = 'blocked'
        diagnosis = $Diagnosis
        steps = @()
        diffKey = ''
    }
}

function Process-RunDirectory {
    param(
        [Parameter(Mandatory)][string]$RunBlobPrefix,
        [Parameter(Mandatory)][string]$VmName
    )
    $requestBlob = "$RunBlobPrefix/request.json"
    $readyBlob = "$requestBlob.ready"
    $verdictBlob = "$RunBlobPrefix/verdict.json"
    if (-not (Test-Blob -Name $readyBlob) -or (Test-Blob -Name $verdictBlob)) {
        return $false
    }

    $requestText = Get-BlobText -Name $requestBlob
    $readyText = Get-BlobText -Name $readyBlob
    if ($null -eq $requestText -or $null -eq $readyText) {
        return $false
    }
    $expectedSha = $readyText.Trim().ToLowerInvariant()
    $actualSha = Get-Sha256Text -Text $requestText
    $runIdFromPrefix = ($RunBlobPrefix -split '/')[-1]
    if ($actualSha -cne $expectedSha) {
        Write-Log "torn request $requestBlob expected=$expectedSha actual=$actualSha"
        $blocked = New-BlockedVerdict -RunId $runIdFromPrefix -VmName $VmName -Diagnosis 'torn-request'
        Write-Verdict -RunBlobPrefix $RunBlobPrefix -Verdict $blocked
        return $true
    }

    try {
        $request = $requestText | ConvertFrom-Json
        $schemaVersion = [int](Get-PropertyValue -Object $request -Name 'schemaVersion' -Required)
        if ($schemaVersion -ne 1) {
            throw "VMQA_UNSUPPORTED_REQUEST_SCHEMA: $schemaVersion"
        }
        $runId = [string](Get-PropertyValue -Object $request -Name 'runId' -Required)
        $runStartRaw = [string](Get-PropertyValue -Object $request -Name 'runStartUtc' -Required)
        $runStartUtc = ([datetime]$runStartRaw).ToUniversalTime()
        [void](Get-PropertyValue -Object $request -Name 'identifier' -Required)
        $steps = @(Get-PropertyValue -Object $request -Name 'steps' -Required)
    } catch {
        $blocked = New-BlockedVerdict -RunId $runIdFromPrefix -VmName $VmName -Diagnosis ('invalid-request: ' + $_.Exception.Message)
        Write-Verdict -RunBlobPrefix $RunBlobPrefix -Verdict $blocked
        return $true
    }

    $runNonce = Get-RunNonce
    $stepResults = @()
    foreach ($step in $steps) {
        try {
            $stepResults += Invoke-Step -Step $step -Request $request -RunBlobPrefix $RunBlobPrefix -RunId $runId -VmName $VmName -RunNonce $runNonce -RunStartUtc $runStartUtc
        } catch {
            $stepId = 'unknown'
            $verb = 'unknown'
            try { $stepId = [string](Get-PropertyValue -Object $step -Name 'id' -Default 'unknown') } catch { $stepId = 'unknown' }
            try { $verb = [string](Get-PropertyValue -Object $step -Name 'verb' -Default 'unknown') } catch { $verb = 'unknown' }
            $detail = $_.Exception.Message
            Write-Log "step error run=$runId step=$stepId verb=$verb error=$detail"
            $stepResults += New-StepResult -Id $stepId -Verb $verb -Status 'blocked' -Detail $detail
        }
    }

    $overall = Get-OverallStatus -Steps $stepResults
    $diffKey = (($stepResults | ForEach-Object { '{0}={1}' -f $_['id'], $_['status'] }) -join ';')
    $verdict = [ordered]@{
        schemaVersion = 1
        runId = $runId
        vmName = $VmName
        agentSha = Get-AgentSha
        runStartUtc = $runStartUtc.ToString('o')
        agentStartedUtc = $AgentStartedUtc.ToString('o')
        finishedUtc = [datetime]::UtcNow.ToString('o')
        overall = $overall
        steps = @($stepResults)
        diffKey = $diffKey
    }
    if ($overall -eq 'blocked') {
        $blockedDiagnosis = 'blocked'
        foreach ($stepResult in $stepResults) {
            if ($stepResult['status'] -eq 'blocked') {
                $blockedDiagnosis = $stepResult['detail']
                break
            }
        }
        $verdict['diagnosis'] = $blockedDiagnosis
    }
    Write-Verdict -RunBlobPrefix $RunBlobPrefix -Verdict $verdict
    Write-Log "verdict run=$runId overall=$overall diffKey=$diffKey"
    return $true
}

function Get-PendingRunDirectories {
    param([Parameter(Mandatory)][string]$VmName)
    $prefix = "runs/$VmName/"
    $names = @(Get-BlobList -Prefix $prefix)
    $runIds = @{}
    foreach ($name in $names) {
        $rest = $name.Substring($prefix.Length)
        $runId = ($rest -split '/')[0]
        if (-not [string]::IsNullOrWhiteSpace($runId)) {
            $runIds[$runId] = $true
        }
    }
    $pending = @()
    foreach ($runId in @($runIds.Keys | Sort-Object)) {
        $runPrefix = "$prefix$runId"
        if (($names -contains "$runPrefix/request.json.ready") -and (-not ($names -contains "$runPrefix/verdict.json"))) {
            $pending += $runPrefix
        }
    }
    return $pending
}

$backoffSeconds = [Math]::Max(1, $PollSeconds)
$vmName = $null
Write-Log "agent starting task=$TaskName blob=$BlobContainerUri user=$InteractiveUser"

while ($true) {
    try {
        if ([string]::IsNullOrWhiteSpace($vmName)) {
            $vmName = Get-VmNameFromImds
        }
        Write-Heartbeat -VmName $vmName
        $backoffSeconds = [Math]::Max(1, $PollSeconds)

        $pending = @(Get-PendingRunDirectories -VmName $vmName)
        foreach ($runBlobPrefix in $pending) {
            [void](Process-RunDirectory -RunBlobPrefix $runBlobPrefix -VmName $vmName)
            break
        }

        if ($RunOnce) {
            break
        }
        Start-Sleep -Seconds $PollSeconds
    } catch {
        $failure = $_.Exception.Message
        Write-Log "loop error: $failure"
        try {
            if ([string]::IsNullOrWhiteSpace($vmName)) {
                $vmName = Get-VmNameFromImds
            }
            Write-Heartbeat -VmName $vmName -BlobState 'failed' -Failure $failure
        } catch {
            Write-Log ("heartbeat failed: " + $_.Exception.Message)
        }
        if ($RunOnce) {
            break
        }
        Start-Sleep -Seconds $backoffSeconds
        $backoffSeconds = [Math]::Min(60, [Math]::Max(1, $backoffSeconds * 2))
    }
}
