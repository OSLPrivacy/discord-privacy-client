"""Strict C4 native-authority receipt v3 schema and digest helpers.

This module validates only the native receipt contract. It does not decide
whether evidence came from a real named pipe or whether a C4 runtime claim may
be awarded; those authority decisions live in ``verify.py``.
"""

from __future__ import annotations

import base64
import binascii
import copy
import hashlib
import hmac
import json
import re
from typing import Any


SCHEMA_NAME = "osl.c4.native-placement-receipt"
SCHEMA_VERSION = 3
EVIDENCE_KIND = "native-placement"
EXACT_SHIPPING_FEATURES = ["core", "desktop"]
MAX_RECEIPT_BYTES = 64 * 1024
MAX_CARRIER_BYTES = 8 * 1024
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
WINDOWS_EXE_RE = re.compile(r"^[A-Za-z]:\\(?:[^\\/:*?\"<>|\x00]+\\)*[^\\/:*?\"<>|\x00]+\.exe$")


class SchemaError(ValueError):
    """The receipt is malformed, ambiguous, or semantically inadmissible."""


def canonical_json(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        allow_nan=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def sha256_hex(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _object_without_digest(receipt: dict[str, Any]) -> dict[str, Any]:
    body = copy.deepcopy(receipt)
    body.pop("receiptDigestSha256", None)
    return body


def receipt_digest(receipt: dict[str, Any]) -> str:
    return sha256_hex(
        b"OSL/C4/native-placement-receipt/v3\x00"
        + canonical_json(_object_without_digest(receipt))
    )


def target_binding_digest(target: dict[str, Any]) -> str:
    body = copy.deepcopy(target)
    body.pop("bindingSha256", None)
    return sha256_hex(
        b"OSL/C4/native-target-binding/v3\x00" + canonical_json(body)
    )


def seal_receipt(receipt: dict[str, Any]) -> dict[str, Any]:
    sealed = copy.deepcopy(receipt)
    sealed["receiptDigestSha256"] = receipt_digest(sealed)
    return sealed


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise SchemaError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def parse_receipt(encoded: bytes) -> dict[str, Any]:
    if type(encoded) is not bytes:
        raise SchemaError("receipt must be raw bytes")
    if not encoded or len(encoded) > MAX_RECEIPT_BYTES:
        raise SchemaError("receipt byte length is invalid")
    try:
        text = encoded.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise SchemaError("receipt is not strict UTF-8") from error
    try:
        value = json.loads(
            text,
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=lambda value: (_ for _ in ()).throw(
                SchemaError(f"invalid JSON number: {value}")
            ),
        )
    except (json.JSONDecodeError, RecursionError) as error:
        raise SchemaError("receipt is not valid bounded JSON") from error
    if type(value) is not dict:
        raise SchemaError("receipt root must be an object")
    if canonical_json(value) != encoded:
        raise SchemaError("receipt is not canonical JSON")
    validate_receipt(value)
    return value


def _exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    if type(value) is not dict:
        raise SchemaError(f"{label} must be an object")
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        unknown = sorted(actual - expected)
        raise SchemaError(
            f"{label} fields are not exact (missing={missing}, unknown={unknown})"
        )
    return value


def _exact_string(value: Any, expected: str, label: str) -> None:
    if type(value) is not str or value != expected:
        raise SchemaError(f"{label} is invalid")


def _string(value: Any, label: str, *, maximum: int = 4096) -> str:
    if type(value) is not str or not value or len(value) > maximum or "\x00" in value:
        raise SchemaError(f"{label} is invalid")
    return value


def _integer(
    value: Any,
    label: str,
    *,
    minimum: int = 0,
    maximum: int = (1 << 63) - 1,
) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise SchemaError(f"{label} is invalid")
    return value


def _boolean(value: Any, expected: bool, label: str) -> None:
    if type(value) is not bool or value is not expected:
        raise SchemaError(f"{label} is invalid")


def _sha256(value: Any, label: str) -> str:
    if type(value) is not str or SHA256_RE.fullmatch(value) is None:
        raise SchemaError(f"{label} is not lowercase SHA-256")
    return value


def _windows_executable(value: Any, label: str) -> str:
    path = _string(value, label, maximum=1024)
    if (
        WINDOWS_EXE_RE.fullmatch(path) is None
        or "/" in path
        or "\\.\\" in path
        or "\\..\\" in path
        or path.endswith("\\.")
    ):
        raise SchemaError(f"{label} is not a canonical local executable path")
    return path


FILE_IDENTITY_KEYS = {
    "volumeSerialNumber",
    "fileIndex",
    "fileSize",
    "lastWriteTime100ns",
}


def _validate_file_identity(value: Any, label: str) -> None:
    identity = _exact_keys(value, FILE_IDENTITY_KEYS, label)
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


def validate_process_binding(value: Any, label: str) -> None:
    process = _exact_keys(value, PROCESS_KEYS, label)
    _integer(process["pid"], f"{label}.pid", minimum=1, maximum=(1 << 32) - 1)
    _integer(
        process["processStartTime100ns"],
        f"{label}.processStartTime100ns",
        minimum=1,
    )
    _integer(
        process["sessionId"],
        f"{label}.sessionId",
        maximum=(1 << 32) - 1,
    )
    _windows_executable(process["executablePath"], f"{label}.executablePath")
    _sha256(process["executableSha256"], f"{label}.executableSha256")
    _validate_file_identity(process["fileIdentity"], f"{label}.fileIdentity")


TARGET_KEYS = PROCESS_KEYS | {
    "bindingSha256",
    "hwnd",
    "rootHwnd",
    "hostGeneration",
    "publisher",
}


def validate_target(value: Any) -> None:
    target = _exact_keys(value, TARGET_KEYS, "target")
    validate_process_binding(
        {key: target[key] for key in PROCESS_KEYS},
        "target.process",
    )
    _integer(target["hwnd"], "target.hwnd", minimum=1)
    _integer(target["rootHwnd"], "target.rootHwnd", minimum=1)
    _integer(target["hostGeneration"], "target.hostGeneration", minimum=1)
    _exact_string(target["publisher"], "Discord Inc.", "target.publisher")
    claimed = _sha256(target["bindingSha256"], "target.bindingSha256")
    actual = target_binding_digest(target)
    if not hmac.compare_digest(claimed, actual):
        raise SchemaError("target binding digest is invalid")


READBACK_KEYS = {
    "targetBindingSha256",
    "classification",
    "complete",
    "sha256",
    "byteLength",
    "utf16Length",
}


def _validate_readback(
    value: Any,
    *,
    label: str,
    binding: str,
    classification: str,
    digest: str,
    byte_length: int,
    utf16_length: int,
) -> None:
    readback = _exact_keys(value, READBACK_KEYS, label)
    if readback["targetBindingSha256"] != binding:
        raise SchemaError(f"{label} target binding changed")
    _exact_string(readback["classification"], classification, f"{label}.classification")
    _boolean(readback["complete"], True, f"{label}.complete")
    if not hmac.compare_digest(_sha256(readback["sha256"], f"{label}.sha256"), digest):
        raise SchemaError(f"{label} digest does not match")
    if _integer(readback["byteLength"], f"{label}.byteLength") != byte_length:
        raise SchemaError(f"{label} byte length does not match")
    if _integer(readback["utf16Length"], f"{label}.utf16Length") != utf16_length:
        raise SchemaError(f"{label} UTF-16 length does not match")


def _validate_carrier(value: Any, binding: str) -> tuple[str, int, int]:
    carrier = _exact_keys(
        value,
        {
            "targetBindingSha256",
            "utf8B64",
            "sha256",
            "byteLength",
            "utf16Length",
        },
        "carrier",
    )
    if carrier["targetBindingSha256"] != binding:
        raise SchemaError("carrier target binding changed")
    encoded = _string(carrier["utf8B64"], "carrier.utf8B64", maximum=16 * 1024)
    try:
        raw = base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError) as error:
        raise SchemaError("carrier.utf8B64 is invalid") from error
    if base64.b64encode(raw).decode("ascii") != encoded:
        raise SchemaError("carrier.utf8B64 is not canonical")
    if not raw or len(raw) > MAX_CARRIER_BYTES:
        raise SchemaError("carrier byte length is out of bounds")
    try:
        text = raw.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise SchemaError("carrier is not strict UTF-8") from error
    if any(ord(character) < 0x20 and character != "\n" for character in text):
        raise SchemaError("carrier contains a forbidden control character")
    digest = sha256_hex(raw)
    utf16_length = len(text.encode("utf-16-le")) // 2
    if not hmac.compare_digest(_sha256(carrier["sha256"], "carrier.sha256"), digest):
        raise SchemaError("carrier SHA-256 does not match its bytes")
    if _integer(carrier["byteLength"], "carrier.byteLength") != len(raw):
        raise SchemaError("carrier byte length does not match its bytes")
    if _integer(carrier["utf16Length"], "carrier.utf16Length") != utf16_length:
        raise SchemaError("carrier UTF-16 length does not match its bytes")
    return digest, len(raw), utf16_length


def validate_receipt(receipt: dict[str, Any]) -> None:
    root = _exact_keys(
        receipt,
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
        "receipt",
    )
    _exact_string(root["schema"], SCHEMA_NAME, "schema")
    if root["version"] != SCHEMA_VERSION or type(root["version"]) is not int:
        raise SchemaError("version is invalid")
    _exact_string(root["evidenceKind"], EVIDENCE_KIND, "evidenceKind")
    _sha256(root["challenge"], "challenge")
    _integer(root["emittedAtUnixMs"], "emittedAtUnixMs", minimum=1)

    stages = _exact_keys(
        root["monotonicStagesMs"],
        {
            "challengeClaimed",
            "preSendReadback",
            "sendInjected",
            "postContextRevalidated",
            "receiptEmitted",
        },
        "monotonicStagesMs",
    )
    ordered = [
        _integer(stages[name], f"monotonicStagesMs.{name}")
        for name in (
            "challengeClaimed",
            "preSendReadback",
            "sendInjected",
            "postContextRevalidated",
            "receiptEmitted",
        )
    ]
    if ordered != sorted(ordered) or ordered[0] != 0:
        raise SchemaError("monotonic stages are not ordered from zero")

    validate_process_binding(root["emitter"], "emitter")
    build = _exact_keys(
        root["build"],
        {
            "features",
            "debugAssertions",
            "targetOs",
            "targetArch",
            "profile",
        },
        "build",
    )
    if type(build["features"]) is not list or build["features"] != EXACT_SHIPPING_FEATURES:
        raise SchemaError("build feature set is not the exact shipping set")
    _boolean(build["debugAssertions"], False, "build.debugAssertions")
    _exact_string(build["targetOs"], "windows", "build.targetOs")
    _exact_string(build["targetArch"], "x86_64", "build.targetArch")
    _exact_string(build["profile"], "release", "build.profile")

    validate_target(root["target"])
    target = root["target"]
    binding = target["bindingSha256"]
    if root["emitter"]["pid"] == target["pid"]:
        raise SchemaError("emitter and Discord target may not be the same process")

    carrier_digest, carrier_bytes, carrier_utf16 = _validate_carrier(
        root["carrier"], binding
    )
    pre_send = _exact_keys(
        root["preSend"],
        {"targetBindingSha256", "readback", "foreground"},
        "preSend",
    )
    if pre_send["targetBindingSha256"] != binding:
        raise SchemaError("preSend target binding changed")
    _validate_readback(
        pre_send["readback"],
        label="preSend.readback",
        binding=binding,
        classification="exact",
        digest=carrier_digest,
        byte_length=carrier_bytes,
        utf16_length=carrier_utf16,
    )
    foreground = _exact_keys(
        pre_send["foreground"],
        {
            "targetBindingSha256",
            "foregroundHwnd",
            "foregroundRootHwnd",
            "foregroundPid",
            "targetRootHwnd",
            "keyboardFocusProven",
            "targetOwnedByTrustedProcess",
        },
        "preSend.foreground",
    )
    if foreground["targetBindingSha256"] != binding:
        raise SchemaError("foreground target binding changed")
    _integer(foreground["foregroundHwnd"], "preSend.foreground.foregroundHwnd", minimum=1)
    foreground_root = _integer(
        foreground["foregroundRootHwnd"],
        "preSend.foreground.foregroundRootHwnd",
        minimum=1,
    )
    target_root = _integer(
        foreground["targetRootHwnd"],
        "preSend.foreground.targetRootHwnd",
        minimum=1,
    )
    if foreground_root != target_root or target_root != target["rootHwnd"]:
        raise SchemaError("foreground root is not the exact Discord target root")
    if _integer(
        foreground["foregroundPid"],
        "preSend.foreground.foregroundPid",
        minimum=1,
    ) != target["pid"]:
        raise SchemaError("foreground PID is not the Discord target PID")
    _boolean(
        foreground["keyboardFocusProven"],
        True,
        "preSend.foreground.keyboardFocusProven",
    )
    _boolean(
        foreground["targetOwnedByTrustedProcess"],
        True,
        "preSend.foreground.targetOwnedByTrustedProcess",
    )

    action = _exact_keys(
        root["action"],
        {
            "targetBindingSha256",
            "mechanism",
            "attempted",
            "acceptedInputCount",
            "enterCertainty",
            "retryPolicy",
            "actionSequence",
        },
        "action",
    )
    if action["targetBindingSha256"] != binding:
        raise SchemaError("action target binding changed")
    _exact_string(action["mechanism"], "sendinput_enter", "action.mechanism")
    _boolean(action["attempted"], True, "action.attempted")
    if _integer(action["acceptedInputCount"], "action.acceptedInputCount") != 2:
        raise SchemaError("Enter injection was not exactly one key-down/key-up pair")
    _exact_string(action["enterCertainty"], "injected_once", "action.enterCertainty")
    _exact_string(action["retryPolicy"], "never_auto_retry", "action.retryPolicy")
    if _integer(action["actionSequence"], "action.actionSequence", minimum=1) != 1:
        raise SchemaError("send action sequence is not exactly one")

    post_send = _exact_keys(
        root["postSend"],
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
        "postSend",
    )
    if post_send["targetBindingSha256"] != binding:
        raise SchemaError("postSend target binding changed")
    empty_digest = sha256_hex(b"")
    _validate_readback(
        post_send["readback"],
        label="postSend.readback",
        binding=binding,
        classification="empty",
        digest=empty_digest,
        byte_length=0,
        utf16_length=0,
    )
    _boolean(post_send["hostRevalidated"], True, "postSend.hostRevalidated")
    _boolean(
        post_send["overlayContextUnchanged"],
        True,
        "postSend.overlayContextUnchanged",
    )
    _boolean(post_send["carrierConsumed"], True, "postSend.carrierConsumed")
    _boolean(post_send["sentRowProven"], True, "postSend.sentRowProven")
    if _integer(post_send["rowDelta"], "postSend.rowDelta") != 1:
        raise SchemaError("sent-row delta is not exactly +1")
    _exact_string(post_send["status"], "sent", "postSend.status")

    claimed_digest = _sha256(
        root["receiptDigestSha256"], "receiptDigestSha256"
    )
    calculated_digest = receipt_digest(root)
    if not hmac.compare_digest(claimed_digest, calculated_digest):
        raise SchemaError("receipt integrity digest does not match")
