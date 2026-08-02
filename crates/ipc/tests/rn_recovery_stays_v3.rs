//! Recovery controls must remain usable when an OSL-RN session is unavailable.
//!
//! `0x06` and `0x07` retain their historical v2 envelope; `0x05` and `0x0A`
//! use v3.  The shared requirement is that none is an OSL-RN (`0x10`) wire,
//! even after a peer has been pinned to RN and the runtime fuse is open.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::{cmd_osl_build_session_reset, cmd_osl_build_skdm_request};
use ipc::peer_map::PeerEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::wire_rn::{RnPeerPin, RN_WIRE_IN_ENABLED};
use ipc::wire_v2::{
    encrypt_v3, RecipientV3, MSG_TYPE_REVOCATION, MSG_TYPE_SENDER_KEY_DISTRIBUTION,
    MSG_TYPE_SESSION_RESET, MSG_TYPE_SKDM_REQUEST, WIRE_VERSION_V2, WIRE_VERSION_V3,
};
use keystore::generate_identity;

const PEER_DID: &str = "900000000000000019";

fn wire_bytes(wire: &str) -> Vec<u8> {
    STANDARD
        .decode(wire.strip_prefix("DPC0::").expect("OSL wire prefix"))
        .expect("base64 OSL wire")
}

fn assert_legacy_control_wire(wire: &str, version: u8, message_type: u8) {
    let bytes = wire_bytes(wire);
    assert_eq!(bytes.first(), Some(&version), "control wire version");
    assert_eq!(bytes.get(1), Some(&message_type), "control message type");
    assert_ne!(bytes.first(), Some(&0x10), "control must not ride OSL-RN");
}

#[test]
fn recovery_and_group_controls_do_not_ride_a_pinned_open_rn_session() {
    assert!(RN_WIRE_IN_ENABLED, "RN compile-time fuse must be open");

    let state = AppState::new();
    state.set_rn_wire_in_enabled(true);
    let local = generate_identity("t19-c5-local".to_owned());
    let peer = generate_identity("t19-c5-peer".to_owned());
    state.install_identity(local.clone());
    state.peer_map.lock().expect("peer map").insert(
        PEER_DID.to_owned(),
        PeerEntry {
            discord_id: Some(PEER_DID.to_owned()),
            pubkey: Some(STANDARD.encode(peer.x25519_public.as_bytes())),
            ..PeerEntry::default()
        },
    );

    let mut pin = RnPeerPin::UNKNOWN;
    pin.raise_to_rn();
    assert!(pin.is_pinned_to_rn(), "fixture peer must be pinned to RN");

    let skdm_request = cmd_osl_build_skdm_request(
        &state,
        ScopeInput::from(&Scope::gc("t19-c5-group")),
        PEER_DID.to_owned(),
    )
    .expect("build SKDM request");
    assert_legacy_control_wire(&skdm_request, WIRE_VERSION_V2, MSG_TYPE_SKDM_REQUEST);

    let session_reset =
        cmd_osl_build_session_reset(&state, PEER_DID.to_owned()).expect("build session reset");
    assert_legacy_control_wire(&session_reset, WIRE_VERSION_V2, MSG_TYPE_SESSION_RESET);

    let recipient = RecipientV3 {
        x25519_pub: peer.x25519_public,
        mlkem_pub: peer.mlkem_encapsulation_key(),
    };
    let skdm = encrypt_v3(
        &local.x25519_secret,
        &local.x25519_public,
        std::slice::from_ref(&recipient),
        MSG_TYPE_SENDER_KEY_DISTRIBUTION,
        b"sender-key distribution",
    )
    .expect("build SKDM distribution");
    assert_legacy_control_wire(&skdm, WIRE_VERSION_V3, MSG_TYPE_SENDER_KEY_DISTRIBUTION);

    let revocation = encrypt_v3(
        &local.x25519_secret,
        &local.x25519_public,
        &[recipient],
        MSG_TYPE_REVOCATION,
        b"revocation notice",
    )
    .expect("build revocation");
    assert_legacy_control_wire(&revocation, WIRE_VERSION_V3, MSG_TYPE_REVOCATION);
}
