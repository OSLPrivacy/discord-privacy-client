param(
    [string]$EvidencePath = "",
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Continue"

function Quote-Arg([string]$Value) {
    if ($Value -match '[\s"]') {
        '"' + ($Value -replace '"', '\"') + '"'
    } else {
        $Value
    }
}

function Get-FileVersion([string]$Path) {
    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return ""
    }
    $info = (Get-Item -LiteralPath $Path).VersionInfo
    foreach ($candidate in @($info.ProductVersion, $info.FileVersion)) {
        if ($candidate) {
            return ([string]$candidate).Trim()
        }
    }
    return ""
}

function Get-UninstallEntry([string]$Pattern) {
    $roots = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*"
    )
    Get-ItemProperty $roots -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -match $Pattern } |
        Select-Object -First 1
}

function Resolve-Discord {
    $entry = Get-UninstallEntry '^Discord$'
    $install = $entry.InstallLocation
    $exe = ""
    if ($install -and (Test-Path -LiteralPath $install)) {
        $exe = Get-ChildItem -LiteralPath $install -Directory -Filter "app-*" -ErrorAction SilentlyContinue |
            Sort-Object Name -Descending |
            ForEach-Object { Join-Path $_.FullName "Discord.exe" } |
            Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
            Select-Object -First 1
    }
    [pscustomobject]@{ Name = "Discord"; Exe = $exe; Version = $entry.DisplayVersion; Source = "HKCU/HKLM uninstall registry" }
}

function Resolve-Signal {
    $entry = Get-UninstallEntry '^Signal'
    $exe = ""
    if ($entry.DisplayIcon) {
        $exe = ($entry.DisplayIcon -replace ',\d+$', '')
    }
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        $candidate = Join-Path $env:LOCALAPPDATA "Programs\signal-desktop\Signal.exe"
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            $exe = $candidate
        }
    }
    $version = $entry.DisplayVersion
    if (-not $version) {
        $version = Get-FileVersion $exe
    }
    [pscustomobject]@{ Name = "Signal"; Exe = $exe; Version = $version; Source = "HKCU/HKLM uninstall registry plus default install path" }
}

function Resolve-WhatsApp {
    $pkg = Get-AppxPackage 5319275A.WhatsAppDesktop -ErrorAction SilentlyContinue
    $exe = ""
    $version = ""
    if ($pkg) {
        $exe = Join-Path $pkg.InstallLocation "WhatsApp.Root.exe"
        $version = [string]$pkg.Version
    }
    [pscustomobject]@{ Name = "WhatsApp"; Exe = $exe; Version = $version; Source = "Get-AppxPackage 5319275A.WhatsAppDesktop" }
}

function Resolve-Slack {
    $entry = Get-UninstallEntry 'Slack'
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA "slack\slack.exe"),
        (Join-Path $env:LOCALAPPDATA "slack\app-*\slack.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\Slack\slack.exe"),
        (Join-Path $env:ProgramFiles "Slack\slack.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "Slack\slack.exe")
    )
    $exe = ""
    foreach ($candidate in $candidates) {
        $found = Resolve-Path $candidate -ErrorAction SilentlyContinue |
            Sort-Object Path -Descending |
            Select-Object -First 1
        if ($found) {
            $exe = $found.Path
            break
        }
    }
    $version = $entry.DisplayVersion
    if (-not $version) {
        $version = Get-FileVersion $exe
    }
    [pscustomobject]@{ Name = "Slack"; Exe = $exe; Version = $version; Source = "uninstall registry plus common Slack install paths" }
}

function Test-PortAnswer([int]$Port, [int]$TimeoutSeconds) {
    $url = "http://127.0.0.1:$Port/json/version"
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $lastError = ""
    while ((Get-Date) -lt $deadline) {
        try {
            $response = Invoke-WebRequest -Uri $url -UseBasicParsing -TimeoutSec 2
            return [pscustomobject]@{
                Answered = $true
                Url = $url
                StatusCode = $response.StatusCode
                Body = $response.Content
                Error = ""
            }
        } catch {
            $lastError = $_.Exception.Message
            Start-Sleep -Milliseconds 750
        }
    }
    [pscustomobject]@{
        Answered = $false
        Url = $url
        StatusCode = ""
        Body = ""
        Error = $lastError
    }
}

function Stop-LaunchedDebugProcesses([int]$Port, [string]$Profile) {
    $escapedProfile = $Profile.Replace("\", "\\")
    Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
        Where-Object {
            $_.CommandLine -and
            ($_.CommandLine -like "*--remote-debugging-port=$Port*" -or
             $_.CommandLine -like "*$escapedProfile*" -or
             $_.CommandLine -like "*$Profile*")
        } |
        ForEach-Object {
            try {
                Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop
                "stopped pid=$($_.ProcessId) name=$($_.Name)"
            } catch {
                "stop_error pid=$($_.ProcessId) error=$($_.Exception.Message)"
            }
        }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).ProviderPath
$profileRoot = [System.IO.Path]::GetFullPath((Join-Path $repoRoot ".task3402\profiles"))
New-Item -ItemType Directory -Path $profileRoot -Force | Out-Null

$apps = @(
    (Resolve-Discord | Add-Member -NotePropertyName Port -NotePropertyValue 49340 -PassThru),
    (Resolve-Signal | Add-Member -NotePropertyName Port -NotePropertyValue 49341 -PassThru),
    (Resolve-WhatsApp | Add-Member -NotePropertyName Port -NotePropertyValue 49342 -PassThru),
    (Resolve-Slack | Add-Member -NotePropertyName Port -NotePropertyValue 49343 -PassThru)
)

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("# TASK 3402 - page debug port connection audit")
$lines.Add("")
$lines.Add("Measured on $(Get-Date -Format o).")
$lines.Add("")
$lines.Add("## Saved note")
$lines.Add("")
$tick = [char]96

$results = @()
foreach ($app in $apps) {
    $profile = Join-Path $profileRoot $app.Name.ToLowerInvariant()
    New-Item -ItemType Directory -Path $profile -Force | Out-Null
    $args = @("--remote-debugging-port=$($app.Port)", "--user-data-dir=$profile")
    $launchPath = $app.Exe
    if (-not $launchPath) {
        $launchPath = "$($app.Name).exe"
    }
    $quotedArgs = @($args | ForEach-Object { Quote-Arg $_ }) -join ', '
    $command = "Start-Process -FilePath $(Quote-Arg $launchPath) -ArgumentList $quotedArgs -PassThru"
    $launch = ""
    $launchError = ""
    $probe = $null

    if (-not $app.Exe -or -not (Test-Path -LiteralPath $app.Exe -PathType Leaf)) {
        try {
            $process = Start-Process -FilePath $launchPath -ArgumentList $args -PassThru -ErrorAction Stop
            $launch = "pid=$($process.Id)"
        } catch {
            $launchError = $_.Exception.Message
        }
        $probe = Test-PortAnswer -Port $app.Port -TimeoutSeconds 3
    } elseif (-not $app.Version) {
        $launchError = "version number not found; refusing to count this app line"
        $probe = Test-PortAnswer -Port $app.Port -TimeoutSeconds 3
    } else {
        try {
            $process = Start-Process -FilePath $launchPath -ArgumentList $args -PassThru -ErrorAction Stop
            $launch = "pid=$($process.Id)"
        } catch {
            $launchError = $_.Exception.Message
        }
        $probe = Test-PortAnswer -Port $app.Port -TimeoutSeconds $TimeoutSeconds
    }

    $cleanup = Stop-LaunchedDebugProcesses -Port $app.Port -Profile $profile
    $status = if ($probe.Answered) { "answered" } else { "did not answer" }
    $version = if ($app.Version) { $app.Version } else { "NO VERSION NUMBER FOUND" }

    $result = [pscustomobject]@{
        App = $app.Name
        Status = $status
        Version = $version
        Command = $command
        Launch = $launch
        LaunchError = $launchError
        Source = $app.Source
        ProbeUrl = $probe.Url
        ProbeStatusCode = $probe.StatusCode
        ProbeError = $probe.Error
        ProbeBody = $probe.Body
        Cleanup = @($cleanup)
    }
    $results += $result
    $lines.Add("- " + $result.App + ": " + $result.Status + "; version " + $result.Version + "; exact command used: " + $tick + $result.Command + $tick)
}

$lines.Add("")
$lines.Add("## What I ran")
$lines.Add("")
$lines.Add('```powershell')
$lines.Add("powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/qa/task3402-page-debug-port-check.ps1 -EvidencePath /home/liamw/osl-plan/OSL-AUDITS/evidence/3402.md -TimeoutSeconds $TimeoutSeconds")
$lines.Add('```')
$lines.Add("")
$lines.Add("## Raw output")
$lines.Add("")
$lines.Add('```json')
$lines.Add(($results | ConvertTo-Json -Depth 8))
$lines.Add('```')
$lines.Add("")
$lines.Add("## Finish line checked off")
$lines.Add("")
$lines.Add("- four app lines were produced: count=$($results.Count)")
foreach ($result in $results) {
    $hasVersion = if ($result.Version -ne "NO VERSION NUMBER FOUND") { "yes" } else { "no" }
    $lines.Add("- " + $result.App + ": status=" + $tick + $result.Status + $tick + ", version_number_present=" + $hasVersion + ", version=" + $tick + $result.Version + $tick + ", command=" + $tick + $result.Command + $tick)
}
$missingVersions = @($results | Where-Object { $_.Version -eq "NO VERSION NUMBER FOUND" }).Count
$lines.Add("- lines with no version number: $missingVersions")
if ($missingVersions -gt 0) {
    $lines.Add("- task status: impossible as written on this machine because at least one requested app had no installed executable/version to launch")
} else {
    $lines.Add("- task status: finish line satisfied")
}

$content = $lines -join [Environment]::NewLine
if ($EvidencePath) {
    $linuxEvidencePath = $EvidencePath
    if ($EvidencePath -match '^/') {
        $EvidencePath = (& wsl.exe wslpath -w $EvidencePath).Trim()
    }
    $parent = Split-Path -Parent $EvidencePath
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    Set-Content -LiteralPath $EvidencePath -Value $content -Encoding UTF8
    Write-Output "evidence_path=$linuxEvidencePath"
}

Write-Output $content
