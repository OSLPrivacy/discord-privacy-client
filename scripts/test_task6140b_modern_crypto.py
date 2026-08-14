#!/usr/bin/env python3
"""Focused unit tests for TASK 6140b's contract checker and red campaign."""

from __future__ import annotations

import copy
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/check_task6140b_modern_crypto.py"
PROOF = ROOT / "scripts/prove_task6140b_modern_crypto_mutants.py"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


gate = load("task6140b_checker_test", CHECKER)
proof = load("task6140b_proof_test", PROOF)


class Task6140bModernCryptoTests(unittest.TestCase):
    maxDiff = None

    def run_proof(self, *arguments: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(PROOF), *arguments],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_full_campaign_has_exact_counts_and_named_boundary_route_failures(self) -> None:
        result = self.run_proof()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(
            "TASK6140B_PASS domains=5 password_domains=3 carrier_cells=1 constructors=2 "
            "paths=14 tor_observations=14 mutants=61 red=61 restored=61 "
            "starvation_red=94 direct_egress_bytes=0 candidates_keys_accounts_discarded=true",
            result.stdout,
        )
        self.assertEqual(result.stdout.count("TASK6140B_MUTANT id="), 61)
        for fact in (
            "id=recovery:custom-toy-primitive exit=1 domain=recovery category=primitive",
            "id=enclave-export:weak-argon-memory exit=1 domain=enclave-export category=kdf defect=weak-argon-memory",
            "id=personal-export:padded-low-entropy-key",
            "id=backup:shared-cross-domain-key",
            "id=carrier:prepare_peer_prose_text_inner:1000:threshold-plus-one:plaintext",
            "constructor=prepare_peer_prose_text_inner threshold=1000 case=threshold-plus-one byte_count=1001 part=none",
            "id=carrier:split_native_overlay_text:40960:multipart-middle:unauthenticated-encryption",
            "constructor=split_native_overlay_text threshold=40960 case=multipart-middle byte_count=81921 part=1",
            "id=carrier:split_native_overlay_text:40960:final-part-after-long-prefix:direct-route",
            "threshold=40960 case=final-part-after-long-prefix byte_count=286721 part=7 route=direct",
            "operation=provider-route primitive=none effective_key_bits=none kdf=none observed=route=direct direct_egress_bytes=1 support=Supported protection=green delivery=green small_tor=green",
            "id=recovery:32-bit-key exit=1 domain=recovery category=strength defect=key-bits",
            "object=task6140-recovery-object operation=verify primitive=Ed25519 effective_key_bits=32 kdf=none",
            "TASK6140B_STARVATION kind=domain count=5 each_exit=1",
            "TASK6140B_STARVATION kind=path count=14 each_exit=1",
            "TASK6140B_STARVATION kind=tor_observation count=14 each_exit=1",
            "TASK6140B_STARVATION kind=mutant count=61 each_exit=1",
        ):
            self.assertIn(fact, result.stdout)

    def test_baseline_exact_matrix_domains_parameters_aad_and_tor(self) -> None:
        baseline = proof.baseline_contract()
        result = gate.validate(copy.deepcopy(baseline))
        self.assertEqual(
            result,
            {
                "domains": 5,
                "password_domains": 3,
                "carrier_cells": 1,
                "constructors": 2,
                "paths": 14,
                "tor_observations": 14,
                "mutants": 61,
            },
        )
        self.assertEqual(baseline["domainInventory"], list(gate.DOMAINS))
        self.assertEqual(len(baseline["carrierPaths"]), 14)
        self.assertEqual(len(baseline["torObservations"]), 14)
        self.assertEqual(baseline["carrierMatrix"][0]["support"], "Supported")
        for domain in gate.PASSWORD_DOMAINS:
            argon = proof.proof_for(baseline, domain)["argon2id"]
            self.assertEqual(
                (argon["saltBits"], argon["memoryKiB"], argon["iterations"], argon["parallelism"]),
                (128, 65_536, 3, 1),
            )
        for domain in gate.DOMAINS:
            domain_proof = proof.proof_for(baseline, domain)
            self.assertEqual(domain_proof["boundFields"], list(gate.BOUND_FIELDS))
            self.assertEqual(domain_proof["key"]["bits"], 256)
            self.assertEqual(domain_proof["objectId"], f"task6140-{domain}-object")
            self.assertIn(domain_proof["operation"], ("verify", "open", "restore", "send"))
            self.assertEqual(domain_proof["key"]["effectiveBits"], 128 if domain == "recovery" else 256)
            self.assertTrue(domain_proof["key"]["fresh"])
        for path, observation in zip(baseline["carrierPaths"], baseline["torObservations"]):
            self.assertTrue(path["encrypted"] and path["authenticated"] and path["beforeRenderCommit"])
            self.assertEqual(path["capturePlaintext"], 0)
            self.assertEqual(path["exactArrivals"], 1)
            self.assertEqual(path["prefixBytes"], (path["part"] or 0) * path["threshold"])
            self.assertEqual(observation["route"], "bundled-tor")
            self.assertEqual(observation["directEgressBytes"], 0)

    def test_each_throwaway_mutant_is_independently_named_and_rejected(self) -> None:
        baseline = proof.baseline_contract()
        inventory = proof.mutations()
        self.assertEqual([item.mutation_id for item in inventory], list(gate.REQUIRED_MUTANTS))
        with tempfile.TemporaryDirectory(prefix="task-6140b-unit-") as temporary:
            directory = Path(temporary)
            for index, mutation in enumerate(inventory):
                candidate = copy.deepcopy(baseline)
                mutation.apply(candidate)
                proof.require_red(
                    proof.run_checker(candidate, directory, f"unit-{index}"),
                    mutation.domain,
                    mutation.category,
                    mutation.defect,
                    cell=mutation.cell,
                    constructor=mutation.constructor,
                    threshold=mutation.threshold,
                    part=mutation.part,
                    route=mutation.route,
                    case=mutation.case,
                    byte_count=mutation.byte_count,
                    object_id=mutation.object_id,
                    operation=mutation.operation,
                    primitive=mutation.primitive,
                    effective_key_bits=mutation.effective_key_bits,
                    kdf=mutation.kdf,
                )

    def test_every_domain_path_tor_observation_and_mutant_starvation_is_red(self) -> None:
        baseline = proof.baseline_contract()
        with tempfile.TemporaryDirectory(prefix="task-6140b-starve-") as temporary:
            directory = Path(temporary)
            for index, domain in enumerate(gate.DOMAINS):
                candidate = copy.deepcopy(baseline)
                candidate["domainProofs"] = [item for item in candidate["domainProofs"] if item["domain"] != domain]
                proof.require_red(
                    proof.run_checker(candidate, directory, f"domain-{index}"),
                    domain,
                    "starvation",
                    f"domain:{domain}",
                )
            for index, path in enumerate(gate.expected_paths()):
                common = {
                    "cell": gate.CELL,
                    "constructor": path["constructor"],
                    "threshold": str(path["threshold"]),
                    "part": "none" if path["part"] is None else str(path["part"]),
                }
                no_path = copy.deepcopy(baseline)
                no_path["carrierPaths"] = [item for item in no_path["carrierPaths"] if item["pathId"] != path["pathId"]]
                proof.require_red(
                    proof.run_checker(no_path, directory, f"path-{index}"),
                    "carrier", "starvation", f"path:{path['pathId']}", **common,
                    case=path["case"], byte_count=str(path["bytes"]),
                    object_id=path["pathId"], operation="send",
                )
                no_tor = copy.deepcopy(baseline)
                no_tor["torObservations"] = [item for item in no_tor["torObservations"] if item["pathId"] != path["pathId"]]
                proof.require_red(
                    proof.run_checker(no_tor, directory, f"tor-{index}"),
                    "carrier", "starvation", f"tor:{path['pathId']}", route="absent", **common,
                    case=path["case"], byte_count=str(path["bytes"]),
                    object_id=path["pathId"], operation="provider-route",
                )
            for index, mutant in enumerate(gate.REQUIRED_MUTANTS):
                candidate = copy.deepcopy(baseline)
                candidate["requiredMutants"].remove(mutant)
                proof.require_red(
                    proof.run_checker(candidate, directory, f"mutant-{index}"),
                    "contract", "starvation", f"mutant:{mutant}",
                )

    def test_removing_any_driver_mutant_is_a_named_failure(self) -> None:
        for mutant in gate.REQUIRED_MUTANTS:
            result = self.run_proof("--starve-driver-mutant", mutant)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            for fact in (
                "TASK6140B_FAIL",
                "domain=driver",
                "category=driver",
                "defect=execution",
                f"starved driver mutant={mutant}",
            ):
                self.assertIn(fact, result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
