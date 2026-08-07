#!/usr/bin/env python3
"""Check live OSL audit tasks wait for the setup tasks they consume.

The OSL-AUDITS todo corpus is authored as plain text. This checker parses every
task block, follows transitive `gates:` edges, and refuses setup-consuming live
tasks that can run before their required setup task.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import deque
from dataclasses import dataclass, field
from pathlib import Path


DEFAULT_TODO_DIR = Path("/home/liamw/osl-plan/OSL-AUDITS/todo")

TASK_RE = re.compile(r"^TASK ([0-9]{4}[a-z]{0,2})(?: \[x\])? - (.*\S)\s*$")
FIELD_RE = re.compile(r"^(gates|build|who|run by|do|done when|done|proof|blocked)\s*:\s*(.*)$")
GATE_ID_RE = re.compile(r"[0-9]{4}[a-z]{0,2}")
NO_GATES = {"", "-", "none", "n/a", "na", "nothing"}


@dataclass
class Task:
    task_id: str
    title: str
    source: Path
    line: int
    fields: dict[str, str] = field(default_factory=dict)
    gates: list[str] = field(default_factory=list)

    def searchable_text(self) -> str:
        fields = [
            self.title,
            self.fields.get("do", ""),
            self.fields.get("done when", ""),
        ]
        return "\n".join(part for part in fields if part).lower()


@dataclass(frozen=True)
class CheckRule:
    name: str
    target: str


RULES = (
    CheckRule("real-account", "0004"),
    CheckRule("real-browser", "1201"),
    CheckRule("two-machine", "0033"),
)


def parse_tasks(todo_dir: Path) -> dict[str, Task]:
    tasks: dict[str, Task] = {}
    for path in sorted(todo_dir.glob("[0-9][0-9]-*.txt")):
        if path.name.startswith("00-"):
            continue
        current: Task | None = None
        for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            task_match = TASK_RE.match(line)
            if task_match:
                task_id, title = task_match.groups()
                if task_id in tasks:
                    prior = tasks[task_id]
                    raise ValueError(
                        f"duplicate task {task_id}: {path}:{line_number} and {prior.source}:{prior.line}"
                    )
                current = Task(task_id=task_id, title=title, source=path, line=line_number)
                tasks[task_id] = current
                continue

            if current is None:
                continue
            field_match = FIELD_RE.match(line)
            if not field_match:
                continue
            name, value = field_match.groups()
            current.fields.setdefault(name, value.strip())
            if name == "gates" and not current.gates:
                gates_text = value.strip()
                if gates_text.lower() not in NO_GATES:
                    current.gates = GATE_ID_RE.findall(gates_text)

    if not tasks:
        raise ValueError(f"no tasks parsed from {todo_dir}")
    return tasks


def reaches(tasks: dict[str, Task], start: str, target: str) -> bool:
    queue: deque[str] = deque(tasks[start].gates)
    seen: set[str] = set()
    while queue:
        task_id = queue.popleft()
        if task_id == target:
            return True
        if task_id in seen:
            continue
        seen.add(task_id)
        task = tasks.get(task_id)
        if task is not None:
            queue.extend(task.gates)
    return False


def setup_producer(task: Task) -> bool:
    """Tasks that create the setup are not themselves setup consumers."""
    text = task.searchable_text()
    do = task.fields.get("do", "").strip().lower()
    title = task.title.lower()
    if task.task_id in {"0004", "0033", "1200", "1201", "3437"}:
        return True
    if do.startswith(("sign in to ", "start one ", "log in to ")):
        return True
    if title.startswith((
        "start ",
        "make the website driver use a real browser",
        "name the real website driver jobs",
    )):
        return True
    return "fixture" in text or "sample" in text or "seeded " in text


def consumes_real_account(task: Task) -> bool:
    text = task.searchable_text()
    if setup_producer(task):
        return False
    return any(
        re.search(pattern, text)
        for pattern in (
            r"\breal signed-in app\b",
            r"\balready signed in\b",
            r"\brun names the two signed-in accounts\b",
            r"\bwith two real [a-z0-9 .-]+ accounts on two machines\b",
            r"\btwo real mailboxes\b",
            r"\breal protected message from another person\b",
        )
    )


def consumes_real_browser(task: Task) -> bool:
    text = task.searchable_text()
    if setup_producer(task):
        return False
    return any(
        phrase in text
        for phrase in (
            "real browser using",
            "real website driver",
            "reports a real driver",
            "direct driver command opens a local test page",
        )
    )


def consumes_two_machine(task: Task) -> bool:
    text = task.searchable_text()
    if setup_producer(task):
        return False
    if not re.search(r"\b(two machines|two-machine|two windows machines)\b", text):
        return False
    return bool(
        re.search(
            r"\b(protected message|private message|timed message|timer guarantee|story timer|"
            r"view[- ]once|burn guarantee|mailboxes|two-copy command|both people)\b",
            text,
        )
    )


def matching_tasks(tasks: dict[str, Task], rule: CheckRule) -> list[Task]:
    predicate = {
        "real-account": consumes_real_account,
        "real-browser": consumes_real_browser,
        "two-machine": consumes_two_machine,
    }[rule.name]
    return [task for task in tasks.values() if predicate(task)]


def validate(tasks: dict[str, Task]) -> tuple[dict[str, int], list[str]]:
    checked: dict[str, int] = {}
    failures: list[str] = []
    for rule in RULES:
        matched = matching_tasks(tasks, rule)
        checked[rule.name] = len(matched)
        for task in matched:
            if not reaches(tasks, task.task_id, rule.target):
                failures.append(
                    f"TASK {task.task_id} lacks path to {rule.target} "
                    f"for {rule.name}: {task.source.name}:{task.line} {task.title}"
                )
    return checked, failures


def print_report(tasks: dict[str, Task], checked: dict[str, int], failures: list[str]) -> None:
    print(f"tasks parsed: {len(tasks)}")
    for rule in RULES:
        missing = sum(1 for failure in failures if f"for {rule.name}:" in failure)
        print(
            f"{missing} {rule.name} tasks lack a path to {rule.target} "
            f"(checked {checked.get(rule.name, 0)})"
        )
    for failure in failures:
        print(f"FAIL {failure}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--todo-dir",
        type=Path,
        default=DEFAULT_TODO_DIR,
        help="OSL-AUDITS/todo directory to scan",
    )
    args = parser.parse_args(argv)

    try:
        tasks = parse_tasks(args.todo_dir)
        checked, failures = validate(tasks)
        print_report(tasks, checked, failures)
    except Exception as exc:
        print(f"ERROR {exc}", file=sys.stderr)
        return 2
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
