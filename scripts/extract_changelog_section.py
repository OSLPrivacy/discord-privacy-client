#!/usr/bin/env python3
"""Extract the human-written changelog section for an OSL Hub release tag."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


HEADING = re.compile(r"^## \[(?P<version>[^\]]+)\][^\n]*$", re.MULTILINE)
EXCLUSIONS_BLOCK = re.compile(
    r"```json release-exclusions\s*\n(?P<json>.*?)\n```", re.DOTALL
)
REQUIRED_EXCLUSION_IDS = {"code-signing", "osl-notes", "osl-mail"}
DEFAULT_SCOPE_NOTE = (
    Path(__file__).resolve().parents[1] / "docs/release/release-scope-exclusions.md"
)


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


def release_exclusions_from_note(note: str) -> list[dict[str, str]]:
    """Parse and validate the canonical owner-ruling block."""
    match = EXCLUSIONS_BLOCK.search(note)
    if not match:
        raise ChangelogError("release scope note has no release-exclusions block")
    try:
        document = json.loads(match.group("json"))
    except json.JSONDecodeError as error:
        raise ChangelogError(f"release scope note JSON is invalid: {error}") from error
    if (
        not isinstance(document, dict)
        or document.get("owner") != "Liam"
        or not isinstance(document.get("items"), list)
    ):
        raise ChangelogError("release scope note must name Liam and contain items")
    items = document["items"]
    ids = [item.get("id") for item in items if isinstance(item, dict)]
    if len(items) != 3 or set(ids) != REQUIRED_EXCLUSION_IDS or len(ids) != len(set(ids)):
        raise ChangelogError(
            "release scope note must name code-signing, osl-notes, and osl-mail exactly once"
        )
    for item in items:
        required = ("name", "ruling_date", "owner_words", "release_note")
        if not all(isinstance(item.get(key), str) and item[key].strip() for key in required):
            raise ChangelogError(f"release scope exclusion {item.get('id')!r} is incomplete")
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", item["ruling_date"]):
            raise ChangelogError(f"release scope exclusion {item['id']!r} has an invalid date")
        if item["id"] in {"osl-notes", "osl-mail"} and not (
            isinstance(item.get("tile_label"), str) and item["tile_label"].strip()
        ):
            raise ChangelogError(f"release scope exclusion {item['id']!r} has no honest tile label")
    return items


def render_release_exclusions(note: str) -> str:
    """Render the canonical exclusions as user-visible release-note Markdown."""
    lines = ["### Deliberately not in this release", ""]
    for item in release_exclusions_from_note(note):
        lines.append(
            f"- **{item['name']}** ({item['ruling_date']}) — {item['release_note']} "
            f"Liam's recorded words: “{item['owner_words']}”"
        )
    return "\n".join(lines) + "\n"


def release_body(changelog: str, tag: str, scope_note: str) -> str:
    return extract_release_notes(changelog, tag).rstrip() + "\n\n" + render_release_exclusions(scope_note)


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
        "--scope-note",
        type=Path,
        default=DEFAULT_SCOPE_NOTE,
        help="canonical dated owner rulings appended to the release body",
    )
    parser.add_argument(
        "--github-output",
        type=Path,
        help="Write the release_body GitHub Actions output to this file",
    )
    args = parser.parse_args()

    try:
        notes = release_body(
            args.changelog.read_text(encoding="utf-8"),
            args.tag,
            args.scope_note.read_text(encoding="utf-8"),
        )
        if args.github_output:
            write_github_output(notes, args.github_output)
        else:
            sys.stdout.write(notes)
    except (ChangelogError, OSError) as error:
        raise SystemExit(f"changelog extraction failed: {error}") from error


if __name__ == "__main__":
    main()
