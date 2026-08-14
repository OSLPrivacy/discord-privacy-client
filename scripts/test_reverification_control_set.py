#!/usr/bin/env python3
"""Focused green and red proof for TASK 7101's declarative control set."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "scripts" / "run_reverification_control_set.py"
CONTROL = ROOT / "scripts" / "reverification-control-set.json"
ENGINE = ROOT / "scripts" / "reverify_task.py"
PLAN = Path("/home/liamw/osl-plan/OSL-AUDITS")


def invoke(*args: str, expected: int = 0) -> str:
    result = subprocess.run([sys.executable, str(RUNNER), *args], text=True, capture_output=True, cwd=ROOT)
    output = result.stdout + result.stderr
    if result.returncode != expected:
        raise AssertionError(f"expected exit {expected}, got {result.returncode}\n{output}")
    return output


def write_mutation(root: Path, change) -> Path:
    data = json.loads(CONTROL.read_text(encoding="utf-8"))
    change(data)
    path = root / "control.json"
    path.write_text(json.dumps(data), encoding="utf-8")
    return path


def main() -> int:
    output = invoke("--control-set", str(CONTROL), "--engine", str(ENGINE), "--plan-root", str(PLAN))
    assert "CONTROL SET LOADED:" in output and "rows=13" in output
    for task_id in ("0159", "4256", "0037", "5182", "4336b"):
        assert f"CONTROL {task_id} FAILS" in output
    for task_id in ("0011", "6840", "5099"):
        assert (f"CONTROL {task_id} CANNOT-DECIDE raw=CANNOT-DECIDE-MECHANICALLY" in output
                and "missing_surface=" in output)
    for task_id in ("0412", "4403", "6875", "3126b", "6812"):
        assert f"CONTROL {task_id} HOLDS PROVISIONAL" in output
    assert "CONTROL PASS TOTAL: 0" in output
    sweep = invoke("--control-set", str(CONTROL), "--engine", str(ENGINE),
                   "--plan-root", str(PLAN), "--sweep", "0011")
    assert "CONTROL SET PASS: sweep precondition satisfied" in sweep
    assert "SWEEP REPORTS:" in sweep and "TASK 0011:" in sweep

    missing = invoke("--control-set", str(ROOT / "scripts" / "no-such-control.json"), expected=1)
    assert "absent control file" in missing
    no_engine = invoke("--engine", str(ROOT / "scripts" / "no-such-engine.py"), expected=1)
    assert "absent engine" in no_engine

    with tempfile.TemporaryDirectory(prefix="task7101-controls-") as temporary:
        root = Path(temporary)
        absent_fails = write_mutation(root, lambda data: data["rows"].pop(0))
        output = invoke("--control-set", str(absent_fails), "--sweep", "0011", expected=1)
        assert "absent control row: 0159" in output
        assert "SWEEP REPORTS:" not in output and "TASK 0011:" not in output

        absent_class = write_mutation(root, lambda data: data["rows"].__setitem__(slice(5, 8), []))
        output = invoke("--control-set", str(absent_class), expected=1)
        assert "absent verdict class: CANNOT-DECIDE" in output

        absent_fails_class = write_mutation(root, lambda data: data["rows"].__setitem__(slice(0, 5), []))
        output = invoke("--control-set", str(absent_fails_class), expected=1)
        assert "absent verdict class: FAILS" in output

        absent_holds_class = write_mutation(root, lambda data: data["rows"].__setitem__(slice(8, 13), []))
        output = invoke("--control-set", str(absent_holds_class), expected=1)
        assert "absent verdict class: HOLDS" in output

        absent_provisional = write_mutation(root, lambda data: data["rows"][8].__setitem__("provisional", False))
        output = invoke("--control-set", str(absent_provisional), expected=1)
        assert "absent provisional marking: 0412" in output

        collapsed = root / "all-holds.py"
        collapsed.write_text(
            "#!/usr/bin/env python3\nimport sys\nprint('VERDICT: HOLDS')\n",
            encoding="utf-8",
        )
        output = invoke("--engine", str(collapsed), expected=1)
        assert "verdict collapse: every row returned HOLDS" in output

        holds_for_0011 = root / "holds-for-0011.py"
        holds_for_0011.write_text(
            "#!/usr/bin/env python3\nimport subprocess\nimport sys\n"
            "if sys.argv[-1] == '0011':\n"
            "    print('VERDICT: HOLDS')\n"
            "else:\n"
            f"    raise SystemExit(subprocess.run([sys.executable, {str(ENGINE)!r}, *sys.argv[1:]]).returncode)\n",
            encoding="utf-8",
        )
        output = invoke("--engine", str(holds_for_0011), expected=1)
        assert "control CANNOT-DECIDE row did not remain undecidable: 0011 returned HOLDS" in output, output

    print("TASK7101_TEST result=passed rows=13 fails=5 cannot_decide=3 provisional_holds=5 red_cases=9")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
