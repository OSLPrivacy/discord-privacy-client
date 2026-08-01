use ipc::commands::cmd_osl_register_self_snowflake_with_dir;
use ipc::AppState;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

const SNOWFLAKE: &str = "147700845179948241";

fn install_identity(state: &AppState, user_id: &str) -> keystore::Identity {
    let identity = keystore::generate_identity(user_id.to_owned());
    *state.identity.lock().unwrap() = Some(identity.clone());
    identity
}

fn proof_for(identity: &keystore::Identity, snowflake: &str) -> keystore::AccountOwnershipProof {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut challenge = keystore::ProofChallenge::new(
        [0x31; keystore::PROOF_CHALLENGE_NONCE_BYTES],
        snowflake,
        &identity.user_id,
        now.saturating_sub(1),
        now + 600,
    )
    .expect("valid challenge");
    keystore::AccountOwnershipProof::from_challenge(identity, &mut challenge, now)
        .expect("valid proof")
}

#[test]
fn register_self_snowflake_requires_account_ownership_proof() {
    let dir = tempdir().unwrap();
    let missing_state = AppState::new();
    let identity = install_identity(&missing_state, "owner-osl-id");

    let missing = cmd_osl_register_self_snowflake_with_dir(
        &missing_state,
        SNOWFLAKE.to_owned(),
        None,
        dir.path(),
    )
    .expect_err("missing proof must refuse");
    assert!(missing.contains("proof required"), "got: {missing}");
    assert!(
        missing_state
            .identity
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .discord_snowflake
            .is_none(),
        "proofless refusal must not stamp the account"
    );

    let mut wrong_account = proof_for(&identity, SNOWFLAKE);
    wrong_account.platform_id = "999999999999999999".to_owned();
    let err = cmd_osl_register_self_snowflake_with_dir(
        &missing_state,
        SNOWFLAKE.to_owned(),
        Some(wrong_account),
        dir.path(),
    )
    .expect_err("proof for a different account must refuse");
    assert!(err.contains("does not match account"), "got: {err}");

    let valid_state = AppState::new();
    let valid_identity = install_identity(&valid_state, "owner-osl-id");
    let valid = proof_for(&valid_identity, SNOWFLAKE);
    cmd_osl_register_self_snowflake_with_dir(
        &valid_state,
        SNOWFLAKE.to_owned(),
        Some(valid),
        dir.path(),
    )
    .expect("valid proof permits the registration");
    assert_eq!(
        valid_state
            .identity
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .discord_snowflake
            .as_deref(),
        Some(SNOWFLAKE)
    );
}
