//! Phase 9-F0-FIX2 / A31 snowflake-registration regression tests.
//!
//! V2's onboarding has no `keyserver.json` (retired) and no
//! password-set-time identity gen (`cmd_osl_set_main_password`
//! has no user_id to seed `generate_identity` with). The first
//! moment we have a stable carrier account is when boot.js extracts
//! the Discord snowflake from the React runtime and calls
//! `cmd_osl_register_self_snowflake`. A31 changed that boundary:
//! a raw snowflake claim is no longer authority, so this command now
//! requires a loaded identity plus a valid account-ownership proof.
//!
//! These tests drive the test-seam helper
//! `cmd_osl_register_self_snowflake_with_dir`, which takes the
//! config dir explicitly so we can point it at a `tempdir()`
//! instead of the real `%APPDATA%\osl` / `~/.config/osl`.

use ipc::commands::cmd_osl_register_self_snowflake_with_dir;
use ipc::AppState;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

const TEST_SNOWFLAKE: &str = "147700845179948241";

fn ownership_proof_for(
    identity: &keystore::Identity,
    snowflake: &str,
) -> keystore::AccountOwnershipProof {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut challenge = keystore::ProofChallenge::new(
        [0x62; keystore::PROOF_CHALLENGE_NONCE_BYTES],
        snowflake,
        &identity.user_id,
        now.saturating_sub(1),
        now + 600,
    )
    .expect("valid challenge");
    keystore::AccountOwnershipProof::from_challenge(identity, &mut challenge, now)
        .expect("valid ownership proof")
}

#[test]
fn fresh_install_snowflake_registration_without_proof_refuses() {
    let state = AppState::new();
    assert!(
        state.identity.lock().unwrap().is_none(),
        "test precondition: AppState starts with no identity"
    );

    let dir = tempdir().unwrap();
    let result = cmd_osl_register_self_snowflake_with_dir(
        &state,
        TEST_SNOWFLAKE.to_string(),
        None,
        dir.path(),
    );
    assert!(
        result.is_err(),
        "registration without an identity-bound proof must refuse"
    );
    assert!(
        result.unwrap_err().contains("proof required"),
        "error should mention the missing proof"
    );
    assert!(
        state.identity.lock().unwrap().is_none(),
        "proofless registration must not generate an identity"
    );

    let identity_path = dir.path().join("identity.json");
    assert!(
        !identity_path.exists(),
        "proofless registration must not write identity.json at {}",
        identity_path.display()
    );
}

#[test]
fn existing_identity_no_regeneration() {
    // Set up an AppState with an identity already in place (sim
    // bootstrap-loaded state). Different user_id from the snowflake
    // to prove the fix doesn't overwrite an existing identity.
    let state = AppState::new();
    let preexisting_user_id = "alice";
    let preexisting = keystore::generate_identity(preexisting_user_id.to_string());
    let preexisting_x25519 = *preexisting.x25519_public.as_bytes();
    let ownership_proof = ownership_proof_for(&preexisting, TEST_SNOWFLAKE);
    *state.identity.lock().unwrap() = Some(preexisting);

    let dir = tempdir().unwrap();
    let result = cmd_osl_register_self_snowflake_with_dir(
        &state,
        TEST_SNOWFLAKE.to_string(),
        Some(ownership_proof),
        dir.path(),
    );
    assert!(result.is_ok(), "registration must succeed: {result:?}");

    // Identity NOT regenerated — user_id and keypairs unchanged.
    let guard = state.identity.lock().unwrap();
    let id = guard.as_ref().unwrap();
    assert_eq!(
        id.user_id, preexisting_user_id,
        "existing identity must NOT be replaced with snowflake-derived one"
    );
    assert_eq!(
        *id.x25519_public.as_bytes(),
        preexisting_x25519,
        "keypair must be preserved across snowflake registration"
    );

    // Snowflake stamped on the existing identity.
    assert_eq!(id.discord_snowflake.as_deref(), Some(TEST_SNOWFLAKE));
}

#[test]
fn snowflake_validation_unchanged() {
    // The validation (17-20 ASCII digits) gates BEFORE the
    // generation branch — invalid input must fail with a clear
    // error and leave AppState untouched.
    let state = AppState::new();
    let dir = tempdir().unwrap();

    // Too short.
    let r = cmd_osl_register_self_snowflake_with_dir(&state, "12345".to_string(), None, dir.path());
    assert!(r.is_err());
    assert!(r.unwrap_err().contains("invalid format"));

    // Non-digit characters.
    let r = cmd_osl_register_self_snowflake_with_dir(
        &state,
        "147700845179948abc".to_string(),
        None,
        dir.path(),
    );
    assert!(r.is_err());

    // Way too long.
    let r = cmd_osl_register_self_snowflake_with_dir(
        &state,
        "1234567890123456789012345".to_string(),
        None,
        dir.path(),
    );
    assert!(r.is_err());

    // None of the above should have populated the identity.
    assert!(
        state.identity.lock().unwrap().is_none(),
        "validation failures must leave state.identity untouched"
    );

    // And nothing on disk.
    assert!(
        !dir.path().join("identity.json").exists(),
        "validation failures must not leave a partial identity.json"
    );
}

#[test]
fn existing_identity_gets_snowflake_without_rewriting_user_id() {
    // Belt-and-braces check that the snowflake string is the exact
    // carrier account stamped on the identity, not the OSL routing id.
    let state = AppState::new();
    let dir = tempdir().unwrap();
    let snowflake = "987654321012345678";
    let identity = keystore::generate_identity("owner-osl-id".to_string());
    let ownership_proof = ownership_proof_for(&identity, snowflake);
    *state.identity.lock().unwrap() = Some(identity);
    cmd_osl_register_self_snowflake_with_dir(
        &state,
        snowflake.to_string(),
        Some(ownership_proof),
        dir.path(),
    )
    .unwrap();
    let guard = state.identity.lock().unwrap();
    let id = guard.as_ref().unwrap();
    assert_eq!(id.user_id, "owner-osl-id");
    assert_eq!(id.discord_snowflake.as_deref(), Some(snowflake));
}

/// Idempotency: re-running snowflake registration with the SAME
/// snowflake (e.g. boot.js's `oslEnsureSelfSnowflakeRegistered`
/// firing again after a settings-window roundtrip) must not
/// regenerate the identity or rewrite identity.json with new
/// keypairs.
#[test]
fn re_registration_is_idempotent() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    let identity = keystore::generate_identity("owner-osl-id".to_string());
    let ownership_proof = ownership_proof_for(&identity, TEST_SNOWFLAKE);
    *state.identity.lock().unwrap() = Some(identity);

    cmd_osl_register_self_snowflake_with_dir(
        &state,
        TEST_SNOWFLAKE.to_string(),
        Some(ownership_proof.clone()),
        dir.path(),
    )
    .unwrap();
    let first_pub = *state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public
        .as_bytes();

    // Re-run with the same snowflake.
    cmd_osl_register_self_snowflake_with_dir(
        &state,
        TEST_SNOWFLAKE.to_string(),
        Some(ownership_proof),
        dir.path(),
    )
    .unwrap();
    let second_pub = *state
        .identity
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .x25519_public
        .as_bytes();

    assert_eq!(
        first_pub, second_pub,
        "re-registration with same snowflake must not produce new keypair"
    );
}

/// Account-change refusal: a registered identity bound to one
/// snowflake refuses to be re-tagged with a different snowflake.
/// (Pre-FIX2 behavior; preserved.)
#[test]
fn re_registration_with_different_snowflake_refuses() {
    let state = AppState::new();
    let dir = tempdir().unwrap();
    let identity = keystore::generate_identity("owner-osl-id".to_string());
    let ownership_proof = ownership_proof_for(&identity, TEST_SNOWFLAKE);
    *state.identity.lock().unwrap() = Some(identity);

    cmd_osl_register_self_snowflake_with_dir(
        &state,
        TEST_SNOWFLAKE.to_string(),
        Some(ownership_proof),
        dir.path(),
    )
    .unwrap();

    let other = "999999999999999999";
    let current = state.identity.lock().unwrap().as_ref().unwrap().clone();
    let r = cmd_osl_register_self_snowflake_with_dir(
        &state,
        other.to_string(),
        Some(ownership_proof_for(&current, other)),
        dir.path(),
    );
    assert!(r.is_err(), "must refuse retag to a different snowflake");
    let msg = r.unwrap_err();
    assert!(
        msg.contains("snowflake mismatch") || msg.contains("refusing to retag"),
        "error msg should call out the mismatch: {msg}"
    );
}
