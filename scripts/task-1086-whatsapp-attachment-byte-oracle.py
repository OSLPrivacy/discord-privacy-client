#!/usr/bin/env python3
"""Fail-closed byte oracle for a WhatsApp protected-file delivery receipt.

This deliberately accepts paths, not provider-displayed metadata.  The caller
must capture the sender fingerprint before send and pass the file saved after
the receiver's single open.  A copied label alone cannot make this check pass:
both files are read and hashed here, independently.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
from pathlib import Path


def fingerprint(path: Path, evidence: str) -> tuple[int, str]:
    try:
        with path.open("rb", buffering=0) as source:
            digest = hashlib.sha256()
            length = 0
            while block := source.read(1024 * 1024):
                length += len(block)
                digest.update(block)
    except (OSError, ValueError) as error:
        fail(f"WhatsApp missing {evidence}: {error}")
    if length == 0:
        fail(f"WhatsApp missing {evidence}: zero bytes")
    return length, digest.hexdigest()


def fail(message: str) -> None:
    print(f"TASK1086 FAIL {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--receiver", type=Path, required=True)
    parser.add_argument("--keep", type=Path, required=True)
    parser.add_argument("--source-length", type=int, required=True)
    parser.add_argument("--source-sha256", required=True)
    parser.add_argument("--keep-length", type=int, required=True)
    parser.add_argument("--keep-sha256", required=True)
    parser.add_argument("--receiver-open-count", type=int, required=True)
    parser.add_argument("--cover-observation", required=True)
    args = parser.parse_args()

    if args.receiver_open_count != 1:
        fail("WhatsApp missing one-time open evidence: receiver_open_count must equal 1")
    if args.cover_observation != "cover-only":
        fail("WhatsApp missing cover observation: expected cover-only")
    if len(args.source_sha256) != 64 or any(ch not in "0123456789abcdef" for ch in args.source_sha256):
        fail("WhatsApp missing independent source hash")
    if len(args.keep_sha256) != 64 or any(ch not in "0123456789abcdef" for ch in args.keep_sha256):
        fail("WhatsApp missing keep-file hash")

    # Read the sender source and the receiver-opened bytes independently.  Do
    # not reuse the supplied labels as fingerprints; they are expectations.
    source_length, source_hash = fingerprint(args.source, "source bytes")
    received_length, received_hash = fingerprint(args.receiver, "receiver bytes")
    keep_length, keep_hash = fingerprint(args.keep, "keep file")

    if source_length != args.source_length or source_hash != args.source_sha256:
        fail("WhatsApp source changed after its fingerprint was frozen "
             f"expected_length={args.source_length} received_length={source_length} "
             f"expected_sha256={args.source_sha256} received_sha256={source_hash}")
    if keep_length != args.keep_length or keep_hash != args.keep_sha256:
        fail("WhatsApp keep file changed "
             f"expected_length={args.keep_length} received_length={keep_length} "
             f"expected_sha256={args.keep_sha256} received_sha256={keep_hash}")
    if received_length != args.source_length or received_hash != args.source_sha256:
        fail("WhatsApp received bytes mismatch "
             f"expected_length={args.source_length} received_length={received_length} "
             f"expected_sha256={args.source_sha256} received_sha256={received_hash}")

    print("TASK1086 PASS carrier=WhatsApp "
          f"source_length={source_length} receiver_length={received_length} "
          f"sha256={source_hash} receiver_open_count=1 cover_observation=cover-only "
          f"keep_length={keep_length} keep_sha256={keep_hash}")


if __name__ == "__main__":
    main()
