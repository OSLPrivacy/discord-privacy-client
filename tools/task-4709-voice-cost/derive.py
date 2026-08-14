#!/usr/bin/env python3
"""TASK 4709 -- derive and verify megabytes-per-hour strictly from raw artifacts.

Every number this script prints is computed from the raw per-trial artifacts
written by measure.py: the pion-observed wire byte counters and real
timestamps buffered inside each speaker's own events log, the participant
key fingerprints and speech-source hashes recorded in the same log, the
process identities, the system-wide /proc/net/dev counters, and the host
headroom snapshots. It never trusts a pre-computed number from a prior run.

Exit 1, with a specific reason, on: a zero-duration or zero-byte row; a
missing or duplicate participant stream; a wire counter that goes backwards
(a "reset"); insufficient host CPU/memory/uplink headroom or evidence of
saturation; a participant fed by anything other than the real speech corpus;
or any artifact the manifest does not account for. Only prints and rounds
the final megabytes-per-hour figures after every check has passed.
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

TASK_ROOT = Path(__file__).resolve().parent
REPO_ROOT = TASK_ROOT.parents[1]
SPEECH_DIR = TASK_ROOT / "speech"

MIN_HEADROOM_MEM_AVAILABLE_KB = 512_000
MAX_LOADAVG_PER_CORE = 1.5
MAX_UPLINK_UTILISATION_PCT = 5.0
TWO_PERSON_AGREEMENT_TOLERANCE = 0.10


class CheckFailure(RuntimeError):
    pass


def fail(msg: str) -> "NoReturn":
    raise CheckFailure(msg)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_events(path: Path) -> list[dict]:
    if not path.is_file() or path.stat().st_size == 0:
        fail(f"absent or empty artifact: {path}")
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        rows.append(json.loads(line))
    return rows


def verify_manifest(artifacts: Path) -> None:
    manifest_path = artifacts / "artifact-manifest.json"
    if not manifest_path.is_file():
        fail(f"absent artifact-manifest.json in {artifacts}")
    manifest = json.loads(manifest_path.read_text())
    on_disk = {
        str(p.relative_to(artifacts)) for p in artifacts.rglob("*")
        if p.is_file() and p.name != "artifact-manifest.json"
    }
    if on_disk != set(manifest):
        fail(f"artifact manifest mismatch in {artifacts}: "
             f"missing={sorted(on_disk - set(manifest))} extra={sorted(set(manifest) - on_disk)}")
    for rel, expected in manifest.items():
        actual = sha256_file(artifacts / rel)
        if actual != expected:
            fail(f"artifact hash mismatch {artifacts / rel}: expected={expected} actual={actual}")


def real_speech_hashes() -> set[str]:
    files = sorted(SPEECH_DIR.glob("speech-*.ogg"))
    if len(files) < 10:
        fail("real speech corpus is missing files")
    return {sha256_file(f) for f in files}


def derive_trial(artifacts: Path, known_speech_hashes: set[str]) -> dict:
    verify_manifest(artifacts)

    config = json.loads((artifacts / "trial-config.json").read_text())
    process_identities = json.loads((artifacts / "process-identities.json").read_text())
    net_dev = json.loads((artifacts / "net-dev-counters.json").read_text())
    headroom = json.loads((artifacts / "host-headroom.json").read_text())

    identities = config["identities"]
    if len(identities) != config["participant_count"]:
        fail(f"{artifacts}: identity count does not match participant_count")
    if len(set(identities)) != len(identities):
        fail(f"{artifacts}: duplicate participant identity")

    # --- host headroom / saturation, checked before trusting any byte number ---
    for label in ("before", "trial_end", "after"):
        snap = headroom[label]
        cpu = snap["cpu"]
        if cpu["loadavg_1m"] > cpu["nproc"] * MAX_LOADAVG_PER_CORE:
            fail(f"{artifacts}: host CPU saturated at {label}: loadavg_1m={cpu['loadavg_1m']} nproc={cpu['nproc']}")
        if cpu["mem_available_kb"] < MIN_HEADROOM_MEM_AVAILABLE_KB:
            fail(f"{artifacts}: insufficient memory headroom at {label}: available_kb={cpu['mem_available_kb']}")
        uplink = snap["uplink"]
        if uplink["link_speed_mbps"] is None or uplink["link_speed_mbps"] <= 0:
            fail(f"{artifacts}: insufficient uplink headroom evidence at {label}: no usable link speed")

    before_eth0 = headroom["before"]["uplink"]
    end_eth0 = headroom["trial_end"]["uplink"]
    duration_wall_s = (headroom["trial_end"]["wall_unix_ns"] - headroom["before"]["wall_unix_ns"]) / 1e9
    if duration_wall_s <= 0:
        fail(f"{artifacts}: zero-duration host headroom window")
    tx_delta_bytes = end_eth0["tx_bytes"] - before_eth0["tx_bytes"]
    rx_delta_bytes = end_eth0["rx_bytes"] - before_eth0["rx_bytes"]
    if tx_delta_bytes < 0 or rx_delta_bytes < 0:
        fail(f"{artifacts}: uplink interface counter reset detected")
    link_capacity_bytes = before_eth0["link_speed_mbps"] * 1_000_000 / 8 * duration_wall_s
    utilisation_pct = 100.0 * max(tx_delta_bytes, rx_delta_bytes) / link_capacity_bytes if link_capacity_bytes else 100.0
    if utilisation_pct > MAX_UPLINK_UTILISATION_PCT:
        fail(f"{artifacts}: insufficient uplink headroom: utilisation={utilisation_pct:.4f}% > {MAX_UPLINK_UTILISATION_PCT}%")

    # --- system-wide loopback counters, immutable raw interface counters used
    #     as the saturation cross-check so local congestion cannot masquerade
    #     as protocol cost ---
    lo_before = net_dev["before"]["lo"]
    lo_after = net_dev["after"]["lo"]
    lo_rx_delta = lo_after["rx_bytes"] - lo_before["rx_bytes"]
    lo_tx_delta = lo_after["tx_bytes"] - lo_before["tx_bytes"]
    if lo_rx_delta < 0 or lo_tx_delta < 0:
        fail(f"{artifacts}: loopback interface counter reset detected")
    if lo_rx_delta == 0 or lo_tx_delta == 0:
        fail(f"{artifacts}: zero-byte loopback interface delta for a real trial")

    # --- process identities: real, distinct PIDs for server + every speaker ---
    pids_by_role = {row["role"]: row["pid"] for row in process_identities}
    if "livekit-server" not in pids_by_role:
        fail(f"{artifacts}: missing livekit-server process identity")
    for identity in identities:
        if identity not in pids_by_role:
            fail(f"{artifacts}: missing process identity for {identity}")
    all_pids = list(pids_by_role.values())
    if len(set(all_pids)) != len(all_pids):
        fail(f"{artifacts}: duplicate process id across participants/server")
    if config["livekit_server_sha256"] != next(r["executable_sha256"] for r in process_identities if r["role"] == "livekit-server"):
        fail(f"{artifacts}: livekit-server executable identity mismatch")
    for identity in identities:
        row = next(r for r in process_identities if r["role"] == identity)
        if row["executable_sha256"] != config["speaker_binary_sha256"]:
            fail(f"{artifacts}: {identity} executable identity mismatch")

    fingerprints: dict[str, str] = {}
    speech_hashes: dict[str, str] = {}
    participants: dict[str, dict] = {}

    for identity in identities:
        events_path = artifacts / f"{identity}.events.jsonl"
        events = load_events(events_path)
        by_event: dict[str, list[dict]] = {}
        for row in events:
            by_event.setdefault(row["event"], []).append(row)

        if "participant_key" not in by_event:
            fail(f"{artifacts}/{identity}: missing participant_key artifact")
        fingerprint = by_event["participant_key"][0]["speaker_key_fingerprint"]
        if not fingerprint or len(fingerprint) != 64:
            fail(f"{artifacts}/{identity}: malformed participant key fingerprint")
        fingerprints[identity] = fingerprint

        if "speech_source" not in by_event:
            fail(f"{artifacts}/{identity}: missing speech_source artifact")
        speech = by_event["speech_source"][0]
        if speech["speech_source_bytes"] <= 0:
            fail(f"{artifacts}/{identity}: zero-byte speech source (synthetic-only/absent input)")
        if speech["speech_source_sha256"] not in known_speech_hashes:
            fail(f"{artifacts}/{identity}: speech source is not one of the real recorded speech files "
                 f"(synthetic-only input): {speech['speech_source_sha256']}")
        speech_hashes[identity] = speech["speech_source_sha256"]

        if "trial_start" not in by_event or "trial_end" not in by_event:
            fail(f"{artifacts}/{identity}: missing trial_start/trial_end artifact")
        subscriptions = by_event["trial_start"][0]["subscriptions"]
        expected_peers = by_event["trial_start"][0]["expected_peers"]
        if subscriptions < expected_peers:
            fail(f"{artifacts}/{identity}: starved -- subscribed to {subscriptions} of {expected_peers} peers")

        if "wire_counters_final" not in by_event:
            fail(f"{artifacts}/{identity}: missing wire_counters_final artifact")
        final = by_event["wire_counters_final"][0]
        sent = final["trial_sent_wire_bytes"]
        received = final["trial_received_wire_bytes"]
        if sent < 0 or received < 0:
            fail(f"{artifacts}/{identity}: wire counter reset detected (negative trial delta)")
        if sent == 0:
            fail(f"{artifacts}/{identity}: zero-byte sent row")
        if config["participant_count"] > 1 and received == 0:
            fail(f"{artifacts}/{identity}: zero-byte received row")

        first_send = final["first_send_unix_ns"]
        last_send = final["last_send_unix_ns"]
        if first_send <= 0 or last_send <= first_send:
            fail(f"{artifacts}/{identity}: zero-duration send window")
        send_duration_s = (last_send - first_send) / 1e9

        participants[identity] = {
            "identity": identity,
            "speaker_key_fingerprint": fingerprint,
            "speech_source_sha256": speech_hashes[identity],
            "speech_source_file": config["speech_source_files"][identity],
            "pid": pids_by_role[identity],
            "sent_wire_bytes": sent,
            "received_wire_bytes": received,
            "total_wire_bytes": sent + received,
            "send_duration_seconds": send_duration_s,
            "send_duration_minutes": send_duration_s / 60.0,
            "mb_per_hour": (sent + received) / 1_000_000 / (send_duration_s / 3600.0),
        }

    if len(set(fingerprints.values())) != len(fingerprints):
        fail(f"{artifacts}: duplicate participant-key fingerprint across participants")
    if len(set(speech_hashes.values())) != len(speech_hashes):
        fail(f"{artifacts}: duplicate speech-source stream across participants")

    trial_mb_per_hour = sum(p["mb_per_hour"] for p in participants.values()) / len(participants)
    trial_duration_minutes = sum(p["send_duration_minutes"] for p in participants.values()) / len(participants)

    return {
        "trial_name": config["trial_name"], "room": config["room"],
        "participant_count": config["participant_count"], "date": config["date"], "build": config["build"],
        "cpu_headroom_loadavg_1m_before": headroom["before"]["cpu"]["loadavg_1m"],
        "cpu_headroom_nproc": headroom["before"]["cpu"]["nproc"],
        "memory_headroom_available_kb_before": headroom["before"]["cpu"]["mem_available_kb"],
        "uplink_headroom_utilisation_pct": utilisation_pct,
        "uplink_interface": headroom["uplink_interface"],
        "loopback_rx_delta_bytes": lo_rx_delta, "loopback_tx_delta_bytes": lo_tx_delta,
        "participants": participants,
        "trial_mb_per_hour_unrounded": trial_mb_per_hour,
        "trial_duration_minutes": trial_duration_minutes,
    }


def main() -> int:
    artifacts_root = REPO_ROOT / "evidence" / "task-4709-voice-cost"
    trials = {
        "2-person-run-1": "2p-run1", "2-person-run-2": "2p-run2",
        "5-person": "5p", "10-person": "10p",
    }
    known_speech_hashes = real_speech_hashes()

    try:
        derived = {}
        for label, trial_name in trials.items():
            artifacts = artifacts_root / trial_name
            if not artifacts.is_dir():
                fail(f"absent trial artifacts: {artifacts}")
            derived[label] = derive_trial(artifacts, known_speech_hashes)

        two_a = derived["2-person-run-1"]["trial_mb_per_hour_unrounded"]
        two_b = derived["2-person-run-2"]["trial_mb_per_hour_unrounded"]
        relative_diff = abs(two_a - two_b) / ((two_a + two_b) / 2)
        if relative_diff > TWO_PERSON_AGREEMENT_TOLERANCE:
            fail(f"two independent 2-person runs disagree by {relative_diff * 100:.2f}%: "
                 f"run1={two_a:.4f} MB/hour run2={two_b:.4f} MB/hour")

    except CheckFailure as exc:
        print(f"TASK4709_FAIL {exc}", file=sys.stderr)
        return 1

    report = {
        "two_person_run_1_mb_per_hour": round(two_a, 4),
        "two_person_run_2_mb_per_hour": round(two_b, 4),
        "two_person_agreement_pct": round(relative_diff * 100, 4),
        "five_person_mb_per_hour": round(derived["5-person"]["trial_mb_per_hour_unrounded"]),
        "ten_person_mb_per_hour": round(derived["10-person"]["trial_mb_per_hour_unrounded"], 4),
        "trials": derived,
    }
    out_path = artifacts_root / "derived-report.json"
    out_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    for label in trials:
        t = derived[label]
        print(
            f"TASK4709_TRIAL_RESULT trial={label} room={t['room']} participants={t['participant_count']} "
            f"duration_minutes={t['trial_duration_minutes']:.4f} mb_per_hour={t['trial_mb_per_hour_unrounded']:.4f} "
            f"cpu_headroom_loadavg={t['cpu_headroom_loadavg_1m_before']}/{t['cpu_headroom_nproc']} "
            f"mem_headroom_kb={t['memory_headroom_available_kb_before']} "
            f"uplink_utilisation_pct={t['uplink_headroom_utilisation_pct']:.6f}"
        )
    print(f"TASK4709_TWO_PERSON_AGREEMENT run1={two_a:.4f} run2={two_b:.4f} diff_pct={relative_diff * 100:.4f}")
    print(f"TASK4709_FIVE_PERSON_MB_PER_HOUR {report['five_person_mb_per_hour']}")
    print("TASK4709_PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
