param(
    [Parameter(Mandatory)][string]$InvocationId,
    [Parameter(Mandatory)][string]$PayloadPath,
    [Parameter(Mandatory)][string]$PayloadSha256,
    [string]$PayloadArguments = '',
    [int]$SessionId = 1,
    [string]$InteractiveUser = 'osladmin',
    [int]$WaitSeconds = 300
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

if ($InvocationId -notmatch '^[A-Za-z0-9_.-]{1,80}$') {
    throw 'invalid invocation id'
}
if (-not (Test-Path -LiteralPath $PayloadPath -PathType Leaf)) {
    throw "payload missing: $PayloadPath"
}
$actualSha = Get-FileSha256 $PayloadPath
if ($actualSha -cne $PayloadSha256.ToLowerInvariant()) {
    throw "payload hash mismatch: expected $PayloadSha256 actual $actualSha"
}

$queryUser = (& query user) -join "`n"
if ($queryUser -notmatch [regex]::Escape($InteractiveUser)) {
    throw "interactive user $InteractiveUser is not logged on"
}
if ($queryUser -notmatch "\s$SessionId\s+Active\s") {
    throw "session $SessionId is not active"
}

$root = Join-Path $env:SystemDrive "OSL\desktop-runner\$InvocationId"
$out = Join-Path $env:SystemDrive 'OSL\out'
New-Item -ItemType Directory -Force -Path $root, $out | Out-Null
$wrapper = Join-Path $root 'wrapper.ps1'
$result = Join-Path $root 'result.json'
$log = Join-Path $out "$InvocationId.log"
$taskName = "OSLQA_4956_$InvocationId"

$wrapperSource = @"
`$ErrorActionPreference='Stop'
try {
  & '$PayloadPath' $PayloadArguments
  `$exitCode = `$LASTEXITCODE
  if (`$null -eq `$exitCode) { `$exitCode = 0 }
  [pscustomobject]@{Status='completed';ExitCode=[int]`$exitCode} | ConvertTo-Json -Compress | Set-Content -LiteralPath '$result' -Encoding UTF8
  exit `$exitCode
} catch {
  [pscustomobject]@{Status='failed';ExitCode=1;Error=`$_.Exception.Message} | ConvertTo-Json -Compress | Set-Content -LiteralPath '$result' -Encoding UTF8
  throw
}
"@
[IO.File]::WriteAllText($wrapper, $wrapperSource, [Text.UTF8Encoding]::new($false))

$action = New-ScheduledTaskAction -Execute 'cmd.exe' -Argument ('/c powershell.exe -NoProfile -ExecutionPolicy Bypass -File "{0}" > "{1}" 2>&1' -f $wrapper, $log)
$principal = New-ScheduledTaskPrincipal -UserId $InteractiveUser -LogonType Interactive -RunLevel Highest
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DisallowStartIfOnBatteries:$false -ExecutionTimeLimit (New-TimeSpan -Seconds ($WaitSeconds + 60))
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null

try {
    Start-ScheduledTask -TaskName $taskName
    $deadline = [DateTime]::UtcNow.AddSeconds($WaitSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $result -PathType Leaf) { break }
        Start-Sleep -Milliseconds 500
    }
    if (-not (Test-Path -LiteralPath $result -PathType Leaf)) {
        $info = Get-ScheduledTaskInfo -TaskName $taskName
        throw "desktop runner timed out with task result $($info.LastTaskResult)"
    }
    $resultText = Get-Content -LiteralPath $result -Raw
    $payloadExit = 999
    try { $payloadExit = [int](($resultText | ConvertFrom-Json).ExitCode) } catch { }
    Write-Output ("DESKTOP-RUNNER InvocationId={0} Terminal=true TaskName={1} PayloadExitCode={2}" -f $InvocationId, $taskName, $payloadExit)
    if (Test-Path -LiteralPath $log -PathType Leaf) {
        Get-Content -LiteralPath $log -Raw
    }
    exit $payloadExit
} finally {
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false -ErrorAction SilentlyContinue
}
