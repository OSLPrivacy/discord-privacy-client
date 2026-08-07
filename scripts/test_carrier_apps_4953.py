import re
import unittest
from pathlib import Path


INSTALLER = Path(__file__).parent / "qa" / "install-carrier-apps-4953.ps1"
LAUNCH = Path(__file__).parent / "qa" / "verify-carrier-app-windows-4953.ps1"
PINNED_LIST = Path(__file__).parent / "qa" / "carrier-app-installers-4953.json"


class CarrierApps4953StaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.installer = INSTALLER.read_text(encoding="utf-8")
        cls.launch = LAUNCH.read_text(encoding="utf-8")
        cls.pinned_list = PINNED_LIST.read_text(encoding="utf-8")

    def test_six_exact_pins_are_present(self) -> None:
        for app, version, sha in [
            ("Discord", "1.0.9251", "9d1c22124d9e9230ff7cd9ce52c10d6938f7b688fdea7b3b66d25c333b1d58d5"),
            ("Telegram Desktop", "7.0.8", "5fdd32cc29f07f578373aa7d74c0e68aa864b223cddea882394da31a60248183"),
            ("Signal Desktop", "8.22.0", "b2bf0e60ed5d4757915c51fb2ecb49e2b26c2eb4a260e5af2875f0f35a9b27da"),
            ("WhatsApp Desktop", "StoreInstaller-22607.722.4.0", "9284d24602b5af591ca1ec79619401f486902b162d45a06ab64d325f6541150f"),
            ("Chrome", "151.0.7922.76", "3c6f4d683f377b16b4480eb4ceaac98430231cfbaf781b2e3c24ebc938a86bd0"),
            ("Firefox", "153.0.3", "8de41917930c35937a46eac6d0e16c633ed7456c771b32b89dc6fd65d55e512e"),
        ]:
            self.assertIn(f'"Name": "{app}"', self.pinned_list)
            self.assertIn(f'"Version": "{version}"', self.pinned_list)
            self.assertIn(f'"Sha256": "{sha}"', self.pinned_list)

    def test_hash_mismatch_exits_before_installer_invocation(self) -> None:
        mismatch = self.installer.index("VM-4953-HASH-MISMATCH")
        exit_53 = self.installer.index("exit 53")
        installer = self.installer.index("function Invoke-Installer")
        self.assertLess(mismatch, installer)
        self.assertLess(exit_53, installer)
        self.assertIn("VM-4953-HASH-MISMATCH $AppName", self.installer)
        self.assertIn("VM-4953-INSTALL-COUNT $script:InstallCount", self.installer)
        self.assertIn("[string]$PinnedListPath = ''", self.installer)
        self.assertIn("Read-PinnedCarrierSpecs $PinnedListPath", self.installer)

    def test_every_installer_is_downloaded_from_vendor_or_official_store_url(self) -> None:
        for url in [
            "https://stable.dl2.discordapp.net/distro/app/stable/win/x64/1.0.9251/DiscordSetup.exe",
            "https://github.com/telegramdesktop/tdesktop/releases/download/v7.0.8/tsetup-x64.7.0.8.exe",
            "https://updates.signal.org/desktop/signal-desktop-win-8.22.0.exe",
            "https://get.microsoft.com/installer/download/9NKSQGP7F2NH",
            "https://dl.google.com/dl/chrome/install/googlechromestandaloneenterprise64.msi",
            "https://download-installer.cdn.mozilla.net/pub/firefox/releases/153.0.3/win64/en-US/Firefox%20Setup%20153.0.3.exe",
        ]:
            self.assertIn(url, self.pinned_list)

    def test_launch_probe_fails_named_pair_on_missing_window(self) -> None:
        self.assertIn("VM-4953-NO-WINDOW $env:COMPUTERNAME $app", self.launch)
        self.assertIn("exit 1", self.launch)
        self.assertIn("TreeScope]::Children", self.launch)
        self.assertIn("VM-4953-LAUNCH-WINDOW", self.launch)

    def test_launch_probe_is_bounded_to_sixty_seconds(self) -> None:
        self.assertRegex(self.launch, re.compile(r"\[ValidateRange\(5, 60\)\]\s*\[int\]\$WaitSeconds = 60", re.MULTILINE))


if __name__ == "__main__":
    unittest.main()
