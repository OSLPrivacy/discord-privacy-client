from __future__ import annotations

import re
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parent / "qa" / "install-app-installer-4952.ps1"


class AppInstaller4952StaticTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.script = SCRIPT.read_text(encoding="utf-8")

    def test_pins_all_downloaded_package_hashes(self) -> None:
        self.assertIn("https://aka.ms/Microsoft.VCLibs.x64.14.00.Desktop.appx", self.script)
        self.assertIn(
            "https://api.nuget.org/v3-flatcontainer/microsoft.ui.xaml/2.7.3/microsoft.ui.xaml.2.7.3.nupkg",
            self.script,
        )
        self.assertIn(
            "https://github.com/microsoft/winget-cli/releases/download/v1.6.3482/"
            "Microsoft.DesktopAppInstaller_8wekyb3d8bbwe.msixbundle",
            self.script,
        )
        self.assertIn(
            "https://github.com/microsoft/winget-cli/releases/download/v1.6.3482/"
            "24146eb205d040e69ef2d92d7034d97f_License1.xml",
            self.script,
        )
        hashes = re.findall(r"[0-9a-f]{64}", self.script)
        self.assertIn(
            "b56a9101f706f9d95f815f5b7fa6efbac972e86573d378b96a07cff5540c5961",
            hashes,
        )
        self.assertIn(
            "9ef3c54aa8c185603ba87d61673efb527062054adc736e27f2c4a033b5f797a8",
            hashes,
        )
        self.assertIn(
            "c98116463600bf102119938d7a26d2a16a89af8aa7415e8da4701d49d1b4ff1d",
            hashes,
        )
        self.assertIn(
            "8ce30d92abec6522beb2544e7b716983f5cba50751b580d89a36048bf4d90316",
            hashes,
        )
        self.assertIn(
            "61361fcd87aa5744472523ef41b53b1e9b00d579926f4c585bf2e9c98a998abd",
            hashes,
        )

    def test_hash_mismatch_exits_before_any_install(self) -> None:
        self.assertEqual(self.script.count("VM-4952-HASH-MISMATCH"), 1)
        mismatch = self.script.index("VM-4952-HASH-MISMATCH")
        first_install = self.script.index("Add-AppxPackage")
        self.assertLess(mismatch, first_install)
        self.assertRegex(self.script, r"exit\s+52")
        self.assertIn("WrongFirstHash", self.script)

    def test_all_hash_assertions_happen_before_installing_packages(self) -> None:
        first_install = self.script.index("Add-AppxPackage")
        hash_gate_region = self.script[:first_install]
        self.assertIn("foreach ($package in $packages)", hash_gate_region)
        self.assertIn("Assert-ExpectedSha256 $destination $package.Sha256", hash_gate_region)
        self.assertIn("Assert-ExpectedSha256 $xamlAppxPath", hash_gate_region)

    def test_installs_dependencies_before_app_installer_and_checks_winget(self) -> None:
        vclibs = self.script.index("Add-AppxPackage -Path $vclibsPath")
        xaml = self.script.index("Add-AppxPackage -Path $xamlAppxPath")
        app_installer = self.script.index("Add-AppxPackage -Path $appInstallerPath")
        provision = self.script.index("Add-AppxProvisionedPackage")
        winget = self.script.index("& winget --version")
        self.assertLess(vclibs, xaml)
        self.assertLess(xaml, app_installer)
        self.assertLess(app_installer, provision)
        self.assertLess(provision, winget)
        self.assertIn("VM-4952-WINGET-VERSION", self.script)


if __name__ == "__main__":
    unittest.main()
