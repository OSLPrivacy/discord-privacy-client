#!/usr/bin/python3
"""Regression tests for the VMQA test-result reader."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_ROOT = Path(__file__).resolve().parent
RUNNER = SCRIPT_ROOT / "vmqa-run.sh"
READER = SCRIPT_ROOT / "read-test-result.py"

COMPLETE_RECORD = {
    "schemaVersion": 1,
    "kind": "vmqa-test-command-metadata",
    "category": "unit",
    "command": "test:f1",
    "verdict": "pass",
    "reportedResult": "pass",
    "exitCode": 0,
    "oneBuildVersion": "0.0.1",
    "switches": ["feature:desktop"],
    "recordedAtUtc": "2026-08-06T00:00:00Z",
}


class ResultReaderTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def read_result(self, path: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(READER), str(path)],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def write_record(self, record: dict) -> Path:
        path = self.tmp_path / "record.json"
        path.write_text(json.dumps(record) + "\n", encoding="utf-8")
        return path

    def test_real_complete_result_is_accepted(self) -> None:
        metadata = self.tmp_path / "metadata"
        env = os.environ.copy()
        env["VMQA_TEST_METADATA_DIR"] = str(metadata)
        env["OSL_DISABLE_CSP_STRIP"] = "1"
        completed = subprocess.run(
            [str(RUNNER), "test", "f1"],
            check=False,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"test f1 failed\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )
        record_path = metadata / "unit-test-f1.json"
        record = json.loads(record_path.read_text(encoding="utf-8"))
        self.assertTrue(record["oneBuildVersion"].strip())
        self.assertIn("feature:desktop", record["switches"])
        self.assertIn("OSL_DISABLE_CSP_STRIP=1", record["switches"])

        read = self.read_result(record_path)
        self.assertEqual(
            read.returncode, 0, f"reader refused a complete result: {read.stderr}"
        )
        self.assertIn("accepted:", read.stdout)

    def test_hand_made_complete_result_is_accepted(self) -> None:
        read = self.read_result(self.write_record(COMPLETE_RECORD))
        self.assertEqual(read.returncode, 0, read.stderr)
        self.assertIn("accepted: version=0.0.1", read.stdout)

    def test_hand_made_incomplete_results_exit_1(self) -> None:
        def without(key: str) -> dict:
            record = dict(COMPLETE_RECORD)
            del record[key]
            return record

        incomplete = {
            "missing version": without("oneBuildVersion"),
            "blank version": {**COMPLETE_RECORD, "oneBuildVersion": " "},
            "missing switches": without("switches"),
            "empty switches": {**COMPLETE_RECORD, "switches": []},
            "non-list switches": {**COMPLETE_RECORD, "switches": "feature:desktop"},
            "blank switch entry": {**COMPLETE_RECORD, "switches": [""]},
        }
        for label, record in incomplete.items():
            with self.subTest(label):
                read = self.read_result(self.write_record(record))
                self.assertEqual(read.returncode, 1, f"{label}: {read.stdout}")
                self.assertIn("refused:", read.stderr)

    def test_unreadable_and_malformed_results_exit_1(self) -> None:
        read = self.read_result(self.tmp_path / "absent.json")
        self.assertEqual(read.returncode, 1)

        malformed = self.tmp_path / "malformed.json"
        malformed.write_text("{not json", encoding="utf-8")
        self.assertEqual(self.read_result(malformed).returncode, 1)


if __name__ == "__main__":
    unittest.main()
