"""The client-side acceptance rules, exercised over real signatures and real bytes.

The synthetic cases build genuine minisign documents with a throwaway Ed25519 key, so the
signature path is tested by signing and verifying rather than by inspecting strings. The live
case runs the same code over the actual published `hub-v0.1.0` artifact and the actual public key
compiled into `apps/osl-hub/tauri.conf.json`.
"""

from __future__ import annotations

import base64
import hashlib
import json
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

from normalize_updater_manifest import normalize_manifest
from verify_update_feed_acceptance import (
    acceptance_check,
    load_pubkey,
    parse_minisign_public_key,
    parse_minisign_signature,
)

ROOT = SCRIPTS.parent
EMITTED_MANIFEST = SCRIPTS / "fixtures" / "hub-v0.1.0-tauri-action-latest.json"
HUB_CONFIG = ROOT / "apps" / "osl-hub" / "tauri.conf.json"
KEY_ID = b"\x01\x02\x03\x04\x05\x06\x07\x08"


def minisign_public_key(private: Ed25519PrivateKey, key_id: bytes = KEY_ID) -> str:
    raw = private.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    body = base64.b64encode(b"Ed" + key_id + raw).decode()
    return base64.b64encode(
        f"untrusted comment: minisign public key\n{body}\n".encode()
    ).decode()


def minisign_signature(
    private: Ed25519PrivateKey,
    payload: bytes,
    *,
    key_id: bytes = KEY_ID,
    prehashed: bool = True,
    trusted_comment: str = "timestamp:1785518563\tfile:candidate.exe",
) -> str:
    algorithm = b"ED" if prehashed else b"Ed"
    message = hashlib.blake2b(payload, digest_size=64).digest() if prehashed else payload
    signature = private.sign(message)
    global_signature = private.sign(signature + trusted_comment.encode())
    document = (
        "untrusted comment: signature from a test key\n"
        + base64.b64encode(algorithm + key_id + signature).decode()
        + f"\ntrusted comment: {trusted_comment}\n"
        + base64.b64encode(global_signature).decode()
        + "\n"
    )
    return base64.b64encode(document.encode()).decode()


class UpdateFeedAcceptanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.private = Ed25519PrivateKey.generate()
        self.pubkey = minisign_public_key(self.private)
        self.payload = b"a candidate installer, byte for byte" * 64

    def manifest(self, signature: str, url: str = "https://example.invalid/a.exe") -> dict:
        return {
            "version": "0.1.0",
            "platforms": {"windows-x86_64": {"signature": signature, "url": url}},
        }

    def test_accepts_a_genuine_signature_over_the_exact_payload(self) -> None:
        manifest = self.manifest(minisign_signature(self.private, self.payload))
        self.assertEqual(
            acceptance_check(manifest, self.pubkey, self.payload),
            "https://example.invalid/a.exe",
        )

    def test_rejects_an_http_platform_url_before_downloading(self) -> None:
        manifest = self.manifest(
            minisign_signature(self.private, self.payload),
            url="http://example.invalid/a.exe",
        )
        with patch("verify_update_feed_acceptance.urllib.request.urlopen") as urlopen:
            with self.assertRaisesRegex(SystemExit, "URL must be https"):
                acceptance_check(manifest, self.pubkey)
        urlopen.assert_not_called()

    def test_accepts_an_https_platform_url_when_all_else_is_equal(self) -> None:
        manifest = self.manifest(minisign_signature(self.private, self.payload))
        response = MagicMock(status=200)
        response.read.return_value = self.payload
        urlopen = MagicMock()
        urlopen.return_value.__enter__.return_value = response
        with patch("verify_update_feed_acceptance.urllib.request.urlopen", urlopen):
            self.assertEqual(
                acceptance_check(manifest, self.pubkey),
                "https://example.invalid/a.exe",
            )
        urlopen.assert_called_once_with("https://example.invalid/a.exe", timeout=120)

    def test_accepts_the_legacy_non_prehashed_algorithm(self) -> None:
        manifest = self.manifest(minisign_signature(self.private, self.payload, prehashed=False))
        acceptance_check(manifest, self.pubkey, self.payload)

    def test_rejects_a_single_flipped_byte_in_the_artifact(self) -> None:
        manifest = self.manifest(minisign_signature(self.private, self.payload))
        tampered = bytearray(self.payload)
        tampered[0] ^= 0x01
        with self.assertRaisesRegex(SystemExit, "does not verify over the downloaded artifact"):
            acceptance_check(manifest, self.pubkey, bytes(tampered))

    def test_rejects_a_signature_from_a_key_the_app_does_not_ship(self) -> None:
        attacker = Ed25519PrivateKey.generate()
        manifest = self.manifest(minisign_signature(attacker, self.payload))
        # Same key id, different key: the id check passes and the signature check must not.
        with self.assertRaisesRegex(SystemExit, "does not verify over the downloaded artifact"):
            acceptance_check(manifest, self.pubkey, self.payload)

    def test_rejects_a_signature_whose_key_id_is_not_the_shipped_one(self) -> None:
        manifest = self.manifest(
            minisign_signature(self.private, self.payload, key_id=b"\x09" * 8)
        )
        with self.assertRaisesRegex(SystemExit, "different key than the one compiled into the app"):
            acceptance_check(manifest, self.pubkey, self.payload)

    def test_rejects_a_tampered_trusted_comment(self) -> None:
        encoded = minisign_signature(self.private, self.payload)
        document = base64.b64decode(encoded).decode().splitlines()
        document[2] = "trusted comment: timestamp:1\tfile:something-else.exe"
        forged = base64.b64encode(("\n".join(document) + "\n").encode()).decode()
        with self.assertRaisesRegex(SystemExit, "trusted comment is not authentic"):
            acceptance_check(self.manifest(forged), self.pubkey, self.payload)

    def test_rejects_a_manifest_with_no_windows_platform(self) -> None:
        manifest = self.manifest(minisign_signature(self.private, self.payload))
        manifest["platforms"] = {"darwin-aarch64": manifest["platforms"]["windows-x86_64"]}
        with self.assertRaisesRegex(SystemExit, "a Windows client finds nothing"):
            acceptance_check(manifest, self.pubkey, self.payload)

    def test_rejects_a_version_the_client_cannot_compare(self) -> None:
        manifest = self.manifest(minisign_signature(self.private, self.payload))
        manifest["version"] = "hub-latest"
        with self.assertRaisesRegex(SystemExit, "not a semantic version"):
            acceptance_check(manifest, self.pubkey, self.payload)

    def test_rejects_an_unreachable_artifact_url_rather_than_skipping(self) -> None:
        manifest = self.manifest(
            minisign_signature(self.private, self.payload),
            url="https://github.com/OSLPrivacy/discord-privacy-client/releases/download/"
            "hub-v0.0.0-does-not-exist/osl-hub.exe",
        )
        with self.assertRaises(SystemExit):
            acceptance_check(manifest, self.pubkey)

    # --- real published artefacts, no network ------------------------------------------

    def test_published_signature_was_made_by_the_key_the_app_ships(self) -> None:
        """Cross-check the live manifest against the key compiled into the binary."""
        manifest = json.loads(EMITTED_MANIFEST.read_text(encoding="utf-8"))
        signature = manifest["platforms"]["windows-x86_64"]["signature"]
        shipped_key_id, _ = parse_minisign_public_key(load_pubkey(HUB_CONFIG))
        _, signature_key_id, _, _, _ = parse_minisign_signature(signature)
        self.assertEqual(signature_key_id, shipped_key_id)
        self.assertEqual(shipped_key_id.hex(), "d4e8589873e46a3b")

    def test_normalization_keeps_the_manifest_client_acceptable(self) -> None:
        """Normalizing must not disturb the signature the client checks."""
        emitted = json.loads(EMITTED_MANIFEST.read_text(encoding="utf-8"))
        normalized = normalize_manifest(emitted, "hub-v0.1.0", "osl-hub-0.1.0-x64-nsis.exe")
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "latest.json"
            path.write_text(json.dumps(normalized), encoding="utf-8")
            self.assertEqual(
                parse_minisign_signature(
                    normalized["platforms"]["windows-x86_64"]["signature"]
                ),
                parse_minisign_signature(
                    emitted["platforms"]["windows-x86_64"]["signature"]
                ),
            )


if __name__ == "__main__":
    unittest.main()
