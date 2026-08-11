#!/usr/bin/env python3
"""Validate the contract-only Messenger row matrix and zero shipping footprint."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parents[1]
CONTRACT = HERE / "messenger-decrypted-row-contract.json"

TOP_LEVEL_KEYS = {
    "schema", "status", "carrier", "source", "geometry", "fill", "type",
    "line", "spacing", "direction", "seam", "required_row_cases",
}

EXPECTED_SECTIONS: dict[str, dict[str, Any]] = {
    "geometry": {
        "content_width_css_px": 120,
        "one_line_height_css_px": 48,
        "two_line_height_css_px": 64,
        "bubble_radius_css_px": 18,
    },
    "fill": {"incoming": "#e4e6eb", "outgoing": "#0084ff"},
    "type": {
        "family": "Segoe UI", "size_css_px": 15, "weight": 400,
        "incoming_colour": "#050505", "outgoing_colour": "#ffffff",
    },
    "line": {"height_css_px": 20, "maximum_lines": 2, "wrapping": "carrier-bound"},
    "spacing": {
        "padding_block_css_px": 8, "padding_inline_css_px": 12,
        "grouped_gap_css_px": 2, "ungrouped_gap_css_px": 8,
    },
    "direction": {
        "incoming_alignment": "logical-start", "outgoing_alignment": "logical-end",
        "rtl_rule": "logical-mirror",
    },
    "seam": {
        "ring_physical_px": 4, "outside_pixels": "untouched", "outside_alpha": 0,
        "maximum_bleed_physical_px": 0,
    },
}

CASE_SHAPES = (
    ("incoming", "ungrouped", "plain", 1, "default"),
    ("outgoing", "ungrouped", "plain", 1, "default"),
    ("incoming", "grouped", "plain", 1, "default"),
    ("outgoing", "grouped", "plain", 1, "default"),
    ("incoming", "ungrouped", "reply", 2, "default"),
    ("outgoing", "ungrouped", "media", 1, "default"),
    ("incoming", "ungrouped", "plain", 1, "row-actions"),
)


def case_id(conversation: str, shape: tuple[str, str, str, int, str]) -> str:
    direction, grouping, kind, lines, hover = shape
    hover_id = "hover" if hover == "row-actions" else hover
    return f"{conversation}.{direction}.{grouping}.{kind}.{'one' if lines == 1 else 'two'}-line.{hover_id}"


EXPECTED_CASES = {
    case_id(conversation, shape): {
        "id": case_id(conversation, shape), "conversation": conversation,
        "direction": shape[0], "grouping": shape[1], "kind": shape[2],
        "line_count": shape[3], "hover": shape[4],
    }
    for conversation in ("direct-message", "group", "community")
    for shape in CASE_SHAPES
}


class Refusal(Exception):
    pass


def load_contract(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise Refusal(f"contract: unreadable JSON: {error}") from error
    if not isinstance(value, dict):
        raise Refusal("contract: top level must be an object")
    return value


def validate_contract(contract: dict[str, Any]) -> tuple[int, int, int]:
    if set(contract) != TOP_LEVEL_KEYS:
        missing = sorted(TOP_LEVEL_KEYS - set(contract))
        extra = sorted(set(contract) - TOP_LEVEL_KEYS)
        raise Refusal(f"contract: top-level fields mismatch missing={missing} extra={extra}")
    identities = {
        "schema": "osl-messenger-decrypted-row-contract-v1",
        "status": "contract-only",
        "carrier": "Messenger",
        "source": "reviewed-live-messenger-row-matrix",
    }
    for field, expected in identities.items():
        actual = contract.get(field)
        if actual != expected:
            raise Refusal(f"contract field {field}: expected {expected!r}, found {actual!r}")

    field_count = len(identities)
    for section, expected_fields in EXPECTED_SECTIONS.items():
        actual_section = contract.get(section)
        if not isinstance(actual_section, dict):
            raise Refusal(f"contract field {section}: expected non-empty object")
        if set(actual_section) != set(expected_fields):
            missing = sorted(set(expected_fields) - set(actual_section))
            extra = sorted(set(actual_section) - set(expected_fields))
            raise Refusal(f"contract field {section}: keys mismatch missing={missing} extra={extra}")
        for field, expected in expected_fields.items():
            actual = actual_section.get(field)
            if actual != expected:
                raise Refusal(
                    f"contract field {section}.{field}: expected {expected!r}, found {actual!r}"
                )
            field_count += 1

    rows = contract.get("required_row_cases")
    if not isinstance(rows, list) or not rows:
        raise Refusal("contract field required_row_cases: expected non-empty array")
    actual: dict[str, Any] = {}
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            raise Refusal(f"required row case index {index}: expected object")
        row_id = row.get("id")
        if not isinstance(row_id, str) or not row_id:
            raise Refusal(f"required row case index {index}: field id is empty")
        if row_id in actual:
            raise Refusal(f"required row case {row_id}: duplicate")
        actual[row_id] = row
    missing = sorted(set(EXPECTED_CASES) - set(actual))
    extra = sorted(set(actual) - set(EXPECTED_CASES))
    if missing or extra:
        named = (missing or extra)[0]
        raise Refusal(f"required row case {named}: inventory mismatch missing={missing} extra={extra}")
    for row_id, expected in EXPECTED_CASES.items():
        row = actual[row_id]
        if set(row) != set(expected):
            missing_fields = sorted(set(expected) - set(row))
            extra_fields = sorted(set(row) - set(expected))
            raise Refusal(
                f"required row case {row_id}: fields mismatch missing={missing_fields} extra={extra_fields}"
            )
        for field, expected_value in expected.items():
            if row.get(field) != expected_value:
                raise Refusal(
                    f"required row case {row_id} field {field}: "
                    f"expected {expected_value!r}, found {row.get(field)!r}"
                )
            field_count += 1
    return field_count, len(rows), len(EXPECTED_SECTIONS["seam"])


def files_under(root: Path, relatives: Iterable[str]) -> Iterable[Path]:
    for relative in relatives:
        path = root / relative
        if path.is_file():
            yield path
        elif path.is_dir():
            for candidate in sorted(path.rglob("*")):
                if candidate.is_file() and not any(part in {"node_modules", "target", "dist"} for part in candidate.parts):
                    yield candidate


def text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError):
        return ""


CONTRACT_MARKERS = re.compile(
    r"messenger-decrypted-row-contract|messenger_decrypted_row_contract|"
    r"MessengerDecryptedRowContract|osl-messenger-decrypted-row-contract-v1|"
    r"messenger_eye_state|MessengerRowStateWriter|MessengerVisibleRow",
    re.IGNORECASE,
)
PAINTER_MARKERS = re.compile(
    r"messenger-decrypted-row-painter|messenger_decrypted_row_painter|"
    r"MessengerDecryptedRowPainter|data-osl-messenger-decrypted-row",
    re.IGNORECASE,
)
ACTION_MARKERS = re.compile(
    r"messenger-decrypted-row(?:-action)?|messenger_decrypted_row(?:_action)?|"
    r"MessengerDecryptedRow(?:Action)?",
    re.IGNORECASE,
)
MANIFEST_MARKERS = re.compile(
    r"messenger-decrypted-row|messenger_decrypted_row|MessengerDecryptedRow|"
    r"osl-messenger-decrypted-row-contract-v1",
    re.IGNORECASE,
)
IMPORT_LINE = re.compile(
    r"^\s*(?:import\b|export\b.*\bfrom\b|(?:pub\s+)?(?:use|mod)\b|"
    r".*include_(?:str|bytes)!\s*\()"
)


def matching_lines(paths: Iterable[Path], marker: re.Pattern[str], *, imports_only: bool = False) -> list[str]:
    matches: list[str] = []
    for path in paths:
        for number, line in enumerate(text(path).splitlines(), 1):
            if marker.search(line) and (not imports_only or IMPORT_LINE.search(line)):
                matches.append(f"{path}:{number}")
    return matches


def shipping_inventories(root: Path) -> dict[str, list[str]]:
    production = tuple(files_under(root, ("apps/osl-hub-ui/src", "apps/osl-hub/src")))
    package_surfaces = tuple(files_under(root, (
        "apps/osl-hub-ui/src", "apps/osl-hub/src", "apps/osl-hub-ui/package.json",
        "apps/osl-hub-ui/vite.config.ts", "apps/osl-hub/tauri.conf.json",
    )))
    action_surfaces = tuple(files_under(root, (
        "apps/osl-hub/capabilities", "apps/osl-hub/permissions",
        "apps/osl-hub/src/main.rs", "apps/osl-hub/src/lib.rs",
        "apps/osl-hub/src/hub_command_surface.rs",
    )))
    manifest_candidates = [
        path for path in files_under(root, ("apps", "scripts", ".github"))
        if any(token in path.name.lower() for token in ("manifest", "release", "tauri.conf"))
    ]
    return {
        "production_imports": matching_lines(production, CONTRACT_MARKERS, imports_only=True),
        "packaged_painters": matching_lines(package_surfaces, PAINTER_MARKERS),
        "installed_decrypted_pixels_actions": matching_lines(action_surfaces, ACTION_MARKERS),
        "release_manifest_rows": matching_lines(manifest_candidates, MANIFEST_MARKERS),
    }


def check(root: Path, contract_path: Path) -> str:
    field_count, row_count, seam_count = validate_contract(load_contract(contract_path))
    inventories = shipping_inventories(root)
    for name, matches in inventories.items():
        if matches:
            raise Refusal(f"shipping inventory {name}: count={len(matches)} promoted at {matches[0]}")
    counts = " ".join(f"{name}=0" for name in inventories)
    return (
        f"TASK5132_OK contract_fields={field_count} required_row_cases={row_count} "
        f"seam_rules={seam_count} {counts}"
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=REPOSITORY)
    parser.add_argument("--contract", type=Path, default=CONTRACT)
    args = parser.parse_args(argv)
    try:
        print(check(args.root.resolve(), args.contract.resolve()))
        return 0
    except Refusal as error:
        print(f"TASK5132_REFUSED {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
