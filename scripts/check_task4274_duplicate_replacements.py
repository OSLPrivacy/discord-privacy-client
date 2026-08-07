#!/usr/bin/env python3
"""Verify task 4274's duplicate-to-restore replacement report."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


DEFAULT_SOURCE = Path("/home/liamw/osl-plan/OSL-AUDITS/OSL-TODO.txt")
DEFAULT_4272_REPORT = Path("evidence/task-4272-three-app-assumptions.md")
DEFAULT_REPORT = Path("evidence/task-4274-duplicate-replacements.md")

ANSWER_DUPLICATE = "duplicate of something the restore gives us"

EXPECTED_ROWS = {
    "0156": {
        "rebuilds": "X whitelist kind list: direct message and public post.",
        "replacement": "4255",
        "proving": "Prove the restored X kind list still exposes direct message and public post, with no extra kind.",
    },
    "0157": {
        "rebuilds": "X kind-to-auto-rule and allowed-place lookup wiring.",
        "replacement": "4255",
        "proving": "Prove the restored X kind wiring still gives independent saved choices for both X kinds.",
    },
    "0158": {
        "rebuilds": "X allowed-place kind fixtures for direct message and public post, plus rejection of a third kind.",
        "replacement": "4255",
        "proving": "Prove the restored X kind fixtures resolve both allowed kinds and reject the invented third kind.",
    },
    "0159": {
        "rebuilds": "Instagram whitelist kind list: direct message, group chat, and public post.",
        "replacement": "4256",
        "proving": "Prove the restored Instagram kind list still exposes direct message, group chat, and public post.",
    },
    "0160": {
        "rebuilds": "Instagram kind-to-auto-rule and allowed-place lookup wiring.",
        "replacement": "4256",
        "proving": "Prove the restored Instagram kind wiring still gives independent saved choices for all three Instagram kinds.",
    },
    "0161": {
        "rebuilds": "Instagram allowed-place kind fixtures for direct message, group chat, and public post, plus rejection of a fourth kind.",
        "replacement": "4256",
        "proving": "Prove the restored Instagram kind fixtures resolve all three allowed kinds and reject the invented fourth kind.",
    },
    "0162": {
        "rebuilds": "Messenger whitelist kind list: direct message and group chat.",
        "replacement": "4257",
        "proving": "Prove the restored Messenger kind list still exposes direct message and group chat, with no extra kind.",
    },
    "0163": {
        "rebuilds": "Messenger kind-to-auto-rule and allowed-place lookup wiring.",
        "replacement": "4257",
        "proving": "Prove the restored Messenger kind wiring still gives independent saved choices for both Messenger kinds.",
    },
    "0164": {
        "rebuilds": "Messenger allowed-place kind fixtures for direct message and group chat, plus rejection of a third kind.",
        "replacement": "4257",
        "proving": "Prove the restored Messenger kind fixtures resolve both allowed kinds and reject the invented third kind.",
    },
    "3743": {
        "rebuilds": "X missing place kinds for group direct messages and replies.",
        "replacement": "4255",
        "proving": "Prove the restored X service and place-kind lists cover group direct messages and replies before the dependent checks run.",
    },
    "3744": {
        "rebuilds": "Instagram missing place kinds for comments and stories.",
        "replacement": "4256",
        "proving": "Prove the restored Instagram service and place-kind lists cover comments and stories before the dependent checks run.",
    },
    "3745": {
        "rebuilds": "Messenger missing place kind for community conversations.",
        "replacement": "4257",
        "proving": "Prove the restored Messenger service and place-kind lists cover community conversations before the dependent checks run.",
    },
}


@dataclass(frozen=True)
class TaskBlock:
    task_id: str
    title: str
    block: str


@dataclass(frozen=True)
class DuplicateRow:
    task_id: str
    services: str
    rebuilds: str
    replacement_id: str
    replacement_title: str
    proving_work: str
    original_title: str


def numeric_task_id(task_id: str) -> int:
    match = re.match(r"\d+", task_id)
    if not match:
        raise ValueError(f"bad task id: {task_id}")
    return int(match.group(0))


def parse_task_blocks(source: Path) -> dict[str, TaskBlock]:
    blocks: dict[str, TaskBlock] = {}
    current: list[str] = []
    for line in source.read_text(encoding="utf-8").splitlines(keepends=True):
        if line.startswith("TASK "):
            if current:
                record = block_record(current)
                blocks[record.task_id] = record
            current = [line]
            continue
        if current and line.startswith("==="):
            record = block_record(current)
            blocks[record.task_id] = record
            current = []
            continue
        if current:
            current.append(line)
    if current:
        record = block_record(current)
        blocks[record.task_id] = record
    return blocks


def block_record(lines: list[str]) -> TaskBlock:
    block = "".join(lines).rstrip() + "\n"
    title = lines[0].strip()
    match = re.match(r"TASK (\d+[a-z]?)\b", title)
    if not match:
        raise ValueError(f"bad task header: {title}")
    return TaskBlock(match.group(1), title, block)


def duplicate_ids_from_4272(report: Path) -> list[str]:
    ids: list[str] = []
    for line in report.read_text(encoding="utf-8").splitlines():
        match = re.match(r"\| TASK (\d+[a-z]?) \| [^|]+ \| ([^|]+) \|", line)
        if not match:
            continue
        if match.group(2).strip() == ANSWER_DUPLICATE:
            ids.append(match.group(1))
    return sorted(ids, key=lambda value: (numeric_task_id(value), value))


def row_cells(line: str) -> list[str] | None:
    if not line.startswith("| TASK "):
        return None
    cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
    if len(cells) != 6:
        return None
    return cells


def parse_report(report: Path) -> dict[str, DuplicateRow]:
    rows: dict[str, DuplicateRow] = {}
    for line in report.read_text(encoding="utf-8").splitlines():
        cells = row_cells(line)
        if cells is None:
            continue
        task_match = re.fullmatch(r"TASK (\d+[a-z]?)", cells[0])
        restore_match = re.match(r"TASK (\d+[a-z]?)\b(?: - (.*))?$", cells[3])
        if not task_match:
            continue
        task_id = task_match.group(1)
        replacement_id = restore_match.group(1) if restore_match else ""
        replacement_title = restore_match.group(2) if restore_match and restore_match.group(2) else ""
        rows[task_id] = DuplicateRow(
            task_id=task_id,
            services=cells[1],
            rebuilds=cells[2],
            replacement_id=replacement_id,
            replacement_title=replacement_title,
            proving_work=cells[4],
            original_title=cells[5],
        )
    return rows


def render_report(source: Path, report_4272: Path) -> str:
    blocks = parse_task_blocks(source)
    duplicate_ids = duplicate_ids_from_4272(report_4272)
    lines = [
        "# TASK 4274 - duplicate tasks replaced by restore work",
        "",
        f"Source: `{report_4272}` rows whose answer is `{ANSWER_DUPLICATE}`.",
        "",
        "| task | services | what it would rebuild | restore task that already did it | proving work now | original title |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for task_id in duplicate_ids:
        expected = EXPECTED_ROWS[task_id]
        task = blocks[task_id]
        replacement = blocks[expected["replacement"]]
        services = service_label(task.block)
        lines.append(
            "| "
            + " | ".join(
                [
                    f"TASK {task_id}",
                    services,
                    expected["rebuilds"],
                    replacement.title,
                    expected["proving"],
                    task.title.replace("|", "\\|"),
                ]
            )
            + " |"
        )
    lines.append("")
    return "\n".join(lines)


def service_label(block: str) -> str:
    services: list[str] = []
    for service in ("X", "Instagram", "Messenger"):
        if re.search(rf"\b{service}\b", block):
            services.append(service)
    return ", ".join(services)


def verify(source: Path, report_4272: Path, report: Path) -> int:
    blocks = parse_task_blocks(source)
    duplicate_ids = duplicate_ids_from_4272(report_4272)
    report_rows = parse_report(report)
    duplicate_set = set(duplicate_ids)
    report_set = set(report_rows)
    expected_set = set(EXPECTED_ROWS)

    failures: list[str] = []
    missing_from_expected = sorted(duplicate_set - expected_set, key=lambda value: (numeric_task_id(value), value))
    stale_expected = sorted(expected_set - duplicate_set, key=lambda value: (numeric_task_id(value), value))
    missing = sorted(duplicate_set - report_set, key=lambda value: (numeric_task_id(value), value))
    extra = sorted(report_set - duplicate_set, key=lambda value: (numeric_task_id(value), value))

    if missing_from_expected:
        failures.append("4272 duplicate rows missing replacement rules: " + ", ".join(f"TASK {tid}" for tid in missing_from_expected))
    if stale_expected:
        failures.append("replacement rules are stale for non-duplicate rows: " + ", ".join(f"TASK {tid}" for tid in stale_expected))
    if missing:
        failures.append("report missing duplicate rows: " + ", ".join(f"TASK {tid}" for tid in missing))
    if extra:
        failures.append("report has extra rows: " + ", ".join(f"TASK {tid}" for tid in extra))

    no_replacement = 0
    proving_count = 0
    not_deleted_count = 0
    row_count = 0
    for task_id in duplicate_ids:
        row = report_rows.get(task_id)
        if row is None:
            continue
        row_count += 1
        expected = EXPECTED_ROWS[task_id]
        source_task = blocks.get(task_id)
        if source_task is not None:
            not_deleted_count += 1
        else:
            failures.append(f"TASK {task_id} was deleted from the task list")
        if not row.replacement_id:
            no_replacement += 1
            failures.append(f"TASK {task_id} has no replacement named")
        elif row.replacement_id != expected["replacement"]:
            failures.append(
                f"TASK {task_id} replacement TASK {row.replacement_id} should be TASK {expected['replacement']}"
            )
        elif row.replacement_id not in blocks:
            failures.append(f"TASK {task_id} replacement TASK {row.replacement_id} does not exist in the task list")
        if row.rebuilds != expected["rebuilds"]:
            failures.append(f"TASK {task_id} rebuild text changed or blank")
        if row.proving_work != expected["proving"] or not row.proving_work.startswith("Prove "):
            failures.append(f"TASK {task_id} is not changed into the expected proving work")
        else:
            proving_count += 1

    print(f"TASK4274_DUPLICATE_COUNT={len(duplicate_ids)}")
    print(f"TASK4274_REPORT_ROWS={row_count}")
    print(f"TASK4274_NO_REPLACEMENT_NAMED={no_replacement}")
    print(f"TASK4274_PROVING_WORK_COUNT={proving_count}")
    print(f"TASK4274_NOT_DELETED_COUNT={not_deleted_count}")

    for task_id in duplicate_ids:
        row = report_rows.get(task_id)
        if row is None:
            continue
        replacement = f"TASK {row.replacement_id}" if row.replacement_id else "<blank>"
        print(
            "TASK4274_REPLACEMENT="
            f"TASK {task_id} -> {replacement}; rebuilds {row.rebuilds}; proving {row.proving_work}"
        )

    if failures:
        for failure in failures:
            print(f"TASK4274_FAIL={failure}")
        return 1
    print("TASK4274_OK=green")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--from-4272", type=Path, default=DEFAULT_4272_REPORT)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    if args.write:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(render_report(args.source, args.from_4272), encoding="utf-8")
        duplicate_count = len(duplicate_ids_from_4272(args.from_4272))
        print(f"TASK4274_WROTE={args.report}")
        print(f"TASK4274_DUPLICATE_COUNT={duplicate_count}")
        return 0
    return verify(args.source, args.from_4272, args.report)


if __name__ == "__main__":
    sys.exit(main())
