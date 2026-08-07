param(
    [int]$TimeoutSeconds = 30,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$classicClass = 'rctrl_renwnd32'
$newClass = 'WinUIDesktopWin32WindowClass'
$setupDialogClass = 'NUIDialog'
$splashWindowClass = 'MsoSplash'
$processNames = @('OUTLOOK', 'olk')
$titleClasses = @($classicClass, $newClass, $setupDialogClass, $splashWindowClass)
$aumid = 'shell:AppsFolder\Microsoft.OutlookForWindows_8wekyb3d8bbwe!Microsoft.OutlookforWindows'

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class OslOutlookDesktopTitle {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);

  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int count);

  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int count);
}
"@

function Get-OutlookDesktopTitleCandidate {
    $candidatesList = [Collections.ArrayList]::new()
    $callback = [OslOutlookDesktopTitle+EnumWindowsProc]{
        param([IntPtr]$hwnd, [IntPtr]$state)

        if (-not [OslOutlookDesktopTitle]::IsWindowVisible($hwnd)) {
            return $true
        }

        [uint32]$candidateProcessId = 0
        [void][OslOutlookDesktopTitle]::GetWindowThreadProcessId($hwnd, [ref]$candidateProcessId)
        if ($candidateProcessId -eq 0) {
            return $true
        }

        try {
            $process = Get-Process -Id ([int]$candidateProcessId) -ErrorAction Stop
        } catch {
            return $true
        }
        if ($processNames -notcontains $process.ProcessName) {
            return $true
        }

        $class = [Text.StringBuilder]::new(128)
        [void][OslOutlookDesktopTitle]::GetClassName($hwnd, $class, $class.Capacity)
        $className = $class.ToString()
        if ($titleClasses -notcontains $className) {
            return $true
        }

        $title = [Text.StringBuilder]::new(512)
        [void][OslOutlookDesktopTitle]::GetWindowText($hwnd, $title, $title.Capacity)
        $titleText = $title.ToString().Trim()
        if ($titleText.Length -eq 0) {
            return $true
        }

        [void]$candidatesList.Add([pscustomobject][ordered]@{
            processName = $process.ProcessName
            className = $className
            outlookWindowTitle = $titleText
            priority = [Array]::IndexOf($titleClasses, $className)
        })
        return $true
    }
    [void][OslOutlookDesktopTitle]::EnumWindows($callback, [IntPtr]::Zero)
    return @($candidatesList.ToArray() | Sort-Object priority, processName, className, outlookWindowTitle)
}

function Start-OutlookDesktop {
    $roots = @(
        ${env:ProgramFiles},
        ${env:ProgramFiles(x86)}
    ) | Where-Object { $_ }

    foreach ($root in $roots) {
        $classic = Join-Path $root 'Microsoft Office\root\Office16\OUTLOOK.EXE'
        if (Test-Path -LiteralPath $classic) {
            Start-Process -FilePath $classic | Out-Null
            return
        }
    }

    $windowsApps = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\olk.exe'
    if (Test-Path -LiteralPath $windowsApps) {
        Start-Process -FilePath $windowsApps | Out-Null
        return
    }

    Start-Process -FilePath 'explorer.exe' -ArgumentList $aumid | Out-Null
}

if (-not $NoLaunch) {
    $initial = @(Get-OutlookDesktopTitleCandidate)
    if ($initial.Count -eq 0) {
        Start-OutlookDesktop
    }
}

$deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
do {
    $candidates = @(Get-OutlookDesktopTitleCandidate)
    if ($candidates.Count -ge 1) {
        $candidate = $candidates[0]
        [pscustomobject][ordered]@{
            processName = $candidate.processName
            className = $candidate.className
            outlookWindowTitle = $candidate.outlookWindowTitle
        } | ConvertTo-Json -Compress
        exit 0
    }
    Start-Sleep -Milliseconds 250
} while ([DateTime]::UtcNow -lt $deadline)

throw 'OUTLOOK_DESKTOP_TITLE_UNAVAILABLE'
