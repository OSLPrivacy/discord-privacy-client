#!/usr/bin/env python3
"""Strict, closed-schema validation for retained VMQA evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 2
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
RUN_ID_RE = re.compile(r"^[A-Za-z0-9._-]{1,96}$")
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()

REQUEST_KEYS = {
    "schemaVersion",
    "runId",
    "identifier",
    "runStartUtc",
    "exeSha256",
    "buildIdentitySha256",
    "buildIdentity",
    "steps",
}
VERDICT_KEYS = {
    "schemaVersion",
    "runId",
    "requestSha256",
    "requestExeSha256",
    "buildIdentitySha256",
    "vmName",
    "agentSha",
    "win32Sha",
    "runStartUtc",
    "agentStartedUtc",
    "finishedUtc",
    "overall",
    "steps",
    "diffKey",
}
STEP_KEYS = {"id", "verb", "args"}
STEP_RESULT_KEYS = {"id", "verb", "status", "detail", "artifacts", "facts"}

ARG_KEYS = {
    "stage": {"exeSha256"},
    "launch": {"exeSha256", "timeoutSeconds"},
    "ping": set(),
    "shot": {"name", "expectedSurfaceClass"},
    "click": {"winX", "winY", "settleMs"},
    "type": {"text", "settleMs"},
    "key": {"key", "settleMs"},
    "wait": {"ms"},
    "kill": set(),
}
FACT_KEYS = {
    "stage": set(),
    "launch": {"launchedPid", "exeSha256", "launchProcessStarted"},
    "ping": {"markerWindowsTotal"},
    "shot": {
        "surfacePid",
        "surfaceHwnd",
        "surfaceClass",
        "postSurfaceClass",
        "rawRect",
        "dwmRect",
        "postRawRect",
        "postDwmRect",
        "surfaceWidth",
        "surfaceHeight",
        "captureDistinctColors",
        "rawSurfaceWidth",
        "rawSurfaceHeight",
        "boundsSource",
        "boundsWithinVirtualDesktop",
        "coversVirtualDesktop",
        "surfaceDpi",
        "normalizedForCapture",
        "artifactPath",
        "pngSha256",
        "foregroundPre",
        "foregroundPost",
        "sampleGridPre",
        "sampleGridPost",
        "unoccludedPre",
        "unoccludedPost",
        "rectStable",
    },
    "click": set(),
    "type": set(),
    "key": set(),
    "wait": set(),
    "kill": {
        "cleanupPid",
        "cleanupOutcome",
        "cleanupExeSha256",
        "cleanupPidAbsent",
        "cleanupExecutableAbsent",
        "cleanupMatchingExeCount",
    },
}
RECT_KEYS = {"left", "top", "right", "bottom", "width", "height"}


class ContractError(ValueError):
    pass


def load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise ContractError(f"{label} is not valid UTF-8 JSON: {exc}") from exc


def exact_object(value: Any, keys: set[str], label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(f"{label} must be an object")
    actual = set(value)
    if actual != keys:
        raise ContractError(
            f"{label} fields are not exact; missing={sorted(keys - actual)} "
            f"unknown={sorted(actual - keys)}"
        )
    return value


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_sha(value: Any, label: str) -> str:
    if not isinstance(value, str) or not SHA256_RE.fullmatch(value):
        raise ContractError(f"{label} must be lowercase SHA-256")
    return value


def require_text(value: Any, label: str, *, maximum: int = 4096) -> str:
    if not isinstance(value, str) or not value or len(value) > maximum:
        raise ContractError(f"{label} must be a nonempty bounded string")
    return value


def validate_schema_version(value: Any, label: str) -> None:
    if type(value) is not int or value != SCHEMA_VERSION:
        raise ContractError(
            f"{label}.schemaVersion must be exactly {SCHEMA_VERSION}, got {value!r}"
        )


def validate_build_identity(value: Any, *, exe_path: Path | None = None) -> dict[str, Any]:
    identity = exact_object(
        value,
        {"schemaVersion", "source", "ui", "build", "artifacts"},
        "buildIdentity",
    )
    validate_schema_version(identity["schemaVersion"], "buildIdentity")
    source = exact_object(
        identity["source"],
        {"commit", "tree", "clean", "dirtyFingerprint"},
        "buildIdentity.source",
    )
    if not isinstance(source["commit"], str) or not COMMIT_RE.fullmatch(source["commit"]):
        raise ContractError("buildIdentity.source.commit must be full lowercase Git commit")
    if not isinstance(source["tree"], str) or not COMMIT_RE.fullmatch(source["tree"]):
        raise ContractError("buildIdentity.source.tree must be full lowercase Git tree")
    if source["clean"] is not True:
        raise ContractError("buildIdentity.source.clean must be true")
    if source["dirtyFingerprint"] != EMPTY_SHA256:
        raise ContractError("buildIdentity.source.dirtyFingerprint must hash an empty status")

    ui = exact_object(identity["ui"], {"distSha256"}, "buildIdentity.ui")
    require_sha(ui["distSha256"], "buildIdentity.ui.distSha256")

    build = exact_object(
        identity["build"],
        {"target", "features", "profile", "commands", "toolchain"},
        "buildIdentity.build",
    )
    if build["target"] != "x86_64-pc-windows-gnu":
        raise ContractError("buildIdentity.build.target is not the Windows target")
    if build["features"] != ["desktop"]:
        raise ContractError("buildIdentity.build.features must equal ['desktop']")
    if build["profile"] != "release":
        raise ContractError("buildIdentity.build.profile must be release")
    if (
        not isinstance(build["commands"], list)
        or len(build["commands"]) != 2
        or not all(isinstance(command, list) and command for command in build["commands"])
        or not all(
            isinstance(token, str) and token
            for command in build["commands"]
            for token in command
        )
    ):
        raise ContractError("buildIdentity.build.commands must contain two argv arrays")
    expected_commands = [
        ["npm", "run", "build"],
        [
            "osl-cargo",
            "build",
            "--release",
            "--features",
            "desktop",
            "--bin",
            "osl-privacy-hub",
            "--target",
            "x86_64-pc-windows-gnu",
        ],
    ]
    if build["commands"] != expected_commands:
        raise ContractError("buildIdentity.build.commands are not the exact release build argv")
    toolchain = exact_object(
        build["toolchain"],
        {"rustc", "cargo", "node", "npm", "oslCargoSha256"},
        "buildIdentity.build.toolchain",
    )
    for key in ("rustc", "cargo", "node", "npm"):
        require_text(toolchain[key], f"buildIdentity.build.toolchain.{key}", maximum=512)
    require_sha(
        toolchain["oslCargoSha256"],
        "buildIdentity.build.toolchain.oslCargoSha256",
    )

    artifacts = exact_object(
        identity["artifacts"], {"executable", "loader"}, "buildIdentity.artifacts"
    )
    for name, expected_name in (
        ("executable", "osl-privacy-hub.exe"),
        ("loader", "WebView2Loader.dll"),
    ):
        artifact = exact_object(
            artifacts[name], {"name", "sha256", "sizeBytes"}, f"buildIdentity.artifacts.{name}"
        )
        if artifact["name"] != expected_name:
            raise ContractError(f"buildIdentity.artifacts.{name}.name is not {expected_name}")
        require_sha(artifact["sha256"], f"buildIdentity.artifacts.{name}.sha256")
        if type(artifact["sizeBytes"]) is not int or artifact["sizeBytes"] <= 0:
            raise ContractError(f"buildIdentity.artifacts.{name}.sizeBytes must be positive")

    if exe_path is not None:
        executable = artifacts["executable"]
        if not exe_path.is_file():
            raise ContractError(f"independent executable is missing: {exe_path}")
        if sha256_file(exe_path) != executable["sha256"]:
            raise ContractError("build identity executable digest differs from independent bytes")
        if exe_path.stat().st_size != executable["sizeBytes"]:
            raise ContractError("build identity executable size differs from independent bytes")
    return identity


def validate_request(
    value: Any,
    *,
    identity: dict[str, Any],
    identity_sha256: str,
) -> dict[str, Any]:
    request = exact_object(value, REQUEST_KEYS, "request")
    validate_schema_version(request["schemaVersion"], "request")
    if not isinstance(request["runId"], str) or not RUN_ID_RE.fullmatch(request["runId"]):
        raise ContractError("request.runId is unsafe")
    require_text(request["identifier"], "request.identifier", maximum=255)
    require_text(request["runStartUtc"], "request.runStartUtc", maximum=64)
    exe_sha = require_sha(request["exeSha256"], "request.exeSha256")
    if exe_sha != identity["artifacts"]["executable"]["sha256"]:
        raise ContractError("request executable differs from build identity")
    if request["buildIdentitySha256"] != identity_sha256:
        raise ContractError("request buildIdentitySha256 differs from retained identity bytes")
    if request["buildIdentity"] != identity:
        raise ContractError("request embedded build identity differs from retained identity")
    if not isinstance(request["steps"], list) or not request["steps"]:
        raise ContractError("request.steps must be a nonempty array")
    for index, raw_step in enumerate(request["steps"]):
        step = exact_object(raw_step, STEP_KEYS, f"request.steps[{index}]")
        require_text(step["id"], f"request.steps[{index}].id", maximum=96)
        verb = step["verb"]
        if verb not in ARG_KEYS:
            raise ContractError(f"request.steps[{index}].verb is unsupported")
        args = exact_object(
            step["args"], ARG_KEYS[verb], f"request.steps[{index}].args"
        )
        if "exeSha256" in args and args["exeSha256"] != exe_sha:
            raise ContractError(f"request.steps[{index}] executable differs from request")
    return request


def validate_verdict(
    value: Any,
    *,
    request: dict[str, Any],
    request_sha256: str,
    identity_sha256: str,
) -> dict[str, Any]:
    expected_keys = set(VERDICT_KEYS)
    if isinstance(value, dict) and value.get("overall") == "blocked":
        expected_keys.add("diagnosis")
    verdict = exact_object(value, expected_keys, "verdict")
    validate_schema_version(verdict["schemaVersion"], "verdict")
    if verdict["runId"] != request["runId"]:
        raise ContractError("verdict.runId differs from request")
    if verdict["requestSha256"] != request_sha256:
        raise ContractError("verdict.requestSha256 differs from retained request bytes")
    if verdict["requestExeSha256"] != request["exeSha256"]:
        raise ContractError("verdict request executable differs from request")
    if verdict["buildIdentitySha256"] != identity_sha256:
        raise ContractError("verdict build identity digest differs from retained identity")
    for key in ("agentSha", "win32Sha"):
        require_sha(verdict[key], f"verdict.{key}")
    for key in ("vmName", "runStartUtc", "agentStartedUtc", "finishedUtc", "diffKey"):
        if not isinstance(verdict[key], str):
            raise ContractError(f"verdict.{key} must be a string")
    if verdict["overall"] not in ("pass", "fail", "unmeasurable", "blocked"):
        raise ContractError("verdict.overall is unsupported")
    if not isinstance(verdict["steps"], list):
        raise ContractError("verdict.steps must be an array")
    for index, raw_result in enumerate(verdict["steps"]):
        result = exact_object(raw_result, STEP_RESULT_KEYS, f"verdict.steps[{index}]")
        verb = result["verb"]
        if verb not in FACT_KEYS:
            raise ContractError(f"verdict.steps[{index}].verb is unsupported")
        if result["status"] not in ("pass", "fail", "unmeasurable", "blocked"):
            raise ContractError(f"verdict.steps[{index}].status is unsupported")
        require_text(result["id"], f"verdict.steps[{index}].id", maximum=96)
        if not isinstance(result["detail"], str):
            raise ContractError(f"verdict.steps[{index}].detail must be a string")
        if (
            not isinstance(result["artifacts"], list)
            or not all(isinstance(path, str) for path in result["artifacts"])
        ):
            raise ContractError(f"verdict.steps[{index}].artifacts must be strings")
        facts = exact_object(
            result["facts"], set(result["facts"]) & FACT_KEYS[verb], f"verdict.steps[{index}].facts"
        )
        unknown = set(facts) - FACT_KEYS[verb]
        if unknown:
            raise ContractError(
                f"verdict.steps[{index}].facts has unknown fields {sorted(unknown)}"
            )
        for rect_name in ("rawRect", "dwmRect", "postRawRect", "postDwmRect"):
            if rect_name in facts:
                exact_object(
                    facts[rect_name],
                    RECT_KEYS,
                    f"verdict.steps[{index}].facts.{rect_name}",
                )
    return verdict


def verify_pair(args: argparse.Namespace) -> None:
    identity_path = Path(args.build_identity)
    identity = validate_build_identity(
        load_json(identity_path, "build identity"), exe_path=Path(args.exe)
    )
    identity_sha = sha256_file(identity_path)
    for side in ("positive", "negative"):
        request_path = Path(getattr(args, f"{side}_request"))
        verdict_path = Path(getattr(args, f"{side}_verdict"))
        request = validate_request(
            load_json(request_path, f"{side} request"),
            identity=identity,
            identity_sha256=identity_sha,
        )
        validate_verdict(
            load_json(verdict_path, f"{side} verdict"),
            request=request,
            request_sha256=sha256_file(request_path),
            identity_sha256=identity_sha,
        )


def verify_run(args: argparse.Namespace) -> None:
    identity_path = Path(args.build_identity)
    identity = validate_build_identity(
        load_json(identity_path, "build identity"),
        exe_path=Path(args.exe) if args.exe else None,
    )
    identity_sha = sha256_file(identity_path)
    request_path = Path(args.request)
    request = validate_request(
        load_json(request_path, "request"),
        identity=identity,
        identity_sha256=identity_sha,
    )
    validate_verdict(
        load_json(Path(args.verdict), "verdict"),
        request=request,
        request_sha256=sha256_file(request_path),
        identity_sha256=identity_sha,
    )


def validate_build(args: argparse.Namespace) -> None:
    validate_build_identity(
        load_json(Path(args.build_identity), "build identity"),
        exe_path=Path(args.exe) if args.exe else None,
    )


def validate_cleanup(args: argparse.Namespace) -> None:
    directory = Path(args.directory)
    identity_path = directory / "build-identity.json"
    instance_path = directory / "azure-instance-view.json"
    census_path = directory / "azure-subscription-census.json"
    receipt_path = directory / "azure-cleanup-receipt.json"
    instance = exact_object(
        load_json(instance_path, "Azure instance view"),
        {"schemaVersion", "capturedUtc", "subscriptionIdSha256", "vm"},
        "azureInstanceView",
    )
    census = exact_object(
        load_json(census_path, "Azure subscription census"),
        {"schemaVersion", "capturedUtc", "subscriptionIdSha256", "vms"},
        "azureSubscriptionCensus",
    )
    receipt = exact_object(
        load_json(receipt_path, "Azure cleanup receipt"),
        {
            "schemaVersion",
            "runId",
            "exeSha256",
            "buildIdentitySha256",
            "targetVm",
            "targetResourceGroup",
            "instanceViewFile",
            "instanceViewSha256",
            "censusFile",
            "censusSha256",
            "deallocated",
            "runningCount",
            "capturedUtc",
        },
        "azureCleanupReceipt",
    )
    identity = validate_build_identity(load_json(identity_path, "build identity"))
    for value, label in (
        (instance["schemaVersion"], "azureInstanceView"),
        (census["schemaVersion"], "azureSubscriptionCensus"),
        (receipt["schemaVersion"], "azureCleanupReceipt"),
    ):
        validate_schema_version(value, label)
    for payload, label in (
        (instance, "azureInstanceView"),
        (census, "azureSubscriptionCensus"),
        (receipt, "azureCleanupReceipt"),
    ):
        require_text(payload["capturedUtc"], f"{label}.capturedUtc", maximum=64)
    require_sha(
        instance["subscriptionIdSha256"],
        "azureInstanceView.subscriptionIdSha256",
    )
    require_sha(
        census["subscriptionIdSha256"],
        "azureSubscriptionCensus.subscriptionIdSha256",
    )
    if not isinstance(receipt["runId"], str) or not RUN_ID_RE.fullmatch(receipt["runId"]):
        raise ContractError("azureCleanupReceipt.runId is unsafe")
    vm = exact_object(
        instance["vm"],
        {
            "id",
            "name",
            "resourceGroup",
            "location",
            "powerState",
            "agentStatus",
            "provisioningState",
        },
        "azureInstanceView.vm",
    )
    if vm["powerState"] != "VM deallocated":
        raise ContractError("Azure instance view does not prove VM deallocated")
    for key in ("id", "name", "resourceGroup", "location", "agentStatus", "provisioningState"):
        require_text(vm[key], f"azureInstanceView.vm.{key}")
    if not isinstance(census["vms"], list):
        raise ContractError("Azure subscription census vms must be an array")
    running = 0
    for index, raw_vm in enumerate(census["vms"]):
        census_vm = exact_object(
            raw_vm,
            {"id", "name", "resourceGroup", "powerState"},
            f"azureSubscriptionCensus.vms[{index}]",
        )
        if census_vm["powerState"] == "VM running":
            running += 1
        for key in ("id", "name", "resourceGroup", "powerState"):
            require_text(census_vm[key], f"azureSubscriptionCensus.vms[{index}].{key}")
    if running != 0 or receipt["runningCount"] != 0:
        raise ContractError("Azure subscription census contains a running VM")
    if receipt["deallocated"] is not True:
        raise ContractError("Azure cleanup receipt does not assert deallocation")
    if receipt["targetVm"] != vm["name"] or receipt["targetResourceGroup"] != vm["resourceGroup"]:
        raise ContractError("Azure cleanup receipt target differs from instance view")
    if receipt["instanceViewFile"] != instance_path.name:
        raise ContractError("Azure cleanup instance filename is not exact")
    if receipt["censusFile"] != census_path.name:
        raise ContractError("Azure cleanup census filename is not exact")
    if receipt["instanceViewSha256"] != sha256_file(instance_path):
        raise ContractError("Azure instance-view digest mismatch")
    if receipt["censusSha256"] != sha256_file(census_path):
        raise ContractError("Azure subscription-census digest mismatch")
    require_sha(receipt["exeSha256"], "azureCleanupReceipt.exeSha256")
    require_sha(
        receipt["buildIdentitySha256"],
        "azureCleanupReceipt.buildIdentitySha256",
    )
    if receipt["buildIdentitySha256"] != sha256_file(identity_path):
        raise ContractError("Azure cleanup receipt build identity digest mismatch")
    if receipt["exeSha256"] != identity["artifacts"]["executable"]["sha256"]:
        raise ContractError("Azure cleanup receipt executable differs from build identity")
    if instance["subscriptionIdSha256"] != census["subscriptionIdSha256"]:
        raise ContractError("Azure subscription identity differs across retained JSON")


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    pair = subparsers.add_parser("verify-pair")
    pair.add_argument("--positive-request", required=True)
    pair.add_argument("--positive-verdict", required=True)
    pair.add_argument("--negative-request", required=True)
    pair.add_argument("--negative-verdict", required=True)
    pair.add_argument("--build-identity", required=True)
    pair.add_argument("--exe", required=True)
    pair.set_defaults(function=verify_pair)
    run = subparsers.add_parser("verify-run")
    run.add_argument("--request", required=True)
    run.add_argument("--verdict", required=True)
    run.add_argument("--build-identity", required=True)
    run.add_argument("--exe")
    run.set_defaults(function=verify_run)
    build = subparsers.add_parser("validate-build")
    build.add_argument("--build-identity", required=True)
    build.add_argument("--exe")
    build.set_defaults(function=validate_build)
    cleanup = subparsers.add_parser("verify-cleanup")
    cleanup.add_argument("--directory", required=True)
    cleanup.set_defaults(function=validate_cleanup)
    args = parser.parse_args()
    try:
        args.function(args)
    except ContractError as exc:
        print(f"VMQA CONTRACT INVALID: {exc}", file=sys.stderr)
        return 9
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
