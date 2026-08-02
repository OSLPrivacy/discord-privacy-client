<#
Measure a logged-in Telegram Desktop conversation with pywinauto's UIA backend.

The JSON result is deliberately metadata-only: it reports row stability and
whether the composer, title, and participant identity expose text, but never
serializes content or identities. Run from the active logged-in Windows session.
#>
param(
  [ValidateRange(2, 10)] [int]$Samples = 3,
  [ValidateRange(100, 5000)] [int]$SampleDelayMilliseconds = 750
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$python = Get-Command python.exe -ErrorAction SilentlyContinue
if ($null -eq $python) { throw 'python.exe is required in the logged-in Telegram VM session' }
$probe = Join-Path $PSScriptRoot 'telegram.py'
if (-not (Test-Path -LiteralPath $probe -PathType Leaf)) { throw 'telegram.py is missing beside telegram.ps1' }

& $python.Source $probe $Samples ($SampleDelayMilliseconds / 1000.0)
if ($LASTEXITCODE -ne 0) { throw "Telegram UIA probe failed with exit code $LASTEXITCODE" }
