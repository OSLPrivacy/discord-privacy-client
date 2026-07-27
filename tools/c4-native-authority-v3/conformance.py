"""Cross-language conformance boundary for C4 native-authority receipt v3.

The JSON Schema fixes all object shapes, primitive types, enums, ranges and
feature values. This module independently evaluates the relational and
cryptographic rules listed in ``x-c4-semanticRules`` before it invokes the
e601427 verifier. It is a verifier-side test boundary, not a transport or a
native authority source.
"""

from __future__ import annotations

import argparse
import base64
import binascii
import copy
import hashlib
import hmac
import json
from pathlib import Path
import sys
from typing import Any

from jsonschema import Draft202012Validator
from jsonschema.exceptions import SchemaError as JsonSchemaDefinitionError

from schema import MAX_RECEIPT_BYTES, WINDOWS_RESERVED_NAMES, canonical_json
from verify import VerificationContext, VerificationError, verify_receipt


HERE = Path(__file__).resolve().parent
BOUNDARY_PATH = HERE / "boundary.schema.json"
FIXTURE_PATH = HERE / "fixtures" / "conformance-v3.json"

SEMANTIC_RULES = (
    "receipt-canonical-json",
    "receipt-digest",
    "target-binding-digest",
    "carrier-base64-canonical",
    "carrier-hash-and-lengths",
    "target-binding-repeated",
    "pre-readback-exact-carrier",
    "foreground-exact-target",
    "single-enter-never-retry",
    "post-readback-empty-row-plus-one",
    "monotonic-stages-ordered",
    "monotonic-duration-within-wall-clock-elapsed",
    "challenge-fresh-and-matched",
    "emitter-expected-and-pipe-client-equal",
    "target-expected-equal",
    "windows-paths-lexically-canonical",
    "runtime-pipe-protections-all-true",
    "runtime-filesystem-authority-native-verifier",
    "runtime-filesystem-authority-after-receipt",
    "runtime-request-root-and-record-path-exact",
    "synthetic-never-runtime-or-full-c4",
    "runtime-receipt-never-full-c4",
    "point-delta-always-zero",
    "ledger-state-field-coherence",
    "ledger-record-digest",
    "ledger-record-filename-matches-challenge-hash",
    "ledger-root-stable-no-links",
    "ledger-recovery-before-work",
    "ledger-clock-monotonic",
    "ledger-watermark-durable-root-wide",
)
DIGEST_NAMES = {
    "receiptObjectDigestSha256",
    "targetBindingSha256",
    "ledgerRecordDigestSha256",
    "pipeBindingSha256",
    "challengeSha256",
    "receiptFrameSha256",
}


class ConformanceError(ValueError):
    """A boundary document or fixture violates the v3 contract."""


def _reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ConformanceError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def _load_json(path: Path) -> dict[str, Any]:
    try:
        raw = path.read_bytes()
        value = json.loads(
            raw.decode("utf-8", errors="strict"),
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=lambda item: (_ for _ in ()).throw(
                ConformanceError(f"invalid JSON number: {item}")
            ),
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ConformanceError(f"{path.name} is not strict JSON") from error
    if type(value) is not dict:
        raise ConformanceError(f"{path.name} root must be an object")
    return value


def load_boundary() -> dict[str, Any]:
    boundary = _load_json(BOUNDARY_PATH)
    _reject_ambiguous_scalars(boundary)
    try:
        Draft202012Validator.check_schema(boundary)
    except JsonSchemaDefinitionError as error:
        raise ConformanceError("boundary JSON Schema is invalid") from error
    if tuple(boundary.get("x-c4-semanticRules", ())) != SEMANTIC_RULES:
        raise ConformanceError("declared semantic rule set drifted")
    if set(boundary.get("x-c4-digests", {})) != DIGEST_NAMES:
        raise ConformanceError("declared digest names drifted")
    return boundary


def _subschema(boundary: dict[str, Any], name: str) -> dict[str, Any]:
    if name not in boundary["$defs"]:
        raise ConformanceError(f"unknown boundary definition: {name}")
    return {
        "$schema": boundary["$schema"],
        "$ref": f"#/$defs/{name}",
        "$defs": boundary["$defs"],
    }


def _schema_validate(
    boundary: dict[str, Any], name: str, instance: Any
) -> None:
    _reject_ambiguous_scalars(instance)
    errors = sorted(
        Draft202012Validator(_subschema(boundary, name)).iter_errors(instance),
        key=lambda error: tuple(str(item) for item in error.absolute_path),
    )
    if errors:
        first = errors[0]
        path = "/".join(str(item) for item in first.absolute_path) or "<root>"
        raise ConformanceError(f"{name} schema rejected {path}: {first.message}")


def _reject_ambiguous_scalars(value: Any) -> None:
    if type(value) is float:
        raise ConformanceError("floating-point JSON numbers are forbidden")
    if type(value) is str and any(0xD800 <= ord(character) <= 0xDFFF for character in value):
        raise ConformanceError("Unicode surrogate code points are forbidden")
    if type(value) is dict:
        for key, item in value.items():
            _reject_ambiguous_scalars(key)
            _reject_ambiguous_scalars(item)
    elif type(value) is list:
        for item in value:
            _reject_ambiguous_scalars(item)


def validate_document(
    name: str,
    instance: Any,
    *,
    boundary: dict[str, Any] | None = None,
) -> None:
    """Validate one named declarative shape without granting authority."""

    _schema_validate(boundary or load_boundary(), name, instance)


def _sha256(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _digest(
    boundary: dict[str, Any],
    digest_name: str,
    value: dict[str, Any],
) -> str:
    definition = boundary["x-c4-digests"][digest_name]
    if (
        definition["algorithm"] != "SHA-256"
        or definition["inputKind"] != "canonical-json-object-minus-field"
        or type(definition["omitField"]) is not str
    ):
        raise ConformanceError(f"{digest_name} algorithm drifted")
    domain = definition["domainSeparatorUtf8"].encode("utf-8")
    terminator = bytes.fromhex(definition["domainTerminatorHex"])
    body = copy.deepcopy(value)
    omitted = definition["omitField"]
    if omitted not in body:
        raise ConformanceError(f"{digest_name} omitted field is absent")
    body.pop(omitted)
    return _sha256(domain + terminator + canonical_json(body))


def _canonical_b64_decode(value: Any, label: str, *, maximum: int) -> bytes:
    if type(value) is not str:
        raise ConformanceError(f"{label} must be a string")
    try:
        decoded = base64.b64decode(value, validate=True)
    except (binascii.Error, ValueError) as error:
        raise ConformanceError(f"{label} is not strict Base64") from error
    if base64.b64encode(decoded).decode("ascii") != value:
        raise ConformanceError(f"{label} is not canonical Base64")
    if not decoded or len(decoded) > maximum:
        raise ConformanceError(f"{label} decoded length is invalid")
    return decoded


def _parse_receipt_frame(
    boundary: dict[str, Any], encoded: Any
) -> tuple[bytes, dict[str, Any]]:
    raw = _canonical_b64_decode(
        encoded,
        "receiptUtf8B64",
        maximum=MAX_RECEIPT_BYTES,
    )
    try:
        receipt = json.loads(
            raw.decode("utf-8", errors="strict"),
            object_pairs_hook=_reject_duplicate_keys,
            parse_constant=lambda item: (_ for _ in ()).throw(
                ConformanceError(f"invalid receipt number: {item}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ConformanceError("receipt frame is not strict JSON") from error
    if type(receipt) is not dict:
        raise ConformanceError("receipt frame root must be an object")
    try:
        canonical = canonical_json(receipt)
    except (UnicodeEncodeError, ValueError) as error:
        raise ConformanceError("receipt frame cannot be canonically encoded") from error
    if raw != canonical:
        raise ConformanceError("receipt frame is not canonical JSON")
    _schema_validate(boundary, "nativeReceipt", receipt)
    return raw, receipt


def _path_normalized_process(value: dict[str, Any]) -> dict[str, Any]:
    normalized = copy.deepcopy(value)
    path = normalized["executablePath"]
    parts = path[3:].split("\\")
    if (
        not path.isascii()
        or not path.isprintable()
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
        raise ConformanceError("executable path is not canonical")
    normalized["executablePath"] = path.casefold()
    return normalized


def _require_canonical_windows_path(value: str, label: str) -> None:
    parts = value[3:].split("\\")
    if (
        not value.isascii()
        or not value.isprintable()
        or "/" in value
        or "\\\\" in value
        or any(
            not part
            or part in {".", ".."}
            or part.endswith((".", " "))
            or part.split(".", 1)[0].upper() in WINDOWS_RESERVED_NAMES
            for part in parts
        )
    ):
        raise ConformanceError(f"{label} is not lexically canonical")


def _process_equal(left: dict[str, Any], right: dict[str, Any]) -> bool:
    return hmac.compare_digest(
        canonical_json(_path_normalized_process(left)),
        canonical_json(_path_normalized_process(right)),
    )


def _target_identity(value: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(value)
    result.pop("bindingSha256", None)
    return _path_normalized_process(result)


def _target_equal(left: dict[str, Any], right: dict[str, Any]) -> bool:
    return hmac.compare_digest(
        canonical_json(_target_identity(left)),
        canonical_json(_target_identity(right)),
    )


def _require_equal(left: Any, right: Any, label: str) -> None:
    if left != right:
        raise ConformanceError(f"{label} mismatch")


def _validate_receipt_semantics(
    boundary: dict[str, Any], receipt: dict[str, Any]
) -> None:
    receipt_object_digest = _digest(
        boundary,
        "receiptObjectDigestSha256",
        receipt,
    )
    if not hmac.compare_digest(
        receipt["receiptDigestSha256"],
        receipt_object_digest,
    ):
        raise ConformanceError("receipt digest mismatch")

    target = receipt["target"]
    target_digest = _digest(
        boundary,
        "targetBindingSha256",
        target,
    )
    if not hmac.compare_digest(target["bindingSha256"], target_digest):
        raise ConformanceError("target binding digest mismatch")
    if receipt["emitter"]["pid"] == target["pid"]:
        raise ConformanceError("emitter and target process must differ")
    _path_normalized_process(receipt["emitter"])
    _target_identity(target)

    stages = receipt["monotonicStagesMs"]
    ordered = [
        stages["challengeClaimed"],
        stages["preSendReadback"],
        stages["sendInjected"],
        stages["postContextRevalidated"],
        stages["receiptEmitted"],
    ]
    if ordered[0] != 0 or ordered != sorted(ordered):
        raise ConformanceError("monotonic stages are not ordered from zero")

    carrier = receipt["carrier"]
    carrier_raw = _canonical_b64_decode(
        carrier["utf8B64"], "carrier.utf8B64", maximum=8192
    )
    try:
        carrier_text = carrier_raw.decode("utf-8", errors="strict")
    except UnicodeDecodeError as error:
        raise ConformanceError("carrier is not strict UTF-8") from error
    if any(ord(character) < 0x20 and character != "\n" for character in carrier_text):
        raise ConformanceError("carrier contains a forbidden control character")
    carrier_sha256 = _sha256(carrier_raw)
    carrier_utf16 = len(carrier_text.encode("utf-16-le")) // 2
    _require_equal(carrier["sha256"], carrier_sha256, "carrier hash")
    _require_equal(carrier["byteLength"], len(carrier_raw), "carrier byte length")
    _require_equal(carrier["utf16Length"], carrier_utf16, "carrier UTF-16 length")

    binding_paths = (
        carrier,
        receipt["preSend"],
        receipt["preSend"]["readback"],
        receipt["preSend"]["foreground"],
        receipt["action"],
        receipt["postSend"],
        receipt["postSend"]["readback"],
    )
    for value in binding_paths:
        _require_equal(
            value["targetBindingSha256"],
            target_digest,
            "repeated target binding",
        )

    pre = receipt["preSend"]["readback"]
    _require_equal(pre["classification"], "exact", "pre-readback classification")
    _require_equal(pre["complete"], True, "pre-readback completeness")
    _require_equal(pre["sha256"], carrier_sha256, "pre-readback hash")
    _require_equal(pre["byteLength"], len(carrier_raw), "pre-readback byte length")
    _require_equal(pre["utf16Length"], carrier_utf16, "pre-readback UTF-16 length")

    foreground = receipt["preSend"]["foreground"]
    _require_equal(
        foreground["foregroundRootHwnd"],
        target["rootHwnd"],
        "foreground root HWND",
    )
    _require_equal(
        foreground["targetRootHwnd"],
        target["rootHwnd"],
        "foreground target root HWND",
    )
    _require_equal(foreground["foregroundPid"], target["pid"], "foreground PID")
    _require_equal(foreground["keyboardFocusProven"], True, "keyboard focus")
    _require_equal(
        foreground["targetOwnedByTrustedProcess"],
        True,
        "foreground trusted ownership",
    )

    action = receipt["action"]
    for key, expected in (
        ("mechanism", "sendinput_enter"),
        ("attempted", True),
        ("acceptedInputCount", 2),
        ("enterCertainty", "injected_once"),
        ("retryPolicy", "never_auto_retry"),
        ("actionSequence", 1),
    ):
        _require_equal(action[key], expected, f"action.{key}")

    post = receipt["postSend"]
    post_readback = post["readback"]
    for key, expected in (
        ("classification", "empty"),
        ("complete", True),
        ("sha256", _sha256(b"")),
        ("byteLength", 0),
        ("utf16Length", 0),
    ):
        _require_equal(post_readback[key], expected, f"post-readback {key}")
    for key, expected in (
        ("hostRevalidated", True),
        ("overlayContextUnchanged", True),
        ("carrierConsumed", True),
        ("sentRowProven", True),
        ("rowDelta", 1),
        ("status", "sent"),
    ):
        _require_equal(post[key], expected, f"postSend.{key}")


def validate_request(
    request: dict[str, Any],
    *,
    boundary: dict[str, Any] | None = None,
) -> tuple[bytes, dict[str, Any]]:
    """Validate the entire declarative boundary without granting authority."""

    active_boundary = boundary or load_boundary()
    _schema_validate(active_boundary, "verificationRequest", request)
    raw, receipt = _parse_receipt_frame(
        active_boundary, request["receiptUtf8B64"]
    )
    _validate_receipt_semantics(active_boundary, receipt)

    challenge = request["challenge"]
    issued = challenge["issuedAtUnixMs"]
    expires = challenge["expiresAtUnixMs"]
    now = request["nowUnixMs"]
    if not issued < expires <= issued + 60_000:
        raise ConformanceError("challenge lifetime is invalid")
    if now < issued:
        raise ConformanceError("challenge was issued in the future")
    if now >= expires:
        raise ConformanceError("challenge is stale")
    _require_equal(receipt["challenge"], challenge["challenge"], "challenge")
    if not issued <= receipt["emittedAtUnixMs"] <= now:
        raise ConformanceError("receipt emission is outside challenge lifetime")
    if (
        receipt["monotonicStagesMs"]["receiptEmitted"]
        > receipt["emittedAtUnixMs"] - issued
    ):
        raise ConformanceError("receipt monotonic duration exceeds elapsed wall time")

    if not _process_equal(receipt["emitter"], request["expectedEmitter"]):
        raise ConformanceError("receipt emitter differs from expected emitter")
    if not _process_equal(request["pipeClient"], request["expectedEmitter"]):
        raise ConformanceError("pipe client differs from expected emitter")
    if not _process_equal(receipt["emitter"], request["pipeClient"]):
        raise ConformanceError("receipt emitter differs from pipe client")
    if not _target_equal(receipt["target"], request["expectedTarget"]):
        raise ConformanceError("receipt target differs from expected target")
    if request["source"] == "runtime_named_pipe" and not all(
        request["pipeProtections"].values()
    ):
        raise ConformanceError("runtime pipe protections are incomplete")
    attestation = request["filesystemAuthorityAttestation"]
    if request["source"] == "synthetic":
        if attestation is not None:
            raise ConformanceError("synthetic request carries filesystem authority")
    else:
        _require_canonical_windows_path(
            attestation["ledgerRoot"],
            "filesystem attestation root",
        )
        _require_canonical_windows_path(
            attestation["ledgerRecordPath"],
            "filesystem attestation record path",
        )
        _require_equal(
            attestation["challenge"],
            challenge["challenge"],
            "filesystem attestation challenge",
        )
        if not receipt["emittedAtUnixMs"] <= attestation["checkedAtUnixMs"] <= now:
            raise ConformanceError("filesystem attestation time is invalid")
        challenge_hash = _sha256(bytes.fromhex(challenge["challenge"]))
        expected_record_path = (
            attestation["ledgerRoot"] + "\\" + challenge_hash + ".json"
        )
        _require_equal(
            attestation["ledgerRecordPath"],
            expected_record_path,
            "filesystem attestation request path",
        )
    return raw, receipt


def request_context(request: dict[str, Any]) -> VerificationContext:
    challenge = request["challenge"]
    protections = request["pipeProtections"]
    return VerificationContext(
        challenge=challenge["challenge"],
        issued_at_unix_ms=challenge["issuedAtUnixMs"],
        now_unix_ms=request["nowUnixMs"],
        expires_at_unix_ms=challenge["expiresAtUnixMs"],
        source=request["source"],
        expected_emitter=copy.deepcopy(request["expectedEmitter"]),
        pipe_client=copy.deepcopy(request["pipeClient"]),
        expected_target=copy.deepcopy(request["expectedTarget"]),
        pipe_first_instance=protections["firstInstance"],
        pipe_remote_clients_rejected=protections["remoteClientsRejected"],
        pipe_acl_exact_user=protections["aclExactUser"],
        filesystem_authority_attestation=copy.deepcopy(
            request["filesystemAuthorityAttestation"]
        ),
    )


def verify_synthetic_request(
    request: dict[str, Any],
    *,
    boundary: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Run the parser-only vector; runtime sources are intentionally refused."""

    active_boundary = boundary or load_boundary()
    if request.get("source") != "synthetic":
        raise ConformanceError("conformance runner accepts synthetic source only")
    raw, _ = validate_request(request, boundary=active_boundary)
    try:
        verdict = verify_receipt(raw, request_context(request))
    except VerificationError as error:
        raise ConformanceError(
            f"Python verifier rejected boundary-valid input: {error}"
        ) from error
    result = {
        "schema": "osl.c4.native-verification-result",
        "version": 3,
        "source": "synthetic",
        "status": verdict.status,
        "parserCryptoValid": verdict.parser_crypto_valid,
        "runtimeReceiptAccepted": verdict.runtime_receipt_accepted,
        "fullC4Success": verdict.full_c4_success,
        "pointDelta": verdict.point_delta,
        "receiptFrameSha256": verdict.receipt_frame_sha256,
    }
    _schema_validate(active_boundary, "verificationResult", result)
    if (
        result["runtimeReceiptAccepted"]
        or result["fullC4Success"]
        or result["pointDelta"] != 0
    ):
        raise ConformanceError("synthetic result crossed an authority boundary")
    return result


def validate_ledger_record(
    record: dict[str, Any],
    *,
    boundary: dict[str, Any] | None = None,
) -> None:
    active_boundary = boundary or load_boundary()
    _schema_validate(active_boundary, "ledgerRecord", record)
    if not (
        record["issuedAtUnixMs"]
        <= record["updatedAtUnixMs"]
        and record["issuedAtUnixMs"]
        < record["expiresAtUnixMs"]
        <= record["issuedAtUnixMs"] + 60_000
    ):
        raise ConformanceError("ledger timestamps are incoherent")
    if record["state"] == "expired":
        if record["updatedAtUnixMs"] < record["expiresAtUnixMs"]:
            raise ConformanceError("expired ledger timestamp is incoherent")
    elif record["state"] == "issued":
        if record["updatedAtUnixMs"] != record["issuedAtUnixMs"]:
            raise ConformanceError("issued ledger timestamp is incoherent")
    elif record["updatedAtUnixMs"] >= record["expiresAtUnixMs"]:
        raise ConformanceError("non-expired ledger timestamp is incoherent")
    expected = _digest(
        active_boundary,
        "ledgerRecordDigestSha256",
        record,
    )
    if not hmac.compare_digest(record["recordDigestSha256"], expected):
        raise ConformanceError("ledger record digest mismatch")


def _json_pointer_set(document: dict[str, Any], pointer: str, value: Any) -> None:
    if not pointer.startswith("/"):
        raise ConformanceError("mutation path is not an absolute JSON Pointer")
    parts = [
        part.replace("~1", "/").replace("~0", "~")
        for part in pointer[1:].split("/")
    ]
    target: Any = document
    for part in parts[:-1]:
        if type(target) is not dict or part not in target:
            raise ConformanceError(f"mutation path does not exist: {pointer}")
        target = target[part]
    if type(target) is not dict:
        raise ConformanceError(f"mutation parent is not an object: {pointer}")
    target[parts[-1]] = copy.deepcopy(value)


def _json_pointer_delete(document: dict[str, Any], pointer: str) -> None:
    if not pointer.startswith("/"):
        raise ConformanceError("mutation path is not an absolute JSON Pointer")
    parts = [
        part.replace("~1", "/").replace("~0", "~")
        for part in pointer[1:].split("/")
    ]
    target: Any = document
    for part in parts[:-1]:
        if type(target) is not dict or part not in target:
            raise ConformanceError(f"mutation path does not exist: {pointer}")
        target = target[part]
    if type(target) is not dict or parts[-1] not in target:
        raise ConformanceError(f"mutation path does not exist: {pointer}")
    del target[parts[-1]]


def _seal_target(boundary: dict[str, Any], target: dict[str, Any]) -> None:
    target["bindingSha256"] = _digest(boundary, "targetBindingSha256", target)


def _repeat_target_binding(receipt: dict[str, Any]) -> None:
    binding = receipt["target"]["bindingSha256"]
    for value in (
        receipt["carrier"],
        receipt["preSend"],
        receipt["preSend"]["readback"],
        receipt["preSend"]["foreground"],
        receipt["action"],
        receipt["postSend"],
        receipt["postSend"]["readback"],
    ):
        value["targetBindingSha256"] = binding


def _seal_receipt(boundary: dict[str, Any], receipt: dict[str, Any]) -> None:
    receipt["receiptDigestSha256"] = _digest(
        boundary,
        "receiptObjectDigestSha256",
        receipt,
    )


def materialize_case(
    fixture: dict[str, Any],
    case: dict[str, Any] | None,
    *,
    boundary: dict[str, Any],
) -> dict[str, Any]:
    request = copy.deepcopy(fixture["validRequest"])
    receipt = copy.deepcopy(fixture["validReceipt"])
    operation = case.get("operation") if case is not None else None
    if case is not None and operation in (None, "delete_field"):
        target_name = case["target"]
        target = request if target_name == "request" else receipt
        if operation == "delete_field":
            _json_pointer_delete(target, case["path"])
        else:
            _json_pointer_set(target, case["path"], case["value"])
        if case.get("rebindTarget"):
            _seal_target(boundary, receipt["target"])
            _repeat_target_binding(receipt)
        if case.get("resealReceipt"):
            _seal_receipt(boundary, receipt)
    raw = canonical_json(receipt)
    if operation == "duplicate_receipt_version":
        raw = raw[:-1] + b',"version":3}'
    elif operation == "append_receipt_newline":
        raw += b"\n"
    elif operation not in (None, "delete_field"):
        raise ConformanceError(f"unknown fixture operation: {operation}")
    request["receiptUtf8B64"] = base64.b64encode(raw).decode("ascii")
    return request


def run_fixture(
    fixture_path: Path = FIXTURE_PATH,
    *,
    boundary: dict[str, Any] | None = None,
) -> dict[str, Any]:
    active_boundary = boundary or load_boundary()
    fixture = _load_json(fixture_path)
    expected_keys = {
        "schema",
        "version",
        "validRequest",
        "validReceipt",
        "expectedSyntheticResult",
        "invalidMutations",
    }
    if set(fixture) != expected_keys:
        raise ConformanceError("fixture root fields are not exact")
    if (
        fixture["schema"] != "osl.c4.native-conformance-fixture"
        or fixture["version"] != 3
    ):
        raise ConformanceError("fixture identity is invalid")
    valid_request = materialize_case(fixture, None, boundary=active_boundary)
    valid_result = verify_synthetic_request(
        valid_request, boundary=active_boundary
    )
    if valid_result != fixture["expectedSyntheticResult"]:
        raise ConformanceError("synthetic result drifted from fixture")

    invalid = fixture["invalidMutations"]
    if type(invalid) is not list or not invalid:
        raise ConformanceError("invalid mutation corpus is empty")
    names: set[str] = set()
    rejected = 0
    for case in invalid:
        if type(case) is not dict or set(case) not in (
            {"name", "target", "path", "value"},
            {"name", "target", "path", "value", "resealReceipt"},
            {
                "name",
                "target",
                "path",
                "value",
                "rebindTarget",
                "resealReceipt",
            },
            {"name", "operation"},
            {"name", "operation", "target", "path"},
        ):
            raise ConformanceError("mutation fields are not exact")
        name = case["name"]
        if type(name) is not str or not name or name in names:
            raise ConformanceError("mutation names are invalid or duplicated")
        names.add(name)
        request = materialize_case(fixture, case, boundary=active_boundary)
        try:
            verify_synthetic_request(request, boundary=active_boundary)
        except ConformanceError:
            rejected += 1
        else:
            raise ConformanceError(f"invalid mutation was accepted: {name}")
    return {
        "schema": "osl.c4.native-conformance-run",
        "version": 3,
        "syntheticParserCryptoValid": True,
        "runtimeReceiptAccepted": False,
        "fullC4Success": False,
        "pointDelta": 0,
        "invalidMutationsRejected": rejected,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", type=Path, default=FIXTURE_PATH)
    arguments = parser.parse_args()
    try:
        result = run_fixture(arguments.fixture)
    except ConformanceError as error:
        print(
            json.dumps(
                {
                    "schema": "osl.c4.native-conformance-run",
                    "version": 3,
                    "status": "invalid",
                    "error": str(error),
                    "runtimeReceiptAccepted": False,
                    "fullC4Success": False,
                    "pointDelta": 0,
                },
                sort_keys=True,
                separators=(",", ":"),
            )
        )
        return 1
    print(json.dumps(result, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
