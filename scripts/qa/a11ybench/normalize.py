#!/usr/bin/env python3
"""Normalization helpers for accessibility bench evidence."""

from __future__ import annotations

import base64
import binascii
import re
from typing import Any


class NormalizationError(ValueError):
    """Ambiguous, malformed, or unsupported evidence value."""


_HEX_DIGEST_RE = re.compile(r"^[0-9a-fA-F]{64}$")
_HEX_WITH_SEPARATORS_RE = re.compile(
    r"^[0-9a-fA-F]{2}(?:[:\-\s]?[0-9a-fA-F]{2}){31}$"
)
_PREFIXES = ("sha256:", "sha-256:", "sha256=", "sha-256=")


def _reject() -> None:
    raise NormalizationError("provider fingerprint must be a SHA-256 digest")


def _strip_known_prefix(value: str) -> str:
    lowered = value.lower()
    for prefix in _PREFIXES:
        if lowered.startswith(prefix):
            return value[len(prefix) :].strip()
    return value


def _decode_base64_sha256(value: str) -> bytes | None:
    compact = "".join(value.split())
    if not compact:
        return None

    padding = "=" * (-len(compact) % 4)
    for decoder in (base64.b64decode, base64.urlsafe_b64decode):
        try:
            decoded = decoder(compact + padding)
        except (binascii.Error, ValueError):
            continue
        if len(decoded) == 32:
            return decoded
    return None


def normalize_provider_fingerprint(value: Any) -> str:
    """Return a canonical lowercase hex SHA-256 provider fingerprint.

    Signed-profile tooling may surface provider fingerprints as plain hex,
    byte-separated hex, or ``SHA256:`` base64. The canonical form used by QA
    evidence is always 64 lowercase hex characters. Refusal errors do not echo
    the original value because caller input may contain account context.
    """

    if not isinstance(value, str):
        _reject()

    candidate = _strip_known_prefix(value.strip())
    if _HEX_WITH_SEPARATORS_RE.fullmatch(candidate):
        compact = re.sub(r"[:\-\s]", "", candidate)
        if _HEX_DIGEST_RE.fullmatch(compact):
            return compact.lower()

    decoded = _decode_base64_sha256(candidate)
    if decoded is not None:
        return decoded.hex()

    _reject()
