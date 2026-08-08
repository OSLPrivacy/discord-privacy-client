import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "qa" / "signin-survival-4956.ps1"
RUNNER = ROOT / "scripts" / "qa" / "osl-vm-desktop-runner.ps1"


class SignInSurvival4956ScriptTest(unittest.TestCase):
    def test_finish_line_marker_and_red_marker_are_live(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("VM-4956-SURVIVED {0} of 6", text)
        self.assertIn("VM-4956-ASKS-AGAIN", text)
        self.assertRegex(text, r"if \(\$survived -eq 6\) \{ exit 0 \}")
        self.assertRegex(text, r"exit 1")

    def test_all_six_apps_have_control_and_folder(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        apps = re.findall(r"Name='([^']+)'", text)
        self.assertEqual(apps, ["Discord", "Telegram", "Signal", "WhatsApp", "Chrome", "Firefox"])
        self.assertEqual(text.count("KeepSignedInControl="), 6)
        self.assertEqual(text.count("SignInFolder="), 6)
        self.assertIn("WhatsApp Desktop drops a linked device that has not seen its phone for about 14 days", text)
        self.assertIn("Signal Desktop is a linked device", text)

    def test_break_and_restore_delete_only_named_signin_folder(self) -> None:
        text = SCRIPT.read_text(encoding="utf-8")
        self.assertIn("Copy-Item -LiteralPath $source -Destination $target -Recurse -Force", text)
        self.assertIn("Remove-Item -LiteralPath $source -Recurse -Force", text)
        self.assertIn("VM-4956-BREAK-REMOVED", text)
        self.assertIn("VM-4956-RESTORED", text)

    def test_desktop_runner_requires_hash_and_interactive_user(self) -> None:
        text = RUNNER.read_text(encoding="utf-8")
        self.assertIn("payload hash mismatch", text)
        self.assertIn("LogonType Interactive", text)
        self.assertIn("session $SessionId is not active", text)


if __name__ == "__main__":
    unittest.main()
