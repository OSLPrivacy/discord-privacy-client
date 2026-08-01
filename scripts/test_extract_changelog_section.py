from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.extract_changelog_section import extract_release_notes


CHANGELOG = """# Changelog

## [Unreleased]

## [1.2.3] - 2026-07-31

### Fixed

- The updater now preserves release notes.

## [1.2.2]

- Older change.
"""


class ExtractChangelogSectionTests(unittest.TestCase):
    def test_extracts_only_the_tagged_human_written_section(self) -> None:
        notes = extract_release_notes(CHANGELOG, "hub-v1.2.3")
        self.assertEqual(
            notes,
            "### Fixed\n\n- The updater now preserves release notes.\n",
        )

    def test_missing_release_section_fails_before_a_release_can_be_created(self) -> None:
        with self.assertRaisesRegex(ValueError, r"no released section \[9.9.9\]"):
            extract_release_notes(CHANGELOG, "hub-v9.9.9")

    def test_cli_writes_a_github_actions_multiline_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            changelog = root / "CHANGELOG.md"
            output = root / "github-output"
            changelog.write_text(CHANGELOG, encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/extract_changelog_section.py",
                    "--tag",
                    "hub-v1.2.3",
                    "--changelog",
                    str(changelog),
                    "--github-output",
                    str(output),
                ],
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                output.read_text(encoding="utf-8"),
                "release_body<<OSL_CHANGELOG_EOF\n"
                "### Fixed\n\n- The updater now preserves release notes.\n"
                "OSL_CHANGELOG_EOF\n",
            )


if __name__ == "__main__":
    unittest.main()
