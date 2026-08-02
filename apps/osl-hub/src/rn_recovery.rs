//! SESSION_RESET emission over the Hub's authenticated control-inbox transport.
//!
//! The reset wire itself is deliberately built by `ipc`: it resets the local
//! ratchet and encrypts the control payload independently of that ratchet.
//! This module owns the missing final hop to the peer's control inbox.

use crate::broker::{HubBrokerState, SessionResetDeliveryTarget};
use crate::core_bridge::HubCoreState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::time::{SystemTime, UNIX_EPOCH};

const RESET_WIRE_PREFIX: &str = "DPC0::";

/// Build a SESSION_RESET for the active verified conversation and hand the
/// resulting ratchet-independent wire to the authenticated transport.
pub fn emit_active_session_reset(
    core: &HubCoreState,
    broker: &HubBrokerState,
) -> Result<(), String> {
    let target = broker.active_session_reset_delivery_target()?;
    let identity = core
        .osl
        .identity
        .lock()
        .map_err(|_| "OSL identity state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
    let client = core
        .osl
        .keyserver
        .lock()
        .map_err(|_| "OSL key server state is unavailable".to_owned())?
        .clone()
        .ok_or_else(|| "OSL key server is unavailable".to_owned())?;

    build_and_post_session_reset(
        &target,
        |peer_id| ipc::commands::cmd_osl_build_session_reset(&core.osl, peer_id.to_owned()),
        |peer_osl_user_id, scope_id, bundle| {
            client
                .post_control_inbox(&identity, peer_osl_user_id, scope_id, bundle)
                .map(|_| ())
                .map_err(|_| "OSL could not deliver the session recovery request".to_owned())
        },
        || {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .try_into()
                .unwrap_or(i64::MAX);
            core.osl
                .recovery_guard
                .lock()
                .map_err(|_| "OSL recovery state is unavailable".to_owned())?
                .record_emitted(
                    &target.peer_id,
                    ipc::recovery::RecoveryKind::SessionReset,
                    now,
                );
            Ok(())
        },
    )
}

fn build_and_post_session_reset<Build, Post, Confirm>(
    target: &SessionResetDeliveryTarget,
    build: Build,
    post: Post,
    confirm_delivery: Confirm,
) -> Result<(), String>
where
    Build: FnOnce(&str) -> Result<String, String>,
    Post: FnOnce(&str, &str, &[u8]) -> Result<(), String>,
    Confirm: FnOnce() -> Result<(), String>,
{
    let wire = build(&target.peer_id)?;
    let encoded = wire
        .strip_prefix(RESET_WIRE_PREFIX)
        .ok_or_else(|| "OSL session recovery produced an invalid wire".to_owned())?;
    let bundle = STANDARD
        .decode(encoded)
        .map_err(|_| "OSL session recovery produced an invalid wire".to_owned())?;
    if bundle.get(0) != Some(&ipc::wire_v2::WIRE_VERSION_V2)
        || bundle.get(1) != Some(&ipc::wire_v2::MSG_TYPE_SESSION_RESET)
    {
        return Err("OSL session recovery produced an invalid wire".to_owned());
    }
    post(&target.peer_osl_user_id, &target.scope_id, &bundle)?;
    confirm_delivery()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn session_reset_wire_is_handed_to_the_active_transport() {
        let target = SessionResetDeliveryTarget {
            peer_id: "peer-id".to_owned(),
            peer_osl_user_id: "peer-osl-id".to_owned(),
            scope_id: "scope-id".to_owned(),
        };
        let wire = format!(
            "{RESET_WIRE_PREFIX}{}",
            STANDARD.encode([
                ipc::wire_v2::WIRE_VERSION_V2,
                ipc::wire_v2::MSG_TYPE_SESSION_RESET,
                1,
            ])
        );
        let handed_off = RefCell::new(None);

        build_and_post_session_reset(
            &target,
            |_| Ok(wire),
            |peer, scope, bundle| {
                *handed_off.borrow_mut() =
                    Some((peer.to_owned(), scope.to_owned(), bundle.to_vec()));
                Ok(())
            },
            || Ok(()),
        )
        .expect("a built SESSION_RESET must be handed to a transport");

        assert_eq!(
            handed_off.into_inner(),
            Some((
                "peer-osl-id".to_owned(),
                "scope-id".to_owned(),
                vec![
                    ipc::wire_v2::WIRE_VERSION_V2,
                    ipc::wire_v2::MSG_TYPE_SESSION_RESET,
                    1
                ],
            ))
        );
    }

    #[test]
    fn failed_session_reset_delivery_leaves_the_retry_unthrottled() {
        let target = SessionResetDeliveryTarget {
            peer_id: "peer-id".to_owned(),
            peer_osl_user_id: "peer-osl-id".to_owned(),
            scope_id: "scope-id".to_owned(),
        };
        let wire = format!(
            "{RESET_WIRE_PREFIX}{}",
            STANDARD.encode([
                ipc::wire_v2::WIRE_VERSION_V2,
                ipc::wire_v2::MSG_TYPE_SESSION_RESET,
                1,
            ])
        );
        let guard = RefCell::new(ipc::recovery::RecoveryGuard::default());

        assert!(guard.borrow().may_emit(
            &target.peer_id,
            ipc::recovery::RecoveryKind::SessionReset,
            1000
        ));
        let failed = build_and_post_session_reset(
            &target,
            |_| Ok(wire.clone()),
            |_, _, _| Err("transport unavailable".to_owned()),
            || {
                guard.borrow_mut().record_emitted(
                    &target.peer_id,
                    ipc::recovery::RecoveryKind::SessionReset,
                    1000,
                );
                Ok(())
            },
        );
        assert!(failed.is_err(), "failed delivery must be reported");
        assert!(guard.borrow().may_emit(
            &target.peer_id,
            ipc::recovery::RecoveryKind::SessionReset,
            1001
        ));
    }
}
