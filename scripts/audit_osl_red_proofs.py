#!/usr/bin/env python3
"""Audit OSL plan test tasks for red-proof coverage."""

from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path


TASK_RE = re.compile(r"^TASK\s+(\S+)(?:\s+\[x\])?\s+-\s+(.*)$")
FIELD_RE = re.compile(
    r"^(gates|build|who|run by|do|done when|done|proof|blocked):\s*(.*)$"
)

DONE_WHEN_BREAK_RE = re.compile(
    r"""
    \b(?:goes|turns|go)\s+red\b|
    \bcan\s+fail\b|
    \bcheck\s+fails\b|
    \bstays?\s+open\s+naming\b|
    \bfails\s+(?:once|after|with|and\s+names?|naming)\b|
    \bfail\s+naming\b|
    \bexit(?:s)?\s+1(?:,|\s+(?:and\s+names?|and\s+says|naming|because|before|for|instead|rather|reporting|saying|prints?))|
    \bexit(?:s)?\s+non[- ]?zero\b|
    \bthrowaway\b|
    \btest\s+copy\b|
    \bbroken\s+copy\b|
    \binjected\b|
    \bclear\s+failure\b|
    \b(?:changed|missing|closed|hidden|wrong|bad|stale|fake|invalid|non-[a-z]+)\b.{0,180}\b(?:refused|rejected|blocked)\b|
    \bis\s+refused\s+as\b|
    \brefused\s+by\s+name\b|
    \baltered\b.{0,120}\bfails\b|
    \b(?:hand-made|made-up|incomplete|wrong|fake|missing|blank|empty|duplicate|unsafe|stale|expired|absent|removed|changed|bad|mismatched|not-found|surviving|leftover|unnamed)\b.{0,180}\bexit(?:s)?(?:\s+with)?\s+(?:code\s+)?1\b|
    \bmake(?:s)?\s+(?:the\s+|its\s+|this\s+)?(?:check|test|comparison|capture|task|list|guide\s+check|list\s+check)\s+(?:fail|exit\s+1|exit\s+code\s+1|non[- ]?zero)\b|
    \b(?:swapping|removing|deleting|disabling|setting|turning|killing|stopping|copying|sharing|adding|reversing|allowing|changing|blanking|renaming|dropping|omitting|replacing|feeding|making|restoring)\b.{0,180}\bmake(?:s)?\b.{0,120}\b(?:fail|exit\s+1|exit\s+code\s+1|non[- ]?zero)\b|
    \b(?:wrong|fake|missing|blank|empty|duplicate|unsafe|stale|expired|absent|removed|changed|bad|mismatched|not-found|surviving|leftover|hand-made|unnamed)\b.{0,160}\bmake(?:s)?\b.{0,120}\b(?:fail|exit\s+1|exit\s+code\s+1|non[- ]?zero)\b
    """,
    re.IGNORECASE | re.VERBOSE,
)

TITLE_RED_RE = re.compile(
    r"\bprove\b.{0,120}\b(?:can fail|red|goes red|turns red)\b",
    re.IGNORECASE,
)

SIBLING_RED_RE = re.compile(
    r"\b(?:prove|break)\b.{0,120}\b(?:fail|red)\b",
    re.IGNORECASE,
)


@dataclass(frozen=True)
class Task:
    id: str
    title: str
    file: Path
    line: int
    fields: dict[str, str]

    @property
    def location(self) -> str:
        return f"{self.file.name}:{self.line}"

    @property
    def heading(self) -> str:
        return f"TASK {self.id} - {self.title}"

    @property
    def done_when(self) -> str:
        return self.fields.get("done when", "")


def parse_tasks(todo_dir: Path) -> list[Task]:
    tasks: list[Task] = []
    for path in sorted(todo_dir.glob("[0-9][0-9]-*.txt")):
        current: dict[str, object] | None = None
        last_field: str | None = None
        for line_number, line in enumerate(path.read_text().splitlines(), 1):
            task_match = TASK_RE.match(line)
            if task_match:
                if current is not None:
                    tasks.append(
                        Task(
                            id=current["id"],  # type: ignore[arg-type]
                            title=current["title"],  # type: ignore[arg-type]
                            file=current["file"],  # type: ignore[arg-type]
                            line=current["line"],  # type: ignore[arg-type]
                            fields=current["fields"],  # type: ignore[arg-type]
                        )
                    )
                current = {
                    "id": task_match.group(1),
                    "title": task_match.group(2),
                    "file": path,
                    "line": line_number,
                    "fields": {},
                }
                last_field = None
                continue

            if current is None:
                continue

            field_match = FIELD_RE.match(line)
            fields = current["fields"]  # type: ignore[assignment]
            if field_match:
                last_field = field_match.group(1)
                fields[last_field] = field_match.group(2)
            elif last_field in {"do", "done when"} and line.strip():
                fields[last_field] = f"{fields[last_field]} {line.strip()}"

        if current is not None:
            tasks.append(
                Task(
                    id=current["id"],  # type: ignore[arg-type]
                    title=current["title"],  # type: ignore[arg-type]
                    file=current["file"],  # type: ignore[arg-type]
                    line=current["line"],  # type: ignore[arg-type]
                    fields=current["fields"],  # type: ignore[arg-type]
                )
            )
    return tasks


def done_when_names_break(task: Task) -> bool:
    if DONE_WHEN_BREAK_RE.search(task.done_when):
        return True
    return bool(
        TITLE_RED_RE.search(task.title)
        and re.search(r"\b(exit|fail|failed|red)\b", task.done_when, re.IGNORECASE)
    )


def has_red_sibling(task: Task, by_id: dict[str, Task]) -> bool:
    if task.id.endswith("b"):
        return False
    sibling = by_id.get(f"{task.id}b")
    if sibling is None:
        return False
    return done_when_names_break(sibling) or bool(SIBLING_RED_RE.search(sibling.title))


def build_report(todo_dir: Path, output: Path | None) -> tuple[str, dict[str, int]]:
    tasks = parse_tasks(todo_dir)
    by_id = {task.id: task for task in tasks}
    test_tasks = [task for task in tasks if task.fields.get("build") == "test"]

    rows = []
    with_red_proof = []
    sibling_count = 0
    done_when_count = 0
    overlap_count = 0

    for task in test_tasks:
        sibling = has_red_sibling(task, by_id)
        direct = done_when_names_break(task)
        if sibling:
            sibling_count += 1
        if direct:
            done_when_count += 1
        if sibling and direct:
            overlap_count += 1
        if sibling or direct:
            with_red_proof.append(task)
        else:
            rows.append(task)

    counts = {
        "source_files": len(list(todo_dir.glob("[0-9][0-9]-*.txt"))),
        "all_tasks": len(tasks),
        "test_tasks": len(test_tasks),
        "with_red_proof": len(with_red_proof),
        "without_red_proof": len(rows),
        "paired_red_sibling": sibling_count,
        "done_when_break": done_when_count,
        "overlap": overlap_count,
    }
    counts["sum_check"] = counts["with_red_proof"] + counts["without_red_proof"]

    now = datetime.now(timezone.utc).isoformat(timespec="seconds")
    report = [
        "# TASK 3747 - test tasks with no red proof",
        "",
        f"Generated: {now}",
        f"Source: `{todo_dir}/[0-9][0-9]-*.txt`",
        "",
        "## What I Ran",
        "",
        "```sh",
        f"python3 scripts/audit_osl_red_proofs.py --output {output if output is not None else '<output>'}",
        "```",
        "",
        "## What It Printed",
        "",
        "```text",
        f"source_files={counts['source_files']}",
        f"all_tasks={counts['all_tasks']}",
        f"test_tasks={counts['test_tasks']}",
        f"with_red_proof={counts['with_red_proof']}",
        f"without_red_proof={counts['without_red_proof']}",
        f"sum_check={counts['with_red_proof']}+{counts['without_red_proof']}={counts['sum_check']}",
        f"wrote={output if output is not None else '<output>'}",
        "```",
        "",
        "## Rule",
        "",
        "A `build: test` task is counted as having a red proof when either:",
        "",
        "1. its paired task `<id>b` exists and is itself written as a red proof, or",
        "2. its own `done when:` line names a deliberate break/failure condition.",
        "",
        "Plain expected refusals, measurements, screenshots, and ordinary pass/fail checks are not counted unless they also name the break that makes the checker fail.",
        "",
        "## Counts",
        "",
        f"- Source files read: {counts['source_files']}",
        f"- Total task headings read: {counts['all_tasks']}",
        f"- Total `build: test` tasks: {counts['test_tasks']}",
        f"- Test tasks with a red proof: {counts['with_red_proof']}",
        f"- Test tasks without a red proof: {counts['without_red_proof']}",
        f"- Arithmetic check: {counts['with_red_proof']} + {counts['without_red_proof']} = {counts['sum_check']}",
        "",
        "Breakdown, not additive because categories can overlap:",
        "",
        f"- Covered by paired red-proof sibling: {counts['paired_red_sibling']}",
        f"- Covered by own `done when:` break line: {counts['done_when_break']}",
        f"- Covered by both: {counts['overlap']}",
        "",
        "## Finish Line",
        "",
        f"- Total number of test tasks: {counts['test_tasks']}",
        f"- Number with a red proof: {counts['with_red_proof']}",
        f"- Named list of those without: {counts['without_red_proof']} entries below",
        f"- Counts add up to total: {counts['with_red_proof']} + {counts['without_red_proof']} = {counts['sum_check']}",
        f"- Saved list for 3748: `{output if output is not None else '<output>'}`",
        "",
        "## Without Red Proof",
        "",
    ]
    for task in rows:
        report.append(f"- `{task.id}` - {task.title} ({task.location})")

    text = "\n".join(report) + "\n"
    if output is not None:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text)
    return text, counts


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--todo-dir",
        type=Path,
        default=Path("/home/liamw/osl-plan/OSL-AUDITS/todo"),
    )
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()

    _, counts = build_report(args.todo_dir, args.output)
    print(f"source_files={counts['source_files']}")
    print(f"all_tasks={counts['all_tasks']}")
    print(f"test_tasks={counts['test_tasks']}")
    print(f"with_red_proof={counts['with_red_proof']}")
    print(f"without_red_proof={counts['without_red_proof']}")
    print(
        f"sum_check={counts['with_red_proof']}+{counts['without_red_proof']}={counts['sum_check']}"
    )
    if args.output is not None:
        print(f"wrote={args.output}")
    return 0 if counts["sum_check"] == counts["test_tasks"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
