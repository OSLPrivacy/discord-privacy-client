//! Scaffolding placeholder. **No transport implementation in v1 alpha.**
//!
//! Originally scoped to host:
//! - Mullvad WireGuard tunnel (via `boringtun` + Wintun TUN driver)
//! - arti-Tor client for key-server `.onion` traffic
//!
//! Both are **deferred to v2.2** because their pinned release-candidate
//! `x25519-dalek` / `curve25519-dalek` versions conflict with `dryoc`'s
//! curve25519-dalek requirement and with each other:
//!
//! | Dependency       | Pin                                       |
//! | ---              | ---                                        |
//! | boringtun 0.6    | `x25519-dalek =2.0.0-rc.3`                 |
//! | x25519-dalek rc  | `curve25519-dalek =4.0.0-rc.3`             |
//! | arti-client 0.21 | `x25519-dalek ^2.0.0` (incompatible w/ rc) |
//! | dryoc 0.7        | `curve25519-dalek ^4.1.3`                  |
//!
//! Resolving requires either picking newer compatible versions, or
//! isolating the WireGuard / Tor stack into a separate mini-workspace
//! with a self-contained crypto build. See `CHANGELOG.md` for the
//! diff and `docs/THREAT_MODEL.md` "Network-layer protection
//! (v1 alpha vs v2.2)" for the user-facing impact.
//!
//! ## v2.2 expected scope (when this crate is implemented)
//!
//! - Mullvad WireGuard config retrieval and tunnel bring-up scoped
//!   to app traffic.
//! - WireGuard kill-switch enforcement: block app network requests
//!   and surface "VPN disconnected" overlay if the tunnel drops.
//! - arti-Tor client embedded for key-server `.onion` connectivity.
//! - Tor bootstrap on launch with status indicator.
//! - Combined VPN-then-Tor routing for key-server traffic; direct
//!   VPN-only for Discord traffic (Discord blocks Tor exits).
//!
//! ## v1 alpha guidance to users
//!
//! v1 alpha does not bundle a VPN or Tor. Users run Mullvad's
//! official app (or another trustworthy VPN) externally on their
//! machine. The app cannot detect external-VPN status; this is
//! honest disclosure, not a silent fallback.
