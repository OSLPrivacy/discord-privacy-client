//! Discord webpack-selector resilience layer.
//!
//! Spec: `docs/design/key-server-api.md` § "Selector resilience".
//!
//! The Discord client is a closed-source webpack bundle that
//! re-mangles its module names roughly weekly. Our content-encryption
//! hooks rely on accurate selectors for things like
//! `MessageContent`, `MessageTextarea`, etc. — when Discord ships a
//! breaking change we have to ship a manifest update fast.
//!
//! Rather than ship binary updates each time, the manifest of
//! current selectors lives at the key server and is fetched on
//! launch and hourly. To keep that channel honest, the manifest is
//! Ed25519-signed by the release-signing key (a trusted constant
//! baked into the client at build time).
//!
//! ## Modules
//!
//! - [`manifest`] — schema, canonical bytes, sign / verify, 24-hour
//!   staleness check.
//! - [`fetcher`] — primary + CDN-mirror fallback state machine; the
//!   transport layer is parameterised over a [`fetcher::ManifestSource`]
//!   trait so tests don't need a real HTTP server. The integration
//!   layer wires a `KeyServerClient`-backed source for the primary
//!   and a CDN-backed source for the mirror.
//!
//! ## Fail-closed semantics
//!
//! On any of:
//! - manifest fetch failure across **both** primary and CDN, OR
//! - signature verification failure on any source, OR
//! - manifest age > 24 h relative to the active clock,
//!
//! [`fetcher::ManifestState`] becomes [`fetcher::ManifestState::FailClosed`]
//! and encryption is disabled by the integration layer (banner:
//! "Discord update detected — please update Discord Privacy Client
//! to vN.M.O").

pub mod fetcher;
pub mod manifest;

pub use fetcher::{
    FetchError, ManifestFetcher, ManifestSource, ManifestState, SourceLabel,
};
pub use manifest::{
    canonical_manifest_bytes, parse_signed_manifest, sign_manifest,
    verify_manifest, ManifestError, SelectorManifest, SignedManifest,
    MANIFEST_DOMAIN, MAX_MANIFEST_AGE_SECONDS,
};
