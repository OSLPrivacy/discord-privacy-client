//! Authenticated attribution for incoming OSL-RN (`0x10`) wires.
//!
//! OSL-RN encrypts its sender and recipient header fields, so the legacy
//! `inspect_v3_wire` gate cannot classify one of these wires.  The persisted
//! session selected by the already-verified peer binding is instead the
//! authentication boundary: a wire is attributed only after that exact
//! session decrypts it successfully.
//!
//! This intentionally has no "try another peer" branch.  Doing so would turn
//! the peer list into an authentication oracle and could attribute a wire to a
//! conversation other than the one that received it.  Outbound transcript rows
//! are attributed at send time by the plaintext cache; a pairwise ratchet
//! cannot open its own sent ciphertext.

use ipc::wire_rn::{receive_rn_with_sealer, RnSessionStore};

/// The only direction a decryptable remote RN wire can have.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RnWireDirection {
    /// The session bound to the verified peer opened a wire from that peer.
    PeerToSelf,
}

/// The authenticated facts recovered from an incoming OSL-RN wire.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedRnWire {
    pub direction: RnWireDirection,
    pub msg_type: u8,
    pub plaintext: Vec<u8>,
}

/// Deliberately opaque: callers must not distinguish a wrong peer session,
/// malformed ciphertext, or unavailable persisted state to an untrusted row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RnAttributionRefused;

/// Authenticate and attribute one incoming RN wire to one verified peer.
///
/// `peer_identity_x25519` must come from the conversation's verified binding;
/// this function never reads sender data from the wire or searches other
/// sessions.  Therefore a successful decrypt both authenticates the peer and
/// proves the inbound direction.
pub fn authenticate_incoming_rn(
    store: &RnSessionStore,
    sealer: &dyn keystore::sealer::Sealer,
    peer_identity_x25519: &[u8; 32],
    wire: &str,
) -> Result<AuthenticatedRnWire, RnAttributionRefused> {
    let opened = receive_rn_with_sealer(store, sealer, peer_identity_x25519, wire)
        .map_err(|_| RnAttributionRefused)?;
    Ok(AuthenticatedRnWire {
        direction: RnWireDirection::PeerToSelf,
        msg_type: opened.msg_type,
        plaintext: opened.plaintext,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use keystore::sealer::MemorySealer;
    use osl_ratchet_next::test_support::established_pair;
    use osl_ratchet_next::SecureSession;
    use tempfile::TempDir;

    #[test]
    fn t19_t31_wrong_peer_session_refuses_but_bound_peer_authenticates_direction() {
        let root = TempDir::new().expect("temporary RN session root");
        let store = RnSessionStore::new(root.path());
        let sealer = MemorySealer::new();
        let (mut sender, right_peer_session, mut rng) = established_pair(31);
        let (_, wrong_peer_session, _) = established_pair(32);
        let right_peer = [0x11; 32];
        let wrong_peer = [0x22; 32];
        store
            .save_session_with_sealer(&right_peer, &right_peer_session, &sealer)
            .expect("persist right peer session");
        store
            .save_session_with_sealer(&wrong_peer, &wrong_peer_session, &sealer)
            .expect("persist wrong peer session");
        let wire = sender
            .encrypt(0x44, b"authenticated inbound content", &mut rng)
            .expect("encrypt RN wire");

        assert_eq!(
            authenticate_incoming_rn(&store, &sealer, &wrong_peer, &wire),
            Err(RnAttributionRefused),
            "a wire must not be attributed merely because some other peer session exists"
        );

        let attributed = authenticate_incoming_rn(&store, &sealer, &right_peer, &wire)
            .expect("the verified peer session must authenticate its own wire");
        assert_eq!(attributed.direction, RnWireDirection::PeerToSelf);
        assert_eq!(attributed.msg_type, 0x44);
        assert_eq!(attributed.plaintext, b"authenticated inbound content");
    }
}
