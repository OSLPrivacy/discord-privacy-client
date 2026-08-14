#![cfg(feature = "core")]

use osl_privacy_hub::cleanup::execute_verified_gate_burn;
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::startup_gate::{verify_password_role, VerifiedGateRole};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Instant;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const CHANNEL_ID: &str = "task-3236-protected-channel";
const STORE_KEY: [u8; 32] = [52; 32];
const SAVED_LIMITS: &str = include_str!("fixtures/task_3236/attack_limits.json");

#[derive(Debug, Deserialize)]
struct AttackLimits {
    attempt_duration_limit_ms: f64,
    rapid_submission_count: usize,
    burn_password: String,
    non_exact_attempts: Vec<NonExactAttempt>,
}

#[derive(Debug, Deserialize)]
struct NonExactAttempt {
    label: String,
    password: String,
}

struct Scenario {
    _root: TempDir,
    config_dir: PathBuf,
    local_data_dir: PathBuf,
    core_dir: PathBuf,
    state: HubCoreState,
}

impl Scenario {
    fn reseeded(label: &str, marker: &[u8]) -> Self {
        let root = tempfile::Builder::new()
            .prefix(&format!("osl-task-3236-{label}-"))
            .tempdir()
            .expect("create isolated attack root");
        let config_dir = root.path().join("config");
        let local_data_dir = root.path().join("local");
        let core_dir = config_dir.join("osl-core");
        std::fs::create_dir_all(&local_data_dir).expect("create local-data root");
        std::fs::create_dir_all(&core_dir).expect("create core root");
        std::fs::write(core_dir.join("password_marker.json"), marker)
            .expect("install saved burn-password marker");

        seed_three_protected_messages(&core_dir);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(Some(core_dir.clone()));
        ipc::main_password::set_file_storage_key(None);
        let state = HubCoreState::default();

        Self {
            _root: root,
            config_dir,
            local_data_dir,
            core_dir,
            state,
        }
    }

    fn protected_message_count(&self) -> usize {
        protected_message_count(&self.core_dir)
    }

    fn attempt(&self, candidate: &str) -> (VerifiedGateRole, f64) {
        let started = Instant::now();
        let verification = verify_password_role(&self.state, candidate.to_owned())
            .expect("the production gate returns a role");
        let duration_ms = started.elapsed().as_secs_f64() * 1000.0;
        (verification.role, duration_ms)
    }

    fn burn(&self) {
        let report =
            execute_verified_gate_burn(&self.state, &self.config_dir, &self.local_data_dir, true)
                .expect("the verified burn executes");
        assert!(report.local_cleanup_complete, "{report:?}");
        assert!(report.failed_targets.is_empty(), "{report:?}");
        assert!(
            report.removed_targets.iter().any(|item| item == "hub_core"),
            "the real cleanup must remove the protected-message root: {report:?}"
        );
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn saved_marker(burn_password: &str) -> Vec<u8> {
    let root = tempfile::tempdir().expect("create marker template root");
    let main_password = "Main-password-3236-Owner";
    ipc::main_password::set_main_password(root.path(), main_password).expect("save main password");
    ipc::main_password::set_burn_password(root.path(), main_password, burn_password)
        .expect("save distinct burn password");
    let marker = std::fs::read(root.path().join("password_marker.json"))
        .expect("read saved password marker");
    ipc::main_password::set_file_storage_key(None);
    marker
}

fn seed_three_protected_messages(core_dir: &Path) {
    let store_dir = core_dir.join("store");
    let message_store = MessageStore::open(&store_dir, &STORE_KEY)
        .expect("open the real encrypted protected-message store");
    for index in 0..3 {
        message_store
            .put(&StoredMessage {
                discord_message_id: format!("task-3236-message-{index}"),
                channel_id: CHANNEL_ID.to_owned(),
                sender_discord_id: "task-3236-sender".to_owned(),
                sender_osl_user_id: "osl-task-3236-sender".to_owned(),
                plaintext: format!("task 3236 protected body {index}"),
                decrypted_at: 1_900_003_236 + index,
                reply_parent_id: None,
                edit_revision: 1,
                burned: false,
            })
            .expect("seed protected message");
    }
    assert_eq!(
        message_store
            .count_live_by_channel(CHANNEL_ID, None)
            .expect("count seeded protected messages"),
        3
    );
}

fn protected_message_count(core_dir: &Path) -> usize {
    let store_dir = core_dir.join("store");
    if !store_dir.join("messages.sqlite").is_file() {
        return 0;
    }
    MessageStore::open(&store_dir, &STORE_KEY)
        .expect("reopen protected-message store for count")
        .count_live_by_channel(CHANNEL_ID, None)
        .expect("count protected messages")
}

fn assert_duration(label: &str, duration_ms: f64, saved_limit_ms: f64) {
    println!(
        "TASK3236_ATTEMPT label={label} duration_ms={duration_ms:.3} saved_limit_ms={saved_limit_ms:.3} within_limit={}",
        duration_ms <= saved_limit_ms
    );
    assert!(
        duration_ms <= saved_limit_ms,
        "TASK3236 duration exceeded saved limit: label={label} duration_ms={duration_ms:.3} saved_limit_ms={saved_limit_ms:.3}"
    );
}

#[test]
fn task_3236_exact_only_burns_reseeded_three_message_stores() {
    let limits: AttackLimits =
        serde_json::from_str(SAVED_LIMITS).expect("parse saved task-3236 timing limits");
    assert_eq!(limits.non_exact_attempts.len(), 4);
    assert!(limits.rapid_submission_count > 1);
    let marker = saved_marker(&limits.burn_password);

    let first_exact = Scenario::reseeded("first-exact", &marker);
    let first_before = first_exact.protected_message_count();
    let (first_role, first_duration_ms) = first_exact.attempt(&limits.burn_password);
    assert_eq!(first_role, VerifiedGateRole::Burn);
    first_exact.burn();
    let first_after = first_exact.protected_message_count();
    assert_eq!((first_before, first_after), (3, 0));
    assert_duration(
        "first_exact_burn",
        first_duration_ms,
        limits.attempt_duration_limit_ms,
    );
    println!("TASK3236_EXACT phase=first role=burn before={first_before} after={first_after}");
    drop(first_exact);

    for attack in &limits.non_exact_attempts {
        let scenario = Scenario::reseeded(&attack.label, &marker);
        let before = scenario.protected_message_count();
        let (role, duration_ms) = scenario.attempt(&attack.password);
        let after = scenario.protected_message_count();
        assert_eq!(role, VerifiedGateRole::Wrong, "{}", attack.label);
        assert_eq!((before, after), (3, 3), "{}", attack.label);
        assert_duration(&attack.label, duration_ms, limits.attempt_duration_limit_ms);
        println!(
            "TASK3236_REFUSED label={} role=wrong before={before} after={after}",
            attack.label
        );
    }

    let rapid = Scenario::reseeded("rapid-non-exact", &marker);
    let rapid_before = rapid.protected_message_count();
    let rapid_batch_started = Instant::now();
    for index in 0..limits.rapid_submission_count {
        let candidate = format!("{}-rapid-non-exact-{index}", limits.burn_password);
        let (role, duration_ms) = rapid.attempt(&candidate);
        let after = rapid.protected_message_count();
        assert_eq!(role, VerifiedGateRole::Wrong, "rapid attempt {index}");
        assert_eq!(after, 3, "rapid attempt {index} changed the store");
        assert_duration(
            &format!("rapid_non_exact_{index}"),
            duration_ms,
            limits.attempt_duration_limit_ms,
        );
        println!(
            "TASK3236_RAPID index={index} role=wrong after={after} duration_ms={duration_ms:.3} saved_limit_ms={:.3}",
            limits.attempt_duration_limit_ms
        );
    }
    let rapid_batch_duration_ms = rapid_batch_started.elapsed().as_secs_f64() * 1000.0;
    let rapid_after = rapid.protected_message_count();
    assert_eq!((rapid_before, rapid_after), (3, 3));
    println!(
        "TASK3236_RAPID_SUMMARY submissions={} before={rapid_before} after={rapid_after} batch_duration_ms={rapid_batch_duration_ms:.3}",
        limits.rapid_submission_count
    );
    drop(rapid);

    let final_exact = Scenario::reseeded("final-exact", &marker);
    let final_before = final_exact.protected_message_count();
    let (final_role, final_duration_ms) = final_exact.attempt(&limits.burn_password);
    assert_eq!(final_role, VerifiedGateRole::Burn);
    final_exact.burn();
    let final_after = final_exact.protected_message_count();
    assert_eq!((final_before, final_after), (3, 0));
    assert_duration(
        "final_exact_burn",
        final_duration_ms,
        limits.attempt_duration_limit_ms,
    );
    println!("TASK3236_EXACT phase=final role=burn before={final_before} after={final_after}");
    println!(
        "TASK3236_FINISH first=3->0 refused_cases={} rapid_submissions={} rapid=3->3 final=3->0",
        limits.non_exact_attempts.len(),
        limits.rapid_submission_count
    );
}
