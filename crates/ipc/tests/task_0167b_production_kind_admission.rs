use ipc::allowed_places::allowed_places_db_path;
use ipc::auto_whitelist_rules::DiscordWhitelistKind;
use ipc::commands::cmd_osl_save_auto_whitelist_rule;
use ipc::email_whitelist_kinds::EmailWhitelistKind;
use ipc::production_kind_admission::{
    admit_discord_provider_discovery, admit_email_provider_discovery, ProductionAdmissionObserver,
    ShippingDiscordProviderRegistry, DISCORD_DECLARED_KINDS, EMAIL_DECLARED_KINDS,
    PRODUCTION_KIND_DESERIALIZER,
};
use ipc::shipping_email::ShippingEmailProviderRegistry;
use ipc::state::AppState;
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use std::process::Command;

const DISCORD_ACCOUNT: &str = "signed-in-discord-account-0167b";
const DISCORD_BINDING: &str = "0e1bc132d0e04d4988a30f347615fd2da1b08e1084095697f0ab585f8e6bc819";
const EMAIL_ACCOUNT: &str = "independently-verified-gmail-account-0167b";

fn random_hex(bytes: usize) -> String {
    let mut raw = vec![0u8; bytes];
    OsRng.fill_bytes(&mut raw);
    raw.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn revision() -> String {
    format!("production-revision-0167b-{}", random_hex(16))
}

fn discord_json(revision: &str, kind: Value, suffix: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "connectorRevision": revision,
        "kind": kind,
        "personName": format!("person-{suffix}"),
        "providerPlaceId": format!("discord-place-{suffix}")
    }))
    .unwrap()
}

fn email_json(revision: &str, kind: Value, suffix: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "connectorRevision": revision,
        "kind": kind,
        "providerMessageId": format!("gmail-message-{suffix}"),
        "senderHeader": format!("sender-{suffix}@example.test")
    }))
    .unwrap()
}

fn run_discord_controls(
    state: &AppState,
    dir: &std::path::Path,
    revision: &str,
    phase: &str,
    observer: &mut ProductionAdmissionObserver,
) -> Vec<String> {
    let account = ShippingDiscordProviderRegistry::authoritative()
        .verify_signed_in_account("native_discord", DISCORD_ACCOUNT, DISCORD_BINDING, revision)
        .expect("real signed-in Discord connector session");
    DISCORD_DECLARED_KINDS
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let suffix = format!("{phase}-{index}");
            let receipt = admit_discord_provider_discovery(
                state,
                dir,
                &account,
                &discord_json(revision, json!(kind), &suffix),
                observer,
            )
            .expect("declared Discord kind control");
            assert_eq!(receipt.carrier, "discord");
            assert_eq!(receipt.raw_kind, *kind);
            assert_eq!(receipt.normalized_kind, *kind);
            assert_eq!(receipt.rule_key, format!("discord:{kind}"));
            assert_eq!(receipt.rule_choice, "always");
            assert_eq!(receipt.outcome, "allowed");
            format!(
                "{}={}:{}",
                receipt.raw_kind, receipt.rule_key, receipt.outcome
            )
        })
        .collect()
}

fn run_email_controls(
    state: &AppState,
    dir: &std::path::Path,
    revision: &str,
    phase: &str,
    observer: &mut ProductionAdmissionObserver,
) -> Vec<String> {
    let account = ShippingEmailProviderRegistry::authoritative()
        .verify_connected_account("gmail", EMAIL_ACCOUNT, revision)
        .expect("independently verified supported email connector session");
    EMAIL_DECLARED_KINDS
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let suffix = format!("{phase}-{index}");
            let receipt = admit_email_provider_discovery(
                state,
                dir,
                &account,
                &email_json(revision, json!(kind), &suffix),
                observer,
            )
            .expect("declared email kind control");
            let expected_rule = if *kind == "address" {
                "email_address"
            } else {
                "email_domain"
            };
            assert_eq!(receipt.carrier, "email");
            assert_eq!(receipt.raw_kind, *kind);
            assert_eq!(receipt.normalized_kind, *kind);
            assert_eq!(receipt.rule_key, expected_rule);
            assert_eq!(receipt.rule_choice, "always");
            assert_eq!(receipt.outcome, "provider_allowed");
            format!(
                "{}={}:{}",
                receipt.raw_kind, receipt.rule_key, receipt.outcome
            )
        })
        .collect()
}

fn first_forbidden_action(observer: &ProductionAdmissionObserver) -> Option<&'static str> {
    if observer.normalized_kinds > 0 {
        Some("normalization")
    } else if observer.rule_lookups > 0 {
        Some("lookup")
    } else if observer.allowed_rows > 0 || observer.pending_rows > 0 {
        Some("row")
    } else if observer.prompts > 0 {
        Some("prompt")
    } else if observer.notices > 0 {
        Some("notice")
    } else if observer.provider_actions > 0 {
        Some("provider_action")
    } else if observer.output_surface_rows > 0 {
        Some("output_surface")
    } else {
        None
    }
}

fn report_forbidden_action(
    carrier: &str,
    raw_kind: &str,
    outcome: &str,
    observer: &ProductionAdmissionObserver,
) {
    let first = first_forbidden_action(observer).expect("a forbidden downstream action occurred");
    println!(
        "TASK0167B_FORBIDDEN carrier={} raw_kind={} first_forbidden={} outcome={} downstream_total={} normalized={} rule_lookups={} allowed_rows={} pending_rows={} prompts={} notices={} provider_actions={} output_rows={}",
        carrier,
        raw_kind,
        first,
        outcome,
        observer.downstream_total(),
        observer.normalized_kinds,
        observer.rule_lookups,
        observer.allowed_rows,
        observer.pending_rows,
        observer.prompts,
        observer.notices,
        observer.provider_actions,
        observer.output_surface_rows,
    );
}

fn spawn_hostile(carrier: &str, revision: &str, label: &str, json: &[u8]) {
    let output = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("task_0167b_hostile_probe")
        .arg("--nocapture")
        .env("TASK0167B_HOSTILE_CARRIER", carrier)
        .env("TASK0167B_HOSTILE_REVISION", revision)
        .env("TASK0167B_HOSTILE_LABEL", label)
        .env(
            "TASK0167B_HOSTILE_JSON",
            String::from_utf8(json.to_vec()).unwrap(),
        )
        .output()
        .expect("run production-boundary hostile helper");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    assert_eq!(
        output.status.code(),
        Some(1),
        "{carrier} hostile {label} must exit 1"
    );
    assert!(stderr.contains(&format!("carrier={carrier}")));
    assert!(stderr.contains(&format!("raw_kind={label}")));
    assert!(stderr.contains("downstream_total=0"));
    println!("TASK0167B_HOSTILE carrier={carrier} raw_kind={label} exit_code=1");
}

#[test]
fn production_discovery_refuses_every_unknown_kind_before_all_downstream_effects() {
    let discord_registry: Vec<_> = DiscordWhitelistKind::ALL
        .into_iter()
        .map(DiscordWhitelistKind::id)
        .collect();
    let email_registry: Vec<_> = EmailWhitelistKind::ALL
        .into_iter()
        .map(EmailWhitelistKind::id)
        .collect();
    assert_eq!(discord_registry, DISCORD_DECLARED_KINDS);
    assert_eq!(email_registry, EMAIL_DECLARED_KINDS);
    assert!(!DISCORD_DECLARED_KINDS.is_empty());
    assert!(!EMAIL_DECLARED_KINDS.is_empty());

    let discord_connector = ShippingDiscordProviderRegistry::authoritative()
        .signed_in_connector("native_discord")
        .expect("shipping signed-in Discord connector");
    assert!(discord_connector.supports_signed_in_discovery);
    assert!(discord_connector.supports_provider_action);
    assert!(ShippingDiscordProviderRegistry::authoritative()
        .signed_in_connector("fixture_discord")
        .is_none());
    let email_connector = ShippingEmailProviderRegistry::authoritative()
        .supported_connector("gmail")
        .expect("shipping independently verified email connector");
    assert!(email_connector.supports_sender_discovery);
    assert!(email_connector.supports_provider_action);
    assert!(ShippingEmailProviderRegistry::authoritative()
        .supported_connector("outlook")
        .is_none());

    let data = tempfile::tempdir().unwrap();
    let state = AppState::new();
    for kind in DISCORD_DECLARED_KINDS {
        cmd_osl_save_auto_whitelist_rule(
            &state,
            format!("discord:{kind}"),
            "always".to_owned(),
            None,
        )
        .unwrap();
    }
    for rule in ["email_address", "email_domain"] {
        cmd_osl_save_auto_whitelist_rule(&state, rule.to_owned(), "always".to_owned(), None)
            .unwrap();
    }

    let revision = revision();
    let mut controls = ProductionAdmissionObserver::default();
    let discord_before =
        run_discord_controls(&state, data.path(), &revision, "before", &mut controls);
    let email_before = run_email_controls(&state, data.path(), &revision, "before", &mut controls);
    println!(
        "TASK0167B_REGISTRIES deserializer={} revision={} discord_count={} discord={} email_count={} email={} signed_in_discord={} supported_email={}",
        PRODUCTION_KIND_DESERIALIZER,
        revision,
        DISCORD_DECLARED_KINDS.len(),
        DISCORD_DECLARED_KINDS.join(","),
        EMAIL_DECLARED_KINDS.len(),
        EMAIL_DECLARED_KINDS.join(","),
        discord_connector.connector_id,
        email_connector.provider_id,
    );
    println!(
        "TASK0167B_CONTROLS phase=before discord={} email={}",
        discord_before.join("|"),
        email_before.join("|")
    );

    let independent_discord_unknown = format!("unknown_{}", random_hex(16));
    let independent_email_unknown = format!("unknown_{}", random_hex(16));
    assert!(!DISCORD_DECLARED_KINDS.contains(&independent_discord_unknown.as_str()));
    assert!(!EMAIL_DECLARED_KINDS.contains(&independent_email_unknown.as_str()));

    let discord_hostiles = [
        (
            "<absent>".to_owned(),
            discord_json(&revision, json!("direct_message"), "absent"),
        ),
        (
            "<empty>".to_owned(),
            discord_json(&revision, json!(""), "empty"),
        ),
        (
            "Direct_Message".to_owned(),
            discord_json(&revision, json!("Direct_Message"), "case"),
        ),
        (
            independent_discord_unknown.clone(),
            discord_json(&revision, json!(independent_discord_unknown), "unknown"),
        ),
        (
            "forum_channel_v2".to_owned(),
            discord_json(&revision, json!("forum_channel_v2"), "future"),
        ),
        (
            "<malformed:object>".to_owned(),
            discord_json(&revision, json!({"nested": "thread"}), "malformed"),
        ),
    ];
    let email_hostiles = [
        (
            "<absent>".to_owned(),
            email_json(&revision, json!("address"), "absent"),
        ),
        (
            "<empty>".to_owned(),
            email_json(&revision, json!(""), "empty"),
        ),
        (
            "Address".to_owned(),
            email_json(&revision, json!("Address"), "case"),
        ),
        (
            independent_email_unknown.clone(),
            email_json(&revision, json!(independent_email_unknown), "unknown"),
        ),
        (
            "conversation_v2".to_owned(),
            email_json(&revision, json!("conversation_v2"), "future"),
        ),
        (
            "<malformed:object>".to_owned(),
            email_json(&revision, json!({"nested": "domain"}), "malformed"),
        ),
    ];

    for (index, (label, mut hostile)) in discord_hostiles.into_iter().enumerate() {
        if index == 0 {
            let mut value: Value = serde_json::from_slice(&hostile).unwrap();
            value.as_object_mut().unwrap().remove("kind");
            hostile = serde_json::to_vec(&value).unwrap();
        }
        spawn_hostile("discord", &revision, &label, &hostile);
    }
    for (index, (label, mut hostile)) in email_hostiles.into_iter().enumerate() {
        if index == 0 {
            let mut value: Value = serde_json::from_slice(&hostile).unwrap();
            value.as_object_mut().unwrap().remove("kind");
            hostile = serde_json::to_vec(&value).unwrap();
        }
        spawn_hostile("email", &revision, &label, &hostile);
    }

    let discord_after =
        run_discord_controls(&state, data.path(), &revision, "after", &mut controls);
    let email_after = run_email_controls(&state, data.path(), &revision, "after", &mut controls);
    println!(
        "TASK0167B_CONTROLS phase=after discord={} email={}",
        discord_after.join("|"),
        email_after.join("|")
    );
    assert_eq!(discord_before, discord_after);
    assert_eq!(email_before, email_after);
    assert_eq!(controls.deserializer_entries, 14);
    assert_eq!(controls.normalized_kinds, 14);
    assert_eq!(controls.rule_lookups, 14);
    assert_eq!(controls.allowed_rows, 14);
    assert_eq!(controls.provider_actions, 14);
    assert_eq!(controls.output_surface_rows, 14);
    assert_eq!(controls.refusals, 0);
    assert_eq!(controls.pending_rows, 0);
    assert_eq!(controls.prompts, 0);
    assert_eq!(controls.notices, 0);
    println!(
        "TASK0167B_CONTROL_COUNTS discord_before=5 discord_after=5 email_before=2 email_after=2 normalized={} rule_lookups={} allowed_rows={} pending_rows={} prompts={} notices={} provider_actions={} output_rows={}",
        controls.normalized_kinds,
        controls.rule_lookups,
        controls.allowed_rows,
        controls.pending_rows,
        controls.prompts,
        controls.notices,
        controls.provider_actions,
        controls.output_surface_rows,
    );
}

#[test]
#[ignore = "TASK 0167b helper exits 1 when the production admission refuses"]
fn task_0167b_hostile_probe() {
    let Ok(carrier) = std::env::var("TASK0167B_HOSTILE_CARRIER") else {
        return;
    };
    let revision = std::env::var("TASK0167B_HOSTILE_REVISION").unwrap();
    let label = std::env::var("TASK0167B_HOSTILE_LABEL").unwrap();
    let provider_json = std::env::var("TASK0167B_HOSTILE_JSON").unwrap();
    let data = tempfile::tempdir().unwrap();
    let state = AppState::new();
    for kind in DISCORD_DECLARED_KINDS {
        cmd_osl_save_auto_whitelist_rule(
            &state,
            format!("discord:{kind}"),
            "always".to_owned(),
            None,
        )
        .unwrap();
    }
    for rule in ["email_address", "email_domain"] {
        cmd_osl_save_auto_whitelist_rule(&state, rule.to_owned(), "always".to_owned(), None)
            .unwrap();
    }
    let mut observer = ProductionAdmissionObserver::default();
    let result = if carrier == "discord" {
        let account = ShippingDiscordProviderRegistry::authoritative()
            .verify_signed_in_account(
                "native_discord",
                DISCORD_ACCOUNT,
                DISCORD_BINDING,
                &revision,
            )
            .unwrap();
        admit_discord_provider_discovery(
            &state,
            data.path(),
            &account,
            provider_json.as_bytes(),
            &mut observer,
        )
    } else {
        let account = ShippingEmailProviderRegistry::authoritative()
            .verify_connected_account("gmail", EMAIL_ACCOUNT, &revision)
            .unwrap();
        admit_email_provider_discovery(
            &state,
            data.path(),
            &account,
            provider_json.as_bytes(),
            &mut observer,
        )
    };
    match result {
        Err(refusal) => {
            assert_eq!(refusal.carrier, carrier);
            assert_eq!(observer.deserializer_entries, 1);
            assert_eq!(observer.refusals, 1);
            if observer.downstream_total() != 0 {
                report_forbidden_action(&carrier, &label, "refused_after_side_effect", &observer);
                std::process::exit(0);
            }
            assert_eq!(observer.downstream_total(), 0);
            assert!(!allowed_places_db_path(data.path()).exists());
            eprintln!(
                "TASK0167B_REFUSED carrier={} raw_kind={} observed_raw={:?} exit_code=1 deserializer_entries={} refusals={} downstream_total={} normalized=0 rule_lookups=0 allowed_rows=0 pending_rows=0 prompts=0 notices=0 provider_actions=0 output_rows=0 reason={}",
                carrier,
                label,
                refusal.raw_kind,
                observer.deserializer_entries,
                observer.refusals,
                observer.downstream_total(),
                refusal.reason,
            );
            std::process::exit(1);
        }
        Ok(receipt) => {
            report_forbidden_action(&carrier, &label, &receipt.outcome, &observer);
            std::process::exit(0);
        }
    }
}
