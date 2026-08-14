#!/usr/bin/env python3
"""Read-only, evidence-backed re-verification of one or more OSL tasks.

The program deliberately has no output-file option.  It reads the plan and
evidence, checks any paths the evidence names, and writes its report only to
stdout.  Keeping the read-only boundary structural (rather than conditional)
is important: this is intended to audit a tick, never to change one.
"""

from __future__ import annotations

import argparse
import glob
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


DEFAULT_PLAN = Path("/home/liamw/osl-plan/OSL-AUDITS")
# This copy of the read-only engine lives in lane j. Resolve relative
# artifacts against this lane rather than another concurrently-running lane.
DEFAULT_WORKTREE = Path("/home/liamw/osl-exec-i")
VERDICTS = ("HOLDS", "FAILS", "CANNOT-DECIDE-MECHANICALLY")
WORD_NUMBERS = {
    "zero": 0, "one": 1, "two": 2, "three": 3, "four": 4,
    "five": 5, "six": 6, "seven": 7, "eight": 8, "nine": 9,
    "ten": 10,
}
STOP_WORDS = {
    "a", "an", "and", "are", "as", "at", "by", "can", "check", "each",
    "every", "for", "from", "in", "is", "it", "its", "makes", "no", "not",
    "of", "on", "or", "the", "this", "to", "when", "while", "with", "will",
}
TASK_INDEX: dict[Path, dict[str, tuple[Path, list[str], int, re.Match[str]]]] = {}


class InputAbsent(RuntimeError):
    pass


class EvidenceMissing(RuntimeError):
    def __init__(self, exact: Path, pattern: Path):
        self.exact = exact
        self.pattern = pattern
        super().__init__(f"missing evidence file: {exact} or {pattern}")


@dataclass(frozen=True)
class LocatedLine:
    text: str
    line: int


@dataclass(frozen=True)
class Task:
    task_id: str
    name: LocatedLine
    finish_line: LocatedLine
    done_lines: tuple[LocatedLine, ...]
    proof_lines: tuple[LocatedLine, ...]
    path: Path


@dataclass(frozen=True)
class ClauseResult:
    clause: str
    verdict: str
    reason: str


def require_available(name: str, starve: set[str]) -> None:
    if name in starve:
        raise InputAbsent(f"absent input: {name}")


def read_whole(path: Path, label: str, starve: set[str]) -> str:
    require_available(label, starve)
    if not path.is_file():
        raise InputAbsent(f"absent input: {label} ({path})")
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise InputAbsent(f"absent input: {label} ({path}): {exc}") from exc


def parse_task(task_id: str, plan_root: Path, starve: set[str]) -> Task:
    require_available("todo read", starve)
    if not re.fullmatch(r"[0-9]{4}[a-z]?", task_id):
        raise InputAbsent(f"absent input: task id {task_id!r} is not a task id")
    todo = plan_root / "todo"
    if not todo.is_dir():
        raise InputAbsent(f"absent input: todo read ({todo})")
    cache_key = todo.resolve()
    if cache_key not in TASK_INDEX:
        index: dict[str, tuple[Path, list[str], int, re.Match[str]]] = {}
        any_heading = re.compile(r"^TASK (\d{4}[a-z]?)(?: \[x\])? - (.+)$")
        for path in sorted(todo.glob("*")):
            if not path.is_file():
                continue
            lines = read_whole(path, "todo read", starve).splitlines()
            for line_number, line in enumerate(lines):
                match = any_heading.match(line)
                if match:
                    found_id = match.group(1)
                    if found_id in index:
                        raise InputAbsent(f"absent input: task {found_id} is ambiguous in todo")
                    index[found_id] = (path, lines, line_number, match)
        TASK_INDEX[cache_key] = index
    found = TASK_INDEX[cache_key].get(task_id)
    if found is None:
        raise InputAbsent(f"absent input: task {task_id} in todo")
    path, lines, start, match = found
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("TASK ")), len(lines))
    block = list(enumerate(lines[start:end], start=start + 1))
    finish = next(((text.removeprefix("done when: "), number)
                   for number, text in block if text.startswith("done when: ")), None)
    if finish is None:
        raise InputAbsent(f"absent input: done when for task {task_id}")
    done = tuple(LocatedLine(text, number) for number, text in block if text.startswith("done:"))
    proof = tuple(LocatedLine(text, number) for number, text in block if text.startswith("proof:"))
    return Task(
        task_id=task_id,
        name=LocatedLine(match.group(2), start + 1),
        finish_line=LocatedLine(finish[0], finish[1]),
        done_lines=done,
        proof_lines=proof,
        path=path,
    )


def resolve_evidence(task_id: str, plan_root: Path, starve: set[str]) -> Path:
    require_available("evidence read", starve)
    directory = plan_root / "evidence"
    exact = directory / f"{task_id}.md"
    matches = ([exact] if exact.is_file() else [])
    matches.extend(Path(item) for item in sorted(glob.glob(str(directory / f"{task_id}-*.md"))))
    # This preserves record-done.sh's exact-first behavior without opening any
    # candidate until a single result has been chosen.
    if not matches:
        raise EvidenceMissing(exact, directory / (task_id + "-*.md"))
    return matches[0]


def split_clauses(finish_line: str, starve: set[str]) -> list[str]:
    require_available("clause split", starve)
    text = re.sub(r"\s+", " ", finish_line.strip())
    # Semicolons and full stops are unambiguous finish-line boundaries.  The
    # remaining split handles the plan's recurrent independent "while", "and
    # starving", and comma-led acceptance arms while retaining simple phrases
    # such as "direct message, group chat, ..." as one clause.
    pieces = re.split(r"(?<=[.;])\s+", text)
    clauses: list[str] = []
    boundary = re.compile(
        r",\s+(?:and\s+)?(?=(?:while\s+)?(?:an?|the|every|all|one|direct|"
        r"starving|merely|firing|running|pointing|failure|a proven)\b)", re.I)
    for piece in pieces:
        clauses.extend(part.strip(" ,") for part in boundary.split(piece) if part.strip(" ,"))
    if not clauses:
        raise InputAbsent("absent input: clause split")
    return clauses


def word_numbers(text: str) -> set[int]:
    values = {int(item) for item in re.findall(r"\b\d+\b", text)}
    values.update(WORD_NUMBERS[item.lower()] for item in re.findall(r"\b[a-z]+\b", text.lower())
                  if item.lower() in WORD_NUMBERS)
    return values


def cited_artifacts(evidence: str, plan_root: Path, worktree: Path, starve: set[str]) -> list[tuple[str, bool]]:
    require_available("artifact check", starve)
    # Paths in prose/code that identify a file rather than a URL or a command
    # option.  A path must have an extension so `/tmp/scratch` is not misread as
    # an artifact claim.
    raw = re.findall(r"(?<!https:)(?<!http:)(?:/[A-Za-z0-9_.@*+-]+)+|(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.@*+-]+", evidence)
    seen: set[str] = set()
    result: list[tuple[str, bool]] = []
    for item in raw:
        item = item.rstrip(".,:;`)]}")
        if "*" in item or not re.search(r"\.[A-Za-z0-9]{1,10}$", item):
            continue
        # Source files named in a command explain how a check was run, but are
        # not the produced artifact that the evidence relies upon.  The latter
        # conventionally lives in an evidence/artifact/capture location or has
        # a data/capture extension.
        lowered = item.lower()
        if not (any(marker in lowered for marker in ("/evidence/", "/artifact", "/screenshot", "/capture"))
                or lowered.endswith((".json", ".png", ".jpg", ".jpeg", ".pdf", ".csv", ".log"))):
            continue
        if item in seen:
            continue
        seen.add(item)
        candidate = Path(item)
        if candidate.is_absolute():
            exists = candidate.exists()
        else:
            paths = [worktree / candidate, plan_root / candidate]
            # Evidence commands frequently `cd` into a package before naming a
            # source path.  A read-only basename/path lookup recognizes that
            # convention without treating the command's current directory as
            # an asserted plan location.
            exists = any(path.exists() for path in paths) or any(worktree.rglob(item))
        result.append((item, exists))
    return result


def evidence_mentions(clause: str, evidence: str) -> bool:
    words = [word.lower() for word in re.findall(r"[A-Za-z]{4,}", clause)
             if word.lower() not in STOP_WORDS]
    if not words:
        return True
    evidence_lower = evidence.lower()
    hits = sum(word in evidence_lower for word in set(words))
    return hits >= max(1, (len(set(words)) + 1) // 2)


def numeric_conflict(clause: str, evidence: str) -> str | None:
    required = word_numbers(clause)
    present = word_numbers(evidence)
    if not required:
        return None
    lower_clause = clause.lower()
    lower_evidence = evidence.lower()
    if "instagram" in lower_clause and 0 in required and re.search(r"\b(inventory|sources?|entries?|catalogue|catalog)\b", lower_clause):
        inventory = re.search(r"(?:lists requiring instagram|instagram entries|instagram[^\n]{0,50}(?:count|entries|lists))[^\n]*?=\s*(\d+)", lower_evidence)
        if inventory and int(inventory.group(1)) != 0:
            return f"inverted inventory count: required 0 Instagram entries; evidence shows {inventory.group(1)}"
        # TASK 4256's historical evidence uses this exact spelling.
        inventory = re.search(r"task4256_lists_requiring_instagram=(\d+)", lower_evidence)
        if inventory and int(inventory.group(1)) != 0:
            return f"inverted inventory count: required 0 Instagram entries; evidence shows {inventory.group(1)}"
    exact = re.search(r"\bexactly\s+(?:these\s+)?(\d+|" + "|".join(WORD_NUMBERS) + r")\b", lower_clause)
    if exact:
        wanted = WORD_NUMBERS.get(exact.group(1), int(exact.group(1)) if exact.group(1).isdigit() else -1)
        observed = re.search(r"\bcount\s*[=:]\s*(\d+)", lower_evidence)
        if observed and int(observed.group(1)) != wanted:
            return f"contradictory required count: required {wanted}; evidence shows {observed.group(1)}"
        named_kinds = re.search(r"\bexactly\s+(\d+|" + "|".join(WORD_NUMBERS) + r")\s+named kinds", lower_evidence)
        if named_kinds:
            observed_value = WORD_NUMBERS.get(named_kinds.group(1), int(named_kinds.group(1)) if named_kinds.group(1).isdigit() else -1)
            if observed_value != wanted:
                return f"contradictory required count: required {wanted}; evidence says exactly {observed_value}"
        for number in present:
            if number != wanted and re.search(rf"\bexactly\s+{number}\b", lower_evidence):
                return f"contradictory required count: required {wanted}; evidence says exactly {number}"
    return None


def evidence_records_run_arm(clause: str, evidence: str) -> bool:
    """Require the red result and the exercised surface on one evidence line.

    An unrelated ``exit 1`` (for example, a sibling search) must not prove an
    unnamed-build or starvation arm.  This is deliberately line-oriented: the
    evidence format records commands and their result on the same line.
    """
    anchors = {
        word.lower() for word in re.findall(r"[A-Za-z]{4,}", clause)
        if word.lower() not in STOP_WORDS
    }
    red_result = re.compile(r"\b(starv\w*|mutat\w*|exit\s*[=:]?\s*1|red proof)\b", re.I)
    return any(
        red_result.search(line) and any(anchor in line.lower() for anchor in anchors)
        for line in evidence.splitlines()
    )


def assess_clause(clause: str, evidence: str, missing_artifacts: list[str]) -> ClauseResult:
    lower = clause.lower()
    conflict = numeric_conflict(clause, evidence)
    if conflict:
        return ClauseResult(clause, "FAILS", conflict)
    # A quoted user-facing sentence such as "could not be sent" is not a
    # failed verification run.  Treat only an explicit unmet result, or a
    # report that says a command could not start/run/compile/execute, as a
    # failed evidence surface.
    if re.search(r"\b(not met|not achieved|0 real (?:card|bitcoin|monero|payment))\b|\bcould not\b[^.\n]{0,100}\b(?:start|run|compile|execut)\w*", evidence, re.I):
        if evidence_mentions(clause, evidence):
            return ClauseResult(clause, "FAILS", "evidence explicitly records the required result as not met")
    if missing_artifacts and evidence_mentions(clause, evidence):
        return ClauseResult(clause, "FAILS", "evidence cites absent artifact(s): " + ", ".join(missing_artifacts))
    is_run_arm = bool(re.search(r"\b(starving|merely hiding|makes .*check(?: exit 1| fail)|makes .* exit 1|firing at)\b", lower))
    if is_run_arm:
        if evidence_records_run_arm(clause, evidence):
            return ClauseResult(clause, "HOLDS", "evidence records the required red/run arm")
        return ClauseResult(clause, "CANNOT-DECIDE-MECHANICALLY", "starvation arm was never run")
    if evidence_mentions(clause, evidence):
        return ClauseResult(clause, "HOLDS", "evidence addresses the clause")
    return ClauseResult(clause, "FAILS", "evidence does not address this clause")


def overall(results: Iterable[ClauseResult]) -> str:
    values = {result.verdict for result in results}
    if "FAILS" in values:
        return "FAILS"
    if "CANNOT-DECIDE-MECHANICALLY" in values:
        return "CANNOT-DECIDE-MECHANICALLY"
    return "HOLDS"


def report(task: Task, evidence_path: Path, evidence: str, clauses: list[str], artifacts: list[tuple[str, bool]]) -> str:
    missing = [item for item, exists in artifacts if not exists]
    results = [assess_clause(clause, evidence, missing) for clause in clauses]
    lines = [
        f"TASK {task.task_id}: {task.name.text}",
        f"TASK SOURCE: {task.path}:{task.name.line}",
        f"FINISH LINE: {task.path}:{task.finish_line.line}",
        f"EVIDENCE SOURCE: {evidence_path}:1",
    ]
    for item in task.done_lines:
        lines.append(f"DONE: {task.path}:{item.line}: {item.text}")
    for item in task.proof_lines:
        lines.append(f"PROOF: {task.path}:{item.line}: {item.text}")
    lines.extend(["CLAUSES:", "| clause | result | reason |", "|---|---|---|"])
    for result in results:
        lines.append(f"| {result.clause} | {result.verdict} | {result.reason} |")
    lines.append("ARTIFACTS:")
    if artifacts:
        lines.extend(f"| {path} | {'exists' if exists else 'MISSING'} |" for path, exists in artifacts)
    else:
        lines.append("| no file artifact paths cited | n/a |")
    lines.append(f"VERDICT: {overall(results)}")
    return "\n".join(lines)


def report_missing_evidence(task: Task, clauses: list[str], missing: EvidenceMissing) -> str:
    reason = str(missing)
    results = [ClauseResult(clause, "CANNOT-DECIDE-MECHANICALLY", reason) for clause in clauses]
    lines = [
        f"TASK {task.task_id}: {task.name.text}",
        f"TASK SOURCE: {task.path}:{task.name.line}",
        f"FINISH LINE: {task.path}:{task.finish_line.line}",
        f"EVIDENCE SOURCE: MISSING {missing.exact} or {missing.pattern}",
    ]
    for item in task.done_lines:
        lines.append(f"DONE: {task.path}:{item.line}: {item.text}")
    for item in task.proof_lines:
        lines.append(f"PROOF: {task.path}:{item.line}: {item.text}")
    lines.extend(["CLAUSES:", "| clause | result | reason |", "|---|---|---|"])
    for result in results:
        lines.append(f"| {result.clause} | {result.verdict} | {result.reason} |")
    lines.extend(["ARTIFACTS:", "| evidence file absent; artifact check unavailable | n/a |", "VERDICT: CANNOT-DECIDE-MECHANICALLY"])
    return "\n".join(lines)


def handle_trace(plan_root: Path) -> str:
    """Report an observed Linux handle trace, after all input reads are closed."""
    watched = (plan_root / "todo").resolve(), (plan_root / "evidence").resolve()
    writable: list[str] = []
    observed: list[str] = []
    fd_dir = Path("/proc/self/fd")
    for entry in fd_dir.iterdir():
        try:
            target = Path(os.readlink(entry)).resolve()
            if not any(target == root or root in target.parents for root in watched):
                continue
            flags = (Path("/proc/self/fdinfo") / entry.name).read_text(encoding="utf-8")
            flag_line = next(line for line in flags.splitlines() if line.startswith("flags:"))
            mode = int(flag_line.split()[1], 8)
            observed.append(f"fd={entry.name}:{target}")
            if mode & (os.O_WRONLY | os.O_RDWR):
                writable.append(f"fd={entry.name}:{target}")
        except (FileNotFoundError, OSError, StopIteration, ValueError):
            continue
    return ("HANDLE TRACE /proc/self/fd "
            f"observed_under_todo_or_evidence={len(observed)} "
            f"writable_under_todo_or_evidence={len(writable)}" +
            (" paths=" + ",".join(writable) if writable else ""))


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("task_ids", nargs="+", help="one or more four-digit task ids")
    parser.add_argument("--plan-root", type=Path, default=DEFAULT_PLAN)
    parser.add_argument("--worktree", type=Path, default=DEFAULT_WORKTREE)
    parser.add_argument("--starve", action="append", choices=("todo read", "evidence read", "clause split", "artifact check"), default=[])
    parser.add_argument("--trace-handles", action="store_true", help="append an observed /proc handle trace")
    args = parser.parse_args(argv)
    starve = set(args.starve)
    starve.update(filter(None, os.environ.get("OSL_REVERIFY_STARVE", "").split(",")))
    reports: list[str] = []
    try:
        for task_id in args.task_ids:
            task = parse_task(task_id, args.plan_root, starve)
            clauses = split_clauses(task.finish_line.text, starve)
            try:
                evidence_path = resolve_evidence(task_id, args.plan_root, starve)
            except EvidenceMissing as missing:
                reports.append(report_missing_evidence(task, clauses, missing))
                continue
            evidence = read_whole(evidence_path, "evidence read", starve)
            artifacts = cited_artifacts(evidence, args.plan_root, args.worktree, starve)
            reports.append(report(task, evidence_path, evidence, clauses, artifacts))
    except InputAbsent as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1
    print("\n\n".join(reports))
    if args.trace_handles:
        print(handle_trace(args.plan_root))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
