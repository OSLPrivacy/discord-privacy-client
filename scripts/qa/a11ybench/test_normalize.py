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


def normalize_provider_fingerprints_for_signed_profiles() -> None:
    fingerprint = bytes.fromhex(FINGERPRINT_HEX)
    urlsafe_fingerprint = bytes.fromhex(
        "fbff00112233445566778899aabbccdd"
        "eeff102132435465768798a9babbdcdd"
    )
    urlsafe_hex = urlsafe_fingerprint.hex()
    signed_profile_urlsafe = base64.urlsafe_b64encode(fingerprint).decode("ascii").rstrip("=")
    signed_profile_urlsafe_with_alt_alphabet = (
        base64.urlsafe_b64encode(urlsafe_fingerprint).decode("ascii").rstrip("=")
    )
    signed_profile_wrapped = "\n  ".join(
        base64.b64encode(fingerprint).decode("ascii")[index : index + 16]
        for index in range(0, 44, 16)
    )
    signed_profile_separated = "-".join(
        FINGERPRINT_HEX[index : index + 2]
        for index in range(0, len(FINGERPRINT_HEX), 2)
    )

    testcase = unittest.TestCase()
    testcase.assertEqual(
        NORMALIZE.normalize_provider_fingerprint(f"SHA256={signed_profile_urlsafe}"),
        FINGERPRINT_HEX,
    )
    testcase.assertIn("-", signed_profile_urlsafe_with_alt_alphabet)
    testcase.assertIn("_", signed_profile_urlsafe_with_alt_alphabet)
    testcase.assertEqual(
        NORMALIZE.normalize_provider_fingerprint(
            f"SHA256:{signed_profile_urlsafe_with_alt_alphabet}"
        ),
        urlsafe_hex,
    )
    testcase.assertEqual(
        NORMALIZE.normalize_provider_fingerprint(f"sha256:{signed_profile_wrapped}"),
        FINGERPRINT_HEX,
    )
    testcase.assertEqual(
        NORMALIZE.normalize_provider_fingerprint(f"sha-256={signed_profile_separated.upper()}"),
        FINGERPRINT_HEX,
    )

    with testcase.assertRaises(NORMALIZE.NormalizationError) as raised:
        NORMALIZE.normalize_provider_fingerprint("signed-profile alice@example.test sha256=bad")

    reason = str(raised.exception)
    testcase.assertIn("provider fingerprint", reason)
    testcase.assertNotIn("alice@example.test", reason)
    testcase.assertNotIn("bad", reason)


normalize_provider_fingerprints_for_signed_profiles.__name__ = (
    "Normalize provider fingerprints for signed profiles"
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    del tests, pattern
    suite = loader.loadTestsFromTestCase(ProviderFingerprintNormalizationTests)
    suite.addTest(unittest.FunctionTestCase(
        normalize_provider_fingerprints_for_signed_profiles,
    ))
    return suite


if __name__ == "__main__":
    unittest.main(verbosity=2)
