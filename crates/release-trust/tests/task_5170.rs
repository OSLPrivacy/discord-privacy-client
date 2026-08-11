use ed25519_dalek::{Signer, SigningKey};
use release_trust::{
    canonical_json, Envelope, KeyValue, MetadataSignature, PublicKey, ReleaseConsumer, Role,
    RootMetadata, SequentialTrustClient, TrustError, TrustedTarget,
    OFFLINE_RECOVERY_INSTALLER_PATH,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const NOW: u64 = 1_786_435_200;
const VALID_UNTIL: u64 = NOW + 10_000_000;
const ROLES: [&str; 3] = ["update", "build-proof", "carrier-table"];
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

struct ReleaseKeys {
    root: [SigningKey; 3],
    targets: SigningKey,
    snapshot: SigningKey,
    timestamp: SigningKey,
    delegated: [SigningKey; 3],
}

impl ReleaseKeys {
    fn deterministic(version: u8) -> Self {
        let key = |slot: u8| SigningKey::from_bytes(&[version.wrapping_mul(31) ^ slot; 32]);
        Self {
            root: [key(1), key(2), key(3)],
            targets: key(4),
            snapshot: key(5),
            timestamp: key(6),
            delegated: [key(7), key(8), key(9)],
        }
    }
}

#[derive(Clone, Copy)]
struct ReleaseOptions {
    metadata_version: u64,
    timestamp_expires: u64,
    artifact_nonce: u64,
    revoked_signer: Option<(usize, usize)>,
}

impl ReleaseOptions {
    fn normal(version: u64) -> Self {
        Self {
            metadata_version: version,
            timestamp_expires: VALID_UNTIL,
            artifact_nonce: 0,
            revoked_signer: None,
        }
    }
}

struct Fixture {
    path: PathBuf,
    trusted_root: PathBuf,
    keys: Vec<ReleaseKeys>,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "osl-task-5170-{label}-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("metadata/roots")).unwrap();
        fs::create_dir_all(path.join("metadata/delegated")).unwrap();
        for role in ROLES {
            fs::create_dir_all(path.join("artifacts").join(role)).unwrap();
        }
        let keys = (1..=4)
            .map(|version| ReleaseKeys::deterministic(version as u8))
            .collect::<Vec<_>>();
        let trusted_root = path.join("trusted-root-v1.json");
        let body = root_body(1, &keys[0], VALID_UNTIL);
        write_envelope(
            &trusted_root,
            &signed(body, [&keys[0].root[0], &keys[0].root[1]]),
        );
        let fixture = Self {
            path,
            trusted_root,
            keys,
        };
        fixture.write_release(1, ReleaseOptions::normal(1));
        fixture
    }

    fn write_release(&self, root_version: usize, options: ReleaseOptions) {
        let active = &self.keys[root_version - 1];
        let root = root_body(root_version as u64, active, VALID_UNTIL);
        write_envelope(
            &self.path.join("metadata/root.json"),
            &signed(root.clone(), [&active.root[0], &active.root[1]]),
        );
        if root_version > 1 {
            let old = &self.keys[root_version - 2];
            write_envelope(
                &self
                    .path
                    .join(format!("metadata/roots/{root_version}.root.json")),
                &signed(
                    root,
                    [&old.root[0], &old.root[1], &active.root[0], &active.root[1]],
                ),
            );
        }

        let mut delegated_envelopes = Vec::new();
        for (index, role) in ROLES.iter().enumerate() {
            let artifact_name = match *role {
                "update" => "latest.json",
                "build-proof" => "unmodified-build.json",
                "carrier-table" => "carriers.json",
                _ => unreachable!(),
            };
            let artifact_path = format!("{role}/{artifact_name}");
            let bytes = format!(
                "TASK5170 trusted {role} root={root_version} metadata={} nonce={}\n",
                options.metadata_version, options.artifact_nonce
            )
            .into_bytes();
            fs::write(self.path.join("artifacts").join(&artifact_path), &bytes).unwrap();
            let artifact_targets =
                BTreeMap::from([(artifact_path.clone(), descriptor_bytes(&bytes))]);
            let body = json!({
                "_type": "targets",
                "spec_version": "1.0.31",
                "version": options.metadata_version,
                "expires": VALID_UNTIL,
                "targets": artifact_targets,
            });
            let signer = options
                .revoked_signer
                .filter(|(revoked_role, _)| *revoked_role == index)
                .map(|(_, key_version)| &self.keys[key_version - 1].delegated[index])
                .unwrap_or(&active.delegated[index]);
            delegated_envelopes.push(signed(body, [signer]));
        }

        let mut delegation_keys = Map::new();
        let mut delegation_roles = Vec::new();
        for (index, role) in ROLES.iter().enumerate() {
            let id = key_id(&active.delegated[index]);
            delegation_keys.insert(
                id.clone(),
                serde_json::to_value(public_key(&active.delegated[index])).unwrap(),
            );
            delegation_roles.push(json!({
                "name": role,
                "keyids": [id],
                "threshold": 1,
                "paths": [format!("{role}/")],
                "terminating": true,
            }));
        }
        let targets = signed(
            json!({
                "_type": "targets",
                "spec_version": "1.0.31",
                "version": options.metadata_version,
                "expires": VALID_UNTIL,
                "targets": {},
                "delegations": {
                    "keys": Value::Object(delegation_keys),
                    "roles": delegation_roles,
                }
            }),
            [&active.targets],
        );

        let mut snapshot_meta = Map::new();
        snapshot_meta.insert("targets.json".to_owned(), descriptor_envelope(&targets));
        for (role, envelope) in ROLES.iter().zip(&delegated_envelopes) {
            snapshot_meta.insert(
                format!("delegated/{role}.json"),
                descriptor_envelope(envelope),
            );
        }
        let snapshot = signed(
            json!({
                "_type": "snapshot",
                "spec_version": "1.0.31",
                "version": options.metadata_version,
                "expires": VALID_UNTIL,
                "meta": Value::Object(snapshot_meta),
            }),
            [&active.snapshot],
        );
        let timestamp = signed(
            json!({
                "_type": "timestamp",
                "spec_version": "1.0.31",
                "version": options.metadata_version,
                "expires": options.timestamp_expires,
                "meta": {"snapshot.json": descriptor_envelope(&snapshot)},
            }),
            [&active.timestamp],
        );

        write_envelope(&self.path.join("metadata/targets.json"), &targets);
        write_envelope(&self.path.join("metadata/snapshot.json"), &snapshot);
        write_envelope(&self.path.join("metadata/timestamp.json"), &timestamp);
        for (role, envelope) in ROLES.iter().zip(&delegated_envelopes) {
            write_envelope(
                &self.path.join(format!("metadata/delegated/{role}.json")),
                envelope,
            );
        }
    }

    fn write_equivocating_current_root(&self, version: usize) {
        let active = &self.keys[version - 1];
        let body = root_body(version as u64, active, VALID_UNTIL + 1);
        write_envelope(
            &self.path.join("metadata/root.json"),
            &signed(body, [&active.root[0], &active.root[1]]),
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn public_key(signing: &SigningKey) -> PublicKey {
    PublicKey {
        keytype: "ed25519".to_owned(),
        scheme: "ed25519".to_owned(),
        keyval: KeyValue {
            public: hex::encode(signing.verifying_key().to_bytes()),
        },
    }
}

fn key_id(signing: &SigningKey) -> String {
    let key = public_key(signing);
    let value = json!({
        "keytype": key.keytype,
        "keyval": {"public": key.keyval.public},
        "scheme": key.scheme,
    });
    hex::encode(Sha256::digest(canonical_json(&value)))
}

fn root_body(version: u64, keys: &ReleaseKeys, expires: u64) -> Value {
    let mut public = BTreeMap::new();
    let mut root_ids = Vec::new();
    for key in &keys.root {
        let id = key_id(key);
        root_ids.push(id.clone());
        public.insert(id, public_key(key));
    }
    let targets = key_id(&keys.targets);
    let snapshot = key_id(&keys.snapshot);
    let timestamp = key_id(&keys.timestamp);
    public.insert(targets.clone(), public_key(&keys.targets));
    public.insert(snapshot.clone(), public_key(&keys.snapshot));
    public.insert(timestamp.clone(), public_key(&keys.timestamp));
    serde_json::to_value(RootMetadata {
        kind: "root".to_owned(),
        spec_version: "1.0.31".to_owned(),
        version,
        expires,
        keys: public,
        roles: BTreeMap::from([
            (
                "root".to_owned(),
                Role {
                    keyids: root_ids,
                    threshold: 2,
                },
            ),
            (
                "targets".to_owned(),
                Role {
                    keyids: vec![targets],
                    threshold: 1,
                },
            ),
            (
                "snapshot".to_owned(),
                Role {
                    keyids: vec![snapshot],
                    threshold: 1,
                },
            ),
            (
                "timestamp".to_owned(),
                Role {
                    keyids: vec![timestamp],
                    threshold: 1,
                },
            ),
        ]),
    })
    .unwrap()
}

fn signed<'a>(body: Value, signers: impl IntoIterator<Item = &'a SigningKey>) -> Envelope {
    let payload = canonical_json(&body);
    Envelope {
        signatures: signers
            .into_iter()
            .map(|key| MetadataSignature {
                keyid: key_id(key),
                sig: hex::encode(key.sign(&payload).to_bytes()),
            })
            .collect(),
        signed: body,
    }
}

fn canonical_envelope(envelope: &Envelope) -> Vec<u8> {
    canonical_json(&serde_json::to_value(envelope).unwrap())
}

fn descriptor_bytes(bytes: &[u8]) -> Value {
    json!({
        "length": bytes.len() as u64,
        "hashes": {"sha256": hex::encode(Sha256::digest(bytes))},
    })
}

fn descriptor_envelope(envelope: &Envelope) -> Value {
    let bytes = canonical_envelope(envelope);
    json!({
        "version": envelope.signed["version"].as_u64().unwrap(),
        "length": bytes.len() as u64,
        "hashes": {"sha256": hex::encode(Sha256::digest(&bytes))},
    })
}

fn write_envelope(path: &Path, envelope: &Envelope) {
    fs::write(path, serde_json::to_vec_pretty(envelope).unwrap()).unwrap();
}

fn retain_transition_signers(path: &Path, retained: &BTreeSet<String>) {
    let mut envelope: Envelope = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    envelope
        .signatures
        .retain(|signature| retained.contains(&signature.keyid));
    write_envelope(path, &envelope);
}

fn consume(
    client: &mut SequentialTrustClient,
    repository: &Path,
    consumer: ReleaseConsumer,
) -> release_trust::Result<TrustedTarget> {
    match consumer {
        ReleaseConsumer::WindowsUpdate => client.load_windows_update(repository, NOW),
        ReleaseConsumer::UnmodifiedBuildProof => {
            client.load_unmodified_build_proof(repository, NOW)
        }
        ReleaseConsumer::CarrierTable => client.load_carrier_table(repository, NOW),
    }
}

fn client_at_v4(fixture: &Fixture) -> SequentialTrustClient {
    let mut client = SequentialTrustClient::bootstrap(&fixture.trusted_root, NOW).unwrap();
    consume(&mut client, &fixture.path, ReleaseConsumer::WindowsUpdate).unwrap();
    for version in 2..=4 {
        fixture.write_release(version, ReleaseOptions::normal(version as u64));
        let trusted = consume(&mut client, &fixture.path, ReleaseConsumer::WindowsUpdate).unwrap();
        assert_eq!(trusted.root_version, version as u64);
    }
    client
}

#[test]
fn task_5170_three_persisted_clients_follow_every_rotation() {
    let fixture = Fixture::new("sequential");
    let consumers = ReleaseConsumer::ALL;
    let state_paths = consumers
        .iter()
        .map(|consumer| fixture.path.join(format!("state-{}.json", consumer.name())))
        .collect::<Vec<_>>();

    for (consumer, state) in consumers.iter().zip(&state_paths) {
        let mut client = SequentialTrustClient::bootstrap(&fixture.trusted_root, NOW).unwrap();
        let initial = consume(&mut client, &fixture.path, *consumer).unwrap();
        assert_eq!(initial.root_version, 1);
        assert!(!initial.bytes.is_empty());
        client.save(state).unwrap();
    }

    let mut replacement_signers = BTreeMap::<&str, BTreeSet<String>>::new();
    for version in 2..=4 {
        fixture.write_release(version, ReleaseOptions::normal(version as u64));
        for (consumer, state) in consumers.iter().zip(&state_paths) {
            let mut reopened = SequentialTrustClient::load(state, NOW).unwrap();
            assert_eq!(reopened.root_version(), version as u64 - 1);
            let trusted = consume(&mut reopened, &fixture.path, *consumer).unwrap();
            assert_eq!(trusted.consumer, *consumer);
            assert_eq!(trusted.root_version, version as u64);
            assert!(trusted.bytes.starts_with(b"TASK5170 trusted "));
            replacement_signers
                .entry(consumer.name())
                .or_default()
                .insert(trusted.target_signer);
            reopened.save(state).unwrap();
            let reopened_again = SequentialTrustClient::load(state, NOW).unwrap();
            assert_eq!(reopened_again.root_version(), version as u64);
        }
    }
    assert_eq!(replacement_signers.len(), 3);
    assert!(replacement_signers.values().all(|keys| keys.len() == 3));
    println!(
        "TASK5170 instances=3 consecutive_dual_threshold_transitions=3 replacement_target_signers_per_client=3 total_replacement_signers=9 consumers=windows-update,unmodified-build-proof,carrier-table"
    );
}

#[test]
fn task_5170_refuses_revocation_freeze_rollback_mix_match_equivocation_and_compromise() {
    let fixture = Fixture::new("negative");
    let client = client_at_v4(&fixture);

    let mut revoked_refusals = 0;
    for artifact in 0..30 {
        let role = artifact % ROLES.len();
        let revoked_key_version = artifact % 3 + 1;
        fixture.write_release(
            4,
            ReleaseOptions {
                metadata_version: 10 + artifact as u64,
                timestamp_expires: VALID_UNTIL,
                artifact_nonce: artifact as u64 + 1,
                revoked_signer: Some((role, revoked_key_version)),
            },
        );
        let mut attempt = client.clone();
        let mut released_content = Vec::new();
        let result = consume(&mut attempt, &fixture.path, ReleaseConsumer::ALL[role]);
        if let Ok(target) = result.as_ref() {
            released_content.extend_from_slice(&target.bytes);
        }
        let error = result.unwrap_err();
        assert!(matches!(error, TrustError::WrongRole { .. }));
        assert_eq!(
            released_content.len(),
            0,
            "failed trust returned target content"
        );
        revoked_refusals += 1;
    }
    assert_eq!(revoked_refusals, 30);
    println!("TASK5170 revoked_key_artifacts_refused=30 content_bytes_released=0");

    fixture.write_release(
        4,
        ReleaseOptions {
            metadata_version: 50,
            timestamp_expires: NOW,
            artifact_nonce: 50,
            revoked_signer: None,
        },
    );
    let mut expired_attempt = client.clone();
    let expired = expired_attempt
        .load_windows_update(&fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(&expired, TrustError::Expired { role, .. } if role == "timestamp"));
    println!("TASK5170 expired_timestamp_refused={expired}");

    fixture.write_release(
        4,
        ReleaseOptions {
            metadata_version: 3,
            timestamp_expires: VALID_UNTIL,
            artifact_nonce: 51,
            revoked_signer: None,
        },
    );
    let mut rollback_attempt = client.clone();
    let rollback = rollback_attempt
        .load_unmodified_build_proof(&fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(rollback, TrustError::Rollback { .. }));
    println!("TASK5170 metadata_rollback_refused=true");

    fixture.write_release(
        4,
        ReleaseOptions {
            metadata_version: 60,
            timestamp_expires: VALID_UNTIL,
            artifact_nonce: 60,
            revoked_signer: None,
        },
    );
    let old_snapshot = fs::read(fixture.path.join("metadata/snapshot.json")).unwrap();
    fixture.write_release(
        4,
        ReleaseOptions {
            metadata_version: 61,
            timestamp_expires: VALID_UNTIL,
            artifact_nonce: 61,
            revoked_signer: None,
        },
    );
    fs::write(fixture.path.join("metadata/snapshot.json"), old_snapshot).unwrap();
    let mut mixed_attempt = client.clone();
    let mixed = mixed_attempt
        .load_carrier_table(&fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(
        mixed,
        TrustError::Version { .. } | TrustError::Length { .. } | TrustError::Hash { .. }
    ));
    println!("TASK5170 mix_and_match_refused=true");

    fixture.write_release(4, ReleaseOptions::normal(4));
    fixture.write_equivocating_current_root(4);
    let mut equivocation_attempt = client.clone();
    let equivocation = equivocation_attempt
        .load_windows_update(&fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(
        equivocation,
        TrustError::Equivocation {
            ref role,
            version: 4
        } if role == "root"
    ));
    println!("TASK5170 same_version_root_equivocation_refused=true version=4 variants=2");

    fixture.write_release(3, ReleaseOptions::normal(3));
    let mut root_rollback_attempt = client.clone();
    let root_rollback = root_rollback_attempt
        .load_windows_update(&fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(
        root_rollback,
        TrustError::Rollback {
            ref role,
            pinned: 4,
            presented: 3
        } if role == "root"
    ));
    println!("TASK5170 root_rollback_refused=true pinned=4 presented=3");

    fixture.write_release(4, ReleaseOptions::normal(4));
    let artifact = fixture.path.join("artifacts/carrier-table/carriers.json");
    fs::write(&artifact, b"untrusted bytes must never escape").unwrap();
    let mut tamper_attempt = client.clone();
    let mut released_content = Vec::new();
    let result = tamper_attempt.load_carrier_table(&fixture.path, NOW);
    if let Ok(target) = result.as_ref() {
        released_content.extend_from_slice(&target.bytes);
    }
    assert!(result.is_err());
    assert_eq!(released_content.len(), 0);
    println!("TASK5170 pre_trust_content_bytes=0");

    fixture.write_release(4, ReleaseOptions::normal(4));
    let mut compromised = client.clone();
    compromised.mark_root_threshold_compromised();
    let compromised_state = fixture.path.join("compromised-client-state.json");
    compromised.save(&compromised_state).unwrap();
    let mut compromised = SequentialTrustClient::load(&compromised_state, NOW).unwrap();
    let installed_path = fixture.path.join("compromised-root-install.exe");
    fs::write(&installed_path, b"").unwrap();
    let result = compromised.load_windows_update(&fixture.path, NOW);
    if let Ok(target) = result.as_ref() {
        fs::write(&installed_path, &target.bytes).unwrap();
    }
    let recovery = result.unwrap_err();
    assert!(matches!(
        &recovery,
        TrustError::OfflineRecoveryRequired {
            offline_recovery,
            ..
        } if *offline_recovery == OFFLINE_RECOVERY_INSTALLER_PATH
    ));
    assert_eq!(fs::metadata(&installed_path).unwrap().len(), 0);
    assert!(recovery
        .to_string()
        .contains(OFFLINE_RECOVERY_INSTALLER_PATH));
    println!(
        "TASK5170 compromised_root_installed_bytes=0 offline_recovery_path={OFFLINE_RECOVERY_INSTALLER_PATH}"
    );

    let skipped_fixture = Fixture::new("skipped");
    let mut skipped_client =
        SequentialTrustClient::bootstrap(&skipped_fixture.trusted_root, NOW).unwrap();
    skipped_fixture.write_release(3, ReleaseOptions::normal(3));
    let skipped = skipped_client
        .load_windows_update(&skipped_fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(
        skipped,
        TrustError::SkippedRoot {
            expected: 2,
            presented: 3
        }
    ));
    println!("TASK5170 skipped_intermediate_root_refused=true expected=2 presented=3");

    let new_only_fixture = Fixture::new("new-threshold-only");
    new_only_fixture.write_release(2, ReleaseOptions::normal(2));
    let new_ids = new_only_fixture.keys[1]
        .root
        .iter()
        .map(key_id)
        .collect::<BTreeSet<_>>();
    retain_transition_signers(
        &new_only_fixture.path.join("metadata/roots/2.root.json"),
        &new_ids,
    );
    let mut new_only_client =
        SequentialTrustClient::bootstrap(&new_only_fixture.trusted_root, NOW).unwrap();
    let old_starved = new_only_client
        .load_windows_update(&new_only_fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(
        old_starved,
        TrustError::Threshold {
            ref role,
            valid: 0,
            threshold: 2
        } if role == "root-old-threshold"
    ));

    let old_only_fixture = Fixture::new("old-threshold-only");
    old_only_fixture.write_release(2, ReleaseOptions::normal(2));
    let old_ids = old_only_fixture.keys[0]
        .root
        .iter()
        .map(key_id)
        .collect::<BTreeSet<_>>();
    retain_transition_signers(
        &old_only_fixture.path.join("metadata/roots/2.root.json"),
        &old_ids,
    );
    let mut old_only_client =
        SequentialTrustClient::bootstrap(&old_only_fixture.trusted_root, NOW).unwrap();
    let new_starved = old_only_client
        .load_windows_update(&old_only_fixture.path, NOW)
        .unwrap_err();
    assert!(matches!(
        new_starved,
        TrustError::Threshold {
            ref role,
            valid: 0,
            threshold: 2
        } if role == "root-new-threshold"
    ));
    println!(
        "TASK5170 dual_threshold_starvation_refused=2 old_role=root-old-threshold new_role=root-new-threshold"
    );
}
