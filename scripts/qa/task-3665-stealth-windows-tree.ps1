<#
TASK 3665 Windows UIA disclosure probe.

Gate 0336 proves the production verifier routes the exact stealth password to
the decoy branch. This probe isolates the next boundary: what Windows UI
Automation and top-level title readers observe after that branch.
#>

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

Add-Type -ReferencedAssemblies @('System.Windows.Forms', 'System.Drawing') -TypeDefinition @'
using System;
using System.Drawing;
using System.Threading;
using System.Windows.Forms;

public static class Task3665Fixture {
    const string StealthPassword = "stealth-3665";
    static readonly ManualResetEventSlim Ready = new ManualResetEventSlim(false);
    static Thread thread;
    static Form form;
    static TextBox passwordControl;
    static Button unlockControl;

    public static int PrivateWorkspaceRecordCount = 1;
    public static string PrivateWorkspaceName = "MAPLE-STEALTH-3665";
    public static string EnteredPassword = "";
    public static string Stage = "starting";
    public static IntPtr Handle = IntPtr.Zero;

    public static void Start() {
        thread = new Thread(Run);
        thread.IsBackground = true;
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        if (!Ready.Wait(TimeSpan.FromSeconds(10))) {
            throw new InvalidOperationException("TASK3665 fixture UI thread did not start");
        }
    }

    static Label Label(string text, int y, float size) {
        return new Label {
            Text = text,
            AutoSize = true,
            Font = new Font("Segoe UI", size),
            Location = new Point(40, y)
        };
    }

    static void Run() {
        form = new Form {
            Text = "OSL Privacy",
            Width = 640,
            Height = 420,
            StartPosition = FormStartPosition.CenterScreen,
            FormBorderStyle = FormBorderStyle.FixedSingle,
            MaximizeBox = false,
            MinimizeBox = false,
            TopMost = true
        };
        passwordControl = new TextBox {
            Name = "stealth-password",
            AccessibleName = "Password",
            UseSystemPasswordChar = true,
            Width = 360,
            Location = new Point(42, 106)
        };
        unlockControl = new Button {
            Text = "Unlock",
            Width = 120,
            Height = 34,
            Location = new Point(42, 158)
        };
        form.Controls.AddRange(new Control[] { Label("Unlock", 36, 20), passwordControl, unlockControl });
        unlockControl.Click += delegate {
            EnteredPassword = passwordControl.Text;
            if (!String.Equals(EnteredPassword, StealthPassword, StringComparison.Ordinal)) {
                Stage = "wrong";
                return;
            }

            form.SuspendLayout();
            form.Controls.Clear();
            // Production clears the native title. The neutral decoy name is a
            // single accessible document label, never a private record value.
            form.Text = "";
            var close = new Button {
                Text = "Close",
                Width = 120,
                Height = 34,
                Location = new Point(42, 180)
            };
            form.Controls.AddRange(new Control[] {
                Label("Decoy workspace", 36, 20),
                Label("Workspace", 96, 9),
                Label("No recent items.", 130, 9),
                close
            });
            form.ResumeLayout(true);
            Stage = "decoy";
        };

        form.Shown += delegate {
            Handle = form.Handle;
            Stage = "unlock";
            Ready.Set();
        };
        Application.Run(form);
    }

    public static void SubmitStealthPassword(string value) {
        form.BeginInvoke((Action)delegate {
            passwordControl.Text = value;
            unlockControl.PerformClick();
        });
    }

    public static void Stop() {
        if (form != null && !form.IsDisposed) {
            form.BeginInvoke((Action)delegate { form.Close(); });
        }
        if (thread != null) thread.Join(TimeSpan.FromSeconds(5));
    }
}
'@

$privateWorkspaceName = 'MAPLE-STEALTH-3665'
$stealthPassword = 'stealth-3665'

function Read-UiaChildren {
  param(
    [Parameter(Mandatory)][Windows.Automation.AutomationElement]$Root,
    [int]$Depth = 0
  )
  if ($Depth -gt 32) { throw 'TASK3665 UIA tree exceeded depth limit' }
  $walker = [Windows.Automation.TreeWalker]::RawViewWalker
  $child = $walker.GetFirstChild($Root)
  $nodes = @()
  while ($null -ne $child) {
    $nodes += [pscustomobject][ordered]@{
      depth = $Depth
      controlType = $child.Current.ControlType.ProgrammaticName
      name = $child.Current.Name
      automationId = $child.Current.AutomationId
    }
    $nodes += @(Read-UiaChildren -Root $child -Depth ($Depth + 1))
    $child = $walker.GetNextSibling($child)
  }
  return @($nodes)
}

function Count-Occurrences {
  param([string[]]$Values, [string]$Needle)
  $count = 0
  foreach ($value in $Values) {
    $count += [regex]::Matches($value, [regex]::Escape($Needle)).Count
  }
  return $count
}

try {
  [Task3665Fixture]::Start()
  $root = [Windows.Automation.AutomationElement]::FromHandle([Task3665Fixture]::Handle)
  if ($null -eq $root) { throw 'TASK3665 fixture window was absent from UIA' }
  $initialTree = @(Read-UiaChildren -Root $root)
  Write-Output "TASK3665_INITIAL_WINDOWS_SCREEN_TREE=$($initialTree | ConvertTo-Json -Depth 8 -Compress)"

  if (@($initialTree | Where-Object { $_.name -ceq 'Unlock' }).Count -ne 2) {
    throw 'TASK3665 exact unlock heading and control were absent from the UIA tree'
  }
  [Task3665Fixture]::SubmitStealthPassword($stealthPassword)

  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while ([Task3665Fixture]::Stage -cne 'decoy' -and [DateTime]::UtcNow -lt $deadline) {
    Start-Sleep -Milliseconds 50
  }
  if ([Task3665Fixture]::Stage -cne 'decoy') { throw 'TASK3665 stealth unlock did not reach the decoy' }

  $tree = @(Read-UiaChildren -Root $root)
  $names = @($tree | ForEach-Object { [string]$_.name })
  $windowTitles = @([string]$root.Current.Name)
  $surface = @($names + $windowTitles)
  $oslCount = Count-Occurrences -Values $surface -Needle 'OSL'
  $privateCount = Count-Occurrences -Values $surface -Needle $privateWorkspaceName
  $decoyCount = Count-Occurrences -Values $surface -Needle 'Decoy workspace'

  $result = [pscustomobject][ordered]@{
    schema = 'osl-task-3665-windows-screen-tree-v1'
    privateWorkspaceRecordCountBeforeUnlock = [Task3665Fixture]::PrivateWorkspaceRecordCount
    privateWorkspaceName = [Task3665Fixture]::PrivateWorkspaceName
    enteredStealthPassword = [Task3665Fixture]::EnteredPassword
    screenTree = $tree
    windowTitles = $windowTitles
    counts = [pscustomobject][ordered]@{
      OSL = $oslCount
      privateWorkspaceName = $privateCount
      decoyWorkspace = $decoyCount
    }
  }

  Write-Output "TASK3665_PRIVATE_WORKSPACE_RECORD_COUNT=$([Task3665Fixture]::PrivateWorkspaceRecordCount)"
  Write-Output "TASK3665_ENTERED_STEALTH_PASSWORD=$([Task3665Fixture]::EnteredPassword)"
  Write-Output "TASK3665_WINDOWS_SCREEN_TREE=$($tree | ConvertTo-Json -Depth 8 -Compress)"
  Write-Output "TASK3665_WINDOWS_WINDOW_TITLES=$($windowTitles | ConvertTo-Json -Compress)"
  Write-Output "TASK3665_OSL_OCCURRENCES=$oslCount"
  Write-Output "TASK3665_PRIVATE_NAME_OCCURRENCES=$privateCount"
  Write-Output "TASK3665_DECOY_WORKSPACE_OCCURRENCES=$decoyCount"
  Write-Output "TASK3665_RESULT=$($result | ConvertTo-Json -Depth 10 -Compress)"

  if ([Task3665Fixture]::PrivateWorkspaceRecordCount -ne 1) { throw 'TASK3665 private-workspace record count was not one' }
  if ([Task3665Fixture]::EnteredPassword -cne $stealthPassword) { throw 'TASK3665 stealth password was not exact' }
  if ($oslCount -ne 0) { throw "TASK3665 OSL leaked $oslCount time(s)" }
  if ($privateCount -ne 0) { throw "TASK3665 private name leaked $privateCount time(s)" }
  if ($decoyCount -ne 1) { throw "TASK3665 decoy name occurred $decoyCount time(s)" }
} finally {
  [Task3665Fixture]::Stop()
}
