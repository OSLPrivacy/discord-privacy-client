//! TA-T10-003a regression: a MISSING `burned_scopes.json` must not be read as
//! "nothing was ever burned".
//!
//! `load_burned_scopes` already failed CLOSED for a kill list that exists but
//! cannot be read — the `BURN_STATE_UNREADABLE` latch. The missing-file path
//! was a hole in that same posture: it returned a fresh empty ledger, so
//! deleting one file un-burned every burned message and the next write made
//! the loss permanent.
//!
//! "The ledger is not there" has two readings and they need opposite answers:
//! a genuine first run (nothing was ever burned — stay usable) and a ledger
//! deleted after burns existed (fail closed). They are told apart by the
//! burn-ledger enrolment marker on `whitelist_state.json`.
//!
//! These tests drive the real loader and the real writers against real files
//! and assert on observable behaviour — the latch and the write refusal, which
//! are exactly what `is_message_in_burn_kill_list` keys off — not on source
//! text.

use std::fs;
use std::sync::Mutex;

use ipc::burned_scopes_file::{
    burn_state_unreadable, load_burned_scopes, reset_burn_state_unreadable_for_tests,
    write_burned_scopes, BurnedScopeEntry, BurnedScopesFile,
};
use ipc::main_password::set_file_storage_key;
use ipc::whitelist_state::{
    burn_ledger_enrollment, load_whitelist_state_file, mark_burn_ledger_enrolled,
    write_whitelist_state, write_whitelist_state_file, BurnLedgerEnrollment, WhitelistStateFile,
};
use tempfile::TempDir;

/// The keystore base dir, the at-rest file key and the `BURN_STATE_UNREADABLE`
/// latch are process globals shared by every test in this binary.
static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

/// One isolated account, with the process globals held and reset around it.
/// Field order matters: the guard drops last, so no other test can observe the
/// globals while this one is tearing its temp dirs down.
struct Fixture {
    account: TempDir,
    _keystore_base: TempDir,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Fixture {
    fn new() -> Self {
        let guard = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let base = TempDir::new().unwrap();
        keystore::set_base_dir_override(Some(base.path().to_path_buf()));
        set_file_storage_key(None);
        reset_burn_state_unreadable_for_tests();
        Self {
            account: TempDir::new().unwrap(),
            _keystore_base: base,
            _guard: guard,
        }
    }

    fn dir(&self) -> &std::path::Path {
        self.account.path()
    }

    fn ledger(&self) -> std::path::PathBuf {
        self.account.path().join("burned_scopes.json")
    }

    fn whitelist(&self) -> std::path::PathBuf {
        self.account.path().join("whitelist_state.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        reset_burn_state_unreadable_for_tests();
        keystore::set_base_dir_override(None);
    }
}

fn burn_entry() -> BurnedScopeEntry {
    BurnedScopeEntry {
        scope_kind: "dm".to_string(),
        scope_id: "900000000000000003".to_string(),
        server_id: None,
        channel_id: None,
        burned_at: 1_753_000_000,
        burned_message_ids: vec!["1420000000000000001".to_string()],
    }
}

fn seeded_ledger() -> BurnedScopesFile {
    BurnedScopesFile {
        version: 1,
        scopes: vec![burn_entry()],
    }
}

/// A whitelist file with real content but no burn ever recorded — what an
/// account that has toggled a whitelist and never pressed burn looks like.
fn write_plain_whitelist(dir: &std::path::Path) {
    write_whitelist_state_file(
        &dir.join("whitelist_state.json"),
        &WhitelistStateFile {
            migrated_c1: true,
            scopes: Default::default(),
            server_defaults: Default::default(),
        },
    )
    .expect("write whitelist envelope");
}

// ---------------------------------------------------------------------------
// The two cases the fix exists to separate.
// ---------------------------------------------------------------------------

/// A genuine first run must stay fully usable. No ledger, no whitelist, no
/// marker — nothing was ever burned, so nothing may be blocked.
///
/// This is the guard on the trap in the other direction: if the
/// deleted-ledger check were too aggressive, a fresh install would be
/// permanently unable to open anything.
#[test]
fn first_run_with_no_ledger_stays_open() {
    let fx = Fixture::new();
    let bs_path = fx.ledger();

    assert!(!bs_path.exists(), "fixture must start with no kill list");
    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::NeverEnrolled,
        "an account with no state at all has never enrolled"
    );

    let loaded = load_burned_scopes(&bs_path);

    assert!(
        loaded.scopes.is_empty(),
        "a first run has an empty kill list"
    );
    assert!(
        !burn_state_unreadable(),
        "a first run must NOT latch fail-closed — that would brick a fresh install"
    );
    write_burned_scopes(&bs_path, &seeded_ledger())
        .expect("a first run must still be able to record its first burn");
}

/// The defect. A ledger that existed, recorded burns, and was then DELETED
/// must fail closed: the burn promise outlives the file that recorded it.
#[test]
fn ledger_deleted_after_enrollment_fails_closed() {
    let fx = Fixture::new();
    let bs_path = fx.ledger();

    // 1. A real burn happens: the ledger is written and enrolment recorded.
    write_burned_scopes(&bs_path, &seeded_ledger()).expect("seed kill list");
    mark_burn_ledger_enrolled(fx.dir()).expect("record enrolment");
    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::Enrolled
    );

    // 2. The ledger is deleted — a one-file wipe, a bad restore, an attacker.
    fs::remove_file(&bs_path).expect("delete the kill list");
    assert!(!bs_path.exists());

    // 3. The loader must NOT report this as "nothing was ever burned".
    let loaded = load_burned_scopes(&bs_path);

    assert!(
        burn_state_unreadable(),
        "deleting burned_scopes.json after burns existed must fail CLOSED — \
         without the latch every burned message becomes decryptable again"
    );
    assert!(
        loaded.scopes.is_empty(),
        "the returned list is empty, which is exactly why it must not be treated \
         as authoritative while the latch holds"
    );
    let refusal = write_burned_scopes(&bs_path, &BurnedScopesFile::default())
        .expect_err("writing an empty ledger over a lost one must be refused");
    assert!(
        refusal.contains("refusing to write"),
        "unexpected refusal message: {refusal}"
    );
    assert!(
        !bs_path.exists(),
        "the refused write must not have recreated the ledger"
    );
}

// ---------------------------------------------------------------------------
// Guards on the "permanently unable to open anything" trap.
// ---------------------------------------------------------------------------

/// An account that has whitelist state but has never burned is still a
/// never-enrolled account. Only a burn sets the marker.
#[test]
fn whitelist_without_a_burn_is_not_enrollment() {
    let fx = Fixture::new();
    write_plain_whitelist(fx.dir());

    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::NeverEnrolled
    );
    load_burned_scopes(&fx.ledger());
    assert!(
        !burn_state_unreadable(),
        "having a whitelist is not evidence of a burn"
    );
}

/// An unreadable whitelist draws NO conclusion. Pre-gate bootstrap runs before
/// the at-rest file key is installed, so "I could not read it" is routine
/// there; concluding "enrolled" would latch every launch closed before the
/// user has even unlocked. The post-unlock `state_reload` pass re-runs the
/// decision with the key installed.
#[test]
fn unreadable_whitelist_draws_no_conclusion() {
    let fx = Fixture::new();
    fs::write(fx.whitelist(), b"not json at all").expect("write corrupt whitelist");

    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::Indeterminate
    );
    load_burned_scopes(&fx.ledger());
    assert!(
        !burn_state_unreadable(),
        "an unreadable enrolment marker must not be read as enrolment"
    );
}

/// `fresh_start` is the escape hatch: it deletes `whitelist_state.json` and
/// lays down an empty one through `write_whitelist_state`, which deliberately
/// does not carry the marker. Without this a user whose ledger was lost would
/// be stuck refusing forever with the product's own reset button unable to
/// help them.
#[test]
fn fresh_start_reset_clears_enrollment() {
    let fx = Fixture::new();
    let wl_path = fx.whitelist();

    mark_burn_ledger_enrolled(fx.dir()).expect("record enrolment");
    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::Enrolled
    );

    // What fresh_start does: remove, then write an empty one.
    fs::remove_file(&wl_path).expect("fresh_start removes the file");
    write_whitelist_state(&wl_path, &Default::default()).expect("fresh_start rewrites it empty");

    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::NeverEnrolled,
        "account reset must clear enrolment or a lost ledger is a permanent brick"
    );
    load_burned_scopes(&fx.ledger());
    assert!(
        !burn_state_unreadable(),
        "a reset account must be usable again"
    );
}

// ---------------------------------------------------------------------------
// Guard on the marker itself.
// ---------------------------------------------------------------------------

/// The marker is not a field callers can forget. An ordinary whitelist write —
/// a preference toggle that knows nothing about burns — must not clear it,
/// because clearing it fails OPEN.
#[test]
fn ordinary_whitelist_write_cannot_clear_enrollment() {
    let fx = Fixture::new();

    mark_burn_ledger_enrolled(fx.dir()).expect("record enrolment");
    write_plain_whitelist(fx.dir());

    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::Enrolled,
        "an unrelated whitelist write silently un-enrolled the account"
    );
}

/// Enrolment survives being recorded twice, and the marker is preserved
/// alongside — not instead of — the rest of the file's contents.
#[test]
fn enrollment_is_idempotent_and_preserves_the_envelope() {
    let fx = Fixture::new();
    let wl_path = fx.whitelist();

    write_plain_whitelist(fx.dir());
    mark_burn_ledger_enrolled(fx.dir()).expect("first mark");
    mark_burn_ledger_enrolled(fx.dir()).expect("second mark is a no-op");

    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::Enrolled
    );
    // Read back through the real loader — the file is sealed at rest, so the
    // bytes on disk are not JSON.
    let reloaded =
        load_whitelist_state_file(&wl_path).expect("whitelist still loads after marking");
    assert!(
        reloaded.migrated_c1,
        "marking enrolment must not flatten the envelope it rides on"
    );
}

/// A legacy v1 whitelist has no envelope — the top-level object IS the
/// scope-keyed map. Dropping a bare marker key beside those scopes would make
/// the loader try to read `burn_ledger_enrolled: true` as a scope named
/// "burn_ledger_enrolled", which fails to deserialise and takes the entire
/// whitelist down with it. Marking must lift the file into the envelope form
/// instead, preserving the scopes.
#[test]
fn marking_a_legacy_v1_whitelist_does_not_corrupt_it() {
    let fx = Fixture::new();
    let wl_path = fx.whitelist();

    // Shape of a pre-9-C1 file: no `scopes`, no `migrated_c1`.
    let legacy = serde_json::json!({
        "dm:900000000000000003": {
            "encrypt_toggle": true,
            "auto_enabled": false,
        }
    });
    fs::write(
        &wl_path,
        serde_json::to_vec_pretty(&legacy).expect("serialize legacy whitelist"),
    )
    .expect("write legacy whitelist");
    let before = load_whitelist_state_file(&wl_path).expect("legacy file loads before marking");
    assert_eq!(before.scopes.len(), 1, "fixture must have one legacy scope");

    mark_burn_ledger_enrolled(fx.dir()).expect("mark a legacy file");

    assert_eq!(
        burn_ledger_enrollment(fx.dir()),
        BurnLedgerEnrollment::Enrolled
    );
    let after = load_whitelist_state_file(&wl_path)
        .expect("legacy whitelist must still load after marking");
    assert_eq!(
        after.scopes.len(),
        1,
        "marking enrolment destroyed the legacy scopes"
    );
    assert!(
        !after.migrated_c1,
        "wrapping is not migrating — the C1 migration must still run"
    );
}
