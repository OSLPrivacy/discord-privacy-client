use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    cmd_osl_attachment_cache_get, cmd_osl_attachment_cache_put, cmd_osl_set_friend_ids,
};
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use keystore::generate_identity;
use store::MessageStore;

const ROSE_ID: &str = "ROSE-0235";
const RANDOM_FILENAME: &str = "rose-0235.png";
const ACCEPTED_FRIEND_DID: &str = "900000000000235001";
const IMAGE_FINGERPRINT: &str = "IMG-0235";

fn state_with_store(dir: &std::path::Path) -> AppState {
    let state = AppState::new();
    let mut identity = generate_identity("rose-0235-recipient".to_string());
    identity.discord_snowflake = Some("900000000000235999".to_string());
    let secret = *identity.x25519_secret.as_bytes();
    *state.identity.lock().expect("identity mutex poisoned") = Some(identity);
    *state
        .message_store
        .lock()
        .expect("message_store mutex poisoned") =
        Some(MessageStore::open(dir, &secret).expect("open message store"));
    state
}

fn stored_picture_count(state: &AppState) -> usize {
    state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
        .expect("message store installed")
        .live_attachment_count()
        .expect("count live cached pictures")
}

fn request_picture(state: &AppState) -> (Result<String, String>, usize) {
    match cmd_osl_attachment_cache_get(state, ROSE_ID.to_string(), RANDOM_FILENAME.to_string()) {
        Ok(Some(dto)) => {
            let bytes = STANDARD
                .decode(dto.bytes_b64)
                .expect("picture bytes are base64");
            (
                Ok(String::from_utf8(bytes.clone()).expect("fixture is utf8")),
                bytes.len(),
            )
        }
        Ok(None) => (Ok(String::new()), 0),
        Err(err) => (Err(err), 0),
    }
}

#[test]
fn rose_0235_picture_cache_refuses_after_relationship_changes_to_blocked() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let state = state_with_store(dir.path());
    let scope = Scope::dm(ACCEPTED_FRIEND_DID);

    cmd_osl_set_friend_ids(&state, vec![ACCEPTED_FRIEND_DID.to_string()])
        .expect("seed accepted friend relationship");
    cmd_osl_attachment_cache_put(
        &state,
        ROSE_ID.to_string(),
        RANDOM_FILENAME.to_string(),
        "image/png".to_string(),
        STANDARD.encode(IMAGE_FINGERPRINT.as_bytes()),
        Some(ScopeInput::from(&scope)),
        Some(ACCEPTED_FRIEND_DID.to_string()),
    )
    .expect("seed ROSE-0235 cached picture");

    let before_count = stored_picture_count(&state);
    let rose_readable = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
        .expect("message store installed")
        .get_attachment(ROSE_ID, RANDOM_FILENAME)
        .expect("direct store read")
        .is_some();
    println!("ROSE-0235 readable={rose_readable}");
    println!("stored picture count before={before_count}");

    let (accepted_fingerprint, accepted_bytes) = request_picture(&state)
        .0
        .map(|fingerprint| {
            let bytes = fingerprint.as_bytes().len();
            (fingerprint, bytes)
        })
        .expect("accepted friend request should return picture");
    let after_accepted_count = stored_picture_count(&state);
    println!("accepted image fingerprint={accepted_fingerprint}");
    println!("accepted image bytes={accepted_bytes}");
    println!("stored picture count after accepted={after_accepted_count}");
    assert_eq!(
        accepted_fingerprint, IMAGE_FINGERPRINT,
        "TASK0235 missing IMG-0235: accepted image fingerprint={accepted_fingerprint:?} bytes={accepted_bytes}"
    );
    assert!(
        accepted_bytes > 0,
        "TASK0235 missing IMG-0235: accepted image bytes={accepted_bytes}"
    );

    cmd_osl_set_friend_ids(&state, Vec::new()).expect("relationship changed to blocked");
    let (blocked_result, blocked_bytes) = request_picture(&state);
    let blocked_error = blocked_result.expect_err("blocked relationship must refuse picture");
    let after_blocked_count = stored_picture_count(&state);
    let unchanged_fingerprint = state
        .message_store
        .lock()
        .expect("message_store mutex poisoned")
        .as_ref()
        .expect("message store installed")
        .get_attachment(ROSE_ID, RANDOM_FILENAME)
        .expect("direct store read after block")
        .map(|(_mime, bytes)| String::from_utf8(bytes).expect("fixture is utf8"))
        .expect("cached picture remains stored");
    println!("blocked refusal={blocked_error}");
    println!("blocked image bytes={blocked_bytes}");
    println!("stored picture count after blocked={after_blocked_count}");
    println!("stored image fingerprint after blocked={unchanged_fingerprint}");

    assert!(rose_readable);
    assert_eq!(before_count, 1);
    assert_eq!(accepted_fingerprint, IMAGE_FINGERPRINT);
    assert!(accepted_bytes > 0);
    assert_eq!(after_accepted_count, 1);
    assert_eq!(blocked_error, "OSL: picture access blocked");
    assert_eq!(blocked_bytes, 0);
    assert_eq!(after_blocked_count, 1);
    assert_eq!(unchanged_fingerprint, IMAGE_FINGERPRINT);
}
