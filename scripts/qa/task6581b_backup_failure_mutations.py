#!/usr/bin/env python3
"""Separate-copy red proof for TASK 6581/6580."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import task6581_backup_failure_boundary as gate


ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / "scripts/qa/task6581_backup_failure_boundary.py"
ORACLE_RELATIVE = "contracts/task-6580-backup-failure-domain-oracle.json"


def candidate_files() -> set[str]:
    files = set(gate.CONFIG_FACTS)
    files.update(path for _, path in gate.SURFACES.values())
    oracle = json.loads((ROOT / ORACLE_RELATIVE).read_text(encoding="utf-8"))
    files.update(row["path"] for row in oracle["deletionAndErasureObligations"])
    files.add(ORACLE_RELATIVE)
    return files


def copy_candidate(destination: Path) -> None:
    for relative in candidate_files():
        source = ROOT / relative
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def load_oracle(root: Path) -> dict:
    return json.loads((root / ORACLE_RELATIVE).read_text(encoding="utf-8"))


def save_oracle(root: Path, oracle: dict) -> None:
    (root / ORACLE_RELATIVE).write_text(json.dumps(oracle, indent=2) + "\n", encoding="utf-8")


def replace(root: Path, relative: str, old: str, new: str) -> None:
    path = root / relative
    text = path.read_text(encoding="utf-8")
    if old not in text:
        raise RuntimeError(f"mutation source missing path={relative} text={old}")
    path.write_text(text.replace(old, new), encoding="utf-8")


def mutate(root: Path, name: str) -> str:
    if name.startswith("hide-axis-"):
        axis = name.removeprefix("hide-axis-")
        oracle = load_oracle(root)
        oracle["axes"] = [row for row in oracle["axes"] if row["id"] != axis]
        save_oracle(root, oracle)
        return f"axis={axis}"

    if name.startswith("remove-surface-"):
        surface = name.removeprefix("remove-surface-")
        relative = gate.SURFACES[surface][1]
        (root / relative).unlink()
        return f"surface starvation={surface}"

    if name == "omit-loss-hosted-d1":
        replace(root, "docs/backup-and-disaster-recovery.md", "identities, prekeys, wrapped keys", "hosted records")
        return "data_class=hosted-d1"
    if name == "omit-loss-hosted-r2":
        replace(root, "cipher-store-cf/DEPLOY.md", "R2 message and attachment ciphertext", "Object payload bytes")
        return "data_class=hosted-r2"
    if name == "claim-off-site":
        with (root / "docs/backup-and-disaster-recovery.md").open("a", encoding="utf-8") as handle:
            handle.write("\nOff-site backup protection is active.\n")
        return "false words=off-site backup protection is active"
    if name == "claim-independent":
        with (root / "docs/backup-and-disaster-recovery.md").open("a", encoding="utf-8") as handle:
            handle.write("\nIndependent backup protection is active.\n")
        return "false words=independent backup protection is active"
    if name == "claim-disaster-isolated":
        with (root / "docs/backup-and-disaster-recovery.md").open("a", encoding="utf-8") as handle:
            handle.write("\nDisaster-isolated protection is active.\n")
        return "false words=disaster-isolated protection is active"
    if name == "promise-recovery":
        with (root / "docs/backup-and-disaster-recovery.md").open("a", encoding="utf-8") as handle:
            handle.write("\nRecovery is guaranteed after shared-domain failure.\n")
        return "false words=recovery is guaranteed after shared-domain failure"
    if name == "forged-smaller-inventory":
        oracle = load_oracle(root)
        oracle["axes"] = [row for row in oracle["axes"] if row["id"] != "credential"]
        for _, relative in gate.SURFACES.values():
            path = root / relative
            text = path.read_text(encoding="utf-8")
            path.write_text(text.replace("credential", "session").replace("Credential", "Session"), encoding="utf-8")
        save_oracle(root, oracle)
        return "axis=credential"
    if name == "remove-task-6582":
        replace(root, "README.md", "Task 6582", "Future work")
        return "missing deferred task=6582"
    if name == "defer-deletion":
        replace(
            root,
            "README.md",
            "an object whose delete credential OSL no longer holds is counted as a burn\n"
            "failure and kept for retry, and is never reported as destroyed",
            "backup deletion and erasure are deferred to task 6582",
        )
        return "wrongly deferred obligation=remote-delete-failure-retry"
    raise RuntimeError(f"unknown mutant={name}")


def invoke(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(GATE), "--root", str(root)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def main() -> int:
    starved = os.environ.get("TASK6581_STARVE_MUTANT", "")
    if starved:
        if starved not in gate.MUTANTS:
            print(f"TASK6581B FAIL unknown mutant starvation={starved}", file=sys.stderr)
            return 1
        print(f"TASK6581B FAIL absent mutant starvation={starved}", file=sys.stderr)
        return 1

    control = invoke(ROOT)
    if control.returncode != 0:
        print(f"TASK6581B FAIL green control failed: {control.stderr.strip()}", file=sys.stderr)
        return 1

    discarded = 0
    for name in gate.MUTANTS:
        candidate_path: Path | None = None
        with tempfile.TemporaryDirectory(prefix=f"task6581-{name}-") as directory:
            candidate_path = Path(directory) / "candidate"
            copy_candidate(candidate_path)
            expected = mutate(candidate_path, name)
            result = invoke(candidate_path)
            if result.returncode != 1 or expected not in result.stderr:
                print(
                    f"TASK6581B FAIL mutant={name} expected_exit=1 expected={expected} "
                    f"actual_exit={result.returncode} stderr={result.stderr.strip()}",
                    file=sys.stderr,
                )
                return 1
            print(f"TASK6581B MUTANT name={name} exit=1 diagnostic={expected}")
        if candidate_path is None or candidate_path.exists():
            print(f"TASK6581B FAIL candidate not discarded mutant={name}", file=sys.stderr)
            return 1
        discarded += 1

    restored = invoke(ROOT)
    if restored.returncode != 0:
        print(f"TASK6581B FAIL restoration failed: {restored.stderr.strip()}", file=sys.stderr)
        return 1
    print(
        f"TASK6581B PASS mutants={len(gate.MUTANTS)} red_exit=1 restoration=green "
        f"inventories_discarded={discarded} candidates_discarded={discarded} temp_remaining=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
