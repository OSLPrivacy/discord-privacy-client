#!/usr/bin/env python3
"""Write the append-by-sweep re-verification verdict ledger.

The ledger is deliberately not evidence and is only allowed below
``proof/reverification/``.  It records the engine's result without turning an
undecidable result into a pass or a progress figure.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

from reverify_task import (
    DEFAULT_PLAN,
    DEFAULT_WORKTREE,
    EvidenceMissing,
    assess_clause,
    cited_artifacts,
    overall,
    parse_task,
    read_whole,
    resolve_evidence,
    split_clauses,
)


VERDICTS = ("HOLDS", "FAILS", "CANNOT-DECIDE-MECHANICALLY")
SCHEMA = "osl-reverification-verdict-ledger/v1"


class LedgerError(RuntimeError):
    pass


def default_ledger(worktree: Path) -> Path:
    return worktree / "proof" / "reverification" / "verdict-ledger.json"


def resolved(path: Path) -> Path:
    return path.resolve(strict=False)


def require_ledger_path(path: Path, plan_root: Path, worktree: Path) -> Path:
    target = resolved(path)
    forbidden = (resolved(plan_root / "todo"), resolved(plan_root / "evidence"))
    for root in forbidden:
        if target == root or root in target.parents:
            raise LedgerError(f"refusing ledger path under {root.name}: {target}")
    permitted = resolved(worktree / "proof" / "reverification")
    if target != permitted and permitted not in target.parents:
        raise LedgerError(f"refusing ledger path outside proof/reverification: {target}")
    return target


def empty_document() -> dict:
    return {"schema": SCHEMA, "sweeps": {}}


def load_document(path: Path) -> dict:
    if not path.exists():
        return empty_document()
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise LedgerError(f"invalid ledger: {path}: {exc}") from exc
    if not isinstance(document, dict) or document.get("schema") != SCHEMA:
        raise LedgerError(f"invalid ledger schema: {path}")
    if not isinstance(document.get("sweeps"), dict):
        raise LedgerError(f"invalid ledger sweeps: {path}")
    return document


def count_rows(rows: list[dict]) -> dict[str, int]:
    return {verdict: sum(row.get("verdict") == verdict for row in rows) for verdict in VERDICTS}


def require_counts(counts: object, context: str) -> dict[str, int]:
    if not isinstance(counts, dict):
        raise LedgerError(f"{context}: missing three-way count")
    values: dict[str, int] = {}
    for verdict in VERDICTS:
        value = counts.get(verdict)
        if not isinstance(value, int) or value < 0:
            raise LedgerError(f"{context}: missing three-way count {verdict}")
        values[verdict] = value
    if set(counts) != set(VERDICTS):
        raise LedgerError(f"{context}: missing three-way count")
    return values


def validate_row(row: object, context: str) -> dict:
    if not isinstance(row, dict):
        raise LedgerError(f"{context}: row is not an object")
    task_id = row.get("taskId")
    ident = str(task_id) if task_id else "unknown-id"
    fields = ("taskId", "fileLine", "sweep", "verdict", "clause", "reason", "evidencePath", "runDate")
    labels = {"evidencePath": "evidence path"}
    for field in fields:
        value = row.get(field)
        if not isinstance(value, str) or not value.strip():
            raise LedgerError(f"{context}: {ident}: missing {labels.get(field, field)}")
    if row["verdict"] not in VERDICTS:
        raise LedgerError(f"{context}: {ident}: invalid verdict {row['verdict']!r}")
    if row["verdict"] != "HOLDS":
        for field in ("clause", "reason"):
            if not row[field].strip():
                raise LedgerError(f"{context}: {ident}: missing {field}")
    return row


def validate_sweep(name: str, sweep: object) -> list[dict]:
    if not isinstance(sweep, dict):
        raise LedgerError(f"sweep {name}: invalid sweep")
    rows = sweep.get("rows")
    if not isinstance(rows, list):
        raise LedgerError(f"sweep {name}: missing rows")
    checked = [validate_row(row, f"sweep {name}") for row in rows]
    ids = [row["taskId"] for row in checked]
    if len(ids) != len(set(ids)):
        raise LedgerError(f"sweep {name}: duplicate task id")
    for row in checked:
        if row["sweep"] != name:
            raise LedgerError(f"sweep {name}: {row['taskId']}: sweep field mismatch")
    counts = require_counts(sweep.get("counts"), f"sweep {name}")
    if counts != count_rows(checked):
        raise LedgerError(f"sweep {name}: three-way count does not match rows")
    return checked


def validate_document(document: dict) -> None:
    for name, sweep in document["sweeps"].items():
        if not isinstance(name, str) or not name.strip():
            raise LedgerError("invalid empty sweep name")
        validate_sweep(name, sweep)


def make_row(task_id: str, sweep: str, plan_root: Path, worktree: Path, run_date: str,
             engine_worktree: Path | None = None) -> dict:
    task = parse_task(task_id, plan_root, set())
    clauses = split_clauses(task.finish_line.text, set())
    evidence_path: str
    try:
        source = resolve_evidence(task_id, plan_root, set())
        evidence = read_whole(source, "evidence read", set())
        artifacts = cited_artifacts(evidence, plan_root, engine_worktree or worktree, set())
        missing = [item for item, exists in artifacts if not exists]
        results = [assess_clause(clause, evidence, missing) for clause in clauses]
        verdict = overall(results)
        evidence_path = str(source)
    except EvidenceMissing as exc:
        verdict = "CANNOT-DECIDE-MECHANICALLY"
        results = []
        evidence_path = f"MISSING: {exc.exact}"
        reason = str(exc)

    if verdict == "HOLDS":
        clause = task.finish_line.text
        reason = "all clauses assessed as HOLDS"
    elif results:
        result = next(result for result in results if result.verdict != "HOLDS")
        clause, reason = result.clause, result.reason
    else:
        clause = clauses[0]

    return {
        "taskId": task.task_id,
        "fileLine": f"{task.path}:{task.finish_line.line}",
        "sweep": sweep,
        "verdict": verdict,
        "clause": clause,
        "reason": reason,
        "evidencePath": evidence_path,
        "runDate": run_date,
    }


def render_counts(label: str, counts: dict[str, int]) -> str:
    checked = require_counts(counts, label)
    # One label per verdict is intentionally the only summary format.
    return (f"LEDGER {label} HOLDS={checked['HOLDS']} FAILS={checked['FAILS']} "
            f"CANNOT-DECIDE-MECHANICALLY={checked['CANNOT-DECIDE-MECHANICALLY']}")


def write_document(path: Path, document: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(document, indent=2, sort_keys=True) + "\n"
    fd, temporary = tempfile.mkstemp(prefix=".verdict-ledger-", suffix=".json", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    except BaseException:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass
        raise


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sweep", required=True, help="name of this re-verification sweep")
    parser.add_argument("--ledger", type=Path, help="must be below proof/reverification/")
    parser.add_argument("--plan-root", type=Path, default=DEFAULT_PLAN)
    parser.add_argument("--worktree", type=Path, default=DEFAULT_WORKTREE)
    parser.add_argument("--engine-worktree", type=Path,
                        help="read-only artifact lookup root; ledger still writes below --worktree")
    parser.add_argument("--run-date", help="ISO date; defaults to today's UTC date")
    parser.add_argument("--starve", choices=("verdict", "clause", "reason", "evidence path", "three-way count"),
                        help="test-only: remove a required value before validation")
    parser.add_argument("task_ids", nargs="+", help="one or more task ids for this sweep")
    args = parser.parse_args(argv)
    try:
        if not args.sweep.strip():
            raise LedgerError("missing sweep")
        run_date = args.run_date or datetime.now(timezone.utc).date().isoformat()
        ledger_path = require_ledger_path(args.ledger or default_ledger(args.worktree), args.plan_root, args.worktree)
        document = load_document(ledger_path)
        validate_document(document)
        rows = [make_row(task_id, args.sweep, args.plan_root, args.worktree, run_date,
                         args.engine_worktree) for task_id in args.task_ids]
        if args.starve == "evidence path":
            rows[0]["evidencePath"] = ""
        elif args.starve:
            field = {"verdict": "verdict", "clause": "clause", "reason": "reason"}.get(args.starve)
            if field:
                rows[0][field] = ""
        counts = count_rows(rows)
        if args.starve == "three-way count":
            del counts["CANNOT-DECIDE-MECHANICALLY"]
        candidate = {"runDate": run_date, "rows": rows, "counts": counts}
        validate_sweep(args.sweep, candidate)
        # Replace exactly this named sweep; every other sweep stays byte-for-byte
        # equivalent in the in-memory document and is revalidated before writing.
        document["sweeps"][args.sweep] = candidate
        validate_document(document)
        write_document(ledger_path, document)
        whole_rows = [row for sweep in document["sweeps"].values() for row in sweep["rows"]]
        print(render_counts(f"SWEEP {args.sweep}", count_rows(rows)))
        print(render_counts("WHOLE-RUN", count_rows(whole_rows)))
        print(f"LEDGER PATH {ledger_path}")
        return 0
    except (LedgerError, RuntimeError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
