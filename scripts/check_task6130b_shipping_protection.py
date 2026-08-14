#!/usr/bin/env python3
"""Fail-closed contract checker for TASK 6130b shipping-cell protection proofs."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


IMPLEMENTATION_ID = "native.discord.text.v1"
CONTENT_TYPE = "text"
CONSTRUCTORS = (
    "prepare_peer_prose_text_inner",
    "split_native_overlay_text",
)
AAD_FIELDS = (
    "protocol",
    "version",
    "sender_account",
    "sender_device",
    "recipient_account",
    "recipient_device",
    "conversation_channel",
    "adapter_implementation_id",
    "send_constructor",
    "content_type",
    "message_object_id",
    "delivery_sequence",
)
ATTACKS = ("wrong_key", "bit_flip", "truncation", "reorder", "replay")
ALGORITHMS = {
    "AES-256-GCM": ("aes-gcm", "0.10.3", 96),
    "XChaCha20-Poly1305-IETF": ("chacha20poly1305", "0.10.1", 192),
}
BASE_MUTANTS = (
    "public_reversible_transform",
    "raw_plaintext_cell",
    "custom_toy_authenticated_cipher",
    "low_entropy_32_bit_root",
    "weak_argon2id_salt",
    "weak_argon2id_memory",
    "weak_argon2id_iterations",
    "weak_argon2id_parallelism",
    "shared_global_content_key",
    "missing_hkdf_domain",
    "bypassed_non_text_cell",
    "reused_nonce_material",
    "missing_authentication",
    "algorithm_downgrade",
    "unseparated_content_key",
    "accepted_wrong_key",
    "accepted_bit_flip",
    "accepted_truncation",
    "accepted_reorder",
    "accepted_replay",
)
MUTANT_KINDS = BASE_MUTANTS + tuple(f"omit_aad:{field}" for field in AAD_FIELDS)
REQUIRED_MUTANTS = tuple(
    f"{constructor}:{mutant}"
    for constructor in CONSTRUCTORS
    for mutant in MUTANT_KINDS
)
HEX64 = re.compile(r"^[0-9a-f]{64}$")
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+$")


ACTIVE_CONSTRUCTOR = "matrix"


class Refusal(RuntimeError):
    def __init__(
        self,
        category: str,
        field: str,
        detail: str,
        *,
        cell: str = CONTENT_TYPE,
        constructor: str | None = None,
    ):
        self.category = category
        self.field = field
        self.detail = detail
        self.cell = cell
        self.constructor = constructor or ACTIVE_CONSTRUCTOR
        super().__init__(detail)


def refuse(
    category: str,
    field: str,
    detail: str,
    *,
    cell: str = CONTENT_TYPE,
    constructor: str | None = None,
) -> None:
    raise Refusal(category, field, detail, cell=cell, constructor=constructor)


def require(condition: bool, category: str, field: str, detail: str, *, cell: str = CONTENT_TYPE) -> None:
    if not condition:
        refuse(category, field, detail, cell=cell)


def exact_keys(value: Any, keys: set[str], category: str, field: str) -> dict[str, Any]:
    require(isinstance(value, dict), category, field, "must be an object")
    require(set(value) == keys, category, field, f"fields must be exactly {sorted(keys)}")
    return value


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        refuse("contract", "json", str(error))


def validate_frozen_matrix(contract: dict[str, Any]) -> None:
    expected = [{
        "implementationId": IMPLEMENTATION_ID,
        "contentType": CONTENT_TYPE,
        "constructors": list(CONSTRUCTORS),
    }]
    cells = contract.get("supportedCells")
    if not isinstance(cells, list) or not cells:
        refuse("starvation", "supported_cell", "6101 frozen supported cell is absent")
    require(cells == expected, "matrix", "supported_cells", "6101 supported matrix changed")

    sends = contract.get("productionSends")
    require(isinstance(sends, list), "matrix", "production_sends", "production send inventory missing")
    seen: list[str] = []
    for send in sends:
        require(isinstance(send, dict), "matrix", "production_sends", "production send row is not an object")
        implementation = send.get("implementationId")
        content = send.get("contentType")
        constructor = send.get("constructor")
        if implementation != IMPLEMENTATION_ID or content != CONTENT_TYPE:
            refuse(
                "matrix",
                "unsupported_production_send",
                f"6101 marks production send unsupported constructor={constructor}",
                cell=str(content or "unknown"),
                constructor=str(constructor or "unknown"),
            )
        require(isinstance(constructor, str), "matrix", "constructor", "constructor missing")
        seen.append(constructor)
    missing = [item for item in CONSTRUCTORS if item not in seen]
    if missing:
        refuse("starvation", "constructor", f"frozen constructor absent={missing[0]}")
    require(seen == list(CONSTRUCTORS), "matrix", "constructors", "production constructors changed or duplicated")


def validate_argon(root: dict[str, Any], constructor: str) -> None:
    person_entered = root.get("personEntered")
    require(isinstance(person_entered, bool), "key_strength", "root.person_entered", constructor)
    params = root.get("argon2id")
    if not person_entered:
        require(params is None, "kdf", "argon2id.unexpected", constructor)
        return
    exact_keys(params, {"algorithm", "saltBits", "memoryKiB", "iterations", "parallelism"}, "kdf", "argon2id")
    require(params["algorithm"] == "Argon2id", "kdf", "argon2id.algorithm", constructor)
    require(isinstance(params["saltBits"], int) and params["saltBits"] >= 128, "kdf", "argon2id.salt_bits", constructor)
    require(isinstance(params["memoryKiB"], int) and params["memoryKiB"] >= 65_536, "kdf", "argon2id.memory_kib", constructor)
    require(isinstance(params["iterations"], int) and params["iterations"] >= 3, "kdf", "argon2id.iterations", constructor)
    require(isinstance(params["parallelism"], int) and params["parallelism"] >= 1, "kdf", "argon2id.parallelism", constructor)


def validate_proof(proof: Any, expected_constructor: str) -> tuple[str, str, str]:
    global ACTIVE_CONSTRUCTOR
    ACTIVE_CONSTRUCTOR = expected_constructor
    proof = exact_keys(
        proof,
        {
            "implementationId", "contentType", "constructor", "postBuild",
            "primitive", "root", "kdf", "nonce", "authentication", "aadFields",
            "oracle", "attacks", "transplants", "discarded",
        },
        "proof",
        "shape",
    )
    require(proof["implementationId"] == IMPLEMENTATION_ID, "matrix", "proof.implementation_id", expected_constructor)
    require(proof["contentType"] == CONTENT_TYPE, "matrix", "proof.content_type", expected_constructor)
    require(proof["constructor"] == expected_constructor, "matrix", "proof.constructor", expected_constructor)

    primitive = exact_keys(
        proof["primitive"],
        {"algorithm", "library", "version", "maintained", "custom", "downgradeAllowed"},
        "primitive",
        "shape",
    )
    if primitive["custom"] is True:
        refuse("primitive", "custom_primitive", expected_constructor)
    require(primitive["custom"] is False, "primitive", "custom_primitive", expected_constructor)
    algorithm = primitive["algorithm"]
    require(algorithm in ALGORITHMS, "primitive", "algorithm", str(algorithm))
    library, minimum_version, nonce_bits = ALGORITHMS[algorithm]
    require(primitive["library"] == library, "primitive", "library", expected_constructor)
    require(primitive["version"] == minimum_version and bool(VERSION.fullmatch(str(primitive["version"]))), "primitive", "library_version", expected_constructor)
    require(primitive["maintained"] is True, "primitive", "maintained_library", expected_constructor)
    require(primitive["downgradeAllowed"] is False, "primitive", "downgrade", expected_constructor)

    root = exact_keys(
        proof["root"],
        {"bits", "provenance", "rootId", "lowEntropy", "deterministicallyPadded", "sharedGlobal", "personEntered", "argon2id"},
        "key_strength",
        "root.shape",
    )
    require(root["bits"] == 256, "key_strength", "root.bits", expected_constructor)
    require(root["provenance"] == "CSPRNG:OsRng", "key_strength", "root.provenance", expected_constructor)
    require(root["lowEntropy"] is False and root["deterministicallyPadded"] is False, "key_strength", "root.entropy", expected_constructor)
    require(root["sharedGlobal"] is False, "key_strength", "shared_global_root", expected_constructor)
    require(isinstance(root["rootId"], str) and root["rootId"], "key_strength", "root.id", expected_constructor)
    validate_argon(root, expected_constructor)

    kdf = exact_keys(proof["kdf"], {"name", "domain", "outputBits", "contentKeyId", "separated"}, "kdf", "shape")
    require(kdf["name"] == "HKDF-SHA-256", "kdf", "name", expected_constructor)
    expected_domain = f"OSL/6130/content-key/v1/{expected_constructor}"
    require(kdf["domain"] == expected_domain, "kdf", "hkdf.domain", expected_constructor)
    require(kdf["outputBits"] == 256, "key_strength", "content_key.bits", expected_constructor)
    require(kdf["separated"] is True, "kdf", "content_key.separated", expected_constructor)
    require(isinstance(kdf["contentKeyId"], str) and kdf["contentKeyId"], "kdf", "content_key.id", expected_constructor)

    nonce = exact_keys(proof["nonce"], {"bits", "source", "nonceId", "fresh"}, "nonce", "shape")
    require(nonce["bits"] == nonce_bits, "nonce", "bits", expected_constructor)
    require(nonce["source"] == "CSPRNG:OsRng", "nonce", "source", expected_constructor)
    require(nonce["fresh"] is True, "nonce", "fresh", expected_constructor)
    require(isinstance(nonce["nonceId"], str) and nonce["nonceId"], "nonce", "id", expected_constructor)

    authentication = exact_keys(proof["authentication"], {"required", "beforeRendering"}, "authentication", "shape")
    require(authentication["required"] is True, "authentication", "required", expected_constructor)
    require(authentication["beforeRendering"] is True, "authentication", "before_rendering", expected_constructor)

    aad = proof["aadFields"]
    require(isinstance(aad, list), "aad", "fields", expected_constructor)
    missing_aad = [field for field in AAD_FIELDS if field not in aad]
    if missing_aad:
        omitted = missing_aad[0]
        observed = proof.get("transplants", {}).get(omitted, {})
        original = observed.get("originalContext", "absent")
        target = observed.get("targetContext", "absent")
        unchanged = observed.get("unchangedObject", False)
        released = observed.get("plaintextReleased", 0)
        require(
            unchanged is True and released == 1,
            "aad",
            omitted,
            f"omitted field did not demonstrate unchanged-object acceptance original={original} target={target}",
        )
        refuse(
            "aad",
            omitted,
            f"observed=cross_context_open unchanged_object=true plaintext_released={released} "
            f"original={original} target={target}",
        )
    require(aad == list(AAD_FIELDS), "aad", "fields", "AAD fields changed, duplicated, or reordered")

    post_build = exact_keys(proof["postBuild"], {"uniquePayload", "realDisposableAccount", "independentRecipient"}, "proof", "post_build")
    for field, value in post_build.items():
        require(value is True, "proof", f"post_build.{field}", expected_constructor)
    oracle = exact_keys(proof["oracle"], {"exactOpens", "capturePlaintext", "publicReaderOpens"}, "proof", "oracle")
    require(oracle["exactOpens"] == 1, "proof", "oracle.exact_opens", expected_constructor)
    require(oracle["capturePlaintext"] == 0, "proof", "oracle.capture_plaintext", expected_constructor)
    require(oracle["publicReaderOpens"] == 0, "proof", "oracle.public_reader_opens", expected_constructor)

    attacks = proof["attacks"]
    require(isinstance(attacks, dict), "attack", "inventory", expected_constructor)
    missing_attacks = [attack for attack in ATTACKS if attack not in attacks]
    if missing_attacks:
        refuse(
            "starvation",
            f"attack:{missing_attacks[0]}",
            f"absent attack constructor={expected_constructor}",
        )
    require(set(attacks) == set(ATTACKS), "attack", "inventory", expected_constructor)
    for attack in ATTACKS:
        outcome = exact_keys(attacks[attack], {"plaintextReleased", "deliveries", "duplicateDeliveries"}, "attack", attack)
        require(
            outcome == {"plaintextReleased": 0, "deliveries": 0, "duplicateDeliveries": 0},
            "attack",
            attack,
            f"accepted invalid content constructor={expected_constructor}",
        )

    transplants = proof["transplants"]
    require(isinstance(transplants, dict), "transplant", "inventory", expected_constructor)
    missing_transplant = [field for field in AAD_FIELDS if field not in transplants]
    if missing_transplant:
        refuse("transplant", missing_transplant[0], f"context transplant omitted constructor={expected_constructor}")
    require(set(transplants) == set(AAD_FIELDS), "transplant", "inventory", expected_constructor)
    for field in AAD_FIELDS:
        outcome = exact_keys(
            transplants[field],
            {
                "plaintextReleased", "deliveries", "originalOpens",
                "unchangedObject", "originalContext", "targetContext",
            },
            "transplant",
            field,
        )
        require(
            outcome["plaintextReleased"] == 0
            and outcome["deliveries"] == 0
            and outcome["originalOpens"] == 1
            and outcome["unchangedObject"] is True
            and isinstance(outcome["originalContext"], str)
            and isinstance(outcome["targetContext"], str)
            and outcome["originalContext"]
            and outcome["targetContext"],
            "transplant",
            field,
            f"cross-context object opened constructor={expected_constructor}",
        )
    discarded = exact_keys(proof["discarded"], {"candidate", "account"}, "proof", "discarded")
    require(discarded == {"candidate": True, "account": True}, "proof", "discarded", expected_constructor)
    return root["rootId"], kdf["contentKeyId"], nonce["nonceId"]


def validate(contract: Any) -> dict[str, int]:
    global ACTIVE_CONSTRUCTOR
    ACTIVE_CONSTRUCTOR = "matrix"
    contract = exact_keys(
        contract,
        {"schemaVersion", "frozenAuthoritySha256", "supportedCells", "productionSends", "contextAxes", "proofs", "requiredMutants"},
        "contract",
        "shape",
    )
    require(contract["schemaVersion"] == 1, "contract", "schema_version", "must be 1")
    require(bool(HEX64.fullmatch(str(contract["frozenAuthoritySha256"]))), "contract", "frozen_authority_sha256", "invalid digest")
    validate_frozen_matrix(contract)

    axes = contract["contextAxes"]
    require(isinstance(axes, list), "starvation", "context_axis", "axis inventory missing")
    missing_axes = [field for field in AAD_FIELDS if field not in axes]
    if missing_axes:
        refuse("starvation", f"context_axis:{missing_axes[0]}", "context axis absent")
    require(axes == list(AAD_FIELDS), "aad", "context_axes", "context axes changed, duplicated, or reordered")

    required = contract["requiredMutants"]
    require(isinstance(required, list), "starvation", "mutant_inventory", "mutant inventory missing")
    absent = [mutant for mutant in REQUIRED_MUTANTS if mutant not in required]
    if absent:
        refuse("starvation", f"mutant:{absent[0]}", "required mutant absent")
    require(required == list(REQUIRED_MUTANTS), "starvation", "mutant_inventory", "mutant inventory changed, duplicated, or reordered")

    proofs = contract["proofs"]
    require(isinstance(proofs, list), "proof", "inventory", "proof inventory missing")
    present = [proof.get("constructor") for proof in proofs if isinstance(proof, dict)]
    missing_proofs = [constructor for constructor in CONSTRUCTORS if constructor not in present]
    if missing_proofs:
        refuse(
            "starvation",
            f"proof:{missing_proofs[0]}",
            "constructor proof absent",
            constructor=missing_proofs[0],
        )
    require(len(proofs) == len(CONSTRUCTORS), "proof", "inventory", "unexpected proof rows")
    identities = [validate_proof(proof, constructor) for proof, constructor in zip(proofs, CONSTRUCTORS)]
    roots = [item[0] for item in identities]
    content_keys = [item[1] for item in identities]
    nonces = [item[2] for item in identities]
    require(len(set(roots)) == len(roots), "key_strength", "shared_global_root", "root reused across constructors")
    require(len(set(content_keys)) == len(content_keys), "kdf", "shared_global_content_key", "content key reused across constructors")
    if len(set(nonces)) != len(nonces):
        duplicate_index = next(
            index for index, nonce in enumerate(nonces) if nonce in nonces[:index]
        )
        refuse(
            "nonce",
            "nonce_id",
            f"nonce material reused observed=accepted_reuse nonce_id={nonces[duplicate_index]}",
            constructor=CONSTRUCTORS[duplicate_index],
        )
    return {
        "supported_cells": 1,
        "constructors": len(CONSTRUCTORS),
        "aad_axes": len(AAD_FIELDS),
        "attacks": len(ATTACKS) * len(CONSTRUCTORS),
        "transplants": len(AAD_FIELDS) * len(CONSTRUCTORS),
        "required_mutants": len(REQUIRED_MUTANTS),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--contract", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        result = validate(read_json(args.contract))
    except Refusal as error:
        print(
            f"TASK6130B_FAIL adapter={IMPLEMENTATION_ID} constructor={error.constructor} cell={error.cell} "
            f"category={error.category} field={error.field} detail={error.detail}",
            file=sys.stderr,
        )
        return 1
    print("TASK6130B_OK " + " ".join(f"{key}={value}" for key, value in result.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
