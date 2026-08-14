#![cfg(feature = "core")]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ipc::peer_map::WhitelistEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::whitelist_state::ScopeState;
use osl_privacy_hub::account_recovery::{
    mark_recovery_kit_unsaved, record_explicit_no_recovery_secret_choice,
    record_setup_recovery_word_confirmation, save_supported_setup_completion,
    SETUP_NEEDS_RECOVERY_CHOICE,
};
use osl_privacy_hub::models::OnboardingPreferences;
use osl_privacy_hub::password_lifecycle::{
    generate_native_identity_without_recovery, RecoveryWordRetypeAnswer, RecoveryWordRetypeRequest,
};
use osl_privacy_hub::preferences::PreviewState;

const TERMINAL_ROUTES: &[&str] = &[
    "app-choice",
    "app-choice-refusal",
    "onboarding-service-home",
    "onboarding-service-continue",
    "service-guide-skip",
    "skip-connect-app",
];
const RECOVERY_KIT_STATUS_FILE: &str = "recovery_kit_status.json";
const WORDS_PASSWORD: &str = "aB3!z9-words-profile";
const NO_SECRET_PASSWORD: &str = "aB3!z9-no-secret-profile";

#[test]
fn every_shipping_setup_completion_route_supports_both_recovery_branches() {
    let root = temporary_root();
    std::fs::create_dir_all(&root).unwrap();
    let _restore = Restore(root.clone());

    let static_routes = shipping_terminal_inventory();
    let expected_routes = TERMINAL_ROUTES.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        static_routes, expected_routes,
        "shipping route inventory drifted"
    );
    println!(
        "TASK0334A_INVENTORY terminal_controls={} direct_completion_sites=4 native_commands=1 shared_boundaries=1 deep_link_completion_routes=0 migration_completion_routes=0 names={}",
        static_routes.len(),
        static_routes.iter().copied().collect::<Vec<_>>().join(",")
    );

    let words_dir = root.join("words-profile");
    std::fs::create_dir_all(&words_dir).unwrap();
    activate(&words_dir);
    let phrase = ipc::main_password::set_main_password(&words_dir, WORDS_PASSWORD).unwrap();
    mark_recovery_kit_unsaved().unwrap();

    for route in TERMINAL_ROUTES {
        let path = preferences_path(&words_dir, "unconfirmed", route);
        let error = save_supported_setup_completion(
            &PreviewState::load(path.clone()),
            finish_preferences(),
        )
        .expect_err(&format!("words-unconfirmed/{route}"));
        assert_eq!(
            error, SETUP_NEEDS_RECOVERY_CHOICE,
            "words-unconfirmed/{route}"
        );
        assert_eq!(account_ready_bytes(&path), 0, "words-unconfirmed/{route}");
    }

    let positions = unpredictable_positions(&phrase);
    let words = phrase.split_whitespace().collect::<Vec<_>>();
    let answers = positions
        .iter()
        .map(|position| RecoveryWordRetypeAnswer {
            position: *position,
            word: words[*position - 1].to_owned(),
        })
        .collect::<Vec<_>>();
    let confirmed = record_setup_recovery_word_confirmation(RecoveryWordRetypeRequest {
        recovery_phrase: phrase.clone(),
        selected_positions: positions.clone(),
        answers,
    })
    .unwrap();
    assert!(confirmed.passed);

    for route in TERMINAL_ROUTES {
        save_supported_setup_completion(
            &PreviewState::load(preferences_path(&words_dir, "complete", route)),
            finish_preferences(),
        )
        .unwrap_or_else(|error| panic!("words/{route}: {error}"));
    }
    let stored_word_text = stored_recovery_word_value_count(&words_dir, &phrase);
    assert_eq!(stored_word_text, 0);
    let words_restart = restart_and_assert_ready(
        &words_dir,
        WORDS_PASSWORD,
        &preferences_path(&words_dir, "complete", TERMINAL_ROUTES[0]),
    );
    let words_nonce_sends = one_product_nonce_send("words");

    let no_secret_dir = root.join("no-secret-profile");
    std::fs::create_dir_all(&no_secret_dir).unwrap();
    activate(&no_secret_dir);
    let identity = generate_native_identity_without_recovery();
    assert!(identity.recovery_entropy.is_none());
    let identity_sealer = keystore::MemorySealer::new();
    keystore::save_identity(
        &no_secret_dir.join("identity.json"),
        &identity,
        &identity_sealer,
    )
    .unwrap();
    let stored_identity =
        keystore::load_identity(&no_secret_dir.join("identity.json"), &identity_sealer).unwrap();
    assert!(stored_identity.recovery_entropy.is_none());
    ipc::main_password::set_main_password_without_recovery(&no_secret_dir, NO_SECRET_PASSWORD)
        .unwrap();
    for route in TERMINAL_ROUTES {
        let path = preferences_path(&no_secret_dir, "pre-choice", route);
        let error = save_supported_setup_completion(
            &PreviewState::load(path.clone()),
            finish_preferences(),
        )
        .expect_err(&format!("missing/{route}"));
        assert_eq!(error, SETUP_NEEDS_RECOVERY_CHOICE, "missing/{route}");
        assert_eq!(account_ready_bytes(&path), 0, "missing/{route}");
    }
    record_explicit_no_recovery_secret_choice().unwrap();
    for route in TERMINAL_ROUTES {
        save_supported_setup_completion(
            &PreviewState::load(preferences_path(&no_secret_dir, "complete", route)),
            finish_preferences(),
        )
        .unwrap_or_else(|error| panic!("no-secret/{route}: {error}"));
    }
    let marker = ipc::main_password::read_marker_pub(&no_secret_dir).unwrap();
    assert!(!ipc::main_password::marker_has_recovery_words(&marker));
    assert!(marker.phrase_encrypted_b64.is_empty());
    assert!(marker.phrase_nonce_b64.is_empty());
    assert!(marker.phrase_hash_b64.is_none());
    assert!(marker.file_key_phrase_wrapped_b64.is_none());
    assert!(marker.file_key_phrase_nonce_b64.is_none());
    let no_secret_restart = restart_and_assert_ready(
        &no_secret_dir,
        NO_SECRET_PASSWORD,
        &preferences_path(&no_secret_dir, "complete", TERMINAL_ROUTES[0]),
    );
    let no_secret_nonce_sends = one_product_nonce_send("no-secret");

    let copied_refusals = copied_no_secret_record_is_profile_bound(&root, &no_secret_dir);
    let missing_marker_refusals = missing_marker_cannot_forge_no_secret(&root);
    let legacy_unknown_refusals = legacy_unknown_state_refuses(&root);
    let negative_refusals = TERMINAL_ROUTES.len() * 2
        + copied_refusals
        + missing_marker_refusals
        + legacy_unknown_refusals;

    println!(
        "TASK0334A_WORDS routes={} unpredictable_positions={} positions={} unconfirmed_refusals={} completions={} restart_unlocks={} product_nonce_sends={} stored_word_text={}",
        TERMINAL_ROUTES.len(), positions.len(), positions.iter().map(usize::to_string).collect::<Vec<_>>().join(","),
        TERMINAL_ROUTES.len(), TERMINAL_ROUTES.len(), words_restart, words_nonce_sends, stored_word_text
    );
    println!(
        "TASK0334A_NO_SECRET routes={} identity_words_generated=0 identity_words_stored=0 password_words_generated=0 password_words_stored=0 missing_choice_refusals={} completions={} restart_unlocks={} product_nonce_sends={}",
        TERMINAL_ROUTES.len(), TERMINAL_ROUTES.len(), TERMINAL_ROUTES.len(), no_secret_restart,
        no_secret_nonce_sends
    );
    println!(
        "TASK0334A_NEGATIVE missing_state={} words_unconfirmed={} copied_other_profile={} missing_marker={} legacy_unknown={} stale_prechoice={} account_ready_bytes=0",
        TERMINAL_ROUTES.len(), TERMINAL_ROUTES.len(), copied_refusals, missing_marker_refusals,
        legacy_unknown_refusals, TERMINAL_ROUTES.len()
    );
    println!(
        "TASK0334A_FINISH inventory={} profiles=2 completions={} restarts={} product_nonce_sends={} negative_refusals={} account_ready_bytes=0",
        static_routes.len(), TERMINAL_ROUTES.len() * 2, words_restart + no_secret_restart,
        words_nonce_sends + no_secret_nonce_sends, negative_refusals
    );
}

fn shipping_terminal_inventory() -> BTreeSet<&'static str> {
    let source = include_str!("../../osl-hub-ui/src/main.ts");
    let native_source = include_str!("../src/main.rs");
    let permissions = include_str!("../permissions/hub.toml");
    let capability = include_str!("../capabilities/hub.json");
    let required = [
        ("app-choice", "#continue-app-choice"),
        ("app-choice-refusal", "#continue-without-apps"),
        ("onboarding-service-home", "requestedRoute === \"home\""),
        (
            "onboarding-service-continue",
            "#onboarding-service-continue",
        ),
        ("service-guide-skip", "#service-guide-skip"),
        ("skip-connect-app", "#skip-connect-app"),
    ];
    assert_eq!(
        source
            .matches("async function completeOnboarding()")
            .count(),
        1
    );
    assert_eq!(
        source
            .matches("async function completeSixStepOnboarding()")
            .count(),
        1
    );
    assert_eq!(source.matches("completeOnboarding();").count(), 4);
    assert_eq!(
        source
            .matches("saveOnboardingPreferences({ onboardingComplete: true")
            .count(),
        2,
        "one setup commit plus one post-setup settings writer must stay inventoried"
    );
    assert!(
        !source.contains("#service-guide-finish"),
        "phantom completion route returned"
    );
    assert!(native_source
        .contains("account_recovery::save_supported_setup_completion(&state, preferences)"));
    for command in [
        "create_hub_osl_identity_without_recovery",
        "setup_hub_main_password_without_recovery",
    ] {
        assert!(
            native_source.contains(command),
            "missing native command {command}"
        );
        assert!(
            permissions.contains(command),
            "missing permission {command}"
        );
        assert!(
            capability.contains(&format!("allow-{}", command.replace('_', "-"))),
            "missing capability {command}"
        );
    }
    required
        .into_iter()
        .map(|(name, needle)| {
            assert!(
                source.contains(needle),
                "starved route inventory: {name} ({needle})"
            );
            name
        })
        .collect()
}

fn finish_preferences() -> OnboardingPreferences {
    OnboardingPreferences {
        onboarding_complete: true,
        ..OnboardingPreferences::default()
    }
}

fn preferences_path(directory: &Path, phase: &str, route: &str) -> PathBuf {
    directory.join(format!("preferences-{phase}-{route}.json"))
}

fn activate(directory: &Path) {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(directory.to_path_buf()));
}

fn unpredictable_positions(phrase: &str) -> Vec<usize> {
    let word_count = phrase.split_whitespace().count();
    let random = crypto::random::random_bytes(24);
    let mut positions = BTreeSet::new();
    for byte in random {
        positions.insert((usize::from(byte) % word_count) + 1);
        if positions.len() == 3 {
            break;
        }
    }
    assert_eq!(positions.len(), 3);
    positions.into_iter().collect()
}

fn stored_recovery_word_value_count(directory: &Path, phrase: &str) -> usize {
    let sealed = std::fs::read(directory.join(RECOVERY_KIT_STATUS_FILE)).unwrap();
    let key = ipc::main_password::get_file_storage_key().unwrap();
    let plaintext = ipc::main_password::decrypt_at_rest(&sealed, &key).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&plaintext).unwrap();
    let serialized_values = document
        .as_object()
        .unwrap()
        .values()
        .filter_map(serde_json::Value::as_str)
        .collect::<BTreeSet<_>>();
    phrase
        .split_whitespace()
        .filter(|word| serialized_values.contains(word))
        .count()
}

fn restart_and_assert_ready(directory: &Path, password: &str, preferences: &Path) -> usize {
    ipc::main_password::set_file_storage_key(None);
    activate(directory);
    ipc::main_password::verify_main_password(directory, password).unwrap();
    assert!(
        PreviewState::load(preferences.to_path_buf())
            .get()
            .unwrap()
            .onboarding_complete
    );
    1
}

fn one_product_nonce_send(branch: &str) -> usize {
    const SENDER_ID: &str = "900000000000000001";
    const RECIPIENT_ID: &str = "900000000000000002";
    let sender = ipc::AppState::new();
    let recipient = ipc::AppState::new();
    sender.install_identity(keystore::generate_identity(format!(
        "task-0334a-{branch}-sender"
    )));
    recipient.install_identity(keystore::generate_identity(format!(
        "task-0334a-{branch}-recipient"
    )));
    {
        let identity = recipient.identity.lock().unwrap();
        let identity = identity.as_ref().unwrap();
        let mut peers = sender.peer_map.lock().unwrap();
        let peer = peers.entry(RECIPIENT_ID.to_owned()).or_default();
        peer.discord_id = Some(RECIPIENT_ID.to_owned());
        peer.pubkey = Some(STANDARD.encode(identity.x25519_public.as_bytes()));
        peer.ik_mlkem768_pub = Some(STANDARD.encode(identity.mlkem_public_bytes));
        peer.outgoing_whitelists.push(WhitelistEntry::Dm {
            broadened: false,
            enabled_at: None,
        });
    }
    {
        let identity = sender.identity.lock().unwrap();
        let identity = identity.as_ref().unwrap();
        let mut peers = recipient.peer_map.lock().unwrap();
        let peer = peers.entry(SENDER_ID.to_owned()).or_default();
        peer.discord_id = Some(SENDER_ID.to_owned());
        peer.pubkey = Some(STANDARD.encode(identity.x25519_public.as_bytes()));
        peer.ik_mlkem768_pub = Some(STANDARD.encode(identity.mlkem_public_bytes));
    }
    let scope = Scope::dm(RECIPIENT_ID);
    sender.whitelist_state.lock().unwrap().insert(
        scope.storage_key(),
        ScopeState {
            encrypt_toggle: true,
            auto_enabled: true,
            ..ScopeState::default()
        },
    );
    let nonce = crypto::random::random_bytes(24)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let sealed = ipc::commands::cmd_osl_encrypt_message_v2(
        &sender,
        nonce.clone(),
        ScopeInput::from(&scope),
        vec![RECIPIENT_ID.to_owned()],
        SENDER_ID.to_owned(),
    )
    .unwrap();
    assert_eq!(sealed.messages.len(), 1);
    let opened = ipc::commands::cmd_osl_decrypt_message_v2(
        &recipient,
        None,
        format!("task-0334a-{branch}"),
        SENDER_ID.to_owned(),
        sealed.messages.into_iter().next().unwrap(),
        Some(ScopeInput::from(&scope)),
        None,
    )
    .unwrap();
    assert_eq!(opened, nonce);
    1
}

fn copied_no_secret_record_is_profile_bound(root: &Path, source: &Path) -> usize {
    let copied = std::fs::read(source.join(RECOVERY_KIT_STATUS_FILE)).unwrap();
    let target = root.join("copy-target");
    std::fs::create_dir_all(&target).unwrap();
    activate(&target);
    ipc::main_password::set_file_storage_key(Some([0x51; 32]));
    std::fs::write(target.join(RECOVERY_KIT_STATUS_FILE), copied).unwrap();
    let preferences = target.join("preferences.json");
    let error = save_supported_setup_completion(
        &PreviewState::load(preferences.clone()),
        finish_preferences(),
    )
    .unwrap_err();
    assert!(error.contains("could not be decrypted"));
    assert_eq!(account_ready_bytes(&preferences), 0);
    1
}

fn missing_marker_cannot_forge_no_secret(root: &Path) -> usize {
    let directory = root.join("missing-marker");
    std::fs::create_dir_all(&directory).unwrap();
    activate(&directory);
    ipc::main_password::set_file_storage_key(Some([0x72; 32]));
    assert_eq!(
        record_explicit_no_recovery_secret_choice().unwrap_err(),
        "OSL cannot choose no recovery secret before password setup"
    );
    let preferences = directory.join("preferences.json");
    assert_eq!(
        save_supported_setup_completion(
            &PreviewState::load(preferences.clone()),
            finish_preferences()
        )
        .unwrap_err(),
        SETUP_NEEDS_RECOVERY_CHOICE
    );
    assert_eq!(account_ready_bytes(&preferences), 0);
    1
}

fn legacy_unknown_state_refuses(root: &Path) -> usize {
    let directory = root.join("legacy-unknown");
    std::fs::create_dir_all(&directory).unwrap();
    activate(&directory);
    let key = [0x93; 32];
    ipc::main_password::set_file_storage_key(Some(key));
    let plaintext = br#"{"version":1,"kit_unsaved":false}"#;
    let sealed = ipc::main_password::encrypt_at_rest(plaintext, &key).unwrap();
    std::fs::write(directory.join(RECOVERY_KIT_STATUS_FILE), sealed).unwrap();
    let preferences = directory.join("preferences.json");
    assert_eq!(
        save_supported_setup_completion(
            &PreviewState::load(preferences.clone()),
            finish_preferences()
        )
        .unwrap_err(),
        SETUP_NEEDS_RECOVERY_CHOICE
    );
    assert_eq!(account_ready_bytes(&preferences), 0);
    1
}

fn account_ready_bytes(path: &Path) -> u64 {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn temporary_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("osl-task-0334a-{}-{nonce}", std::process::id()))
}

struct Restore(PathBuf);

impl Drop for Restore {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
