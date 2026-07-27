"""Verifier-side C4 native receipt v3 authority checks.

The pipe server is intentionally out of scope here. Its independently observed
client process facts are mandatory inputs to this verifier; caller-authored
receipt fields never substitute for them.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass
import hmac
from os import PathLike
import re
from typing import Any, Literal, Protocol

from ledger import LedgerError, OneShotLedger, pipe_binding_digest
from schema import (
    MAX_RECEIPT_BYTES,
    PROCESS_KEYS,
    SHA256_RE,
    SchemaError,
    TARGET_KEYS,
    canonical_json,
    parse_receipt,
    sha256_hex,
    validate_process_binding,
    validate_target,
)


EvidenceSource = Literal["synthetic", "runtime_named_pipe"]
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
WINDOWS_FILE_RE = re.compile(
    r"^[A-Za-z]:\\(?:[^\\/:*?\"<>|\x00]+\\)*[^\\/:*?\"<>|\x00]+\.json$"
)
WINDOWS_RESERVED_NAMES = {
    "CON",
    "PRN",
    "AUX",
    "NUL",
    *(f"COM{index}" for index in range(1, 10)),
    *(f"LPT{index}" for index in range(1, 10)),
}


class VerificationError(ValueError):
    """Evidence did not meet the v3 native-authority acceptance contract."""


class FilesystemAuthorityVerifier(Protocol):
    """Future native-owned proof boundary; this package has no implementation.

    The integration owner must supply an implementation backed by Windows
    handle identity and DACL inspection. Caller JSON or Linux path checks are
    never sufficient to return ``True``.
    """

    def verify_filesystem_authority(
        self,
        *,
        ledger_root: PathLike[str],
        ledger_record_path: PathLike[str],
        attestation: dict[str, Any],
        challenge: str,
        expected_emitter: dict[str, Any],
        now_unix_ms: int,
    ) -> bool:
        """Return the literal boolean True only for a native-proven binding."""


@dataclass(frozen=True)
class VerificationContext:
    challenge: str
    issued_at_unix_ms: int
    now_unix_ms: int
    expires_at_unix_ms: int
    source: EvidenceSource
    expected_emitter: dict[str, Any]
    pipe_client: dict[str, Any]
    expected_target: dict[str, Any]
    pipe_first_instance: bool
    pipe_remote_clients_rejected: bool
    pipe_acl_exact_user: bool
    filesystem_authority_attestation: dict[str, Any] | None = None


@dataclass(frozen=True)
class VerificationVerdict:
    status: str
    parser_crypto_valid: bool
    runtime_receipt_accepted: bool
    full_c4_success: bool
    point_delta: int
    receipt_frame_sha256: str


def _normalized_process(process: dict[str, Any]) -> dict[str, Any]:
    normalized = copy.deepcopy(process)
    normalized["executablePath"] = normalized["executablePath"].casefold()
    return normalized


def _process_equal(left: dict[str, Any], right: dict[str, Any]) -> bool:
    return hmac.compare_digest(
        canonical_json(_normalized_process(left)),
        canonical_json(_normalized_process(right)),
    )


def _target_identity(target: dict[str, Any]) -> dict[str, Any]:
    return {key: target[key] for key in TARGET_KEYS if key != "bindingSha256"}


def _target_equal(left: dict[str, Any], right: dict[str, Any]) -> bool:
    normalized_left = _target_identity(left)
    normalized_right = _target_identity(right)
    normalized_left["executablePath"] = normalized_left["executablePath"].casefold()
    normalized_right["executablePath"] = normalized_right["executablePath"].casefold()
    return hmac.compare_digest(
        canonical_json(normalized_left),
        canonical_json(normalized_right),
    )


def _validate_filesystem_attestation(
    attestation: Any,
    context: VerificationContext,
) -> dict[str, Any]:
    if type(attestation) is not dict or set(attestation) != FILESYSTEM_ATTESTATION_KEYS:
        raise VerificationError("filesystem attestation fields are not exact")
    if (
        attestation["schema"] != "osl.c4.filesystem-authority-attestation"
        or type(attestation["schema"]) is not str
        or attestation["version"] != 3
        or type(attestation["version"]) is not int
        or attestation["authoritySource"] != "native_windows_owner"
        or type(attestation["authoritySource"]) is not str
    ):
        raise VerificationError("filesystem attestation identity is invalid")
    if (
        type(attestation["challenge"]) is not str
        or not hmac.compare_digest(attestation["challenge"], context.challenge)
    ):
        raise VerificationError("filesystem attestation challenge is invalid")
    checked = attestation["checkedAtUnixMs"]
    if type(checked) is not int or not (
        context.issued_at_unix_ms <= checked <= context.now_unix_ms
    ):
        raise VerificationError("filesystem attestation time is invalid")
    root = _validate_windows_path(
        attestation["ledgerRoot"],
        "filesystem attestation root",
        file=False,
    )
    record_path = _validate_windows_path(
        attestation["ledgerRecordPath"],
        "filesystem attestation record path",
        file=True,
    )
    if not record_path.startswith(root + "\\"):
        raise VerificationError("filesystem attestation record escapes its ledger root")
    identity = attestation["rootIdentity"]
    if type(identity) is not dict or set(identity) != ROOT_IDENTITY_KEYS:
        raise VerificationError("filesystem root identity fields are not exact")
    for key in ROOT_IDENTITY_KEYS:
        if (
            type(identity[key]) is not int
            or not 1 <= identity[key] <= (1 << 63) - 1
        ):
            raise VerificationError("filesystem root identity is invalid")
    for key in ("daclSha256", "ownerSidSha256"):
        if (
            type(attestation[key]) is not str
            or SHA256_RE.fullmatch(attestation[key]) is None
        ):
            raise VerificationError(f"filesystem attestation {key} is invalid")
    return attestation


def _validate_windows_path(value: Any, label: str, *, file: bool) -> str:
    pattern = WINDOWS_FILE_RE if file else WINDOWS_DIRECTORY_RE
    if (
        type(value) is not str
        or not value.isascii()
        or not value.isprintable()
        or pattern.fullmatch(value) is None
        or "/" in value
        or "\\\\" in value
    ):
        raise VerificationError(f"{label} is invalid")
    parts = value[3:].split("\\")
    if not parts or any(
        not part
        or part in {".", ".."}
        or part.endswith((".", " "))
        or part.split(".", 1)[0].upper() in WINDOWS_RESERVED_NAMES
        for part in parts
    ):
        raise VerificationError(f"{label} is not lexically canonical")
    return value


def _validate_context(context: VerificationContext) -> None:
    if type(context.challenge) is not str or SHA256_RE.fullmatch(context.challenge) is None:
        raise VerificationError("expected challenge is invalid")
    if context.challenge == "0" * 64:
        raise VerificationError("zero expected challenge is forbidden")
    for label, value in (
        ("issued_at_unix_ms", context.issued_at_unix_ms),
        ("now_unix_ms", context.now_unix_ms),
        ("expires_at_unix_ms", context.expires_at_unix_ms),
    ):
        if type(value) is not int or not 0 <= value <= (1 << 63) - 1:
            raise VerificationError(f"{label} is invalid")
    if not (
        context.issued_at_unix_ms
        <= context.now_unix_ms
        < context.expires_at_unix_ms
        <= context.issued_at_unix_ms + 60_000
    ):
        raise VerificationError("challenge lifetime is invalid")
    if context.source not in ("synthetic", "runtime_named_pipe"):
        raise VerificationError("evidence source is unknown")
    if type(context.expected_emitter) is not dict or set(
        context.expected_emitter
    ) != PROCESS_KEYS:
        raise VerificationError("expected emitter fields are not exact")
    if type(context.pipe_client) is not dict or set(context.pipe_client) != PROCESS_KEYS:
        raise VerificationError("pipe client fields are not exact")
    if type(context.expected_target) is not dict or set(
        context.expected_target
    ) != TARGET_KEYS:
        raise VerificationError("expected target fields are not exact")
    try:
        validate_process_binding(context.expected_emitter, "expectedEmitter")
        validate_process_binding(context.pipe_client, "pipeClient")
        validate_target(context.expected_target)
    except SchemaError as error:
        raise VerificationError("independent process binding is invalid") from error
    for label, value in (
        ("pipe_first_instance", context.pipe_first_instance),
        ("pipe_remote_clients_rejected", context.pipe_remote_clients_rejected),
        ("pipe_acl_exact_user", context.pipe_acl_exact_user),
    ):
        if type(value) is not bool:
            raise VerificationError(f"{label} must be a literal boolean")
    if context.source == "synthetic":
        if context.filesystem_authority_attestation is not None:
            raise VerificationError("synthetic evidence may not carry native authority")
    else:
        if not (
            context.pipe_first_instance is True
            and context.pipe_remote_clients_rejected is True
            and context.pipe_acl_exact_user is True
        ):
            raise VerificationError(
                "runtime pipe protections were not independently proven"
            )
        _validate_filesystem_attestation(
            context.filesystem_authority_attestation,
            context,
        )


def verify_receipt(
    encoded: bytes,
    context: VerificationContext,
    *,
    ledger: OneShotLedger | None = None,
    filesystem_authority_verifier: FilesystemAuthorityVerifier | None = None,
) -> VerificationVerdict:
    """Validate one raw frame.

    Synthetic input can prove only parser/digest behavior. Even a byte-perfect
    synthetic receipt is never returned as runtime or full-C4 success.
    """

    _validate_context(context)
    if type(encoded) is not bytes or not encoded or len(encoded) > MAX_RECEIPT_BYTES:
        raise VerificationError("receipt frame length is invalid")
    try:
        receipt = parse_receipt(encoded)
    except ValueError as error:
        raise VerificationError(str(error)) from error

    if not hmac.compare_digest(receipt["challenge"], context.challenge):
        raise VerificationError("receipt challenge does not match the issued challenge")
    emitted = receipt["emittedAtUnixMs"]
    if not context.issued_at_unix_ms <= emitted <= context.now_unix_ms:
        raise VerificationError("receipt was emitted outside the challenge lifetime")
    monotonic_duration = receipt["monotonicStagesMs"]["receiptEmitted"]
    if monotonic_duration > emitted - context.issued_at_unix_ms:
        raise VerificationError("native monotonic duration exceeds elapsed wall time")
    if not _process_equal(receipt["emitter"], context.expected_emitter):
        raise VerificationError("receipt emitter does not match the expected executable")
    if not _process_equal(context.pipe_client, context.expected_emitter):
        raise VerificationError("named-pipe client is not the expected emitter process")
    if not _process_equal(receipt["emitter"], context.pipe_client):
        raise VerificationError("receipt emitter is not the OS-observed pipe client")
    if not _target_equal(receipt["target"], context.expected_target):
        raise VerificationError("Discord target does not match the independent target binding")

    raw_digest = sha256_hex(encoded)
    if context.source == "synthetic":
        if ledger is not None or filesystem_authority_verifier is not None:
            raise VerificationError("synthetic evidence may not carry runtime authority")
        return VerificationVerdict(
            status="synthetic-parser-crypto-valid",
            parser_crypto_valid=True,
            runtime_receipt_accepted=False,
            full_c4_success=False,
            point_delta=0,
            receipt_frame_sha256=raw_digest,
        )

    if ledger is None:
        raise VerificationError("runtime receipt requires the one-shot authority ledger")
    if not ledger.recovery_complete:
        raise VerificationError("runtime ledger recovery was not completed")
    try:
        ledger_record = ledger.read(context.challenge)
    except LedgerError as error:
        raise VerificationError("runtime ledger challenge could not be bound") from error
    if (
        ledger_record["issuedAtUnixMs"] != context.issued_at_unix_ms
        or ledger_record["expiresAtUnixMs"] != context.expires_at_unix_ms
    ):
        raise VerificationError("runtime request window differs from the ledger")
    if ledger_record["state"] != "connected":
        raise VerificationError("runtime ledger challenge is not connected")
    if ledger_record["updatedAtUnixMs"] > context.now_unix_ms:
        raise VerificationError("runtime ledger transition is in the future")
    try:
        expected_pipe_binding = pipe_binding_digest(context.pipe_client)
    except LedgerError as error:
        raise VerificationError("runtime pipe binding is invalid") from error
    if not hmac.compare_digest(
        ledger_record["pipeBindingSha256"] or "",
        expected_pipe_binding,
    ):
        raise VerificationError("runtime request pipe differs from the ledger")
    if filesystem_authority_verifier is None:
        raise VerificationError(
            "runtime receipt requires a native filesystem-authority verifier"
        )
    attestation = _validate_filesystem_attestation(
        context.filesystem_authority_attestation,
        context,
    )
    record_path = ledger.record_path(context.challenge)
    if attestation["ledgerRoot"] != str(ledger.root):
        raise VerificationError("filesystem attestation names another ledger root")
    if attestation["ledgerRecordPath"] != str(record_path):
        raise VerificationError("filesystem attestation names another request path")
    if attestation["checkedAtUnixMs"] < emitted:
        raise VerificationError("filesystem authority predates receipt emission")
    try:
        authority_proven = (
            filesystem_authority_verifier.verify_filesystem_authority(
                ledger_root=ledger.root,
                ledger_record_path=record_path,
                attestation=copy.deepcopy(attestation),
                challenge=context.challenge,
                expected_emitter=copy.deepcopy(context.expected_emitter),
                now_unix_ms=context.now_unix_ms,
            )
        )
    except Exception as error:
        raise VerificationError("native filesystem authority could not be proven") from error
    if type(authority_proven) is not bool or authority_proven is not True:
        raise VerificationError("native filesystem authority was not proven")
    try:
        ledger.consume(
            context.challenge,
            context.pipe_client,
            raw_digest,
            context.now_unix_ms,
        )
    except LedgerError as error:
        raise VerificationError(str(error)) from error
    return VerificationVerdict(
        status="runtime-native-receipt-valid",
        parser_crypto_valid=True,
        runtime_receipt_accepted=True,
        # Screenshot, transcript and cleanup evidence remain separate C4 gates.
        full_c4_success=False,
        point_delta=0,
        receipt_frame_sha256=raw_digest,
    )
