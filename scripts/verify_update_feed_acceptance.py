#!/usr/bin/env python3
"""Prove an updater manifest is one the shipped app would actually install.

Passing `scripts/verify_hub_vm_qa_attestation.py` only proves the manifest satisfies *our*
promotion policy. It says nothing about whether `tauri-plugin-updater`, running inside an
installed OSL build, would accept it. That client applies three tests, and this script applies
the same three against real bytes:

1. the platform URL resolves to a real artifact (the `api.github.com` asset URL `tauri-action`
   emits does not, for an unauthenticated client — which is why normalization exists);
2. the `signature` in the manifest is a valid minisign signature over the *downloaded* bytes;
3. it verifies under the public key compiled into the app at `apps/osl-hub/tauri.conf.json`
   (`plugins.updater.pubkey`) — not a key supplied alongside the manifest.

Test 3 is the load-bearing one. It is the only integrity anchor OSL currently ships, and it is
what makes a published `hub-latest/latest.json` trustworthy rather than merely well-formed.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
HUB_CONFIG = ROOT / "apps" / "osl-hub" / "tauri.conf.json"
CANONICAL_PLATFORM = "windows-x86_64"
SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)*$")
# minisign algorithm tags: "Ed" signs the file directly, "ED" signs its BLAKE2b-512 hash.
LEGACY_ALGORITHM = b"Ed"
PREHASHED_ALGORITHM = b"ED"
DOWNLOAD_LIMIT_BYTES = 512 * 1024 * 1024


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(message)


def _ed25519_verify(public_key: bytes, signature: bytes, message: bytes) -> bool:
    try:
        from cryptography.exceptions import InvalidSignature
        from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
    except ImportError as error:  # pragma: no cover - environment failure, must not be silent
        raise SystemExit(
            "cryptography is required to verify the updater signature: " f"{error}"
        ) from error
    try:
        Ed25519PublicKey.from_public_bytes(public_key).verify(signature, message)
    except InvalidSignature:
        return False
    return True


def parse_minisign_public_key(encoded: str) -> tuple[bytes, bytes]:
    """Return (key_id, ed25519_public_key) from the base64 blob in tauri.conf.json."""
    try:
        text = base64.b64decode(encoded, validate=True).decode("utf-8")
    except (ValueError, UnicodeDecodeError) as error:
        raise SystemExit(f"updater public key is not base64 minisign text: {error}") from error
    lines = [line for line in text.splitlines() if line.strip()]
    require(bool(lines), "updater public key is empty")
    raw = base64.b64decode(lines[-1], validate=True)
    require(len(raw) == 42, "updater public key must be a 42-byte minisign key")
    require(raw[:2] == LEGACY_ALGORITHM, "updater public key is not an Ed25519 minisign key")
    return raw[2:10], raw[10:]


def parse_minisign_signature(encoded: str) -> tuple[bytes, bytes, bytes, bytes, bytes]:
    """Return (algorithm, key_id, signature, trusted_comment, global_signature)."""
    try:
        text = base64.b64decode(encoded, validate=True).decode("utf-8")
    except (ValueError, UnicodeDecodeError) as error:
        raise SystemExit(f"updater signature is not base64 minisign text: {error}") from error
    lines = text.splitlines()
    require(len(lines) >= 4, "updater signature is not a complete minisign document")
    raw = base64.b64decode(lines[1], validate=True)
    require(len(raw) == 74, "updater signature must carry a 64-byte Ed25519 signature")
    marker = "trusted comment: "
    require(lines[2].startswith(marker), "updater signature has no trusted comment")
    trusted_comment = lines[2][len(marker):].encode("utf-8")
    global_signature = base64.b64decode(lines[3], validate=True)
    return raw[:2], raw[2:10], raw[10:], trusted_comment, global_signature


def verify_minisign(encoded_signature: str, encoded_public_key: str, payload: bytes) -> None:
    key_id, public_key = parse_minisign_public_key(encoded_public_key)
    algorithm, sig_key_id, signature, trusted_comment, global_signature = parse_minisign_signature(
        encoded_signature
    )
    require(
        sig_key_id == key_id,
        "updater signature was made by a different key than the one compiled into the app",
    )
    require(
        algorithm in (LEGACY_ALGORITHM, PREHASHED_ALGORITHM),
        "unsupported minisign algorithm in the updater signature",
    )
    message = (
        hashlib.blake2b(payload, digest_size=64).digest()
        if algorithm == PREHASHED_ALGORITHM
        else payload
    )
    require(
        _ed25519_verify(public_key, signature, message),
        "updater signature does not verify over the downloaded artifact",
    )
    require(
        _ed25519_verify(public_key, global_signature, signature + trusted_comment),
        "updater signature's trusted comment is not authentic",
    )


def fetch(url: str) -> bytes:
    require(url.startswith("https://"), "updater artifact URL must be https")
    try:
        with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310 - https enforced
            require(
                response.status == 200,
                f"updater artifact URL returned HTTP {response.status}, not 200",
            )
            payload = response.read(DOWNLOAD_LIMIT_BYTES + 1)
    except urllib.error.HTTPError as error:
        raise SystemExit(f"updater artifact URL returned HTTP {error.code}: {url}") from error
    except OSError as error:
        raise SystemExit(f"updater artifact URL is unreachable: {url} ({error})") from error
    require(len(payload) <= DOWNLOAD_LIMIT_BYTES, "updater artifact exceeds the size limit")
    require(bool(payload), "updater artifact URL served an empty body")
    return payload


def load_pubkey(config_path: Path) -> str:
    try:
        config = json.loads(config_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"app config is unreadable: {error}") from error
    pubkey = config.get("plugins", {}).get("updater", {}).get("pubkey")
    require(isinstance(pubkey, str) and bool(pubkey), "app config carries no updater public key")
    return pubkey


def acceptance_check(manifest: Any, pubkey: str, payload: bytes | None = None) -> str:
    """Apply the client's own acceptance rules. Returns the verified artifact URL."""
    require(isinstance(manifest, dict), "updater manifest must be an object")
    version = manifest.get("version")
    require(
        isinstance(version, str) and bool(SEMVER.fullmatch(version)),
        "updater manifest version is not a semantic version the client can compare",
    )
    platforms = manifest.get("platforms")
    require(isinstance(platforms, dict), "updater manifest platforms are missing")
    require(
        CANONICAL_PLATFORM in platforms,
        f"updater manifest has no {CANONICAL_PLATFORM} entry, so a Windows client finds nothing",
    )
    entry = platforms[CANONICAL_PLATFORM]
    require(isinstance(entry, dict), "platform entry must be an object")
    url = entry.get("url")
    require(isinstance(url, str) and bool(url), "platform entry carries no URL")
    signature = entry.get("signature")
    require(isinstance(signature, str) and bool(signature), "platform entry carries no signature")

    artifact = payload if payload is not None else fetch(url)
    verify_minisign(signature, pubkey, artifact)
    return url


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--config", type=Path, default=HUB_CONFIG)
    parser.add_argument(
        "--installer",
        type=Path,
        default=None,
        help="verify against these local bytes instead of downloading the platform URL",
    )
    args = parser.parse_args()

    try:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise SystemExit(f"updater manifest is unreadable: {error}") from error

    payload = args.installer.read_bytes() if args.installer is not None else None
    url = acceptance_check(manifest, load_pubkey(args.config), payload)
    source = args.installer if args.installer is not None else url
    print(
        "OK: the shipped updater public key verifies this manifest's signature over "
        f"{source} — an installed client would accept it"
    )


if __name__ == "__main__":
    main()
