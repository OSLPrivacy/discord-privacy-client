#!/usr/bin/env python3
"""Break TASK 4701 artifacts on purpose and prove the unchanged check goes red."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

TASK_ROOT = Path(__file__).resolve().parent
REPO_ROOT = TASK_ROOT.parents[1]
SOURCE = REPO_ROOT / "evidence" / "task-4701-forwarding-audio"
VERIFY = TASK_ROOT / "verify.py"


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rebind(root: Path) -> None:
    manifest = json.loads((root / "artifact-manifest.json").read_text())
    for name in list(manifest):
        path = root / name
        if path.is_file():
            manifest[name] = sha256(path)
    (root / "artifact-manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")


def rewrite_jsonl(path: Path, update) -> None:
    rows = [json.loads(line) for line in path.read_text().splitlines()]
    rows = update(rows)
    path.write_text("".join(json.dumps(row, sort_keys=True) + "\n" for row in rows))


def run_mutant(name: str, expected: str, mutate) -> None:
    with tempfile.TemporaryDirectory(prefix=f"osl-4701-{name}-") as temporary:
        root = Path(temporary) / "artifacts"
        shutil.copytree(SOURCE, root)
        mutate(root)
        rebind(root)
        result = subprocess.run(
            [sys.executable, str(VERIFY), "--artifacts", str(root)],
            text=True, capture_output=True,
        )
        combined = result.stdout + result.stderr
        if result.returncode != 1 or expected not in combined:
            raise SystemExit(
                f"TASK4701 mutant={name} expected exit=1 capability={expected}; "
                f"got exit={result.returncode} output={combined!r}"
            )
        print(f"TASK4701 MUTANT name={name} exit=1 named={expected}")


def main() -> int:
    if not SOURCE.is_dir():
        raise SystemExit("run qualify.py before mutants.py")

    for identity in ("speaker-A", "speaker-B", "speaker-C"):
        def starve(root: Path, identity: str = identity) -> None:
            path = root / f"{identity}.jsonl"
            rewrite_jsonl(path, lambda rows: [row for row in rows if not (row.get("event") == "send" and row.get("sequence") == 2999)])
        run_mutant(f"starve-{identity}", f"capability={identity}-frame-count", starve)

    def binary(root: Path) -> None:
        value = json.loads((root / "package-identity.json").read_text())
        value["release_server_binary_sha256"] = "00" * 32
        (root / "package-identity.json").write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    run_mutant("substitute-binary", "capability=release-binary-identity", binary)

    def configuration(root: Path) -> None:
        value = json.loads((root / "runtime-configuration.json").read_text())
        value["livekit_yaml"] += "\n# substituted\n"
        (root / "runtime-configuration.json").write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    run_mutant("substitute-configuration", "capability=release-configuration-identity", configuration)

    def omit(root: Path) -> None:
        (root / "transport-handshakes.jsonl").unlink()
    run_mutant("omit-artifact", "capability=artifact-completeness", omit)

    def decode(root: Path) -> None:
        path = root / "speaker-A.jsonl"
        def update(rows):
            for row in rows:
                if row.get("event") == "trial_end":
                    row["decoded_frames"] = 1
            return rows
        rewrite_jsonl(path, update)
    run_mutant("enable-decode-reencode", "capability=zero-decode-reencode", decode)

    def recording(root: Path) -> None:
        value = json.loads((root / "observed-sinks.json").read_text())
        value["recording_output_count"] = 1
        value["recording_file_sinks"] = ["room.wav"]
        (root / "observed-sinks.json").write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    run_mutant("enable-recording", "capability=no-recording-output", recording)

    def clear(root: Path) -> None:
        (root / "clear-client.txt").write_text("clear transport accepted\n")
    run_mutant("allow-clear-client", "capability=mandatory-transport-encryption", clear)

    def payload(root: Path) -> None:
        path = root / "speaker-B.jsonl"
        sent_hashes = {
            row["payload_sha256"]
            for row in (json.loads(line) for line in (root / "speaker-A.jsonl").read_text().splitlines())
            if row.get("event") == "send"
        }
        def update(rows):
            source = next(
                row for row in rows
                if row.get("event") == "receive"
                and row.get("origin") == "speaker-A"
                and row.get("payload_sha256") in sent_hashes
            )
            source["payload_hex"] = "00" + str(source["payload_hex"])[2:]
            return rows
        rewrite_jsonl(path, update)
    run_mutant("reencode-payload", "capability=payload-pass-through", payload)

    restored = subprocess.run([sys.executable, str(VERIFY), "--artifacts", str(SOURCE)], text=True, capture_output=True)
    if restored.returncode != 0:
        raise SystemExit(f"restored check is not green: {restored.stdout}{restored.stderr}")
    print("TASK4701 MUTANTS restored=green total=10")
    return 0


if __name__ == "__main__":
    sys.exit(main())
