use crypto::ed25519;
use ipc::commands::guard_backup_destination;
use ipc::control_messages::RevocationNotice;
use ipc::revocation::{
    accept_content, apply_inbound_revocation, burn_id, record_content_accepted, scope_commit_key,
    scope_commitment, ContentDecision, InboundDecision, RevocationLedger,
};
use ipc::secure_local_store::{
    RawBackend, RecordId, SealedStore, SecureLocalStore, SecureLocalStoreError,
};
use keystore::identity_bundle::{BundleVerifyError, BundleVerifyPolicy, IdentityBundle};
use keystore::{
    generate_identity, AccountOwnershipError, AccountOwnershipProof, DuressEngine, DuressError,
    DuressHandlers, DuressPaths, ProductionDuressHandlers, ProofChallenge, StepOutcome, WipeStep,
    PROOF_CHALLENGE_NONCE_BYTES,
};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use store::{AnchorRecord, MessageStore, MonotonicAnchor, StoreError, StoredMessage};
use tempfile::TempDir;

const LOCAL_STORE_KEY: [u8; 32] = [0xA7; 32];
const STORE_SECRET: &[u8; 32] = b"a7-anchored-store-secret-32bytes";
const SCOPE: &str = "gc:a7-full-acceptance";
const ALICE_DISCORD_ID: &str = "a7-alice";
const BOB_DISCORD_ID: &str = "a7-bob";

#[derive(Clone, Default)]
struct MemoryBackend {
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    writes: Arc<Mutex<usize>>,
}

impl MemoryBackend {
    fn write_count(&self) -> usize {
        *self.writes.lock().unwrap()
    }

    fn blobs(&self) -> Vec<Vec<u8>> {
        self.blobs.lock().unwrap().values().cloned().collect()
    }
}

impl RawBackend for MemoryBackend {
    fn write_blob(&self, storage_key: &str, blob: &[u8]) -> Result<(), SecureLocalStoreError> {
        *self.writes.lock().unwrap() += 1;
        self.blobs
            .lock()
            .unwrap()
            .insert(storage_key.to_string(), blob.to_vec());
        Ok(())
    }

    fn read_blob(&self, storage_key: &str) -> Result<Option<Vec<u8>>, SecureLocalStoreError> {
        Ok(self.blobs.lock().unwrap().get(storage_key).cloned())
    }

    fn remove_blob(&self, storage_key: &str) -> Result<(), SecureLocalStoreError> {
        self.blobs.lock().unwrap().remove(storage_key);
        Ok(())
    }
}

#[derive(Default)]
struct MemoryAnchor {
    records: Mutex<BTreeMap<[u8; 32], AnchorRecord>>,
    loads: Mutex<usize>,
    advances: Mutex<usize>,
}

impl MemoryAnchor {
    fn load_count(&self) -> usize {
        *self.loads.lock().unwrap()
    }

    fn advance_count(&self) -> usize {
        *self.advances.lock().unwrap()
    }

    fn generation(&self) -> u64 {
        self.records
            .lock()
            .unwrap()
            .values()
            .next()
            .expect("anchor is enrolled")
            .generation
    }
}

impl MonotonicAnchor for MemoryAnchor {
    fn load(&self, store_id: [u8; 32]) -> Result<Option<AnchorRecord>, StoreError> {
        *self.loads.lock().unwrap() += 1;
        Ok(self.records.lock().unwrap().get(&store_id).cloned())
    }

    fn compare_and_advance(
        &self,
        store_id: [u8; 32],
        expected: Option<AnchorRecord>,
        next: AnchorRecord,
    ) -> Result<(), StoreError> {
        *self.advances.lock().unwrap() += 1;
        let mut records = self.records.lock().unwrap();
        if records.get(&store_id).cloned() != expected {
            return Err(StoreError::Anchor("stale anchor compare".to_string()));
        }
        records.insert(store_id, next);
        Ok(())
    }
}

fn message(id: &str, sender: &str, body: &str) -> StoredMessage {
    StoredMessage {
        discord_message_id: id.to_string(),
        channel_id: SCOPE.to_string(),
        sender_discord_id: sender.to_string(),
        sender_osl_user_id: sender.to_string(),
        plaintext: body.to_string(),
        decrypted_at: 1_700_000_000,
        burned: false,
    }
}

fn signed_bundle(identity: &keystore::Identity, revision: u64) -> IdentityBundle {
    let mut bundle = IdentityBundle {
        ed25519_identity_pub: *identity.ed25519_public.as_bytes(),
        x25519_identity_pub: *identity.x25519_public.as_bytes(),
        mlkem768_identity_pub: identity.mlkem_public_bytes,
        capability_bundle: 1,
        revision,
        signature: [0u8; ed25519::SIGNATURE_SIZE],
    };
    let signature = ed25519::sign(&identity.ed25519_secret, &bundle.signed_bytes());
    bundle.signature = *signature.as_bytes();
    bundle
}

fn assert_step(report: &keystore::DuressReport, step: WipeStep, expected: StepOutcome) {
    let actual = report
        .steps
        .iter()
        .find_map(|(candidate, outcome)| (*candidate == step).then_some(outcome))
        .unwrap_or_else(|| panic!("missing duress step {step:?}"));
    assert_eq!(actual, &expected, "unexpected outcome for {step:?}");
}

fn remove_file_handler(path: PathBuf) -> keystore::WipeFn {
    Box::new(move || match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DuressError::Io(format!("remove bound file: {error}"))),
    })
}

fn remove_dir_handler(path: PathBuf) -> keystore::WipeFn {
    Box::new(move || match fs::remove_dir_all(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(DuressError::Io(format!("remove bound directory: {error}"))),
    })
}

fn set_flag_handler(flag: Arc<AtomicBool>) -> keystore::WipeFn {
    Box::new(move || {
        flag.store(true, Ordering::SeqCst);
        Ok(())
    })
}

fn copy_store_artifacts(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for name in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
    ] {
        let source = from.join(name);
        if source.exists() {
            fs::copy(&source, to.join(name)).unwrap();
        }
    }
}

fn restore_store_artifacts(from: &Path, to: &Path) {
    for name in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
    ] {
        let target = to.join(name);
        if target.exists() {
            fs::remove_file(&target).unwrap();
        }
        let source = from.join(name);
        if source.exists() {
            fs::copy(source, target).unwrap();
        }
    }
}

#[test]
fn full_a7_duress_auto_lock_anchor_replay() {
    let alice = generate_identity("a7-alice-osl".to_string());
    let bob = generate_identity("a7-bob-osl".to_string());
    let anchor = Arc::new(MemoryAnchor::default());
    let store_dir = TempDir::new().unwrap();
    let backup_dir = TempDir::new().unwrap();

    {
        let store = MessageStore::open_anchored(store_dir.path(), STORE_SECRET, anchor.clone())
            .expect("first anchored open enrolls");
        assert!(
            anchor.load_count() > 0,
            "anchored open must consult provider"
        );
        assert_eq!(anchor.generation(), 1, "first open enrolls generation one");

        store
            .put(&message("a7-alice-1", ALICE_DISCORD_ID, "alice pre-duress"))
            .unwrap();
        store
            .put(&message(
                "a7-bob-1",
                BOB_DISCORD_ID,
                "bob content must survive alice burn",
            ))
            .unwrap();
        assert!(store.get("a7-alice-1").unwrap().is_some());
        assert!(store.get("a7-bob-1").unwrap().is_some());
        assert!(anchor.generation() >= 3, "message writes advance anchor");
    }
    copy_store_artifacts(store_dir.path(), backup_dir.path());

    let pre_burn_anchor_generation = anchor.generation();

    let backend = MemoryBackend::default();
    let protected_id = RecordId::new("a7-session", "notification-title");
    let protected_plaintext = b"a7 protected local UI state";
    let unlocked = SealedStore::new(LOCAL_STORE_KEY, backend.clone());
    unlocked.put(&protected_id, protected_plaintext).unwrap();
    assert_eq!(unlocked.get(&protected_id).unwrap(), protected_plaintext);
    assert_eq!(backend.write_count(), 1);
    assert!(
        backend.blobs().iter().all(|blob| !blob
            .windows(protected_plaintext.len())
            .any(|w| w == protected_plaintext)),
        "unlocked local-store writes must be sealed, not plaintext"
    );

    let locked = SealedStore::without_key(backend.clone());
    assert!(matches!(
        locked.get(&protected_id),
        Err(SecureLocalStoreError::NoKey)
    ));
    assert!(matches!(
        locked.put(&protected_id, b"must not write while locked"),
        Err(SecureLocalStoreError::NoKey)
    ));
    assert_eq!(
        backend.write_count(),
        1,
        "auto-lock/no-key state must refuse without touching storage"
    );
    assert!(
        guard_backup_destination("store/messages.sqlite", false).is_err(),
        "rollback backup writes must refuse when no encrypted destination is available"
    );

    let duress_dir = TempDir::new().unwrap();
    let identity_file = duress_dir.path().join("identity.json");
    let password_file = duress_dir.path().join("password.json");
    let prekey_file = duress_dir.path().join("prekeys.json");
    let local_cache_dir = duress_dir.path().join("local-cache");
    let anonymous_file = duress_dir.path().join("anonymous-credentials.json");
    let opsec_file = duress_dir.path().join("boot.js");
    fs::write(&identity_file, b"sealed identity").unwrap();
    fs::write(&password_file, b"password hashes").unwrap();
    fs::write(&prekey_file, b"sealed prekeys").unwrap();
    fs::create_dir(&local_cache_dir).unwrap();
    fs::write(local_cache_dir.join("cache.bin"), b"cache").unwrap();
    fs::write(&anonymous_file, b"anonymous token").unwrap();
    fs::write(&opsec_file, b"injection").unwrap();

    let prekeys_wiped = Arc::new(AtomicBool::new(false));
    let dr_wiped = Arc::new(AtomicBool::new(false));
    let sender_keys_wiped = Arc::new(AtomicBool::new(false));
    let peer_ratchets_wiped = Arc::new(AtomicBool::new(false));
    let zeroized = Arc::new(AtomicBool::new(false));
    let handlers = ProductionDuressHandlers::from_handlers(DuressHandlers {
        purge_keyring: Some(Box::new(|| Ok(()))),
        wipe_local_cache_dir: Some(remove_dir_handler(local_cache_dir.clone())),
        wipe_anonymous_credentials: Some(remove_file_handler(anonymous_file.clone())),
        wipe_prekeys: Some(set_flag_handler(prekeys_wiped.clone())),
        wipe_double_ratchet: Some(set_flag_handler(dr_wiped.clone())),
        wipe_sender_keys: Some(set_flag_handler(sender_keys_wiped.clone())),
        wipe_peer_ratchets: Some(set_flag_handler(peer_ratchets_wiped.clone())),
        zeroize_in_memory: Some(set_flag_handler(zeroized.clone())),
        strip_opsec_files: Some(remove_file_handler(opsec_file.clone())),
        unregister_account: Some(Box::new(|| Ok(()))),
    })
    .into_handlers();
    let report = DuressEngine::new(
        duress_dir.path().join("duress.journal"),
        DuressPaths {
            identity_file: identity_file.clone(),
            password_file: password_file.clone(),
            prekey_file: Some(prekey_file.clone()),
        },
        handlers,
    )
    .execute()
    .unwrap();
    assert!(report.completed, "duress flow must reach a terminal report");
    assert_step(&report, WipeStep::IdentityFile, StepOutcome::Wiped);
    assert_step(&report, WipeStep::PasswordHashes, StepOutcome::Wiped);
    assert_step(&report, WipeStep::UnregisterAccount, StepOutcome::Wiped);
    assert_step(&report, WipeStep::PrekeyFile, StepOutcome::Wiped);
    assert_step(&report, WipeStep::LocalCacheDir, StepOutcome::Wiped);
    assert_step(&report, WipeStep::AnonymousCredentials, StepOutcome::Wiped);
    assert_step(&report, WipeStep::DoubleRatchet, StepOutcome::Wiped);
    assert_step(&report, WipeStep::SenderKeys, StepOutcome::Wiped);
    assert_step(&report, WipeStep::PeerRatchets, StepOutcome::Wiped);
    assert_step(&report, WipeStep::InMemoryZeroize, StepOutcome::Wiped);
    assert_step(&report, WipeStep::StripOpsecFiles, StepOutcome::Wiped);
    assert!(report.failed_steps().is_empty());
    assert!(report.skipped_steps().is_empty());
    for path in [
        &identity_file,
        &password_file,
        &prekey_file,
        &anonymous_file,
        &opsec_file,
    ] {
        assert!(!path.exists(), "duress must remove {}", path.display());
    }
    assert!(!local_cache_dir.exists());
    for flag in [
        prekeys_wiped,
        dr_wiped,
        sender_keys_wiped,
        peer_ratchets_wiped,
        zeroized,
    ] {
        assert!(
            flag.load(Ordering::SeqCst),
            "bound duress callback did not run"
        );
    }

    let key =
        scope_commit_key(alice.x25519_public.as_bytes(), bob.x25519_public.as_bytes()).unwrap();
    let commitment = scope_commitment(&key, SCOPE);
    let mut ledger = RevocationLedger::default();
    for seq in 1..=2 {
        record_content_accepted(&mut ledger, &commitment, seq).unwrap();
    }
    let notice = RevocationNotice {
        scope_commitment: commitment,
        burn_epoch: 1,
        burn_upto_seq: 2,
        message_commitments: Vec::new(),
        burn_id: burn_id(&key, &commitment, 1, 2),
        issued_at: 1_700_000_001,
    };
    let first_burn = apply_inbound_revocation(&mut ledger, &key, &notice, 1_700_000_002).unwrap();
    assert_eq!(first_burn.decision, InboundDecision::Applied);
    assert_eq!(first_burn.destroy_upto_seq, 2);

    {
        let store = MessageStore::open_anchored(store_dir.path(), STORE_SECRET, anchor.clone())
            .expect("reopened backup state is still current for its own generation");
        assert_eq!(
            store
                .wipe_wrapped_keys_in_scope("gc", SCOPE, Some(ALICE_DISCORD_ID))
                .unwrap(),
            1,
            "auto-burn must wipe only the duress owner's authored content"
        );
        assert_eq!(store.get("a7-alice-1").unwrap(), None);
        assert_eq!(
            store.get("a7-bob-1").unwrap(),
            Some(message(
                "a7-bob-1",
                BOB_DISCORD_ID,
                "bob content must survive alice burn"
            ))
        );
    }
    assert!(
        anchor.generation() > pre_burn_anchor_generation,
        "auto-burn mutation must publish a newer anchor generation"
    );

    for seq in 3..=4 {
        assert_eq!(
            accept_content(&ledger, &commitment, seq),
            ContentDecision::Accept,
            "content after the original burn must remain acceptable"
        );
        record_content_accepted(&mut ledger, &commitment, seq).unwrap();
    }
    let replay = apply_inbound_revocation(&mut ledger, &key, &notice, 1_700_010_000).unwrap();
    assert_eq!(replay.decision, InboundDecision::AlreadyApplied);
    assert_eq!(replay.destroy_upto_seq, 0);
    for seq in 3..=4 {
        assert_eq!(
            accept_content(&ledger, &commitment, seq),
            ContentDecision::Accept
        );
    }

    let policy = BundleVerifyPolicy::new();
    let current_bundle = signed_bundle(&alice, 7);
    assert_eq!(
        policy.verify(&current_bundle, &alice.ed25519_public, Some(6)),
        Ok(7)
    );
    let replayed_bundle = signed_bundle(&alice, 7);
    assert_eq!(
        policy.verify(&replayed_bundle, &alice.ed25519_public, Some(7)),
        Err(BundleVerifyError::RevisionNotMonotonic {
            got: 7,
            last_known: 7
        }),
        "identity-bundle anchor must refuse a stale keyserver replay"
    );
    let substitute = signed_bundle(&bob, 8);
    assert_eq!(
        policy.verify(&substitute, &alice.ed25519_public, Some(7)),
        Err(BundleVerifyError::SignatureInvalid),
        "sender attribution must stay anchored to the pinned identity key"
    );

    let mut challenge = ProofChallenge::new(
        [0x11; PROOF_CHALLENGE_NONCE_BYTES],
        "a7-platform-account",
        &alice.user_id,
        1_700_000_000,
        1_700_000_060,
    )
    .unwrap();
    let proof = AccountOwnershipProof::from_challenge(&alice, &mut challenge, 1_700_000_001)
        .expect("fresh ownership proof");
    assert_eq!(proof.platform_id, "a7-platform-account");
    assert_eq!(
        AccountOwnershipProof::from_challenge(&alice, &mut challenge, 1_700_000_002),
        Err(AccountOwnershipError::ProofReplayed),
        "spent account ownership challenge must refuse replay"
    );
    for rendered in [
        format!("{proof:?}"),
        proof.to_string(),
        format!("{:?}", AccountOwnershipError::ProofReplayed),
    ] {
        assert!(!rendered.contains("a7-platform-account"));
        assert!(!rendered.contains(&alice.user_id));
    }

    let advance_count_before_restore = anchor.advance_count();
    restore_store_artifacts(backup_dir.path(), store_dir.path());
    let stale_open = MessageStore::open_anchored(store_dir.path(), STORE_SECRET, anchor.clone());
    assert!(
        matches!(stale_open, Err(StoreError::Anchor(message)) if message.contains("behind external anchor")),
        "coherent SQLite replay must be refused by the external anchor"
    );
    assert_eq!(
        anchor.advance_count(),
        advance_count_before_restore,
        "stale replay must fail before any provider advance"
    );
}
