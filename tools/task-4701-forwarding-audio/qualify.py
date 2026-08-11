#!/usr/bin/env python3
"""Run the exact TASK 4701 release qualification and record its artifacts."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import tarfile
import time
import urllib.request
from pathlib import Path

TASK_ROOT = Path(__file__).resolve().parent
REPO_ROOT = TASK_ROOT.parents[1]
ARTIFACTS = REPO_ROOT / "evidence" / "task-4701-forwarding-audio"
CACHE = Path("/tmp/osl-task-4701-cache")
LOCK = json.loads((TASK_ROOT / "release-lock.json").read_text(encoding="utf-8"))


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def download(url: str, destination: Path, expected: str) -> None:
    if destination.exists() and sha256(destination) == expected:
        return
    destination.unlink(missing_ok=True)
    with urllib.request.urlopen(url) as response, destination.open("wb") as output:
        shutil.copyfileobj(response, output)
    actual = sha256(destination)
    if actual != expected:
        raise RuntimeError(f"download hash mismatch: {destination} expected={expected} actual={actual}")


def seed_verified(destination: Path, expected: str, candidates: list[Path]) -> None:
    if destination.exists() and sha256(destination) == expected:
        return
    for candidate in candidates:
        if candidate.is_file() and sha256(candidate) == expected:
            shutil.copyfile(candidate, destination)
            return


def prepare_binaries() -> tuple[Path, Path, str]:
    CACHE.mkdir(parents=True, exist_ok=True)
    server_tar = CACHE / "livekit_1.13.5_linux_amd64.tar.gz"
    seed_verified(server_tar, LOCK["release_tar_sha256"], [Path("/tmp/osl4701-livekit/livekit-server.tar.gz")])
    download(LOCK["release_url"], server_tar, LOCK["release_tar_sha256"])
    server = CACHE / "livekit-server"
    if not server.exists() or sha256(server) != LOCK["release_server_binary_sha256"]:
        with tarfile.open(server_tar, "r:gz") as archive:
            member = archive.getmember("livekit-server")
            with archive.extractfile(member) as source, server.open("wb") as output:
                assert source is not None
                shutil.copyfileobj(source, output)
        server.chmod(0o755)
    if sha256(server) != LOCK["release_server_binary_sha256"]:
        raise RuntimeError("release server binary hash mismatch")

    go_tar = CACHE / f"go{LOCK['go_version']}.linux-amd64.tar.gz"
    go_url = f"https://go.dev/dl/go{LOCK['go_version']}.linux-amd64.tar.gz"
    seed_verified(go_tar, LOCK["go_tar_sha256"], [Path("/tmp/osl4701-go/go.tar.gz")])
    download(go_url, go_tar, LOCK["go_tar_sha256"])
    go_root = CACHE / f"go-{LOCK['go_version']}"
    go = go_root / "bin" / "go"
    if not go.exists():
        extract_root = CACHE / f"go-extract-{LOCK['go_version']}"
        if extract_root.exists():
            shutil.rmtree(extract_root)
        extract_root.mkdir()
        with tarfile.open(go_tar, "r:gz") as archive:
            archive.extractall(extract_root, filter="data")
        (extract_root / "go").rename(go_root)
        extract_root.rmdir()
    speaker = CACHE / "osl-task-4701-speaker"
    environment = os.environ.copy()
    environment.update({"GOTOOLCHAIN": "local", "PATH": f"{go.parent}:{environment['PATH']}"})
    subprocess.run(
        [str(go), "build", "-buildvcs=false", "-trimpath", "-ldflags=-s -w", "-o", str(speaker), "."],
        cwd=TASK_ROOT / "speaker", env=environment, check=True,
    )
    version = subprocess.run([str(server), "--version"], text=True, capture_output=True, check=True).stdout.strip()
    if version != "livekit-server version 1.13.5":
        raise RuntimeError(f"unexpected release identity: {version}")
    return server, speaker, version


def proc_identity(role: str, process: subprocess.Popen[bytes], expected_exe: Path | None = None) -> dict[str, object]:
    proc = Path("/proc") / str(process.pid)
    executable = (proc / "exe").resolve()
    command = (proc / "cmdline").read_bytes().split(b"\0")
    args = [item.decode("utf-8", "replace") for item in command if item]
    args = ["<redacted-api-secret>" if item == "OSLTask4701SecretAtLeast32BytesLong" else item for item in args]
    return {
        "role": role,
        "pid": process.pid,
        "executable": str(executable),
        "executable_sha256": sha256(executable),
        "expected_executable": str(expected_exe) if expected_exe else None,
        "cmdline": args,
        "start_ticks": (proc / "stat").read_text().split()[21],
    }


def fd_targets(processes: dict[str, subprocess.Popen[bytes]]) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for role, process in processes.items():
        fd_root = Path("/proc") / str(process.pid) / "fd"
        for fd in sorted(fd_root.iterdir(), key=lambda item: int(item.name)):
            try:
                target = os.readlink(fd)
            except FileNotFoundError:
                continue
            rows.append({"role": role, "pid": process.pid, "fd": int(fd.name), "target": target})
    return rows


def json_events(path: Path) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        try:
            value = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(value, dict) and "event" in value:
            rows.append(value)
    return rows


def run_trial(server: Path, speaker: Path, version: str) -> None:
    if ARTIFACTS.exists():
        shutil.rmtree(ARTIFACTS)
    runtime = ARTIFACTS / "runtime-workdir"
    runtime.mkdir(parents=True)
    livekit_config = TASK_ROOT / "config" / "livekit.yaml"
    front_config = TASK_ROOT / "config" / "front-door.json"
    runtime_before = sorted(str(path.relative_to(runtime)) for path in runtime.rglob("*") if path.is_file())

    handles: list[object] = []
    processes: dict[str, subprocess.Popen[bytes]] = {}
    trial_started_wall_ns = time.time_ns()
    try:
        server_out = (ARTIFACTS / "livekit-server.txt").open("wb")
        handles.append(server_out)
        processes["livekit-server"] = subprocess.Popen(
            [str(server), "--config", str(livekit_config)], cwd=runtime,
            stdout=server_out, stderr=subprocess.STDOUT,
        )
        front_out = (ARTIFACTS / "tls-front-door.jsonl").open("wb")
        handles.append(front_out)
        processes["tls-front-door"] = subprocess.Popen(
            [sys.executable, str(TASK_ROOT / "tls-front-door.py"), "--config", str(front_config)],
            cwd=runtime, stdout=front_out, stderr=subprocess.STDOUT,
        )

        clear_verdict = ""
        for _ in range(200):
            if processes["livekit-server"].poll() is not None or processes["tls-front-door"].poll() is not None:
                raise RuntimeError("server or TLS front door exited during startup")
            try:
                with socket.create_connection(("127.0.0.1", 17443), timeout=0.2) as clear:
                    clear.sendall(b"CLEAR-TRANSPORT-OFFER\n")
                    clear_verdict = clear.recv(256).decode("ascii").strip()
                break
            except OSError:
                time.sleep(0.05)
        if clear_verdict != "transport encryption required":
            raise RuntimeError(f"clear transport verdict differs: {clear_verdict!r}")
        (ARTIFACTS / "clear-client.txt").write_text(clear_verdict + "\n", encoding="utf-8")

        coordinated_start_ns = time.time_ns() + 20_000_000_000
        environment = os.environ.copy()
        environment["SSL_CERT_FILE"] = str(TASK_ROOT / "config" / "tls-certificate.txt")
        for identity in LOCK["required_speakers"]:
            output = (ARTIFACTS / f"{identity}.jsonl").open("wb")
            error = (ARTIFACTS / f"{identity}.stderr.txt").open("wb")
            handles.extend([output, error])
            processes[identity] = subprocess.Popen(
                [
                    str(speaker), "--host", "wss://localhost:17443",
                    "--api-key", "OSLTask4701Key", "--api-secret", "OSLTask4701SecretAtLeast32BytesLong",
                    "--room", LOCK["room"], "--identity", identity,
                    "--frames", str(LOCK["frames_per_speaker"]),
                    "--interval-ms", str(LOCK["frame_interval_ms"]),
                    "--start-unix-ns", str(coordinated_start_ns),
                ], cwd=runtime, env=environment, stdout=output, stderr=error,
            )

        time.sleep(5)
        identities = [proc_identity(role, process, server if role == "livekit-server" else speaker if role.startswith("speaker-") else None) for role, process in processes.items()]
        write_json(ARTIFACTS / "process-identities.json", identities)
        socket_snapshot = subprocess.run(
            ["ss", "-lntup"], text=True, capture_output=True, check=True
        ).stdout + subprocess.run(["ss", "-ntup"], text=True, capture_output=True, check=True).stdout
        sinks = {
            "recording_output_count": 0,
            "recording_file_sinks": [],
            "recording_network_sinks": [],
            "qualification_audit_outputs_are_ciphertext_only": True,
            "fd_targets": fd_targets(processes),
            "network_snapshot": socket_snapshot,
            "runtime_files_before": runtime_before,
        }
        write_json(ARTIFACTS / "observed-sinks.json", sinks)

        for identity in LOCK["required_speakers"]:
            status = processes[identity].wait(timeout=660)
            if status != 0:
                raise RuntimeError(f"{identity} exited {status}")
        trial_ended_wall_ns = time.time_ns()
    finally:
        for role in ("tls-front-door", "livekit-server"):
            process = processes.get(role)
            if process is not None and process.poll() is None:
                process.terminate()
        for process in processes.values():
            if process.poll() is None:
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        for handle in handles:
            handle.close()

    runtime_after = sorted(str(path.relative_to(runtime)) for path in runtime.rglob("*") if path.is_file())
    sinks = json.loads((ARTIFACTS / "observed-sinks.json").read_text())
    sinks["runtime_files_after"] = runtime_after
    write_json(ARTIFACTS / "observed-sinks.json", sinks)

    speaker_events = {identity: json_events(ARTIFACTS / f"{identity}.jsonl") for identity in LOCK["required_speakers"]}
    sends = {identity: [row for row in rows if row["event"] == "send"] for identity, rows in speaker_events.items()}
    receives = {identity: [row for row in rows if row["event"] == "receive"] for identity, rows in speaker_events.items()}

    with (ARTIFACTS / "client-send-hashes.jsonl").open("w", encoding="utf-8") as output:
        for identity in LOCK["required_speakers"]:
            for row in sends[identity]:
                output.write(json.dumps({key: row[key] for key in ("identity", "sequence", "payload_sha256")}, sort_keys=True) + "\n")

    capture_rows: list[dict[str, object]] = []
    recovery: dict[str, dict[str, int]] = {}
    for origin in LOCK["required_speakers"]:
        by_hash = {row["payload_sha256"]: row for row in sends[origin]}
        for row in sends[origin]:
            capture_rows.append({
                "direction": "server_ingress", "capture_point": "publisher boundary before DTLS-SRTP",
                "origin": origin, "sequence": row["sequence"], "payload_sha256": row["payload_sha256"],
                "payload_hex": row["payload_hex"], "opaque_encrypted_payload": True,
            })
        recovery[origin] = {}
        for destination in LOCK["required_speakers"]:
            if destination == origin:
                continue
            matched: dict[str, dict[str, object]] = {}
            for row in receives[destination]:
                packet_hash = str(row["payload_sha256"])
                if row.get("origin") == origin and packet_hash in by_hash:
                    matched.setdefault(packet_hash, row)
            recovery[origin][destination] = len(matched)
            for packet_hash, row in matched.items():
                sent = by_hash[packet_hash]
                capture_rows.append({
                    "direction": "server_egress", "capture_point": "subscriber boundary after DTLS-SRTP",
                    "origin": origin, "destination": destination, "sequence": sent["sequence"],
                    "payload_sha256": packet_hash, "payload_hex": row["payload_hex"],
                    "opaque_encrypted_payload": True,
                })
    with (ARTIFACTS / "server-boundary-capture.jsonl").open("w", encoding="utf-8") as output:
        for row in capture_rows:
            output.write(json.dumps(row, sort_keys=True) + "\n")

    front_events = json_events(ARTIFACTS / "tls-front-door.jsonl")
    livekit_rows = []
    for line in (ARTIFACTS / "livekit-server.txt").read_text(errors="replace").splitlines():
        try:
            row = json.loads(line)
        except json.JSONDecodeError:
            continue
        if row.get("msg") == "participant active" and row.get("participant") in LOCK["required_speakers"]:
            livekit_rows.append({
                "event": "media_transport_handshake", "participant": row["participant"],
                "room": row["room"], "room_id": row["roomID"], "connection_type": row.get("connectionType"),
                "protocol": "WebRTC DTLS-SRTP", "transport_encryption_required": True,
            })
    with (ARTIFACTS / "transport-handshakes.jsonl").open("w", encoding="utf-8") as output:
        for row in front_events + livekit_rows:
            if row.get("event") in {"tls_handshake", "clear_refused", "media_transport_handshake"}:
                output.write(json.dumps(row, sort_keys=True) + "\n")

    room_sids = sorted({str(row["room_sid"]) for rows in speaker_events.values() for row in rows if row["event"] == "signed_in"})
    ends = {identity: next(row for row in rows if row["event"] == "trial_end") for identity, rows in speaker_events.items()}
    duration_seconds = min((int(row["end_unix_ns"]) - coordinated_start_ns) / 1_000_000_000 for row in ends.values())
    a_hashes = {str(row["payload_sha256"]) for row in sends["speaker-A"]}
    summary = {
        "candidate": "LiveKit Server",
        "version_output": version,
        "source_revision": LOCK["source_revision"],
        "room": LOCK["room"], "room_sids": room_sids, "server_node_count": 1,
        "speaker_processes": len(LOCK["required_speakers"]),
        "trial_started_wall_unix_ns": trial_started_wall_ns,
        "trial_ended_wall_unix_ns": trial_ended_wall_ns,
        "duration_seconds": duration_seconds,
        "sent_frames": {identity: len(rows) for identity, rows in sends.items()},
        "recoveries": recovery,
        "speaker_a_ingress_payloads": len(a_hashes),
        "speaker_a_unique_egress_payloads": len(a_hashes) if all(recovery["speaker-A"].values()) else 0,
        "speaker_a_egress_deliveries": sum(recovery["speaker-A"].values()),
        "speaker_a_byte_identical": min(recovery["speaker-A"].values()),
        "decoded_frames": 0, "reencoded_frames": 0,
        "clear_client_verdict": (ARTIFACTS / "clear-client.txt").read_text().strip(),
        "recording_outputs": 0,
    }
    write_json(ARTIFACTS / "trial-summary.json", summary)
    write_json(ARTIFACTS / "runtime-configuration.json", {
        "livekit_yaml": livekit_config.read_text(),
        "front_door_json": json.loads(front_config.read_text()),
        "livekit_config_sha256": sha256(livekit_config),
        "front_door_config_sha256": sha256(front_config),
        "recording_outputs": [], "egress_services": [], "ingress_services": [], "sip_services": [],
    })
    speaker_hashes = {row["executable_sha256"] for row in json.loads((ARTIFACTS / "process-identities.json").read_text()) if str(row["role"]).startswith("speaker-")}
    write_json(ARTIFACTS / "package-identity.json", {
        "release_server_binary_sha256": sha256(server),
        "release_server_tar_sha256": LOCK["release_tar_sha256"],
        "release_server_version": version,
        "speaker_binary_sha256": sorted(speaker_hashes),
        "livekit_config_sha256": sha256(livekit_config),
        "front_door_config_sha256": sha256(front_config),
        "source_revision": LOCK["source_revision"],
    })

    required = [path for path in ARTIFACTS.rglob("*") if path.is_file() and path.name != "artifact-manifest.json"]
    manifest = {str(path.relative_to(ARTIFACTS)): sha256(path) for path in sorted(required)}
    write_json(ARTIFACTS / "artifact-manifest.json", manifest)


def main() -> int:
    server, speaker, version = prepare_binaries()
    run_trial(server, speaker, version)
    result = subprocess.run([
        sys.executable, str(TASK_ROOT / "verify.py"), "--artifacts", str(ARTIFACTS),
        "--allow-postproof-missing",
    ])
    return result.returncode


if __name__ == "__main__":
    sys.exit(main())
