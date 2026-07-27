#!/usr/bin/env python3
"""Strict, closed-schema validation for retained VMQA evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from vmqa_build_evidence import EvidenceError, verify_evidence


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


def require_int(
    value: Any, label: str, *, minimum: int | None = None
) -> int:
    if type(value) is not int or (minimum is not None and value < minimum):
        raise ContractError(f"{label} must be an integer with minimum {minimum}")
    return value


def require_bool(value: Any, label: str) -> bool:
    if type(value) is not bool:
        raise ContractError(f"{label} must be a boolean")
    return value


def require_timestamp(value: Any, label: str) -> datetime:
    require_text(value, label, maximum=64)
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ContractError(f"{label} must be an ISO-8601 timestamp") from exc
    if parsed.tzinfo is None:
        raise ContractError(f"{label} must include a UTC offset")
    return parsed.astimezone(timezone.utc)


def validate_schema_version(value: Any, label: str) -> None:
    if type(value) is not int or value != SCHEMA_VERSION:
        raise ContractError(
            f"{label}.schemaVersion must be exactly {SCHEMA_VERSION}, got {value!r}"
        )


def validate_build_identity(
    value: Any,
    *,
    exe_path: Path,
    evidence_dir: Path,
    expected_commit: str,
    expected_tree: str,
) -> dict[str, Any]:
    identity = exact_object(
        value,
        {"schemaVersion", "source", "ui", "build", "artifacts", "evidence"},
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
    if (
        not COMMIT_RE.fullmatch(expected_commit)
        or not COMMIT_RE.fullmatch(expected_tree)
    ):
        raise ContractError("independently expected source commit/tree is invalid")
    if source["commit"] != expected_commit or source["tree"] != expected_tree:
        raise ContractError("build identity differs from independently expected source")

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

    evidence = exact_object(
        identity["evidence"],
        {
            "sourceArchiveSha256",
            "distArchiveSha256",
            "distManifestSha256",
            "npmBuildLogSha256",
            "cargoBuildLogSha256",
            "buildLogSha256",
        },
        "buildIdentity.evidence",
    )
    for name, digest in evidence.items():
        require_sha(digest, f"buildIdentity.evidence.{name}")
    executable = artifacts["executable"]
    if not exe_path.is_file():
        raise ContractError(f"independent executable is missing: {exe_path}")
    if sha256_file(exe_path) != executable["sha256"]:
        raise ContractError("build identity executable digest differs from independent bytes")
    if exe_path.stat().st_size != executable["sizeBytes"]:
        raise ContractError("build identity executable size differs from independent bytes")
    try:
        verify_evidence(evidence_dir, exe_path, identity)
    except EvidenceError as exc:
        raise ContractError(str(exc)) from exc
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
    require_timestamp(request["runStartUtc"], "request.runStartUtc")
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
        require_text(verb, f"request.steps[{index}].verb", maximum=32)
        if verb not in ARG_KEYS:
            raise ContractError(f"request.steps[{index}].verb is unsupported")
        args = exact_object(
            step["args"], ARG_KEYS[verb], f"request.steps[{index}].args"
        )
        if "exeSha256" in args and args["exeSha256"] != exe_sha:
            raise ContractError(f"request.steps[{index}] executable differs from request")
        if "exeSha256" in args:
            require_sha(args["exeSha256"], f"request.steps[{index}].args.exeSha256")
        if verb == "launch":
            require_int(
                args["timeoutSeconds"],
                f"request.steps[{index}].args.timeoutSeconds",
                minimum=1,
            )
        elif verb == "shot":
            require_text(args["name"], f"request.steps[{index}].args.name", maximum=128)
            require_text(
                args["expectedSurfaceClass"],
                f"request.steps[{index}].args.expectedSurfaceClass",
                maximum=255,
            )
        elif verb == "click":
            require_int(args["winX"], f"request.steps[{index}].args.winX")
            require_int(args["winY"], f"request.steps[{index}].args.winY")
            require_int(
                args["settleMs"],
                f"request.steps[{index}].args.settleMs",
                minimum=0,
            )
        elif verb == "type":
            require_text(args["text"], f"request.steps[{index}].args.text")
            require_int(
                args["settleMs"],
                f"request.steps[{index}].args.settleMs",
                minimum=0,
            )
        elif verb == "key":
            require_text(args["key"], f"request.steps[{index}].args.key", maximum=64)
            require_int(
                args["settleMs"],
                f"request.steps[{index}].args.settleMs",
                minimum=0,
            )
        elif verb == "wait":
            require_int(args["ms"], f"request.steps[{index}].args.ms", minimum=0)
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
    request_start = require_timestamp(request["runStartUtc"], "request.runStartUtc")
    verdict_start = require_timestamp(verdict["runStartUtc"], "verdict.runStartUtc")
    agent_start = require_timestamp(verdict["agentStartedUtc"], "verdict.agentStartedUtc")
    finished = require_timestamp(verdict["finishedUtc"], "verdict.finishedUtc")
    if verdict_start != request_start:
        raise ContractError("verdict run start differs from request")
    if agent_start > finished or verdict_start > finished:
        raise ContractError("verdict timestamps are not coherent")
    for key in ("agentSha", "win32Sha"):
        require_sha(verdict[key], f"verdict.{key}")
    require_text(verdict["vmName"], "verdict.vmName", maximum=255)
    if not isinstance(verdict["diffKey"], str):
        raise ContractError("verdict.diffKey must be a string")
    if verdict["overall"] not in ("pass", "fail", "unmeasurable", "blocked"):
        raise ContractError("verdict.overall is unsupported")
    if not isinstance(verdict["steps"], list):
        raise ContractError("verdict.steps must be an array")
    if len(verdict["steps"]) != len(request["steps"]):
        if verdict["overall"] != "blocked" or verdict["steps"]:
            raise ContractError("verdict steps differ from requested step count")
    integer_facts = {
        "launchedPid",
        "markerWindowsTotal",
        "surfacePid",
        "surfaceHwnd",
        "surfaceWidth",
        "surfaceHeight",
        "captureDistinctColors",
        "rawSurfaceWidth",
        "rawSurfaceHeight",
        "surfaceDpi",
        "cleanupPid",
        "cleanupMatchingExeCount",
    }
    boolean_facts = {
        "launchProcessStarted",
        "boundsWithinVirtualDesktop",
        "coversVirtualDesktop",
        "normalizedForCapture",
        "foregroundPre",
        "foregroundPost",
        "sampleGridPre",
        "sampleGridPost",
        "unoccludedPre",
        "unoccludedPost",
        "rectStable",
        "cleanupPidAbsent",
        "cleanupExecutableAbsent",
    }
    string_facts = {
        "exeSha256",
        "surfaceClass",
        "postSurfaceClass",
        "boundsSource",
        "artifactPath",
        "pngSha256",
        "cleanupOutcome",
        "cleanupExeSha256",
    }
    for index, raw_result in enumerate(verdict["steps"]):
        result = exact_object(raw_result, STEP_RESULT_KEYS, f"verdict.steps[{index}]")
        verb = result["verb"]
        require_text(verb, f"verdict.steps[{index}].verb", maximum=32)
        if verb not in FACT_KEYS:
            raise ContractError(f"verdict.steps[{index}].verb is unsupported")
        require_text(result["status"], f"verdict.steps[{index}].status", maximum=32)
        if result["status"] not in ("pass", "fail", "unmeasurable", "blocked"):
            raise ContractError(f"verdict.steps[{index}].status is unsupported")
        require_text(result["id"], f"verdict.steps[{index}].id", maximum=96)
        if not isinstance(result["detail"], str):
            raise ContractError(f"verdict.steps[{index}].detail must be a string")
        if (
            not isinstance(result["artifacts"], list)
            or not all(isinstance(path, str) and path for path in result["artifacts"])
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
                rect = facts[rect_name]
                for key in RECT_KEYS:
                    require_int(
                        rect[key],
                        f"verdict.steps[{index}].facts.{rect_name}.{key}",
                    )
                if (
                    rect["width"] <= 0
                    or rect["height"] <= 0
                    or rect["right"] - rect["left"] != rect["width"]
                    or rect["bottom"] - rect["top"] != rect["height"]
                ):
                    raise ContractError(
                        f"verdict.steps[{index}].facts.{rect_name} is inconsistent"
                    )
        for name, value in facts.items():
            label = f"verdict.steps[{index}].facts.{name}"
            if name in integer_facts:
                require_int(value, label, minimum=0)
            elif name in boolean_facts:
                require_bool(value, label)
            elif name in string_facts:
                require_text(value, label)
        for name in ("exeSha256", "pngSha256", "cleanupExeSha256"):
            if name in facts:
                require_sha(facts[name], f"verdict.steps[{index}].facts.{name}")
        if index < len(request["steps"]):
            requested = request["steps"][index]
            if result["id"] != requested["id"] or result["verb"] != requested["verb"]:
                raise ContractError(
                    f"verdict.steps[{index}] does not answer the requested step"
                )
    if verdict["overall"] == "blocked":
        require_text(verdict["diagnosis"], "verdict.diagnosis")
    return verdict


def verify_pair(args: argparse.Namespace) -> None:
    identity_path = Path(args.build_identity)
    identity = validate_build_identity(
        load_json(identity_path, "build identity"),
        exe_path=Path(args.exe),
        evidence_dir=Path(args.evidence_dir),
        expected_commit=args.expected_commit,
        expected_tree=args.expected_tree,
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
        exe_path=Path(args.exe),
        evidence_dir=Path(args.evidence_dir),
        expected_commit=args.expected_commit,
        expected_tree=args.expected_tree,
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
        exe_path=Path(args.exe),
        evidence_dir=Path(args.evidence_dir),
        expected_commit=args.expected_commit,
        expected_tree=args.expected_tree,
    )


def validate_cleanup(args: argparse.Namespace) -> None:
    directory = Path(args.directory)
    identity_path = directory / "build-identity.json"
    request_path = directory / "request.json"
    verdict_path = directory / "verdict.json"
    raw_instance_path = directory / "azure-instance-view.raw.json"
    raw_census_path = directory / "azure-subscription-census.raw.json"
    raw_pages_path = directory / "azure-subscription-pages.raw.json"
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
            "requestFile",
            "requestSha256",
            "verdictFile",
            "verdictSha256",
            "rawInstanceViewFile",
            "rawInstanceViewSha256",
            "rawCensusFile",
            "rawCensusSha256",
            "rawSubscriptionPagesFile",
            "rawSubscriptionPagesSha256",
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
    identity = validate_build_identity(
        load_json(identity_path, "build identity"),
        exe_path=Path(args.exe),
        evidence_dir=directory / "build-evidence",
        expected_commit=args.expected_commit,
        expected_tree=args.expected_tree,
    )
    identity_sha = sha256_file(identity_path)
    request = validate_request(
        load_json(request_path, "request"),
        identity=identity,
        identity_sha256=identity_sha,
    )
    verdict = validate_verdict(
        load_json(verdict_path, "verdict"),
        request=request,
        request_sha256=sha256_file(request_path),
        identity_sha256=identity_sha,
    )
    raw_instance = load_json(raw_instance_path, "raw Azure instance view")
    raw_census = load_json(raw_census_path, "raw Azure subscription census")
    raw_pages = load_json(raw_pages_path, "raw Azure subscription pages")
    if not isinstance(raw_instance, dict):
        raise ContractError("raw Azure instance view must be an object")
    if not isinstance(raw_census, list):
        raise ContractError("raw Azure subscription census must be an array")
    if not isinstance(raw_pages, list) or not raw_pages:
        raise ContractError("raw Azure subscription pages must be a nonempty array")
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
            "powerStateCode",
            "agentStatus",
            "provisioningState",
        },
        "azureInstanceView.vm",
    )
    if (
        vm["powerStateCode"] != "PowerState/deallocated"
        or vm["powerState"] != "VM deallocated"
    ):
        raise ContractError("Azure instance view does not authoritatively prove VM deallocated")
    for key in (
        "id",
        "name",
        "resourceGroup",
        "location",
        "powerStateCode",
        "agentStatus",
        "provisioningState",
    ):
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
    if type(receipt["runningCount"]) is not int:
        raise ContractError("azureCleanupReceipt.runningCount must be an integer")
    if receipt["deallocated"] is not True:
        raise ContractError("Azure cleanup receipt does not assert deallocation")
    if receipt["targetVm"] != vm["name"] or receipt["targetResourceGroup"] != vm["resourceGroup"]:
        raise ContractError("Azure cleanup receipt target differs from instance view")
    if verdict["vmName"] != vm["name"]:
        raise ContractError("Azure cleanup target differs from retained verdict VM")
    if receipt["runId"] != request["runId"] or verdict["runId"] != request["runId"]:
        raise ContractError("Azure cleanup receipt run differs from retained request/verdict")
    if directory.name != request["runId"]:
        raise ContractError("Azure cleanup directory differs from retained run ID")
    if receipt["requestFile"] != request_path.name:
        raise ContractError("Azure cleanup request filename is not exact")
    if receipt["verdictFile"] != verdict_path.name:
        raise ContractError("Azure cleanup verdict filename is not exact")
    if receipt["requestSha256"] != sha256_file(request_path):
        raise ContractError("Azure cleanup request digest mismatch")
    if receipt["verdictSha256"] != sha256_file(verdict_path):
        raise ContractError("Azure cleanup verdict digest mismatch")
    if receipt["rawInstanceViewFile"] != raw_instance_path.name:
        raise ContractError("Azure cleanup raw instance-view filename is not exact")
    if receipt["rawCensusFile"] != raw_census_path.name:
        raise ContractError("Azure cleanup raw census filename is not exact")
    if receipt["rawSubscriptionPagesFile"] != raw_pages_path.name:
        raise ContractError("Azure cleanup raw subscription-pages filename is not exact")
    if receipt["rawInstanceViewSha256"] != sha256_file(raw_instance_path):
        raise ContractError("Azure raw instance-view digest mismatch")
    if receipt["rawCensusSha256"] != sha256_file(raw_census_path):
        raise ContractError("Azure raw census digest mismatch")
    if receipt["rawSubscriptionPagesSha256"] != sha256_file(raw_pages_path):
        raise ContractError("Azure raw subscription-pages digest mismatch")
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
    if request["exeSha256"] != receipt["exeSha256"] or verdict["requestExeSha256"] != receipt["exeSha256"]:
        raise ContractError("Azure cleanup executable differs from retained request/verdict")
    if instance["subscriptionIdSha256"] != census["subscriptionIdSha256"]:
        raise ContractError("Azure subscription identity differs across retained JSON")
    if not (
        receipt["capturedUtc"] == instance["capturedUtc"] == census["capturedUtc"]
    ):
        raise ContractError("Azure cleanup capture timestamps differ")
    captured = require_timestamp(receipt["capturedUtc"], "azureCleanupReceipt.capturedUtc")
    finished = require_timestamp(verdict["finishedUtc"], "verdict.finishedUtc")
    if captured < finished:
        raise ContractError("Azure cleanup predates the retained verdict")

    raw_instance_view = raw_instance.get("instanceView")
    if not isinstance(raw_instance_view, dict):
        raise ContractError("raw Azure instanceView must be an object")
    raw_statuses = raw_instance_view.get("statuses")
    raw_agent = raw_instance_view.get("vmAgent")
    if not isinstance(raw_statuses, list) or not isinstance(raw_agent, dict):
        raise ContractError("raw Azure instanceView statuses/vmAgent types are invalid")
    raw_agent_statuses = raw_agent.get("statuses")
    if not isinstance(raw_agent_statuses, list):
        raise ContractError("raw Azure vmAgent.statuses must be an array")
    raw_power = next(
        (
            status.get("displayStatus")
            for status in raw_statuses
            if isinstance(status, dict)
            and isinstance(status.get("code"), str)
            and status["code"].startswith("PowerState/")
        ),
        "unknown",
    )
    raw_power_code = next(
        (
            status.get("code")
            for status in raw_statuses
            if isinstance(status, dict)
            and isinstance(status.get("code"), str)
            and status["code"].startswith("PowerState/")
        ),
        "unknown",
    )
    raw_provisioning = next(
        (
            status.get("displayStatus")
            for status in raw_statuses
            if isinstance(status, dict)
            and isinstance(status.get("code"), str)
            and status["code"].startswith("ProvisioningState/")
        ),
        "unknown",
    )
    raw_agent = (
        raw_agent_statuses[0].get("displayStatus")
        if raw_agent_statuses and isinstance(raw_agent_statuses[0], dict)
        else "unknown"
    )
    expected_vm = {
        "id": raw_instance.get("id"),
        "name": raw_instance.get("name"),
        "resourceGroup": raw_instance.get("resourceGroup"),
        "location": raw_instance.get("location"),
        "powerState": raw_power,
        "powerStateCode": raw_power_code,
        "agentStatus": raw_agent,
        "provisioningState": raw_provisioning,
    }
    if vm != expected_vm:
        raise ContractError("Azure instance projection differs from retained raw response")
    expected_census = sorted(
        (
            {
                "id": raw_vm.get("id"),
                "name": raw_vm.get("name"),
                "resourceGroup": raw_vm.get("resourceGroup"),
                "powerState": raw_vm.get("powerState", "unknown"),
            }
            for raw_vm in raw_census
            if isinstance(raw_vm, dict)
        ),
        key=lambda item: str(item["id"]),
    )
    if census["vms"] != expected_census:
        raise ContractError("Azure census projection differs from retained raw response")
    target_members = [
        candidate
        for candidate in census["vms"]
        if candidate["id"] == vm["id"]
        and candidate["name"] == vm["name"]
        and candidate["resourceGroup"] == vm["resourceGroup"]
    ]
    if len(target_members) != 1:
        raise ContractError("target VM is not present exactly once in subscription census")
    target_id = vm["id"]
    target_parts = target_id.split("/")
    if (
        len(target_parts) < 3
        or target_parts[1].lower() != "subscriptions"
        or not target_parts[2]
    ):
        raise ContractError("target VM ID does not expose an Azure subscription identity")
    paged_ids: list[str] = []
    previous_next: str | None = None
    for index, raw_page in enumerate(raw_pages):
        page = exact_object(
            raw_page, {"requestUrl", "response"}, f"azureSubscriptionPages[{index}]"
        )
        request_url = require_text(
            page["requestUrl"],
            f"azureSubscriptionPages[{index}].requestUrl",
            maximum=8192,
        )
        response = page["response"]
        if not isinstance(response, dict) or not isinstance(response.get("value"), list):
            raise ContractError(
                f"azureSubscriptionPages[{index}].response must contain a value array"
            )
        if index == 0:
            if (
                f"/subscriptions/{target_parts[2]}/providers/Microsoft.Compute/"
                "virtualMachines"
            ).lower() not in request_url.lower():
                raise ContractError("Azure subscription pagination starts at the wrong scope")
        elif request_url != previous_next:
            raise ContractError("Azure subscription pagination did not follow nextLink")
        for item_index, item in enumerate(response["value"]):
            if not isinstance(item, dict):
                raise ContractError(
                    f"azureSubscriptionPages[{index}].response.value[{item_index}] "
                    "must be an object"
                )
            paged_ids.append(
                require_text(
                    item.get("id"),
                    f"azureSubscriptionPages[{index}].response.value[{item_index}].id",
                )
            )
        next_link = response.get("nextLink")
        if index < len(raw_pages) - 1:
            previous_next = require_text(
                next_link, f"azureSubscriptionPages[{index}].response.nextLink", maximum=8192
            )
        elif next_link not in (None, ""):
            raise ContractError("Azure subscription pagination is incomplete")
    census_ids = [candidate["id"] for candidate in census["vms"]]
    if sorted(paged_ids) != sorted(census_ids) or len(set(paged_ids)) != len(paged_ids):
        raise ContractError(
            "Azure detailed census differs from the complete paginated subscription census"
        )
    raw_subscription_sha = hashlib.sha256(
        target_parts[2].lower().encode("utf-8")
    ).hexdigest()
    if instance["subscriptionIdSha256"] != raw_subscription_sha:
        raise ContractError("Azure subscription digest differs from retained raw target ID")


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
    pair.add_argument("--evidence-dir", required=True)
    pair.add_argument("--expected-commit", required=True)
    pair.add_argument("--expected-tree", required=True)
    pair.set_defaults(function=verify_pair)
    run = subparsers.add_parser("verify-run")
    run.add_argument("--request", required=True)
    run.add_argument("--verdict", required=True)
    run.add_argument("--build-identity", required=True)
    run.add_argument("--exe", required=True)
    run.add_argument("--evidence-dir", required=True)
    run.add_argument("--expected-commit", required=True)
    run.add_argument("--expected-tree", required=True)
    run.set_defaults(function=verify_run)
    build = subparsers.add_parser("validate-build")
    build.add_argument("--build-identity", required=True)
    build.add_argument("--exe", required=True)
    build.add_argument("--evidence-dir", required=True)
    build.add_argument("--expected-commit", required=True)
    build.add_argument("--expected-tree", required=True)
    build.set_defaults(function=validate_build)
    cleanup = subparsers.add_parser("verify-cleanup")
    cleanup.add_argument("--directory", required=True)
    cleanup.add_argument("--exe", required=True)
    cleanup.add_argument("--expected-commit", required=True)
    cleanup.add_argument("--expected-tree", required=True)
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
