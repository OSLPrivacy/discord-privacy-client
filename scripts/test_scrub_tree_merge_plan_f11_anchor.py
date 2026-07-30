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


def _worktree_records() -> list[dict[str, str]]:
    records: list[dict[str, str]] = []
    for block in _git("worktree", "list", "--porcelain").split("\n\n"):
        record: dict[str, str] = {}
        for line in block.splitlines():
            key, _, value = line.partition(" ")
            if value:
                record[key] = value
        if record:
            records.append(record)
    return records


def integration_branch_anchor_records_osl_newest_integration_403cfa2() -> None:
    markdown = DOC.read_text(encoding="utf-8")
    fields = _field_table(markdown)
    rows = _verification_rows(markdown)

    assert re.findall(r"`([^`]+)`", fields["Branch"]) == ["unit-f11"]
    assert re.findall(r"`([^`]+)`", fields["Worktree path"]) == ["/home/<user>/osl-unit-f11"]
    base_sha = re.findall(r"`([^`]+)`", fields["Base SHA (per plan §2 \"Base tree\")"])[0]
    assert base_sha == "16778b297d3ec8d0358b7d3812a95f4f8443e462"
    assert rows["osl-newest-integration"][0] == INTEGRATION_PIN

    branch_commit = _git("rev-parse", "--verify", "unit-f11^{commit}")
    commit_and_parents = _git("rev-list", "--parents", "-n", "1", branch_commit).split()
    assert commit_and_parents == [branch_commit, base_sha]
    assert _git("rev-parse", "--verify", f"{INTEGRATION_PIN}^{{commit}}") == INTEGRATION_PIN

    worktrees = [
        record for record in _worktree_records()
        if record.get("branch") == "refs/heads/unit-f11"
    ]
    assert worktrees == [{
        "worktree": str(Path.home() / "osl-unit-f11"),
        "HEAD": branch_commit,
        "branch": "refs/heads/unit-f11",
    }]

    commit_subject = _git("show", "--no-patch", "--format=%s", branch_commit)
    assert commit_subject == "f11: record integration-branch anchor point for the Scrub tree merge"


def test_integration_branch_anchor_records_osl_newest_integration_403cfa2() -> None:
    integration_branch_anchor_records_osl_newest_integration_403cfa2()


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
