#!/usr/bin/env python3
"""Executable completeness contract for TASK 3206's known-limits list."""

from __future__ import annotations

import argparse
import re
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_LIMITS = ROOT / "docs/security/osl-known-limits.md"
DEFAULT_PROOF_DIR = ROOT / "proof"
ENTRY_RE = re.compile(r"^- (KL-\d{3}) \| TASK (\d{4}[a-z]?) \| (\S.*)$")
STILL_UNPROVEN_RE = re.compile(r"still\s+unproven", re.IGNORECASE)
EXPECTED_IDS = {f"KL-{number:03d}" for number in range(1, 37)}


def normalize(value: str) -> str:
    return re.sub(r"\s+", " ", value).strip()


def proof_lines(proof_dir: Path) -> list[tuple[str, int, str]]:
    found: list[tuple[str, int, str]] = []
    if not proof_dir.is_dir():
        raise ValueError(f"proof folder does not exist: {proof_dir}")
    for path in sorted(item for item in proof_dir.rglob("*") if item.is_file()):
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except (UnicodeDecodeError, OSError):
            continue
        for line_number, line in enumerate(lines, start=1):
            if STILL_UNPROVEN_RE.search(line):
                found.append((path.relative_to(proof_dir).as_posix(), line_number, normalize(line)))
    return found


def validate(text: str, proof_dir: Path) -> tuple[list[str], list[tuple[str, str, str]]]:
    errors: list[str] = []
    entries: list[tuple[str, str, str]] = []
    malformed = [
        (line_number, line)
        for line_number, line in enumerate(text.splitlines(), start=1)
        if line.startswith("- KL-") and not ENTRY_RE.fullmatch(line)
    ]
    for line_number, line in malformed:
        errors.append(f"malformed limit line {line_number}: {line}")

    for line in text.splitlines():
        match = ENTRY_RE.fullmatch(line)
        if match:
            entries.append(match.groups())

    ids = [entry[0] for entry in entries]
    duplicate_ids = sorted({entry_id for entry_id in ids if ids.count(entry_id) > 1})
    if duplicate_ids:
        errors.append("duplicate limit ids: " + ",".join(duplicate_ids))
    missing_ids = sorted(EXPECTED_IDS - set(ids))
    extra_ids = sorted(set(ids) - EXPECTED_IDS)
    if missing_ids:
        errors.append("missing required known limits: " + ",".join(missing_ids))
    if extra_ids:
        errors.append("unexpected known limits: " + ",".join(extra_ids))

    normalized_entry_lines = [normalize(line) for line in text.splitlines() if ENTRY_RE.fullmatch(line)]
    for relative_path, line_number, proof_line in proof_lines(proof_dir):
        occurrences = sum(proof_line in entry for entry in normalized_entry_lines)
        if occurrences != 1:
            errors.append(
                f"proof line {relative_path}:{line_number} appears on {occurrences} limit lines: {proof_line}"
            )

    return errors, entries


def self_test(text: str) -> list[str]:
    mutants: dict[str, tuple[str, str | None]] = {
        "missing-required-limit": (re.sub(r"^- KL-036 .*\n", "", text, count=1, flags=re.MULTILINE), None),
        "missing-task-number": (text.replace("- KL-034 | TASK 3401 |", "- KL-034 | TASK NONE |", 1), None),
        "unlisted-proof-line": (text, "TASK 9999: synthetic adapter is still unproven"),
    }
    survivors: list[str] = []
    for name, (mutant_text, injected_proof) in mutants.items():
        with tempfile.TemporaryDirectory(prefix="task3206-") as temporary:
            temporary_path = Path(temporary)
            if injected_proof is not None:
                (temporary_path / "synthetic-proof.txt").write_text(injected_proof + "\n", encoding="utf-8")
            mutant_errors, _ = validate(mutant_text, temporary_path)
            if not mutant_errors:
                survivors.append(name)
    return survivors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--limits", type=Path, default=DEFAULT_LIMITS)
    parser.add_argument("--proof-dir", type=Path, default=DEFAULT_PROOF_DIR)
    parser.add_argument("--list", action="store_true", dest="print_list")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    text = args.limits.read_text(encoding="utf-8")
    errors, entries = validate(text, args.proof_dir)
    unproven = proof_lines(args.proof_dir)
    print(f"TASK3206 known_limits={len(entries)}")
    print(f"TASK3206 task_numbered_lines={len(entries)}")
    print(f"TASK3206 proof_still_unproven_lines={len(unproven)}")
    print(f"TASK3206 proof_still_unproven_missing={sum(error.startswith('proof line ') for error in errors)}")

    if args.print_list:
        for entry_id, task, limit in entries:
            print(f"TASK3206 {entry_id} TASK {task} {limit}")

    if args.self_test:
        survivors = self_test(text)
        print(f"TASK3206 self_test_mutants=3 killed={3 - len(survivors)}")
        if survivors:
            print("TASK3206 surviving_mutants=" + ",".join(survivors))
            return 1

    if errors:
        for error in errors:
            print(f"TASK3206 ERROR: {error}")
        return 1
    print("TASK3206 known_limits=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
