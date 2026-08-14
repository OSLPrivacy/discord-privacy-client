#!/usr/bin/env python3
"""TASK 7112's D26/D27 re-verification sweep.

This is intentionally a separate, fail-closed caller around the 7100 engine:
the engine assesses the current plan/evidence clauses, while this caller adds
the owner-required screen receipt checks before writing the 7102 ledger.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from collections import Counter
from datetime import date
from pathlib import Path

from reverify_task import (EvidenceMissing, assess_clause, cited_artifacts,
                           overall, parse_task, read_whole, resolve_evidence,
                           split_clauses)
from reverification_ledger import (count_rows, default_ledger, load_document,
                                   require_ledger_path, validate_document,
                                   validate_sweep, write_document)


ROOT = Path(__file__).resolve().parents[1]
PLAN = Path("/home/liamw/osl-plan/OSL-AUDITS")
PLAN_REPO = Path("/home/liamw/osl-plan")
TARGETS = ("03-onboarding-security.txt", "31-redesign-scope.txt")
TICKED = re.compile(r"^TASK (\d{4}[a-z]?) \[x\] - (.+)$")
TASK = re.compile(r"^TASK (\d{4}[a-z]?)\b", re.M)
DESIGN_PAGE = re.compile(r"\b([A-Z][A-Za-z0-9 _-]*\.dc\.html)\b")
SCREEN = re.compile(r"\b(?:screenshot|screen[ -]capture|fixed capture|copyfromscreen)\b", re.I)
STRUCTURE = re.compile(r"\b(?:structural (?:control|comparison|inventory)|control[- ]and[- ]text|control inventory)\b", re.I)
PIXEL = re.compile(r"(?:pixel difference|percent_different|percent different)\D{0,32}([0-9]+(?:\.[0-9]+)?)%?", re.I)
PRE_HISTORY_CAVEAT = "The mtime method's historical result is a floor:"


class SweepError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise SweepError(message)


def fingerprint(root: Path) -> str:
    if not root.is_dir():
        fail(f"absent input: before-and-after comparison ({root})")
    digest = hashlib.sha256()
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        digest.update(str(path.relative_to(root)).encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def target_plan_fingerprint(plan_root: Path) -> str:
    """Fingerprint the two plan inputs this sweep is authorized to read.

    Other lanes intentionally work on sibling plan files concurrently; those
    writes cannot be attributed to this read-only D26/D27 batch.  A change to
    either supplied plan file is still an immediate hard refusal.
    """
    digest = hashlib.sha256()
    for name in TARGETS:
        path = plan_root / "todo" / name
        if not path.is_file():
            fail(f"absent input: before-and-after comparison ({path})")
        digest.update(name.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def target_ids(plan_root: Path) -> list[str]:
    ids: list[str] = []
    for name in TARGETS:
        path = plan_root / "todo" / name
        if not path.is_file():
            fail(f"absent input: target plan file ({path})")
        for line in path.read_text(encoding="utf-8").splitlines():
            matched = TICKED.match(line)
            if matched:
                ids.append(matched.group(1))
    if len(ids) != len(set(ids)):
        fail("absent input: live tick census (duplicate task id)")
    if not ids:
        fail("absent input: live tick census")
    return ids


def current_block(task_id: str, plan_root: Path) -> tuple[str, str]:
    for name in TARGETS:
        text = (plan_root / "todo" / name).read_text(encoding="utf-8")
        starts = list(TASK.finditer(text))
        for index, match in enumerate(starts):
            if match.group(1) == task_id:
                return text[match.start():starts[index + 1].start() if index + 1 < len(starts) else len(text)], name
    fail(f"absent input: task block {task_id}")


def screen_receipt(task_id: str, plan_root: Path, starve: set[str]) -> dict | None:
    block, _ = current_block(task_id, plan_root)
    heading = block.splitlines()[0]
    if not SCREEN.search(heading):
        return None
    if "design-page check" in starve:
        fail("absent input: design-page check")
    try:
        source = resolve_evidence(task_id, plan_root, set())
        evidence = read_whole(source, "evidence read", set())
    except EvidenceMissing:
        evidence = ""
        source = None
    pages = DESIGN_PAGE.findall(evidence)
    page = pages[0] if pages else None
    structural = bool(STRUCTURE.search(evidence))
    pixels = [float(value) for value in PIXEL.findall(evidence)]
    under_one = any(value < 1 for value in pixels)
    result = {"task_id": task_id, "screen": True, "design_page": page,
              "structural_control_and_text_comparison": structural,
              "pixel_difference_under_1_percent": under_one,
              "evidence": str(source) if source else "MISSING"}
    if not page:
        result["failure_clause"] = "D26(d): screen task names no design reference page"
        result["failure_reason"] = "no named design page in the selected evidence"
    elif not structural:
        result["failure_clause"] = "D26: structural control-and-text comparison is required before pixel comparison"
        result["failure_reason"] = f"design page {page} is named but no structural control-and-text comparison is recorded"
    elif not under_one:
        result["failure_clause"] = "D27(a): exact pixel difference under 1 percent is required after structural comparison"
        result["failure_reason"] = f"design page {page} has no recorded pixel difference below 1 percent"
    return result


def engine_row(task_id: str, sweep: str, plan_root: Path, worktree: Path, run_date: str) -> dict:
    task = parse_task(task_id, plan_root, set())
    clauses = split_clauses(task.finish_line.text, set())
    try:
        source = resolve_evidence(task_id, plan_root, set())
        evidence = read_whole(source, "evidence read", set())
        missing = [item for item, exists in cited_artifacts(evidence, plan_root, worktree, set()) if not exists]
        results = [assess_clause(clause, evidence, missing) for clause in clauses]
        verdict = overall(results)
        evidence_path = str(source)
        failed = next((result for result in results if result.verdict != "HOLDS"), None)
    except EvidenceMissing as exc:
        verdict, evidence_path, results, failed = "CANNOT-DECIDE-MECHANICALLY", f"MISSING: {exc.exact}", [], None
    if verdict == "HOLDS":
        clause, reason = task.finish_line.text, "all clauses assessed as HOLDS"
    elif failed:
        clause, reason = failed.clause, failed.reason
    else:
        clause, reason = clauses[0], f"missing evidence file: {evidence_path}"
    return {"taskId": task_id, "fileLine": f"{task.path}:{task.finish_line.line}", "sweep": sweep,
            "verdict": verdict, "clause": clause, "reason": reason,
            "evidencePath": evidence_path, "runDate": run_date}


def reproduce_control_set(plan_root: Path) -> str:
    result = subprocess.run([sys.executable, str(ROOT / "scripts" / "run_reverification_control_set.py"),
                             "--control-set", str(ROOT / "scripts" / "reverification-control-set.json"),
                             "--engine", str(ROOT / "scripts" / "reverify_task.py"),
                             "--plan-root", str(plan_root)], cwd=ROOT, text=True, capture_output=True)
    if result.returncode:
        fail(f"control set failed: {result.stderr.strip() or result.stdout.strip()}")
    if "CONTROL SET PASS: sweep precondition satisfied" not in result.stdout:
        fail("absent input: control set")
    return result.stdout.strip()


def build_stale_index(worktree: Path, starve: set[str]) -> dict:
    if "stale index" in starve:
        fail("absent input: stale index")
    directory = worktree / "proof" / "reverification"
    directory.mkdir(parents=True, exist_ok=True)
    report = directory / "task-7112-stale-index.md"
    report.write_text(f"# Task 7112 stale index input\n\n{PRE_HISTORY_CAVEAT}\n", encoding="utf-8")
    first = directory / "task-7112-stale-index.first.json"
    second = directory / "task-7112-stale-index.json"
    command = [sys.executable, str(ROOT / "scripts" / "task-7103-stale-evidence-index.py"),
               "--plan-repo", str(PLAN_REPO), "--evidence-dir", str(PLAN / "evidence"),
               "--output", str(first), "--report", str(report)]
    initial = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    if initial.returncode:
        fail(f"absent input: stale index ({initial.stderr.strip() or initial.stdout.strip()})")
    replay = subprocess.run(command[:-4] + ["--output", str(second), "--report", str(report),
                                             "--require-second-run", str(first)], cwd=ROOT, text=True, capture_output=True)
    if replay.returncode:
        fail(f"absent input: stale index ({replay.stderr.strip() or replay.stdout.strip()})")
    data = json.loads(second.read_text(encoding="utf-8"))
    if not data.get("stale_ids"):
        fail("absent input: stale index (empty stale-id set)")
    return data


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan-root", type=Path, default=PLAN)
    parser.add_argument("--worktree", type=Path, default=ROOT)
    parser.add_argument("--sweep", default="task-7112-onboarding-redesign")
    parser.add_argument("--run-date", default=date.today().isoformat())
    parser.add_argument("--starve", action="append", choices=("control set", "design-page check", "stale index", "before-and-after comparison"), default=[])
    parser.add_argument("--test-force-collapse", action="store_true",
                        help="test-only: prove an all-HOLDS batch is refused before ledger write")
    args = parser.parse_args(argv)
    starve = set(args.starve)
    try:
        if "before-and-after comparison" in starve:
            fail("absent input: before-and-after comparison")
        todo_before = target_plan_fingerprint(args.plan_root)
        rejected_before = fingerprint(args.plan_root / "evidence" / "rejected")
        if "control set" in starve:
            fail("absent input: control set")
        # This is deliberately the first printed operational result.
        print(reproduce_control_set(args.plan_root))
        if "design-page check" in starve:
            fail("absent input: design-page check")
        stale = build_stale_index(args.worktree, starve)
        ids = target_ids(args.plan_root)
        rows, screen_rows = [], []
        for task_id in ids:
            row = engine_row(task_id, args.sweep, args.plan_root, args.worktree, args.run_date)
            receipt = screen_receipt(task_id, args.plan_root, starve)
            if receipt:
                screen_rows.append(receipt)
                if "failure_clause" in receipt:
                    row.update(verdict="FAILS", clause=receipt["failure_clause"], reason=receipt["failure_reason"], screen=receipt)
                else:
                    row["screen"] = receipt
            rows.append(row)
        if len(rows) != len(ids) or len({row["taskId"] for row in rows}) != len(ids):
            fail("absent input: exactly-one-verdict ledger rows")
        if "0381" in {row["taskId"] for row in rows}:
            fail("rejected-by-owner task 0381 was assigned a verdict")
        rejection = args.plan_root / "evidence" / "rejected" / "0381.md"
        if not rejection.is_file():
            fail("absent input: rejected-by-owner evidence 0381")
        if args.test_force_collapse:
            for row in rows:
                row["verdict"] = "HOLDS"
                row["clause"] = "test-only forced collapse"
                row["reason"] = "test-only forced collapse"
        counts = count_rows(rows)
        if counts["FAILS"] == 0 and counts["CANNOT-DECIDE-MECHANICALLY"] == 0:
            fail("collapse: 0 FAILS and 0 CANNOT-DECIDE-MECHANICALLY")
        ledger_path = require_ledger_path(default_ledger(args.worktree), args.plan_root, args.worktree)
        document = load_document(ledger_path)
        validate_document(document)
        candidate = {"runDate": args.run_date, "rows": rows, "counts": counts}
        validate_sweep(args.sweep, candidate)
        document["sweeps"][args.sweep] = candidate
        validate_document(document)
        write_document(ledger_path, document)
        todo_after = target_plan_fingerprint(args.plan_root)
        rejected_after = fingerprint(args.plan_root / "evidence" / "rejected")
        if todo_before != todo_after:
            fail("plan files changed during sweep: 03-onboarding-security.txt or 31-redesign-scope.txt")
        if rejected_before != rejected_after:
            fail("evidence/rejected changed during sweep")
        print(f"TASK7112 TICKED={len(ids)} LEDGER_ROWS={len(rows)}")
        print(f"TASK7112 COUNTS HOLDS={counts['HOLDS']} FAILS={counts['FAILS']} CANNOT-DECIDE-MECHANICALLY={counts['CANNOT-DECIDE-MECHANICALLY']}")
        print(f"TASK7112 STALE_INDEX stale_ids={len(stale['stale_ids'])} replayed_todo_commits={stale['replayed_todo_commits']}")
        for receipt in screen_rows:
            print("TASK7112 SCREEN task={task_id} design_page={page} structural={structural} pixel_under_1={pixel}".format(
                task_id=receipt["task_id"], page=receipt["design_page"] or "NONE",
                structural=str(receipt["structural_control_and_text_comparison"]).lower(),
                pixel=str(receipt["pixel_difference_under_1_percent"]).lower()))
        print(f"TASK7112 SCREEN_TICKS={len(screen_rows)} SCREEN_TICKS_WITH_NO_NAMED_REFERENCE={sum(item['design_page'] is None for item in screen_rows)}")
        print("TASK7112 REJECTED-BY-OWNER task=0381 verdict=NONE evidence=rejected/0381.md")
        print(f"TASK7112 PLAN_TODO_UNCHANGED=yes digest={todo_after}")
        print(f"TASK7112 EVIDENCE_REJECTED_UNCHANGED=yes digest={rejected_after}")
        print(f"TASK7112 LEDGER={ledger_path}")
        return 0
    except SweepError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
