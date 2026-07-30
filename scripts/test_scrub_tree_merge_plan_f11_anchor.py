from __future__ import annotations

import re
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DOC = ROOT / "docs" / "plans" / "scrub-tree-merge-plan-f11-anchor.md"
INTEGRATION_PIN = "403cfa2e090bf76ae4cb2950f3febcc72204fc59"


def _git(*args: str) -> str:
    return subprocess.check_output(
        ["git", *args],
        cwd=ROOT,
        text=True,
        stderr=subprocess.STDOUT,
    ).strip()


def _table_cells(line: str) -> list[str]:
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def _field_table(markdown: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for line in markdown.splitlines():
        cells = _table_cells(line) if line.startswith("|") else []
        if len(cells) == 2 and cells[0] not in {"Field", "---"}:
            fields[cells[0]] = cells[1]
    return fields


def _verification_rows(markdown: str) -> dict[str, list[str]]:
    rows: dict[str, list[str]] = {}
    for line in markdown.splitlines():
        cells = _table_cells(line) if line.startswith("|") else []
        if len(cells) == 4 and cells[0] not in {"Line", "---"}:
            rows[cells[0]] = re.findall(r"`([^`]+)`", cells[1])
    return rows


def integration_branch_anchor_records_osl_newest_integration_403cfa2() -> None:
    markdown = DOC.read_text(encoding="utf-8")
    fields = _field_table(markdown)
    rows = _verification_rows(markdown)

    assert re.findall(r"`([^`]+)`", fields["Branch"]) == ["unit-f11"]
    base_sha = re.findall(r"`([^`]+)`", fields["Base SHA (per plan §2 \"Base tree\")"])[0]
    assert base_sha == "16778b297d3ec8d0358b7d3812a95f4f8443e462"
    assert rows["osl-newest-integration"][0].startswith("403cfa2")

    branch_commit = _git("rev-parse", "--verify", "unit-f11^{commit}")
    parent = _git("rev-list", "--parents", "-n", "1", branch_commit).split()[1]
    assert parent == base_sha
    assert _git("rev-parse", "--verify", f"{INTEGRATION_PIN}^{{commit}}") == INTEGRATION_PIN

    commit_subject = _git("show", "--no-patch", "--format=%s", branch_commit)
    assert commit_subject == "f11: record integration-branch anchor point for the Scrub tree merge"


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del loader, tests, pattern
    suite = unittest.TestSuite()
    suite.addTest(unittest.FunctionTestCase(
        integration_branch_anchor_records_osl_newest_integration_403cfa2,
    ))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
