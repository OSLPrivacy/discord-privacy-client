#!/usr/bin/env python3
"""TASK 4817 red proof: the throwaway plaintext-body mutation.

This rewrites `crates/ipc/src/sealed_sync.rs` so that the sealed sync path
still produces an envelope with a byte-identical server-visible header, the
same recipient slots and the same size class -- but with the body region
carrying the padded plaintext instead of the AEAD ciphertext. The receiving
side is rewritten to read that body without authenticating it, so the sync
values still merge correctly and every other leg of TASK 4817 still passes.
Only the byte inventory of the relay surfaces can tell the difference.

It is a mutation, not a feature. `--apply` keeps a `.orig` backup beside the
file and `--restore` puts it back; the tree must never be committed with this
applied. Run:

    python3 tools/task-4817-relay/red-proof/plaintext-mutation-red-proof.py --apply
    cargo test -p ipc --test task_4817_sealed_sync_confidentiality -- --nocapture --test-threads=1
    python3 tools/task-4817-relay/red-proof/plaintext-mutation-red-proof.py --restore
"""

import pathlib
import sys

TARGET = pathlib.Path(__file__).resolve().parents[3] / "crates/ipc/src/sealed_sync.rs"
BACKUP = TARGET.with_suffix(".rs.orig")

SEAL_FROM = """    let wire = wire_v2::encrypt_v3(
        sender_ik_sk,
        sender_ik_pub,
        recipients,
        MSG_TYPE_CONTENT,
        &padded,
    )?;
"""

SEAL_TO = """    let wire = wire_v2::encrypt_v3(
        sender_ik_sk,
        sender_ik_pub,
        recipients,
        MSG_TYPE_CONTENT,
        &padded,
    )?;
    // TASK 4817 RED PROOF MUTATION: keep every server-visible byte of the
    // envelope and put the body in the clear inside it.
    let wire = {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let mut raw = STANDARD
            .decode(wire.strip_prefix("DPC0::").expect("prefix"))
            .expect("base64");
        let n = raw[34] as usize;
        let start = 35 + n * crate::wire_v2::SLOT_V3_BYTES + 12;
        raw[start..start + padded.len()].copy_from_slice(&padded);
        format!("DPC0::{}", STANDARD.encode(&raw))
    };
"""

OPEN_FROM = """    let decrypted = wire_v2::decrypt_v3_for_sender(
        wire,
        recipient_ik_sk,
        recipient_mlkem_sk,
        expected_sender_ik,
    )?;
"""

OPEN_TO = """    // TASK 4817 RED PROOF MUTATION: read the body without authenticating it.
    let decrypted = {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine as _;
        let _ = (recipient_ik_sk, recipient_mlkem_sk, expected_sender_ik);
        let raw = STANDARD
            .decode(
                wire.strip_prefix("DPC0::")
                    .ok_or_else(|| SealedSyncError::Wire("missing DPC0 prefix".to_owned()))?,
            )
            .map_err(|err| SealedSyncError::Wire(err.to_string()))?;
        let n = raw[34] as usize;
        let start = 35 + n * crate::wire_v2::SLOT_V3_BYTES + 12;
        if raw.len() < start + 16 {
            return Err(SealedSyncError::Wire("short body".to_owned()));
        }
        crate::wire_v2::DecryptedV2 {
            msg_type: raw[1],
            plaintext: raw[start..raw.len() - 16].to_vec(),
        }
    };
"""


def apply() -> None:
    source = TARGET.read_text()
    if "RED PROOF MUTATION" in source:
        sys.exit("mutation already applied")
    for needle in (SEAL_FROM, OPEN_FROM):
        if needle not in source:
            sys.exit("sealed_sync.rs does not match the expected shape")
    BACKUP.write_text(source)
    source = source.replace(SEAL_FROM, SEAL_TO, 1).replace(OPEN_FROM, OPEN_TO, 1)
    TARGET.write_text(source)
    print("TASK4817_MUTATION_APPLIED file=crates/ipc/src/sealed_sync.rs backup=sealed_sync.rs.orig")


def restore() -> None:
    if not BACKUP.exists():
        sys.exit("no backup to restore")
    TARGET.write_text(BACKUP.read_text())
    BACKUP.unlink()
    print("TASK4817_MUTATION_RESTORED file=crates/ipc/src/sealed_sync.rs")


if __name__ == "__main__":
    if len(sys.argv) != 2 or sys.argv[1] not in ("--apply", "--restore"):
        sys.exit(__doc__)
    apply() if sys.argv[1] == "--apply" else restore()
