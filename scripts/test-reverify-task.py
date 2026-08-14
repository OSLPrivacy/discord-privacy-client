#!/usr/bin/env python3
"""Focused acceptance and starvation tests for scripts/reverify_task.py."""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
ENGINE = ROOT / "scripts" / "reverify_task.py"


def run(*args: str, expected: int = 0) -> str:
    result = subprocess.run([sys.executable, str(ENGINE), *args], text=True,
                            capture_output=True, cwd=ROOT)
    if result.returncode != expected:
        raise AssertionError(f"expected {expected}, got {result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
    return result.stdout + result.stderr


def fixture(root: Path, task_id: str, finish: str, evidence: str | None) -> None:
    todo = root / "todo"
    evidence_dir = root / "evidence"
    todo.mkdir(parents=True, exist_ok=True)
    evidence_dir.mkdir(exist_ok=True)
    task_file = todo / "tasks.txt"
    with task_file.open("a", encoding="utf-8") as handle:
        handle.write(
        f"TASK {task_id} [x] - fixture\n"
        f"done when: {finish}\n"
        "done: today by lane fixture\n"
        f"proof: evidence/{task_id}.md\n")
    if evidence is not None:
        (evidence_dir / f"{task_id}.md").write_text(evidence, encoding="utf-8")


def main() -> int:
    with tempfile.TemporaryDirectory(prefix="reverify-engine-") as temporary:
        plan = Path(temporary) / "plan"
        fixture(plan, "9000", "the widget scan prints 0 old labels; starving the widget scan makes the check exit 1.",
                "widget scan prints 0 old labels\nmutation starved the widget scan and exit 1\n")
        output = run("--plan-root", str(plan), "9000")
        assert "VERDICT: HOLDS" in output and "CLAUSES:" in output and "tasks.txt:1" in output

        fixture(plan, "9003", "the suffix evidence is read.", "suffix evidence is read\n")
        (plan / "evidence" / "9003.md").rename(plan / "evidence" / "9003-suffix.md")
        output = run("--plan-root", str(plan), "9003")
        assert "VERDICT: HOLDS" in output and "9003-suffix.md:1" in output

        fixture(plan, "9001", "the second scan prints 0 old labels.", None)
        output = run("--plan-root", str(plan), "9001")
        assert "VERDICT: CANNOT-DECIDE-MECHANICALLY" in output and "missing evidence file" in output

        fixture(plan, "9002", "the artifact is present.", "artifact evidence/missing.json is present\n")
        output = run("--plan-root", str(plan), "9002")
        assert "VERDICT: FAILS" in output and "evidence/missing.json | MISSING" in output

        for input_name in ("todo read", "evidence read", "clause split", "artifact check"):
            output = run("--plan-root", str(plan), "--starve", input_name, "9000", expected=1)
            assert f"absent input: {input_name}" in output, output

    # Fixed real-plan seeds are the regression contract commissioned by 7100.
    output = run("0159")
    assert "VERDICT: FAILS" in output and "exactly these 5" in output
    output = run("4256")
    assert "VERDICT: FAILS" in output and "inverted inventory count" in output
    assert output.count("DONE:") == 3 and output.count("PROOF:") == 3
    output = run("6840")
    assert "VERDICT: CANNOT-DECIDE-MECHANICALLY" in output and "starvation arm was never run" in output
    output = run("--trace-handles", "0159")
    assert "HANDLE TRACE /proc/self/fd" in output and "writable_under_todo_or_evidence=0" in output

    # The 20-id caller intentionally prints independent verdict lines only;
    # it has no aggregate, pass-total, or combined-class output.
    ids = "0159 4256 6840 0412 4403 6875 3126b 6812 0011 5099 0037 5182 4336b 0160 0201 0223 0405 0413 0429 0430".split()
    output = run(*ids)
    assert "VERDICT: HOLDS" in output
    assert "VERDICT: FAILS" in output
    assert "VERDICT: CANNOT-DECIDE-MECHANICALLY" in output
    assert "combined" not in output.lower() and "pass count" not in output.lower()
    print("TASK7100_TEST result=passed seeded=3 starvation=4 batch_ids=20")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
