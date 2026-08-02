//! OSL-owned transport boundaries.
//!
//! Tor routing is implemented as a supervised Arti SOCKS5 subprocess rather
//! than an embedded `arti-client`. See [`tor`] for the fail-closed store
//! client boundary.

pub mod tor;
