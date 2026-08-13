#!/usr/bin/env python3
"""Static packaging/starvation gate for TASK 5200.

This checker deliberately reads independent production, UI, catalogue,
permission, schema and reader inventories. It never imports exporter code.
"""
from __future__ import annotations
import json
import re
import sys
from pathlib import Path


def fail(message: str) -> None:
    print(f"TASK5200B FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def read(root: Path, relative: str) -> str:
    path = root / relative
    if not path.is_file():
        fail(f"starved required file {relative}")
    return path.read_text(encoding="utf-8")


def require(text: str, needle: str, diagnostic: str) -> None:
    if needle not in text:
        fail(diagnostic)


def main() -> None:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else ".").resolve()
    core = read(root, "apps/osl-hub/src/account_export.rs")
    native = read(root, "apps/osl-hub/src/main.rs")
    commands = read(root, "apps/osl-hub/src/hub_command_surface.rs")
    permission = read(root, "apps/osl-hub/permissions/hub.toml")
    capability = read(root, "apps/osl-hub/capabilities/hub.json")
    ui_main = read(root, "apps/osl-hub-ui/src/main.ts")
    ui_export = read(root, "apps/osl-hub-ui/src/account-export.ts")
    catalogue = read(root, "apps/osl-hub-ui/src/catalogue/en.ts")
    reader = read(root, "tools/task-5200-account-export/src/main.rs")
    reader_manifest = read(root, "tools/task-5200-account-export/Cargo.toml")
    exporter_proof_manifest = read(root, "tools/task-5200-exporter-proof/Cargo.toml")
    exporter_proof_lib = read(root, "tools/task-5200-exporter-proof/src/lib.rs")
    reader_faults = read(root, "scripts/qa/task5200_clean_reader_faults.py")
    mutation_proof = read(root, "scripts/qa/test_task5200b_account_export_mutations.py")
    read(root, "docs/design/osl-account-export-v1.md")
    schema = json.loads(read(root, "docs/design/osl-account-export-v1.schema.json"))

    require(ui_main, '["export", "Export my data"]', "packaged Settings route unreachable")
    require(ui_main, 'settingsSection === "export"', "packaged Settings content unreachable")
    require(ui_main, 'querySelector<HTMLFormElement>("#account-export-form")', "export form user boundary absent")
    require(native, 'verification.role != VerifiedGateRole::Main', "main-account reauthorization boundary absent")
    require(native, 'caller.label() != "main"', "trusted Settings caller boundary absent")
    if native.count(".blocking_save_file()") < 2:
        fail("one or both native save journeys absent")
    require(native, 'set_title("Save encrypted OSL account export")', "native archive save journey absent")
    require(native, 'set_title("Save separate OSL export key")', "native key save journey absent")
    require(native, '.ok_or_else(|| "Archive save was cancelled".to_owned())?', "archive cancellation boundary absent")
    require(native, '.ok_or_else(|| "Key save was cancelled".to_owned())?', "key cancellation boundary absent")
    require(native, 'active_unlocked_osl_user_id(&core)? != owner', "post-enumeration account binding absent")

    warning1 = "OSL cannot recover your export key. Save and verify it before leaving this screen."
    warning2 = "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL."
    require(catalogue, warning1, "exact export-key warning absent or softened")
    require(catalogue, warning2, "exact independent-copy warning absent or softened")
    require(ui_export, "englishCatalogue.accountExportKeyWarning", "export-key warning routed around catalogue")
    require(ui_export, "englishCatalogue.accountExportIndependentCopyWarning", "independent-copy warning routed around catalogue")

    for needle, diagnostic in [
        ('read_entire_file(key_path, KEY_LIMIT, "saved key")', "full saved-key readback absent"),
        ('read_entire_file(archive_path, MAX_ARCHIVE_BYTES, "saved archive")', "full archive readback absent"),
        ("decrypt_archive_bytes(&saved_archive, &saved_key)", "full archive authentication absent"),
        ("expected_blocks.insert(0);", "authenticated manifest block absent from receipt comparison"),
        ("if verified.authenticated_blocks != expected_blocks", "authenticated-block set comparison absent"),
        ("for expected in 0..count {", "every authenticated block loop absent"),
        ("if !cursor.finished()", "final block/EOF authentication absent"),
        ("manifest.blocks.len() as u64 + 1 != count", "manifest completeness comparison absent"),
        ("saved archive full-readback byte count mismatch", "full archive byte-count comparison absent"),
        ("saved key full-readback byte count mismatch", "full key byte-count comparison absent"),
    ]:
        require(core, needle, diagnostic)
    for fault in ["FailedWriteArchive", "DiskFullKey", "ShortWriteArchive", "DeleteKey", "TruncateArchive", "CorruptArchive", "TearFinalBlock"]:
        if core.count(fault) < 2:
            fail(f"named media fault absent: {fault}")
    for branch in [
        "PostWriteMediaFault::FailedWriteArchive =>",
        "PostWriteMediaFault::DiskFullKey =>",
        "PostWriteMediaFault::ShortWriteArchive =>",
        "PostWriteMediaFault::DeleteKey =>",
        "PostWriteMediaFault::TruncateArchive | PostWriteMediaFault::TearFinalBlock =>",
        "PostWriteMediaFault::CorruptArchive =>",
    ]:
        require(core, branch, f"injected media fault branch absent: {branch}")

    require(commands, "export_hub_account_data,", "export command registration absent")
    require(permission, 'commands.allow = ["export_hub_account_data"]', "export command permission absent")
    require(capability, '"allow-export-hub-account-data"', "export command capability absent")
    if schema.get("$id") != "https://openstandardlibraries.org/schemas/osl-account-export-v1.schema.json":
        fail("published versioned schema identity absent")

    inventory_pattern = re.compile(r"pub const (SOURCE|SCHEMA|STORAGE)_OWNERSHIP_INVENTORY:.*?= &\[(.*?)\];", re.S)
    inventories = {name: tuple(re.findall(r'"([a-z_]+)"', body)) for name, body in inventory_pattern.findall(core)}
    required_classes = ("identity_profile", "settings", "friend_relationships", "messages", "attachments")
    if set(inventories) != {"SOURCE", "SCHEMA", "STORAGE"}:
        fail("one independent ownership inventory is starved")
    if any(value != required_classes for value in inventories.values()):
        fail("source/schema/storage ownership inventory mismatch")

    for forbidden in ["osl-hub", "osl_privacy_hub", "../apps", "../../crates"]:
        if forbidden in reader_manifest:
            fail(f"clean reader depends on production exporter: {forbidden}")
    require(
        exporter_proof_lib,
        '#[path = "../../../apps/osl-hub/src/account_export.rs"]',
        "exporter proof does not compile the production account-export module",
    )
    require(
        exporter_proof_manifest,
        'path = "../../apps/osl-hub/tests/task_5200_account_export.rs"',
        "exporter proof does not run the production test target",
    )
    for needle, diagnostic in [
        ("for expected in 0..frame_count", "clean reader skips authenticated blocks"),
        ("if !cursor.finished()", "clean reader skips final EOF check"),
        ("validate_parameters(&header, &key_file)", "clean reader skips KDF/AEAD parameter verification"),
        ("validate_manifest(&header, manifest, &verified, frame_count, oracle)", "clean reader skips complete manifest"),
        ("This is the first point at which any plaintext-derived information", "clean reader zero-plaintext release boundary absent"),
    ]:
        require(reader, needle, diagnostic)

    archive_attack_tokens = {
        "first-production-page": '"first-production-page-only"',
        "message41": '"truncate-only-message-41"',
        "attachment7": '"drop-only-attachment-7"',
        "osl-held-key": '"osl-held-key-unavailable"',
        "missing-format-field": '"missing-required-format-field"',
        "foreign-owner": '"seeded-second-person-private-canary"',
        "flipped-ciphertext": '"flipped-ciphertext-bit"',
        "reordered-blocks": '"reordered-authenticated-blocks"',
        "truncated-blocks": '"torn-final-authenticated-block"',
        "repeated-nonce": '"repeated-export-nonce-reuse"',
    }
    for attack, token in archive_attack_tokens.items():
        require(reader_faults, token, f"{attack} attack proof absent")
        require(mutation_proof, f'"{attack}"', f"{attack} red-proof registry absent")

    print("TASK5200B PASS routes=1 reauth=main native_saves=2 warnings=2 inventories=3 classes=5 full_readbacks=2 media_faults=7 clean_reader=independent")


if __name__ == "__main__":
    main()
