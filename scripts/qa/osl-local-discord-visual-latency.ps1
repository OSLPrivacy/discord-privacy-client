[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$OslExePath,

  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[0-9a-fA-F]{64}$')]
  [string]$OslExeSha256,

  [Parameter(Mandatory = $true)]
  [string]$OutputDirectory,

  [ValidateRange(3, 30)]
  [int]$TimeoutSeconds = 12
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName Accessibility
Add-Type -ReferencedAssemblies Accessibility.dll -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public sealed class OslQaHarnessException : Exception {
  public string FailureClass { get; private set; }

  public OslQaHarnessException(string failureClass) : base(failureClass) {
    FailureClass = failureClass;
  }
}

public static class OslLocalDiscordVisualNative {
  public delegate bool EnumWindowsProc(IntPtr hwnd, IntPtr state);

  [StructLayout(LayoutKind.Sequential)]
  public struct Point {
    public int X;
    public int Y;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct Rect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  // x64 INPUT is 40 bytes because of MOUSEINPUT. A union declaring only
  // KEYBDINPUT marshals to 32, and SendInput rejects a wrong cbSize outright,
  // silently failing every synthetic keystroke.
  [StructLayout(LayoutKind.Sequential, Size = 40)]
  public struct Input {
    public uint Type;
    public InputUnion Union;
  }

  [StructLayout(LayoutKind.Explicit)]
  public struct InputUnion {
    [FieldOffset(0)] public KeyboardInput Keyboard;
  }

  [StructLayout(LayoutKind.Sequential)]
  public struct KeyboardInput {
    public ushort VirtualKey;
    public ushort ScanCode;
    public uint Flags;
    public uint Time;
    public UIntPtr ExtraInfo;
  }

  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")]
  public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr state);
  [DllImport("user32.dll")]
  public static extern bool IsWindow(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool IsIconic(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern bool IsZoomed(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern IntPtr GetParent(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern IntPtr GetWindow(IntPtr hwnd, uint command);
  [DllImport("user32.dll")]
  public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")]
  public static extern bool SetWindowPos(
    IntPtr hwnd, IntPtr after, int x, int y, int width, int height, uint flags
  );
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hwnd, int command);
  [DllImport("user32.dll")]
  public static extern uint SendInput(uint count, Input[] inputs, int size);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hwnd, StringBuilder value, int capacity);
  [DllImport("oleacc.dll")]
  private static extern int AccessibleObjectFromPoint(
    Point point,
    [Out, MarshalAs(UnmanagedType.Interface)] out Accessibility.IAccessible accessible,
    [Out, MarshalAs(UnmanagedType.Struct)] out object childId
  );

  private sealed class ComposerCandidate {
    public Accessibility.IAccessible Accessible;
    public string Name;
    public int Left;
    public int Top;
    public int Width;
    public int Height;
    public int Role;
  }

  private sealed class BoundedResult {
    public object Value;
    public Exception Error;
  }

  public static string WindowText(IntPtr hwnd) {
    var value = new StringBuilder(512);
    GetWindowText(hwnd, value, value.Capacity);
    return value.ToString();
  }

  public static string WindowClass(IntPtr hwnd) {
    var value = new StringBuilder(256);
    GetClassName(hwnd, value, value.Capacity);
    return value.ToString();
  }

  public static IntPtr[] VisibleTopLevelWindowsFor(uint expectedProcessId) {
    var values = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      GetWindowThreadProcessId(hwnd, out processId);
      if (processId == expectedProcessId && IsWindowVisible(hwnd)) values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }

  public static IntPtr[] TopLevelWindowsFor(uint expectedProcessId) {
    var values = new List<IntPtr>();
    EnumWindows((hwnd, ignored) => {
      uint processId;
      GetWindowThreadProcessId(hwnd, out processId);
      if (processId == expectedProcessId) values.Add(hwnd);
      return true;
    }, IntPtr.Zero);
    return values.ToArray();
  }

  public static IntPtr[] WindowsFor(uint[] expectedProcessIds) {
    var expected = new HashSet<uint>(expectedProcessIds);
    var values = new HashSet<IntPtr>();
    EnumWindows((root, ignored) => {
      uint processId;
      GetWindowThreadProcessId(root, out processId);
      if (expected.Contains(processId)) values.Add(root);
      EnumChildWindows(root, (child, childIgnored) => {
        GetWindowThreadProcessId(child, out processId);
        if (expected.Contains(processId)) values.Add(child);
        return true;
      }, IntPtr.Zero);
      return true;
    }, IntPtr.Zero);
    var result = new IntPtr[values.Count];
    values.CopyTo(result);
    return result;
  }

  private const uint INPUT_KEYBOARD = 1;
  private const uint KEYEVENTF_KEYUP = 0x0002;
  private const uint KEYEVENTF_UNICODE = 0x0004;
  private const uint KEYEVENTF_SCANCODE = 0x0008;
  private const ushort ENTER_SCAN_CODE = 0x1c;
  private const ushort SHIFT_SCAN_CODE = 0x2a;
  private const ushort CONTROL_SCAN_CODE = 0x1d;
  private const ushort A_SCAN_CODE = 0x1e;
  private const ushort BACKSPACE_SCAN_CODE = 0x0e;
  private const int MAX_SEED_LENGTH = 512;
  private const int INPUT_POLL_MS = 15;
  private const uint STATE_SYSTEM_FOCUSED = 0x00000004;
  private const int SELFLAG_TAKEFOCUS = 0x00000001;

  private static Input KeyboardInputFor(ushort scanOrUnit, uint flags) {
    return new Input {
      Type = INPUT_KEYBOARD,
      Union = new InputUnion {
        Keyboard = new KeyboardInput {
          VirtualKey = 0,
          ScanCode = scanOrUnit,
          Flags = flags,
          Time = 0,
          ExtraInfo = UIntPtr.Zero
        }
      }
    };
  }

  private static bool SendInputs(Input[] inputs) {
    if (inputs == null || inputs.Length == 0) return false;
    return SendInput((uint)inputs.Length, inputs, Marshal.SizeOf(typeof(Input))) ==
      (uint)inputs.Length;
  }

  // Type real Unicode keystrokes, exactly like the product's send path
  // (native_discord_adapter.rs send_unicode_chunk). Slate.js only builds its
  // document from its own input pipeline, so MSAA set_accValue can never seed
  // the composer; synthetic KEYEVENTF_UNICODE input can.
  private static bool SendUnicodeChunk(char[] units) {
    if (units == null || units.Length == 0) return false;
    var inputs = new Input[units.Length * 2];
    for (var index = 0; index < units.Length; index += 1) {
      var unit = (ushort)units[index];
      inputs[index * 2] = KeyboardInputFor(unit, KEYEVENTF_UNICODE);
      inputs[index * 2 + 1] = KeyboardInputFor(unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
    }
    return SendInputs(inputs);
  }

  // Discord's soft line break. A bare Enter would send a message, so Shift is
  // held around the single physical Enter and its release is always attempted
  // so a rejected carrier can never leave Shift latched on the desktop.
  private static bool SendShiftEnter() {
    var shiftDown = SendInputs(new[] {
      KeyboardInputFor(SHIFT_SCAN_CODE, KEYEVENTF_SCANCODE)
    });
    var newline = shiftDown && SendInputs(new[] {
      KeyboardInputFor(ENTER_SCAN_CODE, KEYEVENTF_SCANCODE),
      KeyboardInputFor(ENTER_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)
    });
    var shiftUp = SendInputs(new[] {
      KeyboardInputFor(SHIFT_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)
    });
    return shiftDown && newline && shiftUp;
  }

  // Select-all then Backspace, so the disposable draft is removed through
  // Slate's own editing pipeline instead of an MSAA write it would revert.
  private static bool SendSelectAllThenDelete() {
    var controlDown = SendInputs(new[] {
      KeyboardInputFor(CONTROL_SCAN_CODE, KEYEVENTF_SCANCODE)
    });
    var selectAll = controlDown && SendInputs(new[] {
      KeyboardInputFor(A_SCAN_CODE, KEYEVENTF_SCANCODE),
      KeyboardInputFor(A_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)
    });
    var controlUp = SendInputs(new[] {
      KeyboardInputFor(CONTROL_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)
    });
    var removed = controlDown && selectAll && controlUp && SendInputs(new[] {
      KeyboardInputFor(BACKSPACE_SCAN_CODE, KEYEVENTF_SCANCODE),
      KeyboardInputFor(BACKSPACE_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)
    });
    return controlDown && selectAll && controlUp && removed;
  }

  private static void RejectUntypeableSeed(string text) {
    if (text == null || text.Length == 0 || text.Length > MAX_SEED_LENGTH ||
        text.IndexOf('\r') >= 0) {
      throw new OslQaHarnessException("native-composer-seed-text-rejected");
    }
    foreach (var character in text) {
      if (character != '\n' && Char.IsControl(character)) {
        throw new OslQaHarnessException("native-composer-seed-text-rejected");
      }
    }
  }

  private static bool TypeUnicodeText(string text) {
    RejectUntypeableSeed(text);
    var segments = text.Split('\n');
    for (var index = 0; index < segments.Length; index += 1) {
      // Newlines are Shift+Enter only; a bare Enter is never emitted here.
      if (index > 0 && !SendShiftEnter()) return false;
      if (segments[index].Length == 0) continue;
      if (!SendUnicodeChunk(segments[index].ToCharArray())) return false;
    }
    return true;
  }

  private static object RunBounded(Func<object> operation, int timeoutMs, string failureClass) {
    if (timeoutMs < 100 || timeoutMs > 10000) throw new ArgumentOutOfRangeException("timeoutMs");
    var result = new BoundedResult();
    var worker = new Thread(() => {
      try {
        result.Value = operation();
      } catch (Exception error) {
        result.Error = error;
      }
    });
    worker.IsBackground = true;
    worker.SetApartmentState(ApartmentState.STA);
    worker.Start();
    if (!worker.Join(timeoutMs)) {
      try { worker.Abort(); } catch {}
      throw new TimeoutException(failureClass);
    }
    if (result.Error != null) {
      var classified = result.Error as OslQaHarnessException;
      if (classified != null) throw classified;
      if (result.Error is TimeoutException) throw new TimeoutException(failureClass);
      throw new OslQaHarnessException(
        failureClass.Replace("-timeout", "-interop-rejected")
      );
    }
    return result.Value;
  }

  private static Accessibility.IAccessible AccessibleSelfAtPoint(Point point) {
    Accessibility.IAccessible accessible;
    object childId;
    if (AccessibleObjectFromPoint(point, out accessible, out childId) < 0 ||
        accessible == null || childId == null) {
      return null;
    }
    int id;
    try {
      id = Convert.ToInt32(childId);
    } catch {
      return null;
    }
    if (id == 0) return accessible;
    try {
      var child = accessible.get_accChild(childId) as Accessibility.IAccessible;
      if (child != null) return child;
    } catch {}
    return accessible;
  }

  private static Accessibility.IAccessible AccessibleParent(
    Accessibility.IAccessible accessible
  ) {
    try {
      return accessible.accParent as Accessibility.IAccessible;
    } catch {
      return null;
    }
  }

  private static bool ValidComposerName(string name) {
    if (String.IsNullOrWhiteSpace(name)) return false;
    var trimmed = name.Trim();
    string remainder;
    if (trimmed.StartsWith("Message ", StringComparison.Ordinal)) {
      remainder = trimmed.Substring(8);
    } else if (trimmed.StartsWith("message ", StringComparison.Ordinal)) {
      remainder = trimmed.Substring(8);
    } else {
      return false;
    }
    remainder = remainder.TrimStart('@', '#').Trim();
    if (remainder.Length == 0 || Encoding.UTF8.GetByteCount(remainder) > 512) return false;
    foreach (var character in remainder) {
      if (Char.IsControl(character)) return false;
    }
    return true;
  }

  private static ComposerCandidate ReadCandidate(
    Accessibility.IAccessible accessible,
    Rect root
  ) {
    string name;
    int role;
    uint state;
    int left;
    int top;
    int width;
    int height;
    try {
      name = accessible.get_accName(0);
      role = Convert.ToInt32(accessible.get_accRole(0));
      state = unchecked((uint)Convert.ToInt32(accessible.get_accState(0)));
      accessible.accLocation(out left, out top, out width, out height, 0);
    } catch {
      return null;
    }
    const uint unavailable = 0x00000001;
    const uint readOnly = 0x00000040;
    const uint invisible = 0x00008000;
    if (role != 42 || (state & (unavailable | readOnly | invisible)) != 0 ||
        !ValidComposerName(name) || width < 96 || height < 16 || height > 160) {
      return null;
    }
    long right = (long)left + width;
    long bottom = (long)top + height;
    int rootWidth = root.Right - root.Left;
    int rootHeight = root.Bottom - root.Top;
    if (rootWidth <= 0 || rootHeight <= 0 || width <= 0 || height <= 0 ||
        left < root.Left || top < root.Top || right > root.Right || bottom > root.Bottom ||
        top < root.Top + rootHeight / 2) {
      return null;
    }
    int nearBottom = Math.Max(64, Math.Min(180, rootHeight / 3));
    if (bottom < root.Bottom - nearBottom) return null;
    return new ComposerCandidate {
      Accessible = accessible,
      Name = name,
      Left = left,
      Top = top,
      Width = width,
      Height = height,
      Role = role
    };
  }

  private static bool SameCandidate(ComposerCandidate left, ComposerCandidate right) {
    return left.Role == right.Role &&
      String.Equals(left.Name, right.Name, StringComparison.Ordinal) &&
      left.Left == right.Left && left.Top == right.Top &&
      left.Width == right.Width && left.Height == right.Height;
  }

  private static Accessibility.IAccessible FindExactComposer(
    IntPtr hwnd,
    Stopwatch timer,
    int timeoutMs
  ) {
    Rect root;
    if (!IsWindow(hwnd) || !IsWindowVisible(hwnd) || !GetWindowRect(hwnd, out root)) {
      throw new OslQaHarnessException("native-composer-window-unavailable");
    }
    int rootWidth = root.Right - root.Left;
    int rootHeight = root.Bottom - root.Top;
    if (rootWidth <= 0 || rootHeight <= 0) {
      throw new OslQaHarnessException("native-composer-root-geometry-invalid");
    }
    ComposerCandidate exact = null;
    int probes = 0;
    foreach (var xPercent in new[] { 40, 45, 50, 55 }) {
      foreach (var bottomOffset in new[] { 28, 37, 48 }) {
        probes += 1;
        if (probes > 12 || timer.ElapsedMilliseconds >= timeoutMs) {
          throw new OslQaHarnessException("native-composer-probe-timeout");
        }
        var accessible = AccessibleSelfAtPoint(new Point {
          X = root.Left + rootWidth * xPercent / 100,
          Y = root.Bottom - bottomOffset
        });
        for (var depth = 0; accessible != null && depth < 12; depth += 1) {
          if (timer.ElapsedMilliseconds >= timeoutMs) {
            throw new OslQaHarnessException("native-composer-probe-timeout");
          }
          var candidate = ReadCandidate(accessible, root);
          if (candidate != null) {
            if (exact == null) {
              exact = candidate;
            } else if (!SameCandidate(exact, candidate)) {
              throw new OslQaHarnessException("native-composer-ambiguous");
            }
            break;
          }
          accessible = AccessibleParent(accessible);
        }
      }
    }
    if (exact == null) throw new OslQaHarnessException("native-composer-not-found");
    return exact.Accessible;
  }

  private static string ReadNormalizedComposerValue(
    Accessibility.IAccessible composer
  ) {
    string name;
    string value;
    try {
      name = composer.get_accName(0);
      value = composer.get_accValue(0) ?? String.Empty;
    } catch (COMException) {
      throw new OslQaHarnessException("native-composer-value-com-rejected");
    }
    // Discord exposes its placeholder as the MSAA value while the actual
    // editable document is empty. Normalize only the exact, independently
    // validated placeholder identity; every real draft remains byte-exact.
    if (String.Equals(value, name, StringComparison.Ordinal) && ValidComposerName(name)) {
      return String.Empty;
    }
    return value;
  }

  public static string ReadComposerValue(IntPtr hwnd, int timeoutMs) {
    return (string)RunBounded(() => {
      var timer = Stopwatch.StartNew();
      var composer = FindExactComposer(hwnd, timer, timeoutMs);
      return (object)ReadNormalizedComposerValue(composer);
    }, timeoutMs, "native-composer-read-timeout");
  }

  // The worker thread is hard-aborted at timeoutMs, so every poll loop stops
  // slightly earlier and surfaces its own precise failure class instead.
  private static long VerifyDeadlineMs(int timeoutMs) {
    return (long)Math.Max(300, timeoutMs - 200);
  }

  // Real keystrokes go wherever the desktop focus is, so nothing may be typed
  // until the exact borrowed Discord window is foreground AND the exact
  // resolved composer reports MSAA keyboard focus. Both checks fail closed.
  private static void FocusExactComposer(
    IntPtr hwnd,
    Accessibility.IAccessible composer,
    Stopwatch timer,
    int timeoutMs
  ) {
    if (!IsWindow(hwnd) || !IsWindowVisible(hwnd)) {
      throw new OslQaHarnessException("native-composer-window-unavailable");
    }
    var deadlineMs = Math.Min(
      VerifyDeadlineMs(timeoutMs),
      timer.ElapsedMilliseconds + (long)Math.Max(200, timeoutMs * 2 / 5)
    );
    if (GetForegroundWindow() != hwnd) {
      SetForegroundWindow(hwnd);
      while (GetForegroundWindow() != hwnd) {
        if (timer.ElapsedMilliseconds >= deadlineMs) {
          throw new OslQaHarnessException("native-composer-foreground-rejected");
        }
        Thread.Sleep(INPUT_POLL_MS);
      }
    }
    var requested = false;
    var stateReadable = false;
    for (;;) {
      try {
        var state = unchecked((uint)Convert.ToInt32(composer.get_accState(0)));
        stateReadable = true;
        if ((state & STATE_SYSTEM_FOCUSED) != 0) return;
      } catch {
        // A transient MSAA rejection is retried; a state that never becomes
        // readable fails closed below without a key ever being emitted.
      }
      if (!requested) {
        requested = true;
        try {
          composer.accSelect(SELFLAG_TAKEFOCUS, 0);
        } catch {}
      }
      if (timer.ElapsedMilliseconds >= deadlineMs) {
        throw new OslQaHarnessException(
          stateReadable
            ? "native-composer-focus-rejected"
            : "native-composer-focus-state-rejected"
        );
      }
      Thread.Sleep(INPUT_POLL_MS);
    }
  }

  private static bool AwaitComposerValue(
    Accessibility.IAccessible composer,
    string expected,
    Stopwatch timer,
    long deadlineMs
  ) {
    for (;;) {
      try {
        if (String.Equals(
          ReadNormalizedComposerValue(composer), expected, StringComparison.Ordinal
        )) {
          return true;
        }
      } catch (OslQaHarnessException) {
        // Slate re-renders while it consumes the keystrokes, so a transient
        // MSAA rejection is retried instead of aborting the acceptance run.
      }
      if (timer.ElapsedMilliseconds >= deadlineMs) return false;
      Thread.Sleep(INPUT_POLL_MS);
    }
  }

  private static void RestoreForeground(IntPtr previous, IntPtr current) {
    if (previous == IntPtr.Zero || previous == current || !IsWindow(previous)) return;
    try {
      SetForegroundWindow(previous);
    } catch {}
  }

  // Seed the disposable draft with real synthetic Unicode input. The composer
  // must already hold exactly expectedCurrent both before and after focusing,
  // so a draft the operator typed themselves is never typed into.
  public static bool TypeComposerValue(
    IntPtr hwnd,
    string expectedCurrent,
    string next,
    int timeoutMs
  ) {
    if (expectedCurrent == null || next == null) throw new ArgumentNullException();
    RejectUntypeableSeed(next);
    return (bool)RunBounded(() => {
      var timer = Stopwatch.StartNew();
      var restoreTo = GetForegroundWindow();
      try {
        var composer = FindExactComposer(hwnd, timer, timeoutMs);
        if (!String.Equals(
          ReadNormalizedComposerValue(composer), expectedCurrent, StringComparison.Ordinal
        )) {
          return (object)false;
        }
        FocusExactComposer(hwnd, composer, timer, timeoutMs);
        if (!String.Equals(
          ReadNormalizedComposerValue(composer), expectedCurrent, StringComparison.Ordinal
        )) {
          return (object)false;
        }
        if (!TypeUnicodeText(next)) return (object)false;
        return (object)AwaitComposerValue(composer, next, timer, VerifyDeadlineMs(timeoutMs));
      } finally {
        RestoreForeground(restoreTo, hwnd);
      }
    }, timeoutMs, "native-composer-write-timeout");
  }

  // Clear the disposable draft with real select-all plus Backspace. An empty
  // expectation is refused outright so this can only ever remove the exact
  // draft this harness generated and still observes.
  public static bool ClearComposerValue(
    IntPtr hwnd,
    string expectedCurrent,
    int timeoutMs
  ) {
    if (expectedCurrent == null) throw new ArgumentNullException();
    if (expectedCurrent.Length == 0) {
      throw new OslQaHarnessException("native-composer-clear-target-rejected");
    }
    return (bool)RunBounded(() => {
      var timer = Stopwatch.StartNew();
      var restoreTo = GetForegroundWindow();
      try {
        var composer = FindExactComposer(hwnd, timer, timeoutMs);
        if (!String.Equals(
          ReadNormalizedComposerValue(composer), expectedCurrent, StringComparison.Ordinal
        )) {
          return (object)false;
        }
        FocusExactComposer(hwnd, composer, timer, timeoutMs);
        if (!String.Equals(
          ReadNormalizedComposerValue(composer), expectedCurrent, StringComparison.Ordinal
        )) {
          return (object)false;
        }
        if (!SendSelectAllThenDelete()) return (object)false;
        return (object)AwaitComposerValue(
          composer, String.Empty, timer, VerifyDeadlineMs(timeoutMs)
        );
      } finally {
        RestoreForeground(restoreTo, hwnd);
      }
    }, timeoutMs, "native-composer-write-timeout");
  }
}
'@

$exactPath = [IO.Path]::GetFullPath($OslExePath)
if (-not [IO.File]::Exists($exactPath)) { throw 'exact OSL executable is absent' }
$actualSha = (Get-FileHash -LiteralPath $exactPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualSha -cne $OslExeSha256.ToLowerInvariant()) {
  throw 'exact OSL executable hash mismatch'
}
[IO.Directory]::CreateDirectory([IO.Path]::GetFullPath($OutputDirectory)) | Out-Null
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$stagePath = Join-Path $outputRoot 'acceptance-stage.txt'

function Set-AcceptanceStage([string]$Stage) {
  $script:acceptanceStage = $Stage
  [IO.File]::WriteAllText($stagePath, $Stage, [Text.UTF8Encoding]::new($false))
}

function Get-SafeInnerFailureClass([System.Management.Automation.ErrorRecord]$Record) {
  $exception = $Record.Exception
  for ($depth = 0; $depth -lt 8; $depth += 1) {
    if ($exception -is [OslQaHarnessException]) {
      return $exception.FailureClass
    }
    if ($null -eq $exception.InnerException) { break }
    $exception = $exception.InnerException
  }
  switch ($exception.GetType().Name) {
    'TimeoutException' { 'timeout'; break }
    'ElementNotAvailableException' { 'element-unavailable'; break }
    'COMException' { 'com-rejected'; break }
    'InvalidOperationException' { 'invalid-operation'; break }
    'UnauthorizedAccessException' { 'access-denied'; break }
    'ArgumentException' { 'contract-rejected'; break }
    'ArgumentNullException' { 'contract-rejected'; break }
    'ArgumentOutOfRangeException' { 'contract-rejected'; break }
    default { 'interop-rejected' }
  }
}

$disposablePlaintext = 'OSL visual QA ' + [Guid]::NewGuid().ToString('N')
$generatedNativeDraft = 'OSL native QA ' + [Guid]::NewGuid().ToString('N')
$nativeDraft = $null
$preexistingNativeDraftDetected = $false
$preexistingNativeDraftPreserved = $false
$disposableNativeDraftSeeded = $false
$disposableNativeDraftCleaned = $false
$plaintextBytes = [Text.Encoding]::UTF8.GetBytes($disposablePlaintext)
$sha = [Security.Cryptography.SHA256]::Create()
try {
  $plaintextSha256 = -join ($sha.ComputeHash($plaintextBytes) | ForEach-Object {
    $_.ToString('x2')
  })
} finally {
  $sha.Dispose()
  [Array]::Clear($plaintextBytes, 0, $plaintextBytes.Length)
}

function Get-Rect([IntPtr]$Hwnd) {
  $rect = New-Object OslLocalDiscordVisualNative+Rect
  if (-not [OslLocalDiscordVisualNative]::GetWindowRect($Hwnd, [ref]$rect)) {
    throw 'window geometry is unavailable'
  }
  [pscustomobject]@{
    Left = $rect.Left
    Top = $rect.Top
    Width = $rect.Right - $rect.Left
    Height = $rect.Bottom - $rect.Top
  }
}

function Test-Contained([object]$Inner, [object]$Outer, [int]$Tolerance = 3) {
  $Inner.Left -ge ($Outer.Left - $Tolerance) -and
    $Inner.Top -ge ($Outer.Top - $Tolerance) -and
    ($Inner.Left + $Inner.Width) -le ($Outer.Left + $Outer.Width + $Tolerance) -and
    ($Inner.Top + $Inner.Height) -le ($Outer.Top + $Outer.Height + $Tolerance)
}

function Get-ExactOsl([bool]$RequireVisibleOverlay = $true) {
  $matches = @(Get-CimInstance Win32_Process | Where-Object {
    try {
      [string]::Equals(
        [IO.Path]::GetFullPath([string]$_.ExecutablePath),
        $exactPath,
        [StringComparison]::OrdinalIgnoreCase
      ) -and [string]$_.CommandLine -notmatch '--osl-borrowed-window-guardian-v1'
    } catch {
      $false
    }
  })
  if ($matches.Count -ne 1) {
    throw [OslQaHarnessException]::new('osl-process-unavailable-or-ambiguous')
  }
  $process = Get-Process -Id ([int]$matches[0].ProcessId) -ErrorAction Stop
  $windows = @([OslLocalDiscordVisualNative]::TopLevelWindowsFor([uint32]$process.Id))
  $main = @($windows | Where-Object {
    [OslLocalDiscordVisualNative]::WindowText($_) -ceq 'OSL Privacy' -and
      [OslLocalDiscordVisualNative]::WindowClass($_) -ceq 'Tauri Window' -and
      [OslLocalDiscordVisualNative]::IsWindowVisible($_)
  })
  $overlay = @($windows | Where-Object {
    [OslLocalDiscordVisualNative]::WindowText($_) -ceq 'OSL private composer' -and
      [OslLocalDiscordVisualNative]::WindowClass($_) -ceq 'Tauri Window'
  })
  if ($main.Count -ne 1) {
    throw [OslQaHarnessException]::new('osl-main-window-unavailable-or-ambiguous')
  }
  if ($overlay.Count -ne 1) {
    throw [OslQaHarnessException]::new('osl-overlay-window-unavailable-or-ambiguous')
  }
  if ($RequireVisibleOverlay -and
      -not [OslLocalDiscordVisualNative]::IsWindowVisible([IntPtr]$overlay[0])) {
    throw [OslQaHarnessException]::new('osl-overlay-not-visible')
  }
  [pscustomobject]@{
    Process = $process
    MainHwnd = [IntPtr]$main[0]
    OverlayHwnd = [IntPtr]$overlay[0]
    MainRoot = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$main[0])
    OverlayRoot = [Windows.Automation.AutomationElement]::FromHandle([IntPtr]$overlay[0])
  }
}

function Get-ExactDiscord([object]$Osl) {
  $oslCim = Get-CimInstance Win32_Process -Filter "ProcessId=$($Osl.Process.Id)"
  $allowedNames = @('Discord.exe', 'DiscordPTB.exe', 'DiscordCanary.exe')
  $discordProcesses = @(Get-CimInstance Win32_Process | Where-Object {
    $_.ExecutablePath -and
      $_.SessionId -eq $oslCim.SessionId -and
      [IO.Path]::GetFileName([string]$_.ExecutablePath) -cin $allowedNames
  })
  if ($discordProcesses.Count -eq 0 -or $discordProcesses.Count -gt 32) {
    throw 'bounded Discord process inventory is unavailable'
  }
  $discordPids = [uint32[]]@($discordProcesses | ForEach-Object { [uint32]$_.ProcessId })
  $windows = @([OslLocalDiscordVisualNative]::WindowsFor($discordPids) | Where-Object {
    [OslLocalDiscordVisualNative]::WindowClass($_) -ceq 'Chrome_WidgetWin_1' -and
      [OslLocalDiscordVisualNative]::IsWindowVisible($_)
  })
  if ($windows.Count -ne 1) { throw 'exact embedded Discord window is unavailable or ambiguous' }
  $hwnd = [IntPtr]$windows[0]
  if ([OslLocalDiscordVisualNative]::GetWindow($hwnd, 4) -ne $Osl.MainHwnd) {
    throw 'the exact Discord window is not embedded in OSL'
  }
  [uint32]$windowPid = 0
  [void][OslLocalDiscordVisualNative]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
  $owner = @($discordProcesses | Where-Object { [uint32]$_.ProcessId -eq $windowPid })
  if ($owner.Count -ne 1) { throw 'exact Discord window owner is unavailable' }
  $signature = Get-AuthenticodeSignature -LiteralPath ([string]$owner[0].ExecutablePath)
  if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
      -not $signature.SignerCertificate -or
      $signature.SignerCertificate.Subject -notmatch '(?i)Discord') {
    throw 'Discord publisher identity is not trusted'
  }
  [pscustomobject]@{
    Hwnd = $hwnd
    Root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    ProcessId = [int]$windowPid
    ProcessIds = @($discordProcesses | ForEach-Object { [int]$_.ProcessId } | Sort-Object)
  }
}

function Resolve-Control(
  [Windows.Automation.AutomationElement]$Root,
  [Windows.Automation.ControlType]$Type,
  [string]$AutomationId,
  [bool]$Enabled = $true
) {
  $condition = [Windows.Automation.AndCondition]::new([Windows.Automation.Condition[]]@(
    [Windows.Automation.PropertyCondition]::new(
      [Windows.Automation.AutomationElement]::ControlTypeProperty,
      $Type
    ),
    [Windows.Automation.PropertyCondition]::new(
      [Windows.Automation.AutomationElement]::AutomationIdProperty,
      $AutomationId
    )
  ))
  $matches = @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object {
      -not $_.Current.IsOffscreen -and
      -not $_.Current.BoundingRectangle.IsEmpty -and
      (-not $Enabled -or $_.Current.IsEnabled)
    })
  if ($matches.Count -ne 1) { throw "exact UI control is unavailable: $AutomationId" }
  $matches[0]
}

function Get-Value([Windows.Automation.AutomationElement]$Element) {
  $pattern = $Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
  [string]$pattern.Current.Value
}

function Set-Value(
  [Windows.Automation.AutomationElement]$Element,
  [string]$Value
) {
  $pattern = $Element.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)
  $pattern.SetValue($Value)
}

function Get-Toggle([Windows.Automation.AutomationElement]$Element) {
  $pattern = $Element.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
  $pattern.Current.ToggleState -eq [Windows.Automation.ToggleState]::On
}

function Invoke-Toggle([Windows.Automation.AutomationElement]$Element) {
  $pattern = $Element.GetCurrentPattern([Windows.Automation.TogglePattern]::Pattern)
  $pattern.Toggle()
}

function Get-NativeDiscordDraft([object]$Discord) {
  try {
    [OslLocalDiscordVisualNative]::ReadComposerValue($Discord.Hwnd, 1500)
  } catch {
    $inner = Get-SafeInnerFailureClass $_
    $failure = if ($inner.StartsWith('native-composer-', [StringComparison]::Ordinal)) {
      $inner
    } else {
      'native-composer-read-' + $inner
    }
    throw [OslQaHarnessException]::new($failure)
  }
}

function Get-NativeWriteFailureClass([System.Management.Automation.ErrorRecord]$Record) {
  $inner = Get-SafeInnerFailureClass $Record
  if ($inner.StartsWith('native-composer-', [StringComparison]::Ordinal)) {
    $inner
  } else {
    'native-composer-write-' + $inner
  }
}

# Seeds the disposable draft with real synthetic Unicode keystrokes, the same
# way the product's send path does. Only ever called with an $Expected the
# composer must already hold exactly; the native side re-proves that after it
# takes focus and before a single key is emitted.
function Set-NativeDiscordDraft(
  [object]$Discord,
  [string]$Expected,
  [string]$Next
) {
  if ([string]::IsNullOrEmpty($Next)) {
    throw [OslQaHarnessException]::new('native-composer-seed-text-rejected')
  }
  try {
    if (-not [OslLocalDiscordVisualNative]::TypeComposerValue(
      $Discord.Hwnd,
      $Expected,
      $Next,
      4000
    )) {
      throw [OslQaHarnessException]::new('native-composer-write-compare-rejected')
    }
  } catch [OslQaHarnessException] {
    throw
  } catch {
    throw [OslQaHarnessException]::new((Get-NativeWriteFailureClass $_))
  }
}

# Clears the disposable draft with real select-all plus Backspace. $Expected is
# mandatory and must be the exact disposable draft, so this can never remove a
# draft the operator typed themselves.
function Clear-NativeDiscordDraft(
  [object]$Discord,
  [string]$Expected
) {
  if ([string]::IsNullOrEmpty($Expected)) {
    throw [OslQaHarnessException]::new('native-composer-clear-target-rejected')
  }
  try {
    if (-not [OslLocalDiscordVisualNative]::ClearComposerValue(
      $Discord.Hwnd,
      $Expected,
      4000
    )) {
      throw [OslQaHarnessException]::new('native-composer-clear-compare-rejected')
    }
  } catch [OslQaHarnessException] {
    throw
  } catch {
    throw [OslQaHarnessException]::new((Get-NativeWriteFailureClass $_))
  }
}

function Set-ComposerLock([bool]$Protected, [string]$StagePrefix) {
  Set-AcceptanceStage "$StagePrefix-discover"
  $before = Get-ExactOsl $false
  Set-AcceptanceStage "$StagePrefix-resolve-control"
  $lock = Resolve-Control $before.MainRoot ([Windows.Automation.ControlType]::Button) `
    'discord-qa-toggle-composer'
  Set-AcceptanceStage "$StagePrefix-read-state"
  if ((Get-Toggle $lock) -eq $Protected) {
    return [pscustomobject]@{
      Protected = $Protected
      ToggleMs = 0
      OverlayReused = $true
    }
  }
  $overlayHwnd = $before.OverlayHwnd
  $timer = [Diagnostics.Stopwatch]::StartNew()
  Set-AcceptanceStage "$StagePrefix-toggle"
  Invoke-Toggle $lock
  Set-AcceptanceStage "$StagePrefix-wait"
  $state = Wait-For {
    $fresh = Get-ExactOsl $false
    $freshLock = Resolve-Control $fresh.MainRoot ([Windows.Automation.ControlType]::Button) `
      'discord-qa-toggle-composer'
    $visible = [OslLocalDiscordVisualNative]::IsWindowVisible($fresh.OverlayHwnd)
    if ((Get-Toggle $freshLock) -ne $Protected -or $visible -ne $Protected) {
      return $null
    }
    if ($fresh.OverlayHwnd -ne $overlayHwnd) {
      throw 'protected composer WebView was recreated during lock swap'
    }
    $fresh
  } 'composer-lock-toggle-timeout'
  $timer.Stop()
  [pscustomobject]@{
    Protected = $Protected
    ToggleMs = [int][Math]::Ceiling($timer.Elapsed.TotalMilliseconds)
    OverlayReused = $state.OverlayHwnd -eq $overlayHwnd
  }
}

function Get-VisibleExactTextCount(
  [Windows.Automation.AutomationElement]$Root,
  [string]$Expected
) {
  @(Get-VisibleExactTextElements $Root $Expected).Count
}

function Get-VisibleExactTextElements(
  [Windows.Automation.AutomationElement]$Root,
  [string]$Expected
) {
  $condition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ControlTypeProperty,
    [Windows.Automation.ControlType]::Text
  )
  @($Root.FindAll([Windows.Automation.TreeScope]::Descendants, $condition) |
    Where-Object {
      -not $_.Current.IsOffscreen -and
      -not $_.Current.BoundingRectangle.IsEmpty -and
      [string]$_.Current.Name -ceq $Expected
    })
}

function Get-SentMarked([Windows.Automation.AutomationElement]$Root) {
  $status = Resolve-Control $Root ([Windows.Automation.ControlType]::Text) 'overlay-status' $false
  [string]$status.Current.Name -ceq
    'Sent privately through OSL. Discord received only the private-message marker.'
}

function Wait-For([scriptblock]$Check, [string]$FailureClass) {
  $waitDeadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    try {
      $value = & $Check
      if ($value) { return $value }
    } catch [Windows.Automation.ElementNotAvailableException] {
    } catch [System.Runtime.InteropServices.COMException] {
    } catch [System.Management.Automation.RuntimeException] {
    }
    Start-Sleep -Milliseconds 10
  } while ([DateTime]::UtcNow -lt $waitDeadline)
  throw $FailureClass
}

function Capture-Window([IntPtr]$Hwnd, [string]$Name) {
  $rect = Get-Rect $Hwnd
  if ($rect.Width -lt 1 -or $rect.Height -lt 1) { throw 'screenshot geometry is invalid' }
  $path = Join-Path $outputRoot $Name
  $bitmap = New-Object Drawing.Bitmap $rect.Width, $rect.Height
  try {
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    try {
      $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)
    } finally {
      $graphics.Dispose()
    }
    $bitmap.Save($path, [Drawing.Imaging.ImageFormat]::Png)
  } finally {
    $bitmap.Dispose()
  }
  [pscustomobject]@{
    Filename = [IO.Path]::GetFileName($path)
    Sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    Width = $rect.Width
    Height = $rect.Height
  }
}

function Expose-ForScreenshot([object]$Osl, [IntPtr]$ExpectedForeground) {
  $showNoActivate = [uint32](0x0010 -bor 0x0001 -bor 0x0002 -bor 0x0040)
  if (-not [OslLocalDiscordVisualNative]::SetWindowPos(
    $Osl.MainHwnd, [IntPtr]::Zero, 0, 0, 0, 0, $showNoActivate
  )) {
    throw 'no-activate OSL screenshot stacking was rejected'
  }
  if (-not [OslLocalDiscordVisualNative]::SetWindowPos(
    $Osl.OverlayHwnd, [IntPtr]::Zero, 0, 0, 0, 0, $showNoActivate
  )) {
    throw 'no-activate overlay screenshot stacking was rejected'
  }
  if ([OslLocalDiscordVisualNative]::GetForegroundWindow() -ne $ExpectedForeground) {
    throw 'screenshot preparation changed the foreground window'
  }
}

function Get-GeometryState([object]$Osl, [object]$Discord, [string]$Phase) {
  $mainRect = Get-Rect $Osl.MainHwnd
  $overlayRect = Get-Rect $Osl.OverlayHwnd
  $discordRect = Get-Rect $Discord.Hwnd
  [pscustomobject]@{
    Phase = $Phase
    Main = $mainRect
    Discord = $discordRect
    Overlay = $overlayRect
    DiscordParentExact = [OslLocalDiscordVisualNative]::GetWindow($Discord.Hwnd, 4) -eq
      $Osl.MainHwnd
    DiscordContained = Test-Contained $discordRect $mainRect
    OverlayContained = Test-Contained $overlayRect $mainRect
    MainMinimized = [OslLocalDiscordVisualNative]::IsIconic($Osl.MainHwnd)
    MainMaximized = [OslLocalDiscordVisualNative]::IsZoomed($Osl.MainHwnd)
    ForegroundUnchanged = [OslLocalDiscordVisualNative]::GetForegroundWindow() -eq
      $originalForeground
  }
}

function Wait-GeometryState([object]$Discord, [string]$Phase) {
  Wait-For {
    $freshOsl = Get-ExactOsl
    $state = Get-GeometryState $freshOsl $Discord $Phase
    if (-not $state.DiscordParentExact -or
        -not $state.DiscordContained -or
        -not $state.OverlayContained) {
      return $null
    }
    $state
  } "geometry-tether-timeout-$Phase"
}

function Set-Eye(
  [object]$Osl,
  [bool]$Visible,
  [bool]$ExpectedLock
) {
  $eye = Resolve-Control $Osl.MainRoot ([Windows.Automation.ControlType]::Button) `
    'discord-qa-transcript-visibility'
  if ((Get-Toggle $eye) -ne $Visible) { Invoke-Toggle $eye }
  $state = Wait-For {
    $freshOsl = Get-ExactOsl
    $freshEye = Resolve-Control $freshOsl.MainRoot ([Windows.Automation.ControlType]::Button) `
      'discord-qa-transcript-visibility'
    if ((Get-Toggle $freshEye) -ne $Visible) { return $null }
    $freshLock = Resolve-Control $freshOsl.MainRoot ([Windows.Automation.ControlType]::Button) `
      'discord-qa-toggle-composer'
    if ((Get-Toggle $freshLock) -ne $ExpectedLock) {
      throw 'eye toggle changed the protected-composer lock'
    }
    $visibleCount = Get-VisibleExactTextCount $freshOsl.OverlayRoot $disposablePlaintext
    if (($Visible -and $visibleCount -ne 1) -or (-not $Visible -and $visibleCount -ne 0)) {
      return $null
    }
    [pscustomobject]@{
      EyeVisible = $Visible
      LockProtected = $ExpectedLock
      PlaintextRenderCount = $visibleCount
    }
  } 'eye-toggle-timeout'
  $state
}

$originalForeground = [OslLocalDiscordVisualNative]::GetForegroundWindow()
$receiptPath = Join-Path $outputRoot 'acceptance-receipt.json'
$receipt = $null
$osl = $null
$originalRect = $null
$originalMaximized = $false
$acceptanceStage = 'bootstrap'
Set-AcceptanceStage 'discover-osl'
try {
  $osl = Get-ExactOsl $false
  Set-AcceptanceStage 'bootstrap-protected-lock'
  $bootstrapLock = Set-ComposerLock $true 'bootstrap-protected-lock'
  Set-AcceptanceStage 'bootstrap-visible-overlay'
  $osl = Get-ExactOsl $true
  Set-AcceptanceStage 'discover-discord'
  $discord = Get-ExactDiscord $osl
  Set-AcceptanceStage 'initial-geometry'
  $originalRect = Get-Rect $osl.MainHwnd
  $originalMaximized = [OslLocalDiscordVisualNative]::IsZoomed($osl.MainHwnd)
  $initialGeometry = Get-GeometryState $osl $discord 'initial'
  if (-not $initialGeometry.DiscordParentExact -or -not $initialGeometry.DiscordContained) {
    throw 'initial Discord tether is invalid'
  }

  Set-AcceptanceStage 'composer-controls'
  $lock = Resolve-Control $osl.MainRoot ([Windows.Automation.ControlType]::Button) `
    'discord-qa-toggle-composer'
  $lockProtected = Get-Toggle $lock
  if (-not $lockProtected) { throw 'protected composer lock is not enabled' }
  $draft = Resolve-Control $osl.OverlayRoot ([Windows.Automation.ControlType]::Edit) `
    'protected-draft'
  if ((Get-Value $draft).Length -ne 0) {
    throw 'protected composer must be empty before acceptance'
  }

  Set-AcceptanceStage 'initial-native-unlock'
  $initialUnlock = Set-ComposerLock $false 'initial-native-unlock'
  Set-AcceptanceStage 'native-composer-resolve-read-initial'
  $initialNativeDraft = Get-NativeDiscordDraft $discord
  if ($initialNativeDraft.Length -gt 0) {
    $preexistingNativeDraftDetected = $true
    $nativeDraft = $initialNativeDraft
    Set-AcceptanceStage 'native-composer-preexisting-draft-memory-only'
  } else {
    $nativeDraft = $generatedNativeDraft
    Set-AcceptanceStage 'native-composer-resolve-write-seed'
    Set-NativeDiscordDraft $discord '' $nativeDraft
    $disposableNativeDraftSeeded = $true
    Set-AcceptanceStage 'native-composer-resolve-verify-seed'
    if ((Get-NativeDiscordDraft $discord) -cne $nativeDraft) {
      throw [OslQaHarnessException]::new('native-composer-disposable-seed-rejected')
    }
  }
  $initialNativeDraft = $null

  Set-AcceptanceStage 'initial-protected-lock'
  $initialLock = Set-ComposerLock $true 'initial-protected-lock'
  Set-AcceptanceStage 'initial-native-suspension'
  if ((Get-NativeDiscordDraft $discord).Length -ne 0) {
    throw 'native draft was not suspended when protection opened'
  }
  $osl = Get-ExactOsl
  $draft = Resolve-Control $osl.OverlayRoot ([Windows.Automation.ControlType]::Edit) `
    'protected-draft'
  Set-Value $draft ($disposablePlaintext + 'XY')
  if (-not [OslLocalDiscordVisualNative]::SetForegroundWindow($osl.OverlayHwnd)) {
    throw 'exact protected composer could not receive editing keys'
  }
  $draft.SetFocus()
  [Windows.Forms.SendKeys]::SendWait('{END}{BACKSPACE}')
  if ((Get-Value $draft) -cne ($disposablePlaintext + 'X')) {
    throw 'Backspace did not edit the protected draft exactly'
  }
  [Windows.Forms.SendKeys]::SendWait('{HOME}{DELETE}')
  if ((Get-Value $draft) -cne $disposablePlaintext.Substring(1)) {
    throw 'Delete did not edit the protected draft exactly'
  }
  Set-Value $draft $disposablePlaintext

  $swapResults = @()
  for ($swap = 1; $swap -le 3; $swap += 1) {
    Set-AcceptanceStage "draft-swap-$swap-unlock"
    $off = Set-ComposerLock $false "draft-swap-$swap-unlock"
    Set-AcceptanceStage "draft-swap-$swap-native-resolve-read"
    if ((Get-NativeDiscordDraft $discord) -cne $nativeDraft) {
      throw 'native draft was not restored exactly during lock swap'
    }
    Set-AcceptanceStage "draft-swap-$swap-lock"
    $on = Set-ComposerLock $true "draft-swap-$swap-lock"
    Set-AcceptanceStage "draft-swap-$swap-native-resolve-suspended"
    if ((Get-NativeDiscordDraft $discord).Length -ne 0) {
      throw 'native draft was not suspended exactly during lock swap'
    }
    Set-AcceptanceStage "draft-swap-$swap-protected-read"
    $freshOsl = Get-ExactOsl
    $freshDraft = Resolve-Control $freshOsl.OverlayRoot ([Windows.Automation.ControlType]::Edit) `
      'protected-draft'
    if ((Get-Value $freshDraft) -cne $disposablePlaintext) {
      throw 'protected draft was not retained exactly during lock swap'
    }
    $swapResults += [pscustomobject]@{
      Iteration = $swap
      UnlockMs = $off.ToggleMs
      LockMs = $on.ToggleMs
      OverlayReused = $off.OverlayReused -and $on.OverlayReused
      NativeDraftRestored = $true
      ProtectedDraftRestored = $true
    }
  }
  $osl = Get-ExactOsl
  $draft = Resolve-Control $osl.OverlayRoot ([Windows.Automation.ControlType]::Edit) `
    'protected-draft'

  Set-AcceptanceStage 'single-enter-send'
  if (-not [OslLocalDiscordVisualNative]::SetForegroundWindow($osl.OverlayHwnd)) {
    throw 'exact protected composer could not receive the one Enter gesture'
  }
  $draft.SetFocus()
  $enterIssuedUnixMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
  $sendStarted = [Diagnostics.Stopwatch]::StartNew()
  [Windows.Forms.SendKeys]::SendWait('{ENTER}')
  $postSend = Wait-For {
    $freshOsl = Get-ExactOsl
    $freshDraft = Resolve-Control $freshOsl.OverlayRoot ([Windows.Automation.ControlType]::Edit) `
      'protected-draft' $false
    if ((Get-Value $freshDraft).Length -ne 0) { return $null }
    if (-not (Get-SentMarked $freshOsl.OverlayRoot)) { return $null }
    $rendered = @(Get-VisibleExactTextElements $freshOsl.OverlayRoot $disposablePlaintext)
    if ($rendered.Count -ne 1) {
      return $null
    }
    $renderedBounds = $rendered[0].Current.BoundingRectangle
    $renderedRow = [pscustomobject]@{
      Left = [int]$renderedBounds.Left
      Top = [int]$renderedBounds.Top
      Width = [int]$renderedBounds.Width
      Height = [int]$renderedBounds.Height
    }
    $discordRect = Get-Rect $discord.Hwnd
    if ($renderedRow.Width -lt 1 -or $renderedRow.Height -lt 12 -or
        -not (Test-Contained $renderedRow $discordRect)) {
      return $null
    }
    [pscustomobject]@{
      EnterIssuedUnixMs = $enterIssuedUnixMs
      ConfirmedUnixMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
      EnterToFlagAndRenderMs = [int][Math]::Ceiling($sendStarted.Elapsed.TotalMilliseconds)
      ComposerEmpty = $true
      ShapeMatchedFlagProven = $true
      PlaintextRenderedOverFlag = $true
      RenderedRow = $renderedRow
    }
  } 'single-enter-send-timeout'
  $sendStarted.Stop()

  if ($originalForeground -ne [IntPtr]::Zero) {
    [void][OslLocalDiscordVisualNative]::SetForegroundWindow($originalForeground)
  }
  Set-AcceptanceStage 'eye-on'
  $eyeOn = Set-Eye $osl $true $lockProtected
  Expose-ForScreenshot (Get-ExactOsl) $originalForeground
  $eyeOnShot = Capture-Window $osl.MainHwnd 'eye-on.png'
  Set-AcceptanceStage 'eye-off'
  $eyeOff = Set-Eye $osl $false $lockProtected
  Expose-ForScreenshot (Get-ExactOsl) $originalForeground
  $eyeOffShot = Capture-Window $osl.MainHwnd 'eye-off.png'
  $eyeRestored = Set-Eye $osl $true $lockProtected

  Set-AcceptanceStage 'geometry-transitions'
  $geometry = @($initialGeometry)
  $normalFlags = [uint32](0x0010 -bor 0x0004)
  $moveWidth = [Math]::Max(900, [Math]::Min(1220, $originalRect.Width - 40))
  $moveHeight = [Math]::Max(650, [Math]::Min(900, $originalRect.Height - 30))
  if (-not [OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 9)) {
    throw 'OSL restore before move was rejected'
  }
  if (-not [OslLocalDiscordVisualNative]::SetWindowPos(
    $osl.MainHwnd,
    [IntPtr]::Zero,
    [Math]::Max(0, $originalRect.Left + 18),
    [Math]::Max(0, $originalRect.Top + 18),
    $moveWidth,
    $moveHeight,
    $normalFlags
  )) {
    throw 'no-activate OSL move and resize was rejected'
  }
  $geometry += Wait-GeometryState $discord 'moveResize'

  if (-not [OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 3)) {
    throw 'no-activate OSL maximize was rejected'
  }
  $geometry += Wait-GeometryState $discord 'maximized'
  Expose-ForScreenshot (Get-ExactOsl) $originalForeground
  $maximizedShot = Capture-Window $osl.MainHwnd 'maximized.png'

  if (-not [OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 6)) {
    throw 'no-activate OSL minimize was rejected'
  }
  $minimized = Wait-For {
    $fresh = Get-ExactOsl $false
    if (-not [OslLocalDiscordVisualNative]::IsIconic($fresh.MainHwnd) -or
        [OslLocalDiscordVisualNative]::IsWindowVisible($fresh.OverlayHwnd) -or
        [OslLocalDiscordVisualNative]::GetWindow($discord.Hwnd, 4) -ne $fresh.MainHwnd) {
      return $null
    }
    [pscustomobject]@{
      MainMinimized = $true
      OverlayHidden = $true
      DiscordParentExact = $true
    }
  } 'minimize-tether-timeout'

  if (-not [OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 9)) {
    throw 'no-activate OSL restore was rejected'
  }
  if (-not [OslLocalDiscordVisualNative]::SetWindowPos(
    $osl.MainHwnd,
    [IntPtr]::Zero,
    $originalRect.Left,
    $originalRect.Top,
    $originalRect.Width,
    $originalRect.Height,
    $normalFlags
  )) {
    throw 'original OSL geometry restoration was rejected'
  }
  if ($originalMaximized) {
    [void][OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 3)
  }
  $geometry += Wait-GeometryState $discord 'restored'
  Expose-ForScreenshot (Get-ExactOsl) $originalForeground
  $restoredShot = Capture-Window $osl.MainHwnd 'restored.png'

  $tetherPassed = @($geometry | Where-Object {
    -not $_.DiscordParentExact -or
      -not $_.DiscordContained -or
      -not $_.OverlayContained -or
      -not $_.ForegroundUnchanged
  }).Count -eq 0 -and $minimized.OverlayHidden -and $minimized.DiscordParentExact
  Set-AcceptanceStage 'generated-draft-cleanup'
  $cleanupOsl = Get-ExactOsl
  $cleanupDraft = Resolve-Control $cleanupOsl.OverlayRoot `
    ([Windows.Automation.ControlType]::Edit) 'protected-draft'
  if ((Get-Value $cleanupDraft).Length -ne 0) {
    throw 'generated protected draft was not cleared after send'
  }
  $cleanupUnlock = Set-ComposerLock $false 'cleanup-unlock'
  Set-AcceptanceStage 'cleanup-native-resolve-read'
  if ((Get-NativeDiscordDraft $discord) -cne $nativeDraft) {
    throw [OslQaHarnessException]::new('native-composer-final-draft-mismatch')
  }
  $cleanupLock = $null
  if ($preexistingNativeDraftDetected) {
    $preexistingNativeDraftPreserved = $true
    Set-AcceptanceStage 'cleanup-preexisting-native-preserved'
  } else {
    # Real input can destroy a draft, so the clear is refused unless this run
    # is the one that generated the exact draft currently in the composer.
    if (-not $disposableNativeDraftSeeded -or $nativeDraft -cne $generatedNativeDraft) {
      throw [OslQaHarnessException]::new('native-composer-clear-target-rejected')
    }
    Set-AcceptanceStage 'cleanup-native-resolve-clear'
    Clear-NativeDiscordDraft $discord $nativeDraft
    Set-AcceptanceStage 'cleanup-native-verify-clear'
    if ((Get-NativeDiscordDraft $discord).Length -ne 0) {
      throw [OslQaHarnessException]::new('native-composer-disposable-clear-rejected')
    }
    $disposableNativeDraftCleaned = $true
    Set-AcceptanceStage 'cleanup-protected-lock'
    $cleanupLock = Set-ComposerLock $true 'cleanup-protected-lock'
  }
  $receipt = [ordered]@{
    Schema = 1
    Status = if ($tetherPassed) { 'passed' } else { 'tether-failed' }
    ExecutableSha256 = $actualSha
    DisposablePlaintextSha256 = $plaintextSha256
    Process = @{
      OslProcessId = $osl.Process.Id
      DiscordProcessId = $discord.ProcessId
      DiscordProcessCount = $discord.ProcessIds.Count
    }
    DraftEditing = @{
      Backspace = $true
      Delete = $true
    }
    DraftSwaps = @{
      InitialUnlockMs = $initialUnlock.ToggleMs
      InitialLockMs = $initialLock.ToggleMs
      Iterations = $swapResults
      AllOverlayReused = @($swapResults | Where-Object { -not $_.OverlayReused }).Count -eq 0
      PreexistingNativeDraftDetected = $preexistingNativeDraftDetected
      PreexistingNativeDraftPreserved = $preexistingNativeDraftPreserved
      DisposableNativeDraftSeeded = $disposableNativeDraftSeeded
      DisposableNativeDraftCleaned = $disposableNativeDraftCleaned
      CleanupUnlockMs = $cleanupUnlock.ToggleMs
      CleanupLockMs = if ($null -ne $cleanupLock) { $cleanupLock.ToggleMs } else { $null }
    }
    Send = $postSend
    Eye = @{
      On = $eyeOn
      Off = $eyeOff
      Restored = $eyeRestored
      LockUnchanged = $eyeOn.LockProtected -and $eyeOff.LockProtected -and
        $eyeRestored.LockProtected
    }
    Screenshots = @($eyeOnShot, $eyeOffShot, $maximizedShot, $restoredShot)
    Geometry = $geometry
    Minimize = $minimized
    ForegroundRestored = $false
  }
  Set-AcceptanceStage 'passed'
} catch {
  $safeFailure = Get-SafeInnerFailureClass $_
  $receiptFailure = if (
    $safeFailure.StartsWith('native-composer-', [StringComparison]::Ordinal) -or
    $safeFailure.StartsWith('osl-', [StringComparison]::Ordinal)
  ) {
    $safeFailure
  } else {
    'harness-' + $safeFailure
  }
  $receipt = [ordered]@{
    Schema = 1
    Status = 'failed-closed'
    ExecutableSha256 = $actualSha
    DisposablePlaintextSha256 = $plaintextSha256
    ErrorStage = $acceptanceStage
    ErrorClass = $receiptFailure
    ForegroundRestored = $false
  }
} finally {
  if ($null -ne $osl -and $null -ne $originalRect -and
      [OslLocalDiscordVisualNative]::IsWindow($osl.MainHwnd)) {
    $restoreFlags = [uint32](0x0010 -bor 0x0004)
    [void][OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 9)
    [void][OslLocalDiscordVisualNative]::SetWindowPos(
      $osl.MainHwnd,
      [IntPtr]::Zero,
      $originalRect.Left,
      $originalRect.Top,
      $originalRect.Width,
      $originalRect.Height,
      $restoreFlags
    )
    if ($originalMaximized) {
      [void][OslLocalDiscordVisualNative]::ShowWindowAsync($osl.MainHwnd, 3)
    }
  }
  $foregroundRestored = if ($originalForeground -ne [IntPtr]::Zero) {
    [OslLocalDiscordVisualNative]::SetForegroundWindow($originalForeground)
  } else {
    $true
  }
  if ($null -ne $receipt) { $receipt.ForegroundRestored = $foregroundRestored }
  $disposablePlaintext = $null
  $generatedNativeDraft = $null
  $nativeDraft = $null
}

$json = $receipt | ConvertTo-Json -Depth 8
[IO.File]::WriteAllText($receiptPath, $json, [Text.UTF8Encoding]::new($false))
$json
if ($receipt.Status -cne 'passed') { exit 1 }
