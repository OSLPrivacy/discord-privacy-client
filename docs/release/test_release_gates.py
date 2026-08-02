#!/usr/bin/env python3
"""Keep the release-gate inventory complete and auditable."""

from __future__ import annotations

import json
import unittest
from pathlib import Path


INVENTORY = Path(__file__).with_name("release-gates.md")
REQUIRED_GATES = frozenset((
    "version-and-tag",
    "trusted-frontend",
    "core-library",
    "windows-identity-lifecycle",
    "supply-chain-policy",
    "release-notes",
    "public-claims",
    "candidate-update-manifest",
    "checksums-hashes-and-provenance",
    "protected-approval-and-two-clean-vms",
    "bootstrap-waiver-expiry",
    "promoted-client-acceptance",
    "reproducible-build-evidence",
))


def read_inventory(path: Path = INVENTORY) -> list[dict[str, str]]:
    document = path.read_text(encoding="utf-8")
    _, _, after_open = document.partition("```json\n")
    payload, closing, _ = after_open.partition("\n```")
    if not after_open or not closing:
        raise ValueError("release-gates.md must contain one JSON gate inventory")
    records = json.loads(payload)
    if not isinstance(records, list):
        raise ValueError("release-gates.md inventory must be a list")
    if not all(isinstance(record, dict) for record in records):
        raise ValueError("each release-gate inventory record must be an object")
    return records


class ReleaseGateInventoryTests(unittest.TestCase):
    def test_every_release_gate_has_a_recorded_refusal_control(self) -> None:
        records = read_inventory()
        ids = [record.get("id") for record in records]
        self.assertEqual(set(ids), REQUIRED_GATES)
        self.assertEqual(len(ids), len(set(ids)))
        for record in records:
            with self.subTest(gate=record["id"]):
                self.assertTrue(record.get("phase"))
                self.assertTrue(record.get("blocks"))
                self.assertTrue(record.get("command"))
                self.assertTrue(record.get("redControl"))
                self.assertEqual(record.get("result"), "rejected")


if __name__ == "__main__":
    unittest.main()
