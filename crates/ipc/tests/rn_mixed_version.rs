//! T19-T26: a `0x10` wire aimed at an older receiver is visible on every
//! receive boundary instead of disappearing.

use base64::Engine as _;
use ipc::commands::cmd_osl_decrypt_message_v2;
use ipc::state::AppState;

const BROKER_SOURCE: &str = include_str!("../../../apps/osl-hub/src/broker.rs");

#[test]
fn rn_wire_is_reported_by_the_command_dispatcher_and_overlay_drain() {
    let state = AppState::new();
    state.install_identity(keystore::generate_identity("t19-e5-receiver".to_owned()));

    // A legacy build cannot parse the body.  Its dispatcher must still return
    // a named failure to its caller; `Ok` would turn this into a silent drop.
    let unknown_rn_wire = format!(
        "DPC0::{}",
        base64::engine::general_purpose::STANDARD.encode([0x10_u8])
    );
    let dispatcher_report = cmd_osl_decrypt_message_v2(
        &state,
        Some("t19-e5-message".to_owned()),
        "t19-e5-channel".to_owned(),
        "900000000000000105".to_owned(),
        unknown_rn_wire,
        None,
        None,
    )
    .expect_err("a receiver that cannot decode 0x10 must report, never return a no-op");
    assert!(
        dispatcher_report.starts_with("OSL:"),
        "dispatcher failure must be a named user-visible OSL report: {dispatcher_report}"
    );

    // The overlay drain is in the desktop crate and cannot be linked back into
    // ipc without creating a dependency cycle.  Keep this integration proof
    // on its exact drop arm: removing B5's counter increment (or restoring a
    // bare `continue`) makes this assertion fail.
    let drain = BROKER_SOURCE
        .split_once("if !ipc::wire_v2::is_native_overlay_relay_bundle(&bundle) {")
        .and_then(|(_, tail)| tail.split_once("let wire = format!(\"DPC0::{}\", STANDARD.encode(&bundle));"))
        .map(|(drain, _)| drain)
        .expect("overlay drain's unknown-wire branch must remain present");
    assert!(
        BROKER_SOURCE.contains("let mut unrecognized_wire_rows = 0u32;"),
        "the drain must expose a named unrecognised-wire report counter"
    );
    assert!(
        drain.contains("unrecognized_wire_rows = unrecognized_wire_rows.saturating_add(1);"),
        "unknown overlay wires must increment the named report counter"
    );
    assert!(
        drain.contains("unrecognized_wire_rows = unrecognized_wire_rows.saturating_add(1);\n            continue;"),
        "the retained row is reported before this build declines to consume it"
    );
}
