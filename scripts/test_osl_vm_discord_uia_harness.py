from __future__ import annotations

import re
import unittest
from pathlib import Path


HARNESS = Path(__file__).parent / "qa" / "osl-vm-discord-uia-harness.ps1"
UNLOCK = Path(__file__).parent / "qa" / "osl-vm-unlock-from-key-vault.ps1"

REQUIRED_ACTIONS = (
    "Inventory",
    "Inspect",
    "OpenCurrent",
    "PrepareOverlay",
    "OpenVerifiedFriend",
    "SetDeterministic",
    "Send",
    "InspectInbound",
    "ToggleVisibility",
    "ExerciseWindowLifecycle",
)


def function_body(source: str, name: str) -> str:
    match = re.search(
        rf"(?ims)^\s*function\s+{re.escape(name)}(?:\s*\(.*?\))?\s*\{{",
        source,
    )
    if match is None:
        raise AssertionError(f"missing PowerShell function {name}")
    return braced_body(source, match.end() - 1)


def action_body(source: str, action: str) -> str:
    match = re.search(
        rf"(?im)^\s*['\"]?{re.escape(action)}['\"]?\s*\{{", source
    )
    if match is None:
        raise AssertionError(f"missing switch arm for action {action}")
    return braced_body(source, match.end() - 1)


def braced_body(source: str, opening_brace: int) -> str:
    depth = 0
    quote: str | None = None
    index = opening_brace
    while index < len(source):
        character = source[index]
        if quote is not None:
            if character == "`":
                index += 2
                continue
            if character == quote:
                quote = None
        elif character in "'\"":
            quote = character
        elif character == "{":
            depth += 1
        elif character == "}":
            depth -= 1
            if depth == 0:
                return source[opening_brace + 1 : index]
        index += 1
    raise AssertionError("unbalanced PowerShell braces")


class OslVmDiscordUiaHarnessStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = HARNESS.read_text(encoding="utf-8")
        cls.unlock = UNLOCK.read_text(encoding="utf-8")

    def test_exposes_only_the_required_semantic_actions(self) -> None:
        validate_set = re.search(
            r"(?is)\[\s*ValidateSet\s*\((.*?)\)\s*\]\s*\[\s*string\s*\]\s*\$Action\b",
            self.source,
        )
        self.assertIsNotNone(validate_set, "Action must have an explicit ValidateSet")
        actions = re.findall(r"['\"]([A-Za-z][A-Za-z0-9]*)['\"]", validate_set.group(1))
        self.assertEqual(list(REQUIRED_ACTIONS), actions)
        for action in REQUIRED_ACTIONS:
            action_body(self.source, action)

    def test_inventory_is_capped_to_visible_button_metadata(self) -> None:
        inventory = action_body(self.source, "Inventory")
        self.assertRegex(inventory, r"ControlType\]::Button")
        self.assertRegex(inventory, r"Select-Object\s+-First\s+48")
        self.assertRegex(inventory, r"IsOffscreen")
        self.assertNotRegex(inventory, r"ValuePattern|ControlType\]::(?:Edit|Text|Document)")

    def test_requires_exact_osl_identity_and_interactive_session_parameters(self) -> None:
        for name, kind in (
            ("OslExePath", "string"),
            ("OslExeSha256", "string"),
            ("SessionId", "int"),
        ):
            self.assertRegex(
                self.source,
                rf"(?is)\[\s*Parameter\s*\(\s*Mandatory\s*=\s*\$true\s*\)\s*\]"
                rf"\s*\[\s*{kind}\s*\]\s*\${name}\b",
            )
        self.assertRegex(self.source, r"(?i)Get-FileHash\b[^\r\n]*-Algorithm\s+SHA256")
        self.assertRegex(self.source, r"(?i)\$OslExeSha256\b")
        self.assertRegex(self.source, r"(?i)ProcessIdToSessionId|\.SessionId\b")
        self.assertRegex(self.source, r"(?i)\$SessionId\b")

        identity = function_body(self.source, "Get-ExactOslProcess")
        self.assertRegex(identity, r"VisibleTopLevelWindowsFor")
        self.assertRegex(identity, r"WindowText")
        self.assertRegex(identity, r"OSL Privacy")
        self.assertRegex(identity, r"WindowClass")
        self.assertRegex(identity, r"Tauri Window")

    def test_never_uses_coordinates_mouse_or_synthetic_pointer_input(self) -> None:
        forbidden = (
            r"System\.Windows\.Forms\.Cursor",
            r"\bSetCursorPos\b",
            r"\bGetCursorPos\b",
            r"\bmouse_event\b",
            r"\bSendInput\b",
            r"\bMOUSEINPUT\b",
            r"\bMoveTo\b",
            r"\bClick\s*\(",
            r"\bpyautogui\b",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.source, rf"(?i){pattern}")

    def test_each_action_rediscovers_live_windows_and_elements(self) -> None:
        for action in REQUIRED_ACTIONS:
            body = action_body(self.source, action)
            self.assertRegex(
                body,
                r"(?i)\b(?:Find|Discover|Resolve|Get-Fresh)-?[A-Za-z0-9-]+",
                f"{action} must start from fresh UI discovery",
            )

    def test_does_not_cache_uia_or_hwnd_objects_across_semantic_waits(self) -> None:
        self.assertNotRegex(
            self.source,
            r"(?im)^\s*\$(?:script|global):[A-Za-z0-9_]*(?:uia|element|window|hwnd|handle)",
        )
        wait = function_body(self.source, "Wait-SemanticPostcondition")
        self.assertRegex(wait, r"(?i)&\s*\$Rediscover|\.Invoke\s*\(")
        self.assertNotRegex(
            wait,
            r"(?im)^\s*param\s*\([^)]*\$(?:Element|Window|Hwnd|Handle)\b",
        )

    def test_visible_discord_renderer_ambiguity_is_rejected(self) -> None:
        body = function_body(self.source, "Find-VisibleDiscordRenderer")
        self.assertRegex(body, r"(?i)(?:IsOffscreen|IsVisible|BoundingRectangle)")
        self.assertRegex(body, r"(?i)\.Count\s*-ne\s*1|\.Count\s*-eq\s*0[\s\S]*\.Count\s*-gt\s*1")
        self.assertRegex(body, r"(?i)throw\b")

    def test_foreground_and_keyboard_focus_are_verified_not_assumed(self) -> None:
        body = function_body(self.source, "Assert-ForegroundAndFocus")
        self.assertRegex(body, r"(?i)SetForegroundWindow|SetFocus|\.SetFocus\s*\(")
        self.assertRegex(body, r"(?i)GetForegroundWindow")
        self.assertRegex(body, r"(?i)HasKeyboardFocus|GetFocusedElement")
        self.assertRegex(body, r"(?i)throw\b")

    def test_fullscreen_uses_the_exact_route_independent_titlebar_control(self) -> None:
        body = function_body(self.source, "Invoke-FullscreenFresh")
        self.assertRegex(body, r"(?i)Invoke-FreshControl")
        self.assertRegex(body, r"(?i)window-fullscreen")
        self.assertNotRegex(body, r"(?i)SendKeys|PressF11|window-minimize")

        exit_body = function_body(self.source, "Exit-FullscreenFresh")
        self.assertRegex(exit_body, r"(?i)Get-FreshMainSurface")
        self.assertRegex(exit_body, r"(?i)SetForegroundWindow")
        self.assertRegex(exit_body, r"(?i)GetForegroundWindow")
        self.assertRegex(exit_body, r"(?i)PressF11Keyboard")

    def test_fullscreen_state_uses_exact_window_bounds_not_titlebar_visibility(self) -> None:
        body = function_body(self.source, "Get-WindowLifecycleState")
        self.assertRegex(body, r"(?i)Screen\]::FromHandle")
        self.assertRegex(body, r"(?i)BoundingRectangle")
        self.assertRegex(body, r"(?i)ExpectFullscreen")
        self.assertNotRegex(body, r"(?i)TitlebarCount")

    def test_element_not_available_retry_is_bounded_and_rediscovers(self) -> None:
        body = function_body(self.source, "Invoke-WithElementNotAvailableRetry")
        self.assertRegex(body, r"(?i)ElementNotAvailableException")
        self.assertRegex(body, r"(?i)(?:Deadline|Timeout|MaxAttempts|AttemptLimit)")
        self.assertRegex(body, r"(?i)&\s*\$Rediscover|\.Invoke\s*\(")
        self.assertRegex(body, r"(?i)throw\b")

    def test_mutations_wait_for_semantic_postconditions(self) -> None:
        for action in (
            "OpenCurrent",
            "PrepareOverlay",
            "OpenVerifiedFriend",
            "SetDeterministic",
            "Send",
            "ToggleVisibility",
            "ExerciseWindowLifecycle",
        ):
            self.assertRegex(
                action_body(self.source, action),
                r"(?i)Wait-SemanticPostcondition\b",
                f"{action} must prove its semantic postcondition",
            )

    def test_send_does_not_overstate_discord_integration_proof(self) -> None:
        body = action_body(self.source, "Send")
        self.assertRegex(body, r"DiscordCarrierProven")
        self.assertRegex(body, r"sentDiscordMarked")
        self.assertRegex(body, r"FullDiscordProofPassed\s*=\s*\$false")
        self.assertRegex(body, r"ProtectedSendSucceeded")

    def test_does_not_overwrite_powershell_reserved_host_variable(self) -> None:
        self.assertNotRegex(self.source, r"(?im)^\s*\$host\s*=")

    def test_inbound_and_visibility_count_only_visible_plaintext(self) -> None:
        snapshot = function_body(self.source, "Get-ExactPlaintextSnapshot")
        self.assertRegex(snapshot, r"Current\.IsOffscreen")
        self.assertRegex(snapshot, r"Current\.BoundingRectangle\.IsEmpty")
        toggle = action_body(self.source, "ToggleVisibility")
        self.assertRegex(toggle, r"if\s*\(\$Visibility\s*-eq\s*'On'\)")
        self.assertRegex(toggle, r"ExactPlaintextCount\s*-eq\s*0")

    def test_vm_navigation_uses_stable_automation_ids(self) -> None:
        for automation_id in (
            "skip-onboarding",
            "skip-scrub-setup",
            "home-app-discord",
            "discord-existing-session",
            "local-protected-toggle",
            "native-protect-verified-peer",
            "protected-draft",
            "prepare-protected",
            "protected-decrypt-display",
        ):
            self.assertIn(f"'{automation_id}'", self.source)
        for label in (
            "@('OSL Privacy home')",
            "@('Discord, OSL profile ready')",
            "@('Use existing account')",
            "@('Protect')",
            "@('Verified friend Verified')",
        ):
            self.assertNotIn(label, self.source)

    def test_main_osl_identity_explicitly_excludes_guardian_mode(self) -> None:
        identity = function_body(self.source, "Get-ExactOslProcess")
        self.assertIn("--osl-borrowed-window-guardian-v1", identity)
        self.assertRegex(identity, r"(?i)CommandLine\s+-cnotmatch")
        self.assertRegex(identity, r"GuardianProcessIds")
        self.assertIn("--osl-borrowed-window-guardian-v1", self.unlock)
        self.assertRegex(self.unlock, r"(?i)CommandLine\s+-cnotmatch")

    def test_unlock_waits_for_a_stable_ready_route_not_only_form_removal(self) -> None:
        self.assertIn("stableReadySamples", self.unlock)
        self.assertIn("ReadyAutomationId", self.unlock)
        self.assertIn("unlock transition did not reach a stable ready route", self.unlock)
        for automation_id in (
            "skip-onboarding",
            "skip-scrub-setup",
            "home-app-discord",
            "discord-existing-session",
            "native-companion-focus",
        ):
            self.assertIn(f"'{automation_id}'", self.unlock)

    def test_lifecycle_pins_discord_pid_set_hwnd_and_containment(self) -> None:
        identity = function_body(self.source, "Get-InitialDiscordHostIdentity")
        self.assertRegex(identity, r"(?i)DiscordProcessKeys")
        self.assertRegex(identity, r"(?i)DiscordHwnd")
        self.assertRegex(identity, r"(?i)Get-AuthenticodeSignature")
        self.assertRegex(identity, r"(?i)Select-Object\s+-First\s+10|Count\s+-gt\s+16")

        continuity = function_body(self.source, "Get-DiscordContinuityState")
        self.assertRegex(continuity, r"(?i)PidSetStable")
        self.assertRegex(continuity, r"(?i)GetParent")
        self.assertRegex(continuity, r"(?i)IsBranchAbove")
        self.assertRegex(continuity, r"(?i)IsRetethered")
        self.assertRegex(continuity, r"(?i)GeometryRetethered")
        self.assertRegex(continuity, r"(?i)StackedAboveOslBackground")

    def test_lifecycle_proves_automatic_recovery_without_bring_forward(self) -> None:
        lifecycle = action_body(self.source, "ExerciseWindowLifecycle")
        self.assertRegex(lifecycle, r"(?i)Assert-ForegroundAndFocus")
        self.assertRegex(lifecycle, r"(?i)Invoke-WindowTransformFresh")
        self.assertRegex(lifecycle, r"(?i)ShowWindowAsync")
        self.assertRegex(lifecycle, r"(?i)Send-SessionRecoverySequence")
        self.assertRegex(lifecycle, r"(?i)AutoRetethered")
        self.assertRegex(lifecycle, r"ManualBringForwardInvoked\s*=\s*\$false")
        self.assertNotIn("'native-companion-focus'", lifecycle)
        self.assertRegex(lifecycle, r"Select-Object\s+-First\s+10")


if __name__ == "__main__":
    unittest.main()
