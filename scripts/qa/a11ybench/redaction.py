#!/usr/bin/env python3
"""Redaction guard for accessibility bench evidence."""

from __future__ import annotations

import re
from collections.abc import Mapping, Sequence
from typing import Any


class RedactionError(ValueError):
    """Content-bearing, ambiguous, or untrusted bench evidence."""


REQUIRED_TOP_LEVEL_BOUNDARIES = frozenset(
    ("artifacts", "authority", "binding", "consent", "privacy")
)
REQUIRED_PRIVACY_TRUE = frozenset(
    ("boundedArtifacts", "noAccountIdentifiers", "noRawText", "noSecrets")
)
REQUIRED_AUTHORITY_FIELDS = frozenset(("collector", "verifier"))
REQUIRED_BINDING_FIELDS = frozenset(("method", "subjectBindingSha256"))
ALLOWED_TEXT_KEYS = frozenset(("noRawText",))
CONTENT_BEARING_ARTIFACT_KINDS = frozenset(
    (
        "cleartext",
        "content",
        "dom",
        "html",
        "image",
        "jpeg",
        "jpg",
        "log",
        "message",
        "messages",
        "png",
        "payload",
        "plaintext",
        "raw",
        "rawlog",
        "screenshot",
        "snapshot",
        "text",
        "transcript",
        "uia",
        "webp",
    )
)
CONTENT_BEARING_ARTIFACT_SUFFIXES = (
    ".bmp",
    ".gif",
    ".htm",
    ".html",
    ".jpeg",
    ".jpg",
    ".log",
    ".png",
    ".txt",
    ".webp",
)
FORBIDDEN_NORMALIZED_KEYS = frozenset(
    (
        "accountemail",
        "accounthandle",
        "accountid",
        "accountidentifier",
        "accountidentifiers",
        "accountname",
        "accesstoken",
        "apikey",
        "apikeys",
        "automationselector",
        "b64",
        "base64",
        "browserprofile",
        "browserprofiles",
        "cleartext",
        "composertext",
        "cookie",
        "cookies",
        "credential",
        "credentials",
        "handle",
        "hwnd",
        "log",
        "logs",
        "messagebody",
        "messagetext",
        "passphrase",
        "password",
        "payload",
        "plaintext",
        "profilehandle",
        "profilepath",
        "rawlog",
        "rawlogs",
        "rawbytes",
        "rawtext",
        "refreshtoken",
        "secret",
        "secrets",
        "selector",
        "selectors",
        "text",
        "token",
        "tokens",
        "transcript",
        "transcripttext",
        "uiselector",
        "userid",
        "username",
        "utf8b64",
        "utf16b64",
        "windowhandle",
    )
)


def _reject(reason: str) -> None:
    raise RedactionError(reason)


def _normalize_key(key: str) -> str:
    return "".join(char.lower() for char in key if char.isalnum())


def _content_bearing_key(key: str) -> bool:
    if key in ALLOWED_TEXT_KEYS:
        return False
    normalized = _normalize_key(key)
    if normalized in FORBIDDEN_NORMALIZED_KEYS:
        return True
    return normalized.endswith("text") or "content" in normalized


def _content_bearing_artifact_text(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    tokens = {token.lower() for token in re.findall(r"[A-Za-z0-9]+", value)}
    if not tokens:
        return False
    if tokens & CONTENT_BEARING_ARTIFACT_KINDS:
        return True
    return value.lower().endswith(CONTENT_BEARING_ARTIFACT_SUFFIXES)


def _require_mapping(value: Any, label: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        _reject(f"{label} must be an object")
    return value


def _require_true(value: Any, label: str) -> None:
    if value is not True:
        _reject(f"{label} must be true")


def _require_false(value: Any, label: str) -> None:
    if value is not False:
        _reject(f"{label} must be false")


def _require_top_level_boundaries(evidence: Mapping[str, Any]) -> None:
    missing = REQUIRED_TOP_LEVEL_BOUNDARIES - set(evidence)
    if missing:
        _reject("required refusal boundary is absent")

    consent = _require_mapping(evidence["consent"], "consent")
    if consent.get("granted") is not True:
        _reject("consent.granted must be true")

    binding = _require_mapping(evidence["binding"], "binding")
    if not REQUIRED_BINDING_FIELDS.issubset(binding):
        _reject("binding evidence is incomplete")

    authority = _require_mapping(evidence["authority"], "authority")
    if not REQUIRED_AUTHORITY_FIELDS.issubset(authority):
        _reject("authority evidence is incomplete")

    privacy = _require_mapping(evidence["privacy"], "privacy")
    missing_privacy = REQUIRED_PRIVACY_TRUE - set(privacy)
    if missing_privacy:
        _reject("privacy evidence is incomplete")
    for key in REQUIRED_PRIVACY_TRUE:
        _require_true(privacy[key], f"privacy.{key}")


def _reject_content_fields(value: Any) -> None:
    if isinstance(value, Mapping):
        for key, child in value.items():
            if not isinstance(key, str):
                _reject("evidence object contains a non-string field")
            if key == "containsUserContent":
                _require_false(child, "containsUserContent")
            elif _content_bearing_key(key):
                _reject("content-bearing evidence field is not allowed")
            _reject_content_fields(child)
        return

    if isinstance(value, str):
        return
    if isinstance(value, Sequence):
        for child in value:
            _reject_content_fields(child)


def _reject_content_artifacts(evidence: Mapping[str, Any]) -> None:
    artifacts = evidence["artifacts"]
    if not isinstance(artifacts, list):
        _reject("artifacts must be an array")
    for artifact in artifacts:
        artifact = _require_mapping(artifact, "artifact")
        if "containsUserContent" not in artifact:
            _reject("artifact content declaration is absent")
        _require_false(artifact["containsUserContent"], "artifact.containsUserContent")
        if _content_bearing_artifact_text(artifact.get("kind")):
            _reject("content-bearing artifact is not allowed")
        if _content_bearing_artifact_text(artifact.get("relativePath")):
            _reject("content-bearing artifact is not allowed")


def reject_content_fields(evidence: Mapping[str, Any]) -> None:
    """Reject evidence that carries raw content or lacks redaction boundaries.

    The function is intentionally fail-closed. It returns ``None`` only when the
    record has explicit consent, binding, authority, privacy, and artifact
    non-content declarations and no nested raw-content-like fields. Raised
    errors identify the refused condition without echoing evidence values.
    """

    evidence = _require_mapping(evidence, "evidence")
    _require_top_level_boundaries(evidence)
    _reject_content_artifacts(evidence)
    _reject_content_fields(evidence)
