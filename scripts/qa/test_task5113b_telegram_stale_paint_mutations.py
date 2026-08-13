from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path

from scripts.qa import task5113b_telegram_stale_paint_mutations as proof


ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts/qa/task5113b_telegram_stale_paint_mutations.py"


class Task5113bMutationTests(unittest.TestCase):
    def test_production_mutants_turn_parent_acceptance_red_then_restore_green(self) -> None:
        completed = subprocess.run(["python3", str(RUNNER)], cwd=ROOT, text=True, capture_output=True, check=False)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        lines = [line for line in completed.stdout.splitlines() if line.startswith("TASK5113B MUTANT")]
        self.assertEqual(len(lines), len(proof.MUTANTS))
        for name in proof.MUTANTS:
            self.assertTrue(any(f"name={name} " in line for line in lines), name)
        self.assertIn(f"TASK5113B PASS mutants={len(proof.MUTANTS)} red_exit=101 restored_exit=0", completed.stdout)

    def test_starving_the_red_proof_names_the_missing_production_mutation(self) -> None:
        for name in proof.MUTANTS:
            with self.subTest(mutant=name):
                environment = os.environ.copy()
                environment["TASK5113B_STARVE_MUTANT"] = name
                completed = subprocess.run(["python3", str(RUNNER)], cwd=ROOT, env=environment, text=True, capture_output=True, check=False)
                self.assertEqual(completed.returncode, 1)
                self.assertIn(f"missing production mutation={name}", completed.stderr)


if __name__ == "__main__":
    unittest.main()
