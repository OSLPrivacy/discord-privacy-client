from __future__ import annotations

import unittest
import subprocess
import sys
import tempfile
from pathlib import Path

from scripts.check_release_notes_claims import find_banned_phrases, parse_banned_phrases


ALLOWLIST = '''# Claims

## D · NOT ELIGIBLE — these phrases may not appear anywhere

| Forbidden phrase | Why |
|---|---|
| **"Cryptographic burn"** | False claim. |
| **"Burn deletes provider messages" / "Burn removes provider messages"** | False claim. |
| **"Permanent ciphertext" / "permanent gibberish" / "mathematically opaque"** | False claim. |
| **"Disappears forever" / "gone for good"** | False claim. |

## E · Status
'''


class ReleaseNotesClaimTests(unittest.TestCase):
    def test_rejects_a_banned_phrase_in_release_notes(self) -> None:
        phrases = parse_banned_phrases(ALLOWLIST)

        violations = find_banned_phrases(
            "### Changed\n\nBurn deletes provider messages after delivery.\n",
            phrases,
        )

        self.assertEqual(violations, [(3, "Burn deletes provider messages")])

    def test_allows_notes_without_a_public_claim(self) -> None:
        phrases = parse_banned_phrases(ALLOWLIST)

        violations = find_banned_phrases(
            "### Fixed\n\n- The updater now preserves its changelog text.\n",
            phrases,
        )

        self.assertEqual(violations, [])

    def test_cli_refuses_a_banned_phrase_before_release_publish(self) -> None:
        changelog = """# Changelog

## [1.2.3]

Cryptographic burn is now available.
"""
        with tempfile.TemporaryDirectory() as temporary:
            changelog_path = Path(temporary) / "CHANGELOG.md"
            changelog_path.write_text(changelog, encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/check_release_notes_claims.py",
                    "--tag",
                    "hub-v1.2.3",
                    "--changelog",
                    str(changelog_path),
                ],
                capture_output=True,
                text=True,
                check=False,
            )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("forbidden phrase 'Cryptographic burn'", result.stderr)


if __name__ == "__main__":
    unittest.main()
