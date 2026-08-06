#!/usr/bin/env python3
"""Task 3615: reinstall with and without the uninstaller backup choice."""

import json
import shutil
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CONFIG_PATH = ROOT / "apps/osl-hub/tauri.conf.json"
HOOK_PATH = ROOT / "apps/osl-hub/nsis/osl-uninstall-hooks.nsh"
BACKUP_NAME = "OSL local data backup"
OLD_BACKUP_NAME = "OSL identity backup.json"
PROMPT = "Write one OSL local data backup to your Documents folder before uninstall removes local OSL data?"
ROAMING_ROOT = Path("org.oslprivacy.hub")
LOCAL_ROOT = Path("org.oslprivacy.hub")
LEGACY_ROOT = Path("osl")
MARK = "TASK3615-MARKED"


def configured_hook_path() -> Path:
    config = json.loads(CONFIG_PATH.read_text(encoding="utf-8"))
    configured = config["bundle"]["windows"]["nsis"]["installerHooks"]
    return CONFIG_PATH.parent / configured


def assert_hook_is_wired_to_cleanup() -> None:
    text = HOOK_PATH.read_text(encoding="utf-8")
    if configured_hook_path().resolve() != HOOK_PATH.resolve():
        raise AssertionError("tauri.conf.json does not point at the uninstall hook")
    for expected in (
        "!macro NSIS_HOOK_PREUNINSTALL",
        f'"{PROMPT}"',
        f'!define OSL_BACKUP_DIR "$DOCUMENTS\\{BACKUP_NAME}"',
        'FileOpen $0 "$APPDATA\\org.oslprivacy.hub\\osl-core\\hub-active-identity" r',
        'CopyFiles /SILENT "$APPDATA\\org.oslprivacy.hub\\osl-core\\hub-identities\\$1\\identity.json"',
        'CopyFiles /SILENT "$APPDATA\\org.oslprivacy.hub\\osl-core\\messages\\*.*"',
        'RMDir /r "${OSL_BACKUP_DIR}"',
        'Delete "${OSL_OLD_IDENTITY_BACKUP}"',
        'RMDir /r "$APPDATA\\org.oslprivacy.hub"',
        'RMDir /r "$LOCALAPPDATA\\org.oslprivacy.hub"',
        'RMDir /r "$APPDATA\\osl"',
        "IfSilent remove_backup 0",
    ):
        if expected not in text:
            raise AssertionError(f"uninstall hook is missing: {expected}")


def app_root(appdata: Path) -> Path:
    return appdata / ROAMING_ROOT


def local_root(localappdata: Path) -> Path:
    return localappdata / LOCAL_ROOT


def backup_root(documents: Path) -> Path:
    return documents / BACKUP_NAME


def count_identity_files(path: Path) -> int:
    if not path.exists():
        return 0
    return sum(1 for item in path.rglob("identity.json") if item.is_file())


def count_marked_messages(path: Path) -> int:
    if not path.exists():
        return 0
    count = 0
    for item in path.rglob("*"):
        if item.is_file() and item.read_text(encoding="utf-8").find(MARK) != -1:
            count += 1
    return count


def count_backup_artifacts(documents: Path) -> int:
    return int(backup_root(documents).exists()) + int((documents / OLD_BACKUP_NAME).exists())


def write_fixture(appdata: Path, localappdata: Path) -> None:
    core = app_root(appdata) / "osl-core"
    identity = core / "hub-identities" / "owner" / "identity.json"
    messages = core / "messages"
    identity.parent.mkdir(parents=True)
    messages.mkdir(parents=True)
    local_root(localappdata).mkdir(parents=True)
    (appdata / LEGACY_ROOT).mkdir(parents=True)
    (core / "hub-active-identity").write_text("owner", encoding="utf-8")
    identity.write_text('{"user_id":"task-3615-owner"}', encoding="utf-8")
    for index in range(2):
        (messages / f"marked-{index}.json").write_text(
            f'{{"body":"{MARK}-{index}"}}',
            encoding="utf-8",
        )
    (local_root(localappdata) / "cache.json").write_text("local cache", encoding="utf-8")
    (appdata / LEGACY_ROOT / "legacy.json").write_text("legacy", encoding="utf-8")


def simulate_uninstall(choice: str, appdata: Path, localappdata: Path, documents: Path) -> None:
    assert_hook_is_wired_to_cleanup()
    documents.mkdir(parents=True, exist_ok=True)
    backup = backup_root(documents)
    old_backup = documents / OLD_BACKUP_NAME

    if choice == "keep-backup":
        shutil.rmtree(backup, ignore_errors=True)
        old_backup.unlink(missing_ok=True)
        source = app_root(appdata)
        if source.exists():
            shutil.copytree(source, backup / ROAMING_ROOT)
    elif choice == "no-backup":
        shutil.rmtree(backup, ignore_errors=True)
        old_backup.unlink(missing_ok=True)
    else:
        raise AssertionError(f"unknown uninstall choice: {choice}")

    for root in (app_root(appdata), local_root(localappdata), appdata / LEGACY_ROOT):
        shutil.rmtree(root, ignore_errors=True)


def simulate_reinstall(appdata: Path, localappdata: Path) -> None:
    app_root(appdata).mkdir(parents=True, exist_ok=True)
    local_root(localappdata).mkdir(parents=True, exist_ok=True)


def restore_backup(documents: Path, appdata: Path) -> None:
    source = backup_root(documents) / ROAMING_ROOT
    if not source.is_dir():
        raise AssertionError("kept backup is missing")
    destination = app_root(appdata)
    shutil.rmtree(destination, ignore_errors=True)
    shutil.copytree(source, destination)


class ReinstallBackupRestoreTest(unittest.TestCase):
    def test_no_backup_reinstall_then_keep_backup_restore(self) -> None:
        with tempfile.TemporaryDirectory(prefix="osl-3615-no-backup-") as temp:
            root = Path(temp)
            appdata = root / "AppData" / "Roaming"
            localappdata = root / "AppData" / "Local"
            documents = root / "Documents"
            write_fixture(appdata, localappdata)

            no_before_identities = count_identity_files(app_root(appdata))
            no_before_marked = count_marked_messages(app_root(appdata))
            simulate_uninstall("no-backup", appdata, localappdata, documents)
            simulate_reinstall(appdata, localappdata)
            no_after_identities = count_identity_files(app_root(appdata))
            no_after_marked = count_marked_messages(app_root(appdata))

            print(
                "TASK3615 no_backup "
                f"before.identity={no_before_identities} before.marked_messages={no_before_marked} "
                f"after.identity={no_after_identities} after.marked_messages={no_after_marked}"
            )
            self.assertEqual(no_before_identities, 1)
            self.assertEqual(no_before_marked, 2)
            self.assertEqual(no_after_identities, 0)
            self.assertEqual(no_after_marked, 0)

        with tempfile.TemporaryDirectory(prefix="osl-3615-keep-backup-") as temp:
            root = Path(temp)
            appdata = root / "AppData" / "Roaming"
            localappdata = root / "AppData" / "Local"
            documents = root / "Documents"
            write_fixture(appdata, localappdata)

            simulate_uninstall("keep-backup", appdata, localappdata, documents)
            simulate_reinstall(appdata, localappdata)
            keep_backup_count = count_backup_artifacts(documents)
            keep_after_reinstall_identities = count_identity_files(app_root(appdata))
            keep_after_reinstall_marked = count_marked_messages(app_root(appdata))
            restore_backup(documents, appdata)
            restored_identities = count_identity_files(app_root(appdata))
            restored_marked = count_marked_messages(app_root(appdata))

            print(
                "TASK3615 keep_backup "
                f"backup_count={keep_backup_count} "
                f"after_reinstall.identity={keep_after_reinstall_identities} "
                f"after_reinstall.marked_messages={keep_after_reinstall_marked} "
                f"restored.identity={restored_identities} restored.marked_messages={restored_marked}"
            )
            self.assertEqual(keep_backup_count, 1)
            self.assertEqual(keep_after_reinstall_identities, 0)
            self.assertEqual(keep_after_reinstall_marked, 0)
            self.assertEqual(restored_identities, 1)
            self.assertEqual(restored_marked, 2)


if __name__ == "__main__":
    unittest.main()
