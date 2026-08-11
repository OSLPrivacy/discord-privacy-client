//! Offline-rooted release metadata verification.
//!
//! The verifier intentionally contains no signing API. Production role keys
//! stay outside source control, CI, builders, websites, and application
//! packages; this crate accepts public metadata and artifacts only.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const REQUIRED_ARTIFACT_ROLES: [&str; 3] = ["update", "build-proof", "carrier-table"];
pub const REQUIRED_TOP_LEVEL_ROLES: [&str; 4] = ["root", "targets", "snapshot", "timestamp"];

#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    #[error("metadata I/O failed at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid JSON in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid metadata: {0}")]
    Invalid(String),
    #[error("signature key {keyid} is not authorized for role {role}")]
    WrongRole { role: String, keyid: String },
    #[error("role {role} has {valid} valid signatures; threshold is {threshold}")]
    Threshold {
        role: String,
        valid: usize,
        threshold: u32,
    },
    #[error("signature verification failed for role {role}, key {keyid}")]
    BadSignature { role: String, keyid: String },
    #[error("metadata role {role} expired at unix time {expires}; verification time is {now}")]
    Expired {
        role: String,
        expires: u64,
        now: u64,
    },
    #[error("trusted length mismatch for {name}: expected {expected}, got {actual}")]
    Length {
        name: String,
        expected: u64,
        actual: u64,
    },
    #[error("trusted sha256 mismatch for {name}: expected {expected}, got {actual}")]
    Hash {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("trusted version mismatch for {name}: expected {expected}, got {actual}")]
    Version {
        name: String,
        expected: u64,
        actual: u64,
    },
}

pub type Result<T> = std::result::Result<T, TrustError>;

#[derive(Clone, Debug, Deserialize)]
pub struct Envelope {
    pub signatures: Vec<MetadataSignature>,
    pub signed: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MetadataSignature {
    pub keyid: String,
    pub sig: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RootMetadata {
    #[serde(rename = "_type")]
    pub kind: String,
    pub spec_version: String,
    pub version: u64,
    pub expires: u64,
    pub keys: BTreeMap<String, PublicKey>,
    pub roles: BTreeMap<String, Role>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PublicKey {
    pub keytype: String,
    pub scheme: String,
    pub keyval: KeyValue,
}

#[derive(Clone, Debug, Deserialize)]
pub struct KeyValue {
    pub public: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Role {
    pub keyids: Vec<String>,
    pub threshold: u32,
}

#[derive(Clone, Debug, Deserialize)]
struct TimestampMetadata {
    #[serde(rename = "_type")]
    kind: String,
    spec_version: String,
    version: u64,
    expires: u64,
    meta: BTreeMap<String, MetaFile>,
}

#[derive(Clone, Debug, Deserialize)]
struct SnapshotMetadata {
    #[serde(rename = "_type")]
    kind: String,
    spec_version: String,
    version: u64,
    expires: u64,
    meta: BTreeMap<String, MetaFile>,
}

#[derive(Clone, Debug, Deserialize)]
struct TargetsMetadata {
    #[serde(rename = "_type")]
    kind: String,
    spec_version: String,
    version: u64,
    expires: u64,
    #[serde(default)]
    targets: BTreeMap<String, TargetFile>,
    #[serde(default)]
    delegations: Option<Delegations>,
}

#[derive(Clone, Debug, Deserialize)]
struct MetaFile {
    version: u64,
    length: u64,
    hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
struct TargetFile {
    length: u64,
    hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Delegations {
    keys: BTreeMap<String, PublicKey>,
    roles: Vec<DelegatedRole>,
}

#[derive(Clone, Debug, Deserialize)]
struct DelegatedRole {
    name: String,
    keyids: Vec<String>,
    threshold: u32,
    paths: Vec<String>,
    terminating: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedArtifact {
    pub role: String,
    pub path: String,
    pub length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub root_keys: usize,
    pub root_threshold: u32,
    pub verified_roles: Vec<String>,
    pub artifacts: Vec<VerifiedArtifact>,
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| TrustError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_envelope(path: &Path) -> Result<(Envelope, Vec<u8>)> {
    let bytes = read(path)?;
    let envelope = serde_json::from_slice(&bytes).map_err(|source| TrustError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    Ok((envelope, bytes))
}

/// RFC-8785-compatible for this schema: object keys are sorted, strings are
/// JSON escaped, arrays retain order, and metadata contains integer numbers.
pub fn canonical_json(value: &Value) -> Vec<u8> {
    fn append(value: &Value, out: &mut Vec<u8>) {
        match value {
            Value::Null => out.extend_from_slice(b"null"),
            Value::Bool(true) => out.extend_from_slice(b"true"),
            Value::Bool(false) => out.extend_from_slice(b"false"),
            Value::Number(number) => out.extend_from_slice(number.to_string().as_bytes()),
            Value::String(string) => {
                out.extend_from_slice(
                    serde_json::to_string(string)
                        .expect("serializing a JSON string cannot fail")
                        .as_bytes(),
                );
            }
            Value::Array(values) => {
                out.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    append(value, out);
                }
                out.push(b']');
            }
            Value::Object(values) => {
                out.push(b'{');
                let mut entries = values.iter().collect::<Vec<_>>();
                entries.sort_unstable_by_key(|(key, _)| *key);
                for (index, (key, value)) in entries.into_iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    out.extend_from_slice(
                        serde_json::to_string(key)
                            .expect("serializing a JSON key cannot fail")
                            .as_bytes(),
                    );
                    out.push(b':');
                    append(value, out);
                }
                out.push(b'}');
            }
        }
    }

    let mut out = Vec::new();
    append(value, &mut out);
    out
}

fn canonical_envelope(envelope: &Envelope) -> Result<Vec<u8>> {
    let value = serde_json::to_value(envelope_for_serialization(envelope)).map_err(|error| {
        TrustError::Invalid(format!("could not canonicalize envelope: {error}"))
    })?;
    Ok(canonical_json(&value))
}

#[derive(serde::Serialize)]
struct SerializableEnvelope<'a> {
    signatures: Vec<SerializableSignature<'a>>,
    signed: &'a Value,
}

#[derive(serde::Serialize)]
struct SerializableSignature<'a> {
    keyid: &'a str,
    sig: &'a str,
}

fn envelope_for_serialization(envelope: &Envelope) -> SerializableEnvelope<'_> {
    SerializableEnvelope {
        signatures: envelope
            .signatures
            .iter()
            .map(|signature| SerializableSignature {
                keyid: &signature.keyid,
                sig: &signature.sig,
            })
            .collect(),
        signed: &envelope.signed,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn decode_array<const N: usize>(encoded: &str, label: &str) -> Result<[u8; N]> {
    let bytes = hex::decode(encoded)
        .map_err(|_| TrustError::Invalid(format!("{label} must be lowercase hex")))?;
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        TrustError::Invalid(format!("{label} has {} bytes, expected {N}", bytes.len()))
    })
}

fn validate_key(keyid: &str, key: &PublicKey) -> Result<()> {
    if key.keytype != "ed25519" || key.scheme != "ed25519" {
        return Err(TrustError::Invalid(format!(
            "key {keyid} must use ed25519/ed25519"
        )));
    }
    let public = decode_array::<32>(&key.keyval.public, &format!("public key {keyid}"))?;
    let key_value = serde_json::json!({
        "keytype": key.keytype,
        "keyval": {"public": hex::encode(public)},
        "scheme": key.scheme,
    });
    let calculated = sha256_hex(&canonical_json(&key_value));
    if calculated != keyid {
        return Err(TrustError::Invalid(format!(
            "key id {keyid} does not match its canonical public key id {calculated}"
        )));
    }
    Ok(())
}

fn verify_signatures(
    role_name: &str,
    role: &Role,
    keys: &BTreeMap<String, PublicKey>,
    envelope: &Envelope,
) -> Result<()> {
    if role.threshold == 0 || role.threshold as usize > role.keyids.len() {
        return Err(TrustError::Invalid(format!(
            "role {role_name} has impossible threshold {} for {} keys",
            role.threshold,
            role.keyids.len()
        )));
    }
    let authorized = role.keyids.iter().collect::<BTreeSet<_>>();
    if authorized.len() != role.keyids.len() {
        return Err(TrustError::Invalid(format!(
            "role {role_name} repeats a key id"
        )));
    }
    let payload = canonical_json(&envelope.signed);
    let mut valid = BTreeSet::new();
    for signature in &envelope.signatures {
        if !authorized.contains(&signature.keyid) {
            return Err(TrustError::WrongRole {
                role: role_name.to_owned(),
                keyid: signature.keyid.clone(),
            });
        }
        let key = keys.get(&signature.keyid).ok_or_else(|| {
            TrustError::Invalid(format!(
                "role {role_name} references missing key {}",
                signature.keyid
            ))
        })?;
        validate_key(&signature.keyid, key)?;
        let public_bytes = decode_array::<32>(
            &key.keyval.public,
            &format!("public key {}", signature.keyid),
        )?;
        let public = VerifyingKey::from_bytes(&public_bytes).map_err(|_| {
            TrustError::Invalid(format!(
                "public key {} is not valid Ed25519",
                signature.keyid
            ))
        })?;
        let sig = Signature::from_bytes(&decode_array::<64>(
            &signature.sig,
            &format!("signature for {role_name}"),
        )?);
        if public.verify(&payload, &sig).is_err() {
            return Err(TrustError::BadSignature {
                role: role_name.to_owned(),
                keyid: signature.keyid.clone(),
            });
        }
        valid.insert(signature.keyid.as_str());
    }
    if valid.len() < role.threshold as usize {
        return Err(TrustError::Threshold {
            role: role_name.to_owned(),
            valid: valid.len(),
            threshold: role.threshold,
        });
    }
    Ok(())
}

fn typed<T: for<'de> Deserialize<'de>>(envelope: &Envelope, role: &str) -> Result<T> {
    serde_json::from_value(envelope.signed.clone())
        .map_err(|error| TrustError::Invalid(format!("invalid {role} signed body: {error}")))
}

fn check_common(kind: &str, spec_version: &str, expires: u64, role: &str, now: u64) -> Result<()> {
    let expected_kind = if role == "root" {
        "root"
    } else if role == "snapshot" {
        "snapshot"
    } else if role == "timestamp" {
        "timestamp"
    } else {
        "targets"
    };
    if kind != expected_kind {
        return Err(TrustError::Invalid(format!(
            "role {role} has _type {kind}, expected {expected_kind}"
        )));
    }
    if spec_version != "1.0.31" {
        return Err(TrustError::Invalid(format!(
            "role {role} has unsupported spec_version {spec_version}"
        )));
    }
    if expires <= now {
        return Err(TrustError::Expired {
            role: role.to_owned(),
            expires,
            now,
        });
    }
    Ok(())
}

fn required_role<'a>(root: &'a RootMetadata, name: &str) -> Result<&'a Role> {
    root.roles
        .get(name)
        .ok_or_else(|| TrustError::Invalid(format!("root metadata is missing role {name}")))
}

fn validate_root(root: &RootMetadata, envelope: &Envelope, now: u64) -> Result<()> {
    check_common(&root.kind, &root.spec_version, root.expires, "root", now)?;
    if root.version == 0 {
        return Err(TrustError::Invalid(
            "root version must be positive".to_owned(),
        ));
    }
    if root.roles.len() != REQUIRED_TOP_LEVEL_ROLES.len()
        || REQUIRED_TOP_LEVEL_ROLES
            .iter()
            .any(|name| !root.roles.contains_key(*name))
    {
        return Err(TrustError::Invalid(
            "root must name exactly root, targets, snapshot, and timestamp roles".to_owned(),
        ));
    }
    let root_role = required_role(root, "root")?;
    if root_role.keyids.len() != 3 || root_role.threshold != 2 {
        return Err(TrustError::Invalid(format!(
            "root role must name exactly 3 keys with threshold 2; got keys={} threshold={}",
            root_role.keyids.len(),
            root_role.threshold
        )));
    }

    let mut owners = BTreeMap::<&str, &str>::new();
    for role_name in REQUIRED_TOP_LEVEL_ROLES {
        let role = required_role(root, role_name)?;
        if role_name != "root" && (role.keyids.len() != 1 || role.threshold != 1) {
            return Err(TrustError::Invalid(format!(
                "subordinate role {role_name} must name exactly one key with threshold 1"
            )));
        }
        for keyid in &role.keyids {
            if !root.keys.contains_key(keyid) {
                return Err(TrustError::Invalid(format!(
                    "role {role_name} references absent key {keyid}"
                )));
            }
            if let Some(previous) = owners.insert(keyid, role_name) {
                return Err(TrustError::Invalid(format!(
                    "roles {previous} and {role_name} share key {keyid}"
                )));
            }
        }
    }
    if root.keys.len() != owners.len() {
        return Err(TrustError::Invalid(
            "root key map contains a key not assigned to a top-level role".to_owned(),
        ));
    }
    for (keyid, key) in &root.keys {
        validate_key(keyid, key)?;
    }
    verify_signatures("root", root_role, &root.keys, envelope)
}

fn check_meta(name: &str, expected: &MetaFile, envelope: &Envelope) -> Result<()> {
    let actual_version = envelope
        .signed
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| TrustError::Invalid(format!("{name} has no integer version")))?;
    if actual_version != expected.version {
        return Err(TrustError::Version {
            name: name.to_owned(),
            expected: expected.version,
            actual: actual_version,
        });
    }
    let bytes = canonical_envelope(envelope)?;
    check_length_hash(name, expected.length, &expected.hashes, &bytes)
}

fn check_length_hash(
    name: &str,
    expected_length: u64,
    hashes: &BTreeMap<String, String>,
    bytes: &[u8],
) -> Result<()> {
    let actual_length = bytes.len() as u64;
    if actual_length != expected_length {
        return Err(TrustError::Length {
            name: name.to_owned(),
            expected: expected_length,
            actual: actual_length,
        });
    }
    if hashes.len() != 1 || !hashes.contains_key("sha256") {
        return Err(TrustError::Invalid(format!(
            "{name} must have exactly one sha256 hash"
        )));
    }
    let expected = &hashes["sha256"];
    let actual = sha256_hex(bytes);
    if &actual != expected {
        return Err(TrustError::Hash {
            name: name.to_owned(),
            expected: expected.clone(),
            actual,
        });
    }
    Ok(())
}

fn safe_relative(path: &str) -> Result<&Path> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(TrustError::Invalid(format!(
            "unsafe target path {}",
            path.display()
        )));
    }
    Ok(path)
}

fn load_and_check_meta(trust_dir: &Path, name: &str, expected: &MetaFile) -> Result<Envelope> {
    let (envelope, _) = parse_envelope(&trust_dir.join("metadata").join(name))?;
    check_meta(name, expected, &envelope)?;
    Ok(envelope)
}

/// Verify the complete timestamp → snapshot → targets → delegated-role →
/// artifact chain rooted in `metadata/root.json`.
pub fn verify_repository(trust_dir: impl AsRef<Path>, now: u64) -> Result<VerificationReport> {
    let trust_dir = trust_dir.as_ref();
    let metadata_dir = trust_dir.join("metadata");

    let (root_envelope, _) = parse_envelope(&metadata_dir.join("root.json"))?;
    let root: RootMetadata = typed(&root_envelope, "root")?;
    validate_root(&root, &root_envelope, now)?;

    let (timestamp_envelope, _) = parse_envelope(&metadata_dir.join("timestamp.json"))?;
    verify_signatures(
        "timestamp",
        required_role(&root, "timestamp")?,
        &root.keys,
        &timestamp_envelope,
    )?;
    let timestamp: TimestampMetadata = typed(&timestamp_envelope, "timestamp")?;
    check_common(
        &timestamp.kind,
        &timestamp.spec_version,
        timestamp.expires,
        "timestamp",
        now,
    )?;
    if timestamp.version == 0 || timestamp.meta.len() != 1 {
        return Err(TrustError::Invalid(
            "timestamp must contain exactly snapshot.json".to_owned(),
        ));
    }
    let snapshot_ref = timestamp
        .meta
        .get("snapshot.json")
        .ok_or_else(|| TrustError::Invalid("timestamp is missing snapshot.json".to_owned()))?;
    let snapshot_envelope = load_and_check_meta(trust_dir, "snapshot.json", snapshot_ref)?;
    verify_signatures(
        "snapshot",
        required_role(&root, "snapshot")?,
        &root.keys,
        &snapshot_envelope,
    )?;
    let snapshot: SnapshotMetadata = typed(&snapshot_envelope, "snapshot")?;
    check_common(
        &snapshot.kind,
        &snapshot.spec_version,
        snapshot.expires,
        "snapshot",
        now,
    )?;
    if snapshot.version == 0 {
        return Err(TrustError::Invalid(
            "snapshot version must be positive".to_owned(),
        ));
    }

    let targets_ref = snapshot
        .meta
        .get("targets.json")
        .ok_or_else(|| TrustError::Invalid("snapshot is missing targets.json".to_owned()))?;
    let targets_envelope = load_and_check_meta(trust_dir, "targets.json", targets_ref)?;
    verify_signatures(
        "targets",
        required_role(&root, "targets")?,
        &root.keys,
        &targets_envelope,
    )?;
    let targets: TargetsMetadata = typed(&targets_envelope, "targets")?;
    check_common(
        &targets.kind,
        &targets.spec_version,
        targets.expires,
        "targets",
        now,
    )?;
    if targets.version == 0 || !targets.targets.is_empty() {
        return Err(TrustError::Invalid(
            "top-level targets must delegate all release artifacts".to_owned(),
        ));
    }
    let delegations = targets
        .delegations
        .ok_or_else(|| TrustError::Invalid("targets metadata has no delegations".to_owned()))?;
    if delegations.roles.len() != REQUIRED_ARTIFACT_ROLES.len() {
        return Err(TrustError::Invalid(format!(
            "targets must have exactly {} delegated roles",
            REQUIRED_ARTIFACT_ROLES.len()
        )));
    }

    let mut delegation_key_owners = BTreeMap::<&str, &str>::new();
    let mut artifacts = Vec::new();
    for expected_role_name in REQUIRED_ARTIFACT_ROLES {
        let delegated = delegations
            .roles
            .iter()
            .find(|role| role.name == expected_role_name)
            .ok_or_else(|| {
                TrustError::Invalid(format!(
                    "targets is missing delegation {expected_role_name}"
                ))
            })?;
        if delegated.keyids.len() != 1 || delegated.threshold != 1 || !delegated.terminating {
            return Err(TrustError::Invalid(format!(
                "delegation {expected_role_name} must have one key, threshold 1, and terminate"
            )));
        }
        let expected_path = format!("{expected_role_name}/");
        if delegated.paths != [expected_path.clone()] {
            return Err(TrustError::Invalid(format!(
                "delegation {expected_role_name} must own only {expected_path}"
            )));
        }
        let keyid = &delegated.keyids[0];
        if root.keys.contains_key(keyid) {
            return Err(TrustError::Invalid(format!(
                "delegated role {expected_role_name} reuses top-level key {keyid}"
            )));
        }
        if let Some(previous) = delegation_key_owners.insert(keyid, expected_role_name) {
            return Err(TrustError::Invalid(format!(
                "delegated roles {previous} and {expected_role_name} share key {keyid}"
            )));
        }
        let key = delegations.keys.get(keyid).ok_or_else(|| {
            TrustError::Invalid(format!(
                "delegation {expected_role_name} references absent key {keyid}"
            ))
        })?;
        validate_key(keyid, key)?;

        let metadata_name = format!("delegated/{expected_role_name}.json");
        let delegated_ref = snapshot
            .meta
            .get(&metadata_name)
            .ok_or_else(|| TrustError::Invalid(format!("snapshot is missing {metadata_name}")))?;
        let delegated_envelope = load_and_check_meta(trust_dir, &metadata_name, delegated_ref)?;
        let role = Role {
            keyids: delegated.keyids.clone(),
            threshold: delegated.threshold,
        };
        verify_signatures(
            expected_role_name,
            &role,
            &delegations.keys,
            &delegated_envelope,
        )?;
        let delegated_targets: TargetsMetadata = typed(&delegated_envelope, expected_role_name)?;
        check_common(
            &delegated_targets.kind,
            &delegated_targets.spec_version,
            delegated_targets.expires,
            expected_role_name,
            now,
        )?;
        if delegated_targets.version == 0
            || delegated_targets.delegations.is_some()
            || delegated_targets.targets.len() != 1
        {
            return Err(TrustError::Invalid(format!(
                "delegated role {expected_role_name} must name exactly one artifact"
            )));
        }
        let (artifact_path, target) = delegated_targets.targets.iter().next().unwrap();
        if !artifact_path.starts_with(&expected_path) {
            return Err(TrustError::Invalid(format!(
                "role {expected_role_name} cannot authorize artifact {artifact_path}"
            )));
        }
        let relative = safe_relative(artifact_path)?;
        let artifact_bytes = read(&trust_dir.join("artifacts").join(relative))?;
        check_length_hash(
            artifact_path,
            target.length,
            &target.hashes,
            &artifact_bytes,
        )?;
        artifacts.push(VerifiedArtifact {
            role: expected_role_name.to_owned(),
            path: artifact_path.clone(),
            length: artifact_bytes.len() as u64,
            sha256: sha256_hex(&artifact_bytes),
        });
    }

    if delegations.keys.len() != delegation_key_owners.len() {
        return Err(TrustError::Invalid(
            "delegations contain a key not assigned to an artifact role".to_owned(),
        ));
    }
    let expected_snapshot_entries = REQUIRED_ARTIFACT_ROLES.len() + 1;
    if snapshot.meta.len() != expected_snapshot_entries {
        return Err(TrustError::Invalid(format!(
            "snapshot must name exactly {expected_snapshot_entries} metadata files"
        )));
    }

    Ok(VerificationReport {
        root_keys: required_role(&root, "root")?.keyids.len(),
        root_threshold: required_role(&root, "root")?.threshold,
        verified_roles: vec![
            "root".to_owned(),
            "timestamp".to_owned(),
            "snapshot".to_owned(),
            "targets".to_owned(),
            "update".to_owned(),
            "build-proof".to_owned(),
            "carrier-table".to_owned(),
        ],
        artifacts,
    })
}
