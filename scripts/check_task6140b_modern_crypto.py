#!/usr/bin/env python3
"""Fail-closed JSON contract checker for TASK 6140b modern cryptography."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DOMAINS = ("recovery", "enclave-export", "personal-export", "backup", "carrier")
PASSWORD_DOMAINS = ("enclave-export", "personal-export", "backup")
AEAD_DOMAINS = ("enclave-export", "personal-export", "backup", "carrier")
CELL = "text"
IMPLEMENTATION_ID = "native.discord.text.v1"
CONSTRUCTORS = (
    ("prepare_peer_prose_text_inner", 1_000),
    ("split_native_overlay_text", 40_960),
)
CASES = (
    ("threshold-minus-one", -1, None),
    ("exact-threshold", 0, None),
    ("threshold-plus-one", 1, None),
    ("multipart-first", 1, 0),
    ("multipart-middle", 1, 1),
    ("multipart-final", 1, 2),
    ("final-part-after-long-prefix", 1, 7),
)
QUALIFYING_BOUNDARY_CASES = (
    "threshold-plus-one",
    "multipart-middle",
    "final-part-after-long-prefix",
)
BOUND_FIELDS = (
    "version",
    "purpose",
    "owner_account",
    "object_id",
    "generation",
    "chunk_index",
    "chunk_count",
    "domain",
)
COMMON_MUTANTS = (
    "custom-toy-primitive",
    "32-bit-key",
    "padded-low-entropy-key",
    "shared-cross-domain-key",
    "algorithm-kdf-downgrade",
)
ARGON_MUTANTS = (
    "weak-argon-salt",
    "weak-argon-memory",
    "weak-argon-iterations",
    "weak-argon-parallelism",
)
BOUNDARY_MUTANTS = (
    "plaintext",
    "public-encoding",
    "unauthenticated-encryption",
)
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def expected_paths() -> list[dict[str, Any]]:
    paths: list[dict[str, Any]] = []
    for constructor, threshold in CONSTRUCTORS:
        for case, offset, part in CASES:
            if case in ("multipart-first", "multipart-middle", "multipart-final"):
                byte_count = 2 * threshold + 1
            elif case == "final-part-after-long-prefix":
                byte_count = 7 * threshold + 1
            else:
                byte_count = threshold + offset
            prefix_bytes = 0 if part is None else part * threshold
            paths.append({
                "pathId": f"{IMPLEMENTATION_ID}:{CELL}:{constructor}:{threshold}:{case}",
                "implementationId": IMPLEMENTATION_ID,
                "cell": CELL,
                "constructor": constructor,
                "threshold": threshold,
                "case": case,
                "bytes": byte_count,
                "prefixBytes": prefix_bytes,
                "part": part,
            })
    return paths


def required_mutants() -> tuple[str, ...]:
    mutants = [f"{domain}:{kind}" for domain in DOMAINS for kind in COMMON_MUTANTS]
    mutants.extend(
        f"{domain}:{kind}" for domain in PASSWORD_DOMAINS for kind in ARGON_MUTANTS
    )
    for constructor, threshold in CONSTRUCTORS:
        for case in QUALIFYING_BOUNDARY_CASES:
            for kind in BOUNDARY_MUTANTS:
                mutants.append(f"carrier:{constructor}:{threshold}:{case}:{kind}")
            mutants.append(f"carrier:{constructor}:{threshold}:{case}:direct-route")
    return tuple(mutants)


REQUIRED_MUTANTS = required_mutants()


@dataclass
class Refusal(RuntimeError):
    domain: str
    category: str
    defect: str
    detail: str
    cell: str = "none"
    constructor: str = "none"
    threshold: str = "none"
    part: str = "none"
    route: str = "none"
    case: str = "none"
    byte_count: str = "none"
    object_id: str = "none"
    operation: str = "none"
    primitive: str = "none"
    effective_key_bits: str = "none"
    kdf: str = "none"

    def __post_init__(self) -> None:
        RuntimeError.__init__(self, self.detail)


def refuse(
    domain: str,
    category: str,
    defect: str,
    detail: str,
    *,
    cell: str = "none",
    constructor: str = "none",
    threshold: int | str = "none",
    part: int | str | None = "none",
    route: str = "none",
    case: str = "none",
    byte_count: int | str = "none",
    object_id: str = "none",
    operation: str = "none",
    primitive: str = "none",
    effective_key_bits: int | str = "none",
    kdf: str = "none",
) -> None:
    raise Refusal(
        domain, category, defect, detail, cell, constructor, str(threshold),
        "none" if part is None else str(part), route, case, str(byte_count),
        object_id, operation, primitive, str(effective_key_bits), kdf,
    )


def require(condition: bool, *args: Any, **kwargs: Any) -> None:
    if not condition:
        refuse(*args, **kwargs)


def exact_keys(value: Any, keys: set[str], domain: str, defect: str) -> dict[str, Any]:
    require(isinstance(value, dict), domain, "contract", defect, "must be object")
    require(set(value) == keys, domain, "contract", defect, f"fields={sorted(keys)}")
    return value


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        refuse("contract", "contract", "json", str(error))


def validate_primitive(domain: str, proof: dict[str, Any]) -> str:
    primitive = exact_keys(
        proof["primitive"],
        {"algorithm", "library", "version", "maintained", "custom", "downgradeAllowed"},
        domain,
        "primitive-shape",
    )
    expected = (
        ("Ed25519", "ed25519-dalek", "2.2.0")
        if domain == "recovery"
        else ("XChaCha20-Poly1305-IETF", "chacha20poly1305", "0.10.1")
    )
    require(primitive["custom"] is False, domain, "primitive", "custom-toy-primitive", "custom=true")
    require(primitive["algorithm"] == expected[0], domain, "primitive", "algorithm", f"expected={expected[0]}")
    require(primitive["library"] == expected[1], domain, "primitive", "library", f"expected={expected[1]}")
    require(primitive["version"] == expected[2], domain, "primitive", "library-version", f"expected={expected[2]}")
    require(primitive["maintained"] is True, domain, "primitive", "maintained-library", "maintained=false")
    require(primitive["downgradeAllowed"] is False, domain, "downgrade", "algorithm-kdf-downgrade", "downgrade=true")
    return primitive["algorithm"]


def validate_key(domain: str, proof: dict[str, Any]) -> str:
    key = exact_keys(
        proof["key"],
        {"bits", "effectiveBits", "provenance", "keyId", "fresh", "lowEntropy", "deterministicallyPadded", "sharedAcrossDomains", "separation"},
        domain,
        "key-shape",
    )
    require(key["bits"] == 256, domain, "strength", "key-bits", f"bits={key['bits']}")
    expected_effective_bits = 128 if domain == "recovery" else 256
    require(key["effectiveBits"] == expected_effective_bits, domain, "strength", "effective-key-bits", f"effectiveBits={key['effectiveBits']}")
    require(key["provenance"] == "CSPRNG:OsRng", domain, "strength", "key-provenance", str(key["provenance"]))
    require(key["fresh"] is True, domain, "strength", "fresh-key", "fresh=false")
    require(key["lowEntropy"] is False, domain, "strength", "low-entropy-key", "lowEntropy=true")
    require(key["deterministicallyPadded"] is False, domain, "strength", "padded-low-entropy-key", "padded=true")
    require(key["sharedAcrossDomains"] is False, domain, "separation", "shared-cross-domain-key", "shared=true")
    require(key["separation"] == f"OSL/6140/{domain}/v1", domain, "separation", "domain-separation", str(key["separation"]))
    require(isinstance(key["keyId"], str) and key["keyId"], domain, "strength", "key-id", "missing")
    return key["keyId"]


def validate_argon(domain: str, proof: dict[str, Any]) -> None:
    argon = proof["argon2id"]
    if domain not in PASSWORD_DOMAINS:
        require(argon is None, domain, "kdf", "unexpected-argon2id", "must be null")
        return
    argon = exact_keys(
        argon,
        {"algorithm", "saltBits", "memoryKiB", "iterations", "parallelism", "freshSalt", "outputBits"},
        domain,
        "argon2id-shape",
    )
    require(argon["algorithm"] == "Argon2id", domain, "kdf", "argon2id-algorithm", str(argon["algorithm"]))
    require(argon["saltBits"] >= 128, domain, "kdf", "weak-argon-salt", f"saltBits={argon['saltBits']}")
    require(argon["memoryKiB"] >= 65_536, domain, "kdf", "weak-argon-memory", f"memoryKiB={argon['memoryKiB']}")
    require(argon["iterations"] >= 3, domain, "kdf", "weak-argon-iterations", f"iterations={argon['iterations']}")
    require(argon["parallelism"] >= 1, domain, "kdf", "weak-argon-parallelism", f"parallelism={argon['parallelism']}")
    require(argon["freshSalt"] is True, domain, "kdf", "argon-fresh-salt", "freshSalt=false")
    require(argon["outputBits"] == 256, domain, "strength", "argon-output-bits", str(argon["outputBits"]))


def validate_domain(proof: Any, expected_domain: str) -> str:
    proof = exact_keys(
        proof,
        {"domain", "objectId", "operation", "primitive", "key", "argon2id", "nonce", "boundFields", "oracle", "discarded"},
        expected_domain,
        "domain-proof-shape",
    )
    require(proof["domain"] == expected_domain, expected_domain, "inventory", "domain-name", str(proof["domain"]))
    expected_operation = {
        "recovery": "verify",
        "enclave-export": "open",
        "personal-export": "open",
        "backup": "restore",
        "carrier": "send",
    }[expected_domain]
    require(proof["objectId"] == f"task6140-{expected_domain}-object", expected_domain, "inventory", "object-id", str(proof["objectId"]))
    require(proof["operation"] == expected_operation, expected_domain, "oracle", "operation", str(proof["operation"]))
    try:
        validate_primitive(expected_domain, proof)
        key_id = validate_key(expected_domain, proof)
        validate_argon(expected_domain, proof)
    except Refusal as error:
        error.object_id = str(proof.get("objectId", "none"))
        error.operation = str(proof.get("operation", "none"))
        primitive = proof.get("primitive", {})
        key = proof.get("key", {})
        argon = proof.get("argon2id")
        error.primitive = str(primitive.get("algorithm", "none")) if isinstance(primitive, dict) else "none"
        error.effective_key_bits = str(key.get("effectiveBits", "none")) if isinstance(key, dict) else "none"
        error.kdf = str(argon.get("algorithm", "none")) if isinstance(argon, dict) else "none"
        raise
    if expected_domain == "recovery":
        require(proof["nonce"] is None, expected_domain, "nonce", "unexpected-nonce", "must be null")
    else:
        nonce = exact_keys(proof["nonce"], {"bits", "source", "fresh", "nonceId"}, expected_domain, "nonce-shape")
        require(nonce["bits"] == 192, expected_domain, "nonce", "nonce-bits", f"bits={nonce['bits']}")
        require(nonce["source"] == "CSPRNG:OsRng", expected_domain, "nonce", "nonce-source", str(nonce["source"]))
        require(nonce["fresh"] is True, expected_domain, "nonce", "nonce-unique", "fresh=false")
        require(isinstance(nonce["nonceId"], str) and nonce["nonceId"], expected_domain, "nonce", "nonce-id", "missing")
    require(proof["boundFields"] == list(BOUND_FIELDS), expected_domain, "aad", "complete-associated-data", "bound field inventory differs")
    oracle = exact_keys(
        proof["oracle"],
        {"legitimateOpens", "noSecretOpens", "wrongKeyOpens", "tamperOpens", "downgradeOpens", "crossDomainOpens", "exactBytes"},
        expected_domain,
        "oracle-shape",
    )
    require(oracle["legitimateOpens"] == 1 and oracle["exactBytes"] is True, expected_domain, "oracle", "legitimate-open", str(oracle))
    for field in ("noSecretOpens", "wrongKeyOpens", "tamperOpens", "downgradeOpens", "crossDomainOpens"):
        require(oracle[field] == 0, expected_domain, "oracle", field, f"opens={oracle[field]}")
    require(proof["discarded"] == {"candidate": True, "key": True, "account": True}, expected_domain, "discard", "domain-discard", str(proof["discarded"]))
    return key_id


def path_context(path: dict[str, Any]) -> dict[str, Any]:
    return {
        "cell": str(path.get("cell", "none")),
        "constructor": str(path.get("constructor", "none")),
        "threshold": path.get("threshold", "none"),
        "part": path.get("part"),
        "case": str(path.get("case", "none")),
        "byte_count": path.get("bytes", "none"),
        "object_id": str(path.get("pathId", "none")),
        "operation": "send",
    }


def validate_carrier(contract: dict[str, Any]) -> None:
    expected_matrix = [{
        "implementationId": IMPLEMENTATION_ID,
        "cell": CELL,
        "support": "Supported",
        "constructors": [
            {"name": constructor, "threshold": threshold}
            for constructor, threshold in CONSTRUCTORS
        ],
    }]
    require(contract["carrierMatrix"] == expected_matrix, "carrier", "inventory", "carrier-matrix", "5019/6101 matrix differs", cell=CELL)
    expected = expected_paths()
    paths = contract["carrierPaths"]
    require(isinstance(paths, list), "carrier", "inventory", "path-inventory", "missing", cell=CELL)
    by_id = {item.get("pathId"): item for item in paths if isinstance(item, dict)}
    require(len(by_id) == len(paths), "carrier", "inventory", "path-duplicate", "duplicate/unnamed", cell=CELL)
    for wanted in expected:
        path = by_id.get(wanted["pathId"])
        context = path_context(wanted)
        require(path is not None, "carrier", "starvation", f"path:{wanted['pathId']}", "path absent", **context)
        path = exact_keys(path, set(wanted) | {"encrypted", "authenticated", "beforeRenderCommit", "capturePlaintext", "exactArrivals", "exactBytes"}, "carrier", "path-shape")
        for field, value in wanted.items():
            require(path[field] == value, "carrier", "inventory", f"path-{field}", f"expected={value} actual={path[field]}", **context)
        if path["encrypted"] is not True:
            refuse("carrier", "primitive", "boundary-plaintext-or-public", "encrypted=false", **context)
        if path["authenticated"] is not True:
            refuse("carrier", "authentication", "boundary-unauthenticated", "authenticated=false", **context)
        require(path["beforeRenderCommit"] is True, "carrier", "authentication", "before-render-commit", "false", **context)
        require(path["capturePlaintext"] == 0, "carrier", "capture", "plaintext-capture", str(path["capturePlaintext"]), **context)
        require(path["exactArrivals"] == 1 and path["exactBytes"] is True, "carrier", "delivery", "exactly-once-bytes", str(path), **context)
    require(set(by_id) == {item["pathId"] for item in expected}, "carrier", "inventory", "unexpected-path", "extra path", cell=CELL)

    observations = contract["torObservations"]
    require(isinstance(observations, list), "carrier", "route", "tor-inventory", "missing", cell=CELL)
    observed = {item.get("pathId"): item for item in observations if isinstance(item, dict)}
    require(len(observed) == len(observations), "carrier", "route", "tor-duplicate", "duplicate/unnamed", cell=CELL)
    for wanted in expected:
        context = path_context(wanted)
        context["operation"] = "provider-route"
        item = observed.get(wanted["pathId"])
        require(item is not None, "carrier", "starvation", f"tor:{wanted['pathId']}", "Tor observation absent", route="absent", **context)
        item = exact_keys(item, {"pathId", "requestId", "observed", "route", "directEgressBytes"}, "carrier", "tor-shape")
        require(item["observed"] is True, "carrier", "route", "tor-observation", "observed=false", route=str(item.get("route")), **context)
        require(item["route"] == "bundled-tor" and item["directEgressBytes"] == 0, "carrier", "route", "direct-route", f"direct={item['directEgressBytes']}", route=str(item["route"]), **context)
        require(isinstance(item["requestId"], str) and item["requestId"], "carrier", "route", "request-id", "missing", route=str(item["route"]), **context)
    require(set(observed) == {item["pathId"] for item in expected}, "carrier", "route", "unexpected-tor", "extra observation", cell=CELL)


def validate(contract: Any) -> dict[str, int]:
    contract = exact_keys(
        contract,
        {"schemaVersion", "frozenInventorySha256", "domainInventory", "domainProofs", "carrierMatrix", "carrierPaths", "torObservations", "requiredMutants", "discarded"},
        "contract",
        "contract-shape",
    )
    require(contract["schemaVersion"] == 1, "contract", "contract", "schema-version", str(contract["schemaVersion"]))
    require(bool(HEX64.fullmatch(str(contract["frozenInventorySha256"]))), "contract", "inventory", "frozen-inventory-digest", "invalid")
    require(contract["domainInventory"] == list(DOMAINS), "contract", "inventory", "domain-inventory", "must be exact")
    proofs = contract["domainProofs"]
    require(isinstance(proofs, list), "contract", "inventory", "domain-proofs", "missing")
    proof_by_domain = {proof.get("domain"): proof for proof in proofs if isinstance(proof, dict)}
    for domain in DOMAINS:
        require(domain in proof_by_domain, domain, "starvation", f"domain:{domain}", "domain proof absent")
    require(len(proofs) == len(DOMAINS) and set(proof_by_domain) == set(DOMAINS), "contract", "inventory", "domain-proof-inventory", "extra/duplicate")
    key_ids = [validate_domain(proof_by_domain[domain], domain) for domain in DOMAINS]
    require(len(set(key_ids)) == len(key_ids), "contract", "separation", "shared-cross-domain-key", "duplicate keyId")
    validate_carrier(contract)
    required = contract["requiredMutants"]
    require(isinstance(required, list), "contract", "starvation", "mutant-inventory", "missing")
    missing = [mutant for mutant in REQUIRED_MUTANTS if mutant not in required]
    if missing:
        refuse("contract", "starvation", f"mutant:{missing[0]}", "required mutant absent")
    require(required == list(REQUIRED_MUTANTS), "contract", "inventory", "mutant-inventory", "changed/duplicate/extra")
    require(contract["discarded"] == {"candidates": True, "keys": True, "accounts": True}, "contract", "discard", "global-discard", str(contract["discarded"]))
    return {
        "domains": len(DOMAINS),
        "password_domains": len(PASSWORD_DOMAINS),
        "carrier_cells": 1,
        "constructors": len(CONSTRUCTORS),
        "paths": len(expected_paths()),
        "tor_observations": len(expected_paths()),
        "mutants": len(REQUIRED_MUTANTS),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--contract", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        result = validate(read_json(args.contract))
    except Refusal as error:
        print(
            "TASK6140B_FAIL "
            f"domain={error.domain} category={error.category} defect={error.defect} "
            f"cell={error.cell} constructor={error.constructor} threshold={error.threshold} "
            f"case={error.case} byte_count={error.byte_count} part={error.part} route={error.route} "
            f"object={error.object_id} operation={error.operation} primitive={error.primitive} "
            f"effective_key_bits={error.effective_key_bits} kdf={error.kdf} detail={error.detail}",
            file=sys.stderr,
        )
        return 1
    print("TASK6140B_OK " + " ".join(f"{key}={value}" for key, value in result.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
