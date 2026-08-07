#!/usr/bin/env python3
"""Verify task 4272's X/Instagram/Messenger assumption audit."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


DEFAULT_SOURCE = Path("/home/liamw/osl-plan/OSL-AUDITS/OSL-TODO.txt")
DEFAULT_REPORT = Path("evidence/task-4272-three-app-assumptions.md")

SEARCH_PATTERN = r"\b(?:X|Instagram|Messenger)\b"
SEARCH_DESCRIPTION = (
    "task-block parser over /home/liamw/osl-plan/OSL-AUDITS/OSL-TODO.txt "
    f"with regex /{SEARCH_PATTERN}/; non-service X tasks ignored: "
    "0212, 0564, 0565, 0566, 1349, 3551"
)

NON_SERVICE_X_TASKS = {
    "0212",  # VIOLET-X unknown-name fixture, not the service.
    "0564",  # X close button.
    "0565",  # X close button.
    "0566",  # X close button.
    "1349",  # X close action.
    "3551",  # Ctrl+X shortcut.
}

DUPLICATE_RESTORE_TASKS = {
    "0156": "4255 restores X service-list/kind presence.",
    "0157": "4255 restores X service-list/kind presence.",
    "0158": "4255 restores X service-list/kind presence.",
    "0159": "4256 restores Instagram service-list/kind presence.",
    "0160": "4256 restores Instagram service-list/kind presence.",
    "0161": "4256 restores Instagram service-list/kind presence.",
    "0162": "4257 restores Messenger service-list/kind presence.",
    "0163": "4257 restores Messenger service-list/kind presence.",
    "0164": "4257 restores Messenger service-list/kind presence.",
    "3743": "4255 restores X everywhere before more place-kind work.",
    "3744": "4256 restores Instagram everywhere before more place-kind work.",
    "3745": "4257 restores Messenger everywhere before more place-kind work.",
}

RESTORE_ALREADY_RANGE = range(4250, 4278)

ANSWER_NEEDS = "needs the restore first"
ANSWER_COVERS = "restore already covers it"
ANSWER_DUPLICATE = "duplicate of something the restore gives us"


@dataclass(frozen=True)
class Task:
    task_id: str
    title: str
    block: str
    services: tuple[str, ...]
    answer: str
    note: str


def numeric_task_id(task_id: str) -> int:
    match = re.match(r"\d+", task_id)
    if not match:
        raise ValueError(f"bad task id: {task_id}")
    return int(match.group(0))


def parse_task_blocks(source: Path) -> list[tuple[str, str, str]]:
    blocks: list[tuple[str, str, str]] = []
    current: list[str] = []
    for line in source.read_text(encoding="utf-8").splitlines(keepends=True):
        if line.startswith("TASK "):
            if current:
                blocks.append(block_record(current))
            current = [line]
            continue
        if current and line.startswith("==="):
            blocks.append(block_record(current))
            current = []
            continue
        if current:
            current.append(line)
    if current:
        blocks.append(block_record(current))
    return blocks


def block_record(lines: list[str]) -> tuple[str, str, str]:
    block = "".join(lines).rstrip() + "\n"
    title = lines[0].strip()
    match = re.match(r"TASK (\d+[a-z]?)\b", title)
    if not match:
        raise ValueError(f"bad task header: {title}")
    return match.group(1), title, block


def service_hits(block: str) -> tuple[str, ...]:
    hits: list[str] = []
    if re.search(r"\bX\b", block):
        hits.append("X")
    if re.search(r"\bInstagram\b", block):
        hits.append("Instagram")
    if re.search(r"\bMessenger\b", block):
        hits.append("Messenger")
    return tuple(hits)


def classify(task_id: str) -> tuple[str, str]:
    if task_id in DUPLICATE_RESTORE_TASKS:
        return ANSWER_DUPLICATE, DUPLICATE_RESTORE_TASKS[task_id]
    if numeric_task_id(task_id) in RESTORE_ALREADY_RANGE:
        return ANSWER_COVERS, "This is in the 4250-4277 restore/restore-audit block."
    return ANSWER_NEEDS, "It assumes the named service exists before its own check can run."


def build_tasks(source: Path) -> list[Task]:
    rx = re.compile(SEARCH_PATTERN)
    tasks: list[Task] = []
    for task_id, title, block in parse_task_blocks(source):
        if task_id in NON_SERVICE_X_TASKS:
            continue
        if not rx.search(block):
            continue
        services = service_hits(block)
        if not services:
            continue
        answer, note = classify(task_id)
        tasks.append(Task(task_id, title, block, services, answer, note))
    return tasks


def render_report(tasks: list[Task]) -> str:
    counts = count_answers(tasks)
    lines = [
        "# TASK 4272 - X/Instagram/Messenger task assumptions",
        "",
        f"Search: `{SEARCH_DESCRIPTION}`",
        f"Count found: {len(tasks)}",
        "",
        "Answer counts:",
        f"- `{ANSWER_NEEDS}`: {counts[ANSWER_NEEDS]}",
        f"- `{ANSWER_COVERS}`: {counts[ANSWER_COVERS]}",
        f"- `{ANSWER_DUPLICATE}`: {counts[ANSWER_DUPLICATE]}",
        f"- no answer: {len(tasks) - sum(counts.values())}",
        "",
        "| task | services | answer | note | title |",
        "| --- | --- | --- | --- | --- |",
    ]
    for task in tasks:
        lines.append(
            "| "
            + " | ".join(
                [
                    f"TASK {task.task_id}",
                    ", ".join(task.services),
                    task.answer,
                    task.note,
                    task.title.replace("|", "\\|"),
                ]
            )
            + " |"
        )
    lines.append("")
    return "\n".join(lines)


def count_answers(tasks: list[Task]) -> dict[str, int]:
    counts = {ANSWER_NEEDS: 0, ANSWER_COVERS: 0, ANSWER_DUPLICATE: 0}
    for task in tasks:
        counts[task.answer] += 1
    return counts


def report_rows(report: str) -> dict[str, str]:
    rows: dict[str, str] = {}
    for line in report.splitlines():
        match = re.match(r"\| TASK (\d+[a-z]?) \| [^|]+ \| ([^|]+) \|", line)
        if match:
            rows[match.group(1)] = match.group(2).strip()
    return rows


def verify(tasks: list[Task], report_path: Path) -> int:
    expected = {task.task_id: task.answer for task in tasks}
    actual = report_rows(report_path.read_text(encoding="utf-8"))
    missing = sorted(set(expected) - set(actual), key=lambda value: (numeric_task_id(value), value))
    extra = sorted(set(actual) - set(expected), key=lambda value: (numeric_task_id(value), value))
    wrong = sorted(
        (task_id, expected[task_id], actual[task_id])
        for task_id in set(expected) & set(actual)
        if expected[task_id] != actual[task_id]
    )

    counts = count_answers(tasks)
    no_answer = len(tasks) - sum(counts.values())
    print(f"TASK4272_SEARCH={SEARCH_DESCRIPTION}")
    print(f"TASK4272_FOUND={len(tasks)}")
    print(f"TASK4272_REPORT_ROWS={len(actual)}")
    print(f"TASK4272_COUNT_{ANSWER_NEEDS.replace(' ', '_').upper()}={counts[ANSWER_NEEDS]}")
    print(f"TASK4272_COUNT_{ANSWER_COVERS.replace(' ', '_').upper()}={counts[ANSWER_COVERS]}")
    print(f"TASK4272_COUNT_{ANSWER_DUPLICATE.replace(' ', '_').upper()}={counts[ANSWER_DUPLICATE]}")
    print(f"TASK4272_NO_ANSWER={no_answer}")
    print(f"TASK4272_COUNTS_ADD_UP={sum(counts.values()) == len(tasks)}")

    failed = False
    if len(tasks) < 140:
        print(f"TASK4272_FAIL=count below 140: {len(tasks)}")
        failed = True
    if no_answer != 0:
        print(f"TASK4272_FAIL=no-answer count is {no_answer}")
        failed = True
    if sum(counts.values()) != len(tasks):
        print("TASK4272_FAIL=answer counts do not add up to total")
        failed = True
    if missing:
        print(f"TASK4272_FAIL=missing tasks: {', '.join('TASK ' + task for task in missing)}")
        failed = True
    if extra:
        print(f"TASK4272_FAIL=extra tasks: {', '.join('TASK ' + task for task in extra)}")
        failed = True
    for task_id, wanted, got in wrong:
        print(f"TASK4272_FAIL=TASK {task_id} answer {got!r} should be {wanted!r}")
        failed = True
    if not failed:
        print("TASK4272_OK=green")
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    tasks = build_tasks(args.source)
    if args.write:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(render_report(tasks), encoding="utf-8")
        print(f"TASK4272_WROTE={args.report}")
        print(f"TASK4272_FOUND={len(tasks)}")
        return 0
    return verify(tasks, args.report)


if __name__ == "__main__":
    sys.exit(main())
