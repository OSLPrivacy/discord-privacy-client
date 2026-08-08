#!/usr/bin/env python3
"""Verify the task 3980 private-words receive audit record.

The audit is intentionally stored as Markdown for people and as a small
line-oriented record for tests.  Each row is one receive gate that must be true
before private words are allowed to reach a display surface.
"""

from __future__ import annotations

import argparse
import re
import sys
import unittest
from dataclasses import dataclass
from pathlib import Path


DEFAULT_AUDIT = Path("/home/liamw/osl-plan/OSL-AUDITS/evidence/3980.md")
REQUIRED_CATEGORIES = {
    "sender",
    "recipient",
    "conversation",
    "expiry",
    "already_opened",
    "pinned_identity",
    "burn_list",
    "screen_protection",
}
REQUIRED_ROW_IDS = {
    "native_row_proof_matches_visible_row",
    "decrypted_display_enabled_for_scope",
    "sender_filter_and_inbox_sender_match_peer",
    "relay_row_scope_matches_active_conversation",
    "manual_scope_approval_not_burned",
    "relay_notice_sender_is_verified_peer",
    "relay_notice_recipient_is_self",
    "relay_notice_lifetime_is_live",
    "payload_binding_matches_orientation",
    "payload_conversation_and_service_match_context",
    "payload_lifetime_is_live",
    "wrapped_key_metadata_matches_notice_and_payload",
    "view_once_not_already_opened",
    "capture_policy_allows_plaintext",
}


@dataclass(frozen=True)
class CheckRow:
    row_id: str
    category: str
    active: bool
    compares: str
    source: str
    evidence: str


def _field(block: str, name: str) -> str:
    match = re.search(rf"^\s+{re.escape(name)}:\s*(.+?)\s*$", block, re.MULTILINE)
    return match.group(1).strip() if match else ""


def parse_rows(path: Path) -> list[CheckRow]:
    text = path.read_text(encoding="utf-8")
    blocks = re.split(r"(?m)^- id:\s+", text)
    rows: list[CheckRow] = []
    for block in blocks[1:]:
        first, _, rest = block.partition("\n")
        rows.append(
            CheckRow(
                row_id=first.strip(),
                category=_field(rest, "category"),
                active=_field(rest, "active").lower() == "true",
                compares=_field(rest, "compares"),
                source=_field(rest, "source"),
                evidence=_field(rest, "evidence"),
            )
        )
    return rows


class Task3980AuditTests(unittest.TestCase):
    audit_path = DEFAULT_AUDIT

    @classmethod
    def setUpClass(cls) -> None:
        cls.rows = parse_rows(cls.audit_path)

    def test_saved_list_has_at_least_8_active_checks(self) -> None:
        active = [row for row in self.rows if row.active]
        self.assertGreaterEqual(len(active), 8, f"active checks={len(active)}")

    def test_saved_list_has_every_recorded_check_row(self) -> None:
        row_ids = {row.row_id for row in self.rows}
        self.assertEqual(REQUIRED_ROW_IDS - row_ids, set())

    def test_each_check_names_what_it_compares(self) -> None:
        for row in self.rows:
            with self.subTest(row=row.row_id):
                self.assertTrue(row.row_id)
                self.assertIn(row.category, REQUIRED_CATEGORIES)
                self.assertTrue(row.active, f"{row.row_id} is turned off")
                self.assertIn(" vs ", row.compares, row.compares)
                self.assertGreaterEqual(len(row.compares), 12)
                self.assertTrue(row.source)
                self.assertTrue(row.evidence)

    def test_required_receive_categories_are_all_present(self) -> None:
        categories = {row.category for row in self.rows if row.active}
        self.assertEqual(REQUIRED_CATEGORIES - categories, set())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("audit", nargs="?", default=str(DEFAULT_AUDIT))
    args, _ = parser.parse_known_args()
    Task3980AuditTests.audit_path = Path(args.audit)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Task3980AuditTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
