// Protected-download, deletion, and Scrub declarations. Append related work here.
//
// TASK 5166: the quarantine-and-AMSI boundary every protected download crosses
// before any byte is visible outside OSL. Deliberately ungated and dependent on
// nothing but std/sha2/hex/base64, so the 5166b bypass harness can compile this
// exact source file on its own.
pub mod download_zone_handoff;
// TASK 5180: the archive-expansion boundary the quarantine release door runs
// before a container may leave.
pub mod protected_archive;
pub mod protected_download_final_save;
pub mod protected_download_quarantine;
pub mod scrub_erasure;
pub mod scrub_erasure_queue;
pub mod scrub_erasure_tracker;
pub mod scrub_evidence_manifest;
pub mod shared_conversation_scroll;
pub mod shared_place_text;
#[cfg(feature = "core")]
pub mod shipping_icloud_mailbox_receive;
#[cfg(feature = "core")]
pub mod shipping_mailbox_pointer_reader;
pub mod tor_pref;
pub mod scrub_hosted {
    pub mod checkpoint;
    pub mod fixture;
    pub mod friction;
    pub mod ordering;
    pub mod place_scope;
    pub mod proton_mail;
    pub mod reader;
    pub mod verify_surface;
    pub mod x_web;
    pub mod yahoo_mail;
}
#[cfg(feature = "core")]
pub mod remove_everything;
#[cfg(all(feature = "core", feature = "desktop"))]
pub mod revocation_drain_timer;
#[cfg(feature = "core")]
pub mod rn_attribution;
#[cfg(feature = "core")]
pub mod rn_recovery;
#[cfg(feature = "core")]
pub mod run_choices;
pub mod scrub_hosted_port;
pub mod scrub_setup_store;
/// **Binding Ledger 9, the seam ledger.** Adapters declared vs adapters with a
/// live carry receipt, ratcheted in both directions against
/// `carry-receipts/seam-ledger-baseline.json`. Test-only because its inputs --
/// `native_apps::tests::fleet_report` and the receipt verifier -- are, and
/// because a ledger is a gate rather than product code.
#[cfg(test)]
pub(crate) mod seam_ledger;
/// The one production `RawBackend` for `ipc::secure_local_store::SealedStore`.
/// Every other implementation in the tree is `#[cfg(test)]`, which is why the
/// offline send queue could not be wired at all before this module existed.
pub mod secure_disk_backend;
