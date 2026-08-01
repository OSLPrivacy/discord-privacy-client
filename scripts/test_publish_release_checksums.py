"""Checksum publication keeps every release asset and detects signed-list tampering."""

from __future__ import annotations

import base64
import hashlib
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parent
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

from publish_release_checksums import sign_checksums, write_checksums
from verify_update_feed_acceptance import verify_minisign

KEY_ID = b"\x01\x02\x03\x04\x05\x06\x07\x08"


def minisign_public_key(private: Ed25519PrivateKey) -> str:
    raw = private.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    key = base64.b64encode(b"Ed" + KEY_ID + raw).decode()
    return base64.b64encode(
        f"untrusted comment: minisign public key\n{key}\n".encode()
    ).decode()


def minisign_signature(private: Ed25519PrivateKey, payload: bytes) -> str:
    comment = "timestamp:1785518563\tfile:SHA256SUMS.txt"
    signature = private.sign(hashlib.blake2b(payload, digest_size=64).digest())
    global_signature = private.sign(signature + comment.encode())
    document = (
        "untrusted comment: signature from a test key\n"
        + base64.b64encode(b"ED" + KEY_ID + signature).decode()
        + f"\ntrusted comment: {comment}\n"
        + base64.b64encode(global_signature).decode()
        + "\n"
    )
    return base64.b64encode(document.encode()).decode()


class PublishReleaseChecksumsTests(unittest.TestCase):
    def test_signed_checksum_list_covers_every_asset_and_rejects_a_flipped_hex_digit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            installer = directory / "osl-hub-0.1.0-x64-nsis.exe"
            updater_signature = directory / "osl-hub-0.1.0-x64-nsis.exe.sig"
            installer.write_bytes(b"installer bytes")
            updater_signature.write_bytes(b"updater signature")
            checksums = directory / "SHA256SUMS.txt"

            write_checksums(checksums, [updater_signature, installer])

            expected = "".join(
                f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
                for path in (installer, updater_signature)
            ).encode()
            payload = checksums.read_bytes()
            self.assertEqual(payload, expected)

            private = Ed25519PrivateKey.generate()
            signature = minisign_signature(private, payload)
            verify_minisign(signature, minisign_public_key(private), payload)

            tampered = payload.replace(payload[:1], b"0" if payload[:1] != b"0" else b"1", 1)
            with self.assertRaisesRegex(SystemExit, "does not verify"):
                verify_minisign(signature, minisign_public_key(private), tampered)

    def test_signing_renames_tauris_minisign_document_to_the_published_extension(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            checksums = Path(temporary) / "SHA256SUMS.txt"
            checksums.write_text("a" * 64 + "  installer.exe\n", encoding="utf-8")
            private = Ed25519PrivateKey.generate()

            def fake_tauri_sign(command: list[str], *, check: bool) -> None:
                self.assertEqual(command, ["tauri", "signer", "sign", str(checksums)])
                self.assertTrue(check)
                checksums.with_name(f"{checksums.name}.sig").write_text(
                    base64.b64decode(minisign_signature(private, checksums.read_bytes())).decode(),
                    encoding="utf-8",
                )

            with patch("publish_release_checksums.subprocess.run", fake_tauri_sign):
                published_signature = sign_checksums(checksums, "tauri")

            self.assertEqual(published_signature.name, "SHA256SUMS.txt.minisig")
            self.assertFalse(checksums.with_name("SHA256SUMS.txt.sig").exists())
            verify_minisign(
                base64.b64encode(published_signature.read_bytes()).decode(),
                minisign_public_key(private),
                checksums.read_bytes(),
            )


if __name__ == "__main__":
    unittest.main()
