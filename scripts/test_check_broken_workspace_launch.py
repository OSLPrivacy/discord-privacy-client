#!/usr/bin/env python3
"""Focused contract tests for TASK 1608's broken-workspace launch verifier."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECK = ROOT / "scripts" / "check_broken_workspace_launch.py"
FIXTURE = ROOT / "scripts" / "fixtures" / "task-1608-broken-workspace.json"


class BrokenWorkspaceLaunchTests(unittest.TestCase):
    def run_check(self, fixture: dict[str, object]) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "broken-workspace.json"
            path.write_text(json.dumps(fixture), encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(CHECK), "--fixture", str(path)],
                text=True,
                capture_output=True,
                check=False,
            )

    def fixture(self) -> dict[str, object]:
        return json.loads(FIXTURE.read_text(encoding="utf-8"))

    def test_broken_workspace_shows_one_plain_cause_and_opens_none(self) -> None:
        result = self.run_check(self.fixture())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("failure_screens=1", result.stdout)
        self.assertIn("Required local data folder is unavailable.", result.stdout)
        self.assertIn("broken_workspaces_open=0", result.stdout)

    def test_fixture_without_required_local_data_folder_fails(self) -> None:
        fixture = self.fixture()
        fixture.pop("requiredLocalDataFolder")
        result = self.run_check(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("required local data folder record is missing", result.stderr)

    def test_opening_a_broken_workspace_fails(self) -> None:
        fixture = self.fixture()
        fixture["openWorkspaces"] = ["broken-workspace"]
        result = self.run_check(fixture)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("expected 0 broken workspaces open, found 1", result.stderr)


if __name__ == "__main__":
    unittest.main()
