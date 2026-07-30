from __future__ import annotations

import re
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parent / "qa" / "osl-vm-deploy-preserve.ps1"


class OslVmDeployPreserveStaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.script = SCRIPT.read_text(encoding="utf-8")

    def test_downloads_only_from_exact_trusted_host_with_managed_identity(self) -> None:
        self.assertIn("osltestartifactsa7d5.blob.core.windows.net", self.script)
        self.assertRegex(self.script, r"Scheme\s+-cne\s+'https'")
        self.assertRegex(self.script, r"Host\s+-cne\s+\$allowedArtifactHost")
        self.assertRegex(self.script, r"metadata/identity/oauth2/token")
        self.assertRegex(self.script, r"AuthenticationHeaderValue.*Bearer")

    def test_both_artifacts_are_hashed_before_install(self) -> None:
        staging = self.script[: self.script.index("$exactOsl =")]
        self.assertRegex(staging, r"Save-ManagedIdentityArtifact \$ExeUri \$exeStage \$exeExpected")
        self.assertRegex(staging, r"Save-ManagedIdentityArtifact \$WebView2LoaderUri \$loaderStage \$loaderExpected")
        self.assertRegex(self.script, r"Get-FileHash[^\r\n]+SHA256")
        self.assertRegex(self.script, r"downloaded artifact hash mismatch")

    def test_stops_only_the_exact_osl_executable(self) -> None:
        self.assertRegex(self.script, r"Name = 'OSL Privacy\.exe'")
        self.assertRegex(self.script, r"ExecutablePath.*OrdinalIgnoreCase")
        stop_lines = "\n".join(
            line for line in self.script.splitlines() if "Stop-Process" in line
        )
        self.assertRegex(stop_lines, r"Stop-Process -Id")
        self.assertNotRegex(stop_lines, r"Discord|explorer|taskkill")
        self.assertRegex(self.script, r"stopDeadline")

    def test_replaces_atomically_and_rolls_back_transaction(self) -> None:
        self.assertRegex(self.script, r"\[IO\.File\]::Replace\(\$exeStage")
        self.assertRegex(self.script, r"\[IO\.File\]::Replace\(\$loaderStage")
        catch = self.script[self.script.index("} catch {") :]
        self.assertRegex(catch, r"\[IO\.File\]::Replace\(\$loaderBackup, \$loaderPath")
        self.assertRegex(catch, r"\[IO\.File\]::Replace\(\$exeBackup, \$oslExePath")

    def test_relaunches_in_exact_interactive_session_and_is_bounded(self) -> None:
        self.assertRegex(self.script, r"SessionId\s+-eq\s+\$SessionId")
        self.assertRegex(self.script, r"owner\.User\s+-cne\s+'osltest'")
        self.assertRegex(self.script, r"LogonType\s+Interactive")
        self.assertRegex(self.script, r"RunLevel\s+Limited")
        self.assertRegex(self.script, r"launchDeadline")
        self.assertRegex(self.script, r"Unregister-ScheduledTask")

    def test_profiles_and_discord_are_preserved_by_contract(self) -> None:
        self.assertRegex(self.script, r"DiscordPidsBefore\s*=\s*\$discordBefore")
        self.assertRegex(self.script, r"DiscordPidsAfter\s*=\s*\$discordAfter")
        self.assertRegex(self.script, r"DiscordPidSetUnchanged\s*=\s*\$true")
        self.assertRegex(self.script, r"ProfilesPreserved\s*=\s*\$true")
        forbidden = (
            r"AppData",
            r"user-data-dir",
            r"\.discord",
            r"Stop-Process[^\r\n]+Discord",
            r"Remove-Item[^\r\n]+Discord",
        )
        for pattern in forbidden:
            self.assertNotRegex(self.script, pattern)


if __name__ == "__main__":
    unittest.main()
