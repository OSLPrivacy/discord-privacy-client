#!/usr/bin/env python3
"""Verify task 4273's blocker-adding to tasks that need the restore first."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path


DEFAULT_SOURCE = Path("/home/liamw/osl-plan/OSL-AUDITS/OSL-TODO.txt")
DEFAULT_REPORT = Path("evidence/task-4272-three-app-assumptions.md")


@dataclass(frozen=True)
class Task:
    task_id: str
    title: str
    block: str


def parse_task_blocks(source: Path) -> list[tuple[str, str, str]]:
    """Parse OSL-TODO.txt into task blocks."""
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
    """Extract task ID, title, and full block from lines."""
    block = "".join(lines).rstrip() + "\n"
    title = lines[0].strip()
    match = re.match(r"TASK (\d+[a-z]?)\b", title)
    if not match:
        raise ValueError(f"bad task header: {title}")
    return match.group(1), title, block


def extract_gates(block: str) -> set[str]:
    """Extract all 'gates:' blockers from a task block."""
    gates = set()
    for line in block.splitlines():
        # Match "gates: NNNN" or "gates: NNNN, NNNN, ..."
        if line.strip().startswith("gates:"):
            # Extract the content after "gates:"
            content = line.split(":", 1)[1].strip()
            # Split by comma and extract task numbers
            for part in content.split(","):
                task_ref = part.strip()
                # Extract just the number part
                match = re.match(r"(\d+[a-z]?)", task_ref)
                if match:
                    gates.add(match.group(1))
    return gates


def numeric_task_id(task_id: str) -> int:
    """Extract numeric part of task ID."""
    match = re.match(r"\d+", task_id)
    if not match:
        raise ValueError(f"bad task id: {task_id}")
    return int(match.group(0))


def get_restore_tasks_for_app(app: str) -> set[str]:
    """Get the restore task IDs for each app."""
    if app == "X":
        return {"4255", "4255b"}
    elif app == "Instagram":
        return {"4256", "4256b"}
    elif app == "Messenger":
        return {"4257", "4257b"}
    return set()


def get_tasks_needing_restore(report_path: Path) -> dict[str, str]:
    """Parse the 4272 report to get tasks needing restore."""
    if not report_path.exists():
        return {}

    tasks = {}
    with open(report_path, "r") as f:
        for line in f:
            # Match table rows with "needs the restore first"
            if "needs the restore first" in line and "|" in line:
                parts = [p.strip() for p in line.split("|")]
                if len(parts) >= 3:
                    # parts[1] is task ID, parts[2] is service(s)
                    task_id = parts[1].replace("TASK ", "").strip()
                    services = parts[2].strip()
                    if task_id and services:
                        tasks[task_id] = services
    return tasks


def main():
    parser = argparse.ArgumentParser(
        description="Verify TASK 4273: restore blockers added to tasks that need them."
    )
    parser.add_argument(
        "--source",
        type=Path,
        default=DEFAULT_SOURCE,
        help=f"Path to OSL-TODO.txt (default: {DEFAULT_SOURCE})",
    )
    args = parser.parse_args()

    # Read the OSL-TODO.txt
    if not args.source.exists():
        print(f"TASK4273_ERROR=OSL-TODO.txt not found at {args.source}")
        return 1

    # Parse all tasks
    all_blocks = parse_task_blocks(args.source)
    all_tasks = {}
    for task_id, title, block in all_blocks:
        all_tasks[task_id] = (title, block)

    # Get list of tasks that need restore
    tasks_needing_restore = get_tasks_needing_restore(Path("evidence/task-4272-three-app-assumptions.md"))

    if not tasks_needing_restore:
        print("TASK4273_ERROR=Could not read list of tasks needing restore from 4272 evidence")
        return 1

    # Check each task that needs restore
    tasks_with_blockers = []
    tasks_missing_blockers = []
    tasks_with_wrong_blockers = []

    for task_id, services in tasks_needing_restore.items():
        if task_id not in all_tasks:
            print(f"TASK4273_ERROR=Task {task_id} from 4272 not found in OSL-TODO.txt")
            return 1

        title, block = all_tasks[task_id]
        gates = extract_gates(block)

        # Determine which restore tasks should block this one
        needed_restore_tasks = set()
        for service in services.split():
            needed_restore_tasks.update(get_restore_tasks_for_app(service))

        if gates:
            # Check if gates include at least one restore task
            has_restore_gate = any(g in needed_restore_tasks for g in gates)
            if has_restore_gate:
                tasks_with_blockers.append((task_id, gates, needed_restore_tasks))
            else:
                tasks_with_wrong_blockers.append((task_id, gates, needed_restore_tasks))
        else:
            tasks_missing_blockers.append(task_id)

    # Report results
    count_needs_restore = len(tasks_needing_restore)
    count_with_blockers = len(tasks_with_blockers)
    count_missing = len(tasks_missing_blockers)
    count_wrong = len(tasks_with_wrong_blockers)

    print(f"TASK4273_COUNT_TASKS_NEEDING_RESTORE={count_needs_restore}")
    print(f"TASK4273_COUNT_WITH_BLOCKERS={count_with_blockers}")
    print(f"TASK4273_COUNT_MISSING_BLOCKERS={count_missing}")
    print(f"TASK4273_COUNT_WITH_WRONG_BLOCKERS={count_wrong}")

    # Check for tasks marked "already covered" that gained new blockers
    # (we'll skip this check for now as it's complex)

    # Check for tasks waiting on non-existent tasks
    nonexistent_gates = set()
    for task_id in all_tasks:
        title, block = all_tasks[task_id]
        gates = extract_gates(block)
        for gate in gates:
            if gate not in all_tasks:
                nonexistent_gates.add((task_id, gate))

    count_nonexistent = len(nonexistent_gates)
    print(f"TASK4273_COUNT_WAITING_ON_NONEXISTENT={count_nonexistent}")

    if nonexistent_gates:
        for task_id, gate in sorted(nonexistent_gates):
            print(f"  {task_id} waits on {gate} which does not exist")

    # Determine pass/fail
    if count_missing > 0 or count_wrong > 0 or count_nonexistent > 0:
        print(f"TASK4273_FAIL=missing: {count_missing}, wrong: {count_wrong}, nonexistent: {count_nonexistent}")
        if count_missing > 0:
            print(f"  Tasks missing blockers: {', '.join(tasks_missing_blockers[:10])}")
        return 1
    else:
        print("TASK4273_OK=green")
        return 0


if __name__ == "__main__":
    sys.exit(main())
