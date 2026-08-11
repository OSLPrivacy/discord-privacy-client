from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path


class Task6331BMutationMatrixTests(unittest.TestCase):
    def test_local_matrix_receipts(self) -> None:
        script = Path(__file__).with_name("task_6331b_mutation_matrix.py")
        completed = subprocess.run([sys.executable, str(script)], text=True, capture_output=True, check=False)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        lines = completed.stdout.splitlines()
        mutants = [line for line in lines if line.startswith("TASK6331B_LOCAL_MUTANT ")]
        self.assertEqual([line.split(" name=", 1)[1].split(" ", 1)[0] for line in mutants], [
            "quiet-501ms-stall", "busy-501ms-stall", "missing-marker-row", "missing-bound-probe",
            "uia-direct-input", "other-osl-control", "destroyed-identity",
        ])
        self.assertEqual(len(mutants), 7)
        for line in mutants:
            self.assertIn("exit=1 provenance=virtual-fixture installed=false reason=", line)
            self.assertIn("surface=[", line)
            self.assertIn("provider_id=", line)
            self.assertIn("input_id=", line)
        self.assertEqual(lines[-3], "TASK6331B_OUTSIDE name=900ms-outside-interval gate_exit=0 status=PASS provenance=virtual-fixture installed=false")
        self.assertEqual(lines[-2], "TASK6331B_RESTORED name=fresh-quiet-busy gate_exit=0 status=PASS provenance=virtual-fixture installed=false")
        self.assertRegex(lines[-1], r"^TASK6331B_SUMMARY oracle_sha256=[0-9a-f]{64} installed_builds=0 installed_mutants=0 local_mutants=7 forbidden_credit=0 sacrificial_control=unmeasured ingestion=unmeasured harness=unmeasured$")


if __name__ == "__main__":
    unittest.main()
