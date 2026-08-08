#!/usr/bin/env python3
"""Validate the machine-readable invariants in the burn/uninstall copy contract."""

import json
import re
import sys
import unittest
from pathlib import Path


CONTRACT_PATH = Path(__file__).resolve().parents[1] / "docs/release/burn-and-uninstall-contract.md"
EXPECTED_UNINSTALL_PLACE_NAMES = (
    "program files",
    "settings",
    "keys",
    "stored messages",
    "downloaded files",
    "startup entry",
    "logs",
)
EXPECTED_TEMPORARY_UNINSTALL_SECTION_NAMES = (
    "temporary files",
    "logs",
    "crash dumps",
)
EXPECTED_WINDOWS_UNINSTALL_SECTION_NAMES = (
    "OSL-owned browser records",
    "Windows registry keys",
    "scheduled jobs",
    "startup entries",
    "background services",
)


def load_contract() -> dict:
    document = CONTRACT_PATH.read_text(encoding="utf-8")
    match = re.search(r"```json burn-uninstall-contract\n(.*?)\n```", document, re.DOTALL)
    if match is None:
        raise ValueError("burn/uninstall contract JSON block is missing")
    return json.loads(match.group(1))


def load_uninstall_footprint_map() -> dict:
    document = CONTRACT_PATH.read_text(encoding="utf-8")
    match = re.search(r"```json uninstall-footprint-map\n(.*?)\n```", document, re.DOTALL)
    if match is None:
        raise ValueError("uninstall footprint map JSON block is missing")
    return json.loads(match.group(1))


def load_temporary_uninstall_inventory() -> dict:
    document = CONTRACT_PATH.read_text(encoding="utf-8")
    match = re.search(r"```json temporary-uninstall-inventory\n(.*?)\n```", document, re.DOTALL)
    if match is None:
        raise ValueError("temporary uninstall inventory JSON block is missing")
    return json.loads(match.group(1))


def load_windows_uninstall_inventory() -> dict:
    document = CONTRACT_PATH.read_text(encoding="utf-8")
    match = re.search(r"```json windows-uninstall-inventory\n(.*?)\n```", document, re.DOTALL)
    if match is None:
        raise ValueError("Windows uninstall inventory JSON block is missing")
    return json.loads(match.group(1))


def uninstall_places(uninstall_map: dict) -> list[dict]:
    places = uninstall_map["places"]
    if not isinstance(places, list):
        raise ValueError("uninstall footprint map places must be a list")
    return places


def temporary_uninstall_sections(inventory: dict) -> list[dict]:
    sections = inventory["sections"]
    if not isinstance(sections, list):
        raise ValueError("temporary uninstall inventory sections must be a list")
    return sections


def windows_uninstall_sections(inventory: dict) -> list[dict]:
    sections = inventory["sections"]
    if not isinstance(sections, list):
        raise ValueError("Windows uninstall inventory sections must be a list")
    return sections


def marked_uninstall_items(section: dict) -> list[dict]:
    items = section["items"]
    if not isinstance(items, list):
        raise ValueError(f"{section['name']} items must be a list")
    return [
        item
        for item in items
        if item.get("marked_for_uninstall") is True
        and item.get("uninstall_action") == "remove_on_uninstall"
    ]


def print_uninstall_footprint_map() -> int:
    places = uninstall_places(load_uninstall_footprint_map())
    names = [place["name"] for place in places]
    print(f"TASK3182_UNINSTALL_PLACE_COUNT={len(names)}")
    for index, place in enumerate(places, start=1):
        print(f"TASK3182_UNINSTALL_PLACE[{index}]={place['name']} COUNT={place['count']}")
    if names != list(EXPECTED_UNINSTALL_PLACE_NAMES):
        return 1
    if any(place.get("count") != 1 for place in places):
        return 1
    return 0


def print_temporary_uninstall_inventory() -> int:
    sections = temporary_uninstall_sections(load_temporary_uninstall_inventory())
    names = [section["name"] for section in sections]
    print(f"TASK3700_TEMPORARY_UNINSTALL_SECTION_COUNT={len(names)}")
    ok = names == list(EXPECTED_TEMPORARY_UNINSTALL_SECTION_NAMES)
    for index, section in enumerate(sections, start=1):
        marked = marked_uninstall_items(section)
        marked_name = marked[0]["name"] if len(marked) == 1 else ""
        print(
            "TASK3700_TEMPORARY_UNINSTALL_SECTION"
            f"[{index}]={section['name']} HANDLED_MARKED_ITEMS={len(marked)}"
            f" MARKED_ITEM={marked_name}"
        )
        if len(marked) != 1:
            ok = False
    return 0 if ok else 1


def windows_uninstall_inventory_is_complete(sections: list[dict]) -> bool:
    names = [section.get("name") for section in sections]
    if names != list(EXPECTED_WINDOWS_UNINSTALL_SECTION_NAMES):
        return False
    for section in sections:
        records = section.get("records")
        if not isinstance(records, list) or not records:
            return False
        if section.get("count") != len(records):
            return False
        for record in records:
            if not isinstance(record, dict):
                return False
            if not all(record.get(field) for field in ("name", "locations", "ownership", "uninstall_action", "source")):
                return False
    return True


def windows_uninstall_inventory_exit_code(sections: list[dict]) -> int:
    return 0 if windows_uninstall_inventory_is_complete(sections) else 1


def print_windows_uninstall_inventory() -> int:
    sections = windows_uninstall_sections(load_windows_uninstall_inventory())
    print(f"TASK3699_WINDOWS_UNINSTALL_SECTION_COUNT={len(sections)}")
    for index, section in enumerate(sections, start=1):
        print(
            "TASK3699_WINDOWS_UNINSTALL_SECTION"
            f"[{index}]={section.get('name', '')} COUNT={section.get('count', '')}"
        )
    return windows_uninstall_inventory_exit_code(sections)


class BurnAndUninstallContractTest(unittest.TestCase):
    def test_burn_and_uninstall_have_non_overlapping_effects(self) -> None:
        contract = load_contract()
        burn = contract["burn"]
        uninstall = contract["uninstall"]
        claims = contract["claims"]

        self.assertTrue(burn["local_immediate"])
        self.assertFalse(burn["uninstalls_app"])
        self.assertEqual(burn["remote_completion"], "confirmed only after the server acknowledges deletion")
        self.assertEqual(burn["peer_completion"], "confirmed only after the peer acknowledges deletion")
        self.assertTrue(uninstall["removes_application"])
        self.assertTrue(uninstall["deletes_osl_data"])
        self.assertTrue(uninstall["offers_one_identity_backup"])
        self.assertEqual(uninstall["backup_filename"], "OSL identity backup.json")
        self.assertTrue(uninstall["offers_one_local_data_backup"])
        self.assertEqual(uninstall["backup_directory"], "OSL local data backup")
        self.assertFalse(uninstall["is_a_burn"])
        self.assertTrue(uninstall["requires_separate_user_action"])
        self.assertEqual(claims["remote_data_unrecoverable_only_after"], "server confirmation")
        self.assertEqual(claims["burn_status_before_server_confirmation"], "pending")
        self.assertEqual(
            claims["uninstall_status"],
            "application removed; local OSL data removed; identity backup kept only if selected",
        )

    def test_uninstall_footprint_names_every_osl_write_place(self) -> None:
        places = uninstall_places(load_uninstall_footprint_map())
        self.assertEqual([place["name"] for place in places], list(EXPECTED_UNINSTALL_PLACE_NAMES))
        self.assertEqual([place["count"] for place in places], [1] * len(EXPECTED_UNINSTALL_PLACE_NAMES))
        for place in places:
            self.assertTrue(place["locations"], f"{place['name']} must list concrete locations")
            self.assertTrue(place["source"], f"{place['name']} must name the source")

    def test_temporary_uninstall_inventory_names_every_removal_section(self) -> None:
        sections = temporary_uninstall_sections(load_temporary_uninstall_inventory())
        self.assertEqual(
            [section["name"] for section in sections],
            list(EXPECTED_TEMPORARY_UNINSTALL_SECTION_NAMES),
        )
        for section in sections:
            marked = marked_uninstall_items(section)
            self.assertEqual(len(marked), 1, f"{section['name']} must mark exactly one uninstall item")
            self.assertTrue(marked[0]["locations"], f"{section['name']} marked item must list locations")
            self.assertTrue(marked[0]["source"], f"{section['name']} marked item must name source")

    def test_windows_uninstall_inventory_names_every_owned_section(self) -> None:
        sections = windows_uninstall_sections(load_windows_uninstall_inventory())
        self.assertEqual(
            [section["name"] for section in sections],
            list(EXPECTED_WINDOWS_UNINSTALL_SECTION_NAMES),
        )
        self.assertTrue(windows_uninstall_inventory_is_complete(sections))

    def test_windows_uninstall_inventory_refuses_every_single_section_omission(self) -> None:
        sections = windows_uninstall_sections(load_windows_uninstall_inventory())
        for omitted_index, omitted_name in enumerate(EXPECTED_WINDOWS_UNINSTALL_SECTION_NAMES):
            with self.subTest(omitted=omitted_name):
                incomplete = sections[:omitted_index] + sections[omitted_index + 1 :]
                self.assertEqual(
                    windows_uninstall_inventory_exit_code(incomplete),
                    1,
                    f"omitting {omitted_name} must make the direct inventory exit 1",
                )

    def test_uninstall_footprint_names_every_osl_write_place(self) -> None:
        places = uninstall_places(load_uninstall_footprint_map())
        self.assertEqual([place["name"] for place in places], list(EXPECTED_UNINSTALL_PLACE_NAMES))
        self.assertEqual([place["count"] for place in places], [1] * len(EXPECTED_UNINSTALL_PLACE_NAMES))
        for place in places:
            self.assertTrue(place["locations"], f"{place['name']} must list concrete locations")
            self.assertTrue(place["source"], f"{place['name']} must name the source")


if __name__ == "__main__":
    if sys.argv[1:] == ["--print-uninstall-map"]:
        raise SystemExit(print_uninstall_footprint_map())
    if sys.argv[1:] == ["--print-temporary-uninstall-inventory"]:
        raise SystemExit(print_temporary_uninstall_inventory())
    if sys.argv[1:] == ["--print-windows-uninstall-inventory"]:
        raise SystemExit(print_windows_uninstall_inventory())
    unittest.main()
