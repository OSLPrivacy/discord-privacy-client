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
