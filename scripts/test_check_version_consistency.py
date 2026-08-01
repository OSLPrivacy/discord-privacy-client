"""Behavioral contract for the three release version declarations."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
CHECKER = SCRIPTS / "check_version_consistency.py"


class VersionConsistencyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "apps/osl-hub").mkdir(parents=True)
        (self.root / "apps/osl-hub-ui").mkdir(parents=True)
        (self.root / "apps/osl-hub/tauri.conf.json").write_text(
            json.dumps({"version": "1.2.3"}), encoding="utf-8"
        )
        (self.root / "apps/osl-hub/Cargo.toml").write_text(
            '[package]\nversion = "1.2.3"\n', encoding="utf-8"
        )
        (self.root / "apps/osl-hub-ui/package.json").write_text(
            json.dumps({"version": "1.2.3"}), encoding="utf-8"
        )

    def run_checker(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(CHECKER), "--root", str(self.root)],
            capture_output=True,
            text=True,
            check=False,
        )

    def test_accepts_one_version_in_all_three_declarations(self) -> None:
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("1.2.3", result.stdout)

    def test_rejects_a_ui_only_version_bump(self) -> None:
        package = self.root / "apps/osl-hub-ui/package.json"
        package.write_text(json.dumps({"version": "1.2.4"}), encoding="utf-8")

        result = self.run_checker()

        self.assertEqual(result.returncode, 1)
        self.assertIn("version declarations disagree", result.stderr)
        self.assertIn("package.json=1.2.4", result.stderr)

