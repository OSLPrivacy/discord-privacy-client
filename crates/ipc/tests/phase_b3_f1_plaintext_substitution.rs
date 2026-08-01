//! B3-F1 regression: on an install with encryption-at-rest enrolled, a state
//! file that arrives WITHOUT the `OSL-ENC1` envelope is an attacker's file, not
//! a legacy file.
//!
//! `maybe_decrypt` used to return magic-less bytes verbatim even with a key
//! installed, so anyone who could write to the config dir could replace
//! peer_map.json — the trust root — with plaintext of their choosing. The
//! loaders re-write what they load, so the substitution was then RE-SEALED
//! under the victim's key: accepted and laundered into an authentic-looking
//! encrypted file.
//!
//! Behavioural test: real files, real loaders, assertions on the bytes on disk.

use std::fs;
use std::sync::Mutex;

use ipc::main_password::{has_enc_magic, set_file_storage_key, set_main_password};
use ipc::peer_map::{load_peer_map_from_path, write_peer_map, PeerEntry, PeerMap};
use tempfile::TempDir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());

const HONEST_DID: &str = "900000000000000003";
const ROGUE_DID: &str = "900000000000000009";

fn honest_map() -> PeerMap {
    let mut map = PeerMap::new();
    map.insert(
        HONEST_DID.to_string(),
        PeerEntry {
            osl_user_id: Some("liam".to_string()),
            discord_id: Some(HONEST_DID.to_string()),
            ..PeerEntry::default()
        },
    );
    map
}

/// Plaintext peer_map an attacker would drop in: it maps a Discord account
/// they control onto an OSL handle the victim trusts.
const SUBSTITUTED_PLAINTEXT: &str = r#"{
  "900000000000000009": {
    "osl_user_id": "liam",
    "discord_id": "900000000000000009",
    "outgoing_whitelists": [],
    "burned_scopes": []
  }
}"#;

#[test]
fn substituted_plaintext_peer_map_is_rejected_and_not_resealed() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let base = TempDir::new().unwrap();
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));
    set_file_storage_key(None);

    let dir = TempDir::new().unwrap();
    // Enrolment: writes the marker and installs the file storage key, exactly
    // as a user who has set a main password and unlocked.
    set_main_password(dir.path(), "password-one").expect("enrol");

    let path = dir.path().join("peer_map.json");
    write_peer_map(&path, &honest_map()).expect("write honest peer map");
    assert!(
        has_enc_magic(&fs::read(&path).unwrap()),
        "enrolled install must seal peer_map.json"
    );

    // Attacker with write access replaces the trust root with plaintext.
    fs::write(&path, SUBSTITUTED_PLAINTEXT.as_bytes()).unwrap();

    let result = load_peer_map_from_path(&path);
    assert!(
        result.is_err(),
        "an enrolled install must refuse a magic-less peer_map.json; \
         instead it loaded {:?}",
        result.map(|m| m.keys().cloned().collect::<Vec<_>>())
    );

    // The laundering half: the attacker's bytes must NOT have been re-sealed
    // under the victim's key, which would make them indistinguishable from
    // state the app itself wrote.
    let after = fs::read(&path).unwrap();
    assert!(
        !has_enc_magic(&after),
        "substituted plaintext was laundered into an OSL-ENC1 file under the victim's key"
    );
    assert_eq!(
        after,
        SUBSTITUTED_PLAINTEXT.as_bytes(),
        "the rejected file must be left exactly as found, for forensics"
    );

    // And the rogue mapping must never have become reachable.
    assert!(
        !String::from_utf8_lossy(&after).is_empty(),
        "sanity: fixture is non-empty"
    );
    assert!(
        load_peer_map_from_path(&path).is_err(),
        "rejection must be stable across reloads, not a one-shot"
    );
    assert!(
        SUBSTITUTED_PLAINTEXT.contains(ROGUE_DID),
        "sanity: fixture carries the rogue id"
    );

    set_file_storage_key(None);
    keystore::set_base_dir_override(None);
}

/// The legitimate migration path must survive: a user who never enrolled has
/// genuinely plaintext state files, and a key arriving mid-session (the
/// first-ever `set_file_storage_key`) has to adopt and seal them.
#[test]
fn never_enrolled_plaintext_peer_map_still_migrates() {
    let _serial = PROCESS_GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
    let base = TempDir::new().unwrap();
    keystore::set_base_dir_override(Some(base.path().to_path_buf()));

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("peer_map.json");
    fs::write(&path, serde_json::to_vec_pretty(&honest_map()).unwrap()).unwrap();
    assert!(!has_enc_magic(&fs::read(&path).unwrap()));
    assert!(
        !dir.path().join("password_marker.json").exists(),
        "this fixture is the NEVER-enrolled case"
    );

    set_file_storage_key(Some([0x71; 32]));
    let loaded = load_peer_map_from_path(&path).expect("plaintext peer map must still migrate");
    assert_eq!(loaded, honest_map());
    assert!(
        has_enc_magic(&fs::read(&path).unwrap()),
        "migration must seal the file under the newly installed key"
    );

    set_file_storage_key(None);
    keystore::set_base_dir_override(None);
}
