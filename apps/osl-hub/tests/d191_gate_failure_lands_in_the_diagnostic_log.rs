//! D-191 — the failure that motivated wiring a subscriber, reproduced.
//!
//! This does not assert that a subscriber initialises. It drives the **real
//! shipping unlock command** — `cmd_osl_verify_gate_password`, the one D-187
//! established the product actually calls — with a genuinely broken profile,
//! and then reads the diagnostic file back off disk. The claim under test is
//! the one D-142, D-150 and D-187 each paid for separately:
//!
//! > the user is shown a 57-character generic, and the sentence naming the
//! > cause exists **somewhere on the machine**.
//!
//! Two profiles, because the gate has two failure branches and both were
//! silent (`crates/ipc/src/commands.rs:14761` and `:14774`):
//!
//! 1. **an identity that will not reopen** — `identity.json` present, sealed
//!    under a key this machine does not have. `unlock_session` returns `Err`;
//!    this is D-187's exact path.
//! 2. **a state file sealed under a different key** — `unlock_session` returns
//!    `Ok` with a non-empty `reload.errors`.
//!
//! Both also carry the secret half of the defect. Turning a log on in a
//! privacy product is only safe if the call sites are clean, so this asserts
//! what must **not** be in the file: the account name embedded in the profile
//! path, and the recovery phrase the product minted while setting the
//! password up.
//!
//! # Mutants this must survive
//!
//! * delete the `try_init()` in `osl_privacy_hub::diagnostics` → the file has
//!   no tagged line, the "landed" assertions fail;
//! * run with `OSL_LOG=off` (or any filter excluding `ERROR`) → same;
//! * make `ipc::log_id::redact_path` the identity function → the account-name
//!   assertion fails.

use std::path::{Path, PathBuf};

/// Long enough to satisfy `validate_password`, and distinctive enough that a
/// substring search for it is meaningful.
const PASSWORD: &str = "osl-D191-Passw0rd!";

/// The account name that must never survive into a file a friend is asked to
/// send. It sits where a real profile puts it: one component below `home`.
const CANARY_ACCOUNT_NAME: &str = "d191-canary-user";

struct Profile {
    _temp: tempfile::TempDir,
    account: PathBuf,
    /// The recovery phrase the product minted. A real secret, used here as the
    /// canary for "no secret reaches the file".
    phrase: String,
}

/// A profile rooted at `<temp>/home/<CANARY_ACCOUNT_NAME>/osl-core`, with a
/// main password set and an account directory selected — i.e. the state a
/// returning launch finds.
fn onboarded_profile() -> Profile {
    let temp = tempfile::tempdir().expect("tempdir");
    let base = temp
        .path()
        .join("home")
        .join(CANARY_ACCOUNT_NAME)
        .join("osl-core");
    let account = base.join("hub-identities").join("id-d191");
    std::fs::create_dir_all(&account).expect("create account dir");

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(Some(base.clone()));
    keystore::set_active_account_dir(Some(account.clone()));
    ipc::burned_scopes_file::reset_burn_state_unreadable_for_tests();

    let phrase = ipc::main_password::set_main_password(&base, PASSWORD).expect("set password");
    // The gate is the first thing that touches the profile in a fresh process;
    // nothing may be pre-installed.
    ipc::main_password::set_file_storage_key(None);

    Profile {
        _temp: temp,
        account,
        phrase,
    }
}

fn release_profile() {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
}

/// An `identity.json` whose sealed body is not openable by any key on this
/// machine — the "the key that opens this account is gone" shape. The method
/// label is the live sealer's, so the failure is an unseal failure rather than
/// a version or method rejection; if a neighbouring lane has revoked the
/// machine-wide sealing key underneath us the label stops matching and the
/// load fails for that reason instead, which is the same defect from the same
/// branch.
fn write_unopenable_identity(account: &Path) {
    let sealer = keystore::select_best_sealer();
    let blob = keystore::IdentityOnDisk {
        version: keystore::IDENTITY_BLOB_VERSION,
        method: sealer.method_label().to_string(),
        sealed_b64: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
        insecure_banner: None,
    };
    std::fs::write(
        account.join("identity.json"),
        serde_json::to_vec_pretty(&blob).expect("encode identity blob"),
    )
    .expect("write identity.json");
}

/// A `peer_map.json` carrying the at-rest magic but sealed under a key the
/// gate will never hold, so the post-gate quarantine sweep fires.
fn write_wrong_key_peer_map(account: &Path) {
    let wrong_key = [7u8; 32];
    let blob =
        ipc::main_password::encrypt_at_rest(b"{}", &wrong_key).expect("seal under wrong key");
    std::fs::write(account.join("peer_map.json"), blob).expect("write peer_map.json");
}

/// The whole user-visible surface of both failures. If this string ever grows
/// a cause, this test is the place that notices.
const GENERIC: &str = "OSL encrypted security state could not be reloaded safely";

fn read_log(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("no diagnostic at {path:?}: {e}"))
}

#[test]
fn a_gate_refusal_reaches_the_diagnostic_file_while_the_user_still_sees_the_generic() {
    let log_home = tempfile::tempdir().expect("log tempdir");
    let log_path = log_home.path().join("osl-diagnostics.log");
    osl_privacy_hub::diagnostics::init_diagnostic_subscriber_at(log_path.clone());

    // ---------------------------------------------------------------
    // 1. D-187's path: an identity that will not reopen.
    // ---------------------------------------------------------------
    let profile = onboarded_profile();
    write_unopenable_identity(&profile.account);

    let state = ipc::AppState::new();
    let verdict = ipc::commands::cmd_osl_verify_gate_password(&state, PASSWORD.to_owned());

    let shown = verdict.expect_err("an unopenable identity must not unlock the session");
    assert_eq!(
        shown, GENERIC,
        "the user is shown the generic and nothing else"
    );

    let log = read_log(&log_path);
    assert!(
        log.contains("OSL: required post-gate state reload failed; keeping session locked"),
        "the tagged refusal never reached the file:\n{log}"
    );
    assert!(
        log.contains("sealed identity could not be reopened"),
        "the file records the refusal but not WHICH loader refused — that is \
         the whole defect:\n{log}"
    );
    let phrase = profile.phrase.clone();
    release_profile();
    drop(profile);

    // ---------------------------------------------------------------
    // 2. The other branch: a state file sealed under a different key.
    // ---------------------------------------------------------------
    let profile = onboarded_profile();
    write_wrong_key_peer_map(&profile.account);

    let state = ipc::AppState::new();
    let verdict = ipc::commands::cmd_osl_verify_gate_password(&state, PASSWORD.to_owned());
    let shown = verdict.expect_err("a wrong-key state file must not unlock the session");
    assert_eq!(shown, GENERIC, "same generic, second branch");

    let log = read_log(&log_path);
    assert!(
        log.contains("peer_map.json: sealed by a different key; quarantined to"),
        "the per-loader cause did not reach the file:\n{log}"
    );

    // ---------------------------------------------------------------
    // 3. What must NOT be in the file.
    // ---------------------------------------------------------------
    assert!(
        !log.contains(CANARY_ACCOUNT_NAME),
        "the account name embedded in the profile path reached the diagnostic \
         file; `redact_path` is not doing its job:\n{log}"
    );
    assert!(
        log.contains("<user>"),
        "the quarantine path should still be logged, redacted — if this fails \
         the path was dropped rather than redacted, and the assertion above \
         proves nothing:\n{log}"
    );
    assert!(
        !log.contains(PASSWORD),
        "the gate password reached the diagnostic file:\n{log}"
    );
    // A single BIP-39 word is not identifying; a run of them is. Assert on the
    // whole phrase and on every four-word window of it.
    for secret in [phrase.as_str(), profile.phrase.as_str()] {
        assert!(
            !log.contains(secret),
            "a recovery phrase reached the diagnostic file"
        );
        let words: Vec<&str> = secret.split_whitespace().collect();
        for window in words.windows(4) {
            let run = window.join(" ");
            assert!(
                !log.contains(&run),
                "part of a recovery phrase reached the diagnostic file: {run}"
            );
        }
    }

    // The artifact this defect is about. Printed so `--nocapture` yields the
    // file itself, not a claim about it.
    println!("--- {} ---\n{log}--- end ---", log_path.display());

    release_profile();
}
