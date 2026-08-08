#!/usr/bin/env bash
# Bring one OSL-VMQA Windows client to the task-4950 known state.
set -euo pipefail

RG="OSL-VMQA"
LOCATION="spaincentral"
USER_NAME="osladmin"
SESSION_ID="1"

usage() {
  echo "usage: $0 <osl-client-1|osl-client-2|osl-client-4>" >&2
}

if [ "$#" -ne 1 ]; then
  usage
  exit 64
fi

VM="$1"
case "$VM" in
  osl-client-1|osl-client-2|osl-client-4) ;;
  *)
    echo "VM-4950-UNKNOWN-MACHINE"
    exit 64
    ;;
esac

command -v az >/dev/null 2>&1 || {
  echo "az CLI is required" >&2
  exit 69
}
command -v jq >/dev/null 2>&1 || {
  echo "jq is required" >&2
  exit 69
}

CHANGES=()
mark_changed() {
  CHANGES+=("$1")
}

power_state() {
  az vm get-instance-view \
    --resource-group "$RG" \
    --name "$VM" \
    --query "instanceView.statuses[?starts_with(code, 'PowerState/')].displayStatus | [0]" \
    --output tsv 2>/dev/null
}

agent_ready() {
  az vm get-instance-view \
    --resource-group "$RG" \
    --name "$VM" \
    --query "instanceView.vmAgent.statuses[0].displayStatus" \
    --output tsv 2>/dev/null
}

wait_for_guest() {
  local start elapsed state ready
  start="$(date +%s)"
  while :; do
    state="$(power_state || true)"
    ready="$(agent_ready || true)"
    if [ "$state" = "VM running" ] && [ "$ready" = "Ready" ]; then
      return 0
    fi
    elapsed=$(( $(date +%s) - start ))
    if [ "$elapsed" -gt 900 ]; then
      echo "VM-4950-TIMEOUT waiting for $VM guest agent; state=$state agent=$ready" >&2
      return 4
    fi
    sleep 5
  done
}

run_ps() {
  local ps_file="$1"
  local attempt elapsed output rc start stdout stderr
  start="$(date +%s)"
  attempt=0
  while :; do
    attempt=$((attempt + 1))
    set +e
    output="$(az vm run-command invoke \
      --resource-group "$RG" \
      --name "$VM" \
      --command-id RunPowerShellScript \
      --scripts @"$ps_file" \
      --output json 2>&1)"
    rc=$?
    set -e
    if [ "$rc" -eq 0 ]; then
      stdout="$(jq -r '[.value[]? | select(.code == "ComponentStatus/StdOut/succeeded") | .message] | join("\n")' <<<"$output")"
      stderr="$(jq -r '[.value[]? | select(.code == "ComponentStatus/StdErr/succeeded") | .message] | join("\n")' <<<"$output")"
      if [ -n "$stderr" ]; then
        printf '%s\n' "$stderr" >&2
        return 1
      fi
      printf '%s\n' "$stdout"
      return 0
    fi
    if ! grep -q "Run command extension execution is in progress" <<<"$output"; then
      printf '%s\n' "$output" >&2
      return "$rc"
    fi
    elapsed=$(( $(date +%s) - start ))
    if [ "$elapsed" -gt 600 ]; then
      printf '%s\n' "$output" >&2
      echo "VM-4950-TIMEOUT waiting for prior run-command on $VM" >&2
      return "$rc"
    fi
    sleep 10
  done
}

extract_remote_changes() {
  local line clean
  while IFS= read -r line; do
    clean="${line%$'\r'}"
    case "$clean" in
      "VM4950-REMOTE CHANGED "*)
        mark_changed "${clean#VM4950-REMOTE CHANGED }"
        ;;
      "VM4950-REMOTE RESTART")
        REMOTE_RESTART=1
        ;;
    esac
  done
}

ensure_public_ip_static() {
  local vm_location nic_id nic_name ip_config pip_id pip_name allocation before_ip

  vm_location="$(az vm show --resource-group "$RG" --name "$VM" --query location --output tsv)"
  if [ "$vm_location" != "$LOCATION" ]; then
    echo "VM-4950-WRONG-LOCATION $VM $vm_location" >&2
    exit 65
  fi

  nic_id="$(az vm show \
    --resource-group "$RG" \
    --name "$VM" \
    --query "networkProfile.networkInterfaces[?primary == \`true\`].id | [0]" \
    --output tsv)"
  if [ -z "$nic_id" ]; then
    nic_id="$(az vm show \
      --resource-group "$RG" \
      --name "$VM" \
      --query "networkProfile.networkInterfaces[0].id" \
      --output tsv)"
  fi
  if [ -z "$nic_id" ]; then
    echo "VM-4950-NO-NIC $VM" >&2
    exit 65
  fi
  nic_name="${nic_id##*/}"

  ip_config="$(az network nic show \
    --ids "$nic_id" \
    --query "ipConfigurations[?primary == \`true\`].name | [0]" \
    --output tsv)"
  if [ -z "$ip_config" ]; then
    ip_config="$(az network nic show --ids "$nic_id" --query "ipConfigurations[0].name" --output tsv)"
  fi
  if [ -z "$ip_config" ]; then
    echo "VM-4950-NO-IPCONFIG $VM" >&2
    exit 65
  fi

  pip_id="$(az network nic show \
    --ids "$nic_id" \
    --query "ipConfigurations[?name == '$ip_config'].publicIPAddress.id | [0]" \
    --output tsv)"
  if [ -z "$pip_id" ]; then
    pip_name="${VM}PublicIP"
    if ! az network public-ip show --resource-group "$RG" --name "$pip_name" --output none 2>/dev/null; then
      az network public-ip create \
        --resource-group "$RG" \
        --location "$LOCATION" \
        --name "$pip_name" \
        --sku Standard \
        --allocation-method Static \
        --output none
      mark_changed "public-ip-created $pip_name"
    fi
    az network nic ip-config update \
      --resource-group "$RG" \
      --nic-name "$nic_name" \
      --name "$ip_config" \
      --public-ip-address "$pip_name" \
      --output none
    mark_changed "public-ip-attached $pip_name"
    pip_id="$(az network public-ip show --resource-group "$RG" --name "$pip_name" --query id --output tsv)"
  fi

  allocation="$(az network public-ip show --ids "$pip_id" --query publicIPAllocationMethod --output tsv)"
  if [ "$allocation" != "Static" ]; then
    before_ip="$(az network public-ip show --ids "$pip_id" --query ipAddress --output tsv)"
    az network public-ip update --ids "$pip_id" --allocation-method Static --output none
    mark_changed "public-ip-allocation ${allocation:-None}->Static ${before_ip:-unassigned}"
  fi
}

ensure_windows_console_login() {
  local ensure_ps verify_ps message
  ensure_ps="$(mktemp)"
  verify_ps="$(mktemp)"
  trap 'rm -f -- "$ensure_ps" "$verify_ps"' RETURN

  cat >"$ensure_ps" <<'POWERSHELL'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$UserName = 'osladmin'
$SessionId = 1
$WinlogonPath = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon'
$PolicyPath = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System'
$Changed = [System.Collections.Generic.List[string]]::new()

function Add-Change([string]$Name) {
  if (-not $Changed.Contains($Name)) {
    [void]$Changed.Add($Name)
  }
}

function New-Password {
  $bytes = [byte[]]::new(24)
  [Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
  $token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', 'A').Replace('/', 'b')
  return "Osl4950!$token"
}

function Get-RegistryString([string]$Path, [string]$Name) {
  try {
    $item = Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop
    return [string]$item.$Name
  } catch {
    return $null
  }
}

function Set-RegistryStringIfNeeded([string]$Path, [string]$Name, [string]$Value, [string]$ChangeName) {
  $current = Get-RegistryString $Path $Name
  if ($current -cne $Value) {
    if (-not (Test-Path -LiteralPath $Path)) {
      New-Item -Path $Path -Force | Out-Null
    }
    New-ItemProperty -LiteralPath $Path -Name $Name -Value $Value -PropertyType String -Force | Out-Null
    Add-Change $ChangeName
  }
}

function Set-RegistryDwordIfNeeded([string]$Path, [string]$Name, [int]$Value, [string]$ChangeName) {
  try {
    $current = [int](Get-ItemProperty -LiteralPath $Path -Name $Name -ErrorAction Stop).$Name
  } catch {
    $current = $null
  }
  if ($current -ne $Value) {
    if (-not (Test-Path -LiteralPath $Path)) {
      New-Item -Path $Path -Force | Out-Null
    }
    New-ItemProperty -LiteralPath $Path -Name $Name -Value $Value -PropertyType DWord -Force | Out-Null
    Add-Change $ChangeName
  }
}

function Test-ConsoleSessionOne {
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
    [int]$_.SessionId -eq $SessionId
  })
  if ($explorers.Count -ne 1) { return $false }
  $owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
  return ($owner.ReturnValue -eq 0 -and $owner.User -ceq $UserName)
}

$localUser = Get-LocalUser -Name $UserName -ErrorAction SilentlyContinue
$mustSetPassword = $false
if ($null -eq $localUser) {
  $password = New-Password
  New-LocalUser -Name $UserName -Password (ConvertTo-SecureString $password -AsPlainText -Force) -PasswordNeverExpires | Out-Null
  Add-LocalGroupMember -Group 'Administrators' -Member $UserName
  Add-Change 'windows-account-created'
  $mustSetPassword = $false
} else {
  if (-not $localUser.Enabled) {
    Enable-LocalUser -Name $UserName
    Add-Change 'windows-account-activated'
  }
  $password = Get-RegistryString $WinlogonPath 'DefaultPassword'
  if ([string]::IsNullOrWhiteSpace($password)) {
    $mustSetPassword = $true
  }
}

$sessionOk = Test-ConsoleSessionOne
if (-not $sessionOk) {
  $mustSetPassword = $true
}

if ($mustSetPassword) {
  $password = New-Password
  & net.exe user $UserName $password /active:yes | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "net user failed with exit code $LASTEXITCODE"
  }
  Add-Change 'windows-account-password-set'
}

Set-RegistryStringIfNeeded $WinlogonPath 'AutoAdminLogon' '1' 'windows-autologon-enabled'
Set-RegistryStringIfNeeded $WinlogonPath 'ForceAutoLogon' '1' 'windows-force-autologon-enabled'
Set-RegistryStringIfNeeded $WinlogonPath 'DefaultUserName' $UserName 'windows-autologon-user-set'
Set-RegistryStringIfNeeded $WinlogonPath 'DefaultDomainName' $env:COMPUTERNAME 'windows-autologon-domain-set'
Set-RegistryStringIfNeeded $WinlogonPath 'DefaultPassword' $password 'windows-autologon-password-set'
Set-RegistryDwordIfNeeded $PolicyPath 'DisableCAD' 1 'windows-secure-attention-disabled'

try {
  $autoLogonCount = Get-ItemProperty -LiteralPath $WinlogonPath -Name 'AutoLogonCount' -ErrorAction Stop
  if ($null -ne $autoLogonCount) {
    Remove-ItemProperty -LiteralPath $WinlogonPath -Name 'AutoLogonCount' -Force
    Add-Change 'windows-autologon-count-removed'
  }
} catch {
}

$needsRestart = (-not $sessionOk) -or ($Changed.Count -gt 0)
if ($needsRestart) {
  Add-Change 'windows-console-session-restart'
}

foreach ($change in $Changed) {
  Write-Output "VM4950-REMOTE CHANGED $change"
}

if ($needsRestart) {
  Write-Output 'VM4950-REMOTE RESTART'
}
POWERSHELL

  cat >"$verify_ps" <<'POWERSHELL'
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$deadline = [DateTime]::UtcNow.AddMinutes(4)
do {
  $explorers = @(Get-CimInstance Win32_Process -Filter "Name = 'explorer.exe'" | Where-Object {
    [int]$_.SessionId -eq 1
  })
  if ($explorers.Count -eq 1) {
    $owner = Invoke-CimMethod -InputObject $explorers[0] -MethodName GetOwner
    if ($owner.ReturnValue -eq 0 -and $owner.User -ceq 'osladmin') {
      Write-Output 'VM4950-REMOTE VERIFIED'
      exit 0
    }
  }
  Start-Sleep -Seconds 5
} while ([DateTime]::UtcNow -lt $deadline)
throw 'VM-4950 console session 1 is not owned by osladmin'
POWERSHELL

  REMOTE_RESTART=0
  message="$(run_ps "$ensure_ps")"
  extract_remote_changes <<<"$message"
  if [ "$REMOTE_RESTART" -eq 1 ]; then
    az vm restart --resource-group "$RG" --name "$VM" --output none
    wait_for_guest
  fi
  run_ps "$verify_ps" >/dev/null
}

state="$(power_state || true)"
if [ "$state" != "VM running" ]; then
  az vm start --resource-group "$RG" --name "$VM" --output none
  mark_changed "vm-started"
fi
wait_for_guest

ensure_public_ip_static
ensure_windows_console_login

if [ "${#CHANGES[@]}" -eq 0 ]; then
  echo "VM-4950-CLEAN"
else
  for change in "${CHANGES[@]}"; do
    echo "CHANGED $change"
  done
fi
