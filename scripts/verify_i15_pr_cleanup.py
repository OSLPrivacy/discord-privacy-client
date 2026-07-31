#!/usr/bin/env python3
"""Validate the I6 stale-PR disposition register."""

from __future__ import annotations

import argparse
import re
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKLIST = ROOT / "docs/design/osl-internal-build-checklist.md"

START = "<!-- i15-pr-disposition:start -->"
END = "<!-- i15-pr-disposition:end -->"
EXPECTED_PRS = frozenset(range(1, 7))
REQUIRED_DISPOSITION = "closed without merge"


class DispositionError(ValueError):
    pass


def _normalize_cell(value: str) -> str:
    return re.sub(r"\s+", " ", value.strip()).lower()


def disposition_block(markdown: str) -> str:
    if markdown.count(START) != 1 or markdown.count(END) != 1:
        raise DispositionError("I6 PR disposition markers must appear exactly once")
    start = markdown.index(START) + len(START)
    end = markdown.index(END)
    if end <= start:
        raise DispositionError("I6 PR disposition block is empty or inverted")
    return markdown[start:end]


def parse_dispositions(markdown: str) -> dict[int, tuple[str, str, str]]:
    block = disposition_block(markdown)
    rows: dict[int, tuple[str, str, str]] = {}
    for line in block.splitlines():
        match = re.match(
            r"^\s*\|\s*#(?P<number>[1-9][0-9]*)\s*"
            r"\|\s*(?P<title>[^|]+?)\s*"
            r"\|\s*(?P<disposition>[^|]+?)\s*"
            r"\|\s*(?P<rationale>[^|]+?)\s*\|$",
            line,
        )
        if not match:
            continue
        number = int(match.group("number"))
        if number in rows:
            raise DispositionError(f"PR #{number} appears more than once")
        rows[number] = (
            match.group("title").strip(),
            match.group("disposition").strip(),
            match.group("rationale").strip(),
        )
    return rows


def validate_dispositions(markdown: str) -> list[str]:
    errors: list[str] = []
    try:
        rows = parse_dispositions(markdown)
    except DispositionError as error:
        return [str(error)]

    found = set(rows)
    missing = sorted(EXPECTED_PRS - found)
    extra = sorted(found - EXPECTED_PRS)
    if missing:
        errors.append("missing disposition rows for PRs: " + ", ".join(f"#{pr}" for pr in missing))
    if extra:
        errors.append("unexpected disposition rows for PRs: " + ", ".join(f"#{pr}" for pr in extra))

    for number, (title, disposition, rationale) in sorted(rows.items()):
        if not title:
            errors.append(f"PR #{number} title is empty")
        if _normalize_cell(disposition) != REQUIRED_DISPOSITION:
            errors.append(f"PR #{number} disposition must be '{REQUIRED_DISPOSITION}'")
        if not rationale or _normalize_cell(rationale) in {"tbd", "todo", "pending"}:
            errors.append(f"PR #{number} rationale must be concrete")

    block = disposition_block(markdown)
    if "gh pr list --state open --json number --jq length" not in block:
        errors.append("I6 block must record the exact open-PR verification command")
    if "**0**" not in block and "**0 open PRs**" not in block:
        errors.append("I6 block must record zero remaining open PRs")
    return errors


def run(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run this file's unit tests")
    args = parser.parse_args(argv)
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(I15PrCleanupTests)
        result = unittest.TextTestRunner(verbosity=2).run(suite)
        return 0 if result.wasSuccessful() else 1

    errors = validate_dispositions(CHECKLIST.read_text(encoding="utf-8"))
    for error in errors:
        print(error, file=sys.stderr)
    return 1 if errors else 0


def _sample_block(rows: str) -> str:
    return "\n".join(
        [
            "prefix",
            START,
            "  | PR | Title | Disposition | Rationale |",
            "  |---:|---|---|---|",
            "\n".join(f"  {row}" for row in rows.splitlines()),
            "",
            "Post-disposition check: `gh pr list --state open --json number --jq length` = **0**.",
            END,
            "suffix",
        ]
    )


VALID_ROWS = "\n".join(
    f"| #{number} | Title {number} | Closed without merge | Preserved and superseded. |"
    for number in range(1, 7)
)


class I15PrCleanupTests(unittest.TestCase):
    def test_accepts_six_closed_without_merge_rows(self) -> None:
        self.assertEqual(validate_dispositions(_sample_block(VALID_ROWS)), [])

    def test_refuses_missing_pr_row(self) -> None:
        rows = "\n".join(
            f"| #{number} | Title {number} | Closed without merge | Preserved and superseded. |"
            for number in range(1, 6)
        )
        self.assertIn("missing disposition rows for PRs: #6", validate_dispositions(_sample_block(rows)))

    def test_refuses_duplicate_pr_row(self) -> None:
        rows = VALID_ROWS + "\n| #6 | Duplicate | Closed without merge | Preserved. |"
        self.assertIn("PR #6 appears more than once", validate_dispositions(_sample_block(rows)))

    def test_refuses_open_or_pending_disposition(self) -> None:
        rows = VALID_ROWS.replace("Closed without merge", "Pending", 1)
        self.assertIn(
            "PR #1 disposition must be 'closed without merge'",
            validate_dispositions(_sample_block(rows)),
        )

    def test_refuses_missing_zero_open_verification(self) -> None:
        markdown = _sample_block(VALID_ROWS).replace(
            "Post-disposition check: `gh pr list --state open --json number --jq length` = **0**.",
            "Post-disposition check: not run.",
        )
        self.assertIn("I6 block must record the exact open-PR verification command", validate_dispositions(markdown))


if __name__ == "__main__":
    raise SystemExit(run())
