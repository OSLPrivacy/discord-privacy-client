use keystore::{
    Argon2Params, DuressEngine, DuressHandlers, DuressPaths, NoOpSealer, PasswordRecord,
    StepOutcome, WipeStep, generate_identity, save_identity, save_password_record,
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
    let journal_file = dir.path().join("duress.journal");
    (
        DuressPaths {
            identity_file,
            password_file,
        },
        journal_file,
    )
}

#[test]
fn execute_with_no_handlers_walks_every_step() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    // Pre-populate the on-disk identity + password so the file
    // deletion steps actually have something to delete.
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();
    let pw = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&paths.password_file, &pw, &sealer).unwrap();
    assert!(paths.identity_file.exists());
    assert!(paths.password_file.exists());

    let engine =
        DuressEngine::new(journal_path.clone(), paths, DuressHandlers::default());
    let report = engine.execute().unwrap();

    // Every step in the canonical order must appear.
    let want_order: Vec<WipeStep> = WipeStep::ordered().to_vec();
    let got_order: Vec<WipeStep> = report.steps.iter().map(|(s, _)| *s).collect();
    assert_eq!(got_order, want_order);

    // Files were deleted (Wiped) — explicit checks for the two we
    // actually placed on disk.
    let id_outcome = outcome_for(&report.steps, WipeStep::IdentityFile);
    assert_eq!(id_outcome, &StepOutcome::Wiped);
    let pw_outcome = outcome_for(&report.steps, WipeStep::PasswordHashes);
    assert_eq!(pw_outcome, &StepOutcome::Wiped);
    assert!(!report.failed_steps().iter().any(|(s, _)| *s == WipeStep::IdentityFile));

    // Files are gone from disk.
    assert!(!std::path::Path::new(&dir.path().join("identity.json")).exists());
    assert!(!std::path::Path::new(&dir.path().join("password.json")).exists());

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
                assert!(
                    !reason.is_empty(),
                    "skipped step {s:?} must carry a reason"
                );
                assert!(
                    !reason.contains("unimplemented") && !reason.contains("todo"),
                    "skip reason for {s:?} must not look like a silent defer"
                );
            }
            other => panic!("step {s:?} expected Skipped, got {other:?}"),
        }
    }
}

#[test]
fn idempotent_execute_can_be_called_twice() {
    let dir = TempDir::new().unwrap();
    let (paths, journal_path) = build_paths(&dir);
    let sealer = NoOpSealer::new();
    let id = generate_identity("alice".into());
    save_identity(&paths.identity_file, &id, &sealer).unwrap();

    let engine =
        DuressEngine::new(journal_path.clone(), paths, DuressHandlers::default());
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
    let engine =
        DuressEngine::new(journal_path, paths, DuressHandlers::default());
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
        wipe_local_cache_dir: Some(mk_handler("local_cache")),
        wipe_anonymous_credentials: Some(mk_handler("creds")),
        wipe_prekeys: Some(mk_handler("prekeys")),
        wipe_double_ratchet: Some(mk_handler("ratchet")),
        wipe_sender_keys: Some(mk_handler("sender_keys")),
        wipe_peer_ratchets: Some(mk_handler("peer_ratchets")),
        zeroize_in_memory: Some(mk_handler("zeroize")),
        strip_opsec_files: Some(mk_handler("strip")),
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
    std::fs::write(&journal_path, serde_json::to_vec_pretty(&prefilled).unwrap())
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
    let engine =
        DuressEngine::new(journal_path, paths, DuressHandlers::default());
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

fn outcome_for<'a>(
    steps: &'a [(WipeStep, StepOutcome)],
    target: WipeStep,
) -> &'a StepOutcome {
    &steps
        .iter()
        .find(|(s, _)| *s == target)
        .unwrap_or_else(|| panic!("step {target:?} missing from report"))
        .1
}
