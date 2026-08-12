#!/usr/bin/env python3
"""Run throwaway TASK 6140b crypto, boundary, route, and starvation mutants."""

from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/check_task6140b_modern_crypto.py"
SPEC = importlib.util.spec_from_file_location("task6140b_checker", CHECKER)
gate = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = gate
SPEC.loader.exec_module(gate)


@dataclass(frozen=True)
class Mutation:
    mutation_id: str
    domain: str
    category: str
    defect: str
    observed: str
    apply: Callable[[dict[str, Any]], None]
    cell: str = "none"
    constructor: str = "none"
    threshold: str = "none"
    part: str = "none"
    route: str = "none"


def argon2id() -> dict[str, Any]:
    return {
        "algorithm": "Argon2id",
        "saltBits": 128,
        "memoryKiB": 65_536,
        "iterations": 3,
        "parallelism": 1,
        "freshSalt": True,
        "outputBits": 256,
    }


def domain_proof(domain: str, index: int) -> dict[str, Any]:
    recovery = domain == "recovery"
    return {
        "domain": domain,
        "primitive": {
            "algorithm": "Ed25519" if recovery else "XChaCha20-Poly1305-IETF",
            "library": "ed25519-dalek" if recovery else "chacha20poly1305",
            "version": "2.2.0" if recovery else "0.10.1",
            "maintained": True,
            "custom": False,
            "downgradeAllowed": False,
        },
        "key": {
            "bits": 256,
            "provenance": "CSPRNG:OsRng",
            "keyId": f"6140-{domain}-key-{index}",
            "fresh": True,
            "lowEntropy": False,
            "deterministicallyPadded": False,
            "sharedAcrossDomains": False,
            "separation": f"OSL/6140/{domain}/v1",
        },
        "argon2id": argon2id() if domain in gate.PASSWORD_DOMAINS else None,
        "nonce": None if recovery else {
            "bits": 192,
            "source": "CSPRNG:OsRng",
            "fresh": True,
            "nonceId": f"6140-{domain}-nonce-{index}",
        },
        "boundFields": list(gate.BOUND_FIELDS),
        "oracle": {
            "legitimateOpens": 1,
            "noSecretOpens": 0,
            "wrongKeyOpens": 0,
            "tamperOpens": 0,
            "downgradeOpens": 0,
            "crossDomainOpens": 0,
            "exactBytes": True,
        },
        "discarded": {"candidate": True, "key": True, "account": True},
    }


def baseline_contract() -> dict[str, Any]:
    paths = []
    observations = []
    for index, expected in enumerate(gate.expected_paths()):
        paths.append({
            **expected,
            "encrypted": True,
            "authenticated": True,
            "beforeRenderCommit": True,
            "capturePlaintext": 0,
            "exactArrivals": 1,
            "exactBytes": True,
        })
        observations.append({
            "pathId": expected["pathId"],
            "requestId": f"provider-request-{index:02d}",
            "observed": True,
            "route": "bundled-tor",
            "directEgressBytes": 0,
        })
    return {
        "schemaVersion": 1,
        "frozenInventorySha256": hashlib.sha256(b"TASK 6140 frozen pre-candidate inventory").hexdigest(),
        "domainInventory": list(gate.DOMAINS),
        "domainProofs": [domain_proof(domain, index) for index, domain in enumerate(gate.DOMAINS)],
        "carrierMatrix": [{
            "implementationId": gate.IMPLEMENTATION_ID,
            "cell": gate.CELL,
            "constructors": [
                {"name": constructor, "threshold": threshold}
                for constructor, threshold in gate.CONSTRUCTORS
            ],
        }],
        "carrierPaths": paths,
        "torObservations": observations,
        "requiredMutants": list(gate.REQUIRED_MUTANTS),
        "discarded": {"candidates": True, "keys": True, "accounts": True},
    }


def proof_for(contract: dict[str, Any], domain: str) -> dict[str, Any]:
    return next(item for item in contract["domainProofs"] if item["domain"] == domain)


def mutate_proof(domain: str, section: str, **changes: Any) -> Callable[[dict[str, Any]], None]:
    return lambda contract: proof_for(contract, domain)[section].update(changes)


def mutate_argon(domain: str, **changes: Any) -> Callable[[dict[str, Any]], None]:
    return lambda contract: proof_for(contract, domain)["argon2id"].update(changes)


def path_id(constructor: str, threshold: int, case: str) -> str:
    return f"{gate.IMPLEMENTATION_ID}:{gate.CELL}:{constructor}:{threshold}:{case}"


def mutate_path(
    constructor: str,
    threshold: int,
    case: str,
    **changes: Any,
) -> Callable[[dict[str, Any]], None]:
    wanted = path_id(constructor, threshold, case)
    return lambda contract: next(
        item for item in contract["carrierPaths"] if item["pathId"] == wanted
    ).update(changes)


def mutate_route(constructor: str, threshold: int, case: str) -> Callable[[dict[str, Any]], None]:
    wanted = path_id(constructor, threshold, case)
    return lambda contract: next(
        item for item in contract["torObservations"] if item["pathId"] == wanted
    ).update(route="direct", directEgressBytes=1)


def mutations() -> list[Mutation]:
    items: list[Mutation] = []
    for domain in gate.DOMAINS:
        items.extend([
            Mutation(
                f"{domain}:custom-toy-primitive", domain, "primitive", "custom-toy-primitive",
                "functioning_custom_toy=1",
                mutate_proof(domain, "primitive", custom=True, algorithm="ToyPrimitive"),
            ),
            Mutation(
                f"{domain}:32-bit-key", domain, "strength", "key-bits",
                "effective_bits=32", mutate_proof(domain, "key", bits=32),
            ),
            Mutation(
                f"{domain}:padded-low-entropy-key", domain, "strength", "padded-low-entropy-key",
                "source_bits=32 padded_to=256",
                mutate_proof(domain, "key", deterministicallyPadded=True),
            ),
            Mutation(
                f"{domain}:shared-cross-domain-key", domain, "separation", "shared-cross-domain-key",
                "cross_domain_key_reuse=1", mutate_proof(domain, "key", sharedAcrossDomains=True),
            ),
            Mutation(
                f"{domain}:algorithm-kdf-downgrade", domain, "downgrade", "algorithm-kdf-downgrade",
                "downgrade_accepted=1", mutate_proof(domain, "primitive", downgradeAllowed=True),
            ),
        ])
    for domain in gate.PASSWORD_DOMAINS:
        items.extend([
            Mutation(
                f"{domain}:weak-argon-salt", domain, "kdf", "weak-argon-salt",
                "salt_bits=64", mutate_argon(domain, saltBits=64),
            ),
            Mutation(
                f"{domain}:weak-argon-memory", domain, "kdf", "weak-argon-memory",
                "memory_kib=32768", mutate_argon(domain, memoryKiB=32_768),
            ),
            Mutation(
                f"{domain}:weak-argon-iterations", domain, "kdf", "weak-argon-iterations",
                "iterations=2", mutate_argon(domain, iterations=2),
            ),
            Mutation(
                f"{domain}:weak-argon-parallelism", domain, "kdf", "weak-argon-parallelism",
                "parallelism=0", mutate_argon(domain, parallelism=0),
            ),
        ])
    case_parts = {case: part for case, _offset, part in gate.CASES}
    for constructor, threshold in gate.CONSTRUCTORS:
        for case in gate.QUALIFYING_BOUNDARY_CASES:
            common = {
                "domain": "carrier",
                "cell": gate.CELL,
                "constructor": constructor,
                "threshold": str(threshold),
                "part": "none" if case_parts[case] is None else str(case_parts[case]),
            }
            items.extend([
                Mutation(
                    f"carrier:{constructor}:{threshold}:{case}:plaintext",
                    category="primitive", defect="boundary-plaintext-or-public",
                    observed="replacement=plaintext", apply=mutate_path(
                        constructor, threshold, case, encrypted=False, capturePlaintext=1,
                    ), **common,
                ),
                Mutation(
                    f"carrier:{constructor}:{threshold}:{case}:public-encoding",
                    category="primitive", defect="boundary-plaintext-or-public",
                    observed="replacement=public-encoding", apply=mutate_path(
                        constructor, threshold, case, encrypted=False, capturePlaintext=1,
                    ), **common,
                ),
                Mutation(
                    f"carrier:{constructor}:{threshold}:{case}:unauthenticated-encryption",
                    category="authentication", defect="boundary-unauthenticated",
                    observed="replacement=unauthenticated-encryption", apply=mutate_path(
                        constructor, threshold, case, authenticated=False,
                    ), **common,
                ),
                Mutation(
                    f"carrier:{constructor}:{threshold}:{case}:direct-route",
                    category="route", defect="direct-route", route="direct",
                    observed="route=direct direct_egress_bytes=1",
                    apply=mutate_route(constructor, threshold, case), **common,
                ),
            ])
    return items


def write_json(path: Path, value: Any) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def run_checker(contract: dict[str, Any], directory: Path, label: str) -> subprocess.CompletedProcess[str]:
    path = directory / f"{label}.json"
    write_json(path, contract)
    return subprocess.run(
        [sys.executable, str(CHECKER), "--contract", str(path)],
        text=True,
        capture_output=True,
        check=False,
    )


def require_green(result: subprocess.CompletedProcess[str], label: str) -> None:
    if result.returncode != 0 or "TASK6140B_OK" not in result.stdout:
        raise RuntimeError(f"{label} expected green: {result.stdout}{result.stderr}")


def require_red(
    result: subprocess.CompletedProcess[str],
    domain: str,
    category: str,
    defect: str,
    *,
    cell: str = "none",
    constructor: str = "none",
    threshold: str = "none",
    part: str = "none",
    route: str = "none",
) -> None:
    output = result.stdout + result.stderr
    expected = (
        "TASK6140B_FAIL",
        f"domain={domain}",
        f"category={category}",
        f"defect={defect}",
        f"cell={cell}",
        f"constructor={constructor}",
        f"threshold={threshold}",
        f"part={part}",
        f"route={route}",
    )
    if result.returncode != 1 or any(item not in output for item in expected):
        raise RuntimeError(f"expected named red {expected}: exit={result.returncode} {output}")


def prove_mutants(directory: Path, starved: str | None) -> list[str]:
    inventory = mutations()
    ids = [item.mutation_id for item in inventory]
    if ids != list(gate.REQUIRED_MUTANTS):
        raise RuntimeError(f"driver/checker mutant inventory differs driver={ids} checker={gate.REQUIRED_MUTANTS}")
    if starved is not None:
        if starved not in ids:
            raise RuntimeError(f"unknown starved driver mutant={starved}")
        raise RuntimeError(f"starved driver mutant={starved}")
    baseline = baseline_contract()
    require_green(run_checker(baseline, directory, "baseline"), "baseline")
    lines = []
    for index, mutation in enumerate(inventory):
        candidate = copy.deepcopy(baseline)
        mutation.apply(candidate)
        red = run_checker(candidate, directory, f"mutant-{index}")
        require_red(
            red, mutation.domain, mutation.category, mutation.defect,
            cell=mutation.cell, constructor=mutation.constructor,
            threshold=mutation.threshold, part=mutation.part, route=mutation.route,
        )
        require_green(run_checker(baseline, directory, f"restored-{index}"), mutation.mutation_id)
        lines.append(
            f"TASK6140B_MUTANT id={mutation.mutation_id} exit=1 domain={mutation.domain} "
            f"category={mutation.category} defect={mutation.defect} cell={mutation.cell} "
            f"constructor={mutation.constructor} threshold={mutation.threshold} part={mutation.part} "
            f"route={mutation.route} observed={mutation.observed} restored_exit=0 discarded=true"
        )
    return lines


def prove_starvation(directory: Path) -> tuple[list[str], int]:
    baseline = baseline_contract()
    lines: list[str] = []
    count = 0
    for index, domain in enumerate(gate.DOMAINS):
        candidate = copy.deepcopy(baseline)
        candidate["domainProofs"] = [item for item in candidate["domainProofs"] if item["domain"] != domain]
        require_red(
            run_checker(candidate, directory, f"starve-domain-{index}"),
            domain, "starvation", f"domain:{domain}",
        )
        count += 1
    lines.append(f"TASK6140B_STARVATION kind=domain count={len(gate.DOMAINS)} each_exit=1")

    for index, path in enumerate(gate.expected_paths()):
        candidate = copy.deepcopy(baseline)
        candidate["carrierPaths"] = [item for item in candidate["carrierPaths"] if item["pathId"] != path["pathId"]]
        require_red(
            run_checker(candidate, directory, f"starve-path-{index}"),
            "carrier", "starvation", f"path:{path['pathId']}", cell=gate.CELL,
            constructor=path["constructor"], threshold=str(path["threshold"]),
            part="none" if path["part"] is None else str(path["part"]),
        )
        count += 1
    lines.append(f"TASK6140B_STARVATION kind=path count={len(gate.expected_paths())} each_exit=1")

    for index, path in enumerate(gate.expected_paths()):
        candidate = copy.deepcopy(baseline)
        candidate["torObservations"] = [item for item in candidate["torObservations"] if item["pathId"] != path["pathId"]]
        require_red(
            run_checker(candidate, directory, f"starve-tor-{index}"),
            "carrier", "starvation", f"tor:{path['pathId']}", cell=gate.CELL,
            constructor=path["constructor"], threshold=str(path["threshold"]),
            part="none" if path["part"] is None else str(path["part"]), route="absent",
        )
        count += 1
    lines.append(f"TASK6140B_STARVATION kind=tor_observation count={len(gate.expected_paths())} each_exit=1")

    for index, mutant in enumerate(gate.REQUIRED_MUTANTS):
        candidate = copy.deepcopy(baseline)
        candidate["requiredMutants"].remove(mutant)
        require_red(
            run_checker(candidate, directory, f"starve-mutant-{index}"),
            "contract", "starvation", f"mutant:{mutant}",
        )
        count += 1
    lines.append(f"TASK6140B_STARVATION kind=mutant count={len(gate.REQUIRED_MUTANTS)} each_exit=1")
    return lines, count


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--starve-driver-mutant", choices=list(gate.REQUIRED_MUTANTS))
    args = parser.parse_args(argv)
    try:
        with tempfile.TemporaryDirectory(prefix="task-6140b-") as temporary:
            directory = Path(temporary)
            lines = prove_mutants(directory, args.starve_driver_mutant)
            starvation, starvation_count = prove_starvation(directory)
            lines.extend(starvation)
        for line in lines:
            print(line)
        print(
            f"TASK6140B_PASS domains={len(gate.DOMAINS)} password_domains={len(gate.PASSWORD_DOMAINS)} "
            f"carrier_cells=1 constructors={len(gate.CONSTRUCTORS)} paths={len(gate.expected_paths())} "
            f"tor_observations={len(gate.expected_paths())} mutants={len(gate.REQUIRED_MUTANTS)} "
            f"red={len(gate.REQUIRED_MUTANTS)} restored={len(gate.REQUIRED_MUTANTS)} "
            f"starvation_red={starvation_count} direct_egress_bytes=0 candidates_keys_accounts_discarded=true"
        )
    except (OSError, RuntimeError) as error:
        print(
            "TASK6140B_FAIL domain=driver category=driver defect=execution cell=none "
            f"constructor=none threshold=none part=none route=none detail={error}",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
