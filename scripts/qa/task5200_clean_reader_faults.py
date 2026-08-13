#!/usr/bin/env python3
"""Independent hostile-archive and ownership proof for TASK 5200b."""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import struct
import subprocess
import sys
import tempfile
from copy import deepcopy
from pathlib import Path
from typing import Callable

from nacl.bindings import crypto_aead_xchacha20poly1305_ietf_decrypt
from nacl.bindings import crypto_aead_xchacha20poly1305_ietf_encrypt


INFO = b"org.openstandardlibraries.account-export.archive-key.v1"
MAGIC = b"OSLXPORT"
BLOCK_BYTES = 65_536


def decode_base64(value: str) -> bytes:
    return base64.b64decode(value + "=" * (-len(value) % 4))


def sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def derive_key(header: dict, key_file: dict) -> bytes:
    salt = decode_base64(header["kdf"]["saltBase64"])
    secret = decode_base64(key_file["secretBase64"])
    extract = hmac.new(salt, secret, hashlib.sha256).digest()
    return hmac.new(extract, INFO + b"\x01", hashlib.sha256).digest()


def parse_archive(blob: bytes, key_file: dict) -> tuple[dict, list[dict]]:
    if blob[:8] != MAGIC or struct.unpack(">H", blob[8:10])[0] != 1:
        raise AssertionError("fixture format")
    header_length = struct.unpack(">I", blob[10:14])[0]
    header_bytes = blob[14 : 14 + header_length]
    header = json.loads(header_bytes)
    cursor = 14 + header_length
    count = struct.unpack(">Q", blob[cursor : cursor + 8])[0]
    cursor += 8
    key = derive_key(header, key_file)
    prefix = decode_base64(header["noncePrefixBase64"])
    header_hash = hashlib.sha256(header_bytes).digest()
    frames: list[dict] = []
    for _ in range(count):
        start = cursor
        index = struct.unpack(">Q", blob[cursor : cursor + 8])[0]
        kind = blob[cursor + 8]
        plain_length = struct.unpack(">I", blob[cursor + 9 : cursor + 13])[0]
        cipher_length = struct.unpack(">I", blob[cursor + 13 : cursor + 17])[0]
        ciphertext = blob[cursor + 17 : cursor + 17 + cipher_length]
        cursor += 17 + cipher_length
        aad = (
            header_hash
            + index.to_bytes(8, "big")
            + bytes([kind])
            + plain_length.to_bytes(4, "big")
        )
        plaintext = crypto_aead_xchacha20poly1305_ietf_decrypt(
            ciphertext, aad, prefix + index.to_bytes(8, "big"), key
        )
        frames.append(
            {
                "index": index,
                "kind": kind,
                "plaintext": plaintext,
                "start": start,
                "end": cursor,
            }
        )
    if cursor != len(blob):
        raise AssertionError("fixture has trailing bytes")
    return header, frames


def pack_archive(header: dict, frames: list[dict], key_file: dict) -> bytes:
    header_bytes = json.dumps(header, separators=(",", ":")).encode()
    header_hash = hashlib.sha256(header_bytes).digest()
    key = derive_key(header, key_file)
    prefix = decode_base64(header["noncePrefixBase64"])
    result = MAGIC + struct.pack(">HI", 1, len(header_bytes)) + header_bytes
    result += struct.pack(">Q", len(frames))
    for index, frame in enumerate(frames):
        kind = frame["kind"]
        plaintext = frame["plaintext"]
        aad = (
            header_hash
            + index.to_bytes(8, "big")
            + bytes([kind])
            + len(plaintext).to_bytes(4, "big")
        )
        ciphertext = crypto_aead_xchacha20poly1305_ietf_encrypt(
            plaintext, aad, prefix + index.to_bytes(8, "big"), key
        )
        result += struct.pack(">QBII", index, kind, len(plaintext), len(ciphertext))
        result += ciphertext
    return result


def unpack_objects(manifest: dict, frames: list[dict]) -> dict[str, dict]:
    result: dict[str, dict] = {}
    by_index = {frame["index"]: frame["plaintext"] for frame in frames}
    for item in manifest["objects"]:
        entries = [
            entry
            for entry in manifest["blocks"]
            if entry["class"] == item["class"] and entry["objectId"] == item["id"]
        ]
        value = b"".join(by_index[entry["index"]] for entry in entries)
        result[f'{item["class"]}:{item["id"]}'] = {
            "class": item["class"],
            "id": item["id"],
            "metadata": deepcopy(item["metadata"]),
            "bytes": value,
        }
    return result


def rebuild_archive(
    header: dict,
    frames: list[dict],
    key_file: dict,
    change: Callable[[dict[str, dict]], None],
) -> bytes:
    manifest = json.loads(frames[0]["plaintext"])
    objects = unpack_objects(manifest, frames)
    change(objects)

    classes = {name: 0 for name in manifest["classes"]}
    rebuilt_objects: list[dict] = []
    rebuilt_entries: list[dict] = []
    rebuilt_frames: list[dict] = []
    next_index = 1
    total = 0
    for item in objects.values():
        value = item["bytes"]
        chunks = [value[offset : offset + BLOCK_BYTES] for offset in range(0, len(value), BLOCK_BYTES)]
        if not chunks:
            chunks = [b""]
        first_block = next_index
        for position, chunk in enumerate(chunks):
            rebuilt_entries.append(
                {
                    "index": next_index,
                    "class": item["class"],
                    "objectId": item["id"],
                    "objectOffset": position * BLOCK_BYTES,
                    "plaintextBytes": len(chunk),
                    "sha256": sha256(chunk),
                    "finalBlockForObject": position + 1 == len(chunks),
                }
            )
            rebuilt_frames.append({"kind": 1, "plaintext": chunk})
            next_index += 1
        rebuilt_objects.append(
            {
                "class": item["class"],
                "id": item["id"],
                "byteCount": len(value),
                "sha256": sha256(value),
                "firstBlock": first_block,
                "blockCount": len(chunks),
                "metadata": item["metadata"],
            }
        )
        classes[item["class"]] += 1
        total += len(value)
    manifest["classes"] = classes
    manifest["objects"] = rebuilt_objects
    manifest["blocks"] = rebuilt_entries
    manifest["totalPlaintextBytes"] = total
    manifest_frame = {
        "kind": 0,
        "plaintext": json.dumps(manifest, separators=(",", ":")).encode(),
    }
    return pack_archive(header, [manifest_frame, *rebuilt_frames], key_file)


def run_reader(*arguments: Path | str) -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        [str(argument) for argument in arguments],
        capture_output=True,
        timeout=20,
    )


def require_rejection(name: str, result: subprocess.CompletedProcess[bytes], token: str) -> None:
    if result.returncode != 1 or result.stdout != b"":
        raise AssertionError(
            f"{name}: exit={result.returncode} stdout={result.stdout!r} stderr={result.stderr!r}"
        )
    if token.lower().encode() not in result.stderr.lower():
        raise AssertionError(f"{name}: missing {token!r}: {result.stderr!r}")
    print(
        f"TASK5200B_READER={name}|exit=1|plaintext_stdout_bytes=0|"
        f"diagnostic={result.stderr.decode(errors='replace').strip()}"
    )


def main() -> None:
    if len(sys.argv) != 7:
        raise SystemExit(
            "usage: task5200_clean_reader_faults.py "
            "READER ARCHIVE KEY ORACLE OTHER_ARCHIVE OTHER_KEY"
        )
    reader, archive, key, oracle, other_archive, other_key = map(Path, sys.argv[1:])

    control = run_reader(reader, archive, key, oracle)
    if control.returncode != 0:
        raise SystemExit(control.stderr.decode(errors="replace"))
    report = json.loads(control.stdout)
    classes = report["classes"]
    objects = report["objects"]
    if (
        classes.get("messages") != 41
        or classes.get("attachments") != 7
        or len(objects) != 54
        or "messages:message-040" not in objects
        or "attachments:attachment-06" not in objects
        or sum(item["byteCount"] for item in objects.values()) != report["plaintextBytes"]
    ):
        raise SystemExit("control did not recover every oracle-named byte")
    distinct = run_reader(reader, "--compare", archive, key, other_archive, other_key)
    if distinct.returncode != 0:
        raise SystemExit(distinct.stderr.decode(errors="replace"))
    print(
        "TASK5200B_CONTROL=reachable|files=2|full_readbacks=2|"
        f"authenticated_blocks={report['authenticatedBlocks']}|objects=54|"
        f"messages=41|attachments=7|plaintext_bytes={report['plaintextBytes']}|"
        "final_message=message-040|final_attachment=attachment-06|nonce_material=distinct"
    )

    original = archive.read_bytes()
    key_file = json.loads(key.read_text())
    header, frames = parse_archive(original, key_file)
    manifest = json.loads(frames[0]["plaintext"])
    if len(frames) < 20 or manifest["classes"]["messages"] != 41:
        raise AssertionError("fixture did not cross the page/final-block boundary")

    temp_path: Path | None = None
    rejected = 0
    with tempfile.TemporaryDirectory(prefix="task5200b-reader-") as raw_temp:
        temp_path = Path(raw_temp)

        cases: list[tuple[str, bytes, Path, str]] = []

        def add(name: str, blob: bytes, selected_key: Path, token: str) -> None:
            cases.append((name, blob, selected_key, token))

        add("osl-held-key-unavailable", original, temp_path / "deleted.key", "unavailable key")
        add("wrong-key", original, other_key, "cryptographic parameters")

        fieldless_header = deepcopy(header)
        del fieldless_header["aead"]
        fieldless_bytes = json.dumps(fieldless_header, separators=(",", ":")).encode()
        header_length = struct.unpack(">I", original[10:14])[0]
        fieldless = (
            original[:10]
            + struct.pack(">I", len(fieldless_bytes))
            + fieldless_bytes
            + original[14 + header_length :]
        )
        add(
            "missing-required-format-field",
            fieldless,
            key,
            "undocumented or missing required header field",
        )

        def first_page(objects: dict[str, dict]) -> None:
            removed_messages = {
                f"message-{index:03}" for index in range(16, 41)
            }
            for object_key in list(objects):
                item = objects[object_key]
                if item["class"] == "messages" and item["id"] in removed_messages:
                    del objects[object_key]
                elif (
                    item["class"] == "attachments"
                    and item["metadata"]["messageId"] in removed_messages
                ):
                    del objects[object_key]

        add(
            "first-production-page-only",
            rebuild_archive(header, frames, key_file, first_page),
            key,
            "page boundary: missing item attachments:attachment-04",
        )

        def truncate_message_41(objects: dict[str, dict]) -> None:
            item = objects["messages:message-040"]
            document = json.loads(item["bytes"])
            document["fields"]["body"] = document["fields"]["body"][:-1]
            item["bytes"] = json.dumps(document, separators=(",", ":")).encode()

        add(
            "truncate-only-message-41",
            rebuild_archive(header, frames, key_file, truncate_message_41),
            key,
            "hash/byte-count mismatch messages:message-040",
        )

        def drop_attachment_7(objects: dict[str, dict]) -> None:
            del objects["attachments:attachment-06"]
            message = objects["messages:message-040"]
            document = json.loads(message["bytes"])
            document["attachmentIds"] = []
            message["bytes"] = json.dumps(document, separators=(",", ":")).encode()

        add(
            "drop-only-attachment-7",
            rebuild_archive(header, frames, key_file, drop_attachment_7),
            key,
            "missing item attachments:attachment-06",
        )

        def drop_settings_class(objects: dict[str, dict]) -> None:
            del objects["settings:settings"]

        add(
            "drop-production-class-settings",
            rebuild_archive(header, frames, key_file, drop_settings_class),
            key,
            "missing class settings",
        )

        def inject_foreign_owner(objects: dict[str, dict]) -> None:
            item = objects["messages:message-000"]
            document = json.loads(item["bytes"])
            document["ownerId"] = "second-person-private-canary"
            document["fields"]["privateCanary"] = "SECOND-PERSON-PRIVATE-CANARY"
            item["bytes"] = json.dumps(document, separators=(",", ":")).encode()

        add(
            "seeded-second-person-private-canary",
            rebuild_archive(header, frames, key_file, inject_foreign_owner),
            key,
            "foreign owner",
        )

        flipped = bytearray(original)
        flipped[-9] ^= 0x80
        add("flipped-ciphertext-bit", bytes(flipped), key, "block authentication")

        first = frames[1]
        second = frames[2]
        reordered = (
            original[: first["start"]]
            + original[second["start"] : second["end"]]
            + original[first["start"] : first["end"]]
            + original[second["end"] :]
        )
        add("reordered-authenticated-blocks", reordered, key, "reordered authenticated block")
        add("torn-final-authenticated-block", original[:-1], key, "truncated archive")

        skipped = frames[3]
        add(
            "independently-skipped-nonmanifest-block",
            original[: skipped["start"]] + original[skipped["end"] :],
            key,
            "reordered authenticated block",
        )

        for name, blob, selected_key, diagnostic in cases:
            candidate = temp_path / f"{name}.oslexport"
            candidate.write_bytes(blob)
            require_rejection(
                name,
                run_reader(reader, candidate, selected_key, oracle),
                diagnostic,
            )
            rejected += 1

        reused_archive = temp_path / "reused-material.oslexport"
        reused_key = temp_path / "reused-material.key.json"
        reused_archive.write_bytes(original)
        reused_key.write_bytes(key.read_bytes())
        require_rejection(
            "repeated-export-nonce-reuse",
            run_reader(reader, "--compare", archive, key, reused_archive, reused_key),
            "nonce reuse",
        )
        rejected += 1

    if temp_path is None or temp_path.exists():
        raise AssertionError("reader throwaway directory was not discarded")
    print(f"TASK5200B_READER_MUTATIONS={rejected}|remaining_archives_keys=0")


if __name__ == "__main__":
    main()
