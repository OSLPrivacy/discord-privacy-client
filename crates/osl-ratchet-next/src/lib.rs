//! `osl-ratchet-next` — a research messaging-encryption protocol.
//!
//! # Status: UNREVIEWED. NOT WIRED IN. DO NOT CARRY REAL TRAFFIC.
//!
//! This crate is reachable only from its own tests. No other crate or
//! application in this workspace depends on it, and none should until
//! it has had an **external cryptographic review**. It exists to
//! answer the question "what would a genuinely better-than-shipped-
//! Signal ratchet look like on a lossy, low-bandwidth, reorderable
//! carrier?", with the arguments written down and the claims
//! separated from the assertions.
//!
//! Read `DESIGN.md` for the protocol, the state machine and the claims
//! table; `THREAT-MODEL.md` for what it does *not* defend against; and
//! `MIGRATION.md` for what adopting it would cost.
//!
//! # What it is
//!
//! A pairwise, header-encrypted Double Ratchet whose root chain is
//! **hybrid**: every root step mixes an X25519 output, and periodically
//! also an ML-KEM-768 shared secret established by a fragmented,
//! loss-tolerant sub-protocol that never blocks message delivery.
//!
//! ```text
//!            +--------------- classical DH ratchet ---------------+
//!   root --> step --> step --> step --> step --> step --> step --> ...
//!            ^                          ^
//!            | ss_pq[1]                 | ss_pq[2]
//!            +-- PQ epoch ratchet: ML-KEM ciphertexts fragmented
//!                across many messages, retransmitted until acked,
//!                folded in at an explicitly announced step.
//! ```
//!
//! # Module map
//!
//! | Module | Contains |
//! | --- | --- |
//! | [`primitives`] | thin wrappers over `x25519-dalek`, `ml-kem`, `hkdf`, `chacha20poly1305` |
//! | [`kdf`] | every HKDF call in the protocol, in one place |
//! | [`handshake`] | PQXDH-shaped async handshake |
//! | [`pq`] | the PQ epoch ratchet and its fragment transport |
//! | [`skipped`] | bounded skipped-message-key store |
//! | [`session`] | the ratchet, the OSL-RN wire format, state export |
//! | [`api`] | the transport-agnostic [`api::SecureSession`] boundary |
//! | [`codec`] | panic-free varint/binary parsing |
//! | [`test_support`] | seeded two-party harness |

#![forbid(unsafe_code)]
#![deny(clippy::indexing_slicing)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::panic)]

pub mod api;
pub mod codec;
pub mod error;
pub mod handshake;
pub mod kdf;
pub mod negotiate;
pub mod pq;
pub mod primitives;
pub mod session;
pub mod skipped;
pub mod test_support;

pub use api::{
    accept_rn, accept_rn_bound, decrypt_rn, encrypt_rn, initiate_rn_bound, peek_wire_version,
    SecureSession,
};
pub use error::{Error, Result};
pub use handshake::{LocalPrekeys, PeerBundle};
pub use negotiate::Negotiation;
pub use pq::{PqParams, Role};
pub use primitives::{KemPublic, KemSecret, XPublic, XSecret, MLKEM_CT, MLKEM_DK, MLKEM_EK};
pub use session::{
    peek_bootstrap_initiator_identity, Opened, Session, SessionParams, WIRE_VERSION_RN,
};
pub use skipped::SkipParams;
