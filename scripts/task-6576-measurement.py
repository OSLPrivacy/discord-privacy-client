#!/usr/bin/env python3
"""Measure the first non-prompt enclave removal without manufacturing N.

The production benchmark is the authority for enclave generation and re-key
work.  This outer harness freezes policy independently, records the invoking
host and process timings, validates every per-member result, and derives the
nearest-rank P95 distribution.  WSL/VM runs are retained as diagnostics but
are never publishable as the required real-hardware measurement.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import platform
import re
import shlex
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_POLICY = ROOT / "docs/evidence/task-6576/frozen-measurement-policy.json"
DEFAULT_OUTPUT = ROOT / "docs/evidence/task-6576/measurement.json"
DEFAULT_RAW_ROOT = ROOT / "docs/evidence/task-6576/raw"
JSON_PREFIX = "TASK6576_BENCHMARK_JSON="


class MeasurementError(RuntimeError):
    pass


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def command_output(argv: list[str]) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            argv,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=20,
            check=False,
        )
        return {
            "argv": argv,
            "exit_code": completed.returncode,
            "stdout": completed.stdout,
            "stderr": completed.stderr,
        }
    except (OSError, subprocess.TimeoutExpired) as error:
        return {"argv": argv, "error": str(error)}


def host_facts() -> dict[str, Any]:
    facts: dict[str, Any] = {
        "observed_at_utc": utc_now(),
        "platform": platform.platform(),
        "system": platform.system(),
        "release": platform.release(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "logical_cpu_count": os.cpu_count(),
        "python": sys.version,
    }
    uname_release = platform.release().lower()
    facts["wsl"] = "microsoft" in uname_release or bool(os.environ.get("WSL_DISTRO_NAME"))
    facts["virtualization"] = "unknown"
    facts["physical_cores"] = None
    facts["memory_total_bytes"] = None

    if platform.system() == "Linux":
        lscpu = command_output(["lscpu", "-J"])
        facts["lscpu"] = lscpu
        if lscpu.get("exit_code") == 0:
            try:
                rows = json.loads(lscpu["stdout"])["lscpu"]
                values = {row["field"].rstrip(":"): row.get("data") for row in rows}
                facts["cpu_model"] = values.get("Model name")
                cores = values.get("Core(s) per socket")
                sockets = values.get("Socket(s)")
                if str(cores).isdigit() and str(sockets).isdigit():
                    facts["physical_cores"] = int(cores) * int(sockets)
                if values.get("Hypervisor vendor"):
                    facts["virtualization"] = str(values["Hypervisor vendor"])
            except (KeyError, TypeError, ValueError, json.JSONDecodeError):
                pass
        virt = command_output(["systemd-detect-virt"])
        facts["systemd_detect_virt"] = virt
        if virt.get("exit_code") == 0 and str(virt.get("stdout", "")).strip():
            facts["virtualization"] = str(virt["stdout"]).strip()
        try:
            for line in Path("/proc/meminfo").read_text().splitlines():
                if line.startswith("MemTotal:"):
                    facts["memory_total_bytes"] = int(line.split()[1]) * 1024
                    break
        except (OSError, ValueError, IndexError):
            pass
    elif platform.system() == "Windows":
        script = (
            "$cpu=Get-CimInstance Win32_Processor | Select-Object -First 1 Name,NumberOfCores,NumberOfLogicalProcessors;"
            "$cs=Get-CimInstance Win32_ComputerSystem | Select-Object Manufacturer,Model,TotalPhysicalMemory,HypervisorPresent;"
            "@{cpu=$cpu;computer=$cs}|ConvertTo-Json -Compress -Depth 4"
        )
        windows = command_output(["powershell.exe", "-NoProfile", "-Command", script])
        facts["windows_cim"] = windows
        if windows.get("exit_code") == 0:
            try:
                observed = json.loads(windows["stdout"])
                cpu = observed["cpu"]
                computer = observed["computer"]
                facts["cpu_model"] = cpu["Name"]
                facts["physical_cores"] = int(cpu["NumberOfCores"])
                facts["memory_total_bytes"] = int(computer["TotalPhysicalMemory"])
                facts["virtualization"] = "hypervisor" if computer["HypervisorPresent"] else "none"
                facts["computer_manufacturer"] = computer["Manufacturer"]
                facts["computer_model"] = computer["Model"]
            except (KeyError, TypeError, ValueError, json.JSONDecodeError):
                pass

    git = command_output(["git", "rev-parse", "HEAD"])
    facts["git_head"] = str(git.get("stdout", "")).strip() if git.get("exit_code") == 0 else None
    dirty = command_output(["git", "status", "--porcelain"])
    facts["git_dirty_paths"] = (
        [line for line in str(dirty.get("stdout", "")).splitlines() if line]
        if dirty.get("exit_code") == 0
        else None
    )
    return facts


def load_policy(path: Path) -> tuple[dict[str, Any], str]:
    raw = path.read_bytes()
    policy = json.loads(raw)
    if policy.get("schema") != "osl.task6576.frozen-measurement-policy.v1":
        raise MeasurementError("measurement policy schema is absent or unsupported")
    criterion = policy.get("promptness_criterion", {})
    repetitions = criterion.get("minimum_repetitions_per_size")
    threshold = criterion.get("prompt_if_less_than_or_equal_ms")
    if not isinstance(repetitions, int) or repetitions < 3:
        raise MeasurementError("measurement policy requires at least 3 repetitions")
    if not isinstance(threshold, int) or threshold <= 0:
        raise MeasurementError("measurement policy has no positive frozen promptness criterion")
    if policy.get("freeze_precedes_measurement") is not True:
        raise MeasurementError("measurement policy does not attest that the freeze preceded measurement")
    return policy, sha256_bytes(raw)


def profile_failures(policy: dict[str, Any], facts: dict[str, Any]) -> list[str]:
    profile = policy["minimum_supported_real_hardware_profile"]
    failures: list[str] = []
    if facts.get("system") != "Windows":
        failures.append(f"operating_system={facts.get('system')} expected=Windows")
    if facts.get("wsl"):
        failures.append("virtualization=wsl is not publishable real hardware")
    if facts.get("virtualization") not in ("none", None):
        failures.append(f"virtualization={facts.get('virtualization')} expected=none")
    cores = facts.get("physical_cores")
    minimum = profile.get("minimum_physical_cores")
    if not isinstance(cores, int) or not isinstance(minimum, int) or cores < minimum:
        failures.append(f"physical_cores={cores} minimum={minimum}")
    model = str(facts.get("cpu_model") or "")
    if not model:
        failures.append("cpu_model=unobserved")
    return failures


def render_command(template: str, members: int, artifact_dir: Path, run_id: str) -> list[str]:
    try:
        rendered = template.format(
            members=members,
            artifact_dir=str(artifact_dir),
            run_id=run_id,
        )
    except KeyError as error:
        raise MeasurementError(f"benchmark command contains unknown placeholder {error}") from error
    argv = shlex.split(rendered, posix=platform.system() != "Windows")
    if not argv:
        raise MeasurementError("benchmark command is empty")
    return argv


def extract_benchmark_json(stdout: str, artifact_dir: Path) -> dict[str, Any]:
    for line in reversed(stdout.splitlines()):
        stripped = line.strip()
        if stripped.startswith(JSON_PREFIX):
            value = json.loads(stripped[len(JSON_PREFIX):])
            if isinstance(value, dict):
                return value
        if stripped.startswith("{") and stripped.endswith("}"):
            try:
                value = json.loads(stripped)
            except json.JSONDecodeError:
                continue
            if isinstance(value, dict) and (
                value.get("schema") == "osl.task6576.removal-benchmark.v1"
                or "duration_ms" in value
            ):
                return value
    candidates = sorted(artifact_dir.rglob("*.json"), key=lambda path: path.stat().st_mtime_ns)
    for path in reversed(candidates):
        try:
            value = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        if isinstance(value, dict) and (
            value.get("schema") == "osl.task6576.removal-benchmark.v1"
            or "duration_ms" in value
        ):
            return value
    raise MeasurementError("benchmark emitted no task-6576 JSON result")


def field(result: dict[str, Any], *names: str) -> Any:
    for name in names:
        if name in result:
            return result[name]
    return None


def progress_rows(result: dict[str, Any]) -> list[dict[str, Any]]:
    value = field(result, "progress_samples", "progress")
    if not isinstance(value, list) or not value:
        raise MeasurementError("progress is empty")
    if not all(isinstance(row, dict) for row in value):
        raise MeasurementError("progress contains a non-object sample")
    return value


def count_map(value: Any) -> dict[str, int]:
    if isinstance(value, int) and not isinstance(value, bool):
        return {f"aggregate-{index}": 1 for index in range(value)}
    if isinstance(value, dict):
        return {str(key): int(count) for key, count in value.items()}
    if isinstance(value, list):
        output: dict[str, int] = {}
        for index, row in enumerate(value):
            if isinstance(row, dict):
                identity = field(row, "member_id", "memberId", "id")
                count = field(row, "read_count", "readCount", "count")
                output[str(identity if identity is not None else index)] = int(count)
            else:
                output[str(index)] = int(row)
        return output
    raise MeasurementError("remaining-member read counts are absent")


def removed_reads(value: Any) -> dict[str, int]:
    if isinstance(value, dict):
        return {str(key): int(count) for key, count in value.items()}
    if isinstance(value, (int, float)):
        return {"aggregate": int(value)}
    raise MeasurementError("removed-member read-path observations are absent")


def validate_result(result: dict[str, Any], requested: int) -> dict[str, Any]:
    schema = result.get("schema")
    if schema not in (None, "osl.task6576.removal-benchmark.v1"):
        raise MeasurementError(f"benchmark schema={schema} is unsupported")
    observed = field(result, "members", "members_requested", "member_count", "memberCount")
    admitted = field(result, "admitted_members", "members_admitted", "admittedMembers")
    if int(observed) != requested or int(admitted) != requested:
        raise MeasurementError(
            f"member cap or admission mismatch requested={requested} observed={observed} admitted={admitted}"
        )
    complete = field(result, "removal_complete", "removal_completed", "complete")
    if complete is not True:
        raise MeasurementError(f"removal did not complete member_count={requested}")
    duration_ns = field(result, "duration_ns", "complete_removal_duration_ns")
    duration = field(result, "duration_ms", "complete_removal_duration_ms")
    if not isinstance(duration, (int, float)) or isinstance(duration, bool) or duration < 0:
        raise MeasurementError(f"measurement duration is absent member_count={requested}")
    if isinstance(duration_ns, int) and duration_ns > 0:
        normalized_duration_ms = max(1, math.ceil(duration_ns / 1_000_000))
    else:
        normalized_duration_ms = int(math.ceil(float(duration)))

    rows = progress_rows(result)
    previous_completed = -1
    previous_remaining = requested
    epoch: Any = None
    for index, row in enumerate(rows):
        completed = field(row, "completed", "completed_count")
        remaining = field(row, "remaining", "remaining_count")
        row_epoch = field(row, "epoch", "successor_epoch")
        if not isinstance(completed, int) or not isinstance(remaining, int):
            raise MeasurementError(f"progress sample={index} lacks completed/remaining counts")
        expected_completed = index
        expected_remaining = requested - 1 - index
        if completed != expected_completed or remaining != expected_remaining:
            raise MeasurementError(
                "progress is not independently tied to one completed re-key "
                f"sample={index} completed={completed} expected_completed={expected_completed} "
                f"remaining={remaining} expected_remaining={expected_remaining}"
            )
        if completed < previous_completed or remaining > previous_remaining:
            raise MeasurementError(
                f"progress is not monotonic sample={index} completed={completed} remaining={remaining}"
            )
        if completed + remaining != requested - 1:
            raise MeasurementError(
                f"progress member accounting mismatch sample={index} completed={completed} remaining={remaining} member_count={requested}"
            )
        if epoch is not None and row_epoch != epoch:
            raise MeasurementError(f"progress epoch changed sample={index} epoch={row_epoch} expected={epoch}")
        epoch = row_epoch if epoch is None else epoch
        previous_completed, previous_remaining = completed, remaining
    if previous_completed != requested - 1 or previous_remaining != 0:
        raise MeasurementError(
            f"progress stopped early completed={previous_completed} remaining={previous_remaining} member_count={requested}"
        )
    if len(rows) != requested:
        raise MeasurementError(
            f"progress sample starvation samples={len(rows)} expected={requested} member_count={requested}"
        )
    top_completed = field(result, "completed", "completed_count")
    top_remaining = field(result, "remaining", "remaining_count")
    if top_completed != previous_completed or top_remaining != previous_remaining:
        raise MeasurementError(
            "progress final accounting mismatch "
            f"completed={top_completed} expected_completed={previous_completed} "
            f"remaining={top_remaining} expected_remaining={previous_remaining}"
        )
    top_epoch = field(result, "epoch", "successor_epoch")
    if top_epoch != epoch:
        raise MeasurementError(f"epoch final mismatch epoch={top_epoch} progress_epoch={epoch}")
    if field(result, "success_after_last_rekey") is not True:
        raise MeasurementError(
            f"epoch success reported before the last re-key member_count={requested} epoch={epoch}"
        )

    removed = removed_reads(field(result, "removed_new_message_reads", "removed_member_reads"))
    leaking = {path: count for path, count in removed.items() if count != 0}
    if leaking:
        raise MeasurementError(f"removed member leaked message epoch={epoch} paths={leaking}")
    remaining = count_map(field(result, "remaining_member_reads", "remaining_member_read_counts"))
    if len(remaining) != requested - 1 or any(count != 1 for count in remaining.values()):
        raise MeasurementError(
            f"remaining member read mismatch member_count={requested} readers={len(remaining)} counts={remaining}"
        )
    warning = field(result, "warning_shown_before_confirmation", "delay_warning_before_confirmation")
    if warning is not None and not isinstance(warning, bool):
        raise MeasurementError("warning observation is not boolean")
    discarded = field(result, "enclave_discarded", "discarded", "disposed")
    if discarded is not True:
        raise MeasurementError(f"enclave was not discarded member_count={requested}")
    build_hash = field(result, "build_hash", "binary_hash", "buildHash")
    if not isinstance(build_hash, str) or not re.fullmatch(r"[0-9a-fA-F]{32,128}", build_hash):
        raise MeasurementError(f"build hash is absent or malformed member_count={requested}")
    return {
        "duration_ms": normalized_duration_ms,
        "build_hash": build_hash.lower(),
        "epoch": epoch,
        "progress_samples": len(rows),
        "progress_completed": previous_completed,
        "progress_remaining": previous_remaining,
        "removed_read_paths": removed,
        "remaining_reader_count": len(remaining),
        "warning_shown_before_confirmation": warning,
        "discarded": True,
    }


def nearest_rank(values: list[int], percentile: float) -> int:
    ordered = sorted(values)
    rank = max(1, math.ceil(percentile * len(ordered)))
    return ordered[rank - 1]


def distribution(values: list[int]) -> dict[str, Any]:
    return {
        "count": len(values),
        "values_ms": values,
        "min_ms": min(values),
        "median_ms": statistics.median(values),
        "nearest_rank_p95_ms": nearest_rank(values, 0.95),
        "max_ms": max(values),
    }


def run_one(
    template: str,
    members: int,
    repetition: int,
    artifact_root: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    run_id = f"members-{members}-rep-{repetition}"
    artifact_dir = artifact_root / run_id
    artifact_dir.mkdir(parents=True, exist_ok=False)
    argv = render_command(template, members, artifact_dir, run_id)
    started_utc = utc_now()
    started_ns = time.monotonic_ns()
    try:
        completed = subprocess.run(
            argv,
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_seconds,
            check=False,
            env=os.environ.copy(),
        )
    except subprocess.TimeoutExpired as error:
        raise MeasurementError(
            f"benchmark timed out member_count={members} repetition={repetition} timeout_seconds={timeout_seconds}"
        ) from error
    ended_ns = time.monotonic_ns()
    ended_utc = utc_now()
    invocation = {
        "run_id": run_id,
        "members": members,
        "repetition": repetition,
        "argv": argv,
        "started_at_utc": started_utc,
        "ended_at_utc": ended_utc,
        "outer_wall_duration_ms": math.ceil((ended_ns - started_ns) / 1_000_000),
        "exit_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }
    (artifact_dir / "outer-invocation.json").write_bytes(canonical_json(invocation))
    if completed.returncode != 0:
        raise MeasurementError(
            f"benchmark exit={completed.returncode} member_count={members} repetition={repetition}; "
            f"see {artifact_dir / 'outer-invocation.json'}"
        )
    result = extract_benchmark_json(completed.stdout, artifact_dir)
    normalized = validate_result(result, members)
    (artifact_dir / "validated-result.json").write_bytes(canonical_json(result))
    invocation["result"] = result
    invocation["normalized"] = normalized
    return invocation


def measure(args: argparse.Namespace) -> int:
    policy, policy_hash = load_policy(args.policy)
    facts = host_facts()
    failures = profile_failures(policy, facts)
    artifact_root = args.artifact_root
    if artifact_root.exists() and any(artifact_root.iterdir()):
        raise MeasurementError(f"raw artifact directory is not empty: {artifact_root}")
    artifact_root.mkdir(parents=True, exist_ok=True)
    criterion = policy["promptness_criterion"]
    campaign = policy["campaign"]
    repetitions = int(criterion["minimum_repetitions_per_size"])
    threshold = int(criterion["prompt_if_less_than_or_equal_ms"])
    start = int(campaign["starting_member_count"])
    step = int(campaign["member_count_step_before_crossing"])
    template = args.benchmark_command or policy["benchmark_contract"]["command"]
    measurements: list[dict[str, Any]] = []
    build_hash: str | None = None
    n: int | None = None

    for members in range(start, args.max_members + 1, step):
        runs = [
            run_one(template, members, repetition, artifact_root, args.timeout_seconds)
            for repetition in range(1, repetitions + 1)
        ]
        for run in runs:
            observed_hash = run["normalized"]["build_hash"]
            if build_hash is None:
                build_hash = observed_hash
            elif observed_hash != build_hash:
                raise MeasurementError(
                    f"build changed during measurement expected={build_hash} observed={observed_hash}"
                )
        stats = distribution([run["normalized"]["duration_ms"] for run in runs])
        measurements.append({"members": members, "runs": runs, "distribution": stats})
        print(
            f"TASK6576_MEASURE members={members} repetitions={repetitions} "
            f"p95_ms={stats['nearest_rank_p95_ms']} threshold_ms={threshold}"
        )
        if stats["nearest_rank_p95_ms"] > threshold:
            n = members
            break

    if n is None:
        raise MeasurementError(
            f"first non-prompt N was not observed through max_members={args.max_members}; refusing to guess"
        )
    if campaign.get("require_a_measured_size_below_n") and n == start:
        raise MeasurementError(
            f"first measured size={n} is already non-prompt; no measured below-N control exists"
        )
    far_above = max(
        n * int(campaign["far_above_n_multiplier"]),
        n + int(campaign["far_above_n_minimum_delta"]),
    )
    far_runs = [
        run_one(template, far_above, repetition, artifact_root, args.timeout_seconds)
        for repetition in range(1, repetitions + 1)
    ]
    for run in far_runs:
        if run["normalized"]["build_hash"] != build_hash:
            raise MeasurementError("build changed during far-above-N measurement")
    far_stats = distribution([run["normalized"]["duration_ms"] for run in far_runs])
    measurements.append({
        "members": far_above,
        "purpose": "far_above_n_admission_and_removal_control",
        "runs": far_runs,
        "distribution": far_stats,
    })

    output = {
        "schema": "osl.task6576.measurement-campaign.v1",
        "created_at_utc": utc_now(),
        "publishable": not failures,
        "profile_conforms": not failures,
        "non_publishable_reasons": failures,
        "policy_path": str(args.policy.relative_to(ROOT)),
        "policy_sha256": policy_hash,
        "policy": policy,
        "host": facts,
        "benchmark_command_template": template,
        "build_hash": build_hash,
        "first_non_prompt_n": n if not failures else None,
        "provisional_n_on_observed_host": n if failures else None,
        "threshold_ms": threshold,
        "decision_statistic": "nearest_rank_p95_complete_removal_ms",
        "far_above_n": far_above,
        "measurements": measurements,
        "all_enclaves_discarded": True,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(canonical_json(output))
    if failures:
        print(
            f"TASK6576_MEASUREMENT_EXIT=0 publishable=false provisional_n_on_observed_host={n} "
            f"profile={'|'.join(failures)} artifact={args.output}",
        )
        return 0
    print(
        f"TASK6576_MEASUREMENT_EXIT=0 N={n} threshold_ms={threshold} repetitions={repetitions} "
        f"far_above_n={far_above} artifact={args.output}"
    )
    return 0


def verify(args: argparse.Namespace) -> int:
    artifact = json.loads(args.artifact.read_text())
    policy, policy_hash = load_policy(args.policy)
    if artifact.get("schema") != "osl.task6576.measurement-campaign.v1":
        raise MeasurementError("measurement record schema is absent")
    if artifact.get("policy_sha256") != policy_hash:
        raise MeasurementError("measurement record policy digest differs from frozen policy")
    profile_conforms = artifact.get("profile_conforms") is True
    if artifact.get("publishable") is not True:
        if not args.allow_provisional_profile:
            raise MeasurementError(
                f"measurement record is not publishable profile={artifact.get('non_publishable_reasons')}"
            )
        if profile_conforms:
            raise MeasurementError("provisional-profile record inconsistently claims profile_conforms=true")
        if not artifact.get("non_publishable_reasons"):
            raise MeasurementError("provisional-profile record has no measured profile mismatch")
    n_field = "first_non_prompt_n" if profile_conforms else "provisional_n_on_observed_host"
    n = artifact.get(n_field)
    threshold = policy["promptness_criterion"]["prompt_if_less_than_or_equal_ms"]
    rows = artifact.get("measurements")
    if not isinstance(n, int) or not isinstance(rows, list) or not rows:
        raise MeasurementError("measurement record has no measured N or rows")
    hashes: set[str] = set()
    seen_sizes: list[int] = []
    n_row: dict[str, Any] | None = None
    for row in rows:
        members = int(row["members"])
        seen_sizes.append(members)
        runs = row.get("runs")
        minimum = policy["promptness_criterion"]["minimum_repetitions_per_size"]
        if not isinstance(runs, list) or len(runs) < minimum:
            raise MeasurementError(f"measurement starvation member_count={members} repetitions={len(runs or [])}")
        durations: list[int] = []
        for run in runs:
            normalized = validate_result(run["result"], members)
            durations.append(normalized["duration_ms"])
            hashes.add(normalized["build_hash"])
        expected = distribution(durations)
        if row.get("distribution") != expected:
            raise MeasurementError(f"measurement distribution mismatch member_count={members}")
        if members == n:
            n_row = row
    if len(hashes) != 1 or artifact.get("build_hash") not in hashes:
        raise MeasurementError(f"measurement build mismatch hashes={sorted(hashes)}")
    if n_row is None or n_row["distribution"]["nearest_rank_p95_ms"] <= threshold:
        raise MeasurementError(f"measurement N={n} does not cross threshold_ms={threshold}")
    ordinary = [size for size in seen_sizes if size <= n]
    expected_ordinary = list(range(policy["campaign"]["starting_member_count"], n + 1))
    if ordinary != expected_ordinary:
        raise MeasurementError(f"measurement size starvation observed={ordinary} expected={expected_ordinary}")
    for row in rows:
        if row["members"] < n and row["distribution"]["nearest_rank_p95_ms"] > threshold:
            raise MeasurementError(f"fabricated N={n}; earlier non-prompt size={row['members']}")
    far_above = artifact.get("far_above_n")
    if far_above not in seen_sizes or far_above < max(n * 4, n + 100):
        raise MeasurementError(f"far-above-N size is absent N={n} far_above_n={far_above}")
    if artifact.get("all_enclaves_discarded") is not True:
        raise MeasurementError("generated enclaves were not all discarded")
    measurement_kind = "frozen_real_hardware_profile" if profile_conforms else "provisional_profile_mismatch"
    mismatch = "|".join(map(str, artifact.get("non_publishable_reasons", []))) or "none"
    print(
        f"TASK6576_VERIFY_EXIT=0 cap_inventory=separate N={n} threshold_ms={threshold} "
        f"sizes={','.join(map(str, seen_sizes))} build_hash={artifact.get('build_hash')} "
        f"measurement={measurement_kind} profile_mismatch={mismatch}"
    )
    return 0


def parser() -> argparse.ArgumentParser:
    top = argparse.ArgumentParser(description=__doc__)
    sub = top.add_subparsers(dest="mode", required=True)
    measure_parser = sub.add_parser("measure")
    measure_parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    measure_parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    measure_parser.add_argument("--artifact-root", type=Path, default=DEFAULT_RAW_ROOT)
    measure_parser.add_argument("--benchmark-command")
    measure_parser.add_argument("--max-members", type=int, default=4096)
    measure_parser.add_argument("--timeout-seconds", type=int, default=1800)
    verify_parser = sub.add_parser("verify")
    verify_parser.add_argument("--policy", type=Path, default=DEFAULT_POLICY)
    verify_parser.add_argument("--artifact", type=Path, default=DEFAULT_OUTPUT)
    verify_parser.add_argument(
        "--allow-provisional-profile",
        action="store_true",
        help="validate every measured run and N while explicitly retaining a non-publishable host mismatch",
    )
    return top


def main() -> int:
    args = parser().parse_args()
    try:
        if args.mode == "measure":
            return measure(args)
        return verify(args)
    except (MeasurementError, OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"TASK6576_MEASUREMENT_EXIT=1 {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
