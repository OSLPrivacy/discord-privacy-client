use keystore::{
    generate_identity, save_identity, save_password_record, save_prekey_state, Argon2Params,
    DuressEngine, DuressError, DuressHandlers, DuressJournal, DuressPaths, NoOpSealer,
    PasswordRecord, PrekeyConfig, PrekeyState, StepOutcome, TpmEvictOutcome, WipeStep,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

const CRASH_AFTER_JOURNAL_WRITE_ENV: &str = "OSL_DURESS_TEST_CRASH_AFTER_JOURNAL_WRITE";
const CRASH_AFTER_JOURNAL_WRITE_EXIT: i32 = 73;

fn fast() -> Argon2Params {
    Argon2Params::fast_for_tests()
}

fn build_paths(dir: &TempDir) -> (DuressPaths, std::path::PathBuf) {
    let identity_file = dir.path().join("identity.json");
    let password_file = dir.path().join("password.json");
    let prekey_file = dir.path().join("prekeys.json");
    let journal_file = dir.path().join("duress.journal");
    (
        DuressPaths {
            identity_file,
            password_file,
            prekey_file: Some(prekey_file),
        },
        journal_file,
    )
}

fn build_paths_without_prekey(dir: &TempDir) -> (DuressPaths, std::path::PathBuf) {
    let identity_file = dir.path().join("identity.json");
    let password_file = dir.path().join("password.json");
    let journal_file = dir.path().join("duress.journal");
    (
        DuressPaths {
            identity_file,
            password_file,
            prekey_file: None,
        },
        journal_file,
    )
}

fn write_journal_with_all_steps_except(journal_path: &std::path::Path, except: WipeStep) {
    let completed = WipeStep::ordered()
        .iter()
        .copied()
        .filter(|step| *step != except)
        .map(|step| {
            (
                step,
                StepOutcome::Skipped {
                    reason: "prefilled test step".to_string(),
                },
            )
        })
        .collect();
    let journal = DuressJournal {
        completed,
        started_at_unix_seconds: 0,
    };
    std::fs::write(journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
}

fn exit_after_writing_mid_wipe_journal_if_requested() {
    let Some(root) = std::env::var_os(CRASH_AFTER_JOURNAL_WRITE_ENV) else {
        return;
    };
    let journal_path = std::path::PathBuf::from(root).join("duress.journal");
    let journal = DuressJournal {
        completed: vec![(WipeStep::TpmEvict, StepOutcome::AlreadyClean)],
        started_at_unix_seconds: 0,
    };
    std::fs::write(journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
    std::process::exit(CRASH_AFTER_JOURNAL_WRITE_EXIT);
}

#[test]
fn execute_with_no_handlers_walks_every_step() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    // Pre-populate the on-disk identity + password + prekey blob so
    // the file-deletion steps actually have something to delete.
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();
    let pw = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&paths.password_file, &pw, &sealer).unwrap();
    let prekey_state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    save_prekey_state(paths.prekey_file.as_ref().unwrap(), &prekey_state, &sealer).unwrap();
    assert!(paths.identity_file.exists());
    assert!(paths.password_file.exists());
    assert!(paths.prekey_file.as_ref().unwrap().exists());

    let prekey_path = paths.prekey_file.clone().unwrap();
    let engine = DuressEngine::new(journal_path.clone(), paths, DuressHandlers::default());
    let report = engine.execute().unwrap();

    // Every step in the canonical order must appear.
    let want_order: Vec<WipeStep> = WipeStep::ordered().to_vec();
    let got_order: Vec<WipeStep> = report.steps.iter().map(|(s, _)| *s).collect();
    assert_eq!(got_order, want_order);

    // Wired-today file-deletion steps must `Wiped`.
    assert_eq!(
        outcome_for(&report.steps, WipeStep::IdentityFile),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PasswordHashes),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PrekeyFile),
        &StepOutcome::Wiped
    );

    // Files are gone from disk.
    assert!(!dir.path().join("identity.json").exists());
    assert!(!dir.path().join("password.json").exists());
    assert!(!prekey_path.exists());

    // Each deferred handler step is `Skipped` with a non-empty reason.
    for s in [
        WipeStep::UnregisterAccount,
        WipeStep::LocalCacheDir,
        WipeStep::AnonymousCredentials,
        WipeStep::Prekeys,
        WipeStep::DoubleRatchet,
        WipeStep::SenderKeys,
        WipeStep::PeerRatchets,
        WipeStep::InMemoryZeroize,
        WipeStep::StripOpsecFiles,
    ] {
        match outcome_for(&report.steps, s) {
            StepOutcome::Skipped { reason } => {
                assert!(!reason.is_empty(), "skipped step {s:?} must carry a reason");
                let lc = reason.to_lowercase();
                assert!(
                    !lc.contains("unimplemented") && !lc.contains("todo"),
                    "skip reason for {s:?} must not look like a silent defer: {reason}"
                );
                // Must not point at a layer that has already landed.
                // (B4 has landed; messages must not say "wired by B4".)
                assert!(
                    !lc.contains("layer b4"),
                    "skip reason for {s:?} still cites a landed layer: {reason}"
                );
            }
            other => panic!("step {s:?} expected Skipped, got {other:?}"),
        }
    }
}

#[test]
fn prekey_file_step_skipped_when_path_not_supplied() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths_without_prekey(&dir);
    let engine = DuressEngine::new(journal_path, paths, DuressHandlers::default());
    let report = engine.execute().unwrap();
    let outcome = outcome_for(&report.steps, WipeStep::PrekeyFile);
    match outcome {
        StepOutcome::Skipped { reason } => {
            assert!(reason.contains("DuressPaths::prekey_file"));
        }
        other => panic!("expected Skipped, got {other:?}"),
    }
}

#[test]
fn prekey_file_step_already_clean_when_file_missing() {
    // Path supplied but no file on disk — the engine reports
    // `AlreadyClean` (idempotent file-deletion semantic).
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    assert!(!paths.prekey_file.as_ref().unwrap().exists());
    let engine = DuressEngine::new(journal_path, paths, DuressHandlers::default());
    let report = engine.execute().unwrap();
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PrekeyFile),
        &StepOutcome::AlreadyClean
    );
}

#[test]
fn idempotent_execute_can_be_called_twice() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();

    let engine = DuressEngine::new(journal_path.clone(), paths, DuressHandlers::default());
    let r1 = engine.execute().unwrap();
    assert!(r1.completed);
    // Second execute on a clean state — no journal exists, so a
    // fresh run starts. All file-deletion steps see "AlreadyClean".
    let r2 = engine.execute().unwrap();
    assert!(r2.completed);
    let id_outcome = outcome_for(&r2.steps, WipeStep::IdentityFile);
    assert_eq!(id_outcome, &StepOutcome::AlreadyClean);
}

#[test]
fn missing_files_yield_already_clean_not_failure() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    // Don't pre-create any files.
    let engine = DuressEngine::new(journal_path, paths, DuressHandlers::default());
    let report = engine.execute().unwrap();
    assert_eq!(
        outcome_for(&report.steps, WipeStep::IdentityFile),
        &StepOutcome::AlreadyClean
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PasswordHashes),
        &StepOutcome::AlreadyClean
    );
    assert!(report.failed_steps().is_empty());
}

#[test]
fn handlers_run_in_canonical_order() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);

    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mk_handler = |name: &'static str| {
        let calls = calls.clone();
        Box::new(move || {
            calls.lock().unwrap().push(name);
            Ok(())
        }) as keystore::WipeFn
    };

    let handlers = DuressHandlers {
        evict_tpm_key: Some(mk_tpm_evict_handler("tpm_evict", calls.clone())),
        purge_keyring_entry: Some(mk_keyring_purge_handler("keyring_purge", calls.clone())),
        wipe_local_cache_dir: Some(mk_handler("local_cache")),
        wipe_anonymous_credentials: Some(mk_handler("creds")),
        wipe_prekeys: Some(mk_handler("prekeys")),
        wipe_double_ratchet: Some(mk_handler("ratchet")),
        wipe_sender_keys: Some(mk_handler("sender_keys")),
        wipe_peer_ratchets: Some(mk_handler("peer_ratchets")),
        zeroize_in_memory: Some(mk_handler("zeroize")),
        strip_opsec_files: Some(mk_handler("strip")),
        unregister_account: Some(mk_handler("unregister")),
    };

    let engine = DuressEngine::new(journal_path, paths, handlers);
    let report = engine.execute().unwrap();
    assert!(report.completed);
    assert!(report.failed_steps().is_empty());
    assert!(report.skipped_steps().is_empty());

    let calls = calls.lock().unwrap();
    assert_eq!(
        *calls,
        vec![
            "tpm_evict",
            "keyring_purge",
            "unregister",
            "local_cache",
            "creds",
            "prekeys",
            "ratchet",
            "sender_keys",
            "peer_ratchets",
            "zeroize",
            "strip",
        ]
    );
}

#[test]
fn production_duress_config_wires_local_cache_and_opsec_paths() {
    let dir = TempDir::new().unwrap();
    let account_dir = dir.path().join("account");
    let password_dir = dir.path().join("device");
    let opsec_file = account_dir.join("injection.js");
    let opsec_dir = account_dir.join("opsec");
    std::fs::create_dir_all(account_dir.join("store")).unwrap();
    std::fs::create_dir_all(&opsec_dir).unwrap();
    std::fs::create_dir_all(&password_dir).unwrap();
    std::fs::write(account_dir.join("store").join("message-cache"), b"cache").unwrap();
    std::fs::write(&opsec_file, b"opsec").unwrap();
    std::fs::write(opsec_dir.join("config.json"), b"{}").unwrap();

    let mut config =
        keystore::ProductionDuressConfig::new(account_dir.clone(), password_dir.clone());
    config.opsec_paths = vec![opsec_file.clone(), opsec_dir.clone()];

    let parts = keystore::build_production_duress_handlers(config);
    assert_eq!(parts.paths.identity_file, account_dir.join("identity.json"));
    assert_eq!(
        parts.paths.password_file,
        password_dir.join("password_marker.json")
    );
    assert_eq!(
        parts.paths.prekey_file,
        Some(account_dir.join("prekeys.json"))
    );
    assert_eq!(parts.journal_path, account_dir.join("duress.journal"));

    let local_cache_handler = parts.handlers.wipe_local_cache_dir.as_ref().unwrap();
    local_cache_handler().unwrap();
    let opsec_handler = parts.handlers.strip_opsec_files.as_ref().unwrap();
    opsec_handler().unwrap();

    assert!(!account_dir.join("store").exists());
    assert!(!opsec_file.exists());
    assert!(!opsec_dir.exists());
}

#[test]
fn keyring_purge_entry_handler_runs_for_pending_keyring_step() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    write_journal_with_all_steps_except(&journal_path, WipeStep::KeyringPurge);

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_cb = calls.clone();
    let handlers = DuressHandlers {
        purge_keyring_entry: Some(Box::new(move || {
            calls_for_cb.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })),
        ..Default::default()
    };

    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    let report = engine.execute().unwrap();

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "KeyringPurge must call DuressHandlers::purge_keyring_entry"
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::KeyringPurge),
        &StepOutcome::Wiped
    );
    assert!(
        !journal_path.exists(),
        "successful keyring purge handler run must clear the journal"
    );
}

#[test]
fn tpm_evict_and_keyring_purge_handlers_compose() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);

    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let handlers = DuressHandlers {
        evict_tpm_key: Some(mk_tpm_evict_handler("tpm_evict", calls.clone())),
        purge_keyring_entry: Some(mk_keyring_purge_handler("keyring_purge", calls.clone())),
        ..Default::default()
    };

    let engine = DuressEngine::new(journal_path, paths, handlers);
    let report = engine.execute().unwrap();

    assert_eq!(
        outcome_for(&report.steps, WipeStep::TpmEvict),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::KeyringPurge),
        &StepOutcome::Wiped
    );
    assert!(
        report.failed_steps().is_empty(),
        "composed platform handlers must not record failures"
    );

    let calls = calls.lock().unwrap();
    assert_eq!(*calls, vec!["tpm_evict", "keyring_purge"]);
}

#[test]
fn failing_handler_records_failure_but_continues() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);

    let later_calls = Arc::new(AtomicUsize::new(0));
    let later_for_cb = later_calls.clone();

    let handlers = DuressHandlers {
        wipe_prekeys: Some(Box::new(|| {
            Err(keystore::DuressError::Handler("boom".into()))
        })),
        zeroize_in_memory: Some(Box::new(move || {
            later_for_cb.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })),
        ..Default::default()
    };

    let engine = DuressEngine::new(journal_path, paths, handlers);
    let report = engine.execute().unwrap();
    let prekey_outcome = outcome_for(&report.steps, WipeStep::Prekeys);
    assert!(matches!(prekey_outcome, StepOutcome::Failed { error } if error.contains("boom")));
    assert_eq!(
        later_calls.load(Ordering::SeqCst),
        1,
        "engine must continue after a failed step"
    );
    // `completed` is true (every step has an outcome), but
    // `failed_steps()` lists the prekey failure.
    let failed = report.failed_steps();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, WipeStep::Prekeys);
}

#[test]
fn resume_with_no_journal_returns_none() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let engine = DuressEngine::new(journal_path, paths, DuressHandlers::default());
    let r = engine.resume_if_pending().unwrap();
    assert!(r.is_none());
}

#[test]
fn resume_picks_up_partial_journal() {
    // Manually craft a journal file that records the first three
    // steps as completed, then resume — only the remaining steps
    // should run.
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();

    let prefilled = serde_json::json!({
        "completed": [
            ["tpm_evict", "wiped"],
            ["keyring_purge", "wiped"],
            ["identity_file", "wiped"]
        ],
        "started_at_unix_seconds": 0
    });
    std::fs::write(
        &journal_path,
        serde_json::to_vec_pretty(&prefilled).unwrap(),
    )
    .unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_cb = calls.clone();
    let handlers = DuressHandlers {
        zeroize_in_memory: Some(Box::new(move || {
            calls_for_cb.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })),
        ..Default::default()
    };

    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    let report = engine.resume_if_pending().unwrap().expect("resume ran");
    assert!(report.completed);
    // Identity file deletion step was already recorded — engine must
    // NOT re-run it (the file we just saved must still exist).
    // (It does still exist because we re-saved after journal write.)
    // Actually wait: in this test we DID save the identity file
    // *after* writing the journal. The engine reads the journal,
    // sees IdentityFile is already done, and skips it. So the file
    // is still on disk. Verify.
    assert!(
        dir.path().join("identity.json").exists(),
        "engine must respect the journal and skip already-completed steps"
    );
    // Zeroize handler did fire (it wasn't in the journal).
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn resume_pending_journal_completes_remaining_destructive_steps() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();
    let pw = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&paths.password_file, &pw, &sealer).unwrap();
    let prekey_state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    let prekey_path = paths.prekey_file.clone().unwrap();
    save_prekey_state(&prekey_path, &prekey_state, &sealer).unwrap();

    let local_cache = dir.path().join("local-cache");
    let anonymous_store = dir.path().join("anonymous-credential.token");
    let opsec_file = dir.path().join("injection-config.js");
    std::fs::create_dir_all(&local_cache).unwrap();
    std::fs::write(local_cache.join("message-cache"), b"ciphertext").unwrap();
    std::fs::write(&anonymous_store, b"token").unwrap();
    std::fs::write(&opsec_file, b"config").unwrap();

    std::fs::remove_file(&paths.identity_file).unwrap();
    let journal = DuressJournal {
        completed: vec![
            (WipeStep::TpmEvict, StepOutcome::AlreadyClean),
            (WipeStep::KeyringPurge, StepOutcome::Wiped),
            (WipeStep::IdentityFile, StepOutcome::Wiped),
        ],
        started_at_unix_seconds: 1_700_000_001,
    };
    std::fs::write(&journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();

    let remove_path = |path: std::path::PathBuf| {
        Box::new(move || match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.is_dir() => {
                std::fs::remove_dir_all(&path).map_err(|e| DuressError::Io(e.to_string()))
            }
            Ok(_) => std::fs::remove_file(&path).map_err(|e| DuressError::Io(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DuressError::Io(e.to_string())),
        }) as keystore::WipeFn
    };
    let callbacks = Arc::new(AtomicUsize::new(0));
    let counted = |callbacks: Arc<AtomicUsize>| {
        Box::new(move || {
            callbacks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }) as keystore::WipeFn
    };
    let handlers = DuressHandlers {
        wipe_local_cache_dir: Some(remove_path(local_cache.clone())),
        wipe_anonymous_credentials: Some(remove_path(anonymous_store.clone())),
        wipe_prekeys: Some(counted(callbacks.clone())),
        wipe_double_ratchet: Some(counted(callbacks.clone())),
        wipe_sender_keys: Some(counted(callbacks.clone())),
        wipe_peer_ratchets: Some(counted(callbacks.clone())),
        zeroize_in_memory: Some(counted(callbacks.clone())),
        strip_opsec_files: Some(remove_path(opsec_file.clone())),
        ..Default::default()
    };

    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    let report = engine.resume_if_pending().unwrap().expect("resume ran");

    assert!(report.completed);
    assert!(report.failed_steps().is_empty());
    assert_eq!(
        outcome_for(&report.steps, WipeStep::IdentityFile),
        &StepOutcome::Wiped,
        "completed journal entries must not be replayed as already-clean"
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PasswordHashes),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PrekeyFile),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::LocalCacheDir),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::AnonymousCredentials),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::StripOpsecFiles),
        &StepOutcome::Wiped
    );
    assert_eq!(callbacks.load(Ordering::SeqCst), 5);
    assert!(!dir.path().join("identity.json").exists());
    assert!(!dir.path().join("password.json").exists());
    assert!(!prekey_path.exists());
    assert!(!local_cache.exists());
    assert!(!anonymous_store.exists());
    assert!(!opsec_file.exists());
    assert!(!journal_path.exists());
}

#[test]
fn crash_mid_wipe_resume() {
    exit_after_writing_mid_wipe_journal_if_requested();

    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();
    let pw = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&paths.password_file, &pw, &sealer).unwrap();
    let prekey_state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    save_prekey_state(paths.prekey_file.as_ref().unwrap(), &prekey_state, &sealer).unwrap();

    let identity_file = paths.identity_file.clone();
    let password_file = paths.password_file.clone();
    let prekey_file = paths.prekey_file.clone().unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("crash_mid_wipe_resume")
        .arg("--nocapture")
        .env(CRASH_AFTER_JOURNAL_WRITE_ENV, dir.path())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(CRASH_AFTER_JOURNAL_WRITE_EXIT));
    assert!(journal_path.exists());
    let journal: DuressJournal =
        serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
    assert_eq!(
        journal.completed,
        vec![(WipeStep::TpmEvict, StepOutcome::AlreadyClean)],
        "helper must die immediately after a durable mid-wipe journal write"
    );
    assert!(identity_file.exists());
    assert!(password_file.exists());
    assert!(prekey_file.exists());

    let handlers = DuressHandlers {
        evict_tpm_key: Some(Box::new(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))),
        purge_keyring_entry: Some(Box::new(|| Ok(()))),
        wipe_local_cache_dir: Some(Box::new(|| Ok(()))),
        wipe_anonymous_credentials: Some(Box::new(|| Ok(()))),
        wipe_prekeys: Some(Box::new(|| Ok(()))),
        wipe_double_ratchet: Some(Box::new(|| Ok(()))),
        wipe_sender_keys: Some(Box::new(|| Ok(()))),
        wipe_peer_ratchets: Some(Box::new(|| Ok(()))),
        zeroize_in_memory: Some(Box::new(|| Ok(()))),
        strip_opsec_files: Some(Box::new(|| Ok(()))),
        unregister_account: Some(Box::new(|| Ok(()))),
    };
    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    let report = engine.resume_if_pending().unwrap().expect("resume ran");

    assert!(report.completed);
    assert!(report.failed_steps().is_empty());
    assert_eq!(
        outcome_for(&report.steps, WipeStep::IdentityFile),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PasswordHashes),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::PrekeyFile),
        &StepOutcome::Wiped
    );
    assert!(!identity_file.exists());
    assert!(!password_file.exists());
    assert!(!prekey_file.exists());
    assert!(
        !journal_path.exists(),
        "successful crash recovery must clear the pending journal"
    );
}

#[test]
fn successful_run_removes_journal() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let handlers = DuressHandlers {
        evict_tpm_key: Some(Box::new(|| Ok(TpmEvictOutcome::NoTpmNothingToEvict))),
        purge_keyring_entry: Some(Box::new(|| Ok(()))),
        wipe_local_cache_dir: Some(Box::new(|| Ok(()))),
        wipe_anonymous_credentials: Some(Box::new(|| Ok(()))),
        wipe_prekeys: Some(Box::new(|| Ok(()))),
        wipe_double_ratchet: Some(Box::new(|| Ok(()))),
        wipe_sender_keys: Some(Box::new(|| Ok(()))),
        wipe_peer_ratchets: Some(Box::new(|| Ok(()))),
        zeroize_in_memory: Some(Box::new(|| Ok(()))),
        strip_opsec_files: Some(Box::new(|| Ok(()))),
        unregister_account: Some(Box::new(|| Ok(()))),
    };
    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    engine.execute().unwrap();
    assert!(
        !journal_path.exists(),
        "successful run must remove the journal so the next launch \
         doesn't think duress is in progress"
    );
}

#[test]
fn failing_run_retains_journal_for_resume() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let handlers = DuressHandlers {
        wipe_prekeys: Some(Box::new(|| {
            Err(keystore::DuressError::Handler("boom".into()))
        })),
        ..Default::default()
    };
    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    let report = engine.execute().unwrap();
    assert!(!report.failed_steps().is_empty());
    assert!(
        journal_path.exists(),
        "journal must remain so a future relaunch can resume"
    );
}

#[test]
fn report_helpers_classify_outcomes() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let engine = DuressEngine::new(journal_path, paths, DuressHandlers::default());
    let report = engine.execute().unwrap();
    let skipped = report.skipped_steps();
    assert!(skipped.contains(&WipeStep::UnregisterAccount));
    assert!(skipped.contains(&WipeStep::Prekeys));
    assert!(skipped.contains(&WipeStep::DoubleRatchet));
    assert!(skipped.contains(&WipeStep::SenderKeys));
    assert!(report.failed_steps().is_empty());
}

#[test]
fn wipe_step_ordered_covers_all_variants() {
    // Sanity: ensure every variant is in WipeStep::ordered().
    use std::collections::HashSet;
    let listed: HashSet<_> = WipeStep::ordered().iter().copied().collect();
    let expected = [
        WipeStep::TpmEvict,
        WipeStep::KeyringPurge,
        WipeStep::IdentityFile,
        WipeStep::PasswordHashes,
        WipeStep::UnregisterAccount,
        WipeStep::PrekeyFile,
        WipeStep::LocalCacheDir,
        WipeStep::AnonymousCredentials,
        WipeStep::Prekeys,
        WipeStep::DoubleRatchet,
        WipeStep::SenderKeys,
        WipeStep::PeerRatchets,
        WipeStep::InMemoryZeroize,
        WipeStep::StripOpsecFiles,
    ];
    for s in expected {
        assert!(listed.contains(&s), "WipeStep::ordered missing {s:?}");
    }
    assert_eq!(listed.len(), expected.len());
}

fn outcome_for(steps: &[(WipeStep, StepOutcome)], target: WipeStep) -> &StepOutcome {
    &steps
        .iter()
        .find(|(s, _)| *s == target)
        .unwrap_or_else(|| panic!("step {target:?} missing from report"))
        .1
}

fn mk_tpm_evict_handler(
    name: &'static str,
    calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
) -> keystore::TpmEvictFn {
    Box::new(move || {
        calls.lock().unwrap().push(name);
        Ok(TpmEvictOutcome::Evicted)
    })
}

fn mk_keyring_purge_handler(
    name: &'static str,
    calls: Arc<std::sync::Mutex<Vec<&'static str>>>,
) -> keystore::KeyringPurgeFn {
    Box::new(move || {
        calls.lock().unwrap().push(name);
        Ok(())
    })
}
