#!/usr/bin/env python3
"""Structured oracle for Task 6577 removal campaigns and mutants."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any


OBSERVATION_SCHEMA = "osl.task6577.removal-proof.v1"
SIZE_SCHEMA = "osl.task6577.removal-size-observation.v1"
WARNING = "Removal takes time and is not immediate."
REKEY_WORK_LIMITS = [2, 3, 1, 5]
RESTARTS = ("client", "worker", "machine")
EXPECTED_MUTANTS = (
    "cap_at_n",
    "estimated_threshold",
    "warning_suppressed",
    "warning_softened",
    "synthetic_progress",
    "truncate_after_batch_2",
    "truncate_after_batch_3",
    "truncate_after_batch_1",
    "truncate_after_batch_5",
    "lose_client_restart",
    "lose_worker_restart",
    "lose_machine_restart",
    "permanently_pending",
    "success_one_member_early",
    "retained_packaged_reader",
    "retained_service_reader",
)


class ProofFailure(RuntimeError):
    pass


def measured_sizes(record: dict[str, Any]) -> tuple[int, int, int]:
    n = record.get("first_non_prompt_n") or record.get("provisional_n_on_observed_host")
    far = record.get("far_above_n")
    if not isinstance(n, int) or n < 2 or not isinstance(far, int):
        raise ProofFailure(
            f"attack=measurement_record record=invalid N={n} far_above_n={far}"
        )
    return n - 1, n, far


def measurement_basis(record: dict[str, Any]) -> str:
    rows = record.get("measurements")
    if not isinstance(rows, list) or not rows:
        return "estimated_threshold"
    for row in rows:
        runs = row.get("runs") if isinstance(row, dict) else None
        if not isinstance(runs, list) or not runs:
            return "estimated_threshold"
        for run in runs:
            if not isinstance(run, dict):
                return "estimated_threshold"
            if not isinstance(run.get("argv"), list):
                return "estimated_threshold"
            if not isinstance(run.get("stdout"), str):
                return "estimated_threshold"
            if not isinstance(run.get("result"), dict):
                return "estimated_threshold"
            if not isinstance(run.get("started_at_utc"), str):
                return "estimated_threshold"
            if not isinstance(run.get("ended_at_utc"), str):
                return "estimated_threshold"
    return "observed_runs"


def parse_transcript(path: Path) -> list[dict[str, Any]]:
    prefix = "TASK6577_OBSERVATION_JSON="
    observations: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if prefix not in line:
            continue
        payload = line.split(prefix, 1)[1]
        value = json.loads(payload)
        if not isinstance(value, dict):
            raise ProofFailure("attack=transcript observation=non-object")
        observations.append(value)
    return observations


def build_observation(measurement_path: Path, transcript_path: Path) -> dict[str, Any]:
    record = json.loads(measurement_path.read_text(encoding="utf-8"))
    below, at, far = measured_sizes(record)
    return {
        "schema": OBSERVATION_SCHEMA,
        "measurementRecord": str(measurement_path),
        "measurementBasis": measurement_basis(record),
        "n": at,
        "expectedSizes": {
            "belowN": below,
            "atN": at,
            "farAboveN": far,
        },
        "sizes": parse_transcript(transcript_path),
        "expectedMutants": list(EXPECTED_MUTANTS),
    }


def _integer(value: Any, field: str, size: int) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        raise ProofFailure(f"attack=observation observed_size={size} invalid_field={field}")
    return value


def _diagnostic_value(value: Any) -> str:
    if value is None:
        return "absent"
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def verify_observation(observation: dict[str, Any]) -> str:
    if observation.get("schema") != OBSERVATION_SCHEMA:
        raise ProofFailure(f"attack=observation schema={observation.get('schema')}")
    measurement_record = observation.get("measurementRecord")
    basis = observation.get("measurementBasis")
    if basis != "observed_runs":
        raise ProofFailure(
            "attack=estimate "
            f"measurement_record={measurement_record} basis={basis} required=observed_runs"
        )
    n = observation.get("n")
    expected = observation.get("expectedSizes")
    if not isinstance(n, int) or not isinstance(expected, dict):
        raise ProofFailure(
            f"attack=measurement_record measurement_record={measurement_record} N={n}"
        )
    roles = (("belowN", n - 1), ("atN", n), ("farAboveN", expected.get("farAboveN")))
    for role, wanted in roles:
        if expected.get(role) != wanted:
            raise ProofFailure(
                f"attack=measurement_record measurement_record={measurement_record} role={role} size={expected.get(role)} expected={wanted}"
            )
    raw_sizes = observation.get("sizes")
    if not isinstance(raw_sizes, list):
        raise ProofFailure("attack=absent_size role=all observed_sizes=none")
    by_size: dict[int, dict[str, Any]] = {}
    for item in raw_sizes:
        if not isinstance(item, dict) or not isinstance(item.get("size"), int):
            raise ProofFailure("attack=observation observed_size=invalid")
        size = item["size"]
        if size in by_size:
            raise ProofFailure(f"attack=observation observed_size={size} duplicate=true")
        by_size[size] = item
    observed_sizes = ",".join(map(str, sorted(by_size))) or "none"
    for role, wanted in roles:
        if wanted not in by_size:
            raise ProofFailure(
                f"attack=absent_size role={role} absent_size={wanted} observed_sizes={observed_sizes}"
            )
    expected_set = {wanted for _, wanted in roles}
    if set(by_size) != expected_set:
        raise ProofFailure(
            f"attack=size_inventory observed_sizes={observed_sizes} expected_sizes={','.join(map(str, sorted(expected_set)))}"
        )

    for role, size in roles:
        row = by_size[size]
        if row.get("schema") != SIZE_SCHEMA or row.get("n") != n:
            raise ProofFailure(f"attack=observation observed_size={size} schema_or_N_mismatch=true")
        admitted = row.get("admittedSize")
        if admitted != size:
            raise ProofFailure(
                f"attack=cap observed_size={size} N={n} admitted_size={admitted}"
            )
        expected_warning = None if size < n else WARNING
        if row.get("warning") != expected_warning:
            raise ProofFailure(
                "attack=disclosure "
                f"observed_size={size} N={n} warning={_diagnostic_value(row.get('warning'))} "
                f"required={_diagnostic_value(expected_warning)}"
            )
        progress = row.get("progress")
        if not isinstance(progress, dict):
            raise ProofFailure(f"attack=fake_progress observed_size={size} completed=absent remaining=absent")
        completed = _integer(progress.get("completed"), "completed", size)
        remaining = _integer(progress.get("remaining"), "remaining", size)
        total = _integer(progress.get("total"), "total", size)
        rekey_completed = progress.get("rekeyCompleted")
        source = progress.get("source")
        if source != "authenticated-successor-authority-wraps" or rekey_completed != completed:
            raise ProofFailure(
                "attack=fake_progress "
                f"observed_size={size} completed={completed} remaining={remaining} "
                f"rekey_completed={rekey_completed} source={source}"
            )
        boundary = row.get("truncatedAtBoundary")
        if boundary is not None:
            raise ProofFailure(
                "attack=boundary "
                f"observed_size={size} boundary={boundary} completed={completed} remaining={remaining}"
            )
        status = row.get("status")
        if status == "pending":
            raise ProofFailure(
                f"attack=noncompletion observed_size={size} completed={completed} remaining={remaining} status=pending"
            )
        if status == "succeeded" and (completed != total or remaining != 0):
            raise ProofFailure(
                "attack=early_success "
                f"observed_size={size} member_id={row.get('removedMemberId')} epoch={row.get('epoch')} "
                f"completed={completed} remaining={remaining}"
            )
        if status != "succeeded" or total != size - 1 or completed != total or remaining != 0:
            raise ProofFailure(
                f"attack=noncompletion observed_size={size} completed={completed} remaining={remaining} status={status}"
            )
        if row.get("rekeyWorkLimits") != REKEY_WORK_LIMITS:
            raise ProofFailure(
                f"attack=boundary observed_size={size} boundary_inventory={row.get('rekeyWorkLimits')}"
            )
        edges = row.get("rekeyBatchEdges")
        if not isinstance(edges, list) or not edges or edges[-1] != total:
            raise ProofFailure(
                f"attack=boundary observed_size={size} boundary=completion completed={completed} remaining={remaining} edges={edges}"
            )
        if role == "farAboveN":
            if edges[:3] != [2, 5, 6] or any(
                not isinstance(edge, int) or edge < 0 or edge > total for edge in edges
            ):
                raise ProofFailure(
                    f"attack=boundary observed_size={size} boundary=2,3,1,5 completed={completed} remaining={remaining} edges={edges}"
                )
        restarts = row.get("restarts")
        if not isinstance(restarts, dict):
            raise ProofFailure(
                f"attack=restart observed_size={size} restart=all completed={completed} remaining={remaining} status=absent"
            )
        for restart in RESTARTS:
            if restarts.get(restart) != "resumed":
                raise ProofFailure(
                    "attack=restart "
                    f"observed_size={size} restart={restart} completed={completed} remaining={remaining} status={restarts.get(restart)}"
                )
        epoch = row.get("epoch")
        if epoch != 42 or row.get("successAfterLastRekey") is not True:
            raise ProofFailure(
                "attack=early_success "
                f"observed_size={size} member_id={row.get('removedMemberId')} epoch={epoch} "
                f"completed={completed} remaining={remaining}"
            )
        member_id = row.get("removedMemberId")
        fresh = row.get("freshMessage")
        for read_path in ("removedPackagedReads", "removedDirectReads"):
            reads = row.get(read_path)
            if reads != 0:
                raise ProofFailure(
                    "attack=retained_reader "
                    f"observed_size={size} member_id={member_id} epoch={epoch} "
                    f"fresh_message={fresh} path={read_path} reads={reads}"
                )
        if row.get("remainingExactReads") != total:
            raise ProofFailure(
                f"attack=member observed_size={size} completed={completed} remaining={remaining} exact_readers={row.get('remainingExactReads')} expected={total}"
            )
        if row.get("discarded") is not True:
            raise ProofFailure(
                f"attack=discard observed_size={size} member_id={member_id} enclave_discarded={row.get('discarded')}"
            )

    return (
        "TASK6577_PROOF_EXIT=0 "
        f"sizes={','.join(map(str, (n - 1, n, expected['farAboveN'])))} "
        f"N={n} measurement_record={measurement_record} measurement_basis=observed_runs "
        "completed_remaining=all/0 restarts=client,worker,machine "
        "epoch=42 removed_fresh_message_reads=0 discarded=true"
    )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="mode", required=True)
    build = sub.add_parser("build-verify")
    build.add_argument("--measurement", type=Path, required=True)
    build.add_argument("--transcript", type=Path, required=True)
    build.add_argument("--output", type=Path)
    verify = sub.add_parser("verify")
    verify.add_argument("--observation", type=Path, required=True)
    return parser


def main() -> int:
    args = _parser().parse_args()
    try:
        if args.mode == "build-verify":
            observation = build_observation(args.measurement, args.transcript)
            if args.output:
                args.output.write_text(json.dumps(observation, sort_keys=True) + "\n", encoding="utf-8")
        else:
            observation = json.loads(args.observation.read_text(encoding="utf-8"))
        print(verify_observation(observation))
        return 0
    except (ProofFailure, OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"TASK6577_PROOF_EXIT=1 {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
