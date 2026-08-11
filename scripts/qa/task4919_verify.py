#!/usr/bin/env python3
"""Fail-closed verifier for the TASK 4919 Windows Tor lifecycle receipt.

This verifier deliberately does not collect evidence and does not infer a pass from
source code.  It consumes the release-machine bundle produced by independent Windows
kernel, packet, process/image-load, UIA, endpoint, and Tor-circuit collectors.  Unit
tests may exercise the schema with ``require_production=False``; only the CLI uses the
production path and only that path can print TOR-4919-GREEN.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


EXACT_LAN_COPY = (
    "OSL LAN cannot use Tor, so it is off while Tor is selected. "
    "Turn off Tor to use OSL LAN."
)

LIFECYCLE_STATES = (
    "before_launch",
    "fresh_onboarding",
    "ordinary_use",
    "idle_24h",
    "forced_delayed_background_jobs",
    "sleep_wake",
    "network_interface_change",
    "address_change",
    "sidecar_death",
    "sidecar_recovery",
    "app_restart",
    "os_restart",
    "packaged_component_update",
)

ACTIVE_STATES = LIFECYCLE_STATES[1:]

NON_LAN_CLASSES = (
    "socket",
    "dns",
    "http",
    "updater",
    "telemetry",
    "crash",
    "keyserver_directory",
    "message",
    "attachment",
    "polling",
)

LAN_OPERATIONS = (
    ("tcp", "listener"),
    ("tcp", "discovery"),
    ("tcp", "connection"),
    ("udp", "listener"),
    ("udp", "discovery"),
    ("udp", "connection"),
)

REQUIRED_COLLECTORS = {
    "windows_kernel_socket",
    "windows_wfp_packet",
    "windows_kernel_dns",
    "windows_kernel_process",
    "windows_image_load",
    "windows_uia",
    "independent_endpoint",
    "public_tor_circuit",
    "shipping_binary_inventory",
}

HEX64 = re.compile(r"^[0-9a-f]{64}$")


class VerificationError(Exception):
    pass


def fail(message: str) -> None:
    raise VerificationError(message)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        fail(f"cannot read {path.name}: {exc}")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    try:
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if not line.strip():
                fail(f"events.jsonl line {number} is empty")
            value = json.loads(line)
            if not isinstance(value, dict):
                fail(f"events.jsonl line {number} is not an object")
            events.append(value)
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        fail(f"cannot read events.jsonl: {exc}")
    if not events:
        fail("event stream is empty")
    return events


def parse_time(value: Any, label: str) -> datetime:
    if not isinstance(value, str) or not value.endswith("Z"):
        fail(f"{label} must be an RFC3339 UTC timestamp")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        fail(f"{label} is not a valid timestamp")
    if parsed.tzinfo != timezone.utc:
        fail(f"{label} is not UTC")
    return parsed


def require_hex(value: Any, label: str) -> str:
    if not isinstance(value, str) or not HEX64.fullmatch(value):
        fail(f"{label} must be a lowercase SHA-256")
    return value


def safe_artifact(bundle: Path, relative: Any, expected_hash: Any, label: str) -> Path:
    if not isinstance(relative, str) or not relative:
        fail(f"{label} artifact path is missing")
    candidate = (bundle / relative).resolve()
    try:
        candidate.relative_to(bundle.resolve())
    except ValueError:
        fail(f"{label} artifact escapes the receipt bundle")
    if not candidate.is_file():
        fail(f"{label} artifact is missing: {relative}")
    expected = require_hex(expected_hash, f"{label} artifact hash")
    actual = sha256_file(candidate)
    if actual != expected:
        fail(f"{label} artifact hash mismatch: expected {expected}, got {actual}")
    return candidate


def one(events: list[dict[str, Any]], event_type: str, *, label: str, **fields: Any) -> dict[str, Any]:
    matches = [
        event
        for event in events
        if event.get("type") == event_type
        and all(event.get(key) == value for key, value in fields.items())
    ]
    if len(matches) != 1:
        fail(f"{label}: expected exactly 1 {event_type}, found {len(matches)}")
    return matches[0]


def verify_bundle(bundle: Path, *, require_production: bool = True) -> dict[str, Any]:
    bundle = bundle.resolve()
    manifest_path = bundle / "manifest.json"
    manifest = read_json(manifest_path)
    if not isinstance(manifest, dict):
        fail("manifest.json is not an object")
    if manifest.get("schema") != "osl-task-4919-receipt/v1":
        fail("manifest schema is not osl-task-4919-receipt/v1")

    if require_production:
        if manifest.get("receiptKind") != "windows-release-capture/v1":
            fail("production receiptKind is missing")
        if manifest.get("fixture") is not False:
            fail("fixture receipt cannot authorize TOR-4919-GREEN")
    elif manifest.get("receiptKind") not in {
        "windows-release-capture/v1",
        "unit-fixture/v1",
    }:
        fail("unit receiptKind is invalid")

    platform = manifest.get("platform", {})
    if platform.get("os") != "Windows" or platform.get("kernel") != "NT":
        fail("capture was not produced by Windows NT")
    if not platform.get("machineId"):
        fail("Windows machine identity is missing")

    package = manifest.get("package", {})
    package_hash = require_hex(package.get("sha256"), "package sha256")
    if package.get("releasePackaged") is not True or package.get("immutable") is not True:
        fail("build was not one immutable release-packaged build")
    if package.get("format") not in {"msi", "msix", "nsis"}:
        fail("release package format is not MSI, MSIX, or NSIS")
    if not package.get("windowsAuthenticodeVerified"):
        fail("release package Authenticode verification is absent")
    package_path = safe_artifact(
        bundle, package.get("artifact"), package_hash, "release package"
    )
    expected_extension = {"msi": ".msi", "msix": ".msix", "nsis": ".exe"}[
        package["format"]
    ]
    if package_path.suffix.lower() != expected_extension:
        fail("release package extension disagrees with package format")

    start = parse_time(manifest.get("captureStartUtc"), "captureStartUtc")
    end = parse_time(manifest.get("captureEndUtc"), "captureEndUtc")
    if (end - start).total_seconds() < 86_400:
        fail("capture stopped before 24 hours elapsed")
    observation = manifest.get("observation", {})
    if observation.get("continuous") is not True or observation.get("gaps") != []:
        fail("kernel observation was not continuous or contains gaps")
    if observation.get("startedBeforeLaunch") is not True:
        fail("observation did not start before app launch")
    if observation.get("stoppedAfterLastDelayedEvent") is not True:
        fail("observation stopped before the last delayed event")

    collector_rows = manifest.get("collectors")
    if not isinstance(collector_rows, list):
        fail("collector inventory is missing")
    collector_ids: set[str] = set()
    collector_artifacts: set[str] = set()
    for row in collector_rows:
        if not isinstance(row, dict) or not isinstance(row.get("id"), str):
            fail("collector row is malformed")
        collector_id = row["id"]
        if collector_id in collector_ids:
            fail(f"duplicate collector: {collector_id}")
        collector_ids.add(collector_id)
        if row.get("artifact") in collector_artifacts:
            fail(f"collectors share one purportedly independent artifact: {row.get('artifact')}")
        collector_artifacts.add(row.get("artifact"))
        if row.get("independentOfApp") is not True or row.get("continuous") is not True:
            fail(f"collector {collector_id} was app-trusted or discontinuous")
        safe_artifact(bundle, row.get("artifact"), row.get("sha256"), collector_id)
    missing_collectors = REQUIRED_COLLECTORS - collector_ids
    if missing_collectors:
        fail(f"missing kernel observer/collector: {', '.join(sorted(missing_collectors))}")

    inventory_meta = manifest.get("inventory", {})
    inventory_path = safe_artifact(
        bundle,
        inventory_meta.get("artifact"),
        inventory_meta.get("sha256"),
        "shipping inventory",
    )
    if inventory_meta.get("generator") != "independent-windows-pe-import-symbol-runtime-scan/v1":
        fail("shipping inventory was not independently generated")
    if inventory_meta.get("handEdited") is not False:
        fail("shipping inventory permits hand omission")
    inventory = read_json(inventory_path)
    if not isinstance(inventory, list) or not inventory:
        fail("shipping caller inventory is empty")
    if inventory_meta.get("shippingCallerCount") != len(inventory):
        fail("shipping caller inventory cardinality mismatch")

    events_meta = manifest.get("events", {})
    events_path = safe_artifact(
        bundle, events_meta.get("artifact"), events_meta.get("sha256"), "events"
    )
    events = read_jsonl(events_path)
    if events_meta.get("count") != len(events):
        fail("event stream cardinality mismatch")

    previous_sequence = -1
    previous_time: datetime | None = None
    for index, event in enumerate(events):
        sequence = event.get("sequence")
        if not isinstance(sequence, int) or sequence != previous_sequence + 1:
            fail(f"event sequence gap at index {index}")
        previous_sequence = sequence
        timestamp = parse_time(event.get("atUtc"), f"event {sequence} atUtc")
        if timestamp < start or timestamp > end:
            fail(f"event {sequence} is outside capture bounds")
        if previous_time is not None and timestamp < previous_time:
            fail(f"event {sequence} is out of timestamp order")
        previous_time = timestamp

    # A single app assertion cannot stand in for continuous observation.  The
    # combined independent collectors must heartbeat at least once per minute.
    heartbeats = [event for event in events if event.get("type") == "observer_heartbeat"]
    if not heartbeats:
        fail("kernel observer heartbeat stream is missing")
    heartbeat_times = [parse_time(event["atUtc"], "observer heartbeat") for event in heartbeats]
    coverage_times = [start, *heartbeat_times, end]
    for left, right in zip(coverage_times, coverage_times[1:]):
        if (right - left).total_seconds() > 60:
            fail("kernel observer heartbeat gap exceeded 60 seconds")
    for heartbeat in heartbeats:
        if set(heartbeat.get("collectors", [])) != REQUIRED_COLLECTORS:
            fail("observer heartbeat starved a required kernel collector")

    state_ranges: dict[str, tuple[datetime, datetime]] = {}
    last_end = start
    for state in LIFECYCLE_STATES:
        begin_event = one(events, "lifecycle", label=f"{state} begin", state=state, phase="begin")
        end_event = one(events, "lifecycle", label=f"{state} end", state=state, phase="end")
        begin = parse_time(begin_event["atUtc"], f"{state} begin")
        finish = parse_time(end_event["atUtc"], f"{state} end")
        if begin < last_end or finish <= begin:
            fail(f"lifecycle state is missing, empty, or out of order: {state}")
        state_ranges[state] = (begin, finish)
        last_end = finish
    idle_begin, idle_end = state_ranges["idle_24h"]
    if (idle_end - idle_begin).total_seconds() < 86_400:
        fail("idle_24h state did not remain idle for 24 hours")

    # Explicit transitions must be independently observed, rather than merely
    # named as state labels.
    one(events, "power_transition", label="sleep", transition="sleep")
    one(events, "power_transition", label="wake", transition="wake")
    one(events, "network_transition", label="interface change", transition="interface")
    one(events, "network_transition", label="address change", transition="address")
    sidecar_death = one(events, "sidecar_transition", label="sidecar death", transition="death")
    sidecar_recovery = one(
        events, "sidecar_transition", label="sidecar recovery", transition="recovery"
    )
    one(events, "process_transition", label="app restart", transition="app_restart")
    one(events, "process_transition", label="OS restart", transition="os_restart")
    update = one(events, "component_update", label="packaged component update")
    if update.get("packageSha256") != package_hash:
        fail("component update changed the immutable package identity")
    if require_hex(update.get("oldComponentSha256"), "old component hash") == require_hex(
        update.get("newComponentSha256"), "new component hash"
    ):
        fail("packaged component update did not change a component")
    delayed = one(events, "delayed_job", label="forced delayed/background job", forced=True)
    if parse_time(delayed["atUtc"], "last delayed event") >= end:
        fail("observation did not continue after the last delayed event")

    caller_ids: set[str] = set()
    inventory_classes: set[str] = set()
    inventory_lan: set[tuple[str, str]] = set()
    inventory_images: dict[str, str] = {}
    inventory_class_by_caller: dict[str, str] = {}
    inventory_lan_by_caller: dict[str, tuple[str, str]] = {}
    for row in inventory:
        if not isinstance(row, dict):
            fail("shipping inventory row is malformed")
        caller_id = row.get("callerId")
        if not isinstance(caller_id, str) or not caller_id or caller_id in caller_ids:
            fail(f"shipping inventory has missing/duplicate caller: {caller_id!r}")
        caller_ids.add(caller_id)
        image_hash = require_hex(row.get("imageSha256"), f"{caller_id} image hash")
        inventory_images[caller_id] = image_hash
        if row.get("ownedByOsl") is not True or not row.get("process"):
            fail(f"inventory caller is not tied to an OSL-owned process: {caller_id}")
        caller_class = row.get("class")
        if caller_class in NON_LAN_CLASSES:
            inventory_classes.add(caller_class)
            inventory_class_by_caller[caller_id] = caller_class
        elif caller_class == "osl_lan":
            pair = (row.get("protocol"), row.get("operation"))
            if pair not in LAN_OPERATIONS:
                fail(f"unclassified OSL LAN caller: {caller_id}")
            inventory_lan.add(pair)  # type: ignore[arg-type]
            inventory_lan_by_caller[caller_id] = pair  # type: ignore[assignment]
        else:
            fail(f"unclassified shipping caller: {caller_id}")
    missing_classes = set(NON_LAN_CLASSES) - inventory_classes
    if missing_classes:
        fail(f"shipping inventory starved class: {', '.join(sorted(missing_classes))}")
    missing_lan_inventory = set(LAN_OPERATIONS) - inventory_lan
    if missing_lan_inventory:
        fail(f"shipping inventory starved OSL LAN caller: {sorted(missing_lan_inventory)}")

    discovered = [event for event in events if event.get("type") == "kernel_caller_discovered"]
    discovered_ids = {event.get("callerId") for event in discovered}
    if discovered_ids != caller_ids or len(discovered) != len(caller_ids):
        missing = sorted(caller_ids - discovered_ids)
        extra = sorted(str(item) for item in discovered_ids - caller_ids)
        fail(f"kernel caller reconciliation failed; missing={missing}, unclassified={extra}")
    loaded = [event for event in events if event.get("type") == "image_load"]
    loaded_ids = {event.get("callerId") for event in loaded}
    if loaded_ids != caller_ids:
        fail(f"newly loaded caller reconciliation failed: {sorted(caller_ids - loaded_ids)}")
    for event in discovered + loaded:
        caller_id = event.get("callerId")
        if event.get("imageSha256") != inventory_images.get(caller_id):
            fail(f"runtime image hash disagrees with inventory: {caller_id}")

    subject_rows = manifest.get("ownedSubjects")
    if not isinstance(subject_rows, list) or not subject_rows:
        fail("OSL-owned process/child/service/job census is absent")
    subject_ids = {row.get("subjectId") for row in subject_rows if isinstance(row, dict)}
    if None in subject_ids or len(subject_ids) != len(subject_rows):
        fail("OSL-owned subject census has missing or duplicate identities")
    subject_kinds = {row.get("kind") for row in subject_rows if isinstance(row, dict)}
    if subject_kinds != {"process", "child", "service", "scheduled_job"}:
        fail("process/child/service/scheduled-job census is incomplete")
    process_seen = {
        event.get("subjectId")
        for event in events
        if event.get("type") == "kernel_process_discovered"
    }
    if process_seen != subject_ids:
        fail(f"kernel process observer starved subject: {sorted(subject_ids - process_seen)}")

    auth_binding = require_hex(manifest.get("launchAuthBindingSha256"), "launch auth binding")
    sidecars = {
        event.get("sidecarId"): event
        for event in events
        if event.get("type") == "sidecar_launch"
        and event.get("authBindingSha256") == auth_binding
        and event.get("ownedByOsl") is True
        and event.get("packageSha256") == package_hash
    }
    if len(sidecars) < 2:
        fail("sidecar death/recovery did not launch a new authenticated owned sidecar")
    if (
        sidecar_death.get("sidecarId") not in sidecars
        or sidecar_recovery.get("sidecarId") not in sidecars
        or sidecar_death.get("sidecarId") == sidecar_recovery.get("sidecarId")
    ):
        fail("sidecar death/recovery identities are missing, unowned, or reused")
    circuits = {
        event.get("circuitId"): event
        for event in events
        if event.get("type") == "tor_circuit"
        and event.get("verifiedPublicTor") is True
        and event.get("independent") is True
        and isinstance(event.get("relays"), list)
        and len(event["relays"]) >= 3
        and event.get("sidecarId") in sidecars
    }
    if not circuits:
        fail("verified public Tor circuit evidence is absent")

    endpoints = {
        event.get("attemptId"): event
        for event in events
        if event.get("type") == "independent_endpoint"
    }
    endpoint_rows = [event for event in events if event.get("type") == "independent_endpoint"]
    endpoint_counts = Counter(event.get("attemptId") for event in endpoint_rows)
    duplicate_endpoints = sorted(
        str(attempt_id) for attempt_id, count in endpoint_counts.items() if count != 1
    )
    if None in endpoint_counts or duplicate_endpoints:
        fail(f"independent endpoint reconciliation is not one-to-one: {duplicate_endpoints[:3]}")
    positives = [event for event in events if event.get("type") == "positive_attempt"]
    positive_ids = [event.get("attemptId") for event in positives]
    if None in positive_ids or len(set(positive_ids)) != len(positive_ids):
        fail("positive attempt identities are missing or duplicated")
    positives_by_state_class = Counter((event.get("state"), event.get("class")) for event in positives)
    for state in ACTIVE_STATES:
        route = one(events, "route_snapshot", label=f"{state} route", state=state)
        if route.get("torSelected") is not True or route.get("oslLanEnabled") is not False:
            fail(f"persisted controls show Tor and OSL LAN together or Tor absent: {state}")
        for caller_class in NON_LAN_CLASSES:
            count = positives_by_state_class[(state, caller_class)]
            if count < 1:
                fail(f"missing positive attempt: state={state} class={caller_class}")
    for event in positives:
        state = event.get("state")
        caller_class = event.get("class")
        caller_id = event.get("callerId")
        if state not in ACTIVE_STATES or caller_class not in NON_LAN_CLASSES:
            fail(f"unclassified positive attempt: {event.get('attemptId')}")
        if caller_id not in caller_ids:
            fail(f"positive attempt used hand-omitted caller: {caller_id}")
        if inventory_class_by_caller.get(caller_id) != caller_class:
            fail(f"positive attempt class disagrees with inventory: {event.get('attemptId')}")
        if event.get("packageSha256") != package_hash:
            fail(f"positive attempt changed build: {event.get('attemptId')}")
        if event.get("authBindingSha256") != auth_binding or event.get("sidecarId") not in sidecars:
            fail(f"positive attempt bypassed launch-bound authenticated sidecar: {event.get('attemptId')}")
        if event.get("circuitId") not in circuits:
            fail(f"positive attempt lacks verified public Tor circuit: {event.get('attemptId')}")
        if circuits[event["circuitId"]].get("sidecarId") != event.get("sidecarId"):
            fail(f"positive attempt circuit belongs to another sidecar: {event.get('attemptId')}")
        request_hash = require_hex(event.get("requestSha256"), "positive request hash")
        response_hash = require_hex(event.get("responseSha256"), "positive response hash")
        endpoint = endpoints.get(event.get("attemptId"))
        if not endpoint or endpoint.get("independent") is not True:
            fail(f"positive attempt lacks independent endpoint: {event.get('attemptId')}")
        if endpoint.get("requestSha256") != request_hash or endpoint.get("responseSha256") != response_hash:
            fail(f"request/response hash mismatch: {event.get('attemptId')}")
        if endpoint.get("publicTorExitObserved") is not True:
            fail(f"endpoint did not observe a public Tor exit: {event.get('attemptId')}")
    used_positive_callers = {event.get("callerId") for event in positives}
    unused_positive_callers = set(inventory_class_by_caller) - used_positive_callers
    if unused_positive_callers:
        fail(f"shipping non-LAN caller was never attempted: {sorted(unused_positive_callers)}")

    lan_attempts = [event for event in events if event.get("type") == "lan_attempt"]
    lan_ids = [event.get("attemptId") for event in lan_attempts]
    if None in lan_ids or len(set(lan_ids)) != len(lan_ids) or set(lan_ids) & set(positive_ids):
        fail("OSL LAN attempt identities are missing, duplicated, or collide")
    lan_counts = Counter(
        (event.get("state"), event.get("protocol"), event.get("operation"))
        for event in lan_attempts
    )
    for state in ACTIVE_STATES:
        for protocol, operation in LAN_OPERATIONS:
            if lan_counts[(state, protocol, operation)] < 1:
                fail(f"missing OSL LAN attempt: state={state} {protocol}/{operation}")
    for event in lan_attempts:
        label = f"{event.get('state')} {event.get('protocol')}/{event.get('operation')}"
        caller_id = event.get("callerId")
        if inventory_lan_by_caller.get(caller_id) != (
            event.get("protocol"),
            event.get("operation"),
        ):
            fail(f"OSL LAN attempt used hand-omitted or mismatched caller: {label}")
        if event.get("refused") is not True:
            fail(f"OSL LAN attempt was not refused: {label}")
        if event.get("packetCount") != 0 or event.get("bytes") != 0:
            fail(f"escaped LAN byte: {label}")
        if event.get("copy") != EXACT_LAN_COPY:
            fail(f"OSL LAN refusal copy mismatch: {label}")
        uia = one(events, "uia_text", label=f"UIA refusal {event.get('attemptId')}", attemptId=event.get("attemptId"))
        if uia.get("text") != EXACT_LAN_COPY or not isinstance(uia.get("top"), (int, float)) or uia["top"] <= 0:
            fail(f"OSL LAN refusal is not exact above-0 UIA text: {label}")
    used_lan_callers = {event.get("callerId") for event in lan_attempts}
    unused_lan_callers = set(inventory_lan_by_caller) - used_lan_callers
    if unused_lan_callers:
        fail(f"shipping OSL LAN caller was never attempted: {sorted(unused_lan_callers)}")
    choice_uia = one(events, "uia_text", label="route/LAN choice UIA", location="route_lan_choice")
    if choice_uia.get("text") != EXACT_LAN_COPY or choice_uia.get("top", 0) <= 0:
        fail("exact 4916 explanation is not visible above 0 at route/LAN choice")

    # Reconcile the app-level attempts against independent kernel observations.
    kernel_events = [event for event in events if event.get("type") == "kernel_network"]
    before_begin, before_end = state_ranges["before_launch"]
    for event in kernel_events:
        event_time = parse_time(event.get("atUtc"), "kernel event time")
        if before_begin <= event_time <= before_end and event.get("owner") in {"app", "sidecar"}:
            fail(f"OSL network activity occurred before launch: {event.get('flowId')}")
    kernel_attempt_ids = {event.get("attemptId") for event in kernel_events if event.get("attemptId")}
    expected_attempt_ids = {event.get("attemptId") for event in positives + lan_attempts}
    missing_kernel = expected_attempt_ids - kernel_attempt_ids
    if missing_kernel:
        fail(f"kernel observer starved attempted byte: {sorted(missing_kernel)[:3]}")
    kernel_counts = Counter(
        event.get("attemptId") for event in kernel_events if event.get("attemptId")
    )
    duplicated_kernel = sorted(
        str(attempt_id)
        for attempt_id, count in kernel_counts.items()
        if attempt_id in expected_attempt_ids and count != 1
    )
    if duplicated_kernel:
        fail(f"kernel attempt reconciliation is not one-to-one: {duplicated_kernel[:3]}")
    extra_kernel_attempts = kernel_attempt_ids - expected_attempt_ids
    if extra_kernel_attempts:
        fail(f"kernel observer found unclassified attempted caller: {sorted(extra_kernel_attempts)[:3]}")
    for event in kernel_events:
        owner = event.get("owner")
        outside_loopback = event.get("outsideLoopback")
        byte_count = event.get("bytes")
        if not isinstance(byte_count, int) or byte_count < 0:
            fail("kernel event has invalid byte count")
        if event.get("classification") in {None, "unclassified"}:
            fail(f"unclassified socket: {event.get('flowId')}")
        if event.get("protocol") == "dns" and outside_loopback and byte_count > 0:
            fail(f"direct DNS byte escaped Tor: {event.get('flowId')}")
        if outside_loopback:
            if owner != "sidecar" or event.get("classification") not in {
                "tor_authority",
                "tor_relay",
                "tor_bridge",
            }:
                fail(f"direct endpoint/LAN/unclassified byte escaped Tor: {event.get('flowId')}")
        if owner == "app" and event.get("classification") == "owned_sidecar" and not event.get("authenticated"):
            fail(f"app used unauthenticated sidecar flow: {event.get('flowId')}")

    control = [event for event in events if event.get("type") == "off_tor_control"]
    expected_steps = (
        "tor_off",
        "lan_on",
        "lan_tcp_marked",
        "lan_udp_marked",
        "lan_off",
        "tor_on",
        "tor_marked",
        "tor_off_final",
        "direct_marked",
    )
    if tuple(event.get("step") for event in control) != expected_steps:
        fail("same-build off-Tor LAN control sequence is missing or out of order")
    for event in control:
        if event.get("packageSha256") != package_hash:
            fail("off-Tor control used a different build")
        if event.get("torSelected") is True and event.get("oslLanEnabled") is True:
            fail("off-Tor control persisted simultaneous Tor and OSL LAN")
    for step, protocol in (("lan_tcp_marked", "tcp"), ("lan_udp_marked", "udp")):
        row = next(event for event in control if event.get("step") == step)
        if row.get("protocol") != protocol or row.get("packets", 0) < 1 or row.get("bytes", 0) < 1:
            fail(f"off-Tor LAN control emitted no marked {protocol.upper()} traffic")
    tor_control = next(event for event in control if event.get("step") == "tor_marked")
    if tor_control.get("throughAuthenticatedSidecar") is not True or tor_control.get("verifiedPublicTor") is not True:
        fail("same-build control did not return to marked Tor traffic")
    direct_control = next(event for event in control if event.get("step") == "direct_marked")
    if direct_control.get("directBytes", 0) < 1:
        fail("same-build control did not return to marked direct traffic")

    return {
        "package_sha256": package_hash,
        "states": len(LIFECYCLE_STATES),
        "active_states": len(ACTIVE_STATES),
        "idle_seconds": int((idle_end - idle_begin).total_seconds()),
        "collectors": len(REQUIRED_COLLECTORS),
        "callers": len(caller_ids),
        "subjects": len(subject_ids),
        "positive_attempts": len(positives),
        "lan_attempts": len(lan_attempts),
        "uia_copy_observations": len(lan_attempts) + 1,
        "kernel_events": len(kernel_events),
        "unclassified_or_escaped_writes": 0,
        "control_tcp_packets": next(event for event in control if event.get("step") == "lan_tcp_marked")["packets"],
        "control_udp_packets": next(event for event in control if event.get("step") == "lan_udp_marked")["packets"],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        result = verify_bundle(args.bundle, require_production=True)
    except VerificationError as exc:
        print(f"TOR-4919-RED: {exc}", file=sys.stderr)
        return 1
    for key, value in result.items():
        print(f"TASK4919_{key.upper()}={value}")
    print(f"TASK4919_EXACT_COPY={EXACT_LAN_COPY}")
    print("TOR-4919-GREEN")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
