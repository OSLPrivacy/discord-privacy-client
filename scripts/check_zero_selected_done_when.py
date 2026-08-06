#!/usr/bin/env python3
"""Find plan tasks whose zero-failure wording can pass with no checks selected."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


DEFAULT_PLAN_DIR = Path("/home/liamw/osl-plan/OSL-AUDITS/todo")
TASK_HEADER_RE = re.compile(r"^TASK\s+([0-9]+[a-z]?)\s*(?:\[[xX]\])?\s*-\s*(.+)$")
DONE_WHEN_RE = re.compile(r"^done when:\s*(.+)$")
ZERO_FAILURE_RE = re.compile(
    r"\b(?:(?:zero|0|no)\s+(?P<before>(?:[a-z-]+\s+){0,2})failures?"
    r"|failures?\s+(?:is|are|=|:)?\s*(?:zero|0|none))\b",
    re.IGNORECASE,
)
SELF_REFERENCE_RE = re.compile(
    r"\b(?:made-up|fake|feeding the checker|feed(?:ing)? the checker)\b",
    re.IGNORECASE,
)
EXPECTED_SELECTION_RE = re.compile(
    r"\b(?:"
    r"expected\s+(?:number|count|total)|"
    r"(?:number|count|total)\s+[^.;]*\b(?:above|greater than)\s+(?:zero|0)|"
    r"(?:at least|exactly)\s+(?:one|two|three|four|five|six|seven|eight|nine|ten|\d+)\s+"
    r"(?:checks?|tests?|runs?|wrappers?|commands?|pages?|links?|routes?|rows?|items?)|"
    r"all\s+(?:one|two|three|four|five|six|seven|eight|nine|ten|\d+)\s+"
    r"(?:checks?|tests?|runs?|wrappers?|commands?|pages?|links?|routes?|rows?|items?)|"
    r"(?:checks?|tests?|runs?|wrappers?|commands?|pages?|links?|routes?|rows?|items?)\s+"
    r"[^.;]*\b(?:count|total)\s+[^.;]*\b(?:above|greater than)\s+(?:zero|0)|"
    r"(?:\d+|one|two|three|four|five|six|seven|eight|nine|ten)\s+"
    r"(?:checks?|tests?|runs?|wrappers?|commands?|pages?|links?|routes?|rows?|items?)"
    r")\b",
    re.IGNORECASE,
)
NON_SELECTION_FAILURE_KINDS = {"silent"}


@dataclass(frozen=True)
class Task:
    task_id: str
    title: str
    source: str
    line: int
    done_when: str


@dataclass(frozen=True)
class Finding:
    task: Task
    zero_failure_count: int
    expected_selection_count: int


def read_plan_files(plan_dir: Path) -> list[tuple[str, str]]:
    paths = sorted(plan_dir.glob("[0-9][0-9]-*.txt"))
    return [(str(path), path.read_text(encoding="utf-8")) for path in paths]


def parse_tasks(sources: Iterable[tuple[str, str]]) -> list[Task]:
    tasks: list[Task] = []
    current_id: str | None = None
    current_title: str | None = None
    current_source = ""
    current_line = 0
    for source, text in sources:
        for line_number, raw_line in enumerate(text.splitlines(), start=1):
            task_match = TASK_HEADER_RE.match(raw_line)
            if task_match:
                current_id = task_match.group(1)
                current_title = task_match.group(2)
                current_source = source
                current_line = line_number
                continue

            done_match = DONE_WHEN_RE.match(raw_line)
            if done_match and current_id and current_title:
                tasks.append(
                    Task(
                        task_id=current_id,
                        title=current_title,
                        source=current_source,
                        line=current_line,
                        done_when=done_match.group(1),
                    )
                )
    return tasks


def zero_failure_count(done_when: str) -> int:
    count = 0
    for match in ZERO_FAILURE_RE.finditer(done_when):
        preceding_words = match.groupdict().get("before") or ""
        last_preceding = preceding_words.strip().split(" ")[-1:]
        if not last_preceding or last_preceding == [""]:
            last_preceding = done_when[: match.start()].strip().split(" ")[-1:]
        if last_preceding and last_preceding[0].lower() in NON_SELECTION_FAILURE_KINDS:
            continue
        count += 1
    return count


def expected_selection_count(done_when: str) -> int:
    return sum(1 for _ in EXPECTED_SELECTION_RE.finditer(done_when))


def find_zero_selected_risks(tasks: Iterable[Task]) -> list[Finding]:
    findings: list[Finding] = []
    for task in tasks:
        if SELF_REFERENCE_RE.search(task.done_when):
            continue
        failures = zero_failure_count(task.done_when)
        expected = expected_selection_count(task.done_when)
        if failures and expected == 0:
            findings.append(
                Finding(
                    task=task,
                    zero_failure_count=failures,
                    expected_selection_count=expected,
                )
            )
    return findings


def load_sources(args: argparse.Namespace) -> list[tuple[str, str]]:
    if args.input == "-":
        return [("<stdin>", sys.stdin.read())]
    if args.input:
        path = Path(args.input)
        if path.is_dir():
            return read_plan_files(path)
        return [(str(path), path.read_text(encoding="utf-8"))]
    return read_plan_files(args.plan_dir)


def print_report(tasks: list[Task], findings: list[Finding], args: argparse.Namespace) -> None:
    print(f"tasks read: {len(tasks)}")
    if args.expect_task_count is not None:
        verdict = "ok" if len(tasks) == args.expect_task_count else "mismatch"
        print(f"expected whole-plan task count: {args.expect_task_count} ({verdict})")

    flagged_ids = {finding.task.task_id for finding in findings}
    for task_id in args.require_not_flagged:
        verdict = "not flagged" if task_id not in flagged_ids else "FLAGGED"
        print(f"expected-number task check: TASK {task_id} {verdict}")

    print("flagged tasks:")
    if not findings:
        print("  none")
    for finding in findings:
        task = finding.task
        print(
            "  "
            f"TASK {task.task_id} count={finding.expected_selection_count} "
            f"zero_failure_phrases={finding.zero_failure_count} "
            f"{Path(task.source).name}:{task.line} - {task.title}"
        )
    print(f"flagged count: {len(findings)}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan-dir", type=Path, default=DEFAULT_PLAN_DIR)
    parser.add_argument("--input", help="Plan file, plan directory, or '-' for stdin.")
    parser.add_argument("--expect-task-count", type=int)
    parser.add_argument("--require-not-flagged", action="append", default=[])
    parser.add_argument("--expect-flagged-count", type=int)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv or sys.argv[1:])
    tasks = parse_tasks(load_sources(args))
    findings = find_zero_selected_risks(tasks)
    print_report(tasks, findings, args)

    failed = False
    if args.expect_task_count is not None and len(tasks) != args.expect_task_count:
        failed = True
    if args.expect_flagged_count is not None and len(findings) != args.expect_flagged_count:
        failed = True
    for task_id in args.require_not_flagged:
        if any(finding.task.task_id == task_id for finding in findings):
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
