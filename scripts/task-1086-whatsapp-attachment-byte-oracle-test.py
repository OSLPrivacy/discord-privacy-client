#!/usr/bin/env python3
"""Focused executable proof for TASK 1086's received-byte oracle."""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
ORACLE = ROOT / "task-1086-whatsapp-attachment-byte-oracle.py"


def digest(path: Path) -> tuple[int, str]:
    data = path.read_bytes()
    return len(data), hashlib.sha256(data).hexdigest()


def invoke(source: Path, receiver: Path, keep: Path, opens: int = 1, cover: str = "cover-only") -> subprocess.CompletedProcess[str]:
    source_length, source_hash = digest(source)  # frozen before delivery
    keep_length, keep_hash = digest(keep)        # frozen before delivery
    return subprocess.run([sys.executable, str(ORACLE), "--source", str(source), "--receiver", str(receiver), "--keep", str(keep),
        "--source-length", str(source_length), "--source-sha256", source_hash,
        "--keep-length", str(keep_length), "--keep-sha256", keep_hash,
        "--receiver-open-count", str(opens), "--cover-observation", cover], text=True, capture_output=True, check=False)


def must(result: subprocess.CompletedProcess[str], code: int, words: tuple[str, ...]) -> None:
    output = result.stdout + result.stderr
    if result.returncode != code or any(word not in output for word in words):
        raise AssertionError(f"exit={result.returncode} expected={code} words={words!r} output={output!r}")


with tempfile.TemporaryDirectory(prefix="task1086-") as directory:
    root = Path(directory)
    source, receiver, keep = root / "sender.protected", root / "receiver-opened.protected", root / "unrelated.keep"
    source.write_bytes(os.urandom(65537))
    keep.write_bytes(os.urandom(4099))
    shutil.copyfile(source, receiver)  # receiver's independently persisted opened bytes
    baseline = invoke(source, receiver, keep)
    must(baseline, 0, ("carrier=WhatsApp", "receiver_open_count=1", "cover_observation=cover-only"))
    print(baseline.stdout.strip())

    # Freeze source first; changing delivery bytes must expose both hashes.
    frozen_length, frozen_hash = digest(source)
    altered = bytearray(receiver.read_bytes()); altered[0] ^= 1; receiver.write_bytes(altered)
    mutation = subprocess.run([sys.executable, str(ORACLE), "--source", str(source), "--receiver", str(receiver), "--keep", str(keep),
        "--source-length", str(frozen_length), "--source-sha256", frozen_hash,
        "--keep-length", str(digest(keep)[0]), "--keep-sha256", digest(keep)[1],
        "--receiver-open-count", "1", "--cover-observation", "cover-only"], text=True, capture_output=True, check=False)
    must(mutation, 1, ("WhatsApp", "expected_sha256=", "received_sha256="))
    print("TASK1086 RED mutation_exit=1 " + mutation.stderr.strip())

    receiver.write_bytes(b"")
    zero = invoke(source, receiver, keep)
    must(zero, 1, ("WhatsApp", "receiver bytes", "zero bytes"))
    print("TASK1086 RED zero_bytes_exit=1 " + zero.stderr.strip())

    # Every named non-byte witness is mandatory and must fail closed.
    shutil.copyfile(source, receiver)
    source_length, source_hash = digest(source)
    keep_length, keep_hash = digest(keep)
    def raw(source_arg: Path, receiver_arg: Path, source_digest: str = source_hash) -> subprocess.CompletedProcess[str]:
        return subprocess.run([sys.executable, str(ORACLE), "--source", str(source_arg), "--receiver", str(receiver_arg), "--keep", str(keep),
            "--source-length", str(source_length), "--source-sha256", source_digest,
            "--keep-length", str(keep_length), "--keep-sha256", keep_hash,
            "--receiver-open-count", "1", "--cover-observation", "cover-only"], text=True, capture_output=True, check=False)
    for label, result in (("source-bytes", raw(root / "absent-source", receiver)), ("receiver-bytes", raw(source, root / "absent-receiver")), ("independent-hash", raw(source, receiver, "label-only"))):
        must(result, 1, ("WhatsApp", "missing"))
        print(f"TASK1086 RED starvation={label} exit=1 {result.stderr.strip()}")
    for label, result in (("one-time-open", invoke(source, receiver, keep, opens=0)), ("cover-observation", invoke(source, receiver, keep, cover="missing"))):
        must(result, 1, ("WhatsApp", "missing"))
        print(f"TASK1086 RED starvation={label} exit=1 {result.stderr.strip()}")
    keep.write_bytes(os.urandom(4099))
    # Supply the original keep expectation rather than a new label.
    # This branch uses a second independent keep file and its pre-send hash.
    keep2 = root / "unrelated-keep-before"; keep2.write_bytes(os.urandom(2049))
    receiver2 = root / "receiver2"; shutil.copyfile(source, receiver2)
    keep2_length, keep2_hash = digest(keep2); keep2.write_bytes(os.urandom(2049))
    changed_keep = subprocess.run([sys.executable, str(ORACLE), "--source", str(source), "--receiver", str(receiver2), "--keep", str(keep2),
        "--source-length", str(digest(source)[0]), "--source-sha256", digest(source)[1], "--keep-length", str(keep2_length), "--keep-sha256", keep2_hash,
        "--receiver-open-count", "1", "--cover-observation", "cover-only"], text=True, capture_output=True, check=False)
    must(changed_keep, 1, ("WhatsApp", "keep file changed"))
    print("TASK1086 RED starvation=keep-file exit=1 " + changed_keep.stderr.strip())
