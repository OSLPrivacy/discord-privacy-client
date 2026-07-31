from pathlib import Path


SCRIPT = Path(__file__).with_name("osl-local-discord-visual-latency.ps1").read_text(
    encoding="utf-8"
)


def test_disposable_plaintext_never_enters_arguments_or_receipt_errors() -> None:
    assert "PreparedPlaintext" not in SCRIPT
    assert "DisposablePlaintextSha256" in SCRIPT
    assert "ErrorDetail" not in SCRIPT
    assert "$disposablePlaintext = 'OSL visual QA ' + [Guid]::NewGuid()" in SCRIPT


def test_acceptance_covers_editing_three_reused_swaps_and_single_enter() -> None:
    assert "SendWait('{END}{BACKSPACE}')" in SCRIPT
    assert "SendWait('{HOME}{DELETE}')" in SCRIPT
    assert "for ($swap = 1; $swap -le 3; $swap += 1)" in SCRIPT
    assert "protected composer WebView was recreated during lock swap" in SCRIPT
    assert "NativeDraftRestored = $true" in SCRIPT
    assert "ProtectedDraftRestored = $true" in SCRIPT
    assert "SendWait('{ENTER}')" in SCRIPT


def _block(header: str) -> str:
    """The body of a brace-balanced C# member starting at ``header``."""
    start = SCRIPT.index(header)
    depth = 0
    for index in range(SCRIPT.index("{", start), len(SCRIPT)):
        if SCRIPT[index] == "{":
            depth += 1
        elif SCRIPT[index] == "}":
            depth -= 1
            if depth == 0:
                return SCRIPT[start : index + 1]
    raise AssertionError("unbalanced block: " + header)


def test_native_composer_uses_bounded_msaa_without_discord_tree_walk() -> None:
    assert "Resolve-NativeDiscordComposer" not in SCRIPT
    assert "$Discord.Root.FindAll" not in SCRIPT
    assert "AccessibleObjectFromPoint" in SCRIPT
    assert "ReadComposerValue" in SCRIPT
    assert "TypeComposerValue" in SCRIPT
    assert "ClearComposerValue" in SCRIPT
    assert "foreach (var xPercent in new[] { 40, 45, 50, 55 })" in SCRIPT
    assert "foreach (var bottomOffset in new[] { 28, 37, 48 })" in SCRIPT
    assert "depth < 12" in SCRIPT
    assert "worker.Join(timeoutMs)" in SCRIPT
    assert (
        "ReadNormalizedComposerValue(composer), expectedCurrent, StringComparison.Ordinal"
        in SCRIPT
    )
    assert "ReadNormalizedComposerValue" in SCRIPT
    assert "String.Equals(value, name, StringComparison.Ordinal)" in SCRIPT
    assert "native-composer-not-found" in SCRIPT
    assert "native-composer-ambiguous" in SCRIPT
    assert "$initialNativeDraft = Get-NativeDiscordDraft $discord" in SCRIPT
    assert "$nativeDraft = $initialNativeDraft" in SCRIPT
    assert "native-composer-preexisting-draft-memory-only" in SCRIPT
    assert "cleanup-preexisting-native-preserved" in SCRIPT
    assert "PreexistingNativeDraftDetected = $preexistingNativeDraftDetected" in SCRIPT
    assert "PreexistingNativeDraftPreserved = $preexistingNativeDraftPreserved" in SCRIPT
    assert "NativeDraftSha" not in SCRIPT

    preexisting_branch = SCRIPT.split(
        "if ($initialNativeDraft.Length -gt 0) {", maxsplit=1
    )[1].split("} else {", maxsplit=1)[0]
    preserved_cleanup = SCRIPT.split(
        "if ($preexistingNativeDraftDetected) {", maxsplit=1
    )[1].split("} else {", maxsplit=1)[0]
    # A pre-existing operator draft is read into memory and never written,
    # typed into, selected or cleared -- now doubly load bearing, because the
    # harness seeds with real keystrokes rather than a reverted MSAA write.
    for branch in (preexisting_branch, preserved_cleanup):
        for mutator in (
            "Set-NativeDiscordDraft",
            "Clear-NativeDiscordDraft",
            "TypeComposerValue",
            "ClearComposerValue",
            "SendInput",
            "SendKeys",
            "set_accValue(",
        ):
            assert mutator not in branch


def test_native_composer_seeds_with_real_unicode_typing_not_msaa_writes() -> None:
    # Slate.js reverts MSAA set_accValue, so the seed must go through the same
    # synthetic-input path the product's send path uses.
    assert "set_accValue(" not in SCRIPT
    assert "KEYEVENTF_UNICODE" in SCRIPT
    assert "private static bool SendUnicodeChunk(char[] units)" in SCRIPT
    assert "KeyboardInputFor(unit, KEYEVENTF_UNICODE)" in SCRIPT
    assert "KeyboardInputFor(unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)" in SCRIPT

    typing = _block("private static bool TypeUnicodeText(string text)")
    assert "SendUnicodeChunk" in typing
    assert "SendShiftEnter" in typing

    seed = _block(
        "public static bool TypeComposerValue("
    )
    assert "TypeUnicodeText(next)" in seed
    assert "FocusExactComposer(hwnd, composer, timer, timeoutMs)" in seed
    # The pre-write compare survives, and is re-proven after taking focus.
    assert seed.count(
        "ReadNormalizedComposerValue(composer), expectedCurrent, StringComparison.Ordinal"
    ) == 2
    assert "AwaitComposerValue(composer, next, timer, VerifyDeadlineMs(timeoutMs))" in seed
    assert "native-composer-write-timeout" in seed

    # A bare Enter would send a message. The only Enter the native type ever
    # emits is wrapped in Shift, and the old bare-Enter helper is gone.
    assert "SendPhysicalEnter" not in SCRIPT
    shift_enter = _block("private static bool SendShiftEnter()")
    assert "KeyboardInputFor(SHIFT_SCAN_CODE, KEYEVENTF_SCANCODE)" in shift_enter
    assert (
        "KeyboardInputFor(SHIFT_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)"
        in shift_enter
    )
    assert SCRIPT.count("ENTER_SCAN_CODE") == 1 + shift_enter.count("ENTER_SCAN_CODE")
    for enter_free in (typing, seed, _block("public static bool ClearComposerValue(")):
        assert "ENTER_SCAN_CODE" not in enter_free


def test_native_composer_takes_exact_focus_and_restores_it() -> None:
    focus = _block("private static void FocusExactComposer(")
    assert "SetForegroundWindow(hwnd)" in focus
    assert "while (GetForegroundWindow() != hwnd)" in focus
    assert "native-composer-foreground-rejected" in focus
    assert "STATE_SYSTEM_FOCUSED" in focus
    assert "composer.accSelect(SELFLAG_TAKEFOCUS, 0)" in focus
    assert "native-composer-focus-rejected" in focus

    restore = _block("private static void RestoreForeground(")
    assert "SetForegroundWindow(previous)" in restore
    for caller in (
        _block("public static bool TypeComposerValue("),
        _block("public static bool ClearComposerValue("),
    ):
        assert "var restoreTo = GetForegroundWindow();" in caller
        assert "RestoreForeground(restoreTo, hwnd);" in caller


def test_disposable_native_draft_is_cleared_with_real_input_and_verified() -> None:
    clear = _block("public static bool ClearComposerValue(")
    assert "SendSelectAllThenDelete()" in clear
    assert "expectedCurrent.Length == 0" in clear
    assert "native-composer-clear-target-rejected" in clear
    assert (
        "AwaitComposerValue(\n          composer, String.Empty, timer, "
        "VerifyDeadlineMs(timeoutMs)\n        )" in clear
    )

    select_all = _block("private static bool SendSelectAllThenDelete()")
    assert "KeyboardInputFor(CONTROL_SCAN_CODE, KEYEVENTF_SCANCODE)" in select_all
    assert "KeyboardInputFor(A_SCAN_CODE, KEYEVENTF_SCANCODE)" in select_all
    assert (
        "KeyboardInputFor(CONTROL_SCAN_CODE, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)"
        in select_all
    )
    assert "KeyboardInputFor(BACKSPACE_SCAN_CODE, KEYEVENTF_SCANCODE)" in select_all

    assert "function Clear-NativeDiscordDraft(" in SCRIPT
    assert "Clear-NativeDiscordDraft $discord $nativeDraft" in SCRIPT
    assert "Set-AcceptanceStage 'cleanup-native-verify-clear'" in SCRIPT
    assert "native-composer-disposable-clear-rejected" in SCRIPT
    # The clear is refused unless this run generated the exact draft present.
    assert (
        "if (-not $disposableNativeDraftSeeded -or $nativeDraft -cne $generatedNativeDraft) {"
        in SCRIPT
    )


def test_progress_file_exposes_fixed_stage_without_plaintext() -> None:
    assert "acceptance-stage.txt" in SCRIPT
    assert "function Set-AcceptanceStage([string]$Stage)" in SCRIPT
    assert "WriteAllText($stagePath, $Stage" in SCRIPT
    assert "Set-AcceptanceStage 'initial-native-unlock'" in SCRIPT
    assert "$osl = Get-ExactOsl $false" in SCRIPT
    assert "Set-ComposerLock $true 'bootstrap-protected-lock'" in SCRIPT
    assert "Set-AcceptanceStage 'bootstrap-visible-overlay'" in SCRIPT
    assert "osl-overlay-window-unavailable-or-ambiguous" in SCRIPT
    assert "osl-overlay-not-visible" in SCRIPT
    assert 'Set-AcceptanceStage "$StagePrefix-toggle"' in SCRIPT
    assert 'Set-AcceptanceStage "$StagePrefix-wait"' in SCRIPT
    assert "function Invoke-Toggle" in SCRIPT
    assert "$pattern.Toggle()" in SCRIPT
    assert "Invoke-Exact $lock" not in SCRIPT
    assert 'Set-AcceptanceStage "draft-swap-$swap-native-resolve-read"' in SCRIPT
    assert "native-composer-resolve-write-seed" in SCRIPT
    assert "native-composer-read-" in SCRIPT
    assert "native-composer-write-" in SCRIPT
    assert "return $exception.FailureClass" in SCRIPT
    assert "ErrorDetail" not in SCRIPT
    assert "WriteAllText($stagePath, $disposablePlaintext" not in SCRIPT
    assert "WriteAllText($stagePath, $nativeDraft" not in SCRIPT


def test_acceptance_covers_eye_geometry_minimize_and_screenshots() -> None:
    assert "Set-Eye $osl $true $lockProtected" in SCRIPT
    assert "Set-Eye $osl $false $lockProtected" in SCRIPT
    assert "ShowWindowAsync($osl.MainHwnd, 3)" in SCRIPT
    assert "ShowWindowAsync($osl.MainHwnd, 6)" in SCRIPT
    assert "SetWindowPos(" in SCRIPT
    for screenshot in (
        "eye-on.png",
        "eye-off.png",
        "maximized.png",
        "restored.png",
    ):
        assert screenshot in SCRIPT
