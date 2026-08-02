#![cfg(feature = "core")]

//! T16-E2: text encryption is the product floor, never a paid capability.
//!
//! Each case starts with both peers genuinely starved of a paid entitlement:
//! no cache at all, a sealed REVOKED cache, or a sealed EXPIRED cache.  The
//! test drives the public send and receive commands, rather than a crypto
//! primitive, so a future entitlement check anywhere in the text path fails it.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message_v2};
use ipc::license_lifecycle::launch_classify;
use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::whitelist_state::ScopeState;
use keystore::{generate_identity, LicenseCacheInner, LicenseState};
use tempfile::TempDir;

const SENDER_DID: &str = "900000000000000001";
const RECIPIENT_DID: &str = "900000000000000002";
const PLAINTEXT: &str = "encryption stays available without Pro";

#[derive(Clone, Copy)]
enum StarvedEntitlement {
    NoCache,
    Revoked,
    Expired,
}

impl StarvedEntitlement {
    fn expected_status(self) -> &'static str {
        match self {
            Self::NoCache => "Unconfigured",
            Self::Revoked => "REVOKED",
            Self::Expired => "EXPIRED",
        }
    }
}

fn state_with_identity(name: &str) -> AppState {
    let state = AppState::new();
    state.install_identity(generate_identity(name.to_owned()));
    state
}

fn install_peer(sender: &AppState, recipient: &AppState, recipient_did: &str) {
    let identity = recipient.identity.lock().unwrap();
    let identity = identity.as_ref().expect("recipient identity installed");
    let mut peers = sender.peer_map.lock().unwrap();
    let peer = peers.entry(recipient_did.to_owned()).or_default();
    peer.discord_id = Some(recipient_did.to_owned());
    peer.pubkey = Some(STANDARD.encode(identity.x25519_public.as_bytes()));
    peer.ik_mlkem768_pub = Some(STANDARD.encode(identity.mlkem_public_bytes));
}

fn enable_dm_text_encryption(state: &AppState, recipient_did: &str) {
    let scope = Scope::dm(recipient_did);
    state.whitelist_state.lock().unwrap().insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            ..ScopeState::default()
        },
    );
    state
        .peer_map
        .lock()
        .unwrap()
        .entry(recipient_did.to_owned())
        .or_default()
        .outgoing_whitelists
        .push(WhitelistEntry::Dm {
            broadened: false,
            enabled_at: None,
        });
}

fn starve_entitlement(state: &AppState, dir: &TempDir, entitlement: StarvedEntitlement) {
    let cache_path = dir.path().join("license.json");
    let status = match entitlement {
        StarvedEntitlement::NoCache => "ACTIVE",
        StarvedEntitlement::Revoked | StarvedEntitlement::Expired => entitlement.expected_status(),
    };
    let cache = LicenseCacheInner {
        license_plaintext: "OSL-TEST-STARVED".to_owned(),
        last_validated_status: status.to_owned(),
        redeemed_at: Some(1_700_000_000),
        expires_at: None,
        current_period_end: None,
        last_validated_at: 1_700_000_000,
        checksum_ok: true,
    };
    let sealer = keystore::select_best_sealer();
    keystore::save_license_cache(&cache_path, &cache, sealer.as_ref())
        .expect("seal entitlement cache before starvation");
    if matches!(entitlement, StarvedEntitlement::NoCache) {
        std::fs::remove_file(&cache_path).expect("delete license.json before text send");
    }

    launch_classify(state, dir.path());
    let license = state.license_state.lock().unwrap();
    assert_eq!(license.state, LicenseState::Free);
    assert_eq!(license.raw_status, entitlement.expected_status());
    assert_eq!(
        cache_path.exists(),
        !matches!(entitlement, StarvedEntitlement::NoCache),
        "the no-cache case must not accidentally use a stubbed entitlement"
    );
}

#[test]
fn text_seal_send_receive_and_decrypt_work_without_an_entitlement() {
    for entitlement in [
        StarvedEntitlement::NoCache,
        StarvedEntitlement::Revoked,
        StarvedEntitlement::Expired,
    ] {
        let sender = state_with_identity("starved-sender");
        let recipient = state_with_identity("starved-recipient");
        let sender_dir = tempfile::tempdir().expect("sender entitlement directory");
        let recipient_dir = tempfile::tempdir().expect("recipient entitlement directory");

        starve_entitlement(&sender, &sender_dir, entitlement);
        starve_entitlement(&recipient, &recipient_dir, entitlement);
        install_peer(&sender, &recipient, RECIPIENT_DID);
        install_peer(&recipient, &sender, SENDER_DID);
        enable_dm_text_encryption(&sender, RECIPIENT_DID);

        let scope = Scope::dm(RECIPIENT_DID);
        let sealed = cmd_osl_encrypt_message_v2(
            &sender,
            PLAINTEXT.to_owned(),
            ScopeInput::from(&scope),
            vec![RECIPIENT_DID.to_owned()],
            SENDER_DID.to_owned(),
        )
        .expect("text seal/send must work without Pro");
        assert_eq!(sealed.messages.len(), 1, "DM send emits one text carrier");

        let plaintext = cmd_osl_decrypt_message_v2(
            &recipient,
            None,
            "starved-entitlement-channel".to_owned(),
            SENDER_DID.to_owned(),
            sealed.messages.into_iter().next().unwrap(),
            Some(ScopeInput::from(&scope)),
            None,
        )
        .expect("text receive/decrypt must work without Pro");
        assert_eq!(plaintext, PLAINTEXT);
    }
}
