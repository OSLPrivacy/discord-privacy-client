#!/usr/bin/env python3
"""Prove the OSL Hub NSIS uninstall backup choice removes every OSL data root."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONFIG_PATH = ROOT / "apps/osl-hub/tauri.conf.json"
HOOK_PATH = ROOT / "apps/osl-hub/nsis/osl-uninstall-hooks.nsh"
BACKUP_NAME = "OSL identity backup.json"
PROMPT = "Write one OSL identity backup to your Documents folder before uninstall removes local OSL data?"
ROAMING_ROOT = Path("org.oslprivacy.hub")
LOCAL_ROOT = Path("org.oslprivacy.hub")
LEGACY_ROOT = Path("osl")


def configured_hook_path() -> Path:
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    configured = config["bundle"]["windows"]["nsis"]["installerHooks"]
    return CONFIG_PATH.parent / configured


def hook_text() -> str:
    return HOOK_PATH.read_text(encoding="utf-8")


def assert_hook_is_wired_to_cleanup() -> None:
    text = hook_text()
    if configured_hook_path().resolve() != HOOK_PATH.resolve():
        raise AssertionError("tauri.conf.json does not point at the uninstall hook")
    for expected in (
        "!macro NSIS_HOOK_PREUNINSTALL",
        f'"{PROMPT}"',
        f'CopyFiles /SILENT "${{IDENTITY_SOURCE}}" "$DOCUMENTS\\{BACKUP_NAME}"',
        f'Delete "$DOCUMENTS\\{BACKUP_NAME}"',
        'RMDir /r "$APPDATA\\org.oslprivacy.hub"',
        'RMDir /r "$LOCALAPPDATA\\org.oslprivacy.hub"',
        'RMDir /r "$APPDATA\\osl"',
        "IfSilent remove_backup 0",
    ):
        if expected not in text:
            raise AssertionError(f"uninstall hook is missing: {expected}")


def count_entries(path: Path) -> int:
    if not path.exists():
        return 0
    return 1 + sum(1 for _ in path.rglob("*"))


def identity_source(appdata: Path) -> Path | None:
    core = appdata / ROAMING_ROOT / "osl-core"
    flat = core / "identity.json"
    if flat.is_file():
        return flat
    marker = core / "hub-active-identity"
    if marker.is_file():
        slot = marker.read_text(encoding="utf-8")
        slotted = core / "hub-identities" / slot / "identity.json"
        if slotted.is_file():
            return slotted
    return None


def simulate_hook(choice: str, appdata: Path, localappdata: Path, documents: Path) -> None:
    assert_hook_is_wired_to_cleanup()
    backup = documents / BACKUP_NAME
    if choice == "keep":
        source = identity_source(appdata)
        if source is not None:
            documents.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, backup)
    elif choice == "do-not-keep":
        backup.unlink(missing_ok=True)
    else:
        raise AssertionError(f"unknown uninstall choice: {choice}")

    for root in (
        appdata / ROAMING_ROOT,
        localappdata / LOCAL_ROOT,
        appdata / LEGACY_ROOT,
    ):
        shutil.rmtree(root, ignore_errors=True)


def write_fixture(appdata: Path, localappdata: Path) -> None:
    core = appdata / ROAMING_ROOT / "osl-core"
    local = localappdata / LOCAL_ROOT
    legacy = appdata / LEGACY_ROOT
    (core / "store").mkdir(parents=True)
    local.mkdir(parents=True)
    legacy.mkdir(parents=True)
    (core / "identity.json").write_text("sealed-flat-identity", encoding="utf-8")
    (core / "prekeys.json").write_text("prekeys", encoding="utf-8")
    (core / "store" / "messages.sqlite").write_text("messages", encoding="utf-8")
    (local / "service-cache.json").write_text("cache", encoding="utf-8")
    (legacy / "legacy.json").write_text("legacy", encoding="utf-8")


def write_slotted_fixture(appdata: Path, localappdata: Path) -> None:
    core = appdata / ROAMING_ROOT / "osl-core"
    slot = core / "hub-identities" / "slot-active"
    other_slot = core / "hub-identities" / "slot-other"
    local = localappdata / LOCAL_ROOT
    legacy = appdata / LEGACY_ROOT
    slot.mkdir(parents=True)
    other_slot.mkdir(parents=True)
    local.mkdir(parents=True)
    legacy.mkdir(parents=True)
    (core / "hub-active-identity").write_text("slot-active", encoding="utf-8")
    (slot / "identity.json").write_text("sealed-slotted-identity", encoding="utf-8")
    (slot / "prekeys.json").write_text("prekeys", encoding="utf-8")
    (other_slot / "identity.json").write_text("other identity", encoding="utf-8")
    (local / "service-cache.json").write_text("cache", encoding="utf-8")
    (legacy / "legacy.json").write_text("legacy", encoding="utf-8")


class UninstallKeepBackupChoiceTest(unittest.TestCase):
    def test_keep_leaves_one_backup_and_zero_everything_else(self) -> None:
        with tempfile.TemporaryDirectory(prefix="osl-3184-keep-") as temp:
            root = Path(temp)
            appdata = root / "AppData" / "Roaming"
            localappdata = root / "AppData" / "Local"
            documents = root / "Documents"
            write_fixture(appdata, localappdata)

            simulate_hook("keep", appdata, localappdata, documents)

            backup = documents / BACKUP_NAME
            backup_files = 1 if backup.is_file() else 0
            everything_else = (
                count_entries(appdata / ROAMING_ROOT)
                + count_entries(localappdata / LOCAL_ROOT)
                + count_entries(appdata / LEGACY_ROOT)
            )
            self.assertEqual(backup_files, 1)
            self.assertEqual(backup.read_text(encoding="utf-8"), "sealed-flat-identity")
            self.assertEqual(everything_else, 0)
            print(f"TASK3184 keep backup_files={backup_files} everything_else={everything_else}")

    def test_keep_uses_active_slot_when_no_flat_identity_exists(self) -> None:
        with tempfile.TemporaryDirectory(prefix="osl-3184-keep-slot-") as temp:
            root = Path(temp)
            appdata = root / "AppData" / "Roaming"
            localappdata = root / "AppData" / "Local"
            documents = root / "Documents"
            write_slotted_fixture(appdata, localappdata)

            simulate_hook("keep", appdata, localappdata, documents)

            backup = documents / BACKUP_NAME
            backup_files = 1 if backup.is_file() else 0
            everything_else = (
                count_entries(appdata / ROAMING_ROOT)
                + count_entries(localappdata / LOCAL_ROOT)
                + count_entries(appdata / LEGACY_ROOT)
            )
            self.assertEqual(backup_files, 1)
            self.assertEqual(backup.read_text(encoding="utf-8"), "sealed-slotted-identity")
            self.assertEqual(everything_else, 0)
            print(
                f"TASK3184 keep_slotted backup_files={backup_files} everything_else={everything_else}"
            )

    def test_do_not_keep_leaves_zero_including_backup(self) -> None:
        with tempfile.TemporaryDirectory(prefix="osl-3184-no-backup-") as temp:
            root = Path(temp)
            appdata = root / "AppData" / "Roaming"
            localappdata = root / "AppData" / "Local"
            documents = root / "Documents"
            documents.mkdir(parents=True)
            (documents / BACKUP_NAME).write_text("old backup", encoding="utf-8")
            write_fixture(appdata, localappdata)

            simulate_hook("do-not-keep", appdata, localappdata, documents)

            backup_files = 1 if (documents / BACKUP_NAME).is_file() else 0
            everything_including_backup = (
                backup_files
                + count_entries(appdata / ROAMING_ROOT)
                + count_entries(localappdata / LOCAL_ROOT)
                + count_entries(appdata / LEGACY_ROOT)
            )
            self.assertEqual(backup_files, 0)
            self.assertEqual(everything_including_backup, 0)
            print(
                "TASK3184 do_not_keep "
                f"backup_files={backup_files} everything_including_backup={everything_including_backup}"
            )


if __name__ == "__main__":
    unittest.main()
