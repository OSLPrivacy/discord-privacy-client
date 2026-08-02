#!/usr/bin/env python3
"""Ensure the Worker uses a checked-in projection of the company checklist."""

from hashlib import sha256
import json
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
CHECKLIST = ROOT / "docs/design/osl-internal-build-checklist.md"
PROJECTION = ROOT / "keyserver-cf/src/generated/osl-checklist.json"
TELEGRAM = ROOT / "keyserver-cf/src/lib/telegram.ts"


class TelegramChecklistProjectionTest(unittest.TestCase):
    def test_projection_is_bound_to_the_internal_company_checklist(self) -> None:
        projection = json.loads(PROJECTION.read_text(encoding="utf-8"))
        self.assertEqual(projection["schema_version"], 1)
        self.assertEqual(projection["source"], "docs/design/osl-internal-build-checklist.md")
        self.assertEqual(
            projection["source_sha256"],
            sha256(CHECKLIST.read_bytes()).hexdigest(),
        )
        self.assertIn('from "../generated/osl-checklist.json"', TELEGRAM.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
