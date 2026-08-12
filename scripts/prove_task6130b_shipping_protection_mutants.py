#!/usr/bin/env python3
"""Run independent throwaway contract mutants against the unchanged 6130b checker."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/check_task6130b_shipping_protection.py"

import importlib.util

SPEC = importlib.util.spec_from_file_location("task6130b_checker", CHECKER)
gate = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(gate)


@dataclass(frozen=True)
class Mutation:
    mutation_id: str
    constructor: str
    category: str
    field: str
    observed: str
    apply: Callable[[dict[str, Any]], None]


def context_pair(constructor: str, field: str) -> tuple[str, str]:
    original = f"{constructor}:original:{field}=original"
    target = f"{constructor}:target:{field}=different"
    return original, target


def baseline_contract() -> dict[str, Any]:
    proofs = []
    for index, constructor in enumerate(gate.CONSTRUCTORS):
        proofs.append({
            "implementationId": gate.IMPLEMENTATION_ID,
            "contentType": gate.CONTENT_TYPE,
            "constructor": constructor,
            "postBuild": {"uniquePayload": True, "realDisposableAccount": True, "independentRecipient": True},
            "primitive": {
                "algorithm": "AES-256-GCM", "library": "aes-gcm", "version": "0.10.3",
                "maintained": True, "custom": False, "downgradeAllowed": False,
            },
            "root": {
                "bits": 256, "provenance": "CSPRNG:OsRng", "rootId": f"root-{index}",
                "lowEntropy": False, "deterministicallyPadded": False, "sharedGlobal": False,
                "personEntered": False, "argon2id": None,
            },
            "kdf": {
                "name": "HKDF-SHA-256", "domain": f"OSL/6130/content-key/v1/{constructor}",
                "outputBits": 256, "contentKeyId": f"content-key-{index}", "separated": True,
            },
            "nonce": {"bits": 96, "source": "CSPRNG:OsRng", "nonceId": f"nonce-{index}", "fresh": True},
            "authentication": {"required": True, "beforeRendering": True},
            "aadFields": list(gate.AAD_FIELDS),
            "oracle": {"exactOpens": 1, "capturePlaintext": 0, "publicReaderOpens": 0},
            "attacks": {
                attack: {"plaintextReleased": 0, "deliveries": 0, "duplicateDeliveries": 0}
                for attack in gate.ATTACKS
            },
            "transplants": {
                field: {
                    "plaintextReleased": 0,
                    "deliveries": 0,
                    "originalOpens": 1,
                    "unchangedObject": True,
                    "originalContext": context_pair(constructor, field)[0],
                    "targetContext": context_pair(constructor, field)[1],
                }
                for field in gate.AAD_FIELDS
            },
            "discarded": {"candidate": True, "account": True},
        })
    return {
        "schemaVersion": 1,
        "frozenAuthoritySha256": hashlib.sha256(b"TASK 6101 frozen authority").hexdigest(),
        "supportedCells": [{
            "implementationId": gate.IMPLEMENTATION_ID,
            "contentType": gate.CONTENT_TYPE,
            "constructors": list(gate.CONSTRUCTORS),
        }],
        "productionSends": [
            {"implementationId": gate.IMPLEMENTATION_ID, "contentType": gate.CONTENT_TYPE, "constructor": constructor}
            for constructor in gate.CONSTRUCTORS
        ],
        "contextAxes": list(gate.AAD_FIELDS),
        "proofs": proofs,
        "requiredMutants": list(gate.REQUIRED_MUTANTS),
    }


def set_person_secret(contract: dict[str, Any], proof_index: int, **changes: Any) -> None:
    root = contract["proofs"][proof_index]["root"]
    root["personEntered"] = True
    root["argon2id"] = {
        "algorithm": "Argon2id", "saltBits": 128, "memoryKiB": 65_536,
        "iterations": 3, "parallelism": 1,
    }
    root["argon2id"].update(changes)


def update_proof(proof_index: int, section: str, **changes: Any) -> Callable[[dict[str, Any]], None]:
    return lambda contract: contract["proofs"][proof_index][section].update(changes)


def omit_aad(proof_index: int, field: str) -> Callable[[dict[str, Any]], None]:
    def apply(contract: dict[str, Any]) -> None:
        proof = contract["proofs"][proof_index]
        proof["aadFields"].remove(field)
        proof["transplants"][field].update({
            "plaintextReleased": 1,
            "deliveries": 1,
            "originalOpens": 1,
            "unchangedObject": True,
        })
    return apply


def mutations() -> list[Mutation]:
    items: list[Mutation] = []
    for proof_index, constructor in enumerate(gate.CONSTRUCTORS):
        def add(
            mutation_id: str,
            category: str,
            field: str,
            observed: str,
            apply: Callable[[dict[str, Any]], None],
        ) -> None:
            items.append(Mutation(
                f"{constructor}:{mutation_id}", constructor, category, field, observed, apply,
            ))

        add("public_reversible_transform", "primitive", "algorithm", "recovered_plaintext=1 transform=Base64", update_proof(proof_index, "primitive", algorithm="Base64"))
        add("raw_plaintext_cell", "primitive", "algorithm", "recovered_plaintext=1 transform=plaintext", update_proof(proof_index, "primitive", algorithm="PLAINTEXT"))
        add("custom_toy_authenticated_cipher", "primitive", "custom_primitive", "accepted_toy_cipher=1", update_proof(proof_index, "primitive", algorithm="ToyAEAD", custom=True))
        add("low_entropy_32_bit_root", "key_strength", "root.bits", "recovered_low_entropy_secret=1", update_proof(proof_index, "root", bits=32, lowEntropy=True, deterministicallyPadded=True))
        add("weak_argon2id_salt", "kdf", "argon2id.salt_bits", "accepted_weak_argon2id=1", lambda c, i=proof_index: set_person_secret(c, i, saltBits=64))
        add("weak_argon2id_memory", "kdf", "argon2id.memory_kib", "accepted_weak_argon2id=1", lambda c, i=proof_index: set_person_secret(c, i, memoryKiB=32_768))
        add("weak_argon2id_iterations", "kdf", "argon2id.iterations", "accepted_weak_argon2id=1", lambda c, i=proof_index: set_person_secret(c, i, iterations=2))
        add("weak_argon2id_parallelism", "kdf", "argon2id.parallelism", "accepted_weak_argon2id=1", lambda c, i=proof_index: set_person_secret(c, i, parallelism=0))
        add("shared_global_content_key", "key_strength", "shared_global_root", "accepted_shared_global_key=1", update_proof(proof_index, "root", sharedGlobal=True))
        add("missing_hkdf_domain", "kdf", "hkdf.domain", "accepted_missing_domain=1", update_proof(proof_index, "kdf", domain=""))
        add(
            "bypassed_non_text_cell", "matrix", "unsupported_production_send", "recovered_plaintext=1 bypassed_cell=attachment",
            lambda c, ctor=constructor: c["productionSends"].append({
                "implementationId": gate.IMPLEMENTATION_ID,
                "contentType": "attachment",
                "constructor": ctor,
            }),
        )
        other_index = 1 - proof_index
        add(
            "reused_nonce_material", "nonce", "fresh", "accepted_nonce_reuse=1",
            lambda c, i=proof_index, other=other_index: c["proofs"][i]["nonce"].update({
                "nonceId": c["proofs"][other]["nonce"]["nonceId"], "fresh": False,
            }),
        )
        add("missing_authentication", "authentication", "required", "accepted_unauthenticated_plaintext=1", update_proof(proof_index, "authentication", required=False))
        add("algorithm_downgrade", "primitive", "downgrade", "accepted_downgrade=1", update_proof(proof_index, "primitive", downgradeAllowed=True))
        add("unseparated_content_key", "kdf", "content_key.separated", "accepted_unseparated_key=1", update_proof(proof_index, "kdf", separated=False))
        for attack in gate.ATTACKS:
            add(
                f"accepted_{attack}", "attack", attack,
                f"accepted_{attack}=1 plaintext_released=1 deliveries=1",
                lambda c, i=proof_index, attack=attack: c["proofs"][i]["attacks"][attack].update({
                    "plaintextReleased": 1, "deliveries": 1,
                    "duplicateDeliveries": 1 if attack == "replay" else 0,
                }),
            )
        for field in gate.AAD_FIELDS:
            original, target = context_pair(constructor, field)
            add(
                f"omit_aad:{field}", "aad", field,
                f"cross_context_open=1 unchanged_object=true original={original} target={target}",
                omit_aad(proof_index, field),
            )
    return items


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def run_checker(contract: dict[str, Any], directory: Path, label: str) -> subprocess.CompletedProcess[str]:
    path = directory / f"{label}.json"
    write_json(path, contract)
    return subprocess.run(
        [sys.executable, str(CHECKER), "--contract", str(path)],
        text=True, capture_output=True, check=False,
    )


def require_green(result: subprocess.CompletedProcess[str], label: str) -> None:
    if result.returncode != 0 or "TASK6130B_OK" not in result.stdout:
        raise RuntimeError(f"{label} expected green: {result.stdout}{result.stderr}")


def require_red(
    result: subprocess.CompletedProcess[str],
    category: str,
    field: str,
    cell: str = gate.CONTENT_TYPE,
    constructor: str | None = None,
) -> None:
    output = result.stdout + result.stderr
    required = (
        "TASK6130B_FAIL", f"adapter={gate.IMPLEMENTATION_ID}", f"cell={cell}",
        f"category={category}", f"field={field}",
    )
    expected = required + ((f"constructor={constructor}",) if constructor else ())
    if result.returncode != 1 or any(item not in output for item in expected):
        raise RuntimeError(f"expected named red {expected}: exit={result.returncode} {output}")


def prove_mutants(directory: Path, starved: str | None) -> list[str]:
    all_mutations = mutations()
    if [item.mutation_id for item in all_mutations] != list(gate.REQUIRED_MUTANTS):
        raise RuntimeError("driver mutation inventory differs from independent checker inventory")
    if starved is not None:
        all_mutations = [item for item in all_mutations if item.mutation_id != starved]
        if len(all_mutations) == len(gate.REQUIRED_MUTANTS):
            raise RuntimeError(f"unknown starved driver mutant={starved}")
        raise RuntimeError(f"starved driver mutant={starved}")

    baseline = baseline_contract()
    require_green(run_checker(baseline, directory, "baseline"), "baseline")
    lines: list[str] = []
    for index, item in enumerate(all_mutations):
        candidate = copy.deepcopy(baseline)
        item.apply(candidate)
        kind = item.mutation_id.split(":", 1)[1]
        cell = "attachment" if kind == "bypassed_non_text_cell" else gate.CONTENT_TYPE
        red = run_checker(candidate, directory, f"mutant-{index}")
        require_red(red, item.category, item.field, cell, item.constructor)
        require_green(run_checker(baseline, directory, f"restored-{index}"), f"restored {item.mutation_id}")
        lines.append(
            f"TASK6130B_MUTANT id={item.mutation_id} exit=1 adapter={gate.IMPLEMENTATION_ID} "
            f"constructor={item.constructor} cell={cell} category={item.category} "
            f"field={item.field} observed={item.observed} restored_exit=0 discarded=true"
        )
    return lines


def prove_starvation(directory: Path) -> list[str]:
    baseline = baseline_contract()
    lines: list[str] = []

    no_cell = copy.deepcopy(baseline)
    no_cell["supportedCells"] = []
    require_red(run_checker(no_cell, directory, "starve-cell"), "starvation", "supported_cell")
    lines.append("TASK6130B_STARVATION kind=cell count=1 each_exit=1")

    for index, constructor in enumerate(gate.CONSTRUCTORS):
        candidate = copy.deepcopy(baseline)
        candidate["supportedCells"][0]["constructors"].remove(constructor)
        result = run_checker(candidate, directory, f"starve-constructor-{index}")
        require_red(result, "matrix", "supported_cells")
    lines.append(f"TASK6130B_STARVATION kind=constructor count={len(gate.CONSTRUCTORS)} each_exit=1")

    for index, constructor in enumerate(gate.CONSTRUCTORS):
        candidate = copy.deepcopy(baseline)
        candidate["proofs"] = [
            row for row in candidate["proofs"] if row["constructor"] != constructor
        ]
        require_red(
            run_checker(candidate, directory, f"starve-proof-{index}"),
            "starvation", f"proof:{constructor}", constructor=constructor,
        )
    lines.append(f"TASK6130B_STARVATION kind=proof count={len(gate.CONSTRUCTORS)} each_exit=1")

    for index, field in enumerate(gate.AAD_FIELDS):
        candidate = copy.deepcopy(baseline)
        candidate["contextAxes"].remove(field)
        require_red(run_checker(candidate, directory, f"starve-axis-{index}"), "starvation", f"context_axis:{field}")
    lines.append(f"TASK6130B_STARVATION kind=context_axis count={len(gate.AAD_FIELDS)} each_exit=1")

    attack_starvations = 0
    for proof_index, constructor in enumerate(gate.CONSTRUCTORS):
        for attack in gate.ATTACKS:
            candidate = copy.deepcopy(baseline)
            del candidate["proofs"][proof_index]["attacks"][attack]
            require_red(
                run_checker(candidate, directory, f"starve-attack-{proof_index}-{attack}"),
                "starvation", f"attack:{attack}", constructor=constructor,
            )
            attack_starvations += 1
    lines.append(f"TASK6130B_STARVATION kind=attack count={attack_starvations} each_exit=1")

    for index, mutant in enumerate(gate.REQUIRED_MUTANTS):
        candidate = copy.deepcopy(baseline)
        candidate["requiredMutants"].remove(mutant)
        require_red(run_checker(candidate, directory, f"starve-mutant-{index}"), "starvation", f"mutant:{mutant}")
    lines.append(f"TASK6130B_STARVATION kind=mutant count={len(gate.REQUIRED_MUTANTS)} each_exit=1")
    return lines


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--starve-driver-mutant", choices=list(gate.REQUIRED_MUTANTS))
    args = parser.parse_args(argv)
    try:
        with tempfile.TemporaryDirectory(prefix="task-6130b-") as temporary:
            directory = Path(temporary)
            lines = prove_mutants(directory, args.starve_driver_mutant)
            lines.extend(prove_starvation(directory))
        for line in lines:
            print(line)
        print(
            f"TASK6130B_PASS mutants={len(gate.REQUIRED_MUTANTS)} red={len(gate.REQUIRED_MUTANTS)} "
            f"restored={len(gate.REQUIRED_MUTANTS)} supported_cells=1 constructors=2 "
            f"context_axes={len(gate.AAD_FIELDS)} attacks={len(gate.ATTACKS) * len(gate.CONSTRUCTORS)} "
            f"starvation_red={1 + 2 * len(gate.CONSTRUCTORS) + len(gate.AAD_FIELDS) + len(gate.ATTACKS) * len(gate.CONSTRUCTORS) + len(gate.REQUIRED_MUTANTS)} "
            "candidates_discarded=true"
        )
    except (OSError, RuntimeError) as error:
        print(
            f"TASK6130B_FAIL adapter={gate.IMPLEMENTATION_ID} constructor=driver cell={gate.CONTENT_TYPE} "
            f"category=driver field=execution detail={error}",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
