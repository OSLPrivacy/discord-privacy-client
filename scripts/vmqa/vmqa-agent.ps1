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

# Guard on Azure IMDS, not the Windows hostname. This fleet's hostnames are OSLCLIENT1 etc, which
# do not match 'OSL-*' (that needs a literal hyphen), so a hostname guard refused to load on the
# exact machines it exists to permit. IMDS is also unspoofable off-Azure: 169.254.169.254 is
# link-local and answers only on an Azure VM, and it returns the authoritative resource name
# rather than something a user can rename.
$VmqaAllowedVms = @(
    'OSL-Azure-Client-1', 'OSL-Azure-Client-2',
    'OSL-Independent-Client-1', 'OSL-Independent-Client-2',
    'OSL-WhatsApp-Client-1', 'OSL-WhatsApp-Client-2',
    'OSL-Telegram-QA-1', 'OSL-Telegram-QA-2',
    'OSL-Signal-Client-1', 'OSL-Signal-Client-2'
)
try {
    $VmqaImdsName = ([string](Invoke-RestMethod -Method Get -TimeoutSec 5 `
        -Uri 'http://169.254.169.254/metadata/instance/compute/name?api-version=2021-02-01&format=text' `
        -Headers @{ Metadata = 'true' })).Trim()
} catch {
    throw "VMQA_NOT_A_QA_VM: Azure IMDS did not answer, so this is not a QA VM (hostname '$env:COMPUTERNAME'). The VMQA agent synthesizes input and refuses to run here."
}
if ($VmqaAllowedVms -notcontains $VmqaImdsName) {
    throw "VMQA_WRONG_MACHINE: IMDS reports '$VmqaImdsName', which is not in the QA fleet allow-list. Refusing to run the VMQA agent."
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

function Get-Sha256Bytes {
    param([Parameter(Mandatory)][byte[]]$Bytes)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($Bytes)) -replace '-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Clear-StorageToken {
    $script:StorageToken = $null
    $script:StorageTokenExpiresUtc = [datetime]::MinValue
}

function Get-StorageToken {
    # Do the arithmetic on UtcNow, not on the cached expiry. The expiry starts at
    # [datetime]::MinValue, and MinValue.AddMinutes(-5) UNDERFLOWS with "the added or subtracted
    # value results in an un-representable DateTime" — thrown on the very first call, before a
    # token was ever fetched, so the agent could not make a single blob request. Adding to UtcNow
    # cannot overflow, and it also short-circuits on the empty-token check first.
    if (-not [string]::IsNullOrWhiteSpace($script:StorageToken) -and
        $script:StorageTokenExpiresUtc -gt [datetime]::UtcNow.AddMinutes(5)) {
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
    # Probe for the property instead of dereferencing it. Under Set-StrictMode -Version Latest,
    # touching a property the object does not have is a THROWN error, so reading
    # $ErrorRecord.Exception.Response directly turns every non-web exception into
    # "The property 'Response' cannot be found on this object" — an error handler that
    # manufactures its own failure and destroys the original one. That masked the real blob
    # error for an entire agent run and looked like a transport outage.
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        $responseProperty = $ex.PSObject.Properties['Response']
        if ($null -ne $responseProperty -and $null -ne $responseProperty.Value) {
            $statusProperty = $responseProperty.Value.PSObject.Properties['StatusCode']
            if ($null -ne $statusProperty -and $null -ne $statusProperty.Value) {
                $status = $statusProperty.Value
                if ($status -is [int]) { return [int]$status }
                $enumValue = $status.PSObject.Properties['value__']
                if ($null -ne $enumValue) { return [int]$enumValue.Value }
            }
        }
        $ex = $ex.InnerException
    }
    return $null
}

function Get-ErrorDetail {
    <# The full exception chain, so a failure is diagnosable from the log alone. An agent that
       cannot be reached except by RDP is exactly what this transport exists to avoid. #>
    param([Parameter(Mandatory)]$ErrorRecord)
    $parts = @()
    $ex = $ErrorRecord.Exception
    while ($null -ne $ex) {
        $parts += "$($ex.GetType().FullName): $($ex.Message)"
        $ex = $ex.InnerException
    }
    return ($parts -join ' <- ')
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
        # Decode bytes explicitly. These blobs are uploaded as application/octet-stream, so
        # Invoke-WebRequest hands back a byte[], and [string] on a byte[] renders it as
        # space-separated DECIMAL BYTE VALUES ("102 97 49 ...") rather than the text. That made
        # every request's sha256 comparison fail and graded legitimate requests 'torn-request' -
        # a harness that refuses to run anything while reporting a plausible-looking reason.
        $content = $response.Content
        if ($content -is [byte[]]) {
            $text = [Text.UTF8Encoding]::new($false).GetString($content)
        } else {
            $text = [string]$content
        }
        # Decoding the bytes as UTF-8 turns any real BOM into a single U+FEFF, which trims
        # cleanly. This must stay generic: a .ready blob is a bare 64-char hex digest with no
        # JSON punctuation to anchor on, so anything cleverer than a BOM trim risks corrupting it.
        return $text.TrimStart([char]0xFEFF)
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

function Put-BlobBytes {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][byte[]]$Bytes
    )
    [void](Invoke-BlobRequest -Method PUT -Name $Name -Body $Bytes -ContentType 'application/octet-stream' -AdditionalHeaders @{ 'x-ms-blob-type' = 'BlockBlob' })
}

function Put-BlobFile {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][string]$Path
    )
    $bytes = [IO.File]::ReadAllBytes($Path)
    Put-BlobBytes -Name $Name -Bytes $bytes
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
        # Strip the byte-order mark before the [xml] cast. Azure returns the List Blobs body
        # BOM-prefixed, and casting a string that starts with U+FEFF throws "the specified node
        # cannot be inserted as the valid child of this node" - which names the XML tree and reads
        # like malformed markup rather than a leading character. Same BOM family as the trap where
        # a BOM makes the app's QA trigger fall through to the legacy SEND verb.
        $listBody = [string]$response.Content
        # Trim to the first '<' rather than to U+FEFF. Invoke-WebRequest hands back the BOM as the
        # three mis-decoded characters "ï»¿" (the raw EF BB BF read as Latin-1), NOT as a single
        # U+FEFF, so TrimStart([char]0xFEFF) silently matches nothing and the cast still fails.
        # Cutting to the first angle bracket is decoding-agnostic and cannot be fooled by however
        # the stream happened to be interpreted.
        $angle = $listBody.IndexOf('<')
        if ($angle -gt 0) { $listBody = $listBody.Substring($angle) }
        [xml]$xml = $listBody
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
        [Parameter(Mandatory)][string]$ExeSha256,
        # Carry the real step id in. Passing -Id '' to New-StepResult fails to bind, because that
        # parameter is Mandatory and Mandatory rejects an empty string - so `stage` threw before it
        # could report anything, and the caller's later $result['id'] = $stepId never ran.
        [Parameter(Mandatory)][string]$StepId
    )
    $sha = $ExeSha256.ToLowerInvariant()
    $sourceExe = "builds/$sha/osl-privacy-hub.exe"
    $sourceDll = "builds/$sha/WebView2Loader.dll"
    $destDir = Join-Path 'C:\OSL-VMQA' $sha
    $destExe = Join-Path $destDir 'osl-privacy-hub.exe'
    $destDll = Join-Path $destDir 'WebView2Loader.dll'

    New-Item -ItemType Directory -Force -Path $destDir | Out-Null
    if (-not (Get-BlobFile -Name $sourceExe -Path $destExe)) {
        return New-StepResult -Id $StepId -Verb 'stage' -Status 'fail' -Detail "missing source exe: $sourceExe"
    }
    # WebView2Loader.dll must live beside the exe; without it the process hangs before main with no
    # trace file, which is indistinguishable from a corrupt build at the harness layer.
    if (-not (Get-BlobFile -Name $sourceDll -Path $destDll)) {
        return New-StepResult -Id $StepId -Verb 'stage' -Status 'fail' -Detail "missing source WebView2Loader.dll: $sourceDll"
    }

    $destExeHash = Get-Sha256 -Path $destExe
    $destDllHash = Get-Sha256 -Path $destDll
    if ($destExeHash -cne $sha) {
        return New-StepResult -Id $StepId -Verb 'stage' -Status 'fail' -Detail "staged exe sha mismatch expected=$sha actual=$destExeHash"
    }
    return New-StepResult -Id $StepId -Verb 'stage' -Status 'pass' -Detail "staged=$destDir exeSha=$destExeHash webview2Sha=$destDllHash"
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
            return Copy-And-VerifyBuild -ExeSha256 $exeSha -StepId $stepId
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
            # Resolve the subject BEFORE capturing, even though the capture is full-desktop.
            # Without this, `shot` passed on a global distinct-colour count that the wallpaper and
            # taskbar satisfy on their own, so it went green with the application absent, black,
            # covered or minimised — and the self-test leans on distinctColors as proof that the
            # apparatus measured something. A colour count nobody has tied to the subject is not
            # evidence about the subject.
            $shotSubject = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $shotSubject['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $shotSubject['status'] -Detail ("no subject to photograph: " + $shotSubject['detail'])
            }
            $shotRect = Get-VmqaWindowRect -Subject $shotSubject['subject']
            $artifactDir = Join-Path (Join-Path $AgentRoot 'artifacts') $RunId
            $artifactPath = Join-Path $artifactDir ($safeName + '.png')
            $shot = Invoke-FullDesktopShot -OutPath $artifactPath
            $fresh = Test-ArtifactFreshForRun -Path $artifactPath -RunStartUtc $RunStartUtc
            $artifactRel = 'artifacts/' + ($safeName + '.png')
            if (-not $fresh['fresh']) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail $fresh['detail'] -Artifacts @($artifactRel)
            }
            # The verdict is only meaningful if the uploaded bytes are the same bytes named by the
            # measurement; a path re-read can race another agent or retry.
            $pngBytes = [IO.File]::ReadAllBytes($artifactPath)
            $pngSha256 = Get-Sha256Bytes -Bytes $pngBytes
            Put-BlobBytes -Name "$RunBlobPrefix/$artifactRel" -Bytes $pngBytes
            # Report the subject alongside the pixels, so a reader can tell WHICH window this
            # frame is evidence about rather than inferring it from the filename.
            $detail = 'path={0}; width={1}; height={2}; distinctColors={3}; pngSha256={4}; subjectPid={5}; subjectRect={6},{7} {8}x{9}' -f `
                $artifactRel, $shot['width'], $shot['height'], $shot['distinctColors'], $pngSha256, `
                $shotSubject['subject'].Pid, $shotRect.Left, $shotRect.Top, $shotRect.Width, $shotRect.Height
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
            # Undeliverable input is 'unmeasurable', never 'pass'. The click did not reach the
            # subject, so nothing was learned about how the app responds - and reporting a pass
            # here is the "something else worked" false green in its most literal form.
            try {
                $click = Invoke-VmqaClick -Subject $resolved['subject'] -WinX $winX -WinY $winY -SettleMs $settleMs -RunNonce $RunNonce
            } catch {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail ([string]$_.Exception.Message)
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail ("screenX={0}; screenY={1}; foreground={2}; cursorAtTarget={3}; pixelOwnedBySubject={4}" -f $click.ScreenX, $click.ScreenY, $click.Foreground, $click.CursorAtTarget, $click.PixelOwnedBySubject)
        }
        'type' {
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $text = [string](Get-PropertyValue -Object $stepArgs -Name 'text' -Default '')
            $settleMs = [int](Get-PropertyValue -Object $stepArgs -Name 'settleMs' -Default 400)
            try {
                Invoke-VmqaType -Subject $resolved['subject'] -Text $text -SettleMs $settleMs -RunNonce $RunNonce
            } catch {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail ([string]$_.Exception.Message)
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail ("chars={0}" -f $text.Length)
        }
        'key' {
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] -Detail $resolved['detail']
            }
            $key = [string](Get-PropertyValue -Object $stepArgs -Name 'key' -Required)
            $settleMs = [int](Get-PropertyValue -Object $stepArgs -Name 'settleMs' -Default 400)
            try {
                Invoke-VmqaKey -Subject $resolved['subject'] -Key $key -SettleMs $settleMs -RunNonce $RunNonce
            } catch {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail ([string]$_.Exception.Message)
            }
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
    # Every filter result is wrapped in @() before .Count. Where-Object that matches nothing
    # returns $null, and under Set-StrictMode -Version Latest $null.Count is a thrown
    # PropertyNotFoundException — so a run in which EVERY step passed (the empty not-pass filter)
    # crashed the grader instead of returning 'pass'. The all-green path was the broken one, which
    # is why it survived every earlier failing run.
    $statuses = @($Steps | ForEach-Object { $_['status'] })
    if (@($statuses | Where-Object { $_ -ne 'pass' }).Count -eq 0) {
        return 'pass'
    }
    if (@($statuses | Where-Object { $_ -eq 'blocked' }).Count -gt 0) {
        return 'blocked'
    }
    if (@($statuses | Where-Object { $_ -eq 'fail' }).Count -gt 0) {
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
        [string]$RequestSha256 = '',
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
        requestSha256 = $RequestSha256
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
        $blocked = New-BlockedVerdict -RunId $runIdFromPrefix -VmName $VmName -Diagnosis 'torn-request' -RequestSha256 $actualSha
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
        $blocked = New-BlockedVerdict -RunId $runIdFromPrefix -VmName $VmName -Diagnosis ('invalid-request: ' + $_.Exception.Message) -RequestSha256 $actualSha
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
        # Bind the verdict to the exact request bytes that produced it. A runId alone does not do
        # this: reusing a runId (which `run --run-id` permits) would let a verdict left by an
        # earlier attempt be graded as this run's result. The digest is already computed to validate
        # the .ready sentinel, so binding it costs nothing and closes the reuse case completely.
        requestSha256 = $actualSha
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
        $failure = Get-ErrorDetail -ErrorRecord $_
        Write-Log "loop error: $failure"
        try {
            if ([string]::IsNullOrWhiteSpace($vmName)) {
                $vmName = Get-VmNameFromImds
            }
            Write-Heartbeat -VmName $vmName -BlobState 'failed' -Failure $failure
        } catch {
            Write-Log ("heartbeat failed: " + (Get-ErrorDetail -ErrorRecord $_))
        }
        if ($RunOnce) {
            break
        }
        Start-Sleep -Seconds $backoffSeconds
        $backoffSeconds = [Math]::Min(60, [Math]::Max(1, $backoffSeconds * 2))
    }
}
