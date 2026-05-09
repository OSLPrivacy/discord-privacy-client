//! Manifest fetch state machine: primary key server, then CDN
//! mirror, then fail-closed.
//!
//! Per `docs/design/key-server-api.md` § "Manifest mirror" — fetch
//! order is:
//!
//!   1. Primary key server.
//!   2. CDN mirror on primary failure.
//!   3. Fail-closed if both unreachable or signatures invalid.
//!
//! ## Why caller-driven, not async
//!
//! Same reason as `runtime::revalidation`: tests pass a deterministic
//! sequence; the integration layer wires the loop to a tokio
//! interval. No tokio dependency in this crate.

use crate::manifest::{
    parse_signed_manifest, verify_manifest, ManifestError, SelectorManifest,
    SignedManifest,
};
use thiserror::Error;

/// Where a manifest came from. Useful in logs / banner copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLabel {
    Primary,
    CdnMirror,
}

/// Errors a [`ManifestSource`] can return. The fetcher treats all
/// transport / parse / verify errors uniformly: try the next source,
/// and if there isn't one, fail closed.
#[derive(Debug, Error)]
pub enum FetchError {
    #[error("manifest source transport error: {0}")]
    Transport(String),

    #[error("manifest source returned HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("manifest envelope parse: {0}")]
    Parse(#[from] serde_json::Error),

    #[error("manifest signature / staleness / shape: {0}")]
    Manifest(#[from] ManifestError),
}

/// Strategy by which a single source produces signed-manifest JSON
/// bytes. Implementations are typically thin wrappers over an HTTP
/// client; [`MockSource`] (test-only) lets tests script outcomes.
pub trait ManifestSource: Send + Sync {
    fn label(&self) -> SourceLabel;
    /// Return the raw JSON envelope bytes. Caller parses + verifies.
    fn fetch(&self) -> Result<Vec<u8>, FetchError>;
}

/// What the integration layer sees for "current encryption posture".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestState {
    /// No fetch has run yet — encryption disabled, banner suppressed
    /// (we haven't *failed* yet, we're just not ready).
    NotYetFetched,
    /// A valid manifest is loaded. Encryption may proceed against
    /// the contained selectors.
    Loaded {
        manifest: SelectorManifest,
        from: SourceLabel,
    },
    /// Both sources failed (transport, parse, or verify) **OR** the
    /// most recently loaded manifest is now > 24 h old. Encryption
    /// disabled; banner: "Discord update detected — please update
    /// Discord Privacy Client".
    FailClosed { reason: String },
}

impl ManifestState {
    pub fn is_loaded(&self) -> bool {
        matches!(self, ManifestState::Loaded { .. })
    }
    pub fn is_fail_closed(&self) -> bool {
        matches!(self, ManifestState::FailClosed { .. })
    }
    pub fn manifest(&self) -> Option<&SelectorManifest> {
        match self {
            ManifestState::Loaded { manifest, .. } => Some(manifest),
            _ => None,
        }
    }
}

/// Combines primary + CDN mirror sources. The `trusted_pub_b64` is
/// the release-signing key the client trusts; signed manifests must
/// declare a matching `signing_key_b64` to be accepted.
pub struct ManifestFetcher {
    primary: Box<dyn ManifestSource>,
    cdn: Box<dyn ManifestSource>,
    trusted_pub_b64: String,
    state: ManifestState,
}

impl ManifestFetcher {
    pub fn new(
        primary: Box<dyn ManifestSource>,
        cdn: Box<dyn ManifestSource>,
        trusted_pub_b64: impl Into<String>,
    ) -> Self {
        ManifestFetcher {
            primary,
            cdn,
            trusted_pub_b64: trusted_pub_b64.into(),
            state: ManifestState::NotYetFetched,
        }
    }

    pub fn state(&self) -> &ManifestState {
        &self.state
    }

    pub fn manifest(&self) -> Option<&SelectorManifest> {
        self.state.manifest()
    }

    /// Try primary; on any failure, try CDN; on both failures,
    /// transition to [`ManifestState::FailClosed`].
    ///
    /// `now_unix_seconds` is the caller's clock — used for the 24-h
    /// staleness check inside [`verify_manifest`].
    pub fn refresh(&mut self, now_unix_seconds: u64) -> &ManifestState {
        let primary_err =
            match try_source(&*self.primary, &self.trusted_pub_b64, now_unix_seconds) {
                Ok(manifest) => {
                    self.state = ManifestState::Loaded {
                        manifest,
                        from: SourceLabel::Primary,
                    };
                    return &self.state;
                }
                Err(e) => format!("primary: {e}"),
            };
        let cdn_err =
            match try_source(&*self.cdn, &self.trusted_pub_b64, now_unix_seconds) {
                Ok(manifest) => {
                    self.state = ManifestState::Loaded {
                        manifest,
                        from: SourceLabel::CdnMirror,
                    };
                    return &self.state;
                }
                Err(e) => format!("cdn: {e}"),
            };
        self.state = ManifestState::FailClosed {
            reason: format!("{primary_err}; {cdn_err}"),
        };
        &self.state
    }

    /// Re-evaluate freshness without fetching. The caller invokes
    /// this when the wall clock advances; if the in-memory manifest
    /// is now stale (> 24 h since `issued_at`), transitions to
    /// `FailClosed` so the integration layer can flip the banner
    /// without waiting for the next refresh tick.
    pub fn reconsider_staleness(&mut self, now_unix_seconds: u64) -> &ManifestState {
        if let ManifestState::Loaded { manifest, .. } = &self.state {
            let age = now_unix_seconds
                .saturating_sub(manifest.issued_at_unix_seconds);
            if age > crate::manifest::MAX_MANIFEST_AGE_SECONDS {
                self.state = ManifestState::FailClosed {
                    reason: format!(
                        "loaded manifest is now {age}s old (max {})",
                        crate::manifest::MAX_MANIFEST_AGE_SECONDS
                    ),
                };
            }
        }
        &self.state
    }
}

fn try_source(
    src: &dyn ManifestSource,
    trusted_pub_b64: &str,
    now_unix_seconds: u64,
) -> Result<SelectorManifest, FetchError> {
    let bytes = src.fetch()?;
    let signed: SignedManifest = parse_signed_manifest(&bytes)?;
    let m = verify_manifest(&signed, trusted_pub_b64, now_unix_seconds)?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{canonical_manifest_bytes, sign_manifest};
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use crypto::ed25519;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    struct MockSource {
        label: SourceLabel,
        responses: Mutex<Vec<Result<Vec<u8>, FetchError>>>,
    }

    impl MockSource {
        fn new(label: SourceLabel, responses: Vec<Result<Vec<u8>, FetchError>>) -> Self {
            MockSource {
                label,
                responses: Mutex::new(responses),
            }
        }
    }

    impl ManifestSource for MockSource {
        fn label(&self) -> SourceLabel {
            self.label
        }
        fn fetch(&self) -> Result<Vec<u8>, FetchError> {
            let mut g = self.responses.lock().unwrap();
            if g.is_empty() {
                return Err(FetchError::Transport("queue exhausted".into()));
            }
            // Replace Err with a fresh Err each pop since FetchError
            // isn't Clone in general; instead pop the front.
            g.remove(0)
        }
    }

    fn make_signed(now: u64) -> (Vec<u8>, String) {
        let (s, p) = ed25519::generate_keypair();
        let pub_b64 = STANDARD.encode(p.as_bytes());
        let mut sel = BTreeMap::new();
        sel.insert("MessageContent".to_string(), "abcd".to_string());
        let m = SelectorManifest {
            version: 1,
            issued_at_unix_seconds: now,
            client_min_version: "0.1.0".to_string(),
            selectors: sel,
        };
        let signed = sign_manifest(&s, &p, &m);
        let json = serde_json::to_vec(&signed).unwrap();
        (json, pub_b64)
    }

    /// Sign a manifest with a *different* key than the one we publish
    /// as trusted, so the fetcher rejects it.
    fn make_signed_wrong_key(
        trusted_pub_b64: &str,
        now: u64,
    ) -> (Vec<u8>, String) {
        let (s, p) = ed25519::generate_keypair();
        let mut sel = BTreeMap::new();
        sel.insert("MessageContent".to_string(), "abcd".to_string());
        let m = SelectorManifest {
            version: 1,
            issued_at_unix_seconds: now,
            client_min_version: "0.1.0".to_string(),
            selectors: sel,
        };
        let mut signed = sign_manifest(&s, &p, &m);
        // Force the signing key in the envelope to match what the
        // verifier trusts, but the actual signature was made with
        // (s, p). Verifier will detect and reject.
        signed.signing_key_b64 = trusted_pub_b64.to_string();
        let json = serde_json::to_vec(&signed).unwrap();
        let _ = canonical_manifest_bytes; // keep import alive
        (json, trusted_pub_b64.to_string())
    }

    #[test]
    fn primary_success_loads_manifest() {
        let (json, pub_b64) = make_signed(1_700_000_000);
        let primary = MockSource::new(SourceLabel::Primary, vec![Ok(json)]);
        let cdn = MockSource::new(
            SourceLabel::CdnMirror,
            vec![Err(FetchError::Transport("never reached".into()))],
        );
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        let s = f.refresh(1_700_000_000);
        match s {
            ManifestState::Loaded { from, .. } => assert_eq!(*from, SourceLabel::Primary),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn primary_failure_falls_back_to_cdn() {
        let (json, pub_b64) = make_signed(1_700_000_000);
        let primary = MockSource::new(
            SourceLabel::Primary,
            vec![Err(FetchError::Transport("DNS fail".into()))],
        );
        let cdn = MockSource::new(SourceLabel::CdnMirror, vec![Ok(json)]);
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        let s = f.refresh(1_700_000_000);
        match s {
            ManifestState::Loaded { from, .. } => assert_eq!(*from, SourceLabel::CdnMirror),
            other => panic!("expected Loaded, got {other:?}"),
        }
    }

    #[test]
    fn both_sources_fail_transitions_to_fail_closed() {
        let primary = MockSource::new(
            SourceLabel::Primary,
            vec![Err(FetchError::Transport("primary down".into()))],
        );
        let cdn = MockSource::new(
            SourceLabel::CdnMirror,
            vec![Err(FetchError::HttpStatus {
                status: 503,
                body: "cdn down".into(),
            })],
        );
        let mut f = ManifestFetcher::new(
            Box::new(primary),
            Box::new(cdn),
            "trusted_pub".to_string(),
        );
        let s = f.refresh(1_700_000_000);
        match s {
            ManifestState::FailClosed { reason } => {
                assert!(reason.contains("primary"));
                assert!(reason.contains("cdn"));
            }
            other => panic!("expected FailClosed, got {other:?}"),
        }
    }

    #[test]
    fn primary_signature_failure_falls_back_to_cdn() {
        let (good_json, pub_b64) = make_signed(1_700_000_000);
        let (bad_json, _) = make_signed_wrong_key(&pub_b64, 1_700_000_000);
        let primary = MockSource::new(SourceLabel::Primary, vec![Ok(bad_json)]);
        let cdn = MockSource::new(SourceLabel::CdnMirror, vec![Ok(good_json)]);
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        let s = f.refresh(1_700_000_000);
        match s {
            ManifestState::Loaded { from, .. } => assert_eq!(*from, SourceLabel::CdnMirror),
            other => panic!("expected Loaded from CDN, got {other:?}"),
        }
    }

    #[test]
    fn signature_failure_on_both_sources_fails_closed() {
        let pub_b64 = STANDARD.encode([0u8; 32]); // dummy; both bodies will declare it
        let (a, _) = make_signed_wrong_key(&pub_b64, 1_700_000_000);
        let (b, _) = make_signed_wrong_key(&pub_b64, 1_700_000_000);
        let primary = MockSource::new(SourceLabel::Primary, vec![Ok(a)]);
        let cdn = MockSource::new(SourceLabel::CdnMirror, vec![Ok(b)]);
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        let s = f.refresh(1_700_000_000);
        assert!(s.is_fail_closed());
    }

    #[test]
    fn stale_manifest_on_primary_fails_closed_if_cdn_also_stale() {
        // Sign a manifest 25h ago.
        let issued = 1_700_000_000;
        let (json_a, pub_b64) = make_signed(issued);
        let (json_b, _) = make_signed(issued);
        let primary = MockSource::new(SourceLabel::Primary, vec![Ok(json_a)]);
        let cdn = MockSource::new(SourceLabel::CdnMirror, vec![Ok(json_b)]);
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        let now = issued + 25 * 60 * 60;
        let s = f.refresh(now);
        assert!(s.is_fail_closed(), "expected FailClosed for >24h, got {s:?}");
    }

    #[test]
    fn reconsider_staleness_flips_loaded_to_fail_closed() {
        let issued = 1_700_000_000;
        let (json, pub_b64) = make_signed(issued);
        let primary = MockSource::new(SourceLabel::Primary, vec![Ok(json)]);
        let cdn = MockSource::new(
            SourceLabel::CdnMirror,
            vec![Err(FetchError::Transport("never reached".into()))],
        );
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        f.refresh(issued);
        assert!(f.state().is_loaded());
        // Without re-fetching, just advance the clock past 24h.
        f.reconsider_staleness(issued + 25 * 60 * 60);
        assert!(f.state().is_fail_closed());
    }

    #[test]
    fn reconsider_staleness_within_window_keeps_loaded() {
        let issued = 1_700_000_000;
        let (json, pub_b64) = make_signed(issued);
        let primary = MockSource::new(SourceLabel::Primary, vec![Ok(json)]);
        let cdn = MockSource::new(
            SourceLabel::CdnMirror,
            vec![Err(FetchError::Transport("never reached".into()))],
        );
        let mut f = ManifestFetcher::new(Box::new(primary), Box::new(cdn), pub_b64);
        f.refresh(issued);
        f.reconsider_staleness(issued + 12 * 60 * 60);
        assert!(f.state().is_loaded());
    }

    #[test]
    fn initial_state_is_not_yet_fetched() {
        let primary = MockSource::new(SourceLabel::Primary, vec![]);
        let cdn = MockSource::new(SourceLabel::CdnMirror, vec![]);
        let f = ManifestFetcher::new(
            Box::new(primary),
            Box::new(cdn),
            "trusted".to_string(),
        );
        assert!(matches!(f.state(), ManifestState::NotYetFetched));
        assert!(f.manifest().is_none());
    }
}
