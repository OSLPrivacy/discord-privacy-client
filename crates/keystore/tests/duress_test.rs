use keystore::{
    generate_identity, save_identity, save_password_record, save_prekey_state, Argon2Params,
    DuressEngine, DuressHandlers, DuressJournal, DuressPaths, NoOpSealer, PasswordRecord,
    PrekeyConfig, PrekeyState, ProductionDuressHandlers, StepOutcome, WipeStep,
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
fn production_duress_wipes_cache_prekeys_and_identity_files() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();
    let prekey_state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    save_prekey_state(paths.prekey_file.as_ref().unwrap(), &prekey_state, &sealer).unwrap();

    let identity_file = paths.identity_file.clone();
    let prekey_file = paths.prekey_file.clone().unwrap();
    let cache_dir = dir.path().join("local-cache");
    std::fs::create_dir(&cache_dir).unwrap();
    std::fs::write(cache_dir.join("sealed-cache-record"), b"cache").unwrap();

    let handlers = ProductionDuressHandlers::new()
        .with_purge_keyring(Box::new(|| Ok(())))
        .with_wipe_local_cache_dir_path(cache_dir.clone())
        .into_handlers();
    let engine = DuressEngine::new(journal_path, paths, handlers);

    let report = engine.execute().unwrap();

    assert_eq!(
        outcome_for(&report.steps, WipeStep::IdentityFile),
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
    assert!(!identity_file.exists());
    assert!(!prekey_file.exists());
    assert!(!cache_dir.exists());
}

#[test]
fn handlers_run_in_canonical_order() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();
    let pw = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&paths.password_file, &pw, &sealer).unwrap();
    let prekey_state = PrekeyState::new(&id, PrekeyConfig::default(), 1_700_000_000);
    save_prekey_state(paths.prekey_file.as_ref().unwrap(), &prekey_state, &sealer).unwrap();
    let cache_dir = dir.path().join("local-cache");
    std::fs::create_dir(&cache_dir).unwrap();
    std::fs::write(cache_dir.join("sealed-cache-record"), b"cache").unwrap();

    let identity_file = paths.identity_file.clone();
    let password_file = paths.password_file.clone();
    let prekey_file = paths.prekey_file.clone().unwrap();

    let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mk_handler = |name: &'static str| {
        let calls = calls.clone();
        Box::new(move || {
            calls.lock().unwrap().push(name);
            Ok(())
        }) as keystore::WipeFn
    };
    let mk_cache_handler = {
        let calls = calls.clone();
        let cache_dir = cache_dir.clone();
        Box::new(move || {
            calls.lock().unwrap().push("local_cache");
            std::fs::remove_dir_all(&cache_dir)?;
            Ok(())
        }) as keystore::WipeFn
    };

    let handlers = ProductionDuressHandlers::new()
        .with_purge_keyring(mk_handler("keyring"))
        .with_unregister_account(mk_handler("unregister"))
        .with_wipe_local_cache_dir(mk_cache_handler)
        .with_wipe_anonymous_credentials(mk_handler("creds"))
        .with_wipe_prekeys(mk_handler("prekeys"))
        .with_wipe_double_ratchet(mk_handler("ratchet"))
        .with_wipe_sender_keys(mk_handler("sender_keys"))
        .with_wipe_peer_ratchets(mk_handler("peer_ratchets"))
        .with_zeroize_in_memory(mk_handler("zeroize"))
        .with_strip_opsec_files(mk_handler("strip"))
        .into_handlers();

    let engine = DuressEngine::new(journal_path, paths, handlers);
    let report = engine.execute().unwrap();
    assert!(report.completed);
    assert!(report.failed_steps().is_empty());
    assert!(report.skipped_steps().is_empty());
    assert_eq!(
        report
            .steps
            .iter()
            .map(|(step, _)| *step)
            .collect::<Vec<_>>(),
        vec![
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
        ]
    );

    let calls = calls.lock().unwrap();
    assert_eq!(
        *calls,
        vec![
            "keyring",
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
    assert!(!identity_file.exists());
    assert!(!password_file.exists());
    assert!(!prekey_file.exists());
    assert!(!cache_dir.exists());
}

#[test]
fn keyring_purge_handler() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    write_journal_with_all_steps_except(&journal_path, WipeStep::KeyringPurge);

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_cb = calls.clone();
    let handlers = DuressHandlers {
        purge_keyring: Some(Box::new(move || {
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
        "KeyringPurge must call DuressHandlers::purge_keyring"
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
    let calls_for_keyring = calls.clone();
    let calls_for_unregister = calls.clone();
    let handlers = DuressHandlers {
        purge_keyring: Some(Box::new(move || {
            calls_for_keyring.lock().unwrap().push("keyring_purge");
            Ok(())
        })),
        unregister_account: Some(Box::new(move || {
            calls_for_unregister
                .lock()
                .unwrap()
                .push("unregister_account");
            Err(keystore::DuressError::Handler(
                "intentional failure after TPM/keyring composition proof".into(),
            ))
        })),
        ..Default::default()
    };

    let engine = DuressEngine::new(journal_path.clone(), paths, handlers);
    let report = engine.execute().unwrap();

    let tpm_step = report.steps.first().expect("TPM eviction step missing");
    let keyring_step = report.steps.get(1).expect("keyring purge step missing");
    assert_eq!(tpm_step.0, WipeStep::TpmEvict);
    assert_eq!(keyring_step, &(WipeStep::KeyringPurge, StepOutcome::Wiped));
    assert_eq!(
        *calls.lock().unwrap(),
        vec!["keyring_purge", "unregister_account"],
        "keyring purge must run exactly once after the TPM eviction step"
    );
    assert!(
        matches!(
            &tpm_step.1,
            StepOutcome::Wiped | StepOutcome::AlreadyClean | StepOutcome::Failed { .. }
        ),
        "TPM eviction must be attempted and recorded as a terminal outcome"
    );
    assert!(
        matches!(
            outcome_for(&report.steps, WipeStep::UnregisterAccount),
            StepOutcome::Failed { error } if error.contains("intentional failure")
        ),
        "the later failure keeps the journal available for composition checks"
    );
    assert!(
        journal_path.exists(),
        "a later failure must retain the journal with the TPM and keyring outcomes"
    );

    let journal: DuressJournal =
        serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap();
    assert_eq!(journal.completed, report.steps);
    assert_eq!(
        &journal.completed[..2],
        &[
            (WipeStep::TpmEvict, tpm_step.1.clone()),
            (WipeStep::KeyringPurge, StepOutcome::Wiped),
        ],
        "journal order must preserve TPM eviction before keyring purge"
    );
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
        purge_keyring: Some(Box::new(|| Ok(()))),
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
        purge_keyring: Some(Box::new(|| Ok(()))),
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
