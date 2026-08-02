//! Persisted network-route preference and the fail-closed authorization gate.
//!
//! This deliberately decides *before* a send or store request is constructed:
//! a Tor choice with no ready tunnel is a refusal, never permission to use a
//! direct client.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

const PREFERENCE_FILE_VERSION: u8 = 1;
const MAX_PREFERENCE_BYTES: u64 = 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TorPreference {
    Direct,
    Tor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TunnelState {
    Ready,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkOperation {
    Send,
    Store,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkAuthorization {
    Direct,
    Tor,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    ChoiceRequired,
    TorUnavailable(NetworkOperation),
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedPreference {
    version: u8,
    preference: Option<TorPreference>,
}

/// The native authority for the route selected during onboarding.
///
/// A missing or malformed preference is deliberately kept as `None`: network
/// operations then refuse before a store request is constructed. T1-71 owns
/// tunnel lifecycle; until it reports a ready tunnel, a Tor choice also
/// refuses rather than falling back to direct traffic.
pub struct TorPreferenceState {
    path: PathBuf,
    preference: Mutex<Option<TorPreference>>,
}

impl TorPreferenceState {
    pub fn load(path: PathBuf) -> Self {
        let preference = read_preference(&path).unwrap_or(None);
        Self {
            path,
            preference: Mutex::new(preference),
        }
    }

    pub fn preference(&self) -> Result<Option<TorPreference>, String> {
        self.preference
            .lock()
            .map(|preference| *preference)
            .map_err(|_| "OSL network preference is unavailable".to_owned())
    }

    pub fn set_preference(&self, preference: TorPreference) -> Result<TorPreference, String> {
        write_preference(&self.path, preference)?;
        let mut current = self
            .preference
            .lock()
            .map_err(|_| "OSL network preference is unavailable".to_owned())?;
        *current = Some(preference);
        Ok(preference)
    }

    /// Authorize before the caller can construct or send a store request.
    ///
    /// `Unavailable` is intentional here: this app has no direct handle to
    /// T1-71's supervised Tor process yet. Consequently selecting Tor blocks
    /// traffic instead of silently using the direct IPC client.
    pub fn authorize_store(&self) -> Result<NetworkAuthorization, String> {
        let authorization = authorize_network(
            self.preference()?,
            TunnelState::Unavailable,
            NetworkOperation::Store,
        );
        match authorization {
            NetworkAuthorization::Refused(Refusal::ChoiceRequired) => {
                Err("Choose a connection route before sending encrypted text".to_owned())
            }
            NetworkAuthorization::Refused(Refusal::TorUnavailable(_)) => {
                Err("Tor is selected, but its tunnel is unavailable; no message was sent".to_owned())
            }
            allowed => Ok(allowed),
        }
    }
}

fn read_preference(path: &Path) -> Option<Option<TorPreference>> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_PREFERENCE_BYTES,
        "network route preference",
    )
    .ok()
    .flatten()?;
    let persisted = serde_json::from_slice::<PersistedPreference>(&bytes).ok()?;
    (persisted.version == PREFERENCE_FILE_VERSION).then_some(persisted.preference)
}

fn write_preference(path: &Path, preference: TorPreference) -> Result<(), String> {
    let bytes = serde_json::to_vec(&PersistedPreference {
        version: PREFERENCE_FILE_VERSION,
        preference: Some(preference),
    })
    .map_err(|_| "OSL network preference could not be encoded".to_owned())?;
    crate::atomic_file::write_recoverable(path, &bytes, "network route preference")
}

/// Authorize one network operation without providing any fallback path.
pub fn authorize_network(
    preference: Option<TorPreference>,
    tunnel: TunnelState,
    operation: NetworkOperation,
) -> NetworkAuthorization {
    match preference {
        None => NetworkAuthorization::Refused(Refusal::ChoiceRequired),
        Some(TorPreference::Direct) => NetworkAuthorization::Direct,
        Some(TorPreference::Tor) if tunnel == TunnelState::Ready => NetworkAuthorization::Tor,
        Some(TorPreference::Tor) => {
            NetworkAuthorization::Refused(Refusal::TorUnavailable(operation))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_choice_refuses_every_network_operation() {
        for operation in [NetworkOperation::Send, NetworkOperation::Store] {
            assert_eq!(
                authorize_network(None, TunnelState::Ready, operation),
                NetworkAuthorization::Refused(Refusal::ChoiceRequired),
            );
        }
    }

    #[test]
    fn tor_with_an_unavailable_tunnel_fails_closed_for_send_and_store() {
        for operation in [NetworkOperation::Send, NetworkOperation::Store] {
            assert_eq!(
                authorize_network(
                    Some(TorPreference::Tor),
                    TunnelState::Unavailable,
                    operation
                ),
                NetworkAuthorization::Refused(Refusal::TorUnavailable(operation)),
            );
        }
    }

    #[test]
    fn ready_tor_is_authorized_only_as_tor_and_direct_stays_explicit() {
        assert_eq!(
            authorize_network(
                Some(TorPreference::Tor),
                TunnelState::Ready,
                NetworkOperation::Send
            ),
            NetworkAuthorization::Tor,
        );
        assert_eq!(
            authorize_network(
                Some(TorPreference::Direct),
                TunnelState::Unavailable,
                NetworkOperation::Store
            ),
            NetworkAuthorization::Direct,
        );
    }

    #[test]
    fn persisted_tor_with_no_tunnel_cannot_reach_the_store() {
        let directory = tempfile::tempdir().expect("temporary preference directory");
        let state = TorPreferenceState::load(directory.path().join("tor-preference.json"));
        state
            .set_preference(TorPreference::Tor)
            .expect("persist Tor choice");

        assert_eq!(
            state.authorize_store(),
            Err("Tor is selected, but its tunnel is unavailable; no message was sent".to_owned())
        );
    }

    #[test]
    fn shipping_store_sends_are_gated_before_the_broker_is_called() {
        let source = include_str!("main.rs");
        for (command, send) in [
            ("async fn prepare_osl_chat_text(", "broker::prepare_osl_chat_text("),
            (
                "async fn prepare_peer_prose_text(",
                "broker::prepare_peer_prose_text_with_capture(",
            ),
        ] {
            let body = &source[source.find(command).expect("shipping send command must exist")..];
            assert!(
                body.find("app.state::<TorPreferenceState>().authorize_store()?;")
                    .expect("Tor choice must gate the store send")
                    < body.find(send).expect("shipping store send must exist"),
                "the Tor gate must run before encrypted store traffic is constructed"
            );
        }
    }
}
