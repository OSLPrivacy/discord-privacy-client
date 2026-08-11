from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.qa import task6581_backup_failure_boundary as gate


ROOT = Path(__file__).resolve().parents[2]
ORACLE = ROOT / "contracts/task-6580-backup-failure-domain-oracle.json"


def invoke(oracle: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "python3",
            str(ROOT / "scripts/qa/task6581_backup_failure_boundary.py"),
            "--root",
            str(ROOT),
            "--oracle",
            str(oracle),
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


class Task6581BoundaryTests(unittest.TestCase):
    def test_current_failure_domain_copy_passes(self) -> None:
        result = subprocess.run(
            ["python3", str(ROOT / "scripts/qa/task6581_backup_failure_boundary.py")],
            cwd=ROOT,
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        for field in ("provider_iam_reconciliation=matched", "axes=7", "shared_axes=7", "isolated_axes=0", "surfaces=9", "isolation_claims=0", "guaranteed_recovery_claims=0"):
            self.assertIn(field, result.stdout)

    def test_each_axis_and_surface_starvation_is_named(self) -> None:
        original = json.loads(ORACLE.read_text(encoding="utf-8"))
        for axis in gate.AXES:
            with self.subTest(axis=axis), tempfile.TemporaryDirectory(prefix="task6581-axis-") as directory:
                candidate = Path(directory) / "oracle.json"
                mutant = json.loads(json.dumps(original))
                mutant["axes"] = [row for row in mutant["axes"] if row["id"] != axis]
                candidate.write_text(json.dumps(mutant), encoding="utf-8")
                result = invoke(candidate)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"absent axis starvation axis={axis}", result.stderr)
        for surface in gate.SURFACES:
            with self.subTest(surface=surface), tempfile.TemporaryDirectory(prefix="task6581-surface-") as directory:
                candidate = Path(directory) / "oracle.json"
                mutant = json.loads(json.dumps(original))
                mutant["boundarySurfaces"] = [row for row in mutant["boundarySurfaces"] if row["id"] != surface]
                candidate.write_text(json.dumps(mutant), encoding="utf-8")
                result = invoke(candidate)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"surface starvation={surface}", result.stderr)
        print(f"TASK6581_STARVATION axes={len(gate.AXES)} surfaces={len(gate.SURFACES)} exit=1 named=true")

    def test_mutant_inventory_starvation_is_named(self) -> None:
        original = json.loads(ORACLE.read_text(encoding="utf-8"))
        for mutant_name in gate.MUTANTS:
            with self.subTest(mutant=mutant_name), tempfile.TemporaryDirectory(prefix="task6581-mutant-") as directory:
                candidate = Path(directory) / "oracle.json"
                mutant = json.loads(json.dumps(original))
                mutant["requiredMutants"].remove(mutant_name)
                candidate.write_text(json.dumps(mutant), encoding="utf-8")
                result = invoke(candidate)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn(f"mutant starvation={mutant_name}", result.stderr)
        print(f"TASK6581_MUTANT_INVENTORY_STARVATION mutants={len(gate.MUTANTS)} exit=1 named=true")


if __name__ == "__main__":
    unittest.main()
