#!/usr/bin/env python3
"""Run the declarative audit control set before publishing a re-verification sweep.

The JSON file is the only place that names control task IDs and their audit
expectations.  This runner deliberately does not turn provisional HOLDS rows
into a pass count: the integrity audit read their write-ups, not necessarily
the checker that purportedly produced them.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTROL_SET = ROOT / "scripts" / "reverification-control-set.json"
DEFAULT_ENGINE = ROOT / "scripts" / "reverify_task.py"
DEFAULT_PLAN = Path("/home/liamw/osl-plan/OSL-AUDITS")
VALID_VERDICTS = {"FAILS", "CANNOT-DECIDE", "HOLDS"}


class ControlError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ControlError(message)


def strings(value: Any, label: str) -> list[str]:
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        fail(f"absent control case: {label}")
    return value


def read_control_set(path: Path) -> dict[str, Any]:
    if not path.is_file():
        fail(f"absent control file: {path}")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"absent control file: {path}: {exc}")
    if not isinstance(data, dict) or data.get("schema") != "osl-reverification-control-set/v1":
        fail("absent control case: schema")
    return data


def validated_rows(data: dict[str, Any]) -> list[dict[str, Any]]:
    required_ids = strings(data.get("required_task_ids"), "required task ids")
    required_verdicts = strings(data.get("required_verdicts"), "required verdict classes")
    provisional_holds = strings(data.get("provisional_holds"), "provisional holds")
    minimum_rows = data.get("minimum_rows")
    rows = data.get("rows")
    if not isinstance(minimum_rows, int) or not isinstance(rows, list):
        fail("absent control case: rows")
    if set(required_verdicts) != VALID_VERDICTS:
        fail("absent control case: verdict classes")
    result: list[dict[str, Any]] = []
    seen: set[str] = set()
    for row in rows:
        if not isinstance(row, dict):
            fail("absent control case: row")
        task_id = row.get("task_id")
        verdict = row.get("verdict")
        if not isinstance(task_id, str) or not re.fullmatch(r"[0-9]{4}[a-z]?", task_id):
            fail("absent control case: task id")
        if task_id in seen:
            fail(f"absent control case: duplicate task id {task_id}")
        seen.add(task_id)
        if verdict not in VALID_VERDICTS:
            fail(f"absent control case: verdict for {task_id}")
        if verdict == "FAILS":
            if not all(isinstance(row.get(field), str) and row[field] for field in ("audit_clause", "audit_reason")):
                fail(f"absent control case: FAILS clause/reason for {task_id}")
        if verdict == "CANNOT-DECIDE" and not isinstance(row.get("missing_surface"), str):
            fail(f"absent control case: missing surface for {task_id}")
        if verdict == "HOLDS" and row.get("provisional") is not True:
            fail(f"absent provisional marking: {task_id}")
        result.append(row)
    present_verdicts = {str(row["verdict"]) for row in result}
    for verdict in required_verdicts:
        if verdict not in present_verdicts:
            fail(f"absent verdict class: {verdict}")
    for task_id in required_ids:
        if task_id not in seen:
            fail(f"absent control row: {task_id}")
    if len(rows) < minimum_rows:
        fail(f"absent control case: rows (need at least {minimum_rows}, found {len(rows)})")
    actual_provisional = {str(row["task_id"]) for row in result if row["verdict"] == "HOLDS" and row.get("provisional") is True}
    for task_id in provisional_holds:
        if task_id not in actual_provisional:
            fail(f"absent provisional marking: {task_id}")
    if actual_provisional != set(provisional_holds):
        extra = sorted(actual_provisional - set(provisional_holds))[0]
        fail(f"absent control case: undeclared provisional hold {extra}")
    return result


def raw_engine_report(engine: Path, plan_root: Path, task_id: str) -> tuple[str, str]:
    if not engine.is_file():
        fail(f"absent engine: {engine}")
    completed = subprocess.run(
        [sys.executable, str(engine), "--plan-root", str(plan_root), task_id],
        text=True, capture_output=True, cwd=ROOT,
    )
    if completed.returncode:
        fail(f"engine exit {completed.returncode}: {task_id}: {completed.stderr.strip()}")
    output = completed.stdout
    verdicts = re.findall(r"^VERDICT: ([A-Z-]+)$", output, flags=re.MULTILINE)
    if len(verdicts) != 1:
        fail(f"engine produced no single verdict: {task_id}")
    return verdicts[0], output


def run(rows: list[dict[str, Any]], engine: Path, plan_root: Path) -> list[tuple[dict[str, Any], str, str]]:
    reports = [(row, *raw_engine_report(engine, plan_root, str(row["task_id"]))) for row in rows]
    raw_verdicts = {raw for _, raw, _ in reports}
    if len(raw_verdicts) == 1:
        fail(f"verdict collapse: every row returned {next(iter(raw_verdicts))}")
    for row, raw, output in reports:
        if row["verdict"] == "FAILS":
            if raw != "FAILS":
                fail(f"control FAILS row did not fail: {row['task_id']} returned {raw}")
            if row["audit_clause"] not in output or row["audit_reason"] not in output:
                fail(f"control FAILS row missing audit clause/reason: {row['task_id']}")
        elif row["verdict"] == "CANNOT-DECIDE":
            if raw != "CANNOT-DECIDE-MECHANICALLY":
                fail(f"control CANNOT-DECIDE row did not remain undecidable: {row['task_id']} returned {raw}")
    return reports


def run_sweep(engine: Path, plan_root: Path, task_ids: list[str]) -> str:
    """Produce a sweep report only after ``run`` has accepted the controls."""
    completed = subprocess.run(
        [sys.executable, str(engine), "--plan-root", str(plan_root), *task_ids],
        text=True, capture_output=True, cwd=ROOT,
    )
    if completed.returncode:
        fail(f"sweep engine exit {completed.returncode}: {completed.stderr.strip()}")
    return completed.stdout


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control-set", type=Path, default=DEFAULT_CONTROL_SET)
    parser.add_argument("--engine", type=Path, default=DEFAULT_ENGINE)
    parser.add_argument("--plan-root", type=Path, default=DEFAULT_PLAN)
    parser.add_argument("--sweep", nargs="+", metavar="TASK_ID",
                        help="report these tasks only after the control set passes")
    args = parser.parse_args(argv)
    try:
        rows = validated_rows(read_control_set(args.control_set))
        reports = run(rows, args.engine, args.plan_root)
    except ControlError as exc:
        print(f"CONTROL SET FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"CONTROL SET LOADED: {args.control_set} rows={len(rows)}")
    for row, raw, _ in reports:
        task_id = row["task_id"]
        if row["verdict"] == "FAILS":
            print(f"CONTROL {task_id} FAILS raw={raw} clause={row['audit_clause']} reason={row['audit_reason']}")
        elif row["verdict"] == "CANNOT-DECIDE":
            print(f"CONTROL {task_id} CANNOT-DECIDE raw={raw} missing_surface={row['missing_surface']}")
        else:
            print(f"CONTROL {task_id} HOLDS PROVISIONAL raw={raw} counted=false")
    print("CONTROL PASS TOTAL: 0 (all HOLDS are PROVISIONAL and excluded)")
    print("CONTROL SET PASS: sweep precondition satisfied")
    if args.sweep:
        try:
            sweep_output = run_sweep(args.engine, args.plan_root, args.sweep)
        except ControlError as exc:
            print(f"CONTROL SET FAIL: {exc}", file=sys.stderr)
            return 1
        print("SWEEP REPORTS:")
        print(sweep_output.rstrip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
