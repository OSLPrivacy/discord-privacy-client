#!/usr/bin/python3 -I
"""Read-only admission command for one exact VMQA fast-cycle receipt."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.util
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


SCRIPT_ROOT = Path(__file__).resolve().parent
RECEIPT_PROGRAM = SCRIPT_ROOT / "vmqa_fast_cycle_receipt.py"
RECEIPT_SPEC = importlib.util.spec_from_file_location(
    "_vmqa_fast_cycle_receipt_operator", RECEIPT_PROGRAM
)
if RECEIPT_SPEC is None or RECEIPT_SPEC.loader is None:
    raise RuntimeError("cannot load fast-cycle receipt validator")
cycle = importlib.util.module_from_spec(RECEIPT_SPEC)
RECEIPT_SPEC.loader.exec_module(cycle)

PYTHON = Path("/usr/bin/python3")
PRODUCTION_HOME = Path(os.environ.get("OSL_VMQA_PRODUCTION_HOME", "/home/osl-vmqa"))
PRODUCTION_USER = os.environ.get("OSL_VMQA_PRODUCTION_USER", "osl-vmqa")
AZ = Path(os.environ.get("OSL_VMQA_AZ", str(PRODUCTION_HOME / ".local/bin/az")))
AZ_SHA256 = "bd6ddadadca89ca4b6583da52868047c84d24a706f7bf1b9022b6f904f378674"
SUBSCRIPTION_ID = "a7d5d97b-3bf4-460a-8a7c-6bd11b5810b1"
SHARE = SCRIPT_ROOT / "vmqa-share.sh"
BUILD_EVIDENCE = SCRIPT_ROOT / "vmqa_build_evidence.py"
BUILD_CONTRACT = SCRIPT_ROOT / "vmqa-contract.py"
HEARTBEAT_BLOB = f"agent/{cycle.VM_NAME}/heartbeat.json"
MAX_FILE_BYTES = 1024 * 1024 * 1024


class OperatorError(ValueError):
    pass


def sha256_bytes(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def require_absolute_real_path(path: Path, label: str) -> Path:
    if not path.is_absolute():
        raise OperatorError(f"{label} must be an absolute path")
    absolute = Path(os.path.abspath(path))
    try:
        resolved = absolute.resolve(strict=True)
    except OSError as exc:
        raise OperatorError(f"{label} is unavailable") from exc
    if resolved != absolute:
        raise OperatorError(f"{label} must not contain symlinks")
    return absolute


def read_regular(path: Path, label: str, maximum: int) -> bytes:
    path = require_absolute_real_path(path, label)
    before = path.stat()
    if not stat.S_ISREG(before.st_mode) or before.st_size < 1:
        raise OperatorError(f"{label} must be a nonempty regular file")
    if before.st_size > maximum:
        raise OperatorError(f"{label} is too large")
    raw = path.read_bytes()
    after = path.stat()
    if (
        before.st_dev,
        before.st_ino,
        before.st_size,
        before.st_mtime_ns,
    ) != (
        after.st_dev,
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
    ) or len(raw) != before.st_size:
        raise OperatorError(f"{label} changed while being read")
    return raw


def fixed_environment() -> dict[str, str]:
    return {
        "HOME": PRODUCTION_HOME.as_posix(),
        "PATH": f"{(PRODUCTION_HOME / '.local/bin').as_posix()}:/usr/bin:/bin",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "LOGNAME": PRODUCTION_USER,
        "USER": PRODUCTION_USER,
    }


def run_checked(argv: list[str], label: str) -> bytes:
    try:
        completed = subprocess.run(
            argv,
            check=False,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=fixed_environment(),
            timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise OperatorError(f"{label} could not run") from exc
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace").strip()
        raise OperatorError(
            f"{label} refused (exit {completed.returncode}): {detail}"
        )
    return completed.stdout


def verify_azure_account() -> None:
    az_raw = read_regular(AZ, "pinned Azure CLI", 1024 * 1024)
    if sha256_bytes(az_raw) != AZ_SHA256:
        raise OperatorError("pinned Azure CLI hash differs")
    raw = run_checked(
        [
            str(AZ),
            "account",
            "show",
            "--subscription",
            SUBSCRIPTION_ID,
            "--query",
            "{tenantId:tenantId,id:id,name:name,state:state}",
            "-o",
            "json",
            "--only-show-errors",
        ],
        "authorized Azure target check",
    )
    try:
        account = cycle.decode_json(raw, "Azure account response")
    except cycle.ReceiptError as exc:
        raise OperatorError("Azure account response is not strict JSON") from exc
    expected_account = {
        "tenantId": cycle.TENANT_ID,
        "id": SUBSCRIPTION_ID,
        "name": cycle.SUBSCRIPTION_NAME,
        "state": "Enabled",
    }
    if account != expected_account:
        raise OperatorError("active Azure target is not the exact authorized account")
    raw = run_checked(
        [
            str(AZ),
            "vm",
            "show",
            "--subscription",
            SUBSCRIPTION_ID,
            "--resource-group",
            cycle.RESOURCE_GROUP,
            "--name",
            cycle.VM_NAME,
            "--query",
            "{id:id,vmId:vmId,name:name,resourceGroup:resourceGroup,"
            "provisioningState:provisioningState}",
            "-o",
            "json",
            "--only-show-errors",
        ],
        "authorized Azure VM identity check",
    )
    try:
        vm = cycle.decode_json(raw, "Azure VM response")
    except cycle.ReceiptError as exc:
        raise OperatorError("Azure VM response is not strict JSON") from exc
    resource_id = (
        f"/subscriptions/{SUBSCRIPTION_ID}/resourceGroups/"
        f"{cycle.RESOURCE_GROUP}/providers/Microsoft.Compute/"
        f"virtualMachines/{cycle.VM_NAME}"
    )
    expected_vm = {
        "id": resource_id,
        "vmId": cycle.VM_ID,
        "name": cycle.VM_NAME,
        "resourceGroup": cycle.RESOURCE_GROUP,
        "provisioningState": "Succeeded",
    }
    if vm != expected_vm:
        raise OperatorError("Azure VM is not the exact authorized target")


def run_bundle_validators(bundle: Path) -> None:
    exe = bundle / "outputs/osl-privacy-hub.exe"
    run_checked(
        [
            str(PYTHON),
            "-I",
            str(BUILD_EVIDENCE),
            "verify-bundle",
            "--bundle",
            str(bundle),
        ],
        "production bundle validation",
    )
    run_checked(
        [
            str(PYTHON),
            "-I",
            str(BUILD_CONTRACT),
            "validate-build",
            "--build-identity",
            str(bundle / "build-identity.json"),
            "--exe",
            str(exe),
            "--evidence-dir",
            str(bundle / "build-evidence"),
        ],
        "production build identity validation",
    )


def fetch_live_heartbeat() -> bytes:
    with tempfile.TemporaryDirectory(prefix="vmqa-fast-cycle-heartbeat-") as tmp:
        destination = Path(tmp) / "heartbeat.json"
        run_checked(
            [str(SHARE), "get", HEARTBEAT_BLOB, str(destination)],
            "live agent heartbeat fetch",
        )
        return read_regular(
            destination, "live agent heartbeat", cycle.MAX_JSON_BYTES
        )


def parse_heartbeat(raw: bytes) -> dict[str, Any]:
    heartbeat = cycle.decode_json(raw, "live agent heartbeat")
    expected_fields = {
        "schemaVersion",
        "vmName",
        "utc",
        "agentSha256",
        "win32Sha256",
        "sessionId",
        "interactiveUserName",
        "isInteractiveSession",
        "blobState",
        "failure",
    }
    if set(heartbeat) != expected_fields:
        raise OperatorError("live agent heartbeat fields are not exact")
    if (
        heartbeat["schemaVersion"] != 1
        or heartbeat["vmName"] != cycle.VM_NAME
        or heartbeat["sessionId"] != cycle.SESSION_ID
        or heartbeat["isInteractiveSession"] is not True
        or heartbeat["blobState"] != "ok"
        or heartbeat["failure"] != ""
        or not isinstance(heartbeat["interactiveUserName"], str)
        or not heartbeat["interactiveUserName"]
    ):
        raise OperatorError("live agent heartbeat is not the exact interactive target")
    cycle.require_sha(heartbeat["agentSha256"], "live heartbeat agent")
    cycle.require_sha(heartbeat["win32Sha256"], "live heartbeat win32")
    if not isinstance(heartbeat["utc"], str):
        raise OperatorError("live agent heartbeat UTC is absent")
    return heartbeat


def heartbeat_age_seconds(value: str, now: dt.datetime | None = None) -> int:
    try:
        stamp = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise OperatorError("live agent heartbeat UTC is invalid") from exc
    if stamp.tzinfo is None:
        raise OperatorError("live agent heartbeat UTC has no timezone")
    if now is None:
        now = dt.datetime.now(dt.timezone.utc)
    return int((now - stamp.astimezone(dt.timezone.utc)).total_seconds())


def heartbeat_matches_retained(
    raw: bytes, heartbeat: dict[str, Any], retained: dict[str, Any]
) -> bool:
    return (
        sha256_bytes(raw) == retained["receiptSha256"]
        and len(raw) == retained["sizeBytes"]
        and heartbeat["sessionId"] == retained["sessionId"]
        and heartbeat["isInteractiveSession"] == retained["interactive"]
        and heartbeat["agentSha256"] == retained["agentSha256"]
        and heartbeat["win32Sha256"] == retained["win32Sha256"]
    )


def executable_identity_diff(exe_raw: bytes, build: dict[str, Any]) -> str:
    actual_sha = sha256_bytes(exe_raw)
    actual_size = len(exe_raw)
    differences: list[str] = []
    if actual_sha != build["executableSha256"]:
        differences.append(
            "executableSha256 "
            f"expected={build['executableSha256']} actual={actual_sha}"
        )
    if actual_sha != build["stagedExecutableSha256"]:
        differences.append(
            "stagedExecutableSha256 "
            f"expected={build['stagedExecutableSha256']} actual={actual_sha}"
        )
    if actual_size != build["stagedSizeBytes"]:
        differences.append(
            f"stagedSizeBytes expected={build['stagedSizeBytes']} actual={actual_size}"
        )
    return "; ".join(differences)


def validate_operator(
    receipt_path: Path,
    bundle_path: Path,
    post_reset_heartbeat_path: Path,
) -> dict[str, Any]:
    receipt_raw = read_regular(
        receipt_path, "fast-cycle receipt", cycle.MAX_JSON_BYTES
    )
    receipt = cycle.decode_json(receipt_raw, "fast-cycle receipt")
    try:
        admission = cycle.validate_receipt(receipt)
    except cycle.ReceiptError as exc:
        raise OperatorError(f"fast-cycle receipt refused: {exc}") from exc
    if (
        admission["evidenceTier"] != "live"
        or receipt["target"]["subscriptionId"] != SUBSCRIPTION_ID
    ):
        raise OperatorError("receipt target is not the exact authorized live target")

    bundle = require_absolute_real_path(bundle_path, "production bundle")
    if not bundle.is_dir():
        raise OperatorError("production bundle must be a real directory")
    run_bundle_validators(bundle)

    identity_raw = read_regular(
        bundle / "build-identity.json",
        "production build identity",
        cycle.MAX_JSON_BYTES,
    )
    exe_raw = read_regular(
        bundle / "outputs/osl-privacy-hub.exe",
        "production executable",
        MAX_FILE_BYTES,
    )
    build = receipt["build"]
    if sha256_bytes(identity_raw) != build["buildIdentitySha256"]:
        raise OperatorError("production build identity hash differs from receipt")
    if (
        sha256_bytes(exe_raw) != build["executableSha256"]
        or sha256_bytes(exe_raw) != build["stagedExecutableSha256"]
        or len(exe_raw) != build["stagedSizeBytes"]
    ):
        diff = executable_identity_diff(exe_raw, build)
        raise OperatorError(f"exact staged executable bytes differ: {diff}")

    retained = receipt["agentAliveAfter"]
    historic_raw = read_regular(
        post_reset_heartbeat_path,
        "retained post-reset heartbeat",
        cycle.MAX_JSON_BYTES,
    )
    historic = parse_heartbeat(historic_raw)
    if not heartbeat_matches_retained(historic_raw, historic, retained):
        raise OperatorError("retained post-reset heartbeat bytes differ")

    verify_azure_account()
    current_raw = fetch_live_heartbeat()
    current = parse_heartbeat(current_raw)
    age = heartbeat_age_seconds(current["utc"])
    if age < -120 or age > 60:
        raise OperatorError(f"live agent heartbeat age is outside bounds: {age}")
    if (
        current["sessionId"] != retained["sessionId"]
        or current["isInteractiveSession"] != retained["interactive"]
        or current["agentSha256"] != retained["agentSha256"]
        or current["win32Sha256"] != retained["win32Sha256"]
    ):
        raise OperatorError(
            "fresh live heartbeat identity differs from post-reset receipt"
        )

    return {
        "schemaVersion": 1,
        "status": "validated-minimal-live-readiness",
        "evidenceTier": "live",
        "targetVm": cycle.VM_NAME,
        "sourceCommit": admission["sourceCommit"],
        "sourceTree": admission["sourceTree"],
        "executableSha256": admission["executableSha256"],
        "receiptSha256": admission["receiptSha256"],
        "postResetHeartbeatSha256": sha256_bytes(historic_raw),
        "currentHeartbeatSha256": sha256_bytes(current_raw),
        "attemptTwoPositiveReceiptSha256": admission[
            "attemptTwoPositiveReceiptSha256"
        ],
        "attemptTwoNegativeReceiptSha256": admission[
            "attemptTwoNegativeReceiptSha256"
        ],
        "runtimeProofAwarded": False,
        "acceptanceDelta": 0,
    }


def main() -> int:
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument("--receipt", required=True, type=Path)
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument(
        "--post-reset-heartbeat", required=True, type=Path
    )
    try:
        args = parser.parse_args()
        result = validate_operator(
            args.receipt, args.bundle, args.post_reset_heartbeat
        )
    except (OperatorError, cycle.ReceiptError, OSError) as exc:
        print(f"VMQA FAST CYCLE NOT READY: {exc}", file=sys.stderr)
        return 9
    sys.stdout.buffer.write(cycle.canonical_json(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
