#!/usr/bin/env python3
from __future__ import annotations
import shutil
import subprocess
import tempfile
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHECKER = ROOT / "tools/task-5200-account-export/check.py"
FILES = [
    "apps/osl-hub/src/account_export.rs", "apps/osl-hub/src/main.rs",
    "apps/osl-hub/src/hub_command_surface.rs", "apps/osl-hub/permissions/hub.toml",
    "apps/osl-hub/capabilities/hub.json", "apps/osl-hub-ui/src/main.ts",
    "apps/osl-hub-ui/src/account-export.ts", "apps/osl-hub-ui/src/catalogue/en.ts",
    "tools/task-5200-account-export/src/main.rs", "tools/task-5200-account-export/Cargo.toml",
    "docs/design/osl-account-export-v1.md", "docs/design/osl-account-export-v1.schema.json",
    "scripts/qa/task5200_clean_reader_faults.py",
    "scripts/qa/test_task5200b_account_export_mutations.py",
    "tools/task-5200-exporter-proof/Cargo.toml",
    "tools/task-5200-exporter-proof/src/lib.rs",
]

# Immutable journey/format attack ledger.  Keep this separate from the mutable
# source mutation table: a reviewer can see immediately which claims have a
# practical source mutant and which are exercised by the archive fault proof.
REQUIRED_ATTACKS = (
    "settings-route", "reauthorization", "archive-cancel-skip", "key-cancel-skip",
    "archive-native-save", "key-native-save", "warning-softened", "independent-copy-warning",
    "archive-readback", "key-readback", "manifest-auth", "skip-block", "skip-final",
    "lost-key-fault", "disk-full", "short-write", "torn-final", "post-write-corruption",
    "first-production-page", "message41", "attachment7", "class-inventory", "osl-held-key",
    "missing-format-field", "foreign-owner", "flipped-ciphertext", "reordered-blocks",
    "truncated-blocks", "repeated-nonce",
)

MUTANTS = [
    ("settings-route", "apps/osl-hub-ui/src/main.ts", '["export", "Export my data"]', '["account", "Account"]', "Settings route"),
    ("reauthorization", "apps/osl-hub/src/main.rs", "verification.role != VerifiedGateRole::Main", "false", "reauthorization"),
    ("archive-cancel-skip", "apps/osl-hub/src/main.rs", '.ok_or_else(|| "Archive save was cancelled".to_owned())?', ".unwrap()", "archive cancellation"),
    ("key-cancel-skip", "apps/osl-hub/src/main.rs", '.ok_or_else(|| "Key save was cancelled".to_owned())?', ".unwrap()", "key cancellation"),
    ("archive-native-save", "apps/osl-hub/src/main.rs", 'set_title("Save encrypted OSL account export")', 'set_title("Save export")', "archive save"),
    ("key-native-save", "apps/osl-hub/src/main.rs", 'set_title("Save separate OSL export key")', 'set_title("Save key")', "key save"),
    ("warning-softened", "apps/osl-hub-ui/src/catalogue/en.ts", "OSL cannot recover your export key.", "OSL may not recover your key.", "warning"),
    ("independent-copy-warning", "apps/osl-hub-ui/src/catalogue/en.ts", "Exports are independent copies.", "Exports might be copies.", "independent-copy warning"),
    ("archive-readback", "apps/osl-hub/src/account_export.rs", 'read_entire_file(archive_path, MAX_ARCHIVE_BYTES, "saved archive")', "Vec::new()", "archive readback"),
    ("key-readback", "apps/osl-hub/src/account_export.rs", 'read_entire_file(key_path, KEY_LIMIT, "saved key")', "Vec::new()", "key readback"),
    ("manifest-auth", "apps/osl-hub/src/account_export.rs", "decrypt_archive_bytes(&saved_archive, &saved_key)", "unreachable!()", "authentication"),
    ("skip-block", "apps/osl-hub/src/account_export.rs", "for expected in 0..count", "for expected in 0..count.saturating_sub(1)", "every authenticated block"),
    ("skip-final", "apps/osl-hub/src/account_export.rs", "if !cursor.finished()", "if false", "final block"),
    ("lost-key-fault", "apps/osl-hub/src/account_export.rs", "PostWriteMediaFault::DeleteKey =>", "PostWriteMediaFault::DeletedSecret =>", "media fault"),
    ("disk-full", "apps/osl-hub/src/account_export.rs", "PostWriteMediaFault::DiskFullKey =>", "PostWriteMediaFault::DiskFullRemoved =>", "media fault"),
    ("short-write", "apps/osl-hub/src/account_export.rs", "PostWriteMediaFault::ShortWriteArchive =>", "PostWriteMediaFault::ShortWriteRemoved =>", "media fault"),
    ("torn-final", "apps/osl-hub/src/account_export.rs", "PostWriteMediaFault::TruncateArchive | PostWriteMediaFault::TearFinalBlock =>", "PostWriteMediaFault::TruncateArchive =>", "media fault"),
    ("post-write-corruption", "apps/osl-hub/src/account_export.rs", "PostWriteMediaFault::CorruptArchive =>", "PostWriteMediaFault::CorruptionRemoved =>", "media fault"),
    ("first-production-page", "scripts/qa/task5200_clean_reader_faults.py", '"first-production-page-only"', '"removed-first-page"', "first-production-page"),
    ("message41", "scripts/qa/task5200_clean_reader_faults.py", '"truncate-only-message-41"', '"removed-message41"', "message41"),
    ("attachment7", "scripts/qa/task5200_clean_reader_faults.py", '"drop-only-attachment-7"', '"removed-attachment7"', "attachment7"),
    ("class-inventory", "apps/osl-hub/src/account_export.rs", '    "attachments",\n];', "];", "inventory"),
    ("osl-held-key", "scripts/qa/task5200_clean_reader_faults.py", '"osl-held-key-unavailable"', '"removed-osl-held-key"', "osl-held-key"),
    ("missing-format-field", "scripts/qa/task5200_clean_reader_faults.py", '"missing-required-format-field"', '"removed-format-field"', "missing-format-field"),
    ("foreign-owner", "scripts/qa/task5200_clean_reader_faults.py", '"seeded-second-person-private-canary"', '"removed-foreign-owner"', "foreign-owner"),
    ("flipped-ciphertext", "scripts/qa/task5200_clean_reader_faults.py", '"flipped-ciphertext-bit"', '"removed-ciphertext-flip"', "flipped-ciphertext"),
    ("reordered-blocks", "scripts/qa/task5200_clean_reader_faults.py", '"reordered-authenticated-blocks"', '"removed-reordered-blocks"', "reordered-blocks"),
    ("truncated-blocks", "scripts/qa/task5200_clean_reader_faults.py", '"torn-final-authenticated-block"', '"removed-truncated-blocks"', "truncated-blocks"),
    ("repeated-nonce", "scripts/qa/task5200_clean_reader_faults.py", '"repeated-export-nonce-reuse"', '"removed-repeated-nonce"', "repeated-nonce"),
]

def guard(mutants: tuple[tuple[str, str, str, str, str], ...]) -> None:
    attacks = tuple(mutant[0] for mutant in mutants)
    for attack in REQUIRED_ATTACKS:
        if attacks.count(attack) != 1:
            raise SystemExit(f"absent attack: {attack}")
    for attack in attacks:
        if attack not in REQUIRED_ATTACKS:
            raise SystemExit(f"unexpected attack: {attack}")

def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(["python3", str(root / "tools/task-5200-account-export/check.py"), str(root)], text=True, capture_output=True)

def main() -> None:
    if len(sys.argv) == 3 and sys.argv[1] == "--self-check-omit":
        guard(tuple(mutant for mutant in MUTANTS if mutant[0] != sys.argv[2]))
        raise AssertionError("self-check omission unexpectedly passed")
    if len(sys.argv) != 1:
        raise SystemExit("usage: test_task5200b_account_export_mutations.py [--self-check-omit ATTACK]")
    guard(tuple(MUTANTS))
    control = subprocess.run(["python3", str(CHECKER), str(ROOT)], text=True, capture_output=True)
    if control.returncode != 0:
        raise SystemExit(control.stderr)
    for name, relative, before, after, diagnostic in MUTANTS:
        with tempfile.TemporaryDirectory(prefix="task5200b-") as raw:
            temp = Path(raw)
            for item in FILES:
                target = temp / item
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(ROOT / item, target)
            shutil.copy2(CHECKER, temp / "tools/task-5200-account-export/check.py")
            path = temp / relative
            source = path.read_text()
            if before not in source:
                raise AssertionError(f"mutation anchor missing: {name}")
            path.write_text(source.replace(before, after, 1))
            result = run(temp)
            if result.returncode != 1 or diagnostic.lower() not in result.stderr.lower():
                raise AssertionError(f"{name} did not fail with {diagnostic}: {result.returncode} {result.stderr}")
            print(f"TASK5200B_MUTANT={name}|exit=1|diagnostic={result.stderr.strip()}")
    for attack in REQUIRED_ATTACKS:
        result = subprocess.run(["python3", str(Path(__file__).resolve()), "--self-check-omit", attack], text=True, capture_output=True)
        if result.returncode != 1 or f"absent attack: {attack}" not in result.stderr:
            raise AssertionError(f"self-check omission failed for {attack}: {result.stderr}")
        print(f"TASK5200B_ABSENT_GUARD={attack}|exit=1|diagnostic=absent attack: {attack}")
    print(f"TASK5200B_THROWAWAY_PACKAGES={len(MUTANTS)}|remaining=0")

if __name__ == "__main__":
    main()
