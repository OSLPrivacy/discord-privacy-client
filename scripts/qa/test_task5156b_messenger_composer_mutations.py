#!/usr/bin/env python3
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
PROOF = HERE / "task5156b_messenger_composer_visual_break.py"
CHECKER = HERE / "task5156_messenger_composer_fidelity.py"
EXPECTED_IMAGE = HERE / "test_task5156_messenger_composer_fidelity.py"
MUTANTS = (
    "hidden_candidate",
    "catalogue_fixture",
    "edge_shift_1px",
    "packaged_shipping_renderer",
)


def invoke(*arguments: str, starve: str = "") -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    environment["TASK5156B_STARVE_MUTATION"] = starve
    return subprocess.run(
        ["python3", str(PROOF), *arguments],
        cwd=HERE.parents[1],
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )


class Task5156bProofTests(unittest.TestCase):
    def test_all_four_separate_copy_mutants_are_red_and_restoration_is_green(self) -> None:
        result = invoke()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count("TASK5156B_MUTANT"), 4)
        for mutant in MUTANTS:
            self.assertIn(f"name={mutant} ", result.stdout)
        for defect in ("hidden candidate", "catalogue fixture", "edge defect", "illegal shipping promotion"):
            self.assertIn(f"defect={defect}", result.stdout)
        self.assertIn("TASK5156B_RESTORED exit=0 state_passes=3", result.stdout)
        self.assertIn("TASK5156B_PASS mutants=4 qualification_mutants=3 shipping_mutants=1 restoration=1", result.stdout)
        print(result.stdout.strip())

    def test_starving_each_qualification_or_shipping_mutation_exits_one_by_name(self) -> None:
        for mutant in MUTANTS:
            with self.subTest(mutation=mutant):
                result = invoke(starve=mutant)
                scope = "shipping" if mutant == "packaged_shipping_renderer" else "qualification"
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn(f"TASK5156B_FAIL=starved {scope} mutation: {mutant}", result.stderr)
                print(result.stderr.strip())

    def test_checker_and_expected_image_edits_cannot_pass(self) -> None:
        with tempfile.TemporaryDirectory(prefix="task5156b-tamper-") as directory:
            root = Path(directory)
            checker = root / "checker.py"
            checker.write_bytes(CHECKER.read_bytes() + b"\n# tampered\n")
            result = invoke("--checker", str(checker))
            self.assertEqual(result.returncode, 1)
            self.assertIn("TASK5156B_FAIL=checker edit refused", result.stderr)
            print(result.stderr.strip())

            expected_image = root / "expected-image.py"
            expected_image.write_bytes(EXPECTED_IMAGE.read_bytes().replace(b"x * 29", b"x * 31", 1))
            result = invoke("--expected-image", str(expected_image))
            self.assertEqual(result.returncode, 1)
            self.assertIn("TASK5156B_FAIL=expected-image edit refused", result.stderr)
            print(result.stderr.strip())


if __name__ == "__main__":
    unittest.main()
