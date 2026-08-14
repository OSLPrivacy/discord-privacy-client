#!/usr/bin/env python3
"""Fail closed receipt authority for TASK 6490.

This is deliberately a verifier, not a receipt generator.  A campaign may be
complete only when a detached signature binds the frozen candidate, every
current gated-task digest, the complete campaign inventory, and each result.
It also keeps authority validation separate from semantic truth validation:
cryptographically authentic receipts are still refused when their observations
make incompatible claims about the same frozen candidate.
"""

from __future__ import annotations

import argparse
import base64
import copy
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


GATED_TASKS = (
    "5008 5009 5018 5019 5021 5022 5024 5024b 5032 5036 5192 5192b "
    "5196b 5200 5202 5202b 5205 5205b 5212 5212b 5221 5221b 6096 6096b "
    "6100 6100b 6101 6101b 6134 6134b 6135 6135b 6136 6136b 6137 6137b "
    "6140 6140b 6144 6144b 6592 6593"
).split()

# The Tor entries are generated before execution so that a late direct route
# cannot disappear behind an aggregate "Tor tested" heading.
TOR_CELLS = ("direct-message", "attachment", "enclave-attachment")
TOR_CONSTRUCTORS = ("single-part", "multipart")
TOR_THRESHOLDS = ("below", "at", "above")
TOR_MUTANTS = tuple(
    f"tor-direct-{cell}-{constructor}-{threshold}"
    for cell in TOR_CELLS
    for constructor in TOR_CONSTRUCTORS
    for threshold in TOR_THRESHOLDS
)

REQUIRED_MUTANTS = (
    "5018-reached-gap-row",
    "5018-voice-leak-live-control",
    "5018-voice-leak-grey-control",
    "5018-voice-leak-registered-control",
    "same-candidate-false-receipt",
    *TOR_MUTANTS,
    "tor-direct-late-path",
    "backup-missing-executor",
    "backup-invalid-tombstone",
    "backup-wrong-object",
    "backup-broad-erasure",
    "offline-friend",
    "accessibility",
    "retention",
    "paging",
    "exact-disclosure-reachability",
    "voice-absent",
    "voice-hidden-under-tor",
    "voice-inert-leave",
)

REQUIRED_ATTACKS = (
    "stale-receipt",
    "omitted-gated-task",
    "omitted-required-mutant",
    "post-freeze-task-edit",
    "mixed-candidate",
    "behaviorally-false-voice-receipt",
    "predecessor-heading-only",
    "omit-5018",
    "omit-voice-mutant",
    "omit-tor-mutant",
    "omit-item-erasure-mutant",
)

VOICE_GAP_KEYS = ("live", "grey", "registered", "unregistered", "dynamic")
VOICE_MUTANT_OBSERVATIONS = {
    "voice-absent": "absent",
    "voice-hidden-under-tor": "hidden-under-tor",
    "voice-inert-leave": "inert-leave",
}
MUTANT_OBSERVATIONS = {
    "5018-voice-leak-live-control": {"voice_control": "live"},
    "5018-voice-leak-grey-control": {"voice_control": "grey"},
    "5018-voice-leak-registered-control": {"voice_control": "registered"},
    "same-candidate-false-receipt": {"candidate_identity": "same", "voice_control": "present"},
    "backup-missing-executor": {"failure": "missing-executor"},
    "backup-invalid-tombstone": {"failure": "invalid-tombstone"},
    "backup-wrong-object": {"failure": "wrong-object"},
    "backup-broad-erasure": {"failure": "broad-erasure"},
    "offline-friend": {"failure": "offline-friend"},
    "accessibility": {"failure": "accessibility"},
    "retention": {"failure": "retention"},
    "paging": {"failure": "paging"},
    "exact-disclosure-reachability": {"failure": "exact-disclosure-reachability"},
}


class GateError(RuntimeError):
    """A refusal whose text is intended to be retained as a receipt."""


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value: Any) -> str:
    return hashlib.sha256(canonical(value)).hexdigest()


def parse_time(value: str) -> dt.datetime:
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise GateError(f"invalid receipt time={value!r}") from error
    if parsed.tzinfo is None:
        raise GateError(f"receipt time lacks timezone={value!r}")
    return parsed.astimezone(dt.timezone.utc)


def task_blocks(source: Path) -> dict[str, str]:
    """Read current task text from either its canonical file or todo directory."""
    sources = sorted(source.glob("*.txt")) if source.is_dir() else [source]
    blocks: dict[str, str] = {}
    for task_source in sources:
        text = task_source.read_text(encoding="utf8")
        for match in re.finditer(r"(?m)^TASK (\d+[a-z]?)\b.*?(?=^TASK \d+[a-z]?\b|\Z)", text, re.S):
            task = match.group(1)
            if task in GATED_TASKS:
                if task in blocks:
                    raise GateError(f"ambiguous current task text task={task}")
                blocks[task] = match.group(0)
    missing = sorted(set(GATED_TASKS) - set(blocks))
    if missing:
        raise GateError("missing current task text task=" + ",".join(missing))
    return blocks


def task_digests(source: Path) -> dict[str, str]:
    return {task: hashlib.sha256(body.encode()).hexdigest() for task, body in task_blocks(source).items()}


def unsigned(document: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(document)
    result.pop("signature", None)
    return result


def openssl_signature(payload: bytes, private_key: Path) -> str:
    process = subprocess.run(
        ["openssl", "dgst", "-sha256", "-sign", str(private_key)],
        input=payload,
        capture_output=True,
        check=False,
    )
    if process.returncode:
        raise GateError("could not create detached signature")
    return base64.b64encode(process.stdout).decode("ascii")


def verify_signature(document: dict[str, Any], public_key: Path, label: str) -> None:
    signature = document.get("signature", {})
    if signature.get("algorithm") != "rsa-sha256" or not isinstance(signature.get("value"), str):
        raise GateError(f"{label} has no rsa-sha256 detached signature")
    with tempfile.TemporaryDirectory(prefix="osl-6490-signature-") as temporary:
        root = Path(temporary)
        payload = root / "payload.json"
        signed = root / "signature.bin"
        payload.write_bytes(canonical(unsigned(document)))
        try:
            signed.write_bytes(base64.b64decode(signature["value"], validate=True))
        except ValueError as error:
            raise GateError(f"{label} has malformed detached signature") from error
        process = subprocess.run(
            ["openssl", "dgst", "-sha256", "-verify", str(public_key), "-signature", str(signed), str(payload)],
            capture_output=True,
            text=True,
            check=False,
        )
    if process.returncode:
        raise GateError(f"{label} detached signature invalid")


def sign(document: dict[str, Any], private_key: Path) -> dict[str, Any]:
    result = unsigned(document)
    result["signature"] = {"algorithm": "rsa-sha256", "value": openssl_signature(canonical(result), private_key)}
    return result


def freeze(candidate: str, source: Path, frozen_at: str, campaign_started_at: str) -> dict[str, Any]:
    """Create the signed pre-execution authority.

    The campaign start time is committed here instead of being inferred from
    the final receipt.  That makes a copied pre-freeze receipt mechanically
    distinguishable from a fresh run.
    """
    parse_time(frozen_at)
    if parse_time(campaign_started_at) < parse_time(frozen_at):
        raise GateError("campaign start predates freeze")
    return {
        "schema": "osl.task-6490.freeze.v1",
        "task": "6490",
        "candidate_identity": candidate,
        "frozen_at": frozen_at,
        "campaign_started_at": campaign_started_at,
        "task_digests": task_digests(source),
        "required_positive_ids": [f"task-{task}-positive" for task in GATED_TASKS],
        "required_mutant_ids": list(REQUIRED_MUTANTS),
        "required_attack_ids": list(REQUIRED_ATTACKS),
    }


def _unique(values: list[str], kind: str) -> None:
    duplicates = sorted({item for item in values if values.count(item) > 1})
    if duplicates:
        raise GateError(f"duplicate {kind}=" + ",".join(duplicates))


def verify_manifest(manifest: dict[str, Any], source: Path, public_key: Path) -> None:
    verify_signature(manifest, public_key, "6490 manifest")
    if manifest.get("schema") != "osl.task-6490.freeze.v1" or manifest.get("task") != "6490":
        raise GateError("invalid completion authority manifest schema/task")
    if not isinstance(manifest.get("candidate_identity"), str) or not manifest["candidate_identity"]:
        raise GateError("missing 6096 candidate identity")
    parse_time(manifest.get("frozen_at", ""))
    campaign_started = parse_time(manifest.get("campaign_started_at", ""))
    if campaign_started < parse_time(manifest["frozen_at"]):
        raise GateError("campaign start predates freeze")
    actual = task_digests(source)
    expected = manifest.get("task_digests")
    if expected != actual:
        changed = next((task for task in GATED_TASKS if expected.get(task) != actual.get(task)), "unknown") if isinstance(expected, dict) else "unknown"
        raise GateError(f"digest mismatch task={changed}")
    for field, required in (
        ("required_positive_ids", tuple(f"task-{task}-positive" for task in GATED_TASKS)),
        ("required_mutant_ids", REQUIRED_MUTANTS),
        ("required_attack_ids", REQUIRED_ATTACKS),
    ):
        actual_values = manifest.get(field)
        if not isinstance(actual_values, list):
            raise GateError(f"missing {field}")
        _unique(actual_values, field)
        missing = sorted(set(required) - set(actual_values))
        extra = sorted(set(actual_values) - set(required))
        if missing or extra:
            raise GateError(f"invalid {field} missing={','.join(missing) or '-'} extra={','.join(extra) or '-'}")


def verify_campaign(manifest: dict[str, Any], campaign: dict[str, Any], public_key: Path) -> None:
    verify_signature(campaign, public_key, "6490 final receipt")
    if campaign.get("schema") != "osl.task-6490.campaign.v1":
        raise GateError("invalid final receipt schema")
    if campaign.get("manifest_digest") != digest(manifest):
        raise GateError("invalid completion authority manifest digest")
    if campaign.get("candidate_identity") != manifest["candidate_identity"]:
        raise GateError("candidate mismatch final receipt")
    campaign_started = parse_time(campaign.get("started_at", ""))
    if campaign_started != parse_time(manifest["campaign_started_at"]):
        raise GateError("campaign start mismatch final receipt")
    receipts = campaign.get("receipts")
    if not isinstance(receipts, list):
        raise GateError("missing final receipt inventory")
    ids = []
    run_ids = []
    for receipt in receipts:
        if not isinstance(receipt, dict) or not isinstance(receipt.get("id"), str):
            raise GateError("malformed campaign receipt")
        ids.append(receipt["id"])
        if receipt.get("executed_for_task") != "6490" or receipt.get("reuse_count") != 0:
            raise GateError(f"recycled receipt={receipt['id']} task={receipt.get('task')}")
        if not isinstance(receipt.get("run_id"), str) or not receipt["run_id"]:
            raise GateError(f"missing fresh run id receipt={receipt['id']}")
        run_ids.append(receipt["run_id"])
        if receipt.get("candidate_identity") != manifest["candidate_identity"]:
            raise GateError(f"candidate mismatch receipt={receipt['id']}")
        started = parse_time(receipt.get("started_at", ""))
        if started < parse_time(manifest["frozen_at"]) or started < campaign_started:
            raise GateError(f"stale receipt={receipt['id']}")
        if receipt.get("task_digest") != manifest["task_digests"].get(receipt.get("task")):
            raise GateError(f"digest mismatch receipt={receipt['id']} task={receipt.get('task')}")
    _unique(ids, "receipt id")
    _unique(run_ids, "receipt run id")
    expected = set(manifest["required_positive_ids"]) | set(manifest["required_mutant_ids"])
    missing = sorted(expected - set(ids))
    extra = sorted(set(ids) - expected)
    if missing or extra:
        raise GateError(f"receipt inventory mismatch missing={','.join(missing) or '-'} extra={','.join(extra) or '-'}")
    by_id = {receipt["id"]: receipt for receipt in receipts}
    for task in GATED_TASKS:
        positive = by_id[f"task-{task}-positive"]
        if positive.get("task") != task or positive.get("kind") != "positive" or positive.get("result") != "green" or positive.get("exit_code") != 0:
            raise GateError(f"invalid positive receipt task={task}")
    for mutant in manifest["required_mutant_ids"]:
        receipt = by_id[mutant]
        if receipt.get("kind") != "mutant" or receipt.get("result") != "red" or receipt.get("exit_code") != 1:
            raise GateError(f"invalid mutant receipt mutant={mutant}")
        if not receipt.get("threatened_behavior_observed"):
            raise GateError(f"false receipt mutant={mutant} threatened behavior absent")
    # An identity-matched voice receipt cannot stand in for an observed state.
    voice = by_id.get("voice-absent")
    if voice and voice.get("observation", {}).get("voice_control") != "absent":
        raise GateError("false receipt mutant=voice-absent expected=absent")


def verify_semantic_truth(campaign: dict[str, Any]) -> None:
    """Require observations, rather than accepting a green/red label alone."""
    by_id = {receipt["id"]: receipt for receipt in campaign["receipts"]}
    gap = by_id["5018-reached-gap-row"].get("observation", {})
    voice = by_id["task-6592-positive"].get("observation", {})
    zero_controls = all(gap.get(key) == 0 for key in VOICE_GAP_KEYS)
    working_tor_control = (
        voice.get("join_present") is True
        and voice.get("tor_join_state") == "greyed"
        and voice.get("direct_consent_required") is True
        and voice.get("call_completed") is True
        and voice.get("consent_forgotten") is True
    )
    if gap.get("reached_gap_rows") != 1:
        raise GateError("false receipt task=5018 reached gap row count")
    if not zero_controls:
        raise GateError("false receipt task=5018 controls were not all zero")
    if not working_tor_control:
        raise GateError("false receipt task=6592 missing working Tor-greyed Join")
    # The historical 5018 mutant crawls a deliberately broken package.  Its
    # zero controls and the working 6592 control describe different journeys,
    # so one cannot be used to falsify the other.
    for mutant, expected in VOICE_MUTANT_OBSERVATIONS.items():
        actual = by_id[mutant].get("observation", {}).get("voice_control")
        if actual != expected:
            raise GateError(f"false receipt mutant={mutant} expected={expected}")
    for mutant, expected in MUTANT_OBSERVATIONS.items():
        observed = by_id[mutant].get("observation", {})
        if any(observed.get(key) != value for key, value in expected.items()):
            raise GateError(f"false receipt mutant={mutant} threatened behavior absent")
    for mutant in TOR_MUTANTS:
        receipt = by_id[mutant]
        dimensions = next(
            (
                (cell, constructor, threshold)
                for cell in TOR_CELLS
                for constructor in TOR_CONSTRUCTORS
                for threshold in TOR_THRESHOLDS
                if mutant == f"tor-direct-{cell}-{constructor}-{threshold}"
            ),
            None,
        )
        if dimensions is None:  # Kept fail-closed if the generated corpus changes.
            raise GateError(f"false receipt mutant={mutant} has unknown Tor dimensions")
        cell, constructor, threshold = dimensions
        observed = receipt.get("observation", {})
        expected = {"cell": cell, "constructor": constructor, "threshold": threshold, "route": "direct"}
        if observed != expected:
            raise GateError(f"false receipt mutant={mutant} missing direct Tor corpus observation")
    late = by_id["tor-direct-late-path"].get("observation", {})
    if late.get("route") != "direct" or late.get("late_path") is not True:
        raise GateError("false receipt mutant=tor-direct-late-path missing direct late-path observation")
    erased = by_id["task-6136-positive"].get("observation", {})
    if erased != {
        "authenticated_tombstone": True,
        "named_key_erased": True,
        "neighbour_preserved": True,
        "generation_immutable": True,
    }:
        raise GateError("false receipt task=6136 incomplete item-erasure observation")
    tor_positive = by_id["task-6140-positive"].get("observation", {})
    corpus = tor_positive.get("corpus")
    expected_corpus = [
        {"cell": cell, "constructor": constructor, "threshold": threshold, "route": "tor", "direct_egress": 0}
        for cell in TOR_CELLS
        for constructor in TOR_CONSTRUCTORS
        for threshold in TOR_THRESHOLDS
    ]
    if corpus != expected_corpus:
        raise GateError("false receipt task=6140 incomplete Tor corpus or direct egress")


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf8"))


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf8")


def command_freeze(args: argparse.Namespace) -> int:
    manifest = sign(freeze(args.candidate, args.task_source, args.frozen_at, args.campaign_started_at), args.private_key)
    write_json(args.output, manifest)
    print(f"TASK6490_FREEZE candidate={args.candidate} tasks={len(GATED_TASKS)} mutants={len(REQUIRED_MUTANTS)} digest={digest(manifest)}")
    return 0


def command_verify(args: argparse.Namespace) -> int:
    manifest, campaign = read_json(args.manifest), read_json(args.campaign)
    verify_manifest(manifest, args.task_source, args.public_key)
    verify_campaign(manifest, campaign, args.public_key)
    verify_semantic_truth(campaign)
    print("TASK6490_OK")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freeze_parser = commands.add_parser("freeze")
    freeze_parser.add_argument("--candidate", required=True)
    freeze_parser.add_argument("--task-source", type=Path, required=True)
    freeze_parser.add_argument("--frozen-at", required=True)
    freeze_parser.add_argument("--campaign-started-at", required=True)
    freeze_parser.add_argument("--private-key", type=Path, required=True)
    freeze_parser.add_argument("--output", type=Path, required=True)
    verify_parser = commands.add_parser("verify")
    verify_parser.add_argument("--task-source", type=Path, required=True)
    verify_parser.add_argument("--public-key", type=Path, required=True)
    verify_parser.add_argument("--manifest", type=Path, required=True)
    verify_parser.add_argument("--campaign", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        return command_freeze(args) if args.command == "freeze" else command_verify(args)
    except GateError as error:
        print(f"TASK6490_REJECT {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
