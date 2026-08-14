#!/usr/bin/env python3
"""Focused tests for the independent TASK 6130b contract and mutant driver."""

from __future__ import annotations

import copy
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/check_task6130b_shipping_protection.py"
PROOF = ROOT / "scripts/prove_task6130b_shipping_protection_mutants.py"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


gate = load("task6130b_checker_test", CHECKER)
proof = load("task6130b_proof_test", PROOF)


class Task6130bProtectionContractTests(unittest.TestCase):
    def run_proof(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(PROOF), *arguments], text=True, capture_output=True, check=False)

    def test_exact_frozen_matrix_policy_mutants_and_restorations(self) -> None:
        result = self.run_proof()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(
            "TASK6130B_PASS mutants=64 red=64 restored=64 supported_cells=1 "
            "constructors=2 context_axes=12 attacks=10 starvation_red=91 "
            "candidates_discarded=true",
            result.stdout,
        )
        self.assertEqual(result.stdout.count("TASK6130B_MUTANT id="), 64)
        for fact in (
            "id=prepare_peer_prose_text_inner:public_reversible_transform exit=1 adapter=native.discord.text.v1 constructor=prepare_peer_prose_text_inner cell=text category=primitive field=algorithm observed=recovered_plaintext=1",
            "id=split_native_overlay_text:raw_plaintext_cell exit=1 adapter=native.discord.text.v1 constructor=split_native_overlay_text cell=text category=primitive field=algorithm observed=recovered_plaintext=1",
            "id=prepare_peer_prose_text_inner:custom_toy_authenticated_cipher",
            "id=split_native_overlay_text:bypassed_non_text_cell",
            "id=prepare_peer_prose_text_inner:accepted_replay",
            "id=split_native_overlay_text:omit_aad:version",
            "observed=cross_context_open=1 unchanged_object=true original=split_native_overlay_text:original:version=original target=split_native_overlay_text:target:version=different",
        ):
            self.assertIn(fact, result.stdout)

    def test_each_mutant_is_named_and_independently_rejected(self) -> None:
        baseline = proof.baseline_contract()
        with tempfile.TemporaryDirectory(prefix="task-6130b-unit-") as temporary:
            directory = Path(temporary)
            for index, mutation in enumerate(proof.mutations()):
                candidate = copy.deepcopy(baseline)
                mutation.apply(candidate)
                result = proof.run_checker(candidate, directory, f"unit-{index}")
                kind = mutation.mutation_id.split(":", 1)[1]
                cell = "attachment" if kind == "bypassed_non_text_cell" else gate.CONTENT_TYPE
                proof.require_red(
                    result, mutation.category, mutation.field, cell, mutation.constructor,
                )
                if kind.startswith("omit_aad:"):
                    field = kind.split(":", 1)[1]
                    changed_proof = next(
                        row for row in candidate["proofs"]
                        if row["constructor"] == mutation.constructor
                    )
                    transplant = changed_proof["transplants"][field]
                    self.assertEqual(transplant["plaintextReleased"], 1)
                    self.assertEqual(transplant["originalOpens"], 1)
                    self.assertTrue(transplant["unchangedObject"])
        self.assertEqual(len(proof.mutations()), len(gate.REQUIRED_MUTANTS))

    def test_removing_any_context_axis_or_required_mutant_exits_one(self) -> None:
        baseline = proof.baseline_contract()
        with tempfile.TemporaryDirectory(prefix="task-6130b-starve-") as temporary:
            directory = Path(temporary)
            for index, field in enumerate(gate.AAD_FIELDS):
                candidate = copy.deepcopy(baseline)
                candidate["contextAxes"].remove(field)
                proof.require_red(
                    proof.run_checker(candidate, directory, f"axis-{index}"),
                    "starvation", f"context_axis:{field}",
                )
            for index, mutant in enumerate(gate.REQUIRED_MUTANTS):
                candidate = copy.deepcopy(baseline)
                candidate["requiredMutants"].remove(mutant)
                proof.require_red(
                    proof.run_checker(candidate, directory, f"mutant-{index}"),
                    "starvation", f"mutant:{mutant}",
                )

            for proof_index, constructor in enumerate(gate.CONSTRUCTORS):
                for attack in gate.ATTACKS:
                    candidate = copy.deepcopy(baseline)
                    del candidate["proofs"][proof_index]["attacks"][attack]
                    proof.require_red(
                        proof.run_checker(candidate, directory, f"attack-{proof_index}-{attack}"),
                        "starvation", f"attack:{attack}", constructor=constructor,
                    )

    def test_driver_mutant_starvation_exits_one_and_names_absence(self) -> None:
        for mutant in gate.REQUIRED_MUTANTS:
            result = self.run_proof("--starve-driver-mutant", mutant)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            for fact in (
                "TASK6130B_FAIL", "adapter=native.discord.text.v1", "constructor=driver", "cell=text",
                "category=driver", "field=execution", f"starved driver mutant={mutant}",
            ):
                self.assertIn(fact, result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
