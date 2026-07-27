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

function Get-Win32Sha {
    if (Test-Path -LiteralPath $Win32ModulePath -PathType Leaf) {
        return Get-Sha256 -Path $Win32ModulePath
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
        win32Sha256 = Get-Win32Sha
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

function Assert-ExactJsonProperties {
    param(
        [Parameter(Mandatory)]$Object,
        [Parameter(Mandatory)][AllowEmptyCollection()][string[]]$Names,
        [Parameter(Mandatory)][string]$Context
    )
    if ($null -eq $Object) {
        throw "VMQA_SCHEMA_NOT_OBJECT: $Context"
    }
    $actual = @($Object.PSObject.Properties.Name)
    $unknown = @($actual | Where-Object { $_ -notin $Names })
    $missing = @($Names | Where-Object { $_ -notin $actual })
    if ($unknown.Count -gt 0 -or $missing.Count -gt 0) {
        throw ("VMQA_SCHEMA_FIELDS_NOT_EXACT: {0} missing=[{1}] unknown=[{2}]" -f `
            $Context, [string]::Join(',', $missing), [string]::Join(',', $unknown))
    }
}

function Assert-JsonString {
    param([Parameter(Mandatory)]$Value, [Parameter(Mandatory)][string]$Context, [switch]$AllowEmpty)
    if ($Value -isnot [string] -or (-not $AllowEmpty -and [string]::IsNullOrEmpty($Value))) {
        throw "VMQA_SCHEMA_NOT_STRING: $Context"
    }
}

function Assert-JsonInteger {
    param([Parameter(Mandatory)]$Value, [Parameter(Mandatory)][string]$Context, [int64]$Minimum = [int64]::MinValue)
    if (($Value -isnot [int]) -and ($Value -isnot [int64])) {
        throw "VMQA_SCHEMA_NOT_INTEGER: $Context"
    }
    if ([int64]$Value -lt $Minimum) {
        throw "VMQA_SCHEMA_INTEGER_RANGE: $Context"
    }
}

function Assert-JsonBoolean {
    param([Parameter(Mandatory)]$Value, [Parameter(Mandatory)][string]$Context)
    if ($Value -isnot [bool]) {
        throw "VMQA_SCHEMA_NOT_BOOLEAN: $Context"
    }
}

function Assert-StrictBuildIdentity {
    param(
        [Parameter(Mandatory)]$Identity,
        [Parameter(Mandatory)][string]$ExpectedExeSha256
    )
    Assert-ExactJsonProperties -Object $Identity `
        -Names @('schemaVersion','source','ui','build','artifacts','evidence') -Context 'buildIdentity'
    Assert-JsonInteger -Value $Identity.schemaVersion -Context 'buildIdentity.schemaVersion'
    if ([int]$Identity.schemaVersion -ne 2) {
        throw "VMQA_UNSUPPORTED_BUILD_IDENTITY_SCHEMA: $($Identity.schemaVersion)"
    }
    Assert-ExactJsonProperties -Object $Identity.source `
        -Names @('commit','tree','clean','dirtyFingerprint') -Context 'buildIdentity.source'
    Assert-ExactJsonProperties -Object $Identity.ui `
        -Names @('distSha256') -Context 'buildIdentity.ui'
    Assert-ExactJsonProperties -Object $Identity.build `
        -Names @('target','features','profile','commands','toolchain') -Context 'buildIdentity.build'
    Assert-ExactJsonProperties -Object $Identity.build.toolchain `
        -Names @('rustc','cargo','node','npm','oslCargoSha256') -Context 'buildIdentity.build.toolchain'
    Assert-ExactJsonProperties -Object $Identity.artifacts `
        -Names @('executable','loader') -Context 'buildIdentity.artifacts'
    Assert-ExactJsonProperties -Object $Identity.artifacts.executable `
        -Names @('name','sha256','sizeBytes') -Context 'buildIdentity.artifacts.executable'
    Assert-ExactJsonProperties -Object $Identity.artifacts.loader `
        -Names @('name','sha256','sizeBytes') -Context 'buildIdentity.artifacts.loader'
    Assert-ExactJsonProperties -Object $Identity.evidence `
        -Names @('sourceArchiveSha256','distArchiveSha256','distManifestSha256',`
            'npmBuildLogSha256','cargoBuildLogSha256','buildLogSha256') `
        -Context 'buildIdentity.evidence'
    Assert-JsonString -Value $Identity.source.commit -Context 'buildIdentity.source.commit'
    Assert-JsonString -Value $Identity.source.tree -Context 'buildIdentity.source.tree'
    Assert-JsonBoolean -Value $Identity.source.clean -Context 'buildIdentity.source.clean'
    Assert-JsonString -Value $Identity.source.dirtyFingerprint -Context 'buildIdentity.source.dirtyFingerprint'
    Assert-JsonString -Value $Identity.ui.distSha256 -Context 'buildIdentity.ui.distSha256'
    Assert-JsonString -Value $Identity.build.target -Context 'buildIdentity.build.target'
    Assert-JsonString -Value $Identity.build.profile -Context 'buildIdentity.build.profile'
    if ($Identity.build.features -isnot [System.Array] -or $Identity.build.commands -isnot [System.Array]) {
        throw 'VMQA_SCHEMA_BUILD_ARRAY_REQUIRED'
    }
    foreach ($feature in @($Identity.build.features)) {
        Assert-JsonString -Value $feature -Context 'buildIdentity.build.features[]'
    }
    foreach ($command in @($Identity.build.commands)) {
        if ($command -isnot [System.Array]) { throw 'VMQA_SCHEMA_COMMAND_ARRAY_REQUIRED' }
        foreach ($token in @($command)) {
            Assert-JsonString -Value $token -Context 'buildIdentity.build.commands[][]'
        }
    }
    foreach ($name in @('rustc','cargo','node','npm','oslCargoSha256')) {
        Assert-JsonString -Value $Identity.build.toolchain.$name -Context "buildIdentity.build.toolchain.$name"
    }
    foreach ($artifactName in @('executable','loader')) {
        $artifact = $Identity.artifacts.$artifactName
        Assert-JsonString -Value $artifact.name -Context "buildIdentity.artifacts.$artifactName.name"
        Assert-JsonString -Value $artifact.sha256 -Context "buildIdentity.artifacts.$artifactName.sha256"
        Assert-JsonInteger -Value $artifact.sizeBytes -Context "buildIdentity.artifacts.$artifactName.sizeBytes" -Minimum 1
    }
    foreach ($name in @('sourceArchiveSha256','distArchiveSha256','distManifestSha256',`
            'npmBuildLogSha256','cargoBuildLogSha256','buildLogSha256')) {
        Assert-JsonString -Value $Identity.evidence.$name -Context "buildIdentity.evidence.$name"
    }
    $features = @($Identity.build.features)
    $commands = @($Identity.build.commands)
    $command0 = if ($commands.Count -ge 1) {
        [string]::Join([char]31, @($commands[0] | ForEach-Object { [string]$_ }))
    } else { '' }
    $command1 = if ($commands.Count -ge 2) {
        [string]::Join([char]31, @($commands[1] | ForEach-Object { [string]$_ }))
    } else { '' }
    $expectedCommand0 = [string]::Join([char]31, @('npm','run','build'))
    $expectedCommand1 = [string]::Join([char]31, @(
        'osl-cargo','build','--release','--features','desktop','--bin',
        'osl-privacy-hub','--target','x86_64-pc-windows-gnu'
    ))
    if ([string]$Identity.source.commit -cnotmatch '^[0-9a-f]{40}$' -or
        [string]$Identity.source.tree -cnotmatch '^[0-9a-f]{40}$' -or
        $Identity.source.clean -ne $true -or
        [string]$Identity.source.dirtyFingerprint -cne 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855' -or
        [string]$Identity.ui.distSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.build.target -cne 'x86_64-pc-windows-gnu' -or
        $features.Count -ne 1 -or
        [string]$features[0] -cne 'desktop' -or
        [string]$Identity.build.profile -cne 'release' -or
        $commands.Count -ne 2 -or
        $command0 -cne $expectedCommand0 -or
        $command1 -cne $expectedCommand1 -or
        [string]$Identity.build.toolchain.rustc -eq '' -or
        [string]$Identity.build.toolchain.cargo -eq '' -or
        [string]$Identity.build.toolchain.node -eq '' -or
        [string]$Identity.build.toolchain.npm -eq '' -or
        [string]$Identity.build.toolchain.oslCargoSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.artifacts.executable.name -cne 'osl-privacy-hub.exe' -or
        [string]$Identity.artifacts.executable.sha256 -cne $ExpectedExeSha256 -or
        [int64]$Identity.artifacts.executable.sizeBytes -le 0 -or
        [string]$Identity.artifacts.loader.name -cne 'WebView2Loader.dll' -or
        [string]$Identity.artifacts.loader.sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [int64]$Identity.artifacts.loader.sizeBytes -le 0 -or
        [string]$Identity.evidence.sourceArchiveSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.evidence.distArchiveSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.evidence.distManifestSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.evidence.npmBuildLogSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.evidence.cargoBuildLogSha256 -cnotmatch '^[0-9a-f]{64}$' -or
        [string]$Identity.evidence.buildLogSha256 -cnotmatch '^[0-9a-f]{64}$') {
        throw 'VMQA_INVALID_BUILD_IDENTITY'
    }
}

function Assert-StrictRequestSteps {
    param([Parameter(Mandatory)][object[]]$Steps)
    if ($Steps.Count -le 0) {
        throw 'VMQA_REQUEST_STEPS_EMPTY'
    }
    foreach ($step in $Steps) {
        Assert-ExactJsonProperties -Object $step -Names @('id','verb','args') -Context 'step'
        Assert-JsonString -Value $step.id -Context 'step.id'
        Assert-JsonString -Value $step.verb -Context 'step.verb'
        $verb = ([string]$step.verb).ToLowerInvariant()
        $allowedArgs = switch ($verb) {
            'stage' { @('exeSha256') }
            'launch' { @('exeSha256','timeoutSeconds') }
            'ping' { @() }
            'shot' { @('name','expectedSurfaceClass') }
            'click' { @('winX','winY','settleMs') }
            'type' { @('text','settleMs') }
            'key' { @('key','settleMs') }
            'wait' { @('ms') }
            'kill' { @() }
            default { throw "VMQA_UNSUPPORTED_VERB: $verb" }
        }
        Assert-ExactJsonProperties -Object $step.args -Names $allowedArgs -Context "step.$verb.args"
        switch ($verb) {
            'stage' {
                Assert-JsonString -Value $step.args.exeSha256 -Context 'step.stage.args.exeSha256'
            }
            'launch' {
                Assert-JsonString -Value $step.args.exeSha256 -Context 'step.launch.args.exeSha256'
                Assert-JsonInteger -Value $step.args.timeoutSeconds -Context 'step.launch.args.timeoutSeconds' -Minimum 1
            }
            'shot' {
                Assert-JsonString -Value $step.args.name -Context 'step.shot.args.name'
                Assert-JsonString -Value $step.args.expectedSurfaceClass -Context 'step.shot.args.expectedSurfaceClass'
            }
            'click' {
                Assert-JsonInteger -Value $step.args.winX -Context 'step.click.args.winX'
                Assert-JsonInteger -Value $step.args.winY -Context 'step.click.args.winY'
                Assert-JsonInteger -Value $step.args.settleMs -Context 'step.click.args.settleMs' -Minimum 0
            }
            'type' {
                Assert-JsonString -Value $step.args.text -Context 'step.type.args.text'
                Assert-JsonInteger -Value $step.args.settleMs -Context 'step.type.args.settleMs' -Minimum 0
            }
            'key' {
                Assert-JsonString -Value $step.args.key -Context 'step.key.args.key'
                Assert-JsonInteger -Value $step.args.settleMs -Context 'step.key.args.settleMs' -Minimum 0
            }
            'wait' {
                Assert-JsonInteger -Value $step.args.ms -Context 'step.wait.args.ms' -Minimum 0
            }
        }
    }
}

function New-StepResult {
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][string]$Verb,
        [Parameter(Mandatory)][ValidateSet('pass','fail','unmeasurable','blocked')][string]$Status,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Detail,
        [string[]]$Artifacts = @(),
        $Facts = ([ordered]@{})
    )
    return [ordered]@{
        id = $Id
        verb = $Verb
        status = $Status
        detail = $Detail
        artifacts = @($Artifacts)
        facts = $Facts
    }
}

# The pid `launch` established for this run. Once set, every later verb must resolve to THAT
# process, not merely to something wearing the same window class.
$script:VmqaPinnedPid = 0
# A negative launch deliberately fails marker-class resolution after Start-Process. Keep that
# started PID/path separately from trusted-subject provenance so `kill` can remove the exact
# process without ever authorising it as a capture or input target.
$script:VmqaStartedPid = 0
$script:VmqaStartedExePath = ''
$script:VmqaStartedExeSha256 = ''

function Resolve-StepSubject {
    param(
        [Parameter(Mandatory)][string]$Identifier,
        [Parameter(Mandatory)][string]$RunNonce
    )
    try {
        $subject = Resolve-VmqaSubject -Identifier $Identifier -RunNonce $RunNonce
        Assert-VmqaSubject -Subject $subject -RunNonce $RunNonce | Out-Null
        # `launch` verifies the started pid AND its image path, but every later verb re-resolves by
        # window class alone. If the launched process exits and another one carrying the same
        # identifier appears, the run would silently change subject mid-flight and keep grading -
        # the wrong-process false green, arriving late instead of at the start. Pinning the pid
        # makes the swap a hard stop rather than an invisible substitution.
        if ($script:VmqaPinnedPid -ne 0 -and $subject.Pid -ne $script:VmqaPinnedPid) {
            return [ordered]@{ ok = $false; subject = $null; status = 'blocked'
                detail = "VMQA_SUBJECT_SWAPPED: resolved pid $($subject.Pid) is not the launched pid $($script:VmqaPinnedPid)"
                markerWindowsTotal = $subject.MarkerWindowsTotal }
        }
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

function Stop-StagedBuildProcesses {
    param([Parameter(Mandatory)][string]$ExePath)

    if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
        return @()
    }
    $canonicalExePath = Get-CanonicalFilePath -Path $ExePath
    $stopped = [Collections.Generic.List[int]]::new()
    foreach ($process in @(Get-Process -ErrorAction SilentlyContinue)) {
        $processPath = $null
        try { $processPath = $process.Path } catch { continue }
        if (-not [string]::Equals($processPath, $canonicalExePath, [StringComparison]::OrdinalIgnoreCase)) {
            continue
        }
        # This is not a process-name guess: the candidate's live image path must be the exact
        # content-addressed executable this stage is about. A failed launch can leave a
        # marker-less process holding that file, so the later marker-only kill cannot find it.
        Stop-Process -Id $process.Id -Force -ErrorAction Stop
        Wait-Process -Id $process.Id -Timeout 10 -ErrorAction SilentlyContinue
        [void]$stopped.Add($process.Id)
    }
    return @($stopped)
}

function Get-ExactExecutableProcessPids {
    param([Parameter(Mandatory)][string]$CanonicalExePath)

    $pids = [Collections.Generic.List[int]]::new()
    foreach ($process in @(Get-Process -ErrorAction SilentlyContinue)) {
        $processPath = $null
        try { $processPath = $process.Path } catch { continue }
        if ([string]::Equals(
                $processPath, $CanonicalExePath, [StringComparison]::OrdinalIgnoreCase)) {
            [void]$pids.Add([int]$process.Id)
        }
    }
    return @($pids)
}

function Wait-VmqaProcessIdAbsent {
    param(
        [Parameter(Mandatory)][int]$ProcessId,
        [int]$TimeoutMilliseconds = 10000
    )
    $deadline = [datetime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    do {
        if ($null -eq (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)) {
            return $true
        }
        Start-Sleep -Milliseconds 200
    } while ([datetime]::UtcNow -lt $deadline)
    return $null -eq (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
}

function Get-CleanupFacts {
    param(
        # `$PID` is a read-only automatic variable and PowerShell names are
        # case-insensitive. Naming this parameter `Pid` makes binding throw
        # before cleanup can report anything.
        [Parameter(Mandatory)][int]$ProcessId,
        [Parameter(Mandatory)][string]$CanonicalExePath,
        [Parameter(Mandatory)][string]$ExeSha256,
        [Parameter(Mandatory)][ValidateSet('stopped','already-exited')][string]$Outcome
    )

    $pidAbsent = $null -eq (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue)
    $matchingExePids = @(Get-ExactExecutableProcessPids -CanonicalExePath $CanonicalExePath)
    return [ordered]@{
        cleanupPid = $ProcessId
        cleanupOutcome = $Outcome
        cleanupExeSha256 = $ExeSha256
        cleanupPidAbsent = $pidAbsent
        cleanupExecutableAbsent = ($matchingExePids.Count -eq 0)
        cleanupMatchingExeCount = $matchingExePids.Count
    }
}

function ConvertTo-RectFact {
    param([Parameter(Mandatory)]$Rect)
    return [ordered]@{
        left = [int]$Rect.Left
        top = [int]$Rect.Top
        right = [int]$Rect.Right
        bottom = [int]$Rect.Bottom
        width = [int]($Rect.Right - $Rect.Left)
        height = [int]($Rect.Bottom - $Rect.Top)
    }
}

function Copy-And-VerifyBuild {
    param(
        [Parameter(Mandatory)][string]$ExeSha256,
        [Parameter(Mandatory)][string]$LoaderSha256,
        [Parameter(Mandatory)][int64]$ExeSizeBytes,
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
    $stoppedPids = @(Stop-StagedBuildProcesses -ExePath $destExe)
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
    $destExeSize = (Get-Item -LiteralPath $destExe).Length
    if ($destExeHash -cne $sha -or $destExeSize -ne $ExeSizeBytes) {
        return New-StepResult -Id $StepId -Verb 'stage' -Status 'fail' `
            -Detail "staged exe identity mismatch expectedSha=$sha actualSha=$destExeHash expectedSize=$ExeSizeBytes actualSize=$destExeSize"
    }
    if ($destDllHash -cne $LoaderSha256) {
        return New-StepResult -Id $StepId -Verb 'stage' -Status 'fail' `
            -Detail "staged loader sha mismatch expected=$LoaderSha256 actual=$destDllHash"
    }
    $stoppedDetail = if ($stoppedPids.Count -gt 0) { $stoppedPids -join ',' } else { 'none' }
    return New-StepResult -Id $StepId -Verb 'stage' -Status 'pass' -Detail "staged=$destDir exeSha=$destExeHash webview2Sha=$destDllHash stoppedPriorPids=$stoppedDetail"
}

function Invoke-SurfaceShot {
    param(
        [Parameter(Mandatory)][string]$OutPath,
        [Parameter(Mandatory)]$Surface
    )
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing

    [void](Assert-VmqaTrustedSurfaceBounds -Surface $Surface)
    $virtual = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $left = [int]$Surface.Rect.Left
    $top = [int]$Surface.Rect.Top
    $width = [int]($Surface.Rect.Right - $Surface.Rect.Left)
    $height = [int]($Surface.Rect.Bottom - $Surface.Rect.Top)
    if ($width -lt 200 -or $height -lt 120) {
        throw "VMQA_VISIBLE_SURFACE_TOO_SMALL: ${width}x${height}"
    }
    if ($left -lt $virtual.Left -or $top -lt $virtual.Top -or
        ($left + $width) -gt $virtual.Right -or ($top + $height) -gt $virtual.Bottom) {
        throw "VMQA_VISIBLE_SURFACE_OFFSCREEN: rect=$left,$top ${width}x${height}"
    }
    $bitmap = [System.Drawing.Bitmap]::new($width, $height)
    try {
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.CopyFromScreen(
                [System.Drawing.Point]::new($left, $top),
                [System.Drawing.Point]::Empty,
                [System.Drawing.Size]::new($width, $height))
        } finally {
            $graphics.Dispose()
        }

        $directory = Split-Path -Parent $OutPath
        if ($directory -and -not (Test-Path -LiteralPath $directory)) {
            New-Item -ItemType Directory -Force -Path $directory | Out-Null
        }
        $bitmap.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)

        $colors = [System.Collections.Generic.HashSet[int]]::new()
        for ($y = 0; $y -lt $height; $y += 4) {
            for ($x = 0; $x -lt $width; $x += 4) {
                [void]$colors.Add($bitmap.GetPixel($x, $y).ToArgb())
            }
        }
        return [ordered]@{
            path = $OutPath
            width = $width
            height = $height
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
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail $detail `
                -Facts ([ordered]@{ markerWindowsTotal = $markerWindowsTotal })
        }
        'stage' {
            $exeSha = [string](Get-PropertyValue -Object $stepArgs -Name 'exeSha256' -Required)
            $loaderSha = [string]$Request.buildIdentity.artifacts.loader.sha256
            $exeSize = [int64]$Request.buildIdentity.artifacts.executable.sizeBytes
            return Copy-And-VerifyBuild -ExeSha256 $exeSha -LoaderSha256 $loaderSha `
                -ExeSizeBytes $exeSize -StepId $stepId
        }
        'launch' {
            $exeSha = ([string](Get-PropertyValue -Object $stepArgs -Name 'exeSha256' -Required)).ToLowerInvariant()
            $timeoutSeconds = [int](Get-PropertyValue -Object $stepArgs -Name 'timeoutSeconds' -Default 60)
            # Clear any pin from an earlier launch in this same run. A run may legitimately
            # launch, kill and relaunch; without this the relaunch resolves a new pid, fails the
            # pin check and is blocked for a reason that is not a defect.
            $script:VmqaPinnedPid = 0
            $script:VmqaStartedPid = 0
            $script:VmqaStartedExePath = ''
            $script:VmqaStartedExeSha256 = ''
            $exePath = Join-Path (Join-Path 'C:\OSL-VMQA' $exeSha) 'osl-privacy-hub.exe'
            if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' -Detail "staged exe missing: $exePath"
            }
            $canonicalExePath = Get-CanonicalFilePath -Path $exePath
            $process = Start-Process -FilePath $exePath -PassThru
            $script:VmqaStartedPid = [int]$process.Id
            $script:VmqaStartedExePath = $canonicalExePath
            $script:VmqaStartedExeSha256 = $exeSha
            $startedFacts = [ordered]@{
                launchedPid = [int]$process.Id
                exeSha256 = $exeSha
                launchProcessStarted = $true
            }
            # The marker is resolved once for this step. The blind wait gives Tauri time to publish
            # its single-instance marker without creating a second target-selection path.
            if ($timeoutSeconds -gt 0) {
                Start-Sleep -Seconds $timeoutSeconds
            }
            $resolved = Resolve-StepSubject -Identifier $identifier -RunNonce $RunNonce
            if (-not $resolved['ok']) {
                return New-StepResult -Id $stepId -Verb $verb -Status $resolved['status'] `
                    -Detail $resolved['detail'] -Facts $startedFacts
            }
            $subject = $resolved['subject']
            if ($subject.Pid -ne $process.Id) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' `
                    -Detail "marker pid $($subject.Pid) did not match started pid $($process.Id)" `
                    -Facts $startedFacts
            }
            if (-not [string]::Equals($subject.ExePath, $canonicalExePath, [StringComparison]::OrdinalIgnoreCase)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' `
                    -Detail "marker image path '$($subject.ExePath)' did not match staged exe '$canonicalExePath'" `
                    -Facts $startedFacts
            }
            # Pin from here on. This is the only place a subject's identity is established by
            # something stronger than a window class: we started the process ourselves and matched
            # both its pid and its image path.
            $script:VmqaPinnedPid = $subject.Pid
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' `
                -Detail "pid=$($subject.Pid); exe=$($subject.ExePath)" `
                -Facts ([ordered]@{
                    launchedPid = $subject.Pid
                    exeSha256 = $subject.ExeSha256
                    launchProcessStarted = $true
                })
        }
        'shot' {
            if ($script:VmqaPinnedPid -le 0) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'blocked' `
                    -Detail 'VMQA_NO_LAUNCH_PROVENANCE: shot requires a successful exact-path launch in this run'
            }
            $name = [string](Get-PropertyValue -Object $stepArgs -Name 'name' -Required)
            $expectedSurfaceClass =
                [string](Get-PropertyValue -Object $stepArgs -Name 'expectedSurfaceClass' -Required)
            $safeName = [IO.Path]::GetFileName($name)
            if ([string]::IsNullOrWhiteSpace($safeName)) {
                throw 'VMQA_BAD_SHOT_NAME'
            }
            if ([string]::IsNullOrWhiteSpace($expectedSurfaceClass) -or
                $expectedSurfaceClass.Length -gt 511) {
                throw 'VMQA_BAD_EXPECTED_SURFACE_CLASS'
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
            try {
                $shotSurface = Get-VmqaVisibleSurface -Subject $shotSubject['subject'] -RunNonce $RunNonce
            } catch {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail ([string]$_.Exception.Message)
            }
            if ($shotSurface.ClassName -cne $expectedSurfaceClass) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' `
                    -Detail ("VMQA_VISIBLE_SURFACE_CLASS_MISMATCH: expected='{0}' actual='{1}' hwnd={2}" -f `
                        $expectedSurfaceClass, $shotSurface.ClassName, $shotSurface.Hwnd)
            }
            # Foreground transfer may require the VM-only thread-input handoff in vmqa-win32.
            if (-not (Set-VmqaSurfaceForeground -Surface $shotSurface)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail "VMQA_VISIBLE_SURFACE_NOT_FOREGROUND: hwnd=$($shotSurface.Hwnd)"
            }
            # Foreground restoration can legitimately move the outer invisible
            # resize border. The evidence must describe the rectangle actually
            # captured, so discard the pre-foreground geometry and establish a
            # fresh trusted snapshot before any capture precondition is graded.
            try {
                $foregroundSurface = Get-VmqaTrustedSurfaceSnapshot -Hwnd $shotSurface.Hwnd
                if ($foregroundSurface.Pid -ne $shotSubject['subject'].Pid -or
                    $foregroundSurface.ClassName -cne $shotSurface.ClassName -or
                    $foregroundSurface.ClassName -cne $expectedSurfaceClass) {
                    throw 'VMQA_VISIBLE_SURFACE_IDENTITY_CHANGED_AFTER_FOREGROUND'
                }
                $foregroundSurface.NormalizedForCapture = $shotSurface.NormalizedForCapture
                $shotSurface = $foregroundSurface
            } catch {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' `
                    -Detail ([string]$_.Exception.Message)
            }
            if (-not (Test-VmqaSurfaceStillBound -Subject $shotSubject['subject'] -Surface $shotSurface -RunNonce $RunNonce)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail "VMQA_VISIBLE_SURFACE_CHANGED_BEFORE_CAPTURE: hwnd=$($shotSurface.Hwnd)"
            }
            if (-not (Test-VmqaSurfaceOwnsSampleGrid -Surface $shotSurface)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail "VMQA_VISIBLE_SURFACE_OCCLUDED_BEFORE_CAPTURE: hwnd=$($shotSurface.Hwnd)"
            }
            if (-not (Test-VmqaSurfaceUnoccluded -Surface $shotSurface)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail "VMQA_VISIBLE_SURFACE_ZORDER_OCCLUDED_BEFORE_CAPTURE: hwnd=$($shotSurface.Hwnd)"
            }
            $artifactDir = Join-Path (Join-Path $AgentRoot 'artifacts') $RunId
            $artifactPath = Join-Path $artifactDir ($safeName + '.png')
            $shot = Invoke-SurfaceShot -OutPath $artifactPath -Surface $shotSurface
            $surfaceStillForeground = [VmqaNative]::GetForegroundWindow() -eq $shotSurface.Hwnd
            $postShotSurface = $null
            try {
                $postShotSurface = Get-VmqaTrustedSurfaceSnapshot -Hwnd $shotSurface.Hwnd
                $surfaceStillBound =
                    (Test-VmqaSurfaceStillBound -Subject $shotSubject['subject'] `
                        -Surface $shotSurface -RunNonce $RunNonce) -and
                    (Test-VmqaSurfaceSnapshotMatches -Expected $shotSurface -Actual $postShotSurface)
            } catch {
                $surfaceStillBound = $false
            }
            $surfaceStillOwnsGrid = Test-VmqaSurfaceOwnsSampleGrid -Surface $shotSurface
            $surfaceStillUnoccluded = Test-VmqaSurfaceUnoccluded -Surface $shotSurface
            if (-not $surfaceStillForeground -or -not $surfaceStillBound -or
                -not $surfaceStillOwnsGrid -or -not $surfaceStillUnoccluded) {
                Remove-Item -LiteralPath $artifactPath -Force -ErrorAction SilentlyContinue
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail `
                    "VMQA_VISIBLE_SURFACE_CHANGED_DURING_CAPTURE: foreground=$surfaceStillForeground bound=$surfaceStillBound ownsGrid=$surfaceStillOwnsGrid unoccluded=$surfaceStillUnoccluded"
            }
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
            $detail = 'path={0}; width={1}; height={2}; distinctColors={3}; pngSha256={4}; subjectPid={5}; surfaceHwnd={6}; subjectRect={7},{8} {9}x{10}; boundsSource={11}; rawRect={12},{13} {14}x{15}; dpi={16}; normalizedForCapture={17}; foreground=true; sampleGridOwned=true; postCaptureBound=true' -f `
                $artifactRel, $shot['width'], $shot['height'], $shot['distinctColors'], $pngSha256, `
                $shotSubject['subject'].Pid, $shotSurface.Hwnd, $shotSurface.Rect.Left, $shotSurface.Rect.Top, `
                ($shotSurface.Rect.Right - $shotSurface.Rect.Left), ($shotSurface.Rect.Bottom - $shotSurface.Rect.Top), `
                $shotSurface.BoundsSource, $shotSurface.RawRect.Left, $shotSurface.RawRect.Top, `
                ($shotSurface.RawRect.Right - $shotSurface.RawRect.Left), `
                ($shotSurface.RawRect.Bottom - $shotSurface.RawRect.Top), $shotSurface.Dpi, `
                $shotSurface.NormalizedForCapture
            if ([int]$shot['distinctColors'] -lt 16) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'unmeasurable' -Detail $detail -Artifacts @($artifactRel)
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' -Detail $detail -Artifacts @($artifactRel) `
                -Facts ([ordered]@{
                    surfacePid = $shotSubject['subject'].Pid
                    surfaceHwnd = $shotSurface.Hwnd.ToInt64()
                    surfaceClass = $shotSurface.ClassName
                    postSurfaceClass = $postShotSurface.ClassName
                    rawRect = ConvertTo-RectFact -Rect $shotSurface.RawRect
                    dwmRect = ConvertTo-RectFact -Rect $shotSurface.Rect
                    postRawRect = ConvertTo-RectFact -Rect $postShotSurface.RawRect
                    postDwmRect = ConvertTo-RectFact -Rect $postShotSurface.Rect
                    surfaceWidth = $shot['width']
                    surfaceHeight = $shot['height']
                    captureDistinctColors = $shot['distinctColors']
                    rawSurfaceWidth = $shotSurface.RawRect.Right - $shotSurface.RawRect.Left
                    rawSurfaceHeight = $shotSurface.RawRect.Bottom - $shotSurface.RawRect.Top
                    boundsSource = $shotSurface.BoundsSource
                    boundsWithinVirtualDesktop = $true
                    coversVirtualDesktop = $false
                    surfaceDpi = $shotSurface.Dpi
                    normalizedForCapture = $shotSurface.NormalizedForCapture
                    artifactPath = $artifactRel
                    pngSha256 = $pngSha256
                    foregroundPre = $true
                    foregroundPost = $surfaceStillForeground
                    sampleGridPre = $true
                    sampleGridPost = $surfaceStillOwnsGrid
                    unoccludedPre = $true
                    unoccludedPost = $surfaceStillUnoccluded
                    rectStable = $surfaceStillBound
                })
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
            if ($script:VmqaStartedPid -le 0 -or
                [string]::IsNullOrWhiteSpace($script:VmqaStartedExePath) -or
                [string]::IsNullOrWhiteSpace($script:VmqaStartedExeSha256)) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'blocked' `
                    -Detail 'VMQA_NO_STARTED_PROCESS: kill requires Start-Process provenance from this run'
            }
            $startedPid = [int]$script:VmqaStartedPid
            $startedExePath = $script:VmqaStartedExePath
            $startedExeSha = $script:VmqaStartedExeSha256
            $live = Get-Process -Id $startedPid -ErrorAction SilentlyContinue
            $outcome = 'already-exited'
            if ($null -ne $live) {
                $livePath = $null
                try { $livePath = $live.Path } catch {
                    return New-StepResult -Id $stepId -Verb $verb -Status 'blocked' `
                        -Detail "VMQA_CLEANUP_PROCESS_PATH_UNAVAILABLE: pid=$startedPid"
                }
                if (-not [string]::Equals(
                        $livePath, $startedExePath, [StringComparison]::OrdinalIgnoreCase)) {
                    return New-StepResult -Id $stepId -Verb $verb -Status 'blocked' `
                        -Detail "VMQA_CLEANUP_PID_RECYCLED: pid=$startedPid expected='$startedExePath' actual='$livePath'"
                }
                # Cleanup is authorised by the exact PID/path returned by this run's own
                # Start-Process, not by the deliberately wrong negative identifier. This permits
                # removing a failed-negative process without ever treating it as a trusted subject.
                Stop-Process -Id $startedPid -Force -ErrorAction Stop
                Wait-Process -Id $startedPid -Timeout 10 -ErrorAction SilentlyContinue
                $outcome = 'stopped'
            }
            [void](Wait-VmqaProcessIdAbsent -ProcessId $startedPid)
            $cleanupFacts = Get-CleanupFacts -ProcessId $startedPid `
                -CanonicalExePath $startedExePath -ExeSha256 $startedExeSha -Outcome $outcome
            if (-not $cleanupFacts['cleanupPidAbsent'] -or
                -not $cleanupFacts['cleanupExecutableAbsent'] -or
                [int]$cleanupFacts['cleanupMatchingExeCount'] -ne 0) {
                return New-StepResult -Id $stepId -Verb $verb -Status 'fail' `
                    -Detail ("VMQA_CLEANUP_NOT_PROVEN: pid={0} pidAbsent={1} exeAbsent={2} matchingExeCount={3}" -f `
                        $startedPid, $cleanupFacts['cleanupPidAbsent'], `
                        $cleanupFacts['cleanupExecutableAbsent'], `
                        $cleanupFacts['cleanupMatchingExeCount']) `
                    -Facts $cleanupFacts
            }
            return New-StepResult -Id $stepId -Verb $verb -Status 'pass' `
                -Detail "cleanup proven pid=$startedPid outcome=$outcome exactExecutableAbsent=true" `
                -Facts $cleanupFacts
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
        [string]$RequestExeSha256 = '0000000000000000000000000000000000000000000000000000000000000000',
        [string]$BuildIdentitySha256 = '0000000000000000000000000000000000000000000000000000000000000000',
        [Parameter(Mandatory)][string]$RunId,
        [Parameter(Mandatory)][string]$VmName,
        [Parameter(Mandatory)][string]$Diagnosis,
        [string]$RunStartUtc = ''
    )
    return [ordered]@{
        schemaVersion = 2
        runId = $RunId
        vmName = $VmName
        agentSha = Get-AgentSha
        win32Sha = Get-Win32Sha
        runStartUtc = $RunStartUtc
        agentStartedUtc = $AgentStartedUtc.ToString('o')
        finishedUtc = [datetime]::UtcNow.ToString('o')
        overall = 'blocked'
        requestSha256 = $RequestSha256
        requestExeSha256 = $RequestExeSha256
        buildIdentitySha256 = $BuildIdentitySha256
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

    $requestExeSha256 = '0000000000000000000000000000000000000000000000000000000000000000'
    $buildIdentitySha256 = '0000000000000000000000000000000000000000000000000000000000000000'
    try {
        $request = $requestText | ConvertFrom-Json
        # Preserve valid identity scalars even when a different request field makes the envelope
        # invalid (for example schemaVersion 999 or an unknown step arg). The blocked producer must
        # still say which executable/build the rejected request claimed, without coercing objects
        # or accepting malformed digests.
        $requestExeCandidate = Get-PropertyValue -Object $request -Name 'exeSha256'
        if ($requestExeCandidate -is [string] -and
            $requestExeCandidate -cmatch '^[0-9a-f]{64}$') {
            $requestExeSha256 = $requestExeCandidate
        }
        $buildIdentityCandidate = Get-PropertyValue -Object $request -Name 'buildIdentitySha256'
        if ($buildIdentityCandidate -is [string] -and
            $buildIdentityCandidate -cmatch '^[0-9a-f]{64}$') {
            $buildIdentitySha256 = $buildIdentityCandidate
        }
        Assert-ExactJsonProperties -Object $request `
            -Names @('schemaVersion','runId','identifier','runStartUtc','exeSha256',`
                'buildIdentitySha256','buildIdentity','steps') -Context 'request'
        $schemaValue = Get-PropertyValue -Object $request -Name 'schemaVersion' -Required
        Assert-JsonInteger -Value $schemaValue -Context 'request.schemaVersion'
        $schemaVersion = [int]$schemaValue
        if ($schemaVersion -ne 2) {
            throw "VMQA_UNSUPPORTED_REQUEST_SCHEMA: $schemaVersion"
        }
        $runIdValue = Get-PropertyValue -Object $request -Name 'runId' -Required
        Assert-JsonString -Value $runIdValue -Context 'request.runId'
        $runId = [string]$runIdValue
        $runStartValue = Get-PropertyValue -Object $request -Name 'runStartUtc' -Required
        Assert-JsonString -Value $runStartValue -Context 'request.runStartUtc'
        $runStartRaw = [string]$runStartValue
        $runStartUtc = ([datetime]$runStartRaw).ToUniversalTime()
        $identifierValue = Get-PropertyValue -Object $request -Name 'identifier' -Required
        Assert-JsonString -Value $identifierValue -Context 'request.identifier'
        $requestExeValue = Get-PropertyValue -Object $request -Name 'exeSha256' -Required
        Assert-JsonString -Value $requestExeValue -Context 'request.exeSha256'
        $requestExeSha256 = ([string]$requestExeValue).ToLowerInvariant()
        if ($requestExeSha256 -notmatch '^[0-9a-f]{64}$') {
            throw 'VMQA_BAD_REQUEST_EXE_SHA256'
        }
        $buildIdentityShaValue = Get-PropertyValue -Object $request -Name 'buildIdentitySha256' -Required
        Assert-JsonString -Value $buildIdentityShaValue -Context 'request.buildIdentitySha256'
        $buildIdentitySha256 = ([string]$buildIdentityShaValue).ToLowerInvariant()
        if ($buildIdentitySha256 -notmatch '^[0-9a-f]{64}$') {
            throw 'VMQA_BAD_BUILD_IDENTITY_SHA256'
        }
        $buildIdentity = Get-PropertyValue -Object $request -Name 'buildIdentity' -Required
        Assert-StrictBuildIdentity -Identity $buildIdentity -ExpectedExeSha256 $requestExeSha256
        $steps = @(Get-PropertyValue -Object $request -Name 'steps' -Required)
        Assert-StrictRequestSteps -Steps $steps
    } catch {
        $blocked = New-BlockedVerdict -RunId $runIdFromPrefix -VmName $VmName `
            -Diagnosis ('invalid-request: ' + $_.Exception.Message) -RequestSha256 $actualSha `
            -RequestExeSha256 $requestExeSha256 -BuildIdentitySha256 $buildIdentitySha256
        Write-Verdict -RunBlobPrefix $RunBlobPrefix -Verdict $blocked
        return $true
    }

    # CLAIM THE RUN BEFORE EXECUTING ANY STEP.
    #
    # A run is skipped once a verdict exists. But if the agent dies mid-run - crash, restart,
    # reboot - no verdict was ever written, so the run is re-picked and EVERY step executes a
    # second time. Steps are not all idempotent (launch starts a process, and any future verb with
    # a real side effect would be worse), and the re-run can still finish green, so the replay is
    # invisible in the verdict.
    #
    # An interrupted run is therefore refused rather than replayed. "Some steps may already have
    # run and I cannot tell which" is not a measurement, and quietly redoing them to reach a clean
    # result would be manufacturing the evidence rather than collecting it.
    $startedBlob = "$RunBlobPrefix/started.json"
    $priorClaim = Get-BlobText -Name $startedBlob
    if ($null -ne $priorClaim) {
        Write-Log "interrupted run $RunBlobPrefix already claimed; refusing to re-execute its steps"
        $blocked = New-BlockedVerdict -RunId $runIdFromPrefix -VmName $VmName -RequestSha256 $actualSha `
            -RequestExeSha256 $requestExeSha256 -BuildIdentitySha256 $buildIdentitySha256 `
            -RunStartUtc ($runStartUtc.ToString('o')) `
            -Diagnosis 'interrupted-run: this run was already started by an agent that did not finish, so an unknown prefix of its steps has already executed; re-running them could double a side effect and still grade green'
        Write-Verdict -RunBlobPrefix $RunBlobPrefix -Verdict $blocked
        return $true
    }
    $claim = [ordered]@{
        schemaVersion = 2
        runId = $runIdFromPrefix
        vmName = $VmName
        agentSha = Get-AgentSha
        win32Sha = Get-Win32Sha
        claimedUtc = [datetime]::UtcNow.ToString('o')
        agentPid = [Diagnostics.Process]::GetCurrentProcess().Id
    }
    Put-BlobText -Name $startedBlob -Text (($claim | ConvertTo-Json -Depth 6) + [Environment]::NewLine)

    # Clear the pin per run. A pid carried over from a previous run would block this one for a
    # reason that has nothing to do with it.
    $script:VmqaPinnedPid = 0
    $script:VmqaStartedPid = 0
    $script:VmqaStartedExePath = ''
    $script:VmqaStartedExeSha256 = ''
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
        schemaVersion = 2
        runId = $runId
        # Bind the verdict to the exact request bytes that produced it. A runId alone does not do
        # this: reusing a runId (which `run --run-id` permits) would let a verdict left by an
        # earlier attempt be graded as this run's result. The digest is already computed to validate
        # the .ready sentinel, so binding it costs nothing and closes the reuse case completely.
        requestSha256 = $actualSha
        requestExeSha256 = $requestExeSha256
        buildIdentitySha256 = $buildIdentitySha256
        vmName = $VmName
        agentSha = Get-AgentSha
        win32Sha = Get-Win32Sha
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
