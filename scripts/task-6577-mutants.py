#!/usr/bin/env python3
"""Run every Task 6577 attack in an isolated disposable candidate."""

from __future__ import annotations

import argparse
import copy
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
MEASUREMENT = ROOT / "docs/evidence/task-6576/measurement.json"
PROOF_PATH = ROOT / "scripts/task-6577-proof.py"
EXPECTED_TARGET = "/mnt/d/osl-lane-targets/c"


def load_proof() -> Any:
    spec = importlib.util.spec_from_file_location("task6577_proof", PROOF_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load task-6577 proof oracle")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PROOF = load_proof()
EXPECTED_MUTANTS = tuple(PROOF.EXPECTED_MUTANTS)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--inventory-probe", action="store_true")
    parser.add_argument("--omit-mutant", choices=EXPECTED_MUTANTS)
    return parser.parse_args()


def size_row(observation: dict[str, Any], size: int) -> dict[str, Any]:
    return next(row for row in observation["sizes"] if row["size"] == size)


def mutate_cap(observation: dict[str, Any], n: int, far: int) -> None:
    size_row(observation, n)["admittedSize"] = n - 1


def mutate_estimate(observation: dict[str, Any], n: int, far: int) -> None:
    observation["measurementBasis"] = "estimated_threshold"


def mutate_warning_suppressed(observation: dict[str, Any], n: int, far: int) -> None:
    size_row(observation, n)["warning"] = None


def mutate_warning_softened(observation: dict[str, Any], n: int, far: int) -> None:
    size_row(observation, n)["warning"] = "Removal may take a moment."


def mutate_synthetic_progress(observation: dict[str, Any], n: int, far: int) -> None:
    progress = size_row(observation, far)["progress"]
    progress["source"] = "synthetic-heartbeat"
    progress["rekeyCompleted"] = 0


def truncate(boundary: int, completed: int) -> Callable[[dict[str, Any], int, int], None]:
    def apply(observation: dict[str, Any], n: int, far: int) -> None:
        row = size_row(observation, far)
        total = far - 1
        row["truncatedAtBoundary"] = boundary
        row["status"] = "rekeying"
        row["progress"].update(
            completed=completed,
            remaining=total - completed,
            rekeyCompleted=completed,
        )
        row["rekeyBatchEdges"] = [edge for edge in row["rekeyBatchEdges"] if edge <= completed]

    return apply


def lose_restart(boundary: str) -> Callable[[dict[str, Any], int, int], None]:
    def apply(observation: dict[str, Any], n: int, far: int) -> None:
        row = size_row(observation, far)
        row["restarts"][boundary] = "lost"

    return apply


def mutate_pending(observation: dict[str, Any], n: int, far: int) -> None:
    row = size_row(observation, far)
    completed = 6
    row["status"] = "pending"
    row["progress"].update(
        completed=completed,
        remaining=far - 1 - completed,
        rekeyCompleted=completed,
    )


def mutate_early_success(observation: dict[str, Any], n: int, far: int) -> None:
    row = size_row(observation, far)
    completed = far - 2
    row["status"] = "succeeded"
    row["progress"].update(
        completed=completed,
        remaining=1,
        rekeyCompleted=completed,
    )


def mutate_packaged_reader(observation: dict[str, Any], n: int, far: int) -> None:
    size_row(observation, far)["removedPackagedReads"] = 1


def mutate_service_reader(observation: dict[str, Any], n: int, far: int) -> None:
    size_row(observation, far)["removedDirectReads"] = 1


MUTANTS: dict[str, Callable[[dict[str, Any], int, int], None]] = {
    "cap_at_n": mutate_cap,
    "estimated_threshold": mutate_estimate,
    "warning_suppressed": mutate_warning_suppressed,
    "warning_softened": mutate_warning_softened,
    "synthetic_progress": mutate_synthetic_progress,
    "truncate_after_batch_2": truncate(2, 2),
    "truncate_after_batch_3": truncate(3, 5),
    "truncate_after_batch_1": truncate(1, 6),
    "truncate_after_batch_5": truncate(5, 11),
    "lose_client_restart": lose_restart("client"),
    "lose_worker_restart": lose_restart("worker"),
    "lose_machine_restart": lose_restart("machine"),
    "permanently_pending": mutate_pending,
    "success_one_member_early": mutate_early_success,
    "retained_packaged_reader": mutate_packaged_reader,
    "retained_service_reader": mutate_service_reader,
}


EXPECTED_DIAGNOSTICS: dict[str, tuple[str, ...]] = {
    "cap_at_n": ("attack=cap", "observed_size=3", "N=3", "admitted_size=2"),
    "estimated_threshold": ("attack=estimate", "measurement_record=", "basis=estimated_threshold"),
    "warning_suppressed": ("attack=disclosure", "observed_size=3", "warning=absent"),
    "warning_softened": ("attack=disclosure", "observed_size=3", "warning=\"Removal may take a moment.\""),
    "synthetic_progress": ("attack=fake_progress", "completed=102", "remaining=0", "rekey_completed=0"),
    "truncate_after_batch_2": ("attack=boundary", "observed_size=103", "boundary=2", "completed=2", "remaining=100"),
    "truncate_after_batch_3": ("attack=boundary", "observed_size=103", "boundary=3", "completed=5", "remaining=97"),
    "truncate_after_batch_1": ("attack=boundary", "observed_size=103", "boundary=1", "completed=6", "remaining=96"),
    "truncate_after_batch_5": ("attack=boundary", "observed_size=103", "boundary=5", "completed=11", "remaining=91"),
    "lose_client_restart": ("attack=restart", "observed_size=103", "restart=client", "status=lost"),
    "lose_worker_restart": ("attack=restart", "observed_size=103", "restart=worker", "status=lost"),
    "lose_machine_restart": ("attack=restart", "observed_size=103", "restart=machine", "status=lost"),
    "permanently_pending": ("attack=noncompletion", "observed_size=103", "completed=6", "remaining=96", "status=pending"),
    "success_one_member_early": ("attack=early_success", "observed_size=103", "member_id=member-00102", "epoch=42", "completed=101", "remaining=1"),
    "retained_packaged_reader": ("attack=retained_reader", "member_id=member-00102", "epoch=42", "fresh_message=post-removal-canary", "path=removedPackagedReads"),
    "retained_service_reader": ("attack=retained_reader", "member_id=member-00102", "epoch=42", "fresh_message=post-removal-canary", "path=removedDirectReads"),
}


def run_verifier(path: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(PROOF_PATH), "verify", "--observation", str(path)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=30,
        check=False,
    )


def capture_baseline(temp_root: Path) -> dict[str, Any]:
    record = json.loads(MEASUREMENT.read_text(encoding="utf-8"))
    below, n, far = PROOF.measured_sizes(record)
    environment = os.environ.copy()
    environment.update(
        CARGO_TARGET_DIR=EXPECTED_TARGET,
        TASK6576_MEASURED_N=str(n),
        TASK6576_FAR_ABOVE_N=str(far),
    )
    completed = subprocess.run(
        [
            "cargo", "test", "-p", "ipc", "--test", "task_6576_enclave_removal",
            "--", "--nocapture", "--test-threads=1",
        ],
        cwd=ROOT,
        env=environment,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=1800,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"baseline cargo test exited {completed.returncode}\n{completed.stdout}")
    transcript = temp_root / "baseline-transcript.txt"
    transcript.write_text(completed.stdout, encoding="utf-8")
    observation = PROOF.build_observation(MEASUREMENT, transcript)
    print(PROOF.verify_observation(observation))
    print(f"TASK6577_BASELINE_CARGO_EXIT=0 sizes={below},{n},{far} package=ipc test=task_6576_enclave_removal")
    return observation


def verify_candidate(candidate: Path, observation: dict[str, Any], expected: tuple[str, ...]) -> str:
    observation_path = candidate / "observation.json"
    observation_path.write_text(json.dumps(observation, sort_keys=True) + "\n", encoding="utf-8")
    completed = run_verifier(observation_path)
    if completed.returncode != 1:
        raise RuntimeError(f"candidate did not exit 1: {candidate.name}\n{completed.stdout}")
    missing = [token for token in expected if token not in completed.stdout]
    if missing:
        raise RuntimeError(
            f"candidate diagnostic starvation name={candidate.name} missing={missing}\n{completed.stdout}"
        )
    return completed.stdout.strip()


def create_candidate(temp_root: Path, name: str, observation: dict[str, Any]) -> Path:
    candidate = temp_root / name
    service = candidate / "service"
    package = candidate / "package"
    enclave = candidate / "enclave"
    service.mkdir(parents=True)
    package.mkdir()
    enclave.mkdir()
    (service / "candidate.json").write_text(
        json.dumps({"attack": name, "role": "removal-service"}) + "\n", encoding="utf-8"
    )
    far = observation["expectedSizes"]["farAboveN"]
    far_row = next((row for row in observation["sizes"] if row["size"] == far), None)
    removed = far_row["removedMemberId"] if far_row else "absent-with-starved-size"
    (package / "candidate.json").write_text(
        json.dumps({"attack": name, "role": "removed-member-package", "memberId": removed}) + "\n",
        encoding="utf-8",
    )
    (enclave / "state.json").write_text(
        json.dumps({"attack": name, "memberId": removed, "discardRequired": True}) + "\n",
        encoding="utf-8",
    )
    return candidate


def inventory_probe(omitted: str | None) -> int:
    observed = set(EXPECTED_MUTANTS)
    if omitted:
        observed.remove(omitted)
    missing = set(EXPECTED_MUTANTS) - observed
    if missing:
        print(
            f"TASK6577_MUTANTS_EXIT=1 absent_attack=mutant:{','.join(sorted(missing))}",
            file=sys.stderr,
        )
        return 1
    print(f"TASK6577_MUTANTS_EXIT=0 mutants={len(observed)}")
    return 0


def main() -> int:
    args = parse_args()
    if args.inventory_probe:
        return inventory_probe(args.omit_mutant)
    if os.environ.get("CARGO_TARGET_DIR") != EXPECTED_TARGET:
        print(
            f"TASK6577_MUTANTS_EXIT=1 target={os.environ.get('CARGO_TARGET_DIR', 'absent')} expected={EXPECTED_TARGET}",
            file=sys.stderr,
        )
        return 1
    if tuple(MUTANTS) != EXPECTED_MUTANTS or set(EXPECTED_DIAGNOSTICS) != set(EXPECTED_MUTANTS):
        print("TASK6577_MUTANTS_EXIT=1 absent_attack=mutant-inventory-definition", file=sys.stderr)
        return 1

    temp_root = Path(tempfile.mkdtemp(prefix="task-6577-candidates-"))
    mutant_count = 0
    size_starvation_count = 0
    mutant_starvation_count = 0
    try:
        baseline = capture_baseline(temp_root)
        n = baseline["n"]
        far = baseline["expectedSizes"]["farAboveN"]

        for name, mutate in MUTANTS.items():
            candidate_observation = copy.deepcopy(baseline)
            mutate(candidate_observation, n, far)
            candidate = create_candidate(temp_root, name, candidate_observation)
            diagnostic = verify_candidate(candidate, candidate_observation, EXPECTED_DIAGNOSTICS[name])
            mutant_count += 1
            print(f"TASK6577_MUTANT name={name} exit=1 {diagnostic}")
            shutil.rmtree(candidate)
            if candidate.exists():
                raise RuntimeError(f"candidate was not discarded: {candidate}")

        for role in ("belowN", "atN", "farAboveN"):
            candidate_observation = copy.deepcopy(baseline)
            absent_size = candidate_observation["expectedSizes"][role]
            candidate_observation["sizes"] = [
                row for row in candidate_observation["sizes"] if row["size"] != absent_size
            ]
            candidate = create_candidate(temp_root, f"absent-size-{role}", candidate_observation)
            diagnostic = verify_candidate(
                candidate,
                candidate_observation,
                ("attack=absent_size", f"role={role}", f"absent_size={absent_size}"),
            )
            size_starvation_count += 1
            print(f"TASK6577_STARVATION omitted_size={role}:{absent_size} exit=1 {diagnostic}")
            shutil.rmtree(candidate)

        for name in EXPECTED_MUTANTS:
            completed = subprocess.run(
                [sys.executable, __file__, "--inventory-probe", "--omit-mutant", name],
                cwd=ROOT,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                timeout=30,
                check=False,
            )
            expected = f"absent_attack=mutant:{name}"
            if completed.returncode != 1 or expected not in completed.stdout:
                raise RuntimeError(f"mutant starvation probe failed name={name}\n{completed.stdout}")
            mutant_starvation_count += 1
            print(f"TASK6577_STARVATION omitted_mutant={name} exit=1 {expected}")
    finally:
        shutil.rmtree(temp_root)

    if temp_root.exists():
        print(f"TASK6577_MUTANTS_EXIT=1 discard_root={temp_root}", file=sys.stderr)
        return 1
    print(
        "TASK6577_MUTANTS_EXIT=0 "
        f"mutants={mutant_count} size_starvations={size_starvation_count} "
        f"mutant_starvations={mutant_starvation_count} "
        f"services_discarded={mutant_count + size_starvation_count} "
        f"packages_discarded={mutant_count + size_starvation_count} "
        f"enclaves_discarded={mutant_count + size_starvation_count} candidate_root_discarded=true"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
