from __future__ import annotations

import re
import unittest
from pathlib import Path


QA = Path(__file__).parent / "qa"
RUNNER = QA / "osl-vm-desktop-runner.ps1"
WALK = QA / "osl-vm-discord-accessibility-walk.ps1"


class OslVmDesktopRunnerStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.runner = RUNNER.read_text(encoding="utf-8")
        cls.walk = WALK.read_text(encoding="utf-8")

    def test_runner_reuses_logged_on_interactive_task_pattern(self) -> None:
        self.assertIn("'osladmin'", self.runner)
        self.assertRegex(self.runner, r"New-ScheduledTaskPrincipal")
        self.assertRegex(self.runner, r"LogonType\s+Interactive")
        self.assertRegex(self.runner, r"RunLevel\s+Highest")
        self.assertRegex(self.runner, r"New-ScheduledTaskAction\s+-Execute\s+'cmd\.exe'")
        self.assertRegex(self.runner, r"/c powershell\.exe -NoProfile -ExecutionPolicy Bypass -File")
        self.assertRegex(self.runner, r">\s+\"\{1\}\"\s+2>&1")
        self.assertNotRegex(self.runner, r"LogonType\s+InteractiveToken")

    def test_runner_collects_only_scheduled_payload_output(self) -> None:
        self.assertRegex(self.runner, r"Start-ScheduledTask")
        self.assertRegex(self.runner, r"Get-Content\s+-LiteralPath\s+\$resultPath\s+-Raw")
        self.assertRegex(self.runner, r"Stdout")
        self.assertRegex(self.runner, r"Stderr")
        self.assertRegex(self.runner, r"desktop-runner terminal result identity mismatch")
        self.assertIn("PayloadGzipBase64", self.runner)
        self.assertIn("IO.Compression.GzipStream", self.runner)
        self.assertIn("DESKTOP-RUNNER", self.runner)

    def test_runner_itself_does_not_reach_windows(self) -> None:
        forbidden = (
            "UIAutomationClient",
            "AutomationElement",
            "EnumWindows",
            "GetForegroundWindow",
            "SetForegroundWindow",
            "Get-Process -Name Discord",
        )
        for pattern in forbidden:
            self.assertNotIn(pattern, self.runner)

    def test_walk_reports_session_folder_and_waits_for_tree_fill(self) -> None:
        self.assertIn("VM-4951-SESSION $sessionId", self.walk)
        self.assertIn("Join-Path $env:LOCALAPPDATA 'Discord'", self.walk)
        self.assertRegex(self.walk, r"MinimumDescendants\s*=\s*200")
        self.assertRegex(self.walk, r"while\s+\(\[DateTime\]::UtcNow\s+-le\s+\$deadline\)")
        self.assertRegex(self.walk, r"\$elementCount\s+-ge\s+\$MinimumDescendants")
        self.assertIn("MsaaNodeCount", self.walk)
        self.assertIn("RawUiaDescendantCount", self.walk)
        self.assertIn("uia-control-plus-uia-raw-plus-msaa", self.walk)
        self.assertIn("ReturnedElementCount", self.walk)
        self.assertRegex(self.walk, r"TreeFilled")
        self.assertRegex(self.walk, r"emptyOrNotReady")

    def test_walk_emits_metadata_only(self) -> None:
        for forbidden in ("Current.Name", "Current.AutomationId", "ValuePattern", "HelpText"):
            self.assertNotIn(forbidden, self.walk)
        self.assertIn("ControlTypeCounts", self.walk)
        self.assertIn("NamesReturned = $false", self.walk)
        self.assertIn("ValuesReturned = $false", self.walk)
        self.assertIn("TextReturned = $false", self.walk)


if __name__ == "__main__":
    unittest.main()
