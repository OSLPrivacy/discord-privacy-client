from __future__ import annotations

import ast
import contextlib
import importlib.util
import io
import json
import re
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch
from pathlib import Path

ROOT = Path(__file__).resolve().parent


class WhatsAppUiaProbeStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.arm = (ROOT / "arm-whatsapp-uia-probe.ps1").read_text(encoding="utf-8")
        cls.poll = (ROOT / "poll-whatsapp-uia-probe.ps1").read_text(encoding="utf-8")
        cls.orchestrator = (ROOT / "whatsapp-uia-probe-orchestrator.py").read_text(encoding="utf-8")

    def test_exact_official_package_process_window_and_session(self) -> None:
        for fixed in (
            "5319275A.WhatsAppDesktop_cv1g1gvanyjgm",
            "$processName = 'WhatsApp.Root.exe'",
            "$windowClass = 'WinUIDesktopWin32WindowClass'",
            "$windowTitle = 'WhatsApp'",
            "PackageTypeFilter Main",
            "PublisherId -ceq $publisherId",
            '"$programFiles\\WindowsApps"',
            "processes.Count -ne 1",
            "windows.Count -ne 1",
            "creationIdentity",
            "postProcesses.Count -ne 1",
            "postWindows.Count -ne 1",
            "owner.User -ceq 'osltest'",
            "sessions.Count -ne 1",
        ):
            self.assertIn(fixed, self.arm)
        self.assertIn("RunLevel Limited", self.arm)
        self.assertNotIn("RunLevel Highest", self.arm)

    def test_probe_reads_only_explicit_structural_properties(self) -> None:
        for allowed in (
            "ControlType.ProgrammaticName", "AutomationId", "ClassName", "FrameworkId",
            "BoundingRectangle", "IsEnabled", "IsOffscreen", "GetRuntimeId",
            "AccessibleObjectFromWindow", "accChildCount", "accRole",
        ):
            self.assertIn(allowed, self.arm)
        forbidden = (
            r"[.]Current[.]Name\b", r"[.]NameProperty\b", r"[.]Value\b", r"ValueProperty",
            r"TextPattern", r"LegacyIAccessiblePattern", r"HelpText", r"SelectionItemPattern",
            r"accName", r"accValue", r"accDescription", r"accHelp", r"accKeyboardShortcut",
            r"InvokePattern", r"RangeValuePattern", r"ScrollItemPattern", r"WindowPattern",
            r"GetClickablePoint", r"GetSupportedPatterns", r"GetSupportedProperties",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.arm, pattern)
        self.assertIn("AutomationElement]::FromPoint", self.arm)
        self.assertIn("ProcessKind=if", self.arm)
        self.assertIn("EnumChildWindows", self.arm)
        self.assertIn("DescendantWindows", self.arm)

    def test_no_input_foreground_storage_or_private_api(self) -> None:
        forbidden = (
            r"SetForegroundWindow", r"ShowWindow", r"SetFocus", r"SendInput", r"mouse_event",
            r"keybd_event", r"SendKeys", r"[.]Click\(",
            r"taskkill", r"Remove-AppxPackage", r"Reset-AppxPackage",
            r"Get-ChildItem[^\n]+WhatsApp", r"AppData[^\n]+WhatsApp", r"LocalState",
            r"SQLite", r"WebSocket", r"https?://", r"Authorization",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.arm, rf"(?i){pattern}")
        for receipt in (
            "ForegroundChanged = $false", "InputInjected = $false", "ProviderStorageRead = $false",
            "ContentPropertiesRead = $false", "ProcessesTerminated = 0",
        ):
            self.assertIn(receipt, self.arm)

    def test_accessibility_hint_is_transient_and_always_restored(self) -> None:
        self.assertIn("SPI_GETSCREENREADER", self.arm.replace("0x0046", "SPI_GETSCREENREADER"))
        self.assertIn("SetScreenReaderHint($originalScreenReaderHint)", self.arm)
        self.assertIn("} finally {", self.arm)
        self.assertIn("AccessibilityHintRestored", self.arm)
        self.assertIn("$result.AccessibilityHintRestored -ne $true", self.poll)

    def test_accessibility_relaunch_is_exact_graceful_and_process_scoped(self) -> None:
        self.assertIn("ValidateSet('observe', 'gracefulRelaunch', 'verifiedRelaunch')", self.arm)
        self.assertIn("RequestGracefulClose", self.arm)
        self.assertIn("--force-renderer-accessibility=complete", self.arm)
        self.assertIn("AdditionalBrowserArguments", self.arm)
        self.assertIn("shell:AppsFolder\\$appUserModelId", self.arm)
        self.assertIn("AccessibilityOverrideRestored", self.arm)
        self.assertIn("AccessibilityFlagObserved", self.arm)
        self.assertIn("msedgewebview2.exe", self.arm)
        self.assertIn("Stop-Process -Id $pidValue -Force", self.arm)
        self.assertIn("CreationDate -ceq $creationIdentity", self.arm)
        self.assertIn("ProcessesTerminated = 0", self.arm)
        self.assertIn("--graceful-relaunch", self.orchestrator)
        self.assertIn("--verified-relaunch", self.orchestrator)
        self.assertIn("GracefulCloseOnly", self.poll)

    def test_output_is_redacted_bounded_and_atomic(self) -> None:
        for cap in ("$maximumNodes = 512", "$maximumDepth = 12", "$maximumOutputBytes = 262144", "$deadlineSeconds = 15"):
            self.assertIn(cap, self.arm)
        self.assertIn("GeometryQ16", self.arm)
        self.assertIn("Get-StructuralHash", self.arm)
        self.assertIn("Get-StructuralToken", self.arm)
        self.assertIn("Get-SafeAutomationId", self.arm)
        self.assertIn("Get-SafeClassName", self.arm)
        self.assertIn("Get-SafeFrameworkId", self.arm)
        self.assertRegex(self.arm, r"AutomationId = Get-SafeAutomationId")
        self.assertRegex(self.arm, r"ClassName = Get-SafeClassName")
        self.assertRegex(self.arm, r"FrameworkId = Get-SafeFrameworkId")
        self.assertIn("[IO.File]::WriteAllText($resultTemporary", self.arm)
        self.assertIn("[IO.File]::Move($resultTemporary, $request.ResultPath)", self.arm)
        self.assertNotRegex(self.arm, r"(?i)(ExceptionMessage|phone number|message content|credential|token value)")
        self.assertNotIn("PID =", self.arm)
        self.assertNotRegex(self.arm, r"(?m)^\s+ProcessId\s*=")

    def test_fail_closed_on_ambiguity_and_tree_change(self) -> None:
        for phrase in (
            "unavailable or ambiguous", "cross-process UIA node rejected",
            "UIA tree changed during bounded capture", "UIA probe deadline exceeded",
            "whatsapp-uia-structure-probe-failed-closed",
        ):
            self.assertIn(phrase, self.arm)
        self.assertIn("Status = 'failedClosed'", self.arm)
        self.assertIn("NodeCount = 0", self.arm)
        self.assertNotIn("[DateTime]::UtcNow - $started).TotalSeconds -ge $deadlineSeconds", self.arm)

    def test_poll_revalidates_read_only_contract(self) -> None:
        for fixed in (
            "NodeCount -gt 512", "ForegroundChanged -ne $false", "InputInjected -ne $false",
            "ProviderStorageRead -ne $false", "ContentPropertiesRead -ne $false",
        ):
            self.assertIn(fixed, self.poll)
        for mutation in ("Start-ScheduledTask", "Register-ScheduledTask", "Remove-Item", "Set-Content", "WriteAllText"):
            self.assertNotIn(mutation, self.poll)

    def test_orchestrator_is_dedicated_pair_scoped_and_not_executed_by_tests(self) -> None:
        ast.parse(self.orchestrator)
        self.assertIn('"OSL-WhatsApp-Client-1"', self.orchestrator)
        self.assertIn('"OSL-WhatsApp-Client-2"', self.orchestrator)
        self.assertIn('"vm", "run-command", "invoke"', self.orchestrator)
        self.assertIn('"probe-whatsapp-msaa-metadata.ps1"', self.orchestrator)
        self.assertIn('"--bench"', self.orchestrator)
        for forbidden in ('"vm", "start"', '"vm", "restart"', '"vm", "deallocate"', "keyvault secret show"):
            self.assertNotIn(forbidden, self.orchestrator)

    def test_orchestrator_runs_uia_then_msaa_metadata_benches_through_one_session_guard(self) -> None:
        module = load_orchestrator()
        calls: list[list[str]] = []
        responses = iter(
            [
                {"Status": "ready", "ProfileRead": False, "ProviderStorageRead": False, "SessionId": 2},
                {"Status": "armed"},
                {"Terminal": True, "Result": {"Status": "capturedStructure"}},
                {
                    "Schema": "whatsapp-msaa-metadata-probe/v1",
                    "TotalNodes": 17,
                    "NamesReturned": False,
                    "ValuesReturned": False,
                    "ProviderContentReturned": False,
                    "InputSent": False,
                    "WindowForegrounded": False,
                    "WhatsAppPrivateStorageRead": False,
                },
            ]
        )

        def fake_run(command: list[str], **_: object) -> SimpleNamespace:
            calls.append(command)
            return SimpleNamespace(stdout="noise\n" + json.dumps(next(responses)) + "\n")

        argv = [
            "whatsapp-uia-probe-orchestrator.py",
            "--vm",
            "OSL-WhatsApp-Client-1",
            "--invocation",
            "wa-uia-0001",
            "--timeout",
            "10",
        ]
        stdout = io.StringIO()
        with patch.object(sys, "argv", argv), patch.object(module.subprocess, "run", fake_run), contextlib.redirect_stdout(stdout):
            self.assertEqual(module.main(), 0)

        scripts = [Path(command[command.index("--scripts") + 1][1:]).name for command in calls]
        self.assertEqual(
            scripts,
            [
                "discover-whatsapp-session.ps1",
                "arm-whatsapp-uia-probe.ps1",
                "poll-whatsapp-uia-probe.ps1",
                "probe-whatsapp-msaa-metadata.ps1",
            ],
        )
        msaa_command = calls[-1]
        self.assertIn("MaxNodes=2048", msaa_command)
        self.assertIn("SessionId=2", msaa_command)
        receipt = json.loads(stdout.getvalue())
        self.assertEqual(receipt["Uia"]["Result"]["Status"], "capturedStructure")
        self.assertEqual(receipt["Msaa"]["Schema"], "whatsapp-msaa-metadata-probe/v1")

    def test_orchestrator_refuses_msaa_receipt_that_returns_content_or_exceeds_cap(self) -> None:
        module = load_orchestrator()
        responses = iter(
            [
                {"Status": "ready", "ProfileRead": False, "ProviderStorageRead": False, "SessionId": 2},
                {
                    "Schema": "whatsapp-msaa-metadata-probe/v1",
                    "TotalNodes": 65,
                    "NamesReturned": False,
                    "ValuesReturned": True,
                    "ProviderContentReturned": False,
                    "InputSent": False,
                    "WindowForegrounded": False,
                    "WhatsAppPrivateStorageRead": False,
                },
            ]
        )

        def fake_run(command: list[str], **_: object) -> SimpleNamespace:
            return SimpleNamespace(stdout=json.dumps(next(responses)))

        argv = [
            "whatsapp-uia-probe-orchestrator.py",
            "--vm",
            "OSL-WhatsApp-Client-1",
            "--invocation",
            "wa-uia-0001",
            "--bench",
            "msaa",
            "--msaa-max-nodes",
            "64",
        ]
        with patch.object(sys, "argv", argv), patch.object(module.subprocess, "run", fake_run), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(module.main(), 4)


def load_orchestrator():
    path = ROOT / "whatsapp-uia-probe-orchestrator.py"
    spec = importlib.util.spec_from_file_location("whatsapp_uia_probe_orchestrator_under_test", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


if __name__ == "__main__":
    unittest.main()
