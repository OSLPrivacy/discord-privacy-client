#!/usr/bin/env python3
"""Extract the human-written changelog section for an OSL Hub release tag."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


HEADING = re.compile(r"^## \[(?P<version>[^\]]+)\][^\n]*$", re.MULTILINE)


class ChangelogError(ValueError):
    """The requested release cannot safely be described from the changelog."""


def version_from_tag(tag: str) -> str:
    prefix = "hub-v"
    if not tag.startswith(prefix) or not tag[len(prefix):]:
        raise ChangelogError(f"expected a hub-v<version> tag, got {tag!r}")
    return tag[len(prefix):]


def extract_release_notes(changelog: str, tag: str) -> str:
    """Return the non-empty body under the changelog heading for *tag*."""
    version = version_from_tag(tag)
    headings = list(HEADING.finditer(changelog))

    for index, heading in enumerate(headings):
        if heading.group("version") != version:
            continue
        end = headings[index + 1].start() if index + 1 < len(headings) else len(changelog)
        notes = changelog[heading.end():end].strip()
        if not notes:
            raise ChangelogError(f"CHANGELOG.md section [{version}] is empty")
        return notes + "\n"

    raise ChangelogError(f"CHANGELOG.md has no released section [{version}] for tag {tag}")


def write_github_output(notes: str, output: Path) -> None:
    delimiter = "OSL_CHANGELOG_EOF"
    if delimiter in notes:
        raise ChangelogError("release notes contain the GitHub output delimiter")
    with output.open("a", encoding="utf-8") as handle:
        handle.write(f"release_body<<{delimiter}\n{notes}{delimiter}\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tag", required=True, help="Release tag, for example hub-v0.1.0")
    parser.add_argument("--changelog", type=Path, default=Path("CHANGELOG.md"))
    parser.add_argument(
        "--github-output",
        type=Path,
        help="Write the release_body GitHub Actions output to this file",
    )
    args = parser.parse_args()

    try:
        notes = extract_release_notes(args.changelog.read_text(encoding="utf-8"), args.tag)
        if args.github_output:
            write_github_output(notes, args.github_output)
        else:
            sys.stdout.write(notes)
    except (ChangelogError, OSError) as error:
        raise SystemExit(f"changelog extraction failed: {error}") from error


if __name__ == "__main__":
    main()
