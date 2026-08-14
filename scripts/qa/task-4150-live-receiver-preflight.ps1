param(
  [Parameter(Mandatory = $true)]
  [string]$OutputPath
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Electron gives its renderer children the ordinary profile-directory argument.
# The task's prohibition concerns a caller launching either official release with
# a supplied --user-data-dir, so inspect only browser processes (no --type=).
function Get-Endpoint([string]$Release, [string]$ProcessName, [string]$DataDirectory) {
  $processes = @(Get-CimInstance Win32_Process -Filter "Name='$ProcessName.exe'" |
    Where-Object { $_.SessionId -eq 1 })
  $topLevel = @($processes | Where-Object { $_.CommandLine -notmatch '(?i)(?:^|\s)--type=' })
  $windows = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
    Where-Object { $_.SessionId -eq 1 -and -not [string]::IsNullOrWhiteSpace($_.MainWindowTitle) })
  $exe = @($topLevel | Select-Object -First 1).ExecutablePath
  $scope = Join-Path $env:APPDATA "$DataDirectory\sentry\scope_v3.json"
  $accountId = $null
  if (Test-Path -LiteralPath $scope -PathType Leaf) {
    $match = [regex]::Match((Get-Content -LiteralPath $scope -Raw), '\b[0-9]{17,20}\b')
    if ($match.Success) { $accountId = $match.Value }
  }
  [pscustomobject]@{
    release_channel = $Release
    account_id = $accountId
    store = if ($exe) { Split-Path (Split-Path $exe -Parent) -Parent } else { $null }
    version = if ($exe) { [Diagnostics.FileVersionInfo]::GetVersionInfo($exe).ProductVersion } else { $null }
    top_level_processes = $topLevel.Count
    interactive_windows = $windows.Count
    window_titles = @($windows | ForEach-Object { $_.MainWindowTitle })
    top_level_user_data_dir = @($topLevel | Where-Object { $_.CommandLine -match '(?i)(?:^|\s)--user-data-dir(?:\s|=)' }).Count
    ordinary_electron_child_user_data_dir = @($processes | Where-Object {
      $_.CommandLine -match '(?i)(?:^|\s)--type=' -and $_.CommandLine -match '(?i)(?:^|\s)--user-data-dir(?:\s|=)'
    }).Count
  }
}

$endpoints = @(
  Get-Endpoint 'Canary' 'DiscordCanary' 'discordcanary'
  Get-Endpoint 'PTB' 'DiscordPTB' 'discordptb'
)
$oslPaths = @(Get-ChildItem -LiteralPath $env:APPDATA -Directory -Force -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -match '(?i)^osl' } | ForEach-Object { $_.FullName })
$oslProcesses = @(Get-Process -Name 'osl','osl-hub' -ErrorAction SilentlyContinue |
  Where-Object { $_.SessionId -eq 1 } | ForEach-Object { $_.Path })

$distinctAccounts = @($endpoints.account_id | Where-Object { $_ } | Select-Object -Unique).Count -eq 2
$distinctStores = @($endpoints.store | Where-Object { $_ } | Select-Object -Unique).Count -eq 2
$noCallerProfileOverride = @($endpoints | Where-Object { $_.top_level_user_data_dir -ne 0 }).Count -eq 0
$receiverReady = @($endpoints | Where-Object { $_.release_channel -eq 'PTB' }).interactive_windows -eq 1
$ready = $distinctAccounts -and $distinctStores -and $noCallerProfileOverride -and $receiverReady

[pscustomobject]@{
  task = 4150
  captured_at_utc = [DateTime]::UtcNow.ToString('o')
  sender_channel = 'Canary'
  receiver_channel = 'PTB'
  endpoints = $endpoints
  osl_data_paths = $oslPaths
  osl_processes = $oslProcesses
  gates = [pscustomobject]@{
    distinct_accounts = $distinctAccounts
    distinct_stores = $distinctStores
    no_caller_user_data_dir = $noCallerProfileOverride
    receiver_window_ready = $receiverReady
  }
  ready_for_unattended_send = $ready
} | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath $OutputPath -Encoding utf8

if (-not $ready) { exit 1 }
