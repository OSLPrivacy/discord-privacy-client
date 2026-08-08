//! Process-wide egress interlock for the selected network route.
//!
//! `apps/osl-hub`'s Tor gate authorizes one send at a time: a command asks
//! `TorPreferenceState::authorize_store()`, gets a route back, and builds its
//! clients from that route. That is correct for every path a person
//! remembered to gate, and silently clearnet for every path they did not --
//! which is strictly worse than refusing, because the UI still says Tor is on.
//!
//! This module removes "remembering" from the safety argument. Every
//! constructor in the workspace that would otherwise build its *own* direct
//! HTTP client asks [`direct_client_decision`] first:
//!
//! * [`Route::Clearnet`] -- no Tor choice is in force, so build as before.
//! * [`Route::Tor`] -- adopt the already-authorized SOCKS-only client instead
//!   of building a direct one. A path nobody routed by hand is therefore
//!   routed anyway, not leaked.
//! * [`Route::Sealed`] -- Tor is selected and no tunnel is ready. Refuse. There
//!   is deliberately no third answer: a fallback to clearnet here is the exact
//!   defect this interlock exists to make impossible.
//!
//! This lives in `keystore` because it is the lowest crate every HTTP
//! constructor in the product can see: `ipc` depends on `keystore`, `transport`
//! depends on `keystore`, and the hub depends on all three.

use std::net::SocketAddr;
use std::sync::{Mutex, OnceLock, PoisonError};

/// The single sentence a refused unrouted constructor reports.
///
/// It is byte-identical to the refusal `TorPreferenceState::authorize_store`
/// already returns, so a user cannot tell -- and does not need to tell --
/// whether the gate or the interlock stopped the send.
pub const TOR_UNAVAILABLE: &str = "Tor is selected but OSL has no working tunnel";

/// What route this process is permitted to originate traffic on.
#[derive(Clone, Default)]
enum Route {
    /// No Tor choice is in force. Unrouted constructors build direct clients.
    #[default]
    Clearnet,
    /// Tor is selected and this SOCKS-only client is ready.
    Tor {
        client: reqwest::blocking::Client,
        owned_socks_addr: Option<SocketAddr>,
    },
    /// Tor is selected and no tunnel is ready.
    Sealed,
}

/// The answer an unrouted client constructor acts on.
pub enum DirectClientDecision {
    /// Build a direct client, exactly as before this interlock existed.
    Build,
    /// Adopt this already-authorized client rather than building a direct one.
    Adopt(Box<reqwest::blocking::Client>),
    /// Refuse: Tor is selected and its tunnel is unavailable.
    Refuse,
}

/// The answer for a raw TCP path that cannot use the shared HTTP client.
pub enum SocketRouteDecision {
    /// No Tor choice is in force, so the caller may connect directly.
    Direct,
    /// Connect through this exact SOCKS address reported by OSL's sidecar.
    Tor(SocketAddr),
    /// Refuse before DNS resolution or a socket write.
    Refuse,
}

fn route() -> &'static Mutex<Route> {
    static ROUTE: OnceLock<Mutex<Route>> = OnceLock::new();
    ROUTE.get_or_init(|| Mutex::new(Route::Clearnet))
}

fn store(next: Route) {
    // A poisoned lock must not be able to reopen clearnet: recover the guard
    // and write the new route through it either way.
    let mut guard = route().lock().unwrap_or_else(PoisonError::into_inner);
    *guard = next;
}

fn load() -> Route {
    route()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Declare that this process may originate direct traffic.
///
/// Called when the persisted route preference is Direct, or is absent -- an
/// absent choice already refuses at the send gate, and sealing it here would
/// only break first-launch registration without closing a leak.
pub fn permit_clearnet() {
    store(Route::Clearnet);
}

/// Declare that Tor is selected and `client` is the authorized tunnel.
///
/// Every constructor that would have built a direct client adopts `client`
/// from here on, so a command nobody routed by hand still leaves over Tor.
pub fn route_through_tor(client: reqwest::blocking::Client) {
    store(Route::Tor {
        client,
        owned_socks_addr: None,
    });
}

/// Declare a healthy hub-owned Tor route, including its raw SOCKS endpoint.
///
/// HTTP constructors adopt `client`; raw TCP paths use `owned_socks_addr`.
/// Keeping both in one process-wide decision prevents the two route types from
/// disagreeing about tunnel health.
pub fn route_through_owned_tor(client: reqwest::blocking::Client, owned_socks_addr: SocketAddr) {
    store(Route::Tor {
        client,
        owned_socks_addr: Some(owned_socks_addr),
    });
}

/// Declare that Tor is selected and no tunnel is ready: refuse all egress that
/// is not already carried by an authorized client.
pub fn seal() {
    store(Route::Sealed);
}

/// True when an unrouted constructor is allowed to build a direct client.
pub fn clearnet_is_permitted() -> bool {
    matches!(load(), Route::Clearnet)
}

/// True when Tor is selected, regardless of tunnel health.
pub fn tor_is_selected() -> bool {
    !clearnet_is_permitted()
}

/// The decision every direct HTTP client constructor in the workspace makes
/// before it builds anything.
pub fn direct_client_decision() -> DirectClientDecision {
    match load() {
        Route::Clearnet => DirectClientDecision::Build,
        Route::Tor { client, .. } => DirectClientDecision::Adopt(Box::new(client)),
        Route::Sealed => DirectClientDecision::Refuse,
    }
}

/// The decision every raw remote TCP connection makes before DNS or connect.
pub fn socket_route_decision() -> SocketRouteDecision {
    match load() {
        Route::Clearnet => SocketRouteDecision::Direct,
        Route::Tor {
            owned_socks_addr: Some(addr),
            ..
        } => SocketRouteDecision::Tor(addr),
        Route::Tor {
            owned_socks_addr: None,
            ..
        }
        | Route::Sealed => SocketRouteDecision::Refuse,
    }
}

/// Restore the default route. Test-only: shipping code changes the route
/// through the hub's persisted preference, never by resetting it.
#[doc(hidden)]
pub fn reset_for_test() {
    permit_clearnet();
}

/// Restore the default route when this value is dropped, including on a
/// panic. Test-only: a case that arms the interlock must not leave a sealed
/// process behind for whatever runs next in the same binary.
#[doc(hidden)]
pub struct RouteRestoreGuard(());

impl Drop for RouteRestoreGuard {
    fn drop(&mut self) {
        permit_clearnet();
    }
}

#[doc(hidden)]
pub fn restore_clearnet_on_drop() -> RouteRestoreGuard {
    RouteRestoreGuard(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interlock is process-wide, so these cases must not interleave.
    fn serialized() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    #[test]
    fn the_default_route_builds_direct_clients() {
        let _serial = serialized();
        reset_for_test();
        assert!(clearnet_is_permitted());
        assert!(matches!(
            direct_client_decision(),
            DirectClientDecision::Build
        ));
    }

    #[test]
    fn a_sealed_route_refuses_instead_of_falling_back_to_clearnet() {
        let _serial = serialized();
        reset_for_test();
        seal();
        assert!(!clearnet_is_permitted());
        assert!(tor_is_selected());
        assert!(matches!(
            direct_client_decision(),
            DirectClientDecision::Refuse
        ));
        reset_for_test();
    }

    #[test]
    fn a_tor_route_is_adopted_rather_than_rebuilt_direct() {
        let _serial = serialized();
        reset_for_test();
        let client = std::thread::spawn(|| {
            reqwest::blocking::Client::builder()
                .build()
                .expect("build a client")
        })
        .join()
        .expect("client builder thread");
        route_through_tor(client);
        assert!(tor_is_selected());
        assert!(matches!(
            direct_client_decision(),
            DirectClientDecision::Adopt(_)
        ));
        reset_for_test();
    }

    #[test]
    fn raw_tcp_uses_only_the_owned_socks_address() {
        let _serial = serialized();
        reset_for_test();
        let client = std::thread::spawn(|| {
            reqwest::blocking::Client::builder()
                .build()
                .expect("build a client")
        })
        .join()
        .expect("client builder thread");
        let owned: SocketAddr = "127.0.0.1:43119".parse().unwrap();
        route_through_owned_tor(client, owned);
        assert!(matches!(
            socket_route_decision(),
            SocketRouteDecision::Tor(addr) if addr == owned
        ));
        reset_for_test();
    }
}
