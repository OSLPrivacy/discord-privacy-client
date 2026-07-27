"""Verifier-side C4 native receipt v3 authority checks.

The pipe server is intentionally out of scope here. Its independently observed
client process facts are mandatory inputs to this verifier; caller-authored
receipt fields never substitute for them.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass
import hmac
from typing import Any, Literal

from ledger import LedgerError, OneShotLedger
from schema import (
    MAX_RECEIPT_BYTES,
    PROCESS_KEYS,
    SHA256_RE,
    TARGET_KEYS,
    canonical_json,
    parse_receipt,
    sha256_hex,
)


EvidenceSource = Literal["synthetic", "runtime_named_pipe"]


class VerificationError(ValueError):
    """Evidence did not meet the v3 native-authority acceptance contract."""


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


@dataclass(frozen=True)
class VerificationVerdict:
    status: str
    parser_crypto_valid: bool
    runtime_receipt_accepted: bool
    full_c4_success: bool
    point_delta: int
    receipt_sha256: str


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
        if type(value) is not int or value < 0:
            raise VerificationError(f"{label} is invalid")
    if not (
        context.issued_at_unix_ms
        < context.expires_at_unix_ms
        <= context.issued_at_unix_ms + 60_000
    ):
        raise VerificationError("challenge lifetime is invalid")
    if context.now_unix_ms > context.expires_at_unix_ms:
        raise VerificationError("challenge is stale")
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
    if context.source == "runtime_named_pipe" and not (
        context.pipe_first_instance
        and context.pipe_remote_clients_rejected
        and context.pipe_acl_exact_user
    ):
        raise VerificationError("runtime pipe protections were not independently proven")


def verify_receipt(
    encoded: bytes,
    context: VerificationContext,
    *,
    ledger: OneShotLedger | None = None,
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
    if not context.issued_at_unix_ms <= emitted <= context.expires_at_unix_ms:
        raise VerificationError("receipt was emitted outside the challenge lifetime")
    if receipt["monotonicStagesMs"]["receiptEmitted"] > (
        context.expires_at_unix_ms - context.issued_at_unix_ms
    ):
        raise VerificationError("native monotonic receipt duration exceeded challenge lifetime")
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
        if ledger is not None:
            raise VerificationError("synthetic evidence may not consume an authority ledger")
        return VerificationVerdict(
            status="synthetic-parser-crypto-valid",
            parser_crypto_valid=True,
            runtime_receipt_accepted=False,
            full_c4_success=False,
            point_delta=0,
            receipt_sha256=raw_digest,
        )

    if ledger is None:
        raise VerificationError("runtime receipt requires the one-shot authority ledger")
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
        receipt_sha256=raw_digest,
    )
