#!/usr/bin/python3 -I
"""Validate the minimal warm-reset VMQA retry receipt.

This does not operate Azure or run the product.  It closes the retained
receipt shape for the existing fast path and labels fixture evidence
separately from a live candidate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
KIND = "vmqa-fast-cycle-receipt"
PINNED_SOURCE_COMMIT = "1f745c85bb23cf79a956aa87d623905e20f83cf1"
PINNED_SOURCE_TREE = "1b9bbbcaf52fdac66d671d06a5a4ac585ec3167a"
TENANT_ID = "79981b01-1944-4da0-aa9a-fb9f63bddb5e"
SUBSCRIPTION_NAME = "Azure for Students"
RESOURCE_GROUP = "OSL-TWO-CLIENT-LAB"
VM_NAME = "OSL-Azure-Client-1"
VM_ID = "a36c21c3-c563-4320-a8eb-70bc588c7caf"
SESSION_ID = 1
# A later exact-object successor may pin one independently audited live
# receipt. Caller-provided JSON is never sufficient to create live evidence.
PINNED_LIVE_RECEIPT_SHA256: str | None = None
SEQUENCE = [
    "agent-alive-before",
    "exact-build-stage",
    "attempt-1",
    "warm-reset",
    "agent-alive-after",
    "attempt-2",
]
SELFTEST_STEP_SHAPE = "S0:stage,S1:launch,S2:ping,S3:shot,S4:kill"
PASS_STEP_STATUSES = ["pass", "pass", "pass", "pass", "pass"]
NEGATIVE_STEP_STATUSES = ["pass", "blocked", "pass", "blocked", "pass"]
SHA_RE = re.compile(r"^[0-9a-f]{64}$")
UUID_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)
SNAPSHOT_RE = re.compile(
    r"^OSL-Azure-Client-1-WARM-agent-[0-9]{12,14}$"
)
MAX_JSON_BYTES = 1024 * 1024
FORBIDDEN_FIELDS = {
    "secret",
    "password",
    "credential",
    "token",
    "keyBytes",
    "environment",
}


class ReceiptError(ValueError):
    pass


class StrictParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:
        raise ReceiptError(f"arguments refused: {message}")


def canonical_json(value: object) -> bytes:
    return (
        json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("ascii")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        raise ReceiptError(f"{label} fields are not exact")
    return value


def require_sha(value: Any, label: str) -> str:
    if not isinstance(value, str) or SHA_RE.fullmatch(value) is None:
        raise ReceiptError(f"{label} is not a SHA-256 digest")
    return value


def require_int(value: Any, label: str, minimum: int, maximum: int) -> int:
    if (
        not isinstance(value, int)
        or isinstance(value, bool)
        or not minimum <= value <= maximum
    ):
        raise ReceiptError(f"{label} is outside the bounded integer range")
    return value


def reject_secrets(value: Any, label: str = "receipt") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key in FORBIDDEN_FIELDS:
                raise ReceiptError(f"{label} contains forbidden field {key}")
            reject_secrets(child, f"{label}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            reject_secrets(child, f"{label}[{index}]")


def no_duplicate_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ReceiptError(f"duplicate JSON field refused: {key}")
        result[key] = value
    return result


def decode_json(raw: bytes, label: str) -> dict[str, Any]:
    if not raw or len(raw) > MAX_JSON_BYTES:
        raise ReceiptError(f"{label} size is invalid")
    try:
        value = json.loads(
            raw.decode("utf-8", errors="strict"),
            object_pairs_hook=no_duplicate_object,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ReceiptError(f"{label} is not strict JSON") from exc
    if not isinstance(value, dict):
        raise ReceiptError(f"{label} root is not an object")
    reject_secrets(value, label)
    return value


def read_regular_once(path: Path, label: str) -> bytes:
    flags = os.O_RDONLY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise ReceiptError(f"{label} cannot be opened safely") from exc
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1:
            raise ReceiptError(f"{label} is not a single-link regular file")
        raw = b""
        while True:
            block = os.read(descriptor, 65536)
            if not block:
                break
            raw += block
            if len(raw) > MAX_JSON_BYTES:
                raise ReceiptError(f"{label} is too large")
        after = os.fstat(descriptor)
        if (
            before.st_dev,
            before.st_ino,
            before.st_size,
        ) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
        ):
            raise ReceiptError(f"{label} changed while read")
        return raw
    finally:
        os.close(descriptor)


def resource_prefix(subscription_id: str) -> str:
    return (
        f"/subscriptions/{subscription_id}/resourceGroups/{RESOURCE_GROUP}"
        "/providers/Microsoft.Compute"
    )


def validate_target(value: Any) -> dict[str, Any]:
    target = exact_object(
        value,
        {
            "tenantId",
            "subscriptionId",
            "subscriptionName",
            "resourceGroup",
            "vmName",
            "vmId",
            "sessionId",
        },
        "target",
    )
    if (
        target["tenantId"] != TENANT_ID
        or not isinstance(target["subscriptionId"], str)
        or UUID_RE.fullmatch(target["subscriptionId"]) is None
        or target["subscriptionName"] != SUBSCRIPTION_NAME
        or target["resourceGroup"] != RESOURCE_GROUP
        or target["vmName"] != VM_NAME
        or target["vmId"] != VM_ID
        or not isinstance(target["sessionId"], int)
        or isinstance(target["sessionId"], bool)
        or target["sessionId"] != SESSION_ID
    ):
        raise ReceiptError("target tenant/subscription/VM/session differs")
    return target


def validate_lineage(value: Any, subscription_id: str) -> dict[str, Any]:
    lineage = exact_object(
        value,
        {
            "snapshotName",
            "snapshotResourceId",
            "lineageTag",
            "sourceDiskResourceId",
            "restoredDiskResourceId",
            "rawSnapshotSha256",
        },
        "lineage",
    )
    prefix = resource_prefix(subscription_id)
    if (
        not isinstance(lineage["snapshotName"], str)
        or SNAPSHOT_RE.fullmatch(lineage["snapshotName"]) is None
        or lineage["snapshotResourceId"]
        != f"{prefix}/snapshots/{lineage['snapshotName']}"
        or lineage["lineageTag"] != "warm-iteration"
        or not isinstance(lineage["sourceDiskResourceId"], str)
        or not lineage["sourceDiskResourceId"].startswith(f"{prefix}/disks/")
        or not isinstance(lineage["restoredDiskResourceId"], str)
        or not lineage["restoredDiskResourceId"].startswith(f"{prefix}/disks/")
        or lineage["sourceDiskResourceId"] == lineage["restoredDiskResourceId"]
    ):
        raise ReceiptError("warm snapshot or disk lineage differs")
    require_sha(lineage["rawSnapshotSha256"], "raw snapshot")
    return lineage


def validate_agent(value: Any, label: str) -> dict[str, Any]:
    agent = exact_object(
        value,
        {
            "receiptSha256",
            "sizeBytes",
            "interactive",
            "sessionId",
            "ageSeconds",
            "agentSha256",
            "win32Sha256",
        },
        label,
    )
    require_sha(agent["receiptSha256"], f"{label} receipt")
    require_sha(agent["agentSha256"], f"{label} agent")
    require_sha(agent["win32Sha256"], f"{label} win32")
    require_int(agent["sizeBytes"], f"{label} bytes", 1, MAX_JSON_BYTES)
    require_int(agent["ageSeconds"], f"{label} heartbeat age", -120, 60)
    if (
        agent["interactive"] is not True
        or not isinstance(agent["sessionId"], int)
        or isinstance(agent["sessionId"], bool)
        or agent["sessionId"] != SESSION_ID
    ):
        raise ReceiptError(f"{label} does not prove interactive agent alive")
    return agent


def validate_attempt(
    value: Any,
    label: str,
    expected_executable_sha256: str,
    *,
    retry_trigger: bool,
) -> dict[str, Any]:
    attempt = exact_object(
        value,
        {
            "runNonce",
            "executed",
            "executableSha256",
            "result",
            "positive",
            "negative",
        },
        label,
    )
    require_sha(attempt["runNonce"], f"{label} run nonce")
    if attempt["executed"] is not True:
        raise ReceiptError(f"{label} execution was skipped")
    require_sha(attempt["executableSha256"], f"{label} executable")
    if attempt["executableSha256"] != expected_executable_sha256:
        raise ReceiptError(f"{label} ran a stale executable")
    positive = validate_selftest_half(
        attempt["positive"], f"{label} positive"
    )
    negative = validate_selftest_half(
        attempt["negative"], f"{label} negative"
    )
    if (
        negative["outcome"] != "blocked"
        or negative["stepStatuses"] != NEGATIVE_STEP_STATUSES
    ):
        raise ReceiptError(f"{label} negative control is not exact")
    if retry_trigger:
        if (
            attempt["result"] != "retry"
            or positive["outcome"] == "pass"
            or positive["stepStatuses"] == PASS_STEP_STATUSES
        ):
            raise ReceiptError(f"{label} did not justify reset and retry")
    elif (
        attempt["result"] != "pass"
        or positive["outcome"] != "pass"
        or positive["stepStatuses"] != PASS_STEP_STATUSES
    ):
        raise ReceiptError(f"{label} is not a passing selftest pair")
    return attempt


def validate_selftest_half(value: Any, label: str) -> dict[str, Any]:
    half = exact_object(
        value,
        {
            "receiptSha256",
            "sizeBytes",
            "stepsExecuted",
            "stepShape",
            "stepStatuses",
            "outcome",
        },
        label,
    )
    require_sha(half["receiptSha256"], f"{label} receipt")
    require_int(half["sizeBytes"], f"{label} bytes", 1, MAX_JSON_BYTES)
    require_int(half["stepsExecuted"], f"{label} steps", 5, 5)
    if half["stepShape"] != SELFTEST_STEP_SHAPE:
        raise ReceiptError(f"{label} step sequence differs")
    if (
        not isinstance(half["stepStatuses"], list)
        or len(half["stepStatuses"]) != 5
        or any(
            status not in {"pass", "fail", "blocked", "unmeasurable"}
            for status in half["stepStatuses"]
        )
    ):
        raise ReceiptError(f"{label} step statuses differ")
    if half["outcome"] not in {
        "pass",
        "fail",
        "blocked",
        "unmeasurable",
    }:
        raise ReceiptError(f"{label} outcome differs")
    return half


def validate_receipt(
    value: Any, *, allow_simulation: bool = False
) -> dict[str, Any]:
    reject_secrets(value)
    receipt = exact_object(
        value,
        {
            "schemaVersion",
            "kind",
            "evidenceTier",
            "sequence",
            "target",
            "lineage",
            "build",
            "agentAliveBefore",
            "attemptOne",
            "reset",
            "agentAliveAfter",
            "attemptTwo",
        },
        "fast-cycle receipt",
    )
    if (
        not isinstance(receipt["schemaVersion"], int)
        or isinstance(receipt["schemaVersion"], bool)
        or receipt["schemaVersion"] != SCHEMA_VERSION
        or receipt["kind"] != KIND
    ):
        raise ReceiptError("receipt schema or kind differs")
    tier = receipt["evidenceTier"]
    if tier not in {"live", "simulation"}:
        raise ReceiptError("evidence tier differs")
    if tier == "simulation" and not allow_simulation:
        raise ReceiptError("simulation receipt is not live evidence")
    if receipt["sequence"] != SEQUENCE:
        raise ReceiptError("fast-cycle order differs")
    target = validate_target(receipt["target"])
    lineage = validate_lineage(receipt["lineage"], target["subscriptionId"])
    build = exact_object(
        receipt["build"],
        {
            "sourceCommit",
            "sourceTree",
            "buildIdentitySha256",
            "executableSha256",
            "stagedExecutableSha256",
            "stageReceiptSha256",
            "stagedSizeBytes",
        },
        "build stage",
    )
    if (
        build["sourceCommit"] != PINNED_SOURCE_COMMIT
        or build["sourceTree"] != PINNED_SOURCE_TREE
    ):
        raise ReceiptError("stale source build differs")
    for field in (
        "buildIdentitySha256",
        "executableSha256",
        "stagedExecutableSha256",
        "stageReceiptSha256",
    ):
        require_sha(build[field], f"build {field}")
    require_int(
        build["stagedSizeBytes"], "staged executable bytes", 1, 1024**3
    )
    if build["executableSha256"] != build["stagedExecutableSha256"]:
        raise ReceiptError("staged executable is not the exact build")
    before = validate_agent(receipt["agentAliveBefore"], "agent before")
    after = validate_agent(receipt["agentAliveAfter"], "agent after")
    if (
        before["receiptSha256"] == after["receiptSha256"]
        or before["agentSha256"] != after["agentSha256"]
        or before["win32Sha256"] != after["win32Sha256"]
    ):
        raise ReceiptError("post-reset agent observation is stale or changed")
    first = validate_attempt(
        receipt["attemptOne"],
        "attempt one",
        build["executableSha256"],
        retry_trigger=True,
    )
    second = validate_attempt(
        receipt["attemptTwo"],
        "attempt two",
        build["executableSha256"],
        retry_trigger=False,
    )
    if (
        first["runNonce"] == second["runNonce"]
        or first["positive"]["receiptSha256"]
        == second["positive"]["receiptSha256"]
        or first["negative"]["receiptSha256"]
        == second["negative"]["receiptSha256"]
    ):
        raise ReceiptError("retry reused stale run or verdict evidence")
    reset = exact_object(
        receipt["reset"],
        {
            "receiptSha256",
            "sizeBytes",
            "snapshotResourceId",
            "restoredDiskResourceId",
            "oldPid",
            "oldExecutableSha256",
            "oldPidAbsent",
            "oldExecutableAbsent",
        },
        "reset",
    )
    require_sha(reset["receiptSha256"], "reset receipt")
    require_sha(reset["oldExecutableSha256"], "reset old executable")
    require_int(reset["sizeBytes"], "reset receipt bytes", 1, MAX_JSON_BYTES)
    require_int(reset["oldPid"], "reset old PID", 1, 2**31 - 1)
    if (
        reset["snapshotResourceId"] != lineage["snapshotResourceId"]
        or reset["restoredDiskResourceId"]
        != lineage["restoredDiskResourceId"]
        or reset["oldExecutableSha256"] != build["executableSha256"]
        or reset["oldPidAbsent"] is not True
        or reset["oldExecutableAbsent"] is not True
    ):
        raise ReceiptError("reset did not prove exact lineage and old process absence")
    receipt_sha = sha256_bytes(canonical_json(receipt))
    if tier == "live":
        if PINNED_LIVE_RECEIPT_SHA256 is None:
            raise ReceiptError(
                "live receipt is not independently source-pinned"
            )
        if receipt_sha != PINNED_LIVE_RECEIPT_SHA256:
            raise ReceiptError("live receipt differs from the source pin")
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "valid-fast-cycle-receipt",
        "evidenceTier": tier,
        "receiptSha256": receipt_sha,
        "sourceCommit": build["sourceCommit"],
        "sourceTree": build["sourceTree"],
        "executableSha256": build["executableSha256"],
        "snapshotResourceId": lineage["snapshotResourceId"],
        "attemptTwoPositiveReceiptSha256": second["positive"][
            "receiptSha256"
        ],
        "attemptTwoNegativeReceiptSha256": second["negative"][
            "receiptSha256"
        ],
        "runtimeProvenByThisValidator": False,
    }


def build_parser() -> StrictParser:
    parser = StrictParser(allow_abbrev=False)
    parser.add_argument("--receipt", required=True)
    parser.add_argument(
        "--internal-test-fixture",
        action="store_true",
        help=argparse.SUPPRESS,
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        path = Path(args.receipt)
        if not path.is_absolute():
            raise ReceiptError("receipt path must be absolute")
        value = decode_json(read_regular_once(path, "receipt"), "receipt")
        result = validate_receipt(
            value, allow_simulation=args.internal_test_fixture
        )
        sys.stdout.write(canonical_json(result).decode("ascii"))
        return 0
    except ReceiptError as exc:
        print(f"VMQA FAST CYCLE REFUSED: {exc}", file=sys.stderr)
        return 9


if __name__ == "__main__":
    raise SystemExit(main())
