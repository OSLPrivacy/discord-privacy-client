//! T14-D5's OSL Chat attachment proof.
//!
//! The network E2E fixture was already present under a generic peer-attachment
//! name. Make it a named OSL Chat deliverable rather than duplicating its
//! loopback cipher-store, real broker calls, and byte-for-byte decryption
//! checks. It drives begin → deliver → list/take → decrypt → commit exactly as
//! the OSL Chat transport does; its assertions are on recovered bytes, never
//! on an attachment plan.

#![cfg(feature = "core")]

#[path = "peer_attachment_network_e2e.rs"]
mod peer_attachment_network_e2e;
