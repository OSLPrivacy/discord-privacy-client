from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

from scripts.qa import task6581_backup_failure_boundary as gate


ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts/qa/task6581b_backup_failure_mutations.py"


def invoke(environment: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(RUNNER)],
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )


class Task6581MutationTests(unittest.TestCase):
    def test_all_separate_candidates_are_red_restoration_is_green_and_discarded(self) -> None:
        with tempfile.TemporaryDirectory(prefix="task6581b-test-") as directory:
            environment = os.environ.copy()
            environment["TMPDIR"] = directory
            result = invoke(environment)
            self.assertEqual(result.returncode, 0, result.stderr)
            lines = [line for line in result.stdout.splitlines() if line.startswith("TASK6581B MUTANT")]
            self.assertEqual(len(lines), len(gate.MUTANTS))
            for name in gate.MUTANTS:
                self.assertTrue(any(f"name={name} " in line for line in lines), name)
            semantic_mutants = {
                *(f"hide-axis-{axis}" for axis in gate.AXES),
                "omit-loss-hosted-d1",
                "omit-loss-hosted-r2",
                "claim-off-site",
                "claim-independent",
                "claim-disaster-isolated",
                "promise-recovery",
                "forged-smaller-inventory",
                "remove-task-6582",
                "defer-deletion",
            }
            for name in semantic_mutants:
                line = next(line for line in lines if f"name={name} " in line)
                for field in ("edge=", "affected_data=", "expected=", "actual="):
                    self.assertIn(field, line, f"{name} did not name {field}")
            self.assertTrue(
                any("name=forged-smaller-inventory " in line and "attack=self-derived-copy" in line for line in lines)
            )
            self.assertIn(
                f"TASK6581B PASS mutants={len(gate.MUTANTS)} red_exit=1 restoration=green "
                f"inventories_discarded={len(gate.MUTANTS)} candidates_discarded={len(gate.MUTANTS)} temp_remaining=0",
                result.stdout,
            )
            self.assertEqual(list(Path(directory).iterdir()), [])

    def test_removing_any_mutant_makes_red_proof_name_the_starvation(self) -> None:
        for name in gate.MUTANTS:
            with self.subTest(mutant=name):
                environment = os.environ.copy()
                environment["TASK6581_STARVE_MUTANT"] = name
                result = invoke(environment)
                self.assertEqual(result.returncode, 1)
                self.assertIn(f"absent mutant starvation={name}", result.stderr)
        print(f"TASK6581B_STARVATION mutants={len(gate.MUTANTS)} exit=1 named=true")


if __name__ == "__main__":
    unittest.main()
