from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "qa" / "readiness-4958.ps1"
LEGACY_CHECK = ROOT / "scripts" / "qa" / "readiness-check-4958.ps1"
RUNNER = ROOT / "scripts" / "qa" / "osl-vm-desktop-runner.ps1"


class Readiness4958ContractTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.text = CHECK.read_text(encoding="utf-8")
        cls.runner = RUNNER.read_text(encoding="utf-8")

    def test_exact_six_apps_and_required_receipts(self):
        names = re.findall(r"(?m)^\s+Name='([^']+)'; Paths=", self.text)
        self.assertEqual(
            names,
            [
                "Discord",
                "Telegram Desktop",
                "Signal Desktop",
                "WhatsApp Desktop",
                "Chrome",
                "Firefox",
            ],
        )
        for marker in (
            "VM-4958-APP",
            "VM-4958-INSTALLED",
            "VM-4958-VERSIONS",
            "VM-4958-SIGNED-IN",
            "VM-4958-NOT-SIGNED-IN",
            "VM-4958-QUIET",
            "VM-4958-OSL-WINDOW",
            "VM-4958-IMAGE-FILES 0",
            "VM-4958-NOT-READY",
            "VM-4958-READY",
        ):
            self.assertIn(marker, self.text)

    def test_evidence_is_accessibility_only_and_never_an_image(self):
        self.assertIn("UIAutomationClient", self.text)
        self.assertIn("AutomationElement]::RootElement", self.text)
        self.assertIn("LegacyIAccessiblePattern", self.text)
        forbidden = (
            "system.drawing",
            "copyfromscreen",
            "bitmap",
            ".save(",
            ".png",
            ".jpg",
            ".jpeg",
            ".bmp",
            ".gif",
            ".webp",
            "versioninfo",
            "local state",
            "signedinuser.json",
            "signin-folder",
            "folder-fingerprint",
        )
        lowered = self.text.lower()
        for token in forbidden:
            self.assertNotIn(token, lowered)

    def test_fail_closed_predicate_and_sixty_second_bound(self):
        self.assertIn("[ValidateRange(5, 60)]", self.text)
        self.assertRegex(self.text, r"\$ready = \$installedCount -eq 6 -and \$versionCount -eq 6 -and \$signedInCount -eq 6 -and \$quiet -and \$oslWindow")
        self.assertRegex(self.text, r"VM-4958-NOT-READY machine=\$Machine[^\n]*\nexit 1")
        self.assertIn("no-window-inside-60-seconds", self.text)
        self.assertIn("missing-or-not-launchable", self.text)
        self.assertIn("not-signed-in", self.text)

    def test_accounts_and_versions_are_read_from_accessible_properties(self):
        for property_name in (
            "NameProperty",
            "HelpTextProperty",
            "ItemStatusProperty",
            "ValuePattern",
        ):
            self.assertIn(property_name, self.text)
        self.assertIn("Navigate-AccountSurface", self.text)
        self.assertIn("Navigate-VersionSurface", self.text)
        self.assertIn("version-not-in-accessibility-tree", self.text)
        self.assertIn("account-not-in-accessibility-tree", self.text)

    def test_runner_propagates_payload_failure(self):
        self.assertRegex(
            self.runner,
            r"(?m)^exit \[int\]\$result\.PayloadExitCode\s*$",
        )


class ReadinessCheck4958ScriptTest(unittest.TestCase):
    def test_ready_and_not_ready_markers_are_live(self) -> None:
        text = LEGACY_CHECK.read_text(encoding="utf-8")
        self.assertIn('"VM-4958-READY {0}"', text)
        self.assertIn('"VM-4958-NOT-READY {0}"', text)
        for marker in (
            "VM-4958-NOT-SIGNED-IN", "VM-4958-ACCOUNT", '"VM-4958-INSTALLED {0} of 6"',
            '"VM-4958-SIGNED-IN {0} of 6"', "VM-4958-MISSING", "VM-4958-NO-WINDOW",
            "VM-4958-QUIET", "VM-4958-OSL-WINDOW", "VM-4958-IMAGE-FILES",
        ):
            self.assertIn(marker, text)

    def test_all_six_apps_are_named(self) -> None:
        text = LEGACY_CHECK.read_text(encoding="utf-8")
        apps = re.findall(r"Name = '([^']+)'", text)
        self.assertEqual(apps, ["Discord", "Telegram", "Signal", "WhatsApp", "Chrome", "Firefox"])

    def test_missing_signed_out_or_windowless_app_exits_one(self) -> None:
        text = LEGACY_CHECK.read_text(encoding="utf-8")
        self.assertIn("($installedCount -eq 6) -and ($signedInCount -eq 6) -and ($windowCount -eq 6)", text)
        self.assertRegex(text, r"if \(\$ready\) \{\n\s+Write-Output \(\"VM-4958-READY \{0\}\" -f \$Machine\)\n\s+exit 0\n\}")
        self.assertTrue(text.rstrip().endswith("exit 1"))

    def test_window_deadline_defaults_to_sixty_seconds(self) -> None:
        text = LEGACY_CHECK.read_text(encoding="utf-8")
        self.assertIn("[int]$WindowWaitSeconds = 60", text)
        self.assertIn("Wait-UiaWindow $app.ProcessNames $WindowWaitSeconds", text)
        self.assertIn("Wait-UiaWindow $OslProcessNames $WindowWaitSeconds", text)

    def test_evidence_is_accessibility_tree_and_never_a_picture(self) -> None:
        text = LEGACY_CHECK.read_text(encoding="utf-8")
        self.assertIn("UIAutomationClient, UIAutomationTypes", text)
        self.assertIn("System.Windows.Automation.AutomationElement", text)
        for capture_api in (
            "System.Drawing", "CopyFromScreen", "BitBlt", "PrintWindow", "Graphics]::FromImage",
            "Save(", "ImageFormat", "Get-Screenshot",
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
