#!/usr/bin/env python3
"""Mutation tests for accessibility bench normalization helpers."""

from __future__ import annotations

import base64
import importlib.util
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("a11ybench_normalize", HERE / "normalize.py")
assert SPEC and SPEC.loader
NORMALIZE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NORMALIZE)


FINGERPRINT_HEX = (
    "00112233445566778899aabbccddeeff"
    "102132435465768798a9babbdcddedef"
)


class ProviderFingerprintNormalizationTests(unittest.TestCase):
    def test_normalize_provider_fingerprint(self) -> None:
        separated = ":".join(
            FINGERPRINT_HEX[index : index + 2]
            for index in range(0, len(FINGERPRINT_HEX), 2)
        )
        encoded = base64.b64encode(bytes.fromhex(FINGERPRINT_HEX)).decode("ascii")

        self.assertEqual(
            NORMALIZE.normalize_provider_fingerprint(FINGERPRINT_HEX.upper()),
            FINGERPRINT_HEX,
        )
        self.assertEqual(
            NORMALIZE.normalize_provider_fingerprint(f" SHA256:{separated.upper()} "),
            FINGERPRINT_HEX,
        )
        self.assertEqual(
            NORMALIZE.normalize_provider_fingerprint(f"sha-256:{encoded.rstrip('=')}"),
            FINGERPRINT_HEX,
        )

    def test_malformed_provider_fingerprint_is_refused_without_echoing_input(self) -> None:
        secret_bearing_value = "alice@example.test SHA256:not-a-digest"
        with self.assertRaises(NORMALIZE.NormalizationError) as raised:
            NORMALIZE.normalize_provider_fingerprint(secret_bearing_value)

        reason = str(raised.exception)
        self.assertIn("provider fingerprint", reason)
        self.assertNotIn("alice@example.test", reason)
        self.assertNotIn("not-a-digest", reason)

    def test_provider_fingerprint_requires_exact_sha256_length(self) -> None:
        for value in ("a" * 63, "a" * 65, base64.b64encode(b"short").decode("ascii")):
            with self.subTest(value=value):
                with self.assertRaises(NORMALIZE.NormalizationError):
                    NORMALIZE.normalize_provider_fingerprint(value)


if __name__ == "__main__":
    unittest.main(verbosity=2)
