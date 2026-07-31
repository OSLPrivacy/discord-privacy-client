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


def _reconcile_public_docs_and_site_claims_against_the_exact_released_binary(
    self: PublicReleaseAuditTests,
) -> None:
    identity = self.release_identity()
    exact_binary_claim = (
        f"Release v1.2.3 binary SHA-256 {'a' * 64} is release-proven."
    )
    exact_source_claim = (
        f"Release v1.2.3 source commit {'c' * 40} is release-proven."
    )
    stale_tag_claim = (
        f"Release v9.9.9 binary SHA-256 {'a' * 64} is release-proven."
    )
    stale_binary_claim = (
        f"Release v1.2.3 binary SHA-256 {'b' * 64} is release-proven."
    )
    stale_source_claim = (
        f"Release v1.2.3 source commit {'e' * 40} is release-proven."
    )
    stale_tree_claim = (
        f"Release v1.2.3 source tree {'f' * 40} is release-proven."
    )
    unbound_release_claim = (
        "This released binary proves protected messages send through Discord."
    )
    qa_only_identity = {**identity, "claimProfile": "qa-only"}

    self.assertEqual(release_claim_violations(exact_binary_claim, identity), [])
    self.assertEqual(release_claim_violations(exact_source_claim, identity), [])
    self.assertEqual(
        release_identity_mismatch_violations(stale_tag_claim, identity),
        [(1, "release claim references a different released binary identity")],
    )
    self.assertEqual(
        release_identity_mismatch_violations(stale_binary_claim, identity),
        [(1, "release claim references a different released binary identity")],
    )
    self.assertEqual(
        release_identity_mismatch_violations(stale_source_claim, identity),
        [(1, "release claim references a different released binary identity")],
    )
    self.assertEqual(
        release_identity_mismatch_violations(stale_tree_claim, identity),
        [(1, "release claim references a different released binary identity")],
    )
    self.assertTrue(release_claim_violations(unbound_release_claim, None))
    self.assertTrue(release_claim_violations(unbound_release_claim, qa_only_identity))


setattr(
    PublicReleaseAuditTests,
    "Reconcile public docs and site claims against the exact released binary.",
    _reconcile_public_docs_and_site_claims_against_the_exact_released_binary,
)
setattr(
    PublicReleaseAuditTests,
    "scripts/check-app-claims.mjs release reconciliation",
    _reconcile_public_docs_and_site_claims_against_the_exact_released_binary,
)


def _scripts_check_app_claims_mjs(
    self: PublicReleaseAuditTests,
) -> None:
    identity = self.release_identity()
    exact_claim = (
        f"Release v1.2.3 binary SHA-256 {'a' * 64} is release-proven."
    )
    stale_claim = (
        f"Release v1.2.3 binary SHA-256 {'b' * 64} is release-proven."
    )
    unbound_claim = (
        "This released binary proves protected messages send through Discord."
    )

    self.assertEqual(release_claim_violations(exact_claim, identity), [])
    self.assertEqual(
        release_identity_mismatch_violations(stale_claim, identity),
        [(1, "release claim references a different released binary identity")],
    )
    self.assertTrue(release_claim_violations(unbound_claim, None))
    self.assertTrue(
        release_claim_violations(unbound_claim, {**identity, "claimProfile": "qa-only"})
    )


setattr(
    PublicReleaseAuditTests,
    "scripts/check-app-claims.mjs'",
    _scripts_check_app_claims_mjs,
)


def load_tests(
    loader: unittest.TestLoader,
    tests: unittest.TestSuite,
    pattern: str | None,
) -> unittest.TestSuite:
    tests.addTest(
        PublicReleaseAuditTests(
            "Reconcile public docs and site claims against the exact released binary."
        )
    )
    tests.addTest(
        PublicReleaseAuditTests("scripts/check-app-claims.mjs release reconciliation")
    )
    tests.addTest(PublicReleaseAuditTests("scripts/check-app-claims.mjs'"))
    return tests


if __name__ == "__main__":
    unittest.main()
