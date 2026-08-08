//! TASK 3237 / attack 53: Burn while an upload, timed delete, and service
//! delete worker are all alive.
//!
//! Each worker reaches a barrier with a real typed job lease. The saved burn
//! password then runs the shipping durable cleanup. Only after Burn completes
//! are the workers released to attempt their next state-creating/send step.

#![cfg(feature = "task-3234-test")]

use osl_privacy_hub::burn_job_fence::{BurnJobKind, BurnRevoked};
use osl_privacy_hub::cleanup::{task_3235_complete_verified_gate_burn, Task3234PowerCutKeyStore};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::message_expiry::{
    cmd_record_timed_delete_at_path, expire_timed_delete_records_at_path, TimedDeleteProtection,
    TimedDeleteRecord,
};
use osl_privacy_hub::native_attachment_jobs::{
    NativeAttachmentJobRegistry, NativeAttachmentSecrets, NativeAttachmentStage,
};
use osl_privacy_hub::shared_marked_message_deleter::{
    delete_marked_message, SharedMarkedMessage, SharedMarkedMessageRemover, SharedReviewDecision,
};
use osl_privacy_hub::startup_gate::{verify_password_role, VerifiedGateRole};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use store::{MessageStore, StoredMessage};

const KEY: [u8; 32] = [0x37; 32];
const CHANNEL: &str = "task-3237-channel";
const MESSAGE_ID: &str = "task-3237-timed-message";
const MARKER: &[u8] = b"TASK3237-PROTECTED-MARKER";

struct GlobalReset;

impl Drop for GlobalReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

struct ServiceRemover {
    session_path: PathBuf,
}

impl SharedMarkedMessageRemover for ServiceRemover {
    fn service_id(&self) -> &str {
        "discord"
    }

    fn remove_marked_message(&mut self, _message: &SharedMarkedMessage) -> Result<(), String> {
        let parent = self
            .session_path
            .parent()
            .ok_or_else(|| "TASK3237 session path has no parent".to_owned())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::write(&self.session_path, MARKER).map_err(|error| error.to_string())
    }
}

#[test]
fn task_3237_burn_revokes_three_running_jobs_before_any_can_restore_or_send() {
    let _reset = GlobalReset;
    let root = tempfile::tempdir().expect("TASK3237 isolated app root");
    let config = root.path().join("config");
    let local = root.path().join("local");
    let core_dir = config.join("osl-core");
    let staging = local.join("peer-attachment-staging");
    let session = local.join(
        "service-profiles-v2/task-3237-owner/discord/task-3237-account/sessions/signed-in.json",
    );
    let timed_ledger = core_dir.join("timed_delete_records.json");
    let store_dir = core_dir.join("store");
    let upload_state = staging.join("task-3237-upload-complete.oslatt");
    let timed_state = core_dir.join("task-3237-timed-job-finished.json");

    fs::create_dir_all(&staging).expect("TASK3237 staging root");
    fs::create_dir_all(session.parent().unwrap()).expect("TASK3237 signed-in session root");
    fs::write(&upload_state, MARKER).expect("TASK3237 seed protected upload state");
    fs::write(&session, MARKER).expect("TASK3237 seed signed-in service session");

    keystore::set_base_dir_override(Some(core_dir.clone()));
    ipc::main_password::set_main_password(&core_dir, "main-pass-3237")
        .expect("TASK3237 save main password");
    ipc::main_password::set_burn_password(&core_dir, "main-pass-3237", "burn-pass-3237")
        .expect("TASK3237 save burn password");
    ipc::main_password::set_file_storage_key(Some(KEY));

    let store = Arc::new(MessageStore::open(&store_dir, &KEY).expect("TASK3237 message store"));
    store
        .put(&StoredMessage {
            discord_message_id: MESSAGE_ID.to_owned(),
            channel_id: CHANNEL.to_owned(),
            sender_discord_id: "task-3237-sender".to_owned(),
            sender_osl_user_id: "task-3237-owner".to_owned(),
            plaintext: String::from_utf8(MARKER.to_vec()).unwrap(),
            decrypted_at: 1_970_000_000,
            reply_parent_id: None,
            edit_revision: 1,
            burned: false,
        })
        .expect("TASK3237 seed protected timed message");
    cmd_record_timed_delete_at_path(
        &timed_ledger,
        &KEY,
        TimedDeleteRecord {
            app_id: "osl-chat".to_owned(),
            conversation_id: CHANNEL.to_owned(),
            message_locator: MESSAGE_ID.to_owned(),
            sent_at_unix_seconds: 1_970_000_000,
            delete_at_unix_seconds: 1_970_000_060,
            protection: TimedDeleteProtection::Protected,
        },
    )
    .expect("TASK3237 seed timed-delete record");

    let protected_paths = vec![
        upload_state.clone(),
        timed_ledger.clone(),
        store_dir.join("messages.sqlite"),
        session.clone(),
        timed_state.clone(),
    ];
    let protected_before = existing_count(&protected_paths);
    let sessions_before = signed_in_session_count(&local);
    assert!(protected_before > 0);
    assert_eq!(sessions_before, 1);

    let core = HubCoreState::default();
    let ready = Arc::new(Barrier::new(4));
    let release = Arc::new(Barrier::new(4));
    let messages_sent = Arc::new(AtomicUsize::new(0));

    let upload_lease = core
        .burn_jobs
        .start(BurnJobKind::Upload)
        .expect("TASK3237 start upload job");
    let upload_ready = Arc::clone(&ready);
    let upload_release = Arc::clone(&release);
    let upload_messages = Arc::clone(&messages_sent);
    let upload_path = upload_state.clone();
    let upload_worker = thread::spawn(move || {
        let mut jobs = NativeAttachmentJobRegistry::default();
        let staged = jobs
            .stage(
                "task-3237-upload",
                "protected.bin",
                "application/octet-stream",
                MARKER.len() as u64,
                NativeAttachmentSecrets::new(vec![0x37; 16], vec![0x73; 32]).unwrap(),
                1_000,
            )
            .expect("TASK3237 stage upload");
        jobs.begin_protection("task-3237-upload", &staged.job_id, 1_100)
            .unwrap();
        jobs.advance(
            "task-3237-upload",
            &staged.job_id,
            NativeAttachmentStage::Uploading,
            1_200,
        )
        .unwrap();
        upload_ready.wait();
        upload_release.wait();
        upload_lease.run_if_live(|| {
            fs::create_dir_all(upload_path.parent().unwrap()).unwrap();
            fs::write(&upload_path, MARKER).unwrap();
            jobs.advance(
                "task-3237-upload",
                &staged.job_id,
                NativeAttachmentStage::Delivering,
                1_300,
            )
            .unwrap();
            upload_messages.fetch_add(1, Ordering::SeqCst);
        })
    });

    let timed_lease = core
        .burn_jobs
        .start(BurnJobKind::TimedDelete)
        .expect("TASK3237 start timed-delete job");
    let timed_ready = Arc::clone(&ready);
    let timed_release = Arc::clone(&release);
    let timed_store = Arc::clone(&store);
    let timed_ledger_worker = timed_ledger.clone();
    let timed_state_worker = timed_state.clone();
    let timed_worker = thread::spawn(move || {
        timed_ready.wait();
        timed_release.wait();
        timed_lease.run_if_live(|| {
            let _ = expire_timed_delete_records_at_path(
                &timed_ledger_worker,
                &KEY,
                1_970_000_060,
                &timed_store,
            );
            fs::create_dir_all(timed_state_worker.parent().unwrap()).unwrap();
            fs::write(&timed_state_worker, MARKER).unwrap();
        })
    });

    let service_lease = core
        .burn_jobs
        .start(BurnJobKind::ServiceDelete)
        .expect("TASK3237 start service-delete job");
    let service_ready = Arc::clone(&ready);
    let service_release = Arc::clone(&release);
    let service_session = session.clone();
    let service_worker = thread::spawn(move || {
        service_ready.wait();
        service_release.wait();
        service_lease.run_if_live(|| {
            let mut remover = ServiceRemover {
                session_path: service_session,
            };
            delete_marked_message(
                &mut remover,
                "discord",
                Some("task-3237-owner".to_owned()),
                SharedMarkedMessage::new(
                    "discord",
                    "task-3237-service-delete",
                    "discord:dm:task-3237",
                    Some("task-3237-owner".to_owned()),
                    true,
                    SharedReviewDecision::MarkedForDeletion,
                ),
            )
            .unwrap();
        })
    });

    // All workers are alive and parked immediately before their next
    // state-creating/send side effect.
    ready.wait();
    let active_before_burn = core.burn_jobs.active_counts();
    assert_eq!(active_before_burn.upload, 1);
    assert_eq!(active_before_burn.timed_delete, 1);
    assert_eq!(active_before_burn.service_delete, 1);

    let verification = verify_password_role(&core, "burn-pass-3237".to_owned())
        .expect("TASK3237 verify exact saved burn password");
    assert_eq!(verification.role, VerifiedGateRole::Burn);
    let key_store = Task3234PowerCutKeyStore::seeded(true);
    let burn = task_3235_complete_verified_gate_burn(&core, &config, &local, &key_store)
        .expect("TASK3237 complete saved-password Burn");
    assert!(burn.local_cleanup_complete);
    let burn_fence_active = core.burn_jobs.burning();
    assert_eq!(key_store.usable_count(), 0);

    // Burn completed while every worker was still parked and counted active.
    let active_during_burn = core.burn_jobs.active_counts();
    assert_eq!(active_during_burn, active_before_burn);
    assert_eq!(active_during_burn.total(), 3);
    release.wait();

    let upload_result = upload_worker.join().expect("TASK3237 upload worker exits");
    let timed_result = timed_worker
        .join()
        .expect("TASK3237 timed-delete worker exits");
    let service_result = service_worker
        .join()
        .expect("TASK3237 service-delete worker exits");

    let protected_after = existing_count(&protected_paths);
    let restored_by_jobs = usize::from(upload_state.exists())
        + usize::from(timed_state.exists())
        + usize::from(session.exists());
    let sent_after_burn = messages_sent.load(Ordering::SeqCst);
    let sessions_after = signed_in_session_count(&local);
    let active_after = core.burn_jobs.active_counts();

    println!(
        "TASK3237 BEFORE protected_items={} signed_in_service_sessions={} running_upload_jobs={} running_timed_delete_jobs={} running_service_delete_jobs={}",
        protected_before,
        sessions_before,
        active_before_burn.upload,
        active_before_burn.timed_delete,
        active_before_burn.service_delete
    );
    println!(
        "TASK3237 BURN role=burn local_cleanup_complete={} usable_keys_after={} workers_still_running={} burn_fence_active={}",
        burn.local_cleanup_complete,
        key_store.usable_count(),
        active_during_burn.total(),
        burn_fence_active
    );
    println!(
        "TASK3237 AFTER protected_items={} restored_by_jobs={} messages_sent_after_burn={} signed_in_service_sessions={} active_jobs={} upload_result={} timed_delete_result={} service_delete_result={}",
        protected_after,
        restored_by_jobs,
        sent_after_burn,
        sessions_after,
        active_after.total(),
        result_label(&upload_result),
        result_label(&timed_result),
        result_label(&service_result)
    );

    assert!(burn_fence_active);
    assert_eq!(upload_result, Err(BurnRevoked));
    assert_eq!(timed_result, Err(BurnRevoked));
    assert_eq!(service_result, Err(BurnRevoked));
    assert_eq!(protected_after, 0);
    assert_eq!(restored_by_jobs, 0);
    assert_eq!(sent_after_burn, 0);
    assert_eq!(sessions_after, 0);
    assert_eq!(active_after.total(), 0);
    assert!(core.burn_jobs.start(BurnJobKind::Upload).is_err());
}

fn existing_count(paths: &[PathBuf]) -> usize {
    paths.iter().filter(|path| path.exists()).count()
}

fn result_label(result: &Result<(), BurnRevoked>) -> &'static str {
    match result {
        Ok(()) => "completed",
        Err(BurnRevoked) => "burn_revoked",
    }
}

fn signed_in_session_count(root: &Path) -> usize {
    fn walk(path: &Path, inside_sessions: bool) -> usize {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| {
                let path = entry.path();
                let is_sessions = inside_sessions
                    || path.file_name().and_then(|name| name.to_str()) == Some("sessions");
                if path.is_dir() {
                    walk(&path, is_sessions)
                } else {
                    usize::from(is_sessions)
                }
            })
            .sum()
    }
    walk(root, false)
}
