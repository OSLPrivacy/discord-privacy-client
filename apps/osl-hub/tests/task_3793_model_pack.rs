use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use cover_draft::{
    AuthorizedContextEntry, CancellationToken, CoverDraftEngine, DraftRequest, DraftScope, Limits,
};
use osl_privacy_hub::ai_carrier::{ai_carrier_status_for, AiCarrierState};
use osl_privacy_hub::bundled_model_pack::{
    ensure_bundled_model_pack, BundledCoverWriter, MODEL_FILE_NAME,
};

fn request(index: u8) -> DraftRequest<'static> {
    DraftRequest {
        scope: DraftScope {
            account: b"task-3793-account",
            conversation: b"task-3793-conversation",
            recipient: b"task-3793-recipient",
        },
        canonical_message_hash: [index; 32],
        authorized_context: vec![AuthorizedContextEntry::authorize(
            "Recent visible conversation context only.",
        )],
    }
}

#[test]
fn task_3793_builds_ships_verifies_and_uses_the_model_pack_offline() {
    let fresh_install = tempfile::tempdir().expect("fresh install root");
    let status = ensure_bundled_model_pack(fresh_install.path()).expect("bundled pack installs");
    println!(
        "TASK3793 fresh_install.pack_present={} version={} fingerprint={}",
        status.present, status.version, status.fingerprint
    );
    assert!(status.present);
    assert!(status.artifact_path.ends_with(MODEL_FILE_NAME));
    let ai_carrier = AiCarrierState::default();
    ai_carrier
        .ensure_bundled_local_model(fresh_install.path())
        .expect("verified pack makes AI carrier ready");
    assert!(ai_carrier_status_for(&ai_carrier).local_model_ready);

    let tampered_install = tempfile::tempdir().expect("tamper install root");
    let tampered_status =
        ensure_bundled_model_pack(tampered_install.path()).expect("bundled pack installs");
    let mut tampered_bytes = fs::read(&tampered_status.artifact_path).expect("read pack");
    tampered_bytes[0] ^= 0x01;
    fs::write(&tampered_status.artifact_path, tampered_bytes).expect("write tampered pack");

    let changed_pack_loaded_count = AtomicUsize::new(0);
    let refused = match BundledCoverWriter::load(&tampered_status.artifact_path) {
        Ok(_) => {
            changed_pack_loaded_count.fetch_add(1, Ordering::SeqCst);
            panic!("changed pack was loaded");
        }
        Err(error) => error,
    };
    println!(
        "TASK3793 changed_pack.refused_by_name={} changed_pack.loaded_count={}",
        refused.refused_model_name().unwrap_or("<none>"),
        changed_pack_loaded_count.load(Ordering::SeqCst)
    );
    assert_eq!(refused.refused_model_name(), Some(MODEL_FILE_NAME));
    assert_eq!(changed_pack_loaded_count.load(Ordering::SeqCst), 0);

    let mut writer = BundledCoverWriter::load(&status.artifact_path).expect("verified pack loads");
    let engine = CoverDraftEngine::new(Limits::default()).expect("valid limits");
    let mut messages = Vec::new();
    for index in 1..=5 {
        let mut session = engine
            .prepare(&mut writer, request(index), &CancellationToken::default())
            .expect("offline writer prepares a cover message");
        messages.push(session.draft(std::time::Instant::now()).unwrap().to_owned());
    }
    println!(
        "TASK3793 offline.network=disconnected writer.cover_messages={}",
        messages.len()
    );
    assert_eq!(messages.len(), 5);
    assert!(messages.iter().all(|message| !message.trim().is_empty()));
}
