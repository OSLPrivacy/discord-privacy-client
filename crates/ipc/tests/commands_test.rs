use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    cmd_aead_open, cmd_aead_seal, cmd_fetch_pubkeys, cmd_generate_identity,
    cmd_init_keyserver, cmd_load_identity, cmd_register, cmd_save_identity, cmd_status,
    cmd_stego_decode, cmd_stego_encode, cmd_x25519_diffie_hellman, AeadOpenRequest,
    AeadSealRequest, StegoEncodeRequest,
};
use ipc::{AppState, IpcError};
use tempfile::TempDir;

fn rand_b64(n: usize) -> String {
    let bytes = crypto::random::random_bytes(n);
    STANDARD.encode(&bytes)
}

#[test]
fn generate_identity_seeds_state_and_returns_pub_bytes() {
    let state = AppState::new();
    assert!(!state.has_identity());
    let resp = cmd_generate_identity(&state, "alice".to_string()).unwrap();
    assert_eq!(resp.user_id, "alice");
    let x = STANDARD.decode(&resp.ik_x25519_pub_b64).unwrap();
    assert_eq!(x.len(), 32);
    let mlkem = STANDARD.decode(&resp.ik_mlkem768_pub_b64).unwrap();
    assert_eq!(mlkem.len(), 1184);
    assert!(state.has_identity());
}

#[test]
fn generate_identity_rejects_empty_user_id() {
    let state = AppState::new();
    let res = cmd_generate_identity(&state, "".to_string());
    assert!(matches!(res, Err(IpcError::InvalidArgument(_))));
    assert!(!state.has_identity());
    let res = cmd_generate_identity(&state, "   ".to_string());
    assert!(matches!(res, Err(IpcError::InvalidArgument(_))));
}

#[test]
fn save_then_load_round_trips_through_state() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");

    let state_a = AppState::new();
    let resp = cmd_generate_identity(&state_a, "alice".to_string()).unwrap();
    cmd_save_identity(&state_a, path.to_string_lossy().into_owned()).unwrap();

    let state_b = AppState::new();
    let loaded = cmd_load_identity(&state_b, path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(loaded.user_id, "alice");
    assert_eq!(loaded.ik_x25519_pub_b64, resp.ik_x25519_pub_b64);
    assert_eq!(loaded.ik_mlkem768_pub_b64, resp.ik_mlkem768_pub_b64);
    assert!(state_b.has_identity());
}

#[test]
fn save_without_identity_errors() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("identity.json");
    let state = AppState::new();
    let res = cmd_save_identity(&state, path.to_string_lossy().into_owned());
    assert!(matches!(res, Err(IpcError::IdentityMissing)));
}

#[test]
fn init_keyserver_accepts_valid_url() {
    let state = AppState::new();
    cmd_init_keyserver(&state, "http://127.0.0.1:3000".to_string()).unwrap();
    assert!(state.has_keyserver());
}

#[test]
fn init_keyserver_rejects_https() {
    let state = AppState::new();
    let res = cmd_init_keyserver(&state, "https://example.com".to_string());
    assert!(matches!(res, Err(IpcError::Keystore(_))));
}

#[test]
fn register_without_identity_errors() {
    let state = AppState::new();
    cmd_init_keyserver(&state, "http://127.0.0.1:1".to_string()).unwrap();
    let res = cmd_register(&state);
    assert!(matches!(res, Err(IpcError::IdentityMissing)));
}

#[test]
fn register_without_keyserver_errors() {
    let state = AppState::new();
    cmd_generate_identity(&state, "alice".to_string()).unwrap();
    let res = cmd_register(&state);
    assert!(matches!(res, Err(IpcError::KeyserverMissing)));
}

#[test]
fn fetch_pubkeys_without_keyserver_errors() {
    let state = AppState::new();
    let res = cmd_fetch_pubkeys(&state, "anyone".to_string());
    assert!(matches!(res, Err(IpcError::KeyserverMissing)));
}

#[test]
fn aead_seal_open_round_trip() {
    let plaintext = b"hello via the bridge".to_vec();
    let key_b64 = rand_b64(32);
    let nonce_b64 = rand_b64(24);
    let ad_b64 = Some(rand_b64(8));

    let sealed = cmd_aead_seal(AeadSealRequest {
        key_b64: key_b64.clone(),
        nonce_b64: nonce_b64.clone(),
        ad_b64: ad_b64.clone(),
        plaintext_b64: STANDARD.encode(&plaintext),
    })
    .unwrap();

    let opened = cmd_aead_open(AeadOpenRequest {
        key_b64,
        nonce_b64,
        ad_b64,
        ciphertext_b64: sealed.ciphertext_b64,
    })
    .unwrap();

    let recovered = STANDARD.decode(&opened.ciphertext_b64).unwrap();
    assert_eq!(recovered, plaintext);
}

#[test]
fn aead_open_rejects_tampered_ciphertext() {
    let plaintext = b"sensitive".to_vec();
    let key_b64 = rand_b64(32);
    let nonce_b64 = rand_b64(24);
    let sealed = cmd_aead_seal(AeadSealRequest {
        key_b64: key_b64.clone(),
        nonce_b64: nonce_b64.clone(),
        ad_b64: None,
        plaintext_b64: STANDARD.encode(&plaintext),
    })
    .unwrap();
    // Flip a byte in the ciphertext.
    let mut ct = STANDARD.decode(&sealed.ciphertext_b64).unwrap();
    ct[0] ^= 0x80;
    let res = cmd_aead_open(AeadOpenRequest {
        key_b64,
        nonce_b64,
        ad_b64: None,
        ciphertext_b64: STANDARD.encode(&ct),
    });
    assert!(matches!(res, Err(IpcError::Crypto(_))));
}

#[test]
fn aead_seal_rejects_wrong_key_length() {
    let res = cmd_aead_seal(AeadSealRequest {
        key_b64: rand_b64(16), // wrong size
        nonce_b64: rand_b64(24),
        ad_b64: None,
        plaintext_b64: STANDARD.encode(b"hi"),
    });
    assert!(matches!(res, Err(IpcError::InvalidArgument(_))));
}

#[test]
fn stego_round_trip() {
    let original = vec![0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45];
    let encoded = cmd_stego_encode(StegoEncodeRequest {
        ciphertext_b64: STANDARD.encode(&original),
    })
    .unwrap();
    assert!(encoded.stego_message.starts_with("DPC0::"));
    let decoded = cmd_stego_decode(encoded.stego_message).unwrap();
    let recovered = STANDARD.decode(&decoded.ciphertext_b64).unwrap();
    assert_eq!(recovered, original);
}

#[test]
fn stego_decode_rejects_non_mode0() {
    let res = cmd_stego_decode("plain old text".to_string());
    assert!(matches!(res, Err(IpcError::Stego(_))));
}

#[test]
fn status_reflects_state() {
    let state = AppState::new();
    let s = cmd_status(&state);
    assert!(!s.identity_loaded);
    assert!(!s.keyserver_initialised);
    assert!(s.user_id.is_none());

    cmd_generate_identity(&state, "liam".to_string()).unwrap();
    cmd_init_keyserver(&state, "http://localhost:3000".to_string()).unwrap();
    let s = cmd_status(&state);
    assert!(s.identity_loaded);
    assert!(s.keyserver_initialised);
    assert_eq!(s.user_id.as_deref(), Some("liam"));
    assert!(s.x25519_public_b64.is_some());
}

#[test]
fn x25519_diffie_hellman_round_trip() {
    let (a_sec, a_pub) = crypto::x25519::generate_keypair();
    let (b_sec, b_pub) = crypto::x25519::generate_keypair();
    let ab = cmd_x25519_diffie_hellman(
        STANDARD.encode(a_sec.as_bytes()),
        STANDARD.encode(b_pub.as_bytes()),
    )
    .unwrap();
    let ba = cmd_x25519_diffie_hellman(
        STANDARD.encode(b_sec.as_bytes()),
        STANDARD.encode(a_pub.as_bytes()),
    )
    .unwrap();
    assert_eq!(ab, ba);
    let bytes = STANDARD.decode(&ab).unwrap();
    assert_eq!(bytes.len(), 32);
}

#[test]
fn ipc_error_serializes_to_tagged_json() {
    // Confirms the error wire shape JS will see.
    let err = IpcError::IdentityMissing;
    let v: serde_json::Value = serde_json::to_value(&err).unwrap();
    assert_eq!(v["kind"], "IdentityMissing");
    assert!(v.get("message").is_none() || v["message"].is_null());

    let err2 = IpcError::InvalidArgument("bad input".to_string());
    let v: serde_json::Value = serde_json::to_value(&err2).unwrap();
    assert_eq!(v["kind"], "InvalidArgument");
    assert_eq!(v["message"], "bad input");
}
