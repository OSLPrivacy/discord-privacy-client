param(
  [Parameter(Mandatory = $true)][int]$ProcessId,
  [Parameter(Mandatory = $true)][string]$OutputJson,
  [switch]$Capture
)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing

$process = Get-Process -Id $ProcessId -ErrorAction Stop
if ($process.MainWindowHandle -eq 0) { throw 'TASK4708 no OSL main window' }
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$process.MainWindowHandle)
$children = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$elements = @($children | ForEach-Object {
  [pscustomobject]@{
    name = $_.Current.Name
    automation_id = $_.Current.AutomationId
    control_type = $_.Current.ControlType.ProgrammaticName
    bounds = @($_.Current.BoundingRectangle.Left, $_.Current.BoundingRectangle.Top, $_.Current.BoundingRectangle.Width, $_.Current.BoundingRectangle.Height)
  }
})
if (-not $Capture) {
  [pscustomobject]@{ process_id = $ProcessId; window_name = $root.Current.Name; elements = $elements } | ConvertTo-Json -Depth 5
  exit 0
}

$data = $elements | Where-Object { $_.name -match '(?i)data|allowance|this month' } | Select-Object -First 1
if ($null -eq $data) { throw 'TASK4708 UIA Data-this-month element not found' }
$bounds = $data.bounds
if ($bounds[2] -lt 100 -or $bounds[3] -lt 50) { throw 'TASK4708 UIA bounds are too small for Data-this-month capture' }
$bitmap = New-Object System.Drawing.Bitmap ([int]$bounds[2]), ([int]$bounds[3])
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen([int]$bounds[0], [int]$bounds[1], 0, 0, $bitmap.Size)
$png = [IO.Path]::ChangeExtension($OutputJson, '.png')
$bitmap.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose(); $bitmap.Dispose()
[pscustomobject]@{
  source = 'windows-powershell-uia-interactive-session'
  capture_method = 'windows-powershell-uia-copyfromscreen'
  process_id = $ProcessId
  window_name = $root.Current.Name
  uia_name = $data.name
  uia_bounds = $bounds
  png = $png
} | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $OutputJson
