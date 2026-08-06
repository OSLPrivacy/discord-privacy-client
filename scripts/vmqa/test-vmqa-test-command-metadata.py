#!/usr/bin/python3
"""Regression tests for VMQA standard test-command metadata."""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_ROOT = Path(__file__).resolve().parent
RUNNER = SCRIPT_ROOT / "vmqa-run.sh"


class TestCommandMetadataTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.metadata = Path(self.tmp.name) / "metadata"

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def run_command(self, *args: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["VMQA_TEST_METADATA_DIR"] = str(self.metadata)
        completed = subprocess.run(
            [str(RUNNER), *args],
            check=False,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"{args} failed\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )
        return completed

    def assert_record(
        self,
        filename: str,
        *,
        category: str,
        command: str,
    ) -> None:
        path = self.metadata / filename
        self.assertTrue(
            path.is_file(),
            f"missing metadata record for category={category} command={command}",
        )
        record = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(record["schemaVersion"], 1)
        self.assertEqual(record["kind"], "vmqa-test-command-metadata")
        self.assertEqual(record["category"], category)
        self.assertEqual(record["command"], command)
        self.assertEqual(record["verdict"], "pass")
        self.assertEqual(record["reportedResult"], "pass")
        self.assertEqual(record["exitCode"], 0)
        self.assertTrue(record["oneBuildVersion"].strip())
        self.assertIn("feature:desktop", record["switches"])
        self.assertRegex(record["recordedAtUtc"], r"^\d{4}-\d{2}-\d{2}T")

    def test_unit_two_copy_and_screen_commands_each_save_metadata(self) -> None:
        self.run_command("test", "f1")
        self.assert_record(
            "unit-test-f1.json",
            category="unit",
            command="test:f1",
        )

        self.run_command("f1_live_windows_walkthrough_imports_nonempty_receipt")
        self.assert_record(
            "two-copy-f1_live_windows_walkthrough_imports_nonempty_receipt.json",
            category="two-copy",
            command="f1_live_windows_walkthrough_imports_nonempty_receipt",
        )

        self.run_command("f2_real_vm_five_frame_walkthrough")
        self.assert_record(
            "screen-f2_real_vm_five_frame_walkthrough.json",
            category="screen",
            command="f2_real_vm_five_frame_walkthrough",
        )


if __name__ == "__main__":
    unittest.main()
