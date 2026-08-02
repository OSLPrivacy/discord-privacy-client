//! Persisted network-route preference and the fail-closed authorization gate.
//!
//! This deliberately decides *before* a send or store request is constructed:
//! a Tor choice with no ready tunnel is a refusal, never permission to use a
//! direct client.

use serde::{Deserialize, Serialize};

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
}
