//! The swap-in boundary: a transport-agnostic secure-messaging session
//! interface, plus free functions shaped like the ones the app calls
//! today.
//!
//! # Why the trait looks like this
//!
//! Nothing here names Discord, a carrier, an overlay, a scope, a
//! whitelist or a hub. A session is: *start one from a peer's
//! published keys, encrypt a plaintext to an opaque wire string,
//! decrypt an opaque wire string back to a plaintext, and persist or
//! restore the state in between.* That is the whole surface.
//!
//! # How it lines up with the existing wire
//!
//! `ipc::wire_v2` exposes:
//!
//! ```ignore
//! pub fn encrypt_v3(sender_ik_sk, sender_ik_pub, recipients, msg_type, plaintext)
//!     -> Result<String, V2Error>;
//! pub fn decrypt_v3(wire, recipient_ik_sk, recipient_mlkem_sk)
//!     -> Result<DecryptedV2, V2Error>;
//! ```
//!
//! and this crate exposes:
//!
//! ```ignore
//! pub fn encrypt_rn(session: &mut Session, msg_type, plaintext) -> Result<String>;
//! pub fn decrypt_rn(session: &mut Session, wire)  -> Result<Opened>;
//! pub fn accept_rn(local: &LocalPrekeys, wire)    -> Result<(Session, Opened)>;
//! ```
//!
//! Same `DPC0::`-prefixed `String` in and out, same `msg_type: u8`,
//! same `Vec<u8>` plaintext, and [`Opened`] is field-compatible with
//! `DecryptedV2`.
//!
//! ## The two places it cannot mirror `wire_v2`, and why
//!
//! **1. A session replaces the key arguments.** `encrypt_v3` is a pure
//! function of the sender's static keys; `encrypt_rn` needs
//! `&mut Session`. This is not an API preference, it is what a ratchet
//! *is* — forward secrecy comes from state that advances and is
//! destroyed. Any ratchet adopted on the Discord path forces the
//! caller to load, mutate and persist per-peer state around every
//! send. `MIGRATION.md` treats this as the main integration cost.
//!
//! **2. There is no multi-recipient slot array.** `encrypt_v3` wraps
//! one body key to N recipients in one blob. A pairwise ratchet has no
//! such notion: N recipients means N sessions and N blobs, or a
//! separate sender-key layer (which `crypto::sender_keys` already is).
//! This crate deliberately solves the *pairwise* problem only.
//! Group/scope messaging is out of scope and is not claimed.

use crate::error::Result;
use crate::handshake::{LocalPrekeys, PeerBundle};
use crate::primitives::XSecret;
use crate::session::{Opened, Session, SessionParams};
use rand::rngs::OsRng;

/// A general secure-messaging session, free of any app or carrier
/// concept.
///
/// Not object-safe by design: [`SecureSession::import_state`] and the
/// constructors return `Self`. A future integrator that needs dynamic
/// dispatch should wrap this in an enum over wire versions — which is
/// what a v=3 -> OSL-RN migration wants anyway (see `MIGRATION.md`).
pub trait SecureSession: Sized {
    /// The peer's published, already-authenticated key material.
    type Bundle;
    /// The local side's secrets matching its own published bundle.
    type Prekeys;
    /// Tunables.
    type Params;
    /// Result of a successful decrypt.
    type Opened;

    /// Start a session towards a peer from its published bundle.
    fn initiate_session(
        local_identity: &XSecret,
        peer: &Self::Bundle,
        params: Self::Params,
    ) -> Result<Self>;

    /// Adopt a session from an incoming bootstrap message, returning
    /// the session and that message's plaintext.
    fn accept_session(
        local: &Self::Prekeys,
        wire: &str,
        params: Self::Params,
    ) -> Result<(Self, Self::Opened)>;

    /// Encrypt a plaintext to an opaque wire string.
    fn seal(&mut self, msg_type: u8, plaintext: &[u8]) -> Result<String>;

    /// Decrypt an opaque wire string.
    fn open(&mut self, wire: &str) -> Result<Self::Opened>;

    /// Serialize every byte of session state, secrets included.
    fn export(&self) -> Result<Vec<u8>>;

    /// Restore from [`SecureSession::export`].
    fn import(bytes: &[u8]) -> Result<Self>;
}

impl SecureSession for Session {
    type Bundle = PeerBundle;
    type Prekeys = LocalPrekeys;
    type Params = SessionParams;
    type Opened = Opened;

    fn initiate_session(
        local_identity: &XSecret,
        peer: &Self::Bundle,
        params: Self::Params,
    ) -> Result<Self> {
        Session::initiate(local_identity, peer, params, &mut OsRng)
    }

    fn accept_session(
        local: &Self::Prekeys,
        wire: &str,
        params: Self::Params,
    ) -> Result<(Self, Self::Opened)> {
        Session::accept(local, wire, params, &mut OsRng)
    }

    fn seal(&mut self, msg_type: u8, plaintext: &[u8]) -> Result<String> {
        self.encrypt(msg_type, plaintext, &mut OsRng)
    }

    fn open(&mut self, wire: &str) -> Result<Self::Opened> {
        self.decrypt(wire, &mut OsRng)
    }

    fn export(&self) -> Result<Vec<u8>> {
        self.export_state()
    }

    fn import(bytes: &[u8]) -> Result<Self> {
        Session::import_state(bytes)
    }
}

// ---------------------------------------------------------------
// Free functions shaped like `ipc::wire_v2`
// ---------------------------------------------------------------

/// Encode a OSL-RN wire blob. Mirrors `wire_v2::encrypt_v3`'s
/// `(.., msg_type, plaintext) -> Result<String, _>` shape.
pub fn encrypt_rn(session: &mut Session, msg_type: u8, plaintext: &[u8]) -> Result<String> {
    session.encrypt(msg_type, plaintext, &mut OsRng)
}

/// Decode a OSL-RN wire blob against an existing session. Mirrors
/// `wire_v2::decrypt_v3`'s `(wire, ..) -> Result<DecryptedV2, _>`.
pub fn decrypt_rn(session: &mut Session, wire: &str) -> Result<Opened> {
    session.decrypt(wire, &mut OsRng)
}

/// Decode a OSL-RN *bootstrap* blob with no prior session, mirroring
/// `wire_v2::decrypt_v4`'s "reconstruct the receiving state from the
/// wire" behaviour.
///
/// **Protocol-only:** no negotiated version is bound into `SK`. Use
/// [`accept_rn_bound`] in application code.
pub fn accept_rn(
    local: &LocalPrekeys,
    wire: &str,
    params: SessionParams,
) -> Result<(Session, Opened)> {
    Session::accept(local, wire, params, &mut OsRng)
}

/// Start a session with the negotiated version bound into `SK`, drawing
/// from `OsRng`.
///
/// Exists so integrators need no `rand` dependency of their own, and
/// more importantly so the choice of CSPRNG stays inside this crate
/// instead of being re-decided at every call site.
pub fn initiate_rn_bound(
    local_identity: &XSecret,
    peer: &PeerBundle,
    binding: &[u8; 32],
    params: SessionParams,
) -> Result<Session> {
    Session::initiate_bound(local_identity, peer, Some(binding), params, &mut OsRng)
}

/// Decode an OSL-RN *bootstrap* blob with the negotiated version bound
/// into `SK`. See [`crate::negotiate`].
pub fn accept_rn_bound(
    local: &LocalPrekeys,
    wire: &str,
    binding: &[u8; 32],
    params: SessionParams,
) -> Result<(Session, Opened)> {
    Session::accept_bound(local, wire, Some(binding), params, &mut OsRng)
}

/// Cheap version probe for a receiving router: returns the version
/// byte of a `DPC0::` blob without decrypting anything.
///
/// A migrating receiver calls this and dispatches to the v=2..v=5
/// decoders or to [`decrypt_rn`]. Because the version byte is the
/// first byte of the base64 payload — exactly as in `wire_v2` — no
/// existing decoder needs to change to make room for OSL-RN.
pub fn peek_wire_version(wire: &str) -> Option<u8> {
    use base64::Engine as _;
    let body = wire.strip_prefix(crate::session::WIRE_PREFIX)?;
    // Four base64 characters decode to the first three bytes; decoding
    // just the prefix avoids allocating for a large blob.
    let head: String = body.chars().take(4).collect();
    if head.len() < 4 {
        return None;
    }
    base64::engine::general_purpose::STANDARD
        .decode(head)
        .ok()?
        .first()
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::WIRE_VERSION_RN;
    use crate::test_support::Harness;

    #[test]
    fn trait_surface_roundtrips() {
        let mut h = Harness::new(90);
        let wire = h.alice_send(b"via trait").expect("send");
        let opened = h.bob_recv(&wire).expect("recv");
        assert_eq!(opened.plaintext, b"via trait");

        // Export/import through the trait keeps the session usable.
        let blob = SecureSession::export(&h.alice).expect("export");
        let mut restored =
            <crate::session::Session as SecureSession>::import(&blob).expect("import");
        let wire = restored
            .encrypt(0, b"after restore", &mut h.rng)
            .expect("send");
        assert_eq!(h.bob_recv(&wire).expect("recv").plaintext, b"after restore");
    }

    #[test]
    fn version_probe_reports_v6() {
        let mut h = Harness::new(91);
        let wire = h.alice_send(b"x").expect("send");
        assert_eq!(peek_wire_version(&wire), Some(WIRE_VERSION_RN));
        assert_eq!(peek_wire_version("not a wire"), None);
        assert_eq!(peek_wire_version("DPC0::"), None);
    }
}
