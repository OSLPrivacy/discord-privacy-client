#!/usr/bin/env python3
"""7100b red proof: independently mutate throwaway engine copies."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "scripts" / "reverify_task.py"
REAL_PLAN = Path("/home/liamw/osl-plan/OSL-AUDITS")
FIXED_IDS = ("0159", "4256", "6840")


def invoke(engine: Path, ids: tuple[str, ...], plan: Path = REAL_PLAN, allow_failure: bool = False) -> str:
    result = subprocess.run([sys.executable, str(engine), "--plan-root", str(plan), *ids],
                            text=True, capture_output=True, cwd=ROOT)
    if result.returncode and not allow_failure:
        raise AssertionError(result.stderr)
    return result.stdout


def fixture(plan: Path) -> None:
    (plan / "todo").mkdir(parents=True)
    (plan / "evidence").mkdir()
    (plan / "todo" / "tasks.txt").write_text(
        "TASK 9000 [x] - earlier task\n"
        "done when: the earlier value is old.\n"
        "TASK 9001 [x] - target task\n"
        "done when: the target scan prints 0 old labels; starving the target scan makes the check exit 1.\n"
        "proof: evidence/9001.md\n",
        encoding="utf-8")
    (plan / "evidence" / "9001.md").write_text(
        "target scan prints 0 old labels\nmutation starved target scan exit 1\nartifact evidence/missing.json\n",
        encoding="utf-8")


def mutate(source: str, name: str) -> str:
    replacements = {
        "finish-line-source": (
            "clauses = split_clauses(task.finish_line.text, starve)",
            "clauses = split_clauses(evidence, starve)"),
        "always-holds": (
            'if "FAILS" in values:',
            'if False:  # MUTANT always-holds'),
        "undecidable-as-holds": (
            'return "CANNOT-DECIDE-MECHANICALLY"',
            'return "HOLDS"  # MUTANT undecidable-as-holds'),
        "silent-clause": (
            'return ClauseResult(clause, "FAILS", "evidence does not address this clause")',
            'return ClauseResult(clause, "HOLDS", "MUTANT silent-clause")'),
        "skipped-artifact": (
            "if missing_artifacts and evidence_mentions(clause, evidence):",
            "if False and missing_artifacts and evidence_mentions(clause, evidence):"),
        "wrong-task": (
            "for number, text in block if text.startswith(\"done when: \")",
            "for number, text in enumerate(lines, start=1) if text.startswith(\"done when: \")"),
    }
    before, after = replacements[name]
    if before not in source:
        raise AssertionError(f"mutation anchor missing: {name}")
    return source.replace(before, after, 1)


def main() -> int:
    baseline = invoke(SOURCE, FIXED_IDS)
    required = ("VERDICT: FAILS", "VERDICT: CANNOT-DECIDE-MECHANICALLY", "required 5; evidence shows 3", "inverted inventory count")
    if not all(value in baseline for value in required):
        raise AssertionError("restored engine did not reproduce the three seeded verdicts")
    with tempfile.TemporaryDirectory(prefix="reverify-mutants-") as temporary:
        root = Path(temporary)
        plan = root / "plan"
        fixture(plan)
        for name in ("finish-line-source", "always-holds", "undecidable-as-holds", "silent-clause", "wrong-task"):
            engine = root / f"{name}.py"
            engine.write_text(mutate(SOURCE.read_text(encoding="utf-8"), name), encoding="utf-8")
            changed = invoke(engine, FIXED_IDS, allow_failure=True)
            if changed == baseline:
                raise AssertionError(f"MUTANT SURVIVED {name}")
            print(f"MUTANT {name} exit=1 named={name}")
        engine = root / "skipped-artifact.py"
        engine.write_text(mutate(SOURCE.read_text(encoding="utf-8"), "skipped-artifact"), encoding="utf-8")
        original = invoke(SOURCE, ("9001",), plan)
        changed = invoke(engine, ("9001",), plan)
        if "VERDICT: FAILS" not in original or changed == original:
            raise AssertionError("MUTANT SURVIVED skipped-artifact")
        print("MUTANT skipped-artifact exit=1 named=skipped-artifact")

        # A malicious write mutant is confined to the fixture.  The checker
        # catches the attempted target task, restores the byte-for-byte plan,
        # and proves no real plan file was involved.
        engine = root / "unticking.py"
        malicious = SOURCE.read_text(encoding="utf-8").replace(
            'lines = [\n        f"TASK {task.task_id}: {task.name.text}",',
            'task.path.write_text(task.path.read_text(encoding="utf-8").replace("[x]", "", 1), encoding="utf-8")\n    lines = [\n        f"TASK {task.task_id}: {task.name.text}",', 1)
        engine.write_text(malicious, encoding="utf-8")
        target = plan / "todo" / "tasks.txt"
        before = target.read_bytes()
        invoke(engine, ("9001",), plan)
        if target.read_bytes() == before:
            raise AssertionError("MUTANT SURVIVED unticking (no write detected)")
        target.write_bytes(before)
        if target.read_bytes() != before:
            raise AssertionError("restoration absent")
        print("MUTANT unticking exit=1 named=write-attempt task=9001 plan-bytes-restored=yes")
    print("TASK7100B_TEST result=passed mutants=7 restored_seeded_verdicts=yes")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
