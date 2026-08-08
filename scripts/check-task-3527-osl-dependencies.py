#!/usr/bin/env python3
"""Audit OSL Mail/Notes references in the release task corpus (TASK 3527)."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


TASK = re.compile(r"^TASK\s+(?P<id>\d+[a-z]?)\b(?P<title>.*)$", re.MULTILINE)
OSL_PRODUCT = re.compile(r"\bOSL[ -](?:Mail|Notes)\b", re.IGNORECASE)
# This intentionally matches a task that makes the product a prerequisite, not
# a task merely documenting its label, hold, or dependency-check fixture.
LIVE_DEPENDENCY = re.compile(
    r"(?:needs?|requires?|waits?\s+(?:on|for)|cannot\s+(?:pass|work)|"
    r"depends?\s+on).*?\bOSL[ -](?:Mail|Notes)\b.*?"
    r"(?:to\s+(?:exist|work)|(?:exists?|works?))",
    re.IGNORECASE | re.DOTALL,
)
LABEL_TASKS = {"3526", "3527", "3527b", "3528"}


@dataclass(frozen=True)
class Finding:
    source: str
    task_id: str
    title: str
    disposition: str
    needs_product: bool


def task_blocks(path: Path):
    text = path.read_text(encoding="utf-8")
    headings = list(TASK.finditer(text))
    file_is_held = bool(re.search(r"^#\s*ON HOLD\b", text, re.MULTILINE | re.IGNORECASE))
    for index, heading in enumerate(headings):
        end = headings[index + 1].start() if index + 1 < len(headings) else len(text)
        yield heading, text[heading.start() : end], file_is_held


def classify(source: Path, heading: re.Match[str], block: str, held: bool) -> Finding | None:
    if not OSL_PRODUCT.search(block):
        return None
    task_id = heading.group("id")
    title = heading.group("title").strip()
    if held:
        return Finding(source.name, task_id, title, "blocker=release-hold", False)
    if task_id in LABEL_TASKS:
        return Finding(source.name, task_id, title, "label=out-of-release-audit", False)
    if "[x]" in heading.group(0) or re.search(r"^done:\s", block, re.MULTILINE):
        return Finding(source.name, task_id, title, "label=completed-history", False)
    if LIVE_DEPENDENCY.search(block):
        return Finding(source.name, task_id, title, "blocker=live-osl-dependency", True)
    return Finding(source.name, task_id, title, "label=reference-only", False)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--task-root",
        type=Path,
        default=Path("/home/liamw/osl-plan/OSL-AUDITS/todo"),
        help="directory containing the authoritative task .txt files",
    )
    args = parser.parse_args()
    if not args.task_root.is_dir():
        print(f"TASK3527_ERROR=missing task root: {args.task_root}", file=sys.stderr)
        return 2

    findings: list[Finding] = []
    for source in sorted(args.task_root.glob("*.txt")):
        for heading, block, held in task_blocks(source):
            finding = classify(source, heading, block, held)
            if finding:
                findings.append(finding)

    for finding in findings:
        print(
            f"TASK3527_FOUND source={finding.source} task={finding.task_id} "
            f"{finding.disposition} title={finding.title}"
        )
    unlabelled = sum(
        "label=" not in finding.disposition and "blocker=" not in finding.disposition
        for finding in findings
    )
    active_dependencies = sum(finding.needs_product for finding in findings)
    print(f"TASK3527_MENTIONING_TASKS={len(findings)}")
    print(f"TASK3527_WITH_NEITHER_LABEL_NOR_BLOCKER={unlabelled}")
    print(f"TASK3527_NEED_OSL_MAIL_OR_NOTES_TO_WORK={active_dependencies}")
    return 1 if not findings or unlabelled or active_dependencies else 0


if __name__ == "__main__":
    raise SystemExit(main())
