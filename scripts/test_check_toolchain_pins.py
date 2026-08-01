#!/usr/bin/env python3
"""Behaviour tests for check_toolchain_pins.py."""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/check_toolchain_pins.py"


class ToolchainPinsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name)
        for relative in (".github/workflows", "docs/release"):
            (self.repo / relative).mkdir(parents=True, exist_ok=True)
        (self.repo / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.88.0"\n')
        for name in ("rust-test.yml", "osl-hub-release.yml", "reproducible-build.yml"):
            (self.repo / ".github/workflows" / name).write_text(
                "steps:\n  - uses: dtolnay/rust-toolchain@pinned\n    with:\n      toolchain: 1.88.0\n"
            )
        (self.repo / "docs/release/toolchain-pin-map.md").write_text(
            "rustc 1.88.0 (6b00bc388 2025-06-23)\nrelease: 1.88.0\n"
        )
        self.git("init", "-q")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "Toolchain Test")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.git("tag", "hub-v0.1.0")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def git(self, *args: str) -> None:
        subprocess.run(["git", "-C", str(self.repo), *args], check=True, capture_output=True)

    def run_checker(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["python3", str(CHECKER), "--root", str(self.repo)], text=True, capture_output=True
        )

    def test_accepts_matching_pins_and_observation(self) -> None:
        result = self.run_checker()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("hub-v0.1.0\t1.88.0", result.stdout)

    def test_rejects_a_workflow_pin_that_drifted(self) -> None:
        workflow = self.repo / ".github/workflows/reproducible-build.yml"
        workflow.write_text(workflow.read_text().replace("1.88.0", "1.97.1"))
        result = self.run_checker()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("toolchain pins disagree", result.stderr)

    def test_rejects_a_recorded_compiler_that_drifted(self) -> None:
        evidence = self.repo / "docs/release/toolchain-pin-map.md"
        evidence.write_text(evidence.read_text().replace("release: 1.88.0", "release: 1.97.1"))
        result = self.run_checker()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("observed rustc release 1.97.1", result.stderr)


if __name__ == "__main__":
    unittest.main()
