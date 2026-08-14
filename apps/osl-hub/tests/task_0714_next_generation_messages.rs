use ipc::wire_rn::{
    accept_and_persist_with_sealer, initiate_and_persist_with_sealer, send_rn_for_state,
    RnSessionStore, RN_CONTEXT_DISCORD_MANUAL,
};
use keystore::client::{PeerCapabilities, RN_CAP_WIRE_RN};
use keystore::MemorySealer;
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::models::{ForwardSecrecyMode, OnboardingPreferences, PlacementMode, SendMode};
use osl_privacy_hub::preferences::{apply_saved_message_runtime_preferences, PreviewState};
use osl_ratchet_next::test_support::{fresh_bundle, seeded_identity, seeded_rng};
use osl_ratchet_next::{peek_wire_version, SessionParams, WIRE_VERSION_RN};
use serde_json::Value;

#[derive(Debug)]
struct PreparedNextGenerationMessage {
    message_id: String,
    next_generation_protection: String,
    wire_version: u8,
}

fn prepare_direct_next_generation_message(state: &ipc::AppState) -> PreparedNextGenerationMessage {
    let alice_dir = tempfile::tempdir().expect("alice RN dir");
    let bob_dir = tempfile::tempdir().expect("bob RN dir");
    let alice_store = RnSessionStore::new(alice_dir.path().join("rn"));
    let bob_store = RnSessionStore::new(bob_dir.path().join("rn"));
    let sealer = MemorySealer::new();
    let mut rng = seeded_rng(0x0714);
    let (bob_prekeys, bob_bundle) = fresh_bundle(&mut rng);
    let alice_identity = seeded_identity(0x0714_a11c);
    let alice_public = alice_identity.public();
    let bob_peer = *bob_bundle.identity.as_bytes();
    let bob_ek = bob_bundle.pq_prekey.to_bytes();

    initiate_and_persist_with_sealer(
        &alice_store,
        &sealer,
        &alice_identity,
        alice_public.as_bytes(),
        &bob_bundle,
        PeerCapabilities::Verified(RN_CAP_WIRE_RN),
        &bob_ek,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("prepare persisted RN session");

    let message_id = "peer-07140000000000000000000000000000".to_owned();
    let payload = serde_json::json!({
        "messageId": message_id,
        "nextGenerationProtection": "on",
        "body": "task 0714 protected payload"
    })
    .to_string();
    let wire = send_rn_for_state(
        state,
        &alice_store,
        &sealer,
        &bob_peer,
        ipc::wire_v2::MSG_TYPE_CONTENT,
        payload.as_bytes(),
    )
    .expect("state-gated RN message prepare");
    let wire_version = peek_wire_version(&wire).expect("prepared RN wire version");
    assert_eq!(wire_version, WIRE_VERSION_RN);

    let (_session, opened) = accept_and_persist_with_sealer(
        &bob_store,
        &sealer,
        &bob_prekeys,
        bob_bundle.identity.as_bytes(),
        &bob_ek,
        &wire,
        RN_CONTEXT_DISCORD_MANUAL,
        SessionParams::default(),
    )
    .expect("recipient opens prepared RN payload");
    let opened: Value = serde_json::from_slice(&opened.plaintext).expect("json protected payload");
    assert_eq!(opened["messageId"], message_id);
    assert_eq!(opened["nextGenerationProtection"], "on");

    PreparedNextGenerationMessage {
        message_id,
        next_generation_protection: opened["nextGenerationProtection"]
            .as_str()
            .expect("protection label")
            .to_owned(),
        wire_version,
    }
}

#[test]
fn task_0714_saved_on_survives_restart_and_prepares_next_generation_message() {
    let temp = tempfile::tempdir().expect("preference dir");
    let preferences_path = temp.path().join("preview-preferences.json");
    let first_host_preferences = PreviewState::load(preferences_path.clone());
    let saved = first_host_preferences
        .save(OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::Enter,
            placement_mode: PlacementMode::Atomic,
            cover_insertion: None,
            show_plaintext_preview: false,
            window_capture_enabled: true,
            rn_wire_policy_requested: true,
            acknowledge_experimental_send_risk: false,
            forward_secrecy_mode: ForwardSecrecyMode::KeepGroupDelivery,
        })
        .expect("save RN policy on");
    assert!(saved.rn_wire_policy_requested);

    let restarted_preferences = PreviewState::load(preferences_path);
    let saved_setting_after_restart = restarted_preferences
        .get()
        .expect("read restarted preferences")
        .rn_wire_policy_requested;
    assert!(
        saved_setting_after_restart,
        "next-generation setting rn_wire_policy_requested was not saved after restart"
    );

    let restarted_command_host = HubCoreState::default();
    let runtime_gate_after_restart =
        apply_saved_message_runtime_preferences(&restarted_command_host, &restarted_preferences)
            .expect("apply restarted preferences to command host");
    assert!(
        runtime_gate_after_restart,
        "next-generation setting rn_wire_policy_requested did not reopen the restarted runtime gate"
    );

    let prepared = prepare_direct_next_generation_message(&restarted_command_host.osl);
    assert!(!prepared.message_id.is_empty());
    assert_eq!(prepared.next_generation_protection, "on");

    println!("savedSettingAfterRestart={saved_setting_after_restart}");
    println!("runtimeGateAfterRestart={runtime_gate_after_restart}");
    println!("preparedMessageId={}", prepared.message_id);
    println!(
        "nextGenerationProtection={}",
        prepared.next_generation_protection
    );
    println!("wireVersion={}", prepared.wire_version);
}
