from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.extract_changelog_section import (
    extract_release_notes,
    release_body,
    release_exclusions_from_note,
)


CHANGELOG = """# Changelog

## [Unreleased]

## [1.2.3] - 2026-07-31

### Fixed

- The updater now preserves release notes.

## [1.2.2]

- Older change.
"""

SCOPE_NOTE = """# Owner-ruled release exclusions

```json release-exclusions
{"owner":"Liam","items":[
{"id":"code-signing","name":"Code signing","ruling_date":"2026-07-31","owner_words":"ship unsigned","release_note":"Unsigned."},
{"id":"osl-notes","name":"OSL Notes","ruling_date":"2026-08-06","owner_words":"Notes were already out.","release_note":"Notes held.","tile_label":"Not started"},
{"id":"osl-mail","name":"OSL Mail","ruling_date":"2026-08-06","owner_words":"Mail is on hold.","release_note":"Mail held.","tile_label":"Not started"}
]}
```
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

    def test_appends_all_three_dated_owner_rulings_to_the_release_body(self) -> None:
        notes = release_body(CHANGELOG, "hub-v1.2.3", SCOPE_NOTE)
        self.assertIn("### Deliberately not in this release", notes)
        self.assertIn("**Code signing** (2026-07-31)", notes)
        self.assertIn("**OSL Notes** (2026-08-06)", notes)
        self.assertIn("**OSL Mail** (2026-08-06)", notes)
        self.assertEqual(len(release_exclusions_from_note(SCOPE_NOTE)), 3)

    def test_cli_writes_a_github_actions_multiline_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            changelog = root / "CHANGELOG.md"
            scope_note = root / "release-scope-exclusions.md"
            output = root / "github-output"
            changelog.write_text(CHANGELOG, encoding="utf-8")
            scope_note.write_text(SCOPE_NOTE, encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    "scripts/extract_changelog_section.py",
                    "--tag",
                    "hub-v1.2.3",
                    "--changelog",
                    str(changelog),
                    "--scope-note",
                    str(scope_note),
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
                "\n### Deliberately not in this release\n\n"
                "- **Code signing** (2026-07-31) — Unsigned. Liam's recorded words: “ship unsigned”\n"
                "- **OSL Notes** (2026-08-06) — Notes held. Liam's recorded words: “Notes were already out.”\n"
                "- **OSL Mail** (2026-08-06) — Mail held. Liam's recorded words: “Mail is on hold.”\n"
                "OSL_CHANGELOG_EOF\n",
            )


if __name__ == "__main__":
    unittest.main()
