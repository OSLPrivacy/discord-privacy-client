"""Build-hash manifests bind the signed record to the exact installer bytes."""

from __future__ import annotations

import base64
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

SCRIPTS = Path(__file__).resolve().parent
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from build_hash_manifest import (
    build_manifest,
    sign_manifest,
    verify_manifest_artifacts,
    write_manifest,
)
from verify_update_feed_acceptance import verify_minisign

KEY_ID = b"\x01\x02\x03\x04\x05\x06\x07\x08"


def minisign_public_key(private: Ed25519PrivateKey) -> str:
    raw = private.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    body = base64.b64encode(b"Ed" + KEY_ID + raw).decode()
    return base64.b64encode(f"untrusted comment: minisign public key\n{body}\n".encode()).decode()


def minisign_document(private: Ed25519PrivateKey, payload: bytes) -> str:
    comment = "timestamp:1785518563\tfile:build-hashes.json"
    signature = private.sign(hashlib.blake2b(payload, digest_size=64).digest())
    global_signature = private.sign(signature + comment.encode())
    return "\n".join((
        "untrusted comment: signature from a test key",
        base64.b64encode(b"ED" + KEY_ID + signature).decode(),
        f"trusted comment: {comment}",
        base64.b64encode(global_signature).decode(),
        "",
    ))


class BuildHashManifestTests(unittest.TestCase):
    def test_signed_manifest_binds_installer_and_embedded_executable_and_rejects_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            installer = root / "osl-hub-1.2.3-x64-nsis.exe"
            installer.write_bytes(b"installer bytes")
            manifest_path = root / "build-hashes.json"
            private = Ed25519PrivateKey.generate()

            def fake_run(command: list[str], *, check: bool) -> None:
                self.assertTrue(check)
                if command[0] == "7z":
                    output = next(argument[2:] for argument in command if argument.startswith("-o"))
                    executable = Path(output) / "payload" / "osl-privacy-hub.exe"
                    executable.parent.mkdir(parents=True)
                    executable.write_bytes(b"embedded PE bytes")
                else:
                    self.assertEqual(command, ["tauri", "signer", "sign", str(manifest_path)])
                    manifest_path.with_name("build-hashes.json.sig").write_text(
                        minisign_document(private, manifest_path.read_bytes()), encoding="utf-8"
                    )

            with patch("build_hash_manifest.subprocess.run", fake_run):
                manifest = build_manifest(
                    version="1.2.3",
                    tag="hub-v1.2.3",
                    commit="a" * 40,
                    installer=installer,
                    archive_tool="7z",
                )
                payload = write_manifest(manifest_path, manifest)
                signature = sign_manifest(manifest_path, "tauri")
                verify_manifest_artifacts(manifest, installer, "7z")

            self.assertEqual(json.loads(payload)["builds"][0]["version"], "1.2.3")
            verify_minisign(
                base64.b64encode(signature.read_bytes()).decode(),
                minisign_public_key(private),
                payload,
            )

            installer.write_bytes(b"installer bytes, tampered after manifest generation")
            with patch("build_hash_manifest.subprocess.run", fake_run):
                with self.assertRaisesRegex(SystemExit, "installer SHA-256 does not match manifest"):
                    verify_manifest_artifacts(manifest, installer, "7z")


if __name__ == "__main__":
    unittest.main()
