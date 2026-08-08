from __future__ import annotations

import re
import unittest
from pathlib import Path


QA = Path(__file__).parent / "qa"
RUNNER = QA / "osl-vm-desktop-runner.ps1"
QUIET = QA / "osl-vm-quiet-signin.ps1"


class QuietSigninStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.runner = RUNNER.read_text(encoding="utf-8")
        cls.quiet = QUIET.read_text(encoding="utf-8")

    def test_desktop_runner_uses_logged_on_interactive_task_and_long_wait(self) -> None:
        self.assertRegex(self.runner, r"ValidateRange\(5,\s*2400\)")
        self.assertIn("PayloadPath", self.runner)
        self.assertIn("PayloadArguments", self.runner)
        self.assertRegex(self.runner, r"New-ScheduledTaskPrincipal")
        self.assertRegex(self.runner, r"LogonType\s+Interactive")
        self.assertRegex(self.runner, r"RunLevel\s+Highest")
        self.assertRegex(self.runner, r"New-ScheduledTaskAction\s+-Execute\s+'cmd\.exe'")
        self.assertRegex(self.runner, r"TaskPrincipalUser=\$InteractiveUser")
        self.assertIn("PayloadGzipBase64", self.runner)
        self.assertIn("PayloadPath, PayloadUri, PayloadBase64, or PayloadGzipBase64", self.runner)

    def test_power_lock_sleep_and_update_settings_are_zero_or_off(self) -> None:
        for token in (
            "display.videoidle",
            "display.consolelock",
            "sleep.standbyidle",
            "sleep.hibernateidle",
            "sleep.unattendedidle",
            "/setacvalueindex",
            "/setdcvalueindex",
            "/hibernate off",
            "ScreenSaveActive",
            "ScreenSaverIsSecure",
            "ScreenSaveTimeOut",
            "NoLockScreen",
            "DisableLockWorkstation",
            "NoAutoRebootWithLoggedOnUsers",
            "AlwaysAutoRebootAtScheduledTime",
        ):
            self.assertIn(token, self.quiet)

    def test_notifications_focus_assist_and_browser_first_run_policies_are_set(self) -> None:
        for token in (
            "NOC_GLOBAL_SETTING_TOASTS_ENABLED",
            "NOC_GLOBAL_SETTING_DND",
            "HideFirstRunExperience",
            "BrowserSignin",
            "DefaultBrowserSettingEnabled",
            "DisableFirefoxAccounts",
            "DontCheckDefaultBrowser",
            "OverrideFirstRunPage",
            "browser.aboutwelcome.enabled",
            "general.config.filename",
            "identity.fxaccounts.enabled",
            "startup.homepage_welcome_url",
            "trailhead.firstrun.branches",
            "datareporting.policy.dataSubmissionPolicyBypassNotification",
            "defaults\\profile",
            "browser\\defaults\\profile",
            "user.js",
        ):
            self.assertIn(token, self.quiet)

    def test_audit_launches_fresh_profiles_and_counts_required_prompts(self) -> None:
        self.assertRegex(self.quiet, r"--user-data-dir=\$profile")
        self.assertIn("-no-remote", self.quiet)
        self.assertIn("-profile", self.quiet)
        self.assertIn("Count-PromptMatches", self.quiet)
        self.assertIn("firstRunPages", self.quiet)
        self.assertIn("signInPrompts", self.quiet)
        self.assertIn("defaultBrowserPrompts", self.quiet)
        self.assertIn("sameUnlockedPaintedSession", self.quiet)
        self.assertIn("Get-DesktopPaint", self.quiet)
        self.assertIn("SummaryOnly", self.quiet)
        self.assertIn("Invoke-StageSelf", self.quiet)
        self.assertIn("C:\\OSL\\desktop-runner\\payloads", self.quiet)
        self.assertIn("Stop-NamedBrowserProcessesInSession", self.quiet)

    def test_failed_audit_prints_non_vacuous_noisy_marker(self) -> None:
        self.assertIn("VM-4955-NOISY", self.quiet)
        self.assertIn("screensaver", self.quiet)
        self.assertIn("ToLowerInvariant", self.quiet)
        self.assertRegex(self.quiet, r"exit\s+1")


if __name__ == "__main__":
    unittest.main()
