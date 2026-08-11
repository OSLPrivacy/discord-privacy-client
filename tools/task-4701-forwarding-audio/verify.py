#!/usr/bin/env python3
"""Fail-closed verifier for TASK 4701 qualification artifacts."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

TASK_ROOT = Path(__file__).resolve().parent
LOCK = json.loads((TASK_ROOT / "release-lock.json").read_text())


def fail(capability: str, detail: str) -> "NoReturn":
    print(f"TASK4701 FAIL capability={capability} detail={detail}", file=sys.stderr)
    raise SystemExit(1)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def jsonl(path: Path) -> list[dict[str, object]]:
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8", errors="strict").splitlines(), 1):
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            fail("artifact-parse", f"{path.name}:{number}: {error}")
        if not isinstance(value, dict):
            fail("artifact-parse", f"{path.name}:{number}: not an object")
        rows.append(value)
    return rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--allow-postproof-missing", action="store_true")
    args = parser.parse_args()
    root = args.artifacts.resolve()
    required = [
        "artifact-manifest.json", "clear-client.txt", "client-send-hashes.jsonl",
        "livekit-server.txt", "observed-sinks.json", "package-identity.json",
        "process-identities.json", "runtime-configuration.json",
        "server-boundary-capture.jsonl", "speaker-A.jsonl", "speaker-A.stderr.txt",
        "speaker-B.jsonl", "speaker-B.stderr.txt", "speaker-C.jsonl", "speaker-C.stderr.txt",
        "tls-front-door.jsonl", "transport-handshakes.jsonl", "trial-summary.json",
    ]
    if not args.allow_postproof_missing:
        required.extend(["candidate-source-lines.txt", "mutants.txt"])
    for name in required:
        if not (root / name).is_file():
            fail("artifact-completeness", f"missing {name}")

    manifest = json.loads((root / "artifact-manifest.json").read_text())
    for name in required[1:]:
        if manifest.get(name) != sha256(root / name):
            fail("artifact-integrity", f"hash differs for {name}")

    if not args.allow_postproof_missing:
        source_proof = (root / "candidate-source-lines.txt").read_text()
        revisions = {
            "livekit": "3b9f118327b257301083a7c4aa46076c8012918a",
            "mediasoup": "f8b20b9de5831cb9cd5e2f51c2138129fd61b94f",
            "ion-sfu": "a970af33ddc3bf8782bf49d1de4006180e3e1c08",
            "janus-gateway": "07c61050038c7d745013fae8bc8e99d7365c31f1",
        }
        for candidate, revision in revisions.items():
            if f"CANDIDATE {candidate} REVISION {revision}" not in source_proof:
                fail("candidate-comparison", f"missing source revision/lines for {candidate}")
        mutant_proof = (root / "mutants.txt").read_text()
        for name in (
            "starve-speaker-A", "starve-speaker-B", "starve-speaker-C",
            "substitute-binary", "substitute-configuration", "omit-artifact",
            "enable-decode-reencode", "enable-recording", "allow-clear-client", "reencode-payload",
        ):
            if f"name={name} exit=1" not in mutant_proof:
                fail("negative-proof-inventory", f"missing red result {name}")
        if "restored=green total=10" not in mutant_proof:
            fail("negative-proof-inventory", "restored green result missing")

    package = json.loads((root / "package-identity.json").read_text())
    if package.get("release_server_binary_sha256") != LOCK["release_server_binary_sha256"]:
        fail("release-binary-identity", "server binary SHA-256 differs")
    if package.get("release_server_tar_sha256") != LOCK["release_tar_sha256"] or package.get("source_revision") != LOCK["source_revision"]:
        fail("release-binary-identity", "release tar or source revision differs")
    if package.get("release_server_version") != "livekit-server version 1.13.5":
        fail("release-binary-identity", "version output differs")

    runtime_config = json.loads((root / "runtime-configuration.json").read_text())
    exact_livekit = (TASK_ROOT / "config" / "livekit.yaml").read_text()
    exact_front = json.loads((TASK_ROOT / "config" / "front-door.json").read_text())
    if runtime_config.get("livekit_yaml") != exact_livekit or runtime_config.get("livekit_config_sha256") != LOCK["livekit_config_sha256"]:
        fail("release-configuration-identity", "LiveKit configuration differs")
    if runtime_config.get("front_door_json") != exact_front or runtime_config.get("front_door_config_sha256") != LOCK["front_door_config_sha256"]:
        fail("release-configuration-identity", "TLS front-door configuration differs")
    for name in ("recording_outputs", "egress_services", "ingress_services", "sip_services"):
        if runtime_config.get(name) != []:
            fail("no-recording-output", f"configured service/output present: {name}")

    identities = json.loads((root / "process-identities.json").read_text())
    roles = {row["role"]: row for row in identities}
    expected_roles = {"livekit-server", "tls-front-door", *LOCK["required_speakers"]}
    if set(roles) != expected_roles or len({row["pid"] for row in identities}) != 5:
        fail("process-identity", f"roles/PIDs differ: {sorted(roles)}")
    if roles["livekit-server"]["executable_sha256"] != LOCK["release_server_binary_sha256"]:
        fail("release-binary-identity", "running server executable differs")
    speaker_hashes = {roles[name]["executable_sha256"] for name in LOCK["required_speakers"]}
    if len(speaker_hashes) != 1 or package.get("speaker_binary_sha256") != sorted(speaker_hashes):
        fail("process-identity", "speaker executables are not the recorded independent binary")

    summary = json.loads((root / "trial-summary.json").read_text())
    if summary.get("duration_seconds", 0) < LOCK["minimum_duration_seconds"]:
        fail("ten-minute-duration", f"seconds={summary.get('duration_seconds')}")
    if summary.get("speaker_processes") != 3 or summary.get("server_node_count") != 1 or len(summary.get("room_sids", [])) != 1:
        fail("one-room-one-machine", "speaker/node/room identity differs")

    events: dict[str, list[dict[str, object]]] = {}
    sends: dict[str, list[dict[str, object]]] = {}
    receives: dict[str, list[dict[str, object]]] = {}
    for identity in LOCK["required_speakers"]:
        events[identity] = jsonl(root / f"{identity}.jsonl")
        sends[identity] = [row for row in events[identity] if row.get("event") == "send"]
        receives[identity] = [row for row in events[identity] if row.get("event") == "receive"]
        if len(sends[identity]) != LOCK["frames_per_speaker"]:
            fail(f"{identity}-frame-count", f"sent={len(sends[identity])} want=3000")
        if [row.get("sequence") for row in sends[identity]] != list(range(LOCK["frames_per_speaker"])):
            fail(f"{identity}-frame-count", "sequence inventory differs")
        signed = [row for row in events[identity] if row.get("event") == "signed_in"]
        if len(signed) != 1 or signed[0].get("signed_in") is not True or signed[0].get("room") != LOCK["room"]:
            fail(f"{identity}-signed-in", "independent sign-in/room evidence differs")
        ends = [row for row in events[identity] if row.get("event") == "trial_end"]
        if len(ends) != 1 or ends[0].get("decoded_frames") != 0 or ends[0].get("reencoded_frames") != 0:
            fail("zero-decode-reencode", f"{identity} counters differ")

    send_hash_rows = jsonl(root / "client-send-hashes.jsonl")
    if len(send_hash_rows) != 9000:
        fail("client-send-hashes", f"rows={len(send_hash_rows)} want=9000")
    capture = jsonl(root / "server-boundary-capture.jsonl")
    ingress = [row for row in capture if row.get("direction") == "server_ingress"]
    egress = [row for row in capture if row.get("direction") == "server_egress"]
    if len(ingress) != 9000 or len(egress) != 18000:
        fail("server-ingress-egress-capture", f"ingress={len(ingress)} egress={len(egress)}")

    for origin in LOCK["required_speakers"]:
        source = {row["payload_sha256"]: row for row in sends[origin]}
        if len(source) != 3000:
            fail(f"{origin}-frame-count", "payload hashes are not unique")
        for destination in LOCK["required_speakers"]:
            if destination == origin:
                continue
            recovered = {
                row["payload_sha256"]: row for row in receives[destination]
                if row.get("origin") == origin and row.get("payload_sha256") in source
            }
            if len(recovered) != 3000:
                fail("payload-recovery", f"{origin}->{destination} recovered={len(recovered)} want=3000")
            for packet_hash, received in recovered.items():
                if received.get("payload_hex") != source[packet_hash].get("payload_hex"):
                    fail("payload-pass-through", f"{origin}->{destination} hash={packet_hash} byte mismatch")

    a_ingress = [row for row in ingress if row.get("origin") == "speaker-A"]
    a_egress = [row for row in egress if row.get("origin") == "speaker-A"]
    if len(a_ingress) != 3000 or len(a_egress) != 6000 or len({row["payload_sha256"] for row in a_egress}) != 3000:
        fail("speaker-A-3000-ingress-egress", f"ingress={len(a_ingress)} egress={len(a_egress)}")
    a_source = {row["payload_sha256"]: row["payload_hex"] for row in a_ingress}
    if any(a_source.get(row["payload_sha256"]) != row["payload_hex"] for row in a_egress):
        fail("payload-pass-through", "speaker-A ingress/egress bytes differ")
    if summary.get("speaker_a_ingress_payloads") != 3000 or summary.get("speaker_a_unique_egress_payloads") != 3000 or summary.get("speaker_a_byte_identical") != 3000:
        fail("speaker-A-3000-ingress-egress", "summary counts differ")
    if summary.get("decoded_frames") != 0 or summary.get("reencoded_frames") != 0:
        fail("zero-decode-reencode", "summary counters differ")

    clear = (root / "clear-client.txt").read_text().strip()
    handshakes = jsonl(root / "transport-handshakes.jsonl")
    tls = [row for row in handshakes if row.get("event") == "tls_handshake" and row.get("protocol") == "TLSv1.3"]
    media = [row for row in handshakes if row.get("event") == "media_transport_handshake" and row.get("protocol") == "WebRTC DTLS-SRTP"]
    refused = [row for row in handshakes if row.get("event") == "clear_refused" and row.get("verdict") == "transport encryption required"]
    if clear != "transport encryption required" or not refused or len(tls) < 3 or {row.get("participant") for row in media} != set(LOCK["required_speakers"]):
        fail("mandatory-transport-encryption", f"clear={clear!r} tls={len(tls)} media={len(media)}")

    sinks = json.loads((root / "observed-sinks.json").read_text())
    if sinks.get("recording_output_count") != 0 or sinks.get("recording_file_sinks") != [] or sinks.get("recording_network_sinks") != []:
        fail("no-recording-output", "observed recording sink count is not zero")
    if sinks.get("runtime_files_before") != [] or sinks.get("runtime_files_after") != []:
        fail("no-recording-output", f"runtime workdir gained files: {sinks.get('runtime_files_after')}")
    if sinks.get("qualification_audit_outputs_are_ciphertext_only") is not True:
        fail("no-recording-output", "capture classification differs")

    print(
        "TASK4701 PASS candidate=LiveKit-1.13.5 speakers=3 room=1 node=1 "
        f"duration_seconds={summary['duration_seconds']:.3f} "
        "A_ingress=3000 A_unique_egress=3000 A_byte_identical=3000 "
        "B_recovered_A=3000 C_recovered_A=3000 decoded=0 reencoded=0 "
        "clear='transport encryption required' recording_outputs=0 artifacts=20"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
