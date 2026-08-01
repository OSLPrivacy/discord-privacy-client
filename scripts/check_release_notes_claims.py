#!/usr/bin/env python3
"""Reject release notes that repeat a forbidden public claim."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

try:
    from scripts.extract_changelog_section import ChangelogError, extract_release_notes
except ModuleNotFoundError:  # Direct `python scripts/check_release_notes_claims.py` invocation.
    from extract_changelog_section import ChangelogError, extract_release_notes


SECTION_D = re.compile(r"^## D · NOT ELIGIBLE\b.*$", re.MULTILINE)
NEXT_SECTION = re.compile(r"^##\s+", re.MULTILINE)
QUOTED_PHRASE = re.compile(r'"([^"]+)"')
MIN_BANNED_PHRASES = 8


class ReleaseNotesClaimError(ValueError):
    """The release body cannot safely be published."""


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", text.replace("’", "'").replace("‘", "'")).strip().casefold()


def split_alternatives(phrase: str) -> list[str]:
    parts = [part.strip() for part in phrase.split("/") if part.strip()]
    if len(parts) <= 1:
        return parts

    prefix = parts[0].rsplit(" ", 1)[0] + " " if " " in parts[0] else ""
    return [part if index == 0 or " " in part or not prefix else prefix + part for index, part in enumerate(parts)]


def parse_banned_phrases(allowlist: str) -> list[str]:
    """Read the authoritative section-D phrase list; never maintain a copy here."""
    start = SECTION_D.search(allowlist)
    if start is None:
        raise ReleaseNotesClaimError("allowlist has no section D")
    following = allowlist[start.end():]
    end = NEXT_SECTION.search(following)
    section = following[:end.start()] if end else following
    phrases: dict[str, str] = {}

    for line in section.splitlines():
        cells = line.strip().strip("|").split("|")
        if len(cells) < 2:
            continue
        for quoted in QUOTED_PHRASE.findall(cells[0]):
            for phrase in split_alternatives(quoted):
                normalized = normalize(phrase)
                if normalized:
                    phrases[normalized] = phrase

    if len(phrases) < MIN_BANNED_PHRASES:
        raise ReleaseNotesClaimError(
            f"allowlist section D yielded only {len(phrases)} banned phrases; refusing to approve notes"
        )
    return list(phrases.values())


def find_banned_phrases(notes: str, banned_phrases: list[str]) -> list[tuple[int, str]]:
    """Return each forbidden phrase and its first release-note line number."""
    violations: list[tuple[int, str]] = []
    normalized_notes = normalize(notes)
    for phrase in banned_phrases:
        if normalize(phrase) in normalized_notes:
            line = next(
                (index for index, source_line in enumerate(notes.splitlines(), start=1)
                 if normalize(phrase) in normalize(source_line)),
                1,
            )
            violations.append((line, phrase))
    return violations


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="Release tag, for example hub-v0.1.0")
    parser.add_argument("--changelog", type=Path, default=Path("CHANGELOG.md"))
    parser.add_argument(
        "--allowlist", type=Path, default=Path("docs/design/osl-public-claim-allowlist.md")
    )
    args = parser.parse_args()

    try:
        notes = extract_release_notes(args.changelog.read_text(encoding="utf-8"), args.tag)
        violations = find_banned_phrases(
            notes, parse_banned_phrases(args.allowlist.read_text(encoding="utf-8"))
        )
    except (ChangelogError, OSError, ReleaseNotesClaimError) as error:
        raise SystemExit(f"release-note claim check failed: {error}") from error

    if violations:
        for line, phrase in violations:
            print(f"release-note claim check failed: line {line}: forbidden phrase {phrase!r}", file=sys.stderr)
        raise SystemExit(1)

    print("release-note claim check: release body contains no section-D forbidden phrases")


if __name__ == "__main__":
    main()
