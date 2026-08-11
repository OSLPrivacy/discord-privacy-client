#!/usr/bin/env python3
"""Local virtual-fixture mutation matrix for TASK 6331b."""

from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path

from test_task_6331_receive_responsiveness import END, SURFACE, build_pair


def _pair() -> tuple[dict, dict, dict, dict]:
    quiet_observer, quiet_workload = build_pair("quiet")
    busy_observer, busy_workload = build_pair("busy")
    return copy.deepcopy((quiet_observer, quiet_workload, busy_observer, busy_workload))


def _stall(observer: dict, index: int = 100) -> None:
    probe = observer["probes"][index]
    probe["response_qpc"] = probe["sent_qpc"] + 501
    probe["visible_response"]["qpc"] = probe["response_qpc"]
    probe["latency_ms"] = 501


def _cases() -> list[tuple[str, str, str, Callable[..., None]]]:
    def quiet_stall(qo, qw, bo, bw): _stall(qo)
    def busy_stall(qo, qw, bo, bw): _stall(bo)
    def missing_row(qo, qw, bo, bw):
        bo["content_changes"].pop(17); bo["content_change_count"] -= 1
    def missing_bound_probe(qo, qw, bo, bw):
        qo["probes"].pop(20)
        qo["probe_count"] -= 1
    def uia(qo, qw, bo, bw):
        qo["probes"][9].update(injection_api="UIAutomation.InvokePattern", direct_invocation=True)
    def other_control(qo, qw, bo, bw): bo["probes"][33]["target_surface"]["runtime_id"] = [99, 1]
    def destroyed(qo, qw, bo, bw): qo["destroyed_runtime_ids"] = [copy.deepcopy(SURFACE["runtime_id"])]

    return [
        ("quiet-501ms-stall", "quiet", "quiet-input-0100", quiet_stall),
        ("busy-501ms-stall", "busy", "busy-input-0100", busy_stall),
        ("missing-marker-row", "busy", "-", missing_row),
        ("missing-bound-probe", "quiet", "quiet-input-0021", missing_bound_probe),
        ("uia-direct-input", "quiet", "quiet-input-0009", uia),
        ("other-osl-control", "busy", "busy-input-0033", other_control),
        ("destroyed-identity", "quiet", "-", destroyed),
    ]


def _require_context(reason: str, run: str, input_id: str) -> None:
    required = (f"run={run}", "surface=[", "provider_id=", f"input_id={input_id}")
    if not all(value in reason for value in required):
        raise AssertionError(f"missing reconciliation context: {reason}")


def _run_gate(pair: tuple[dict, dict, dict, dict]) -> subprocess.CompletedProcess[str]:
    script = Path(__file__).with_name("task_6331_receive_responsiveness.py")
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        paths = [root / name for name in ("quiet-observer.json", "quiet-workload.json", "busy-observer.json", "busy-workload.json")]
        for path, value in zip(paths, pair, strict=True):
            path.write_text(json.dumps(value), encoding="utf-8")
        return subprocess.run(
            [sys.executable, str(script), "--quiet-observer", str(paths[0]), "--quiet-workload", str(paths[1]), "--busy-observer", str(paths[2]), "--busy-workload", str(paths[3])],
            text=True,
            capture_output=True,
            check=False,
        )


def main() -> int:
    for name, run, input_id, mutate in _cases():
        pair = _pair()
        mutate(*pair)
        completed = _run_gate(pair)
        if completed.returncode != 1 or not completed.stderr.startswith("TASK6331_FAIL="):
            raise AssertionError(f"{name} did not make the unchanged gate exit 1: {completed.stderr}")
        reason = completed.stderr.strip().removeprefix("TASK6331_FAIL=")
        _require_context(reason, run, input_id)
        print(f"TASK6331B_LOCAL_MUTANT name={name} exit=1 provenance=virtual-fixture installed=false reason={reason}")

    outside = _pair()
    outside[0]["outside_interval_stalls"] = [{"start_qpc": END + 1, "duration_ms": 900}]
    outside_result = _run_gate(outside)
    if outside_result.returncode != 0 or not outside_result.stdout.startswith("TASK6331_PASS "):
        raise AssertionError("900ms outside-interval stall did not PASS")
    print("TASK6331B_OUTSIDE name=900ms-outside-interval gate_exit=0 status=PASS provenance=virtual-fixture installed=false")
    restored_result = _run_gate(_pair())
    if restored_result.returncode != 0 or not restored_result.stdout.startswith("TASK6331_PASS "):
        raise AssertionError("restored fresh quiet/busy pair did not PASS")
    print("TASK6331B_RESTORED name=fresh-quiet-busy gate_exit=0 status=PASS provenance=virtual-fixture installed=false")
    oracle_sha = hashlib.sha256(Path(__file__).with_name("task_6331_receive_responsiveness.py").read_bytes()).hexdigest()
    print(f"TASK6331B_SUMMARY oracle_sha256={oracle_sha} installed_builds=0 installed_mutants=0 local_mutants=7 forbidden_credit=0 sacrificial_control=unmeasured ingestion=unmeasured harness=unmeasured")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
