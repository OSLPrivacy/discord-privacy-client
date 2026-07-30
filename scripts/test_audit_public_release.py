from __future__ import annotations

import unittest

from scripts.audit_public_release import (
    release_claim_violations,
    release_identity_mismatch_violations,
)


class PublicReleaseAuditTests(unittest.TestCase):
    def release_identity(self) -> dict[str, object]:
        return {
            "schemaVersion": 1,
            "releaseTag": "v1.2.3",
            "sourceCommit": "c" * 40,
            "sourceTree": "d" * 40,
            "binarySha256": "a" * 64,
            "binarySizeBytes": 123,
            "claimProfile": "release-proven",
        }

    def test_reconcile_public_docs_and_site_claims_against_the_exact_released_binary(self) -> None:
        identity = self.release_identity()

        self.assertEqual(
            release_claim_violations(
                f"Release v1.2.3 binary SHA-256 {'a' * 64} is release-proven.",
                identity,
            ),
            [],
        )

        self.assertEqual(
            release_identity_mismatch_violations(
                f"Release v9.9.9 binary SHA-256 {'a' * 64} is release-proven.",
                identity,
            ),
            [(1, "release claim references a different released binary identity")],
        )
        self.assertEqual(
            release_identity_mismatch_violations(
                f"Release v1.2.3 binary SHA-256 {'b' * 64} is release-proven.",
                identity,
            ),
            [(1, "release claim references a different released binary identity")],
        )


if __name__ == "__main__":
    unittest.main()
