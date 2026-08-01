//! Launch and hourly refresh scheduling for signed selector manifests.
//!
//! The embedding client calls [`ManifestRefresh::boot`] once at launch and
//! [`ManifestRefresh::tick`] whenever its wall-clock timer fires.  This
//! module owns timing only; [`ManifestFetcher`] remains the sole authority on
//! whether a fetched manifest is trusted.

use crate::{ManifestFetcher, ManifestState};

/// Selector manifests are refreshed once per hour while the client is alive.
pub const HOURLY_REFRESH_SECONDS: u64 = 60 * 60;

/// Couples a manifest fetcher to the launch/hourly refresh cadence.
///
/// All public operations return the fetcher's current state.  Consequently a
/// boot fetch failure, a failed hourly fetch, or a manifest that ages out
/// remains observable as `ManifestState::FailClosed` to the embedding layer.
pub struct ManifestRefresh {
    fetcher: ManifestFetcher,
    last_refresh_unix_seconds: Option<u64>,
}

impl ManifestRefresh {
    pub fn new(fetcher: ManifestFetcher) -> Self {
        Self {
            fetcher,
            last_refresh_unix_seconds: None,
        }
    }

    /// Fetch immediately at application launch.  The caller must not enable
    /// selector-dependent encryption until this returns `Loaded`.
    pub fn boot(&mut self, now_unix_seconds: u64) -> &ManifestState {
        self.refresh(now_unix_seconds)
    }

    /// Refresh when an hour has elapsed since the previous attempt.
    ///
    /// Between attempts we still re-check manifest age.  That prevents a
    /// loaded manifest from remaining usable if the app wakes after it has
    /// exceeded its 24-hour validity window.
    pub fn tick(&mut self, now_unix_seconds: u64) -> &ManifestState {
        let refresh_due = self.last_refresh_unix_seconds.map_or(true, |last| {
            now_unix_seconds.saturating_sub(last) >= HOURLY_REFRESH_SECONDS
        });
        if refresh_due {
            self.refresh(now_unix_seconds)
        } else {
            self.fetcher.reconsider_staleness(now_unix_seconds)
        }
    }

    pub fn state(&self) -> &ManifestState {
        self.fetcher.state()
    }

    fn refresh(&mut self, now_unix_seconds: u64) -> &ManifestState {
        self.last_refresh_unix_seconds = Some(now_unix_seconds);
        self.fetcher.refresh(now_unix_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        sign_manifest, FetchError, ManifestSource, SelectorManifest, SignedManifest, SourceLabel,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use crypto::ed25519;
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::Mutex;

    struct MockSource {
        label: SourceLabel,
        responses: Mutex<VecDeque<Result<Vec<u8>, FetchError>>>,
    }

    impl MockSource {
        fn unavailable(label: SourceLabel) -> Self {
            Self {
                label,
                responses: Mutex::new(VecDeque::from([Err(FetchError::Transport(
                    "offline".into(),
                ))])),
            }
        }

        fn bodies(label: SourceLabel, bodies: Vec<Vec<u8>>) -> Self {
            Self::scripted(label, bodies.into_iter().map(Ok))
        }

        fn scripted(
            label: SourceLabel,
            responses: impl IntoIterator<Item = Result<Vec<u8>, FetchError>>,
        ) -> Self {
            Self {
                label,
                responses: Mutex::new(responses.into_iter().collect()),
            }
        }
    }

    impl ManifestSource for MockSource {
        fn label(&self) -> SourceLabel {
            self.label
        }

        fn fetch(&self) -> Result<Vec<u8>, FetchError> {
            self.responses
                .lock()
                .expect("mock lock")
                .pop_front()
                .unwrap_or_else(|| Err(FetchError::Transport("offline".into())))
        }
    }

    fn signed_manifest(issued_at_unix_seconds: u64) -> (Vec<u8>, String) {
        let (secret, public) = ed25519::generate_keypair();
        let mut selectors = BTreeMap::new();
        selectors.insert("MessageContent".into(), "abcd".into());
        let manifest = SelectorManifest {
            version: 1,
            issued_at_unix_seconds,
            client_min_version: "0.1.0".into(),
            selectors,
        };
        (
            serde_json::to_vec(&sign_manifest(&secret, &public, &manifest)).expect("manifest JSON"),
            STANDARD.encode(public.as_bytes()),
        )
    }

    #[test]
    fn t8_t19_boot_fetch_fails_closed_when_both_sources_are_down() {
        let fetcher = ManifestFetcher::new(
            Box::new(MockSource::unavailable(SourceLabel::Primary)),
            Box::new(MockSource::unavailable(SourceLabel::CdnMirror)),
            "trusted-key",
        );
        let mut refresh = ManifestRefresh::new(fetcher);

        assert!(refresh.boot(1_700_000_000).is_fail_closed());
    }

    #[test]
    fn t8_t19_bad_signatures_fail_closed_at_boot() {
        let (signed, trusted_key) = signed_manifest(1_700_000_000);
        let mut tampered: SignedManifest =
            serde_json::from_slice(&signed).expect("signed envelope");
        tampered.signature_b64 = STANDARD.encode([0_u8; 64]);
        let invalid_signature = serde_json::to_vec(&tampered).expect("tampered envelope JSON");
        let fetcher = ManifestFetcher::new(
            Box::new(MockSource::bodies(
                SourceLabel::Primary,
                vec![invalid_signature.clone()],
            )),
            Box::new(MockSource::bodies(
                SourceLabel::CdnMirror,
                vec![invalid_signature],
            )),
            trusted_key,
        );
        let mut refresh = ManifestRefresh::new(fetcher);

        assert!(refresh.boot(1_700_000_000).is_fail_closed());
    }

    #[test]
    fn t8_t19_manifest_older_than_24_hours_fails_closed() {
        let issued = 1_700_000_000;
        let (signed, trusted_key) = signed_manifest(issued);
        let fetcher = ManifestFetcher::new(
            Box::new(MockSource::bodies(
                SourceLabel::Primary,
                vec![signed.clone(); 26],
            )),
            Box::new(MockSource::bodies(SourceLabel::CdnMirror, vec![signed; 26])),
            trusted_key,
        );
        let mut refresh = ManifestRefresh::new(fetcher);

        assert!(refresh.boot(issued).is_loaded());
        for hour in 1..=24 {
            assert!(refresh
                .tick(issued + hour * HOURLY_REFRESH_SECONDS)
                .is_loaded());
        }
        assert!(refresh
            .tick(issued + 25 * HOURLY_REFRESH_SECONDS)
            .is_fail_closed());
    }

    #[test]
    fn waits_until_the_hourly_deadline_before_retrying() {
        let boot = 1_700_000_000;
        let (signed, trusted_key) = signed_manifest(boot);
        let fetcher = ManifestFetcher::new(
            Box::new(MockSource::scripted(
                SourceLabel::Primary,
                vec![Err(FetchError::Transport("offline".into())), Ok(signed)],
            )),
            Box::new(MockSource::unavailable(SourceLabel::CdnMirror)),
            trusted_key,
        );
        let mut refresh = ManifestRefresh::new(fetcher);

        assert!(refresh.boot(boot).is_fail_closed());
        assert!(refresh
            .tick(boot + HOURLY_REFRESH_SECONDS - 1)
            .is_fail_closed());
        assert!(refresh.tick(boot + HOURLY_REFRESH_SECONDS).is_loaded());
    }
}
