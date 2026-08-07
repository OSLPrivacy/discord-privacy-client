import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "qa" / "readiness-check-4958.ps1"
RUNNER = ROOT / "scripts" / "qa" / "osl-vm-desktop-runner.ps1"


class ReadinessCheck4958ScriptTest(unittest.TestCase):
    def test_ready_and_not_ready_markers_are_live(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn('"VM-4958-READY {0}"', text)
        self.assertIn('"VM-4958-NOT-READY {0}"', text)
        self.assertIn("VM-4958-NOT-SIGNED-IN", text)
        self.assertIn("VM-4958-ACCOUNT", text)
        self.assertIn('"VM-4958-INSTALLED {0} of 6"', text)
        self.assertIn('"VM-4958-SIGNED-IN {0} of 6"', text)
        self.assertIn("VM-4958-MISSING", text)
        self.assertIn("VM-4958-NO-WINDOW", text)
        self.assertIn("VM-4958-QUIET", text)
        self.assertIn("VM-4958-OSL-WINDOW", text)
        self.assertIn("VM-4958-IMAGE-FILES", text)

    def test_all_six_apps_are_named(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        apps = re.findall(r"Name = '([^']+)'", text)
        self.assertEqual(apps, ["Discord", "Telegram", "Signal", "WhatsApp", "Chrome", "Firefox"])

    def test_missing_signed_out_or_windowless_app_exits_one(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("($installedCount -eq 6) -and ($signedInCount -eq 6) -and ($windowCount -eq 6)", text)
        self.assertRegex(text, r"if \(\$ready\) \{\n\s+Write-Output \(\"VM-4958-READY \{0\}\" -f \$Machine\)\n\s+exit 0\n\}")
        self.assertTrue(text.rstrip().endswith("exit 1"))

    def test_window_deadline_defaults_to_sixty_seconds(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("[int]$WindowWaitSeconds = 60", text)
        self.assertIn("Wait-UiaWindow $app.ProcessNames $WindowWaitSeconds", text)
        self.assertIn("Wait-UiaWindow $OslProcessNames $WindowWaitSeconds", text)

    def test_evidence_is_accessibility_tree_and_never_a_picture(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("UIAutomationClient, UIAutomationTypes", text)
        self.assertIn("System.Windows.Automation.AutomationElement", text)
        for capture_api in (
            "System.Drawing",
            "CopyFromScreen",
            "BitBlt",
            "PrintWindow",
            "Graphics]::FromImage",
            "Save(",
            "ImageFormat",
            "Get-Screenshot",
        ):
            self.assertNotIn(capture_api, text)
        self.assertIn("($imageCount -eq 0)", text)

    def test_desktop_runner_requires_hash_and_interactive_user(self) -> None:
        text = RUNNER.read_text(encoding="utf-8")
        self.assertIn("payload SHA-256 mismatch", text)
        self.assertIn("-LogonType Interactive", text)
        self.assertIn("interactive session owner is not exact $InteractiveUser identity", text)


if __name__ == "__main__":
    unittest.main()
