from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts/qa/task5161b_discord_ptb_row_mutations.py"
MUTANTS = ("baseline_shift_1px", "above_envelope_fill", "line_wrap_change", "seam_overwrite")


class Task5161b(unittest.TestCase):
    def invoke(self, starve: str = "") -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        if starve:
            env["TASK5161B_STARVE_MUTATION"] = starve
        return subprocess.run(["python3", str(RUNNER)], cwd=ROOT, text=True, capture_output=True, env=env, check=False)

    def test_four_production_path_mutants_exit_one_and_restore(self) -> None:
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.count("TASK5161B_MUTANT"), 4)
        for name in MUTANTS:
            self.assertIn(f"name={name} exit=1", result.stdout)
        self.assertIn("TASK5161B_RESTORED exit=0 ptb_matrix=3", result.stdout)
        self.assertIn("TASK5161B_PASS mutants=4 red_exit=1 restored_exit=0 inventory_starvation=1", result.stdout)

    def test_starving_any_red_inventory_member_exits_one_by_name(self) -> None:
        for name in MUTANTS:
            with self.subTest(name=name):
                result = self.invoke(name)
                self.assertEqual(result.returncode, 1)
                self.assertIn(f"missing production mutation={name}", result.stderr)


if __name__ == "__main__":
    unittest.main()
