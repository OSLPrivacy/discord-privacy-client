#!/usr/bin/env python3
"""Fail-closed reconciliation for TASK 6331 Windows receive observations.

This program does not drive Windows and cannot create acceptance evidence.  It
only reconciles four independently written JSON logs: one outside-observer log
and one workload log for each of the quiet and busy installed runs.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Iterable


OBSERVER_SCHEMA = "osl-task-6331-observer-v1"
WORKLOAD_SCHEMA = "osl-task-6331-workload-v1"
RUN_MS = 300_000
PROBE_STREAM_MAX_MS = 100
PROBE_MAX_LATENCY_MS = 500
COMPLETED_GAP_MAX_MS = 500
RECEIVE_BOUND_MS = 15_000
DISCORD_READ_FLOOR_MS = 750
WHATSAPP_READ_FLOOR_MS = 2_000
BUSY_LOOK_FLOOR_MS = 4_000


class ReconciliationError(RuntimeError):
    """A named, fail-closed TASK 6331 contradiction."""


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _integer(value: Any, label: str, *, minimum: int | None = None) -> int:
    if not _is_int(value) or (minimum is not None and value < minimum):
        raise ReconciliationError(f"{label} must be an integer" + (f" >= {minimum}" if minimum is not None else ""))
    return value


def _string(value: Any, label: str) -> str:
    if not isinstance(value, str) or not value:
        raise ReconciliationError(f"{label} must be a nonempty string")
    return value


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ReconciliationError(f"{label} must be an object")
    return value


def _array(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise ReconciliationError(f"{label} must be an array")
    return value


def _load(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ReconciliationError(f"cannot read {path}: {error}") from error
    return _object(value, str(path))


def _surface(value: Any, label: str) -> dict[str, Any]:
    surface = _object(value, label)
    required = {
        "hwnd",
        "process_id",
        "thread_id",
        "runtime_id",
        "generation",
        "automation_id",
        "name",
    }
    missing = sorted(required - set(surface))
    if missing:
        raise ReconciliationError(f"{label} missing {','.join(missing)}")
    _string(surface["hwnd"], f"{label}.hwnd")
    _integer(surface["process_id"], f"{label}.process_id", minimum=1)
    _integer(surface["thread_id"], f"{label}.thread_id", minimum=1)
    runtime_id = _array(surface["runtime_id"], f"{label}.runtime_id")
    if not runtime_id or any(not _is_int(part) for part in runtime_id):
        raise ReconciliationError(f"{label}.runtime_id must be a nonempty integer array")
    _integer(surface["generation"], f"{label}.generation", minimum=1)
    if surface["automation_id"] != "osl-protected-receive-surface":
        raise ReconciliationError(
            f"{label} names another OSL control automation_id={surface['automation_id']!r} hwnd={surface['hwnd']}"
        )
    if surface["name"] != "Messages prepared or opened in this OSL panel":
        raise ReconciliationError(
            f"{label} is not the visible conversation/receive surface name={surface['name']!r} hwnd={surface['hwnd']}"
        )
    return {key: surface[key] for key in sorted(required)}


def _surface_name(surface: dict[str, Any]) -> str:
    runtime = ",".join(str(part) for part in surface["runtime_id"])
    return (
        f"hwnd={surface['hwnd']} pid={surface['process_id']} "
        f"thread={surface['thread_id']} runtime={runtime} generation={surface['generation']} "
        f"automation_id={surface['automation_id']}"
    )


def _context(run: str, surface: dict[str, Any], *, provider_id: str = "-", input_id: str = "-", timestamps: str = "-") -> str:
    return (
        f"run={run} surface=[{_surface_name(surface)}] provider_id={provider_id} "
        f"input_id={input_id} timestamps={timestamps}"
    )


def _fail(
    reason: str,
    run: str,
    surface: dict[str, Any],
    *,
    provider_id: str = "-",
    input_id: str = "-",
    timestamps: str = "-",
) -> "NoReturn":
    raise ReconciliationError(f"{reason}: {_context(run, surface, provider_id=provider_id, input_id=input_id, timestamps=timestamps)}")


def _ticks_for_ms(milliseconds: int, frequency: int, run: str, surface: dict[str, Any]) -> int:
    numerator = milliseconds * frequency
    if numerator % 1000:
        _fail(f"QPC frequency cannot represent exact {milliseconds}ms schedule", run, surface)
    return numerator // 1000


def _at_most(delta_ticks: int, milliseconds: int, frequency: int) -> bool:
    return delta_ticks >= 0 and delta_ticks * 1000 <= milliseconds * frequency


def _elapsed_ms_ceiling(delta_ticks: int, frequency: int) -> int:
    """Conservatively derive elapsed milliseconds from arbitrary QPC ticks."""
    if delta_ticks < 0:
        raise ReconciliationError(f"negative QPC delta={delta_ticks}")
    numerator = delta_ticks * 1000
    return (numerator + frequency - 1) // frequency


def _required_clock(value: Any, run: str, surface: dict[str, Any], label: str) -> tuple[int, int, int]:
    clock = _object(value, f"{run}.{label}.clock")
    frequency = _integer(clock.get("qpc_frequency"), f"{run}.{label}.clock.qpc_frequency", minimum=1)
    start = _integer(clock.get("run_start_qpc"), f"{run}.{label}.clock.run_start_qpc", minimum=0)
    end = _integer(clock.get("run_end_qpc"), f"{run}.{label}.clock.run_end_qpc", minimum=0)
    expected = _ticks_for_ms(RUN_MS, frequency, run, surface)
    if end - start != expected:
        _fail(
            "run interval is not exactly 300000ms",
            run,
            surface,
            timestamps=f"start_qpc={start},end_qpc={end},frequency={frequency}",
        )
    return frequency, start, end


def _claimed_ms(actual: int, claimed: Any, reason: str, run: str, surface: dict[str, Any], **context: str) -> None:
    if not _is_int(claimed) or claimed != actual:
        _fail(f"fabricated {reason}: claimed={claimed!r} derived={actual}", run, surface, **context)


def _unique_ids(rows: Iterable[dict[str, Any]], field: str, reason: str, run: str, surface: dict[str, Any]) -> list[str]:
    values: list[str] = []
    seen: set[str] = set()
    for row in rows:
        value = _string(row.get(field), f"{run}.{reason}.{field}")
        if value in seen:
            _fail(f"duplicate {reason} {field}", run, surface, provider_id=value if field == "provider_id" else "-", input_id=value if field == "input_id" else "-")
        seen.add(value)
        values.append(value)
    return values


def _expected_look_offsets_ms() -> list[int]:
    offsets: list[int] = []
    at = 0
    for index in range(50):
        offsets.append(at)
        at += 4_000 if index % 2 == 0 else 8_000
    return offsets


def _validate_schedule(
    rows: list[dict[str, Any]],
    offsets_ms: list[int],
    run: str,
    surface: dict[str, Any],
    frequency: int,
    start: int,
    label: str,
    *,
    require_provider: bool = False,
) -> None:
    if len(rows) != len(offsets_ms):
        _fail(f"{label} count={len(rows)} expected={len(offsets_ms)}", run, surface)
    for index, (row, offset_ms) in enumerate(zip(rows, offsets_ms, strict=True)):
        expected = start + _ticks_for_ms(offset_ms, frequency, run, surface)
        scheduled = _integer(row.get("scheduled_qpc"), f"{run}.{label}[{index}].scheduled_qpc", minimum=0)
        observed = _integer(row.get("provider_qpc"), f"{run}.{label}[{index}].provider_qpc", minimum=0)
        provider_id = str(row.get("provider_id", "-"))
        if scheduled != expected:
            _fail(
                f"{label} schedule mismatch index={index} expected_offset_ms={offset_ms}",
                run,
                surface,
                provider_id=provider_id,
                timestamps=f"scheduled_qpc={scheduled},expected_qpc={expected}",
            )
        if not _at_most(observed - scheduled, PROBE_MAX_LATENCY_MS, frequency):
            _fail(
                f"{label} provider event missing or late",
                run,
                surface,
                provider_id=provider_id,
                timestamps=f"scheduled_qpc={scheduled},provider_qpc={observed}",
            )
        claimed_offset = row.get("scheduled_offset_ms")
        _claimed_ms(offset_ms, claimed_offset, f"{label} scheduled offset", run, surface, provider_id=provider_id, timestamps=f"scheduled_qpc={scheduled}")
        if require_provider:
            expected_provider = "Discord" if index % 2 == 0 else "WhatsApp"
            if row.get("provider") != expected_provider:
                _fail(f"{label} provider alternation mismatch expected={expected_provider}", run, surface, provider_id=provider_id, timestamps=f"provider_qpc={observed}")


def _validate_observer(
    observer: dict[str, Any],
    workload: dict[str, Any],
    run: str,
) -> tuple[dict[str, Any], dict[str, dict[str, Any]], int, int, int]:
    if observer.get("schema") != OBSERVER_SCHEMA or observer.get("run") != run:
        raise ReconciliationError(f"run={run} observer schema/run mismatch")
    surface = _surface(observer.get("surface"), f"{run}.observer.surface")
    if _surface(workload.get("surface"), f"{run}.workload.surface") != surface:
        _fail("observer/workload surface mismatch", run, surface)
    frequency, start, end = _required_clock(observer.get("clock"), run, surface, "observer")
    workload_frequency, workload_start, workload_end = _required_clock(workload.get("clock"), run, surface, "workload")
    if (workload_frequency, workload_start, workload_end) != (frequency, start, end):
        _fail(
            "observer/workload QPC clocks do not reconcile",
            run,
            surface,
            timestamps=f"observer={frequency}/{start}/{end},workload={workload_frequency}/{workload_start}/{workload_end}",
        )

    process = _object(observer.get("observer_process"), f"{run}.observer_process")
    observer_pid = _integer(process.get("pid"), f"{run}.observer_process.pid", minimum=1)
    _string(process.get("path"), f"{run}.observer_process.path")
    if process.get("role") != "outside-windows-observer":
        _fail("observer role is not outside Windows", run, surface)
    osl_pid = _integer(workload.get("osl_process_id"), f"{run}.osl_process_id", minimum=1)
    if osl_pid != surface["process_id"]:
        _fail(f"surface process pid={surface['process_id']} does not match OSL pid={osl_pid}", run, surface)
    forbidden_pids = {
        osl_pid,
        _integer(workload.get("harness_process_id"), f"{run}.harness_process_id", minimum=1),
        _integer(workload.get("workload_process_id"), f"{run}.workload_process_id", minimum=1),
    }
    provider_pids = _array(workload.get("provider_process_ids"), f"{run}.provider_process_ids")
    for pid in provider_pids:
        forbidden_pids.add(_integer(pid, f"{run}.provider_process_ids[]", minimum=1))
    if observer_pid in forbidden_pids or observer_pid == surface["process_id"]:
        _fail(f"observer pid={observer_pid} is inside OSL/harness/provider", run, surface)

    destroyed = _array(observer.get("destroyed_runtime_ids"), f"{run}.destroyed_runtime_ids")
    if surface["runtime_id"] in destroyed:
        _fail("bound surface identity was destroyed or reused", run, surface)
    resolutions = _array(observer.get("identity_resolutions"), f"{run}.identity_resolutions")
    if not resolutions:
        _fail("surface identity was never resolved", run, surface)
    for index, resolution in enumerate(resolutions):
        resolution = _object(resolution, f"{run}.identity_resolutions[{index}]")
        at = _integer(resolution.get("qpc"), f"{run}.identity_resolutions[{index}].qpc", minimum=0)
        resolved = _surface(resolution.get("surface"), f"{run}.identity_resolutions[{index}].surface")
        if resolved != surface:
            _fail("stale/recreated or different control identity", run, surface, timestamps=f"resolution_qpc={at}")
    if _integer(resolutions[0].get("qpc"), f"{run}.identity_resolutions[0].qpc", minimum=0) > start:
        _fail("surface identity was not bound before workload", run, surface, timestamps=f"bind_qpc={resolutions[0].get('qpc')},start_qpc={start}")

    probes_raw = _array(observer.get("probes"), f"{run}.probes")
    probes = [_object(row, f"{run}.probes[{index}]") for index, row in enumerate(probes_raw)]
    if not probes:
        _fail("empty probe set", run, surface)
    input_ids = _unique_ids(probes, "input_id", "probe", run, surface)
    by_id: dict[str, dict[str, Any]] = {}
    stream_sends: dict[str, list[int]] = {"pointer": [], "keyboard": []}
    prior_response: int | None = None
    longest_latency = 0
    longest_gap = 0

    for index, (probe, input_id) in enumerate(zip(probes, input_ids, strict=True)):
        kind = probe.get("kind")
        expected_kind = "pointer" if index % 2 == 0 else "keyboard"
        if kind != expected_kind:
            _fail(f"probe streams are not strictly alternating expected={expected_kind}", run, surface, input_id=input_id)
        target = _surface(probe.get("target_surface"), f"{run}.probe[{input_id}].target_surface")
        if target != surface:
            _fail("probe targeted another OSL control or stale identity", run, surface, input_id=input_id)
        sent = _integer(probe.get("sent_qpc"), f"{run}.probe[{input_id}].sent_qpc", minimum=0)
        response = _integer(probe.get("response_qpc"), f"{run}.probe[{input_id}].response_qpc", minimum=0)
        hook = _object(probe.get("os_hook"), f"{run}.probe[{input_id}].os_hook")
        hook_qpc = _integer(hook.get("qpc"), f"{run}.probe[{input_id}].os_hook.qpc", minimum=0)
        expected_hook = "WH_MOUSE_LL" if kind == "pointer" else "WH_KEYBOARD_LL"
        expected_flag = "LLMHF_INJECTED" if kind == "pointer" else "LLKHF_INJECTED"
        if (
            probe.get("injection_api") != "SendInput"
            or probe.get("direct_invocation") is not False
            or hook.get("input_id") != input_id
            or hook.get("hook") != expected_hook
            or hook.get("api") != "SendInput"
            or hook.get("injected_flag") != expected_flag
            or not sent <= hook_qpc <= response
        ):
            _fail(
                "synthetic/UIA/test-hook/direct invocation or missing SendInput OS hook",
                run,
                surface,
                input_id=input_id,
                timestamps=f"sent_qpc={sent},hook_qpc={hook_qpc},response_qpc={response}",
            )
        visible = _object(probe.get("visible_response"), f"{run}.probe[{input_id}].visible_response")
        if (
            visible.get("kind") not in {"focus", "scroll", "selection", "pixel"}
            or visible.get("changed") is not True
            or _integer(visible.get("qpc"), f"{run}.probe[{input_id}].visible_response.qpc", minimum=0) != response
            or _surface(visible.get("surface"), f"{run}.probe[{input_id}].visible_response.surface") != surface
        ):
            _fail("missing same-surface visible response", run, surface, input_id=input_id, timestamps=f"sent_qpc={sent},response_qpc={response}")
        if not start <= sent <= end or response > end:
            _fail("probe is outside workload interval", run, surface, input_id=input_id, timestamps=f"start_qpc={start},sent_qpc={sent},response_qpc={response},end_qpc={end}")
        latency_ms = _elapsed_ms_ceiling(response - sent, frequency)
        _claimed_ms(latency_ms, probe.get("latency_ms"), "probe latency", run, surface, input_id=input_id, timestamps=f"sent_qpc={sent},response_qpc={response}")
        if latency_ms > PROBE_MAX_LATENCY_MS:
            _fail(f"probe pause {latency_ms}ms exceeds 500ms", run, surface, input_id=input_id, timestamps=f"sent_qpc={sent},response_qpc={response}")
        gap_ms: int | None = None
        if prior_response is not None:
            gap_ms = _elapsed_ms_ceiling(response - prior_response, frequency)
            _claimed_ms(gap_ms, probe.get("completed_gap_ms"), "completed-probe gap", run, surface, input_id=input_id, timestamps=f"previous_response_qpc={prior_response},response_qpc={response}")
            if gap_ms > COMPLETED_GAP_MAX_MS:
                _fail(f"completed-probe gap {gap_ms}ms exceeds 500ms", run, surface, input_id=input_id, timestamps=f"previous_response_qpc={prior_response},response_qpc={response}")
            longest_gap = max(longest_gap, gap_ms)
        elif probe.get("completed_gap_ms") is not None:
            _fail("first probe has fabricated completed gap", run, surface, input_id=input_id, timestamps=f"response_qpc={response}")
        prior_response = response
        longest_latency = max(longest_latency, latency_ms)
        stream_sends[kind].append(sent)
        by_id[input_id] = probe

    for kind, sends in stream_sends.items():
        if not sends:
            _fail(f"empty {kind} probe stream", run, surface)
        if not _at_most(sends[0] - start, PROBE_STREAM_MAX_MS, frequency):
            _fail(f"missing initial {kind} probe interval", run, surface, timestamps=f"start_qpc={start},first_qpc={sends[0]}")
        if not _at_most(end - sends[-1], PROBE_STREAM_MAX_MS, frequency):
            _fail(f"missing terminal {kind} probe interval", run, surface, timestamps=f"last_qpc={sends[-1]},end_qpc={end}")
        for previous, current in zip(sends, sends[1:]):
            if not _at_most(current - previous, PROBE_STREAM_MAX_MS, frequency):
                _fail(f"missing {kind} interval above 100ms", run, surface, timestamps=f"previous_qpc={previous},current_qpc={current}")

    _claimed_ms(len(probes), observer.get("probe_count"), "probe count", run, surface)
    _claimed_ms(longest_latency, observer.get("longest_latency_ms"), "longest latency", run, surface)
    _claimed_ms(longest_gap, observer.get("longest_completed_gap_ms"), "longest completed gap", run, surface)
    return surface, by_id, frequency, start, end


def _validate_content(
    observer: dict[str, Any],
    markers: list[dict[str, Any]],
    probes: dict[str, dict[str, Any]],
    run: str,
    surface: dict[str, Any],
    frequency: int,
) -> None:
    changes_raw = _array(observer.get("content_changes"), f"{run}.content_changes")
    changes = [_object(row, f"{run}.content_changes[{index}]") for index, row in enumerate(changes_raw)]
    marker_ids = _unique_ids(markers, "provider_id", "protected marker", run, surface)
    change_ids = _unique_ids(changes, "provider_id", "content change", run, surface)
    if not marker_ids:
        _fail("empty protected marker set", run, surface)
    if set(change_ids) != set(marker_ids) or len(change_ids) != len(marker_ids):
        missing = sorted(set(marker_ids) - set(change_ids))
        extra = sorted(set(change_ids) - set(marker_ids))
        provider_id = (missing or extra or ["-"])[0]
        _fail(f"unmatched provider-ID row/content change missing={missing} extra={extra}", run, surface, provider_id=provider_id)
    marker_by_id = {row["provider_id"]: row for row in markers}
    row_ids: set[str] = set()
    for change in changes:
        provider_id = change["provider_id"]
        marker = marker_by_id[provider_id]
        observed = _integer(change.get("observed_qpc"), f"{run}.content[{provider_id}].observed_qpc", minimum=0)
        provider_qpc = _integer(marker.get("provider_qpc"), f"{run}.marker[{provider_id}].provider_qpc", minimum=0)
        if change.get("marker_text") != marker.get("marker_text") or change.get("change_kind") not in {"row_addition", "content_change"}:
            _fail("unmatched protected marker content row", run, surface, provider_id=provider_id, timestamps=f"provider_qpc={provider_qpc},observed_qpc={observed}")
        if _surface(change.get("surface"), f"{run}.content[{provider_id}].surface") != surface:
            _fail("protected marker changed another OSL control", run, surface, provider_id=provider_id, timestamps=f"observed_qpc={observed}")
        row_runtime_id = _string(change.get("row_runtime_id"), f"{run}.content[{provider_id}].row_runtime_id")
        if row_runtime_id in row_ids:
            _fail("multiple provider IDs matched one content row", run, surface, provider_id=provider_id, timestamps=f"observed_qpc={observed}")
        row_ids.add(row_runtime_id)
        if observed < provider_qpc:
            _fail("protected marker content predates provider event", run, surface, provider_id=provider_id, timestamps=f"provider_qpc={provider_qpc},observed_qpc={observed}")
        latency_ms = _elapsed_ms_ceiling(observed - provider_qpc, frequency)
        _claimed_ms(latency_ms, change.get("latency_ms"), "content latency", run, surface, provider_id=provider_id, timestamps=f"provider_qpc={provider_qpc},observed_qpc={observed}")
        if latency_ms < 0 or latency_ms > RECEIVE_BOUND_MS:
            _fail(f"protected marker receive latency {latency_ms}ms exceeds {RECEIVE_BOUND_MS}ms", run, surface, provider_id=provider_id, timestamps=f"provider_qpc={provider_qpc},observed_qpc={observed}")
        witness_id = _string(change.get("witness_input_id"), f"{run}.content[{provider_id}].witness_input_id")
        witness = probes.get(witness_id)
        if witness is None:
            _fail("content row has missing OS input witness", run, surface, provider_id=provider_id, input_id=witness_id, timestamps=f"observed_qpc={observed}")
        witness_response = _integer(witness.get("response_qpc"), f"{run}.probe[{witness_id}].response_qpc", minimum=0)
        if abs(witness_response - observed) * 1000 > PROBE_STREAM_MAX_MS * frequency:
            _fail("content row is not bound to a nearby OS input/response timestamp", run, surface, provider_id=provider_id, input_id=witness_id, timestamps=f"content_qpc={observed},response_qpc={witness_response}")
    _claimed_ms(len(changes), observer.get("content_change_count"), "content change count", run, surface)


def _validate_reads_and_looks(
    workload: dict[str, Any],
    markers: list[dict[str, Any]],
    run: str,
    surface: dict[str, Any],
    frequency: int,
    quiet_look_count: int | None,
) -> int:
    looks = [_object(row, f"{run}.looks[{index}]") for index, row in enumerate(_array(workload.get("looks"), f"{run}.looks"))]
    reads = [_object(row, f"{run}.carrier_reads[{index}]") for index, row in enumerate(_array(workload.get("carrier_reads"), f"{run}.carrier_reads"))]
    look_ids = _unique_ids(looks, "look_id", "look", run, surface)
    read_ids = _unique_ids(reads, "read_id", "carrier read", run, surface)
    del look_ids, read_ids
    for row in [*looks, *reads]:
        if _surface(row.get("surface"), f"{run}.look/read.surface") != surface:
            identifier = str(row.get("look_id", row.get("read_id", "-")))
            _fail("look/read targeted another OSL control", run, surface, input_id=identifier)
    if {row.get("look_id") for row in looks} != {row.get("look_id") for row in reads}:
        _fail("look and carrier-read IDs do not reconcile", run, surface)
    marker_ids = {row["provider_id"] for row in markers}
    for read in reads:
        if read.get("provider_id") not in marker_ids:
            _fail("carrier read is not bound to a protected provider ID", run, surface, provider_id=str(read.get("provider_id", "-")), timestamps=f"read_qpc={read.get('qpc')}")
    sorted_looks = sorted(looks, key=lambda row: _integer(row.get("qpc"), f"{run}.look.qpc", minimum=0))
    look_deltas = [
        _integer(right["qpc"], "look.qpc") - _integer(left["qpc"], "look.qpc")
        for left, right in zip(sorted_looks, sorted_looks[1:])
    ]
    look_gaps = [_elapsed_ms_ceiling(delta, frequency) for delta in look_deltas]
    if run == "quiet":
        if len(reads) > len(markers) + 5:
            _fail(f"quiet carrier screen reads={len(reads)} exceed received+5={len(markers)+5}", run, surface)
    else:
        if quiet_look_count is None:
            _fail("busy run lacks quiet look count", run, surface)
        if len(looks) * 10 < quiet_look_count * 9 or len(looks) * 10 > quiet_look_count * 11:
            _fail(f"busy look count={len(looks)} is outside 10 percent of quiet={quiet_look_count}", run, surface)
        if not look_deltas or min(look_deltas) * 1000 < BUSY_LOOK_FLOOR_MS * frequency:
            _fail(f"busy closest look gap={min(look_gaps) if look_gaps else 'empty'}ms breaches 4000ms", run, surface)
    by_provider: dict[str, list[int]] = {"Discord": [], "WhatsApp": []}
    for read in reads:
        provider = read.get("provider")
        if provider not in by_provider:
            _fail("unknown carrier-read provider", run, surface, provider_id=str(read.get("provider_id", "-")))
        by_provider[provider].append(_integer(read.get("qpc"), f"{run}.carrier_read.qpc", minimum=0))
    closest: dict[str, int] = {}
    for provider, floor in (("Discord", DISCORD_READ_FLOOR_MS), ("WhatsApp", WHATSAPP_READ_FLOOR_MS)):
        times = sorted(by_provider[provider])
        if len(times) < 2:
            _fail(f"{provider} carrier-read set is empty or incomplete", run, surface)
        deltas = [right - left for left, right in zip(times, times[1:])]
        gaps = [_elapsed_ms_ceiling(delta, frequency) for delta in deltas]
        closest[provider] = min(gaps)
        if min(deltas) * 1000 < floor * frequency:
            _fail(f"{provider} carrier-read spacing={closest[provider]}ms breaches floor={floor}ms", run, surface)
    metrics = _object(workload.get("metrics"), f"{run}.metrics")
    _claimed_ms(len(markers), metrics.get("protected_marker_count"), "protected marker count", run, surface)
    _claimed_ms(len(looks), metrics.get("look_count"), "look count", run, surface)
    _claimed_ms(len(reads), metrics.get("carrier_screen_read_count"), "carrier screen read count", run, surface)
    _claimed_ms(closest["Discord"], metrics.get("discord_closest_read_ms"), "Discord closest read", run, surface)
    _claimed_ms(closest["WhatsApp"], metrics.get("whatsapp_closest_read_ms"), "WhatsApp closest read", run, surface)
    if run == "busy":
        _claimed_ms(min(look_gaps), metrics.get("closest_look_ms"), "busy closest look", run, surface)
        _claimed_ms(quiet_look_count or 0, metrics.get("quiet_look_count"), "busy quiet look reference", run, surface)
    return len(looks)


def _validate_run(observer: dict[str, Any], workload: dict[str, Any], run: str, quiet_look_count: int | None = None) -> dict[str, Any]:
    if workload.get("schema") != WORKLOAD_SCHEMA or workload.get("run") != run:
        raise ReconciliationError(f"run={run} workload schema/run mismatch")
    surface, probes, frequency, start, _end = _validate_observer(observer, workload, run)
    markers = [_object(row, f"{run}.protected_markers[{index}]") for index, row in enumerate(_array(workload.get("protected_markers"), f"{run}.protected_markers"))]
    marker_offsets = [index * 6_000 for index in range(50)] if run == "quiet" else _expected_look_offsets_ms()
    _validate_schedule(markers, marker_offsets, run, surface, frequency, start, "protected markers", require_provider=True)
    _unique_ids(markers, "provider_id", "protected marker", run, surface)
    marker_texts = [row.get("marker_text") for row in markers]
    if any(not isinstance(text, str) or not text for text in marker_texts) or len(set(marker_texts)) != len(marker_texts):
        _fail("protected marker text set is empty or non-unique", run, surface)
    ordinary = [_object(row, f"{run}.ordinary_messages[{index}]") for index, row in enumerate(_array(workload.get("ordinary_messages"), f"{run}.ordinary_messages"))]
    if run == "quiet":
        if ordinary:
            _fail("quiet run contains ordinary carrier messages", run, surface)
    else:
        _validate_schedule(ordinary, [index * 1_500 for index in range(200)], run, surface, frequency, start, "ordinary messages")
        _unique_ids(ordinary, "provider_id", "ordinary message", run, surface)
        all_provider_ids = [row["provider_id"] for row in markers] + [row["provider_id"] for row in ordinary]
        if len(set(all_provider_ids)) != len(all_provider_ids):
            _fail("provider ID reused between protected and ordinary workload", run, surface)
        metrics = _object(workload.get("metrics"), f"{run}.metrics")
        _claimed_ms(len(ordinary), metrics.get("ordinary_message_count"), "ordinary message count", run, surface)
    _validate_content(observer, markers, probes, run, surface, frequency)
    look_count = _validate_reads_and_looks(workload, markers, run, surface, frequency, quiet_look_count)
    return {
        "run": run,
        "surface": surface,
        "probes": len(probes),
        "markers": len(markers),
        "ordinary_messages": len(ordinary),
        "looks": look_count,
        "longest_latency_ms": observer["longest_latency_ms"],
        "longest_completed_gap_ms": observer["longest_completed_gap_ms"],
    }


def reconcile(quiet_observer: dict[str, Any], quiet_workload: dict[str, Any], busy_observer: dict[str, Any], busy_workload: dict[str, Any]) -> dict[str, Any]:
    quiet = _validate_run(quiet_observer, quiet_workload, "quiet")
    busy = _validate_run(busy_observer, busy_workload, "busy", quiet["looks"])
    return {"status": "PASS", "quiet": quiet, "busy": busy}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Reconcile TASK 6331 outside-Windows receive logs")
    parser.add_argument("--quiet-observer", type=Path, required=True)
    parser.add_argument("--quiet-workload", type=Path, required=True)
    parser.add_argument("--busy-observer", type=Path, required=True)
    parser.add_argument("--busy-workload", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        result = reconcile(
            _load(args.quiet_observer),
            _load(args.quiet_workload),
            _load(args.busy_observer),
            _load(args.busy_workload),
        )
    except ReconciliationError as error:
        print(f"TASK6331_FAIL={error}", file=sys.stderr)
        return 1
    quiet = result["quiet"]
    busy = result["busy"]
    print(
        "TASK6331_PASS "
        f"quiet_probes={quiet['probes']} quiet_markers={quiet['markers']} quiet_looks={quiet['looks']} "
        f"busy_probes={busy['probes']} busy_markers={busy['markers']} busy_ordinary={busy['ordinary_messages']} busy_looks={busy['looks']} "
        f"max_latency_ms={max(quiet['longest_latency_ms'], busy['longest_latency_ms'])} "
        f"max_gap_ms={max(quiet['longest_completed_gap_ms'], busy['longest_completed_gap_ms'])}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
