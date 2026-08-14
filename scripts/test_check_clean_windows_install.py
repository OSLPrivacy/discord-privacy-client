#!/usr/bin/env python3
"""Focused contract tests for TASK 1607's receipt verifier."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check_clean_windows_install.py"
SHA256 = "b6ee65316bf40d9ab058d33bdc0e75b46085fccdacffbade9fc263d7614abd24"


def good_receipt() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "installerSha256": SHA256,
        "installer": {"name": "osl-hub-0.1.0-x64-nsis.exe", "sizeBytes": 7126171},
        "installedApps": [{"displayName": "OSL Privacy"}],
        "welcomeLaunch": {"processId": 1607, "processAlive": True, "visibleText": "Welcome"},
    }


class CleanWindowsInstallReceiptTests(unittest.TestCase):
    def run_check(self, receipt: dict[str, object]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "receipt.json"
            path.write_text(json.dumps(receipt), encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(CHECK), "--receipt", str(path), "--expected-sha256", SHA256],
                text=True,
                capture_output=True,
                check=False,
            )

    def test_accepts_checksum_checked_installed_welcome_launch(self) -> None:
        result = self.run_check(good_receipt())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("installed_app=OSL Privacy welcome=Welcome", result.stdout)

    def test_rejects_fixture_without_welcome_launch(self) -> None:
        fixture = good_receipt()
        fixture.pop("welcomeLaunch")
        result = self.run_check(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Welcome launch record is missing", result.stderr)


if __name__ == "__main__":
    unittest.main()
