#!/usr/bin/env python3
"""Run cap/no-cap mutants and transcript-starvation proofs for Task 6586."""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RULES = ROOT / "crates/ipc/src/membership_size_rules.rs"
CHECK = ROOT / "scripts/task-6586-check.py"


def run(command: list[str], *, expect_green: bool, label: str) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, env=os.environ.copy())
    if (completed.returncode == 0) != expect_green:
        sys.stdout.write(completed.stdout)
        sys.stderr.write(completed.stderr)
        raise RuntimeError(f"{label}: exit={completed.returncode} expected_green={expect_green}")
    return completed


def cargo_mutant(label: str, replacement: tuple[str, str], product: str) -> None:
    pristine = RULES.read_text(encoding="utf-8")
    old, new = replacement
    if pristine.count(old) != 1:
        raise RuntimeError(f"{label}: mutation point count={pristine.count(old)}")
    try:
        RULES.write_text(pristine.replace(old, new), encoding="utf-8")
        completed = run(
            [
                "cargo", "test", "-p", "ipc", "--test", "task_6586_membership_size_rules",
                "--", "--nocapture", "--test-threads=1",
            ],
            expect_green=False,
            label=label,
        )
        combined = completed.stdout + completed.stderr
        if "task_6586" not in combined:
            raise RuntimeError(f"{label}: red output did not name focused Task 6586 test")
        print(f"TASK6586_MUTANT_RED product={product} mutant={label} cargo_exit={completed.returncode}")
    finally:
        RULES.write_text(pristine, encoding="utf-8")


def starve(transcript: str, label: str, old: str, new: str, product: str, directory: Path) -> None:
    if transcript.count(old) < 1:
        raise RuntimeError(f"{label}: transcript point count=0")
    path = directory / f"{label}.log"
    path.write_text(transcript.replace(old, new, 1), encoding="utf-8")
    completed = run(
        [sys.executable, str(CHECK), "--transcript", str(path), "--skip-source"],
        expect_green=False,
        label=label,
    )
    combined = completed.stdout + completed.stderr
    if f"product={product}" not in combined:
        raise RuntimeError(f"{label}: diagnostic did not name product={product}: {combined}")
    print(f"TASK6586_STARVATION_RED product={product} starvation={label} check_exit={completed.returncode}")


def main() -> int:
    if os.environ.get("CARGO_TARGET_DIR") != "/mnt/d/osl-lane-targets/c":
        print("TASK6586_MUTATION_EXIT=1 product=group_chat+enclave starvation=cargo_target", file=sys.stderr)
        return 1
    try:
        cargo_mutant(
            "group_cap_19",
            ("pub const GROUP_CHAT_MAX_PEOPLE: usize = 20;", "pub const GROUP_CHAT_MAX_PEOPLE: usize = 19;"),
            "group_chat",
        )
        cargo_mutant(
            "group_cap_21",
            ("pub const GROUP_CHAT_MAX_PEOPLE: usize = 20;", "pub const GROUP_CHAT_MAX_PEOPLE: usize = 21;"),
            "group_chat",
        )
        no_max_body = "pub fn enforce_enclave_candidate_size(_candidate_people: usize) -> Result<(), &'static str> {\n    debug_assert_eq!(enclave_admission_rule().maximum_people, None);\n    Ok(())\n}"
        cargo_mutant(
            "enclave_cap_at_n",
            (
                no_max_body,
                "pub fn enforce_enclave_candidate_size(candidate_people: usize) -> Result<(), &'static str> {\n    if candidate_people > 3 { Err(\"Enclave full at N\") } else { Ok(()) }\n}",
            ),
            "enclave",
        )
        cargo_mutant(
            "shared_group_limiter",
            (
                no_max_body,
                "pub fn enforce_enclave_candidate_size(candidate_people: usize) -> Result<(), &'static str> {\n    enforce_group_chat_candidate_size(candidate_people)\n}",
            ),
            "enclave",
        )

        with tempfile.TemporaryDirectory(prefix="task-6586-") as temporary:
            directory = Path(temporary)
            baseline = run(
                [sys.executable, str(CHECK)], expect_green=True, label="restored_baseline"
            ).stdout
            cases = [
                ("group_product", "group_chat_max=20", "group_chat_max=absent", "group_chat"),
                ("enclave_product", "enclave_max=null", "enclave_max=20", "enclave"),
                ("person_20", "admitted=1-20", "admitted=1-19", "group_chat"),
                ("person_21", "refused=21", "refused=20", "group_chat"),
                ("measured_N", "admitted_sizes=3,20,21,103", "admitted_sizes=20,21,103", "enclave"),
                ("generated_larger", "generated_larger=103", "generated_larger=102", "enclave"),
                ("refusal_atomicity", "key_hash_unchanged=true", "key_hash_unchanged=false", "group_chat"),
                ("progress", "progress_samples=11", "progress_samples=1", "enclave"),
                ("rekey", "fresh_epoch=42", "fresh_epoch=41", "enclave"),
            ]
            for label, old, new, product in cases:
                starve(baseline, label, old, new, product, directory)
        restored = run([sys.executable, str(CHECK)], expect_green=True, label="final_restore")
        print(restored.stdout, end="")
        print("TASK6586_MUTATION_EXIT=0 mutants=4 restored=4 starvation_red=9 final_green=true")
        return 0
    except (OSError, RuntimeError) as error:
        print(f"TASK6586_MUTATION_EXIT=1 {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
