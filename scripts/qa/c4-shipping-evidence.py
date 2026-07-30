#!/usr/bin/env python3
"""Fail-closed verifier for one owner-approved C4 shipping send.

The verifier is intentionally independent of Discord and OSL.  The Windows
collector writes one bounded JSON bundle and one PNG; this program decides only
whether that bundle contains every required, mutually consistent fact.

It never treats a QA-shell trail or receipt as production evidence.  Shipping
has no QA trail, so executable bytes, the production renderer success gate, and
independent UI Automation observations are the authorities.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import hashlib
import hmac
import json
import re
import struct
import sys
import uuid
import zlib
from pathlib import Path
from typing import Any


SCHEMA = "osl-c4-shipping-evidence-v1"
NATIVE_RECEIPT_SCHEMA = "osl.c4.native-placement-receipt"
NATIVE_RESULT_SCHEMA = "osl.c4.native-verification-result"
MAX_BUNDLE_BYTES = 256 * 1024
MAX_RUN_MS = 10 * 60 * 1000
MAX_CARRIER_BYTES = 8 * 1024
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
QA_MARKERS = (
    b"discord-qa-shell",
    b"discord-qa-shell-v1",
    b"send_native_discord_qa_atomic_text",
    b"discord-qa-send-stage-receipt.json",
    b"osl-discord-qa-send-stage.txt",
)
EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()
SUCCESS_STATUS = (
    "Sent privately through OSL. "
    "Discord received only the private-message marker."
)
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
PNG_CHANNELS = {2: 3, 6: 4}
MIN_SCREENSHOT_WIDTH = 64
MIN_SCREENSHOT_HEIGHT = 64
MIN_SCREENSHOT_DISTINCT_COLORS = 8
FILESYSTEM_ATTESTATION_KEYS = {
    "schema",
    "version",
    "authoritySource",
    "challenge",
    "checkedAtUnixMs",
    "ledgerRoot",
    "ledgerRecordPath",
    "rootIdentity",
    "daclSha256",
    "ownerSidSha256",
}
ROOT_IDENTITY_KEYS = {"volumeSerialNumber", "fileIndex"}
WINDOWS_DIRECTORY_RE = re.compile(
    r"^[A-Za-z]:\\(?:[^\\/:*?\"<>|\x00]+\\)*[^\\/:*?\"<>|\x00]+$"
)
WINDOWS_LEDGER_RECORD_RE = re.compile(
    r"^[A-Za-z]:\\(?:[^\\/:*?\"<>|\x00]+\\)*[0-9a-f]{64}\.json$"
)
WINDOWS_RESERVED_NAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{index}" for index in range(1, 10)),
    *(f"LPT{index}" for index in range(1, 10)),
}


class EvidenceError(ValueError):
    """A missing, ambiguous, stale, or contradictory evidence fact."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _object(value: Any, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise EvidenceError(f"{label} must be an object")
    return value


def _list(value: Any, label: str) -> list[Any]:
    if not isinstance(value, list):
        raise EvidenceError(f"{label} must be an array")
    return value


def _string(value: Any, label: str, *, maximum: int = 512) -> str:
    if not isinstance(value, str) or not value or len(value) > maximum:
        raise EvidenceError(f"{label} must be a non-empty bounded string")
    if any(ord(char) < 0x20 for char in value):
        raise EvidenceError(f"{label} contains a control character")
    return value


def _integer(value: Any, label: str, *, minimum: int = 0) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < minimum:
        raise EvidenceError(f"{label} must be an integer >= {minimum}")
    return value


def _boolean(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        raise EvidenceError(f"{label} must be boolean")
    return value


def _sha256(value: Any, label: str) -> str:
    text = _string(value, label, maximum=64)
    if not SHA256_RE.fullmatch(text):
        raise EvidenceError(f"{label} must be lowercase SHA-256")
    return text


def _canonical_json(value: Any) -> bytes:
    try:
        return json.dumps(
            value,
            ensure_ascii=False,
            allow_nan=False,
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
    except (TypeError, ValueError, UnicodeEncodeError) as error:
        raise EvidenceError("native receipt cannot be canonically encoded") from error


def _domain_sha256(domain: bytes, value: Any) -> str:
    return hashlib.sha256(domain + _canonical_json(value)).hexdigest()


def _receipt_object_digest(receipt: dict[str, Any]) -> str:
    body = dict(receipt)
    body.pop("receiptDigestSha256", None)
    return _domain_sha256(b"OSL/C4/native-placement-receipt/v3\x00", body)


def _target_binding_digest(target: dict[str, Any]) -> str:
    body = dict(target)
    body.pop("bindingSha256", None)
    return _domain_sha256(b"OSL/C4/native-target-binding/v3\x00", body)


def _pipe_binding_digest(process: dict[str, Any]) -> str:
    return _domain_sha256(b"OSL/C4/pipe-client-binding/v1\x00", process)


def _challenge_digest(challenge: str) -> str:
    return hashlib.sha256(bytes.fromhex(challenge)).hexdigest()


def _ledger_record_digest(record: dict[str, Any]) -> str:
    body = dict(record)
    body.pop("recordDigestSha256", None)
    return _domain_sha256(b"OSL/C4/challenge-ledger/v2\x00", body)


def _exact_keys(value: dict[str, Any], label: str, expected: set[str]) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise EvidenceError(f"{label} keys differ; missing={missing}, extra={extra}")


def _at(
    value: Any,
    label: str,
    run_start: int,
    run_end: int,
    *,
    after: int | None = None,
    before: int | None = None,
) -> int:
    observed = _integer(value, label, minimum=1)
    lower = run_start if after is None else max(run_start, after)
    upper = run_end if before is None else min(run_end, before)
    if observed < lower or observed > upper:
        raise EvidenceError(f"{label} is outside its run/order window")
    return observed


def _hash_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _resolve_artifact(bundle_path: Path, value: Any, label: str) -> Path:
    relative = Path(_string(value, label, maximum=240))
    if relative.is_absolute():
        raise EvidenceError(f"{label} must be relative to the evidence bundle")
    root = bundle_path.parent.resolve()
    resolved = (root / relative).resolve()
    try:
        resolved.relative_to(root)
    except ValueError as error:
        raise EvidenceError(f"{label} escapes the evidence bundle") from error
    if not resolved.is_file():
        raise EvidenceError(f"{label} is missing")
    return resolved


def _scan_for_markers(path: Path) -> None:
    longest = max(map(len, QA_MARKERS))
    carry = b""
    with path.open("rb") as handle:
        while True:
            block = handle.read(1024 * 1024)
            if not block:
                break
            searchable = (carry + block).lower()
            for marker in QA_MARKERS:
                if marker.lower() in searchable:
                    raise EvidenceError(
                        f"executable contains forbidden QA-shell marker: "
                        f"{marker.decode('ascii')}"
                    )
            carry = searchable[-(longest - 1) :]


def _validate_windows_path(value: Any, label: str, *, record: bool = False) -> str:
    pattern = WINDOWS_LEDGER_RECORD_RE if record else WINDOWS_DIRECTORY_RE
    path = _string(value, label, maximum=1100 if record else 1024)
    parts = path[3:].split("\\")
    if (
        not path.isascii()
        or not path.isprintable()
        or pattern.fullmatch(path) is None
        or "/" in path
        or "\\\\" in path
        or any(
            not part
            or part in {".", ".."}
            or part.endswith((".", " "))
            or part.split(".", 1)[0].upper() in WINDOWS_RESERVED_NAMES
            for part in parts
        )
    ):
        raise EvidenceError(f"{label} is not a canonical Windows path")
    return path


def _verify_filesystem_attestation(
    value: Any,
    *,
    challenge: str,
    emitted_at: int,
    run_end: int,
) -> dict[str, Any]:
    attestation = _object(
        value,
        "nativeAuthority.filesystemAuthorityAttestation",
    )
    _exact_keys(
        attestation,
        "nativeAuthority.filesystemAuthorityAttestation",
        FILESYSTEM_ATTESTATION_KEYS,
    )
    if (
        attestation["schema"] != "osl.c4.filesystem-authority-attestation"
        or attestation["version"] != 3
        or attestation["authoritySource"] != "native_windows_owner"
    ):
        raise EvidenceError("native filesystem attestation identity is invalid")
    if _sha256(attestation["challenge"], "nativeAuthority.filesystemAuthorityAttestation.challenge") != challenge:
        raise EvidenceError("native filesystem attestation challenge does not match")
    checked_at = _at(
        attestation["checkedAtUnixMs"],
        "nativeAuthority.filesystemAuthorityAttestation.checkedAtUnixMs",
        emitted_at,
        run_end,
    )
    root = _validate_windows_path(
        attestation["ledgerRoot"],
        "nativeAuthority.filesystemAuthorityAttestation.ledgerRoot",
    )
    record_path = _validate_windows_path(
        attestation["ledgerRecordPath"],
        "nativeAuthority.filesystemAuthorityAttestation.ledgerRecordPath",
        record=True,
    )
    expected_record_path = root + "\\" + _challenge_digest(challenge) + ".json"
    if record_path != expected_record_path:
        raise EvidenceError("native filesystem attestation names another ledger record")
    identity = _object(
        attestation["rootIdentity"],
        "nativeAuthority.filesystemAuthorityAttestation.rootIdentity",
    )
    _exact_keys(
        identity,
        "nativeAuthority.filesystemAuthorityAttestation.rootIdentity",
        ROOT_IDENTITY_KEYS,
    )
    for key in ROOT_IDENTITY_KEYS:
        _integer(
            identity[key],
            f"nativeAuthority.filesystemAuthorityAttestation.rootIdentity.{key}",
            minimum=1,
        )
    _sha256(attestation["daclSha256"], "nativeAuthority.filesystemAuthorityAttestation.daclSha256")
    _sha256(attestation["ownerSidSha256"], "nativeAuthority.filesystemAuthorityAttestation.ownerSidSha256")
    return {
        "checkedAtUnixMs": checked_at,
        "ledgerRoot": root,
        "ledgerRecordPath": record_path,
    }


def _paeth(left: int, up: int, upper_left: int) -> int:
    prediction = left + up - upper_left
    left_distance = abs(prediction - left)
    up_distance = abs(prediction - up)
    upper_left_distance = abs(prediction - upper_left)
    if left_distance <= up_distance and left_distance <= upper_left_distance:
        return left
    if up_distance <= upper_left_distance:
        return up
    return upper_left


def _png_facts(path: Path) -> dict[str, int]:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise EvidenceError("screenshot bytes could not be read") from error
    if not data.startswith(PNG_SIGNATURE):
        raise EvidenceError("screenshot PNG signature is missing")

    offset = len(PNG_SIGNATURE)
    ihdr: tuple[int, int, int, int, int, int, int] | None = None
    compressed = bytearray()
    saw_iend = False
    chunk_index = 0
    while offset < len(data):
        if offset + 12 > len(data):
            raise EvidenceError("screenshot PNG is truncated")
        length = struct.unpack(">I", data[offset : offset + 4])[0]
        chunk_type = data[offset + 4 : offset + 8]
        chunk_end = offset + 12 + length
        if chunk_end > len(data):
            raise EvidenceError("screenshot PNG chunk is truncated")
        chunk_data = data[offset + 8 : offset + 8 + length]
        claimed_crc = struct.unpack(">I", data[offset + 8 + length : chunk_end])[0]
        actual_crc = zlib.crc32(chunk_type)
        actual_crc = zlib.crc32(chunk_data, actual_crc) & 0xFFFFFFFF
        if actual_crc != claimed_crc:
            raise EvidenceError(f"screenshot PNG CRC mismatch in {chunk_type!r}")

        if chunk_index == 0 and chunk_type != b"IHDR":
            raise EvidenceError("screenshot PNG IHDR is not first")
        if chunk_type == b"IHDR":
            if ihdr is not None or length != 13:
                raise EvidenceError("screenshot PNG IHDR is invalid")
            ihdr = struct.unpack(">IIBBBBB", chunk_data)
        elif chunk_type == b"IDAT":
            compressed.extend(chunk_data)
        elif chunk_type == b"IEND":
            if length != 0:
                raise EvidenceError("screenshot PNG IEND is invalid")
            saw_iend = True
            offset = chunk_end
            break
        offset = chunk_end
        chunk_index += 1

    if ihdr is None or not compressed or not saw_iend or offset != len(data):
        raise EvidenceError("screenshot PNG is missing required chunks")
    width, height, bit_depth, color_type, compression, filtering, interlace = ihdr
    if width < 1 or height < 1:
        raise EvidenceError("screenshot PNG dimensions are empty")
    if (
        bit_depth != 8
        or color_type not in PNG_CHANNELS
        or compression != 0
        or filtering != 0
        or interlace != 0
    ):
        raise EvidenceError("screenshot PNG encoding is unsupported")

    channels = PNG_CHANNELS[color_type]
    row_bytes = width * channels
    try:
        inflated = zlib.decompress(bytes(compressed))
    except zlib.error as error:
        raise EvidenceError("screenshot PNG IDAT cannot be decompressed") from error
    expected = height * (row_bytes + 1)
    if len(inflated) != expected:
        raise EvidenceError("screenshot PNG inflated size is incoherent")

    rows: list[bytes] = []
    previous = bytes(row_bytes)
    cursor = 0
    for _ in range(height):
        filter_type = inflated[cursor]
        encoded = inflated[cursor + 1 : cursor + 1 + row_bytes]
        cursor += row_bytes + 1
        decoded = bytearray(row_bytes)
        for index, value in enumerate(encoded):
            left = decoded[index - channels] if index >= channels else 0
            up = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_type == 0:
                predictor = 0
            elif filter_type == 1:
                predictor = left
            elif filter_type == 2:
                predictor = up
            elif filter_type == 3:
                predictor = (left + up) // 2
            elif filter_type == 4:
                predictor = _paeth(left, up, upper_left)
            else:
                raise EvidenceError("screenshot PNG filter is unsupported")
            decoded[index] = (value + predictor) & 0xFF
        previous = bytes(decoded)
        rows.append(previous)

    colors: set[bytes] = set()
    for y in range(0, height, 4):
        row = rows[y]
        for x in range(0, width, 4):
            start = x * channels
            pixel = row[start : start + channels]
            if color_type == 2:
                pixel += b"\xff"
            colors.add(pixel)

    return {
        "width": width,
        "height": height,
        "distinctColors": len(colors),
    }


def _verify_native_authority(
    value: Any,
    *,
    run_start: int,
    run_end: int,
    not_before_unix_ms: int,
    executable_sha: str,
    executable_pid: int,
    target: str,
    transcript_binding: str,
    carrier_sha: str,
) -> dict[str, Any]:
    authority = _object(value, "nativeAuthority")
    _exact_keys(
        authority,
        "nativeAuthority",
        {
            "receipt",
            "verificationResult",
            "ledgerRecord",
            "filesystemAuthorityAttestation",
        },
    )

    receipt = _object(authority["receipt"], "nativeAuthority.receipt")
    _exact_keys(
        receipt,
        "nativeAuthority.receipt",
        {
            "schema",
            "version",
            "evidenceKind",
            "challenge",
            "emittedAtUnixMs",
            "monotonicStagesMs",
            "emitter",
            "build",
            "target",
            "carrier",
            "preSend",
            "action",
            "postSend",
            "receiptDigestSha256",
        },
    )
    if receipt["schema"] != NATIVE_RECEIPT_SCHEMA:
        raise EvidenceError("native receipt schema is invalid")
    if receipt["version"] != 3 or receipt["evidenceKind"] != "native-placement":
        raise EvidenceError("native receipt identity is invalid")
    challenge = _sha256(receipt["challenge"], "nativeAuthority.receipt.challenge")
    if challenge == "0" * 64:
        raise EvidenceError("zero native challenge is forbidden")
    emitted_at = _at(
        receipt["emittedAtUnixMs"],
        "nativeAuthority.receipt.emittedAtUnixMs",
        run_start,
        run_end,
        after=not_before_unix_ms,
    )

    stages = _object(receipt["monotonicStagesMs"], "nativeAuthority.receipt.monotonicStagesMs")
    _exact_keys(
        stages,
        "nativeAuthority.receipt.monotonicStagesMs",
        {
            "challengeClaimed",
            "preSendReadback",
            "sendInjected",
            "postContextRevalidated",
            "receiptEmitted",
        },
    )
    ordered_stages = [
        _integer(stages[name], f"nativeAuthority.receipt.monotonicStagesMs.{name}")
        for name in (
            "challengeClaimed",
            "preSendReadback",
            "sendInjected",
            "postContextRevalidated",
            "receiptEmitted",
        )
    ]
    if ordered_stages[0] != 0 or ordered_stages != sorted(ordered_stages):
        raise EvidenceError("native receipt monotonic stages are not ordered from zero")

    emitter = _process(receipt["emitter"], "nativeAuthority.receipt.emitter")
    if _integer(emitter["pid"], "nativeAuthority.receipt.emitter.pid") != executable_pid:
        raise EvidenceError("native emitter PID does not match the exact OSL process")
    if emitter["executableSha256"] != executable_sha:
        raise EvidenceError("native emitter executable hash does not match the exact OSL binary")

    build = _object(receipt["build"], "nativeAuthority.receipt.build")
    _exact_keys(
        build,
        "nativeAuthority.receipt.build",
        {"features", "debugAssertions", "targetOs", "targetArch", "profile"},
    )
    if build["features"] != ["core", "desktop"]:
        raise EvidenceError("native receipt build features are not the shipping set")
    if _boolean(build["debugAssertions"], "nativeAuthority.receipt.build.debugAssertions"):
        raise EvidenceError("native receipt came from a debug-assertions build")
    if build["targetOs"] != "windows" or build["targetArch"] != "x86_64":
        raise EvidenceError("native receipt target platform is not Windows x86_64")
    if build["profile"] != "release":
        raise EvidenceError("native receipt build profile is not release")

    native_target = _target(receipt["target"], "nativeAuthority.receipt.target")
    native_binding = native_target["bindingSha256"]
    if emitter["pid"] == native_target["pid"]:
        raise EvidenceError("native receipt emitter and Discord target are the same process")
    expected_transcript_binding = _legacy_target_binding(native_target, target)
    if expected_transcript_binding != transcript_binding:
        raise EvidenceError("native Discord process/HWND does not bind the transcript target")

    carrier = _object(receipt["carrier"], "nativeAuthority.receipt.carrier")
    _exact_keys(
        carrier,
        "nativeAuthority.receipt.carrier",
        {"targetBindingSha256", "utf8B64", "sha256", "byteLength", "utf16Length"},
    )
    if _sha256(carrier["targetBindingSha256"], "nativeAuthority.receipt.carrier.targetBindingSha256") != native_binding:
        raise EvidenceError("native carrier is not bound to the Discord target")
    try:
        carrier_bytes = base64.b64decode(
            _string(carrier["utf8B64"], "nativeAuthority.receipt.carrier.utf8B64", maximum=16 * 1024),
            validate=True,
        )
    except (ValueError, binascii.Error) as error:
        raise EvidenceError("native carrier is not canonical base64") from error
    if base64.b64encode(carrier_bytes).decode("ascii") != carrier["utf8B64"]:
        raise EvidenceError("native carrier base64 is not canonical")
    if not carrier_bytes or len(carrier_bytes) > MAX_CARRIER_BYTES:
        raise EvidenceError("native carrier byte length is out of bounds")
    try:
        carrier_text = carrier_bytes.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise EvidenceError("native carrier is not strict UTF-8") from error
    if any(ord(character) < 0x20 and character != "\n" for character in carrier_text):
        raise EvidenceError("native carrier contains a forbidden control character")
    native_carrier_sha = hashlib.sha256(carrier_bytes).hexdigest()
    native_carrier_utf16 = len(carrier_text.encode("utf-16-le")) // 2
    if _sha256(carrier["sha256"], "nativeAuthority.receipt.carrier.sha256") != native_carrier_sha:
        raise EvidenceError("native carrier hash does not match its bytes")
    if native_carrier_sha != carrier_sha:
        raise EvidenceError("native carrier does not match the pre-Enter readback")
    if _integer(carrier["byteLength"], "nativeAuthority.receipt.carrier.byteLength") != len(carrier_bytes):
        raise EvidenceError("native carrier byte length does not match")
    if _integer(carrier["utf16Length"], "nativeAuthority.receipt.carrier.utf16Length") != native_carrier_utf16:
        raise EvidenceError("native carrier UTF-16 length does not match")

    pre_send = _object(receipt["preSend"], "nativeAuthority.receipt.preSend")
    _exact_keys(pre_send, "nativeAuthority.receipt.preSend", {"targetBindingSha256", "readback", "foreground"})
    if _sha256(pre_send["targetBindingSha256"], "nativeAuthority.receipt.preSend.targetBindingSha256") != native_binding:
        raise EvidenceError("native pre-send is not bound to the Discord target")
    _readback(
        pre_send["readback"],
        "nativeAuthority.receipt.preSend.readback",
        binding=native_binding,
        classification="exact",
        digest=native_carrier_sha,
        byte_length=len(carrier_bytes),
        utf16_length=native_carrier_utf16,
    )
    foreground = _object(pre_send["foreground"], "nativeAuthority.receipt.preSend.foreground")
    _exact_keys(
        foreground,
        "nativeAuthority.receipt.preSend.foreground",
        {
            "targetBindingSha256",
            "foregroundHwnd",
            "foregroundRootHwnd",
            "foregroundPid",
            "targetRootHwnd",
            "keyboardFocusProven",
            "targetOwnedByTrustedProcess",
        },
    )
    if _sha256(foreground["targetBindingSha256"], "nativeAuthority.receipt.preSend.foreground.targetBindingSha256") != native_binding:
        raise EvidenceError("native foreground is not bound to the Discord target")
    _integer(foreground["foregroundHwnd"], "nativeAuthority.receipt.preSend.foreground.foregroundHwnd", minimum=1)
    foreground_root = _integer(foreground["foregroundRootHwnd"], "nativeAuthority.receipt.preSend.foreground.foregroundRootHwnd", minimum=1)
    target_root = _integer(foreground["targetRootHwnd"], "nativeAuthority.receipt.preSend.foreground.targetRootHwnd", minimum=1)
    if foreground_root != target_root or target_root != native_target["rootHwnd"]:
        raise EvidenceError("native foreground root HWND is not the exact Discord root")
    if _integer(foreground["foregroundPid"], "nativeAuthority.receipt.preSend.foreground.foregroundPid", minimum=1) != native_target["pid"]:
        raise EvidenceError("native foreground PID is not the Discord target PID")
    if not _boolean(foreground["keyboardFocusProven"], "nativeAuthority.receipt.preSend.foreground.keyboardFocusProven"):
        raise EvidenceError("native receipt did not prove keyboard focus")
    if not _boolean(foreground["targetOwnedByTrustedProcess"], "nativeAuthority.receipt.preSend.foreground.targetOwnedByTrustedProcess"):
        raise EvidenceError("native receipt did not prove trusted target ownership")

    action = _object(receipt["action"], "nativeAuthority.receipt.action")
    _exact_keys(
        action,
        "nativeAuthority.receipt.action",
        {
            "targetBindingSha256",
            "mechanism",
            "attempted",
            "acceptedInputCount",
            "enterCertainty",
            "retryPolicy",
            "actionSequence",
        },
    )
    if _sha256(action["targetBindingSha256"], "nativeAuthority.receipt.action.targetBindingSha256") != native_binding:
        raise EvidenceError("native action is not bound to the Discord target")
    required_action = {
        "mechanism": "sendinput_enter",
        "attempted": True,
        "acceptedInputCount": 2,
        "enterCertainty": "injected_once",
        "retryPolicy": "never_auto_retry",
        "actionSequence": 1,
    }
    for key, expected in required_action.items():
        if action[key] != expected:
            raise EvidenceError(f"native action.{key} does not prove one Enter send")

    post_send = _object(receipt["postSend"], "nativeAuthority.receipt.postSend")
    _exact_keys(
        post_send,
        "nativeAuthority.receipt.postSend",
        {
            "targetBindingSha256",
            "readback",
            "hostRevalidated",
            "overlayContextUnchanged",
            "carrierConsumed",
            "sentRowProven",
            "rowDelta",
            "status",
        },
    )
    if _sha256(post_send["targetBindingSha256"], "nativeAuthority.receipt.postSend.targetBindingSha256") != native_binding:
        raise EvidenceError("native post-send is not bound to the Discord target")
    _readback(
        post_send["readback"],
        "nativeAuthority.receipt.postSend.readback",
        binding=native_binding,
        classification="empty",
        digest=EMPTY_SHA256,
        byte_length=0,
        utf16_length=0,
    )
    for key in ("hostRevalidated", "overlayContextUnchanged", "carrierConsumed", "sentRowProven"):
        if not _boolean(post_send[key], f"nativeAuthority.receipt.postSend.{key}"):
            raise EvidenceError(f"native post-send {key} is not proven")
    if _integer(post_send["rowDelta"], "nativeAuthority.receipt.postSend.rowDelta") != 1:
        raise EvidenceError("native post-send row delta is not exactly one")
    if post_send["status"] != "sent":
        raise EvidenceError("native post-send status is not sent")
    if _sha256(receipt["receiptDigestSha256"], "nativeAuthority.receipt.receiptDigestSha256") != _receipt_object_digest(receipt):
        raise EvidenceError("native receipt object digest is invalid")

    filesystem = _verify_filesystem_attestation(
        authority["filesystemAuthorityAttestation"],
        challenge=challenge,
        emitted_at=emitted_at,
        run_end=run_end,
    )

    frame_sha = hashlib.sha256(_canonical_json(receipt)).hexdigest()
    result = _object(authority["verificationResult"], "nativeAuthority.verificationResult")
    _exact_keys(
        result,
        "nativeAuthority.verificationResult",
        {
            "schema",
            "version",
            "source",
            "status",
            "parserCryptoValid",
            "runtimeReceiptAccepted",
            "fullC4Success",
            "pointDelta",
            "receiptFrameSha256",
        },
    )
    if (
        result["schema"] != NATIVE_RESULT_SCHEMA
        or result["version"] != 3
        or result["source"] != "runtime_named_pipe"
        or result["status"] != "runtime-native-receipt-valid"
    ):
        raise EvidenceError("native verification result is not runtime authority")
    if not _boolean(result["parserCryptoValid"], "nativeAuthority.verificationResult.parserCryptoValid"):
        raise EvidenceError("native verification parser/crypto did not pass")
    if not _boolean(result["runtimeReceiptAccepted"], "nativeAuthority.verificationResult.runtimeReceiptAccepted"):
        raise EvidenceError("native runtime receipt was not accepted")
    if _boolean(result["fullC4Success"], "nativeAuthority.verificationResult.fullC4Success"):
        raise EvidenceError("native verifier may not claim full C4 success")
    if _integer(result["pointDelta"], "nativeAuthority.verificationResult.pointDelta") != 0:
        raise EvidenceError("native verifier may not award C4 points")
    if _sha256(result["receiptFrameSha256"], "nativeAuthority.verificationResult.receiptFrameSha256") != frame_sha:
        raise EvidenceError("native verification result names another receipt frame")

    ledger = _object(authority["ledgerRecord"], "nativeAuthority.ledgerRecord")
    _exact_keys(
        ledger,
        "nativeAuthority.ledgerRecord",
        {
            "version",
            "challengeSha256",
            "state",
            "issuedAtUnixMs",
            "expiresAtUnixMs",
            "pipeBindingSha256",
            "receiptFrameSha256",
            "updatedAtUnixMs",
            "recordDigestSha256",
        },
    )
    if ledger["version"] != 2 or ledger["state"] != "consumed":
        raise EvidenceError("native challenge ledger was not consumed")
    issued_at = _integer(ledger["issuedAtUnixMs"], "nativeAuthority.ledgerRecord.issuedAtUnixMs")
    expires_at = _integer(ledger["expiresAtUnixMs"], "nativeAuthority.ledgerRecord.expiresAtUnixMs", minimum=issued_at + 1)
    updated_at = _integer(ledger["updatedAtUnixMs"], "nativeAuthority.ledgerRecord.updatedAtUnixMs", minimum=issued_at)
    if issued_at < not_before_unix_ms or not (
        issued_at
        <= emitted_at
        <= filesystem["checkedAtUnixMs"]
        <= updated_at
        <= run_end
    ):
        raise EvidenceError("native challenge consumption is outside this run")
    if expires_at > issued_at + 60_000 or updated_at >= expires_at:
        raise EvidenceError("native challenge lifetime is invalid")
    if _sha256(ledger["challengeSha256"], "nativeAuthority.ledgerRecord.challengeSha256") != _challenge_digest(challenge):
        raise EvidenceError("native ledger challenge does not match the receipt")
    if filesystem["ledgerRecordPath"] != (
        filesystem["ledgerRoot"] + "\\" + ledger["challengeSha256"] + ".json"
    ):
        raise EvidenceError("native filesystem attestation is not bound to the consumed ledger")
    if _sha256(ledger["pipeBindingSha256"], "nativeAuthority.ledgerRecord.pipeBindingSha256") != _pipe_binding_digest(emitter):
        raise EvidenceError("native ledger pipe binding does not match the emitter")
    if _sha256(ledger["receiptFrameSha256"], "nativeAuthority.ledgerRecord.receiptFrameSha256") != frame_sha:
        raise EvidenceError("native ledger consumed another receipt frame")
    if _sha256(ledger["recordDigestSha256"], "nativeAuthority.ledgerRecord.recordDigestSha256") != _ledger_record_digest(ledger):
        raise EvidenceError("native ledger record digest is invalid")

    return {
        "challengeSha256": ledger["challengeSha256"],
        "receiptFrameSha256": frame_sha,
        "discordRootHwnd": native_target["rootHwnd"],
    }


def _same_binding(
    value: dict[str, Any],
    label: str,
    *,
    run_id: str,
    executable_sha: str,
    target: str,
) -> None:
    if value["runId"] != run_id:
        raise EvidenceError(f"{label}.runId does not bind to this run")
    if value["executableSha256"] != executable_sha:
        raise EvidenceError(f"{label}.executableSha256 does not bind to this executable")
    if value["targetConversation"] != target:
        raise EvidenceError(f"{label}.targetConversation is the wrong target")


def _file_identity(value: Any, label: str) -> None:
    identity = _object(value, label)
    _exact_keys(
        identity,
        label,
        {"volumeSerialNumber", "fileIndex", "fileSize", "lastWriteTime100ns"},
    )
    _integer(identity["volumeSerialNumber"], f"{label}.volumeSerialNumber", minimum=1)
    _integer(identity["fileIndex"], f"{label}.fileIndex", minimum=1)
    _integer(identity["fileSize"], f"{label}.fileSize", minimum=1)
    _integer(identity["lastWriteTime100ns"], f"{label}.lastWriteTime100ns", minimum=1)


PROCESS_KEYS = {
    "pid",
    "processStartTime100ns",
    "sessionId",
    "executablePath",
    "executableSha256",
    "fileIdentity",
}

TARGET_KEYS = PROCESS_KEYS | {
    "bindingSha256",
    "hwnd",
    "rootHwnd",
    "hostGeneration",
    "publisher",
}


def _process(value: Any, label: str) -> dict[str, Any]:
    process = _object(value, label)
    _exact_keys(process, label, PROCESS_KEYS)
    _integer(process["pid"], f"{label}.pid", minimum=1)
    _integer(process["processStartTime100ns"], f"{label}.processStartTime100ns", minimum=1)
    _integer(process["sessionId"], f"{label}.sessionId")
    _string(process["executablePath"], f"{label}.executablePath", maximum=1024)
    _sha256(process["executableSha256"], f"{label}.executableSha256")
    _file_identity(process["fileIdentity"], f"{label}.fileIdentity")
    return process


def _target(value: Any, label: str) -> dict[str, Any]:
    target = _object(value, label)
    _exact_keys(target, label, TARGET_KEYS)
    _process({key: target[key] for key in PROCESS_KEYS}, f"{label}.process")
    _integer(target["hwnd"], f"{label}.hwnd", minimum=1)
    _integer(target["rootHwnd"], f"{label}.rootHwnd", minimum=1)
    _integer(target["hostGeneration"], f"{label}.hostGeneration", minimum=1)
    if target["publisher"] != "Discord Inc.":
        raise EvidenceError("native target publisher is not Discord Inc.")
    claimed = _sha256(target["bindingSha256"], f"{label}.bindingSha256")
    if not hmac.compare_digest(claimed, _target_binding_digest(target)):
        raise EvidenceError("native target binding digest is invalid")
    return target


def _readback(
    value: Any,
    label: str,
    *,
    binding: str,
    classification: str,
    digest: str,
    byte_length: int,
    utf16_length: int,
) -> None:
    readback = _object(value, label)
    _exact_keys(
        readback,
        label,
        {
            "targetBindingSha256",
            "classification",
            "complete",
            "sha256",
            "byteLength",
            "utf16Length",
        },
    )
    if _sha256(readback["targetBindingSha256"], f"{label}.targetBindingSha256") != binding:
        raise EvidenceError(f"{label} does not bind the native Discord target")
    if readback["classification"] != classification:
        raise EvidenceError(f"{label} classification is not {classification}")
    if not _boolean(readback["complete"], f"{label}.complete"):
        raise EvidenceError(f"{label} is incomplete")
    if _sha256(readback["sha256"], f"{label}.sha256") != digest:
        raise EvidenceError(f"{label} digest does not match")
    if _integer(readback["byteLength"], f"{label}.byteLength") != byte_length:
        raise EvidenceError(f"{label} byte length does not match")
    if _integer(readback["utf16Length"], f"{label}.utf16Length") != utf16_length:
        raise EvidenceError(f"{label} UTF-16 length does not match")


def _legacy_target_binding(target: dict[str, Any], target_conversation: str) -> str:
    material = (
        f"{target['executableSha256']}|{target['pid']}|"
        f"{target['hwnd']}|{target_conversation}"
    )
    return hashlib.sha256(material.encode("utf-8")).hexdigest()


def verify_bundle(
    bundle_path: Path,
    expected_target: str,
    *,
    expected_run_id: str | None = None,
    not_before_unix_ms: int = 0,
) -> dict[str, Any]:
    bundle_path = bundle_path.resolve()
    if not bundle_path.is_file():
        raise EvidenceError("bundle is missing")
    if bundle_path.stat().st_size <= 0 or bundle_path.stat().st_size > MAX_BUNDLE_BYTES:
        raise EvidenceError("bundle size is invalid")
    try:
        bundle = json.loads(
            bundle_path.read_text(encoding="utf-8"),
            object_pairs_hook=_reject_duplicate_keys,
        )
    except EvidenceError:
        raise
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise EvidenceError("bundle is not bounded UTF-8 JSON") from error
    bundle = _object(bundle, "bundle")
    _exact_keys(
        bundle,
        "bundle",
        {
            "schema",
            "runId",
            "runStartUnixMs",
            "runEndUnixMs",
            "targetConversation",
            "build",
            "executable",
            "commandReceipt",
            "preEnterReadback",
            "postEnterComposer",
            "conversationRows",
            "nativeAuthority",
            "screenshot",
        },
    )
    if bundle["schema"] != SCHEMA:
        raise EvidenceError("unsupported evidence schema")
    try:
        run_id = str(uuid.UUID(_string(bundle["runId"], "runId", maximum=36)))
    except (ValueError, AttributeError) as error:
        raise EvidenceError("runId must be a canonical UUID") from error
    if run_id != bundle["runId"]:
        raise EvidenceError("runId must be a canonical UUID")
    if expected_run_id is not None and run_id != expected_run_id:
        raise EvidenceError("bundle runId does not match the operator-approved run")
    run_start = _integer(bundle["runStartUnixMs"], "runStartUnixMs", minimum=1)
    run_end = _integer(bundle["runEndUnixMs"], "runEndUnixMs", minimum=run_start)
    if run_end - run_start > MAX_RUN_MS:
        raise EvidenceError("run exceeds the ten-minute evidence window")
    if run_start < not_before_unix_ms:
        raise EvidenceError("bundle predates the operator-approved evidence window")
    target = _string(bundle["targetConversation"], "targetConversation", maximum=128)
    if target != expected_target:
        raise EvidenceError("bundle target does not match the operator-approved target")

    build = _object(bundle["build"], "build")
    _exact_keys(
        build,
        "build",
        {
            "frontendCommand",
            "cargoCommand",
            "cargoFeatures",
            "qaShell",
            "observedAtUnixMs",
            "executableSha256",
        },
    )
    if _string(build["frontendCommand"], "build.frontendCommand") != (
        "npm --prefix apps/osl-hub-ui run build"
    ):
        raise EvidenceError("frontend was not built in shipping mode")
    cargo_command = _string(build["cargoCommand"], "build.cargoCommand")
    if "osl-cargo" not in cargo_command:
        raise EvidenceError("shipping desktop build did not use osl-cargo")
    if "--release" not in cargo_command.split():
        raise EvidenceError("shipping desktop build was not a release build")
    features = _list(build["cargoFeatures"], "build.cargoFeatures")
    if features != ["desktop"]:
        raise EvidenceError("shipping build features must be exactly ['desktop']")
    if _boolean(build["qaShell"], "build.qaShell"):
        raise EvidenceError("QA-shell build is inadmissible")
    if "discord-qa" in cargo_command.lower():
        raise EvidenceError("cargo command mentions a QA-shell feature")
    build_at = _integer(build["observedAtUnixMs"], "build.observedAtUnixMs", minimum=1)
    if build_at > run_start:
        raise EvidenceError("shipping build receipt postdates this run")

    executable = _object(bundle["executable"], "executable")
    _exact_keys(
        executable,
        "executable",
        {"path", "sha256", "processId", "processStartedAtUnixMs"},
    )
    executable_path = Path(_string(executable["path"], "executable.path", maximum=1024))
    if not executable_path.is_absolute() or not executable_path.is_file():
        raise EvidenceError("exact executable path is absent")
    executable_sha = _sha256(executable["sha256"], "executable.sha256")
    if _sha256(build["executableSha256"], "build.executableSha256") != executable_sha:
        raise EvidenceError("shipping build receipt is bound to another executable")
    if _hash_file(executable_path) != executable_sha:
        raise EvidenceError("exact executable hash mismatch")
    _scan_for_markers(executable_path)
    executable_pid = _integer(executable["processId"], "executable.processId", minimum=1)
    process_started = _integer(
        executable["processStartedAtUnixMs"],
        "executable.processStartedAtUnixMs",
        minimum=1,
    )
    if process_started < run_start - 5_000 or process_started > run_end:
        raise EvidenceError("process start does not bind to this run")

    receipt = _object(bundle["commandReceipt"], "commandReceipt")
    _exact_keys(
        receipt,
        "commandReceipt",
        {
            "runId",
            "executableSha256",
            "targetConversation",
            "observedAtUnixMs",
            "source",
            "receiptAuthority",
            "uiControlAutomationId",
            "backendCommand",
            "status",
            "placed",
            "enterSent",
            "qaShell",
            "rendererStatus",
        },
    )
    _same_binding(
        receipt,
        "commandReceipt",
        run_id=run_id,
        executable_sha=executable_sha,
        target=target,
    )
    receipt_at = _at(
        receipt["observedAtUnixMs"],
        "commandReceipt.observedAtUnixMs",
        run_start,
        run_end,
    )
    required_receipt_values = {
        "source": "production-overlay-ui",
        "receiptAuthority": "shipping-renderer-success-gate",
        "uiControlAutomationId": "prepare-protected",
        "backendCommand": "send_native_discord_overlay_carrier",
        "status": "sent",
        "rendererStatus": SUCCESS_STATUS,
    }
    for key, expected in required_receipt_values.items():
        if receipt[key] != expected:
            raise EvidenceError(f"commandReceipt.{key} is not shipping production evidence")
    if not _boolean(receipt["placed"], "commandReceipt.placed"):
        raise EvidenceError("production command did not prove placement")
    if not _boolean(receipt["enterSent"], "commandReceipt.enterSent"):
        raise EvidenceError("production command did not prove Enter")
    if _boolean(receipt["qaShell"], "commandReceipt.qaShell"):
        raise EvidenceError("QA-shell command receipt is inadmissible")

    pre = _object(bundle["preEnterReadback"], "preEnterReadback")
    _exact_keys(
        pre,
        "preEnterReadback",
        {
            "runId",
            "executableSha256",
            "targetConversation",
            "observedAtUnixMs",
            "authority",
            "relation",
            "readCount",
            "utf8Bytes",
            "composerTextSha256",
            "exact",
        },
    )
    _same_binding(
        pre,
        "preEnterReadback",
        run_id=run_id,
        executable_sha=executable_sha,
        target=target,
    )
    pre_at = _at(
        pre["observedAtUnixMs"],
        "preEnterReadback.observedAtUnixMs",
        run_start,
        run_end,
        before=receipt_at,
    )
    if pre["authority"] != "native-pre-enter-exact-readback":
        raise EvidenceError("pre-Enter readback lacks the production authority")
    if pre["relation"] != "rawExact":
        raise EvidenceError("pre-Enter readback was not byte-exact")
    if _integer(pre["readCount"], "preEnterReadback.readCount", minimum=0) != 1:
        raise EvidenceError("pre-Enter readback must be present exactly once")
    if _integer(pre["utf8Bytes"], "preEnterReadback.utf8Bytes", minimum=1) > 16_384:
        raise EvidenceError("pre-Enter carrier is unbounded")
    carrier_sha = _sha256(
        pre["composerTextSha256"],
        "preEnterReadback.composerTextSha256",
    )
    if not _boolean(pre["exact"], "preEnterReadback.exact"):
        raise EvidenceError("pre-Enter exact-readback proof is false")

    post = _object(bundle["postEnterComposer"], "postEnterComposer")
    _exact_keys(
        post,
        "postEnterComposer",
        {
            "runId",
            "executableSha256",
            "targetConversation",
            "observedAtUnixMs",
            "readCount",
            "utf8Bytes",
            "composerTextSha256",
            "empty",
        },
    )
    _same_binding(
        post,
        "postEnterComposer",
        run_id=run_id,
        executable_sha=executable_sha,
        target=target,
    )
    post_at = _at(
        post["observedAtUnixMs"],
        "postEnterComposer.observedAtUnixMs",
        run_start,
        run_end,
        after=receipt_at,
    )
    if _integer(post["readCount"], "postEnterComposer.readCount", minimum=0) != 1:
        raise EvidenceError("post-Enter composer evidence must be present exactly once")
    if _integer(post["utf8Bytes"], "postEnterComposer.utf8Bytes", minimum=0) != 0:
        raise EvidenceError("post-Enter composer is not empty")
    if _sha256(post["composerTextSha256"], "postEnterComposer.composerTextSha256") != EMPTY_SHA256:
        raise EvidenceError("post-Enter empty-composer hash is wrong")
    if not _boolean(post["empty"], "postEnterComposer.empty"):
        raise EvidenceError("post-Enter composer evidence is not empty")

    rows = _object(bundle["conversationRows"], "conversationRows")
    _exact_keys(rows, "conversationRows", {"before", "after", "newRows"})

    def row_snapshot(value: Any, label: str, *, after_time: int | None = None) -> tuple[int, str, int]:
        snapshot = _object(value, label)
        _exact_keys(
            snapshot,
            label,
            {
                "runId",
                "executableSha256",
                "targetConversation",
                "observedAtUnixMs",
                "namedConversationMatches",
                "transcriptMatches",
                "readCount",
                "rowCount",
                "targetBindingSha256",
            },
        )
        _same_binding(
            snapshot,
            label,
            run_id=run_id,
            executable_sha=executable_sha,
            target=target,
        )
        observed = _at(
            snapshot["observedAtUnixMs"],
            f"{label}.observedAtUnixMs",
            run_start,
            run_end,
            after=after_time,
        )
        if _integer(snapshot["namedConversationMatches"], f"{label}.namedConversationMatches") != 1:
            raise EvidenceError(f"{label} does not bind one unique named conversation")
        if _integer(snapshot["transcriptMatches"], f"{label}.transcriptMatches") != 1:
            raise EvidenceError(f"{label} does not bind one unique transcript")
        if _integer(snapshot["readCount"], f"{label}.readCount") != 1:
            raise EvidenceError(f"{label} row evidence must occur exactly once")
        count = _integer(snapshot["rowCount"], f"{label}.rowCount")
        binding = _sha256(snapshot["targetBindingSha256"], f"{label}.targetBindingSha256")
        return count, binding, observed

    before_count, before_binding, before_at = row_snapshot(rows["before"], "conversationRows.before")
    if before_at > pre_at:
        raise EvidenceError("before-row snapshot happened after pre-Enter readback")
    after_count, after_binding, after_at = row_snapshot(
        rows["after"],
        "conversationRows.after",
        after_time=post_at,
    )
    if before_binding != after_binding:
        raise EvidenceError("conversation target changed during the send")
    if after_count - before_count != 1:
        raise EvidenceError("conversation must gain exactly one row")

    new_rows = _list(rows["newRows"], "conversationRows.newRows")
    if len(new_rows) != 1:
        raise EvidenceError("new-row evidence must contain exactly one row")
    new_row = _object(new_rows[0], "conversationRows.newRows[0]")
    _exact_keys(
        new_row,
        "conversationRows.newRows[0]",
        {
            "targetConversation",
            "targetBindingSha256",
            "carrierTextSha256",
            "rowIdentitySha256",
            "matchCount",
        },
    )
    if new_row["targetConversation"] != target:
        raise EvidenceError("new row belongs to the wrong target")
    if _sha256(new_row["targetBindingSha256"], "newRow.targetBindingSha256") != before_binding:
        raise EvidenceError("new row is not bound to the named conversation")
    if _sha256(new_row["carrierTextSha256"], "newRow.carrierTextSha256") != carrier_sha:
        raise EvidenceError("new row does not contain the exact pre-Enter carrier")
    _sha256(new_row["rowIdentitySha256"], "newRow.rowIdentitySha256")
    if _integer(new_row["matchCount"], "newRow.matchCount") != 1:
        raise EvidenceError("new carrier row is missing or duplicated")

    native = _verify_native_authority(
        bundle["nativeAuthority"],
        run_start=run_start,
        run_end=run_end,
        not_before_unix_ms=not_before_unix_ms,
        executable_sha=executable_sha,
        executable_pid=executable_pid,
        target=target,
        transcript_binding=before_binding,
        carrier_sha=carrier_sha,
    )

    screenshot = _object(bundle["screenshot"], "screenshot")
    _exact_keys(
        screenshot,
        "screenshot",
        {
            "path",
            "sha256",
            "observedAtUnixMs",
            "targetConversation",
            "namedConversationMatches",
            "newRowMatches",
        },
    )
    screenshot_path = _resolve_artifact(bundle_path, screenshot["path"], "screenshot.path")
    if screenshot_path.suffix.lower() != ".png":
        raise EvidenceError("screenshot must be PNG")
    if _hash_file(screenshot_path) != _sha256(screenshot["sha256"], "screenshot.sha256"):
        raise EvidenceError("screenshot hash mismatch")
    screenshot_facts = _png_facts(screenshot_path)
    if (
        screenshot_facts["width"] < MIN_SCREENSHOT_WIDTH
        or screenshot_facts["height"] < MIN_SCREENSHOT_HEIGHT
        or screenshot_facts["distinctColors"] < MIN_SCREENSHOT_DISTINCT_COLORS
    ):
        raise EvidenceError("screenshot lacks semantic retained-pixel evidence")
    _at(
        screenshot["observedAtUnixMs"],
        "screenshot.observedAtUnixMs",
        run_start,
        run_end,
        after=after_at,
    )
    if screenshot["targetConversation"] != target:
        raise EvidenceError("screenshot is bound to the wrong target")
    if _integer(screenshot["namedConversationMatches"], "screenshot.namedConversationMatches") != 1:
        raise EvidenceError("screenshot lacks one unique named conversation")
    if _integer(screenshot["newRowMatches"], "screenshot.newRowMatches") != 1:
        raise EvidenceError("screenshot lacks one unique new row")
    screenshot_mtime_ms = screenshot_path.stat().st_mtime_ns // 1_000_000
    if screenshot_mtime_ms < run_start or screenshot_mtime_ms > run_end + 2_000:
        raise EvidenceError("screenshot file is stale or outside this run")

    return {
        "verdict": "pass",
        "tier": "runtime-candidate",
        "schema": SCHEMA,
        "runId": run_id,
        "executableSha256": executable_sha,
        "rowDelta": 1,
        "productionReceipt": "sent/placed/enterSent",
        "preEnterReadback": "rawExact",
        "postEnterComposer": "empty",
        "nativeReceipt": "runtime-consumed",
        "nativeChallengeSha256": native["challengeSha256"],
        "nativeReceiptFrameSha256": native["receiptFrameSha256"],
        "screenshotSha256": screenshot["sha256"],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--bundle", type=Path)
    mode.add_argument("--check-executable", type=Path)
    parser.add_argument("--expected-target")
    parser.add_argument("--expected-run-id")
    parser.add_argument("--not-before-unix-ms", type=int, default=0)
    args = parser.parse_args()
    try:
        if args.check_executable is not None:
            if args.expected_target or args.expected_run_id or args.not_before_unix_ms:
                raise EvidenceError("bundle-only expectations are not valid with --check-executable")
            executable = args.check_executable.resolve()
            if not executable.is_file():
                raise EvidenceError("exact executable path is absent")
            _scan_for_markers(executable)
            result = {
                "verdict": "pass",
                "tier": "binary-preflight-only",
                "executableSha256": _hash_file(executable),
                "qaShellMarkers": "absent",
            }
        else:
            if not args.expected_target:
                raise EvidenceError("--expected-target is required with --bundle")
            if not args.expected_run_id:
                raise EvidenceError("--expected-run-id is required with --bundle")
            if args.not_before_unix_ms <= 0:
                raise EvidenceError("--not-before-unix-ms is required with --bundle")
            try:
                expected_run_id = str(uuid.UUID(args.expected_run_id))
            except (ValueError, AttributeError) as error:
                raise EvidenceError("--expected-run-id must be a canonical UUID") from error
            if expected_run_id != args.expected_run_id:
                raise EvidenceError("--expected-run-id must be a canonical UUID")
            result = verify_bundle(
                args.bundle,
                args.expected_target,
                expected_run_id=expected_run_id,
                not_before_unix_ms=args.not_before_unix_ms,
            )
    except EvidenceError as error:
        print(
            json.dumps(
                {
                    "verdict": "reject",
                    "tier": "unmeasurable",
                    "reason": str(error),
                },
                separators=(",", ":"),
            )
        )
        return 1
    print(json.dumps(result, separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
