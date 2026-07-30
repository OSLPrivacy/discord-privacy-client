use keystore::{
    build_partial_duress_handlers, generate_identity, save_identity, save_password_record,
    save_prekey_state, Argon2Params, DuressEngine, DuressHandlers, DuressPaths, NoOpSealer,
    PasswordRecord, PrekeyConfig, PrekeyState, ProductionDuressConfig, StepOutcome, WipeStep,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

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

fn journal_completed_steps(journal_path: &std::path::Path, completed_steps: &[WipeStep]) {
    let completed = completed_steps
        .iter()
        .copied()
        .map(|step| {
            (
                step,
                StepOutcome::Skipped {
                    reason: "prefilled test step".to_owned(),
                },
            )
        })
        .collect::<Vec<_>>();
    let journal = keystore::DuressJournal {
        completed,
        started_at_unix_seconds: 0,
    };
    std::fs::write(journal_path, serde_json::to_vec_pretty(&journal).unwrap()).unwrap();
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
    journal_completed_steps(
        &journal_path,
        &[
            WipeStep::TpmEvict,
            WipeStep::KeyringPurge,
            WipeStep::IdentityFile,
            WipeStep::PasswordHashes,
            WipeStep::PrekeyFile,
        ],
    );
    let cache_dir = dir.path().join("store");
    let opsec_file = dir.path().join("injection.js");
    let opsec_dir = dir.path().join("opsec");
    std::fs::create_dir_all(&cache_dir).unwrap();
    std::fs::create_dir_all(&opsec_dir).unwrap();
    std::fs::write(cache_dir.join("message-cache"), b"cache").unwrap();
    std::fs::write(&opsec_file, b"opsec").unwrap();
    std::fs::write(opsec_dir.join("config.json"), b"{}").unwrap();

    let handlers = build_partial_duress_handlers(
        Some(cache_dir.clone()),
        vec![opsec_file.clone(), opsec_dir.clone()],
    );
    let engine = DuressEngine::new(journal_path, paths, handlers);
    assert!(engine.handler_wired(WipeStep::LocalCacheDir));
    assert!(engine.handler_wired(WipeStep::StripOpsecFiles));
    assert!(!engine.handler_wired(WipeStep::Prekeys));

    let report = engine.execute().unwrap();
    assert!(report.completed);
    assert!(report.failed_steps().is_empty());

    let got_order: Vec<WipeStep> = report.steps.iter().map(|(step, _)| *step).collect();
    assert_eq!(
        got_order,
        vec![
            WipeStep::TpmEvict,
            WipeStep::KeyringPurge,
            WipeStep::IdentityFile,
            WipeStep::PasswordHashes,
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
    assert_eq!(
        outcome_for(&report.steps, WipeStep::LocalCacheDir),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::StripOpsecFiles),
        &StepOutcome::Wiped
    );
    assert!(!cache_dir.exists());
    assert!(!opsec_file.exists());
    assert!(!opsec_dir.exists());
}

#[test]
fn build_production_duress_handlers() {
    let dir = TempDir::new().unwrap();
    let config_dir = dir.path().join("account");
    let password_dir = dir.path().join("device");
    let opsec_file = config_dir.join("injection.js");
    let opsec_dir = config_dir.join("opsec");
    std::fs::create_dir_all(config_dir.join("store")).unwrap();
    std::fs::create_dir_all(&opsec_dir).unwrap();
    std::fs::create_dir_all(&password_dir).unwrap();
    std::fs::write(config_dir.join("store").join("message-cache"), b"cache").unwrap();
    std::fs::write(&opsec_file, b"opsec").unwrap();
    std::fs::write(opsec_dir.join("config.json"), b"{}").unwrap();

    let config = ProductionDuressConfig {
        config_dir: config_dir.clone(),
        password_dir: password_dir.clone(),
        strip_opsec_files: vec![opsec_file.clone(), opsec_dir.clone()],
    };
    let (paths, journal_path) = keystore::build_production_duress_paths(&config);
    assert_eq!(paths.identity_file, config_dir.join("identity.json"));
    assert_eq!(
        paths.password_file,
        password_dir.join("password_marker.json")
    );
    assert_eq!(paths.prekey_file, Some(config_dir.join("prekeys.json")));
    assert_eq!(journal_path, config_dir.join("duress.journal"));
    journal_completed_steps(
        &journal_path,
        &[
            WipeStep::TpmEvict,
            WipeStep::KeyringPurge,
            WipeStep::IdentityFile,
            WipeStep::PasswordHashes,
            WipeStep::PrekeyFile,
        ],
    );

    let handlers = keystore::build_production_duress_handlers(&config);
    let engine = DuressEngine::new(journal_path, paths, handlers);
    assert!(engine.handler_wired(WipeStep::LocalCacheDir));
    assert!(engine.handler_wired(WipeStep::StripOpsecFiles));

    let report = engine.execute().unwrap();

    assert_eq!(
        outcome_for(&report.steps, WipeStep::LocalCacheDir),
        &StepOutcome::Wiped
    );
    assert_eq!(
        outcome_for(&report.steps, WipeStep::StripOpsecFiles),
        &StepOutcome::Wiped
    );
    assert!(!config_dir.join("store").exists());
    assert!(!opsec_file.exists());
    assert!(!opsec_dir.exists());
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
fn successful_run_removes_journal() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let handlers = DuressHandlers {
        wipe_local_cache_dir: Some(Box::new(|| Ok(()))),
        wipe_anonymous_credentials: Some(Box::new(|| Ok(()))),
        wipe_prekeys: Some(Box::new(|| Ok(()))),
        wipe_double_ratchet: Some(Box::new(|| Ok(()))),
        wipe_sender_keys: Some(Box::new(|| Ok(()))),
        wipe_peer_ratchets: Some(Box::new(|| Ok(()))),
        zeroize_in_memory: Some(Box::new(|| Ok(()))),
        strip_opsec_files: Some(Box::new(|| Ok(()))),
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
