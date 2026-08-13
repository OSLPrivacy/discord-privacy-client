//! Device-recipient key epochs for Enclaves.
//!
//! The relay is deliberately only an envelope courier.  It receives a key
//! grant for a recipient at delivery time, but its durable state contains no
//! epoch key (nor material from which one can be reconstructed).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

pub type AccountDevices = BTreeMap<String, BTreeSet<String>>;

/// The signed removal-progress checkpoint. It is a proof population, not an
/// admission ceiling: Enclaves continue accepting current-device recipients
/// above this value.
pub const SIGNED_REMOVAL_PROGRESS_THRESHOLD: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochRecord {
    pub epoch: u64,
    pub parent_hash: String,
    pub operation: String,
    pub accounts: BTreeSet<String>,
    pub device_rosters: AccountDevices,
    pub recipients: BTreeSet<String>,
    pub key_hash: String,
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MembershipOperation {
    Join {
        account: String,
        devices: BTreeSet<String>,
    },
    Leave {
        account: String,
    },
    RemoveDevice {
        account: String,
        device: String,
    },
}

impl MembershipOperation {
    fn name(&self) -> &'static str {
        match self {
            Self::Join { .. } => "join",
            Self::Leave { .. } => "leave",
            Self::RemoveDevice { .. } => "remove-device",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnclaveKeyError {
    Refused {
        account: String,
        device: String,
        epoch: u64,
        path: String,
    },
    InvalidOperation(String),
    InvalidEpoch(String),
    RelayKeyMaterial {
        epoch: u64,
        sink: String,
    },
    Decode(String),
}

impl std::fmt::Display for EnclaveKeyError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused {
                account,
                device,
                epoch,
                path,
            } => write!(
                out,
                "refused account={account} device={device} epoch={epoch} path={path}"
            ),
            Self::InvalidOperation(reason) | Self::InvalidEpoch(reason) | Self::Decode(reason) => {
                out.write_str(reason)
            }
            Self::RelayKeyMaterial { epoch, sink } => {
                write!(out, "relay retained key material epoch={epoch} sink={sink}")
            }
        }
    }
}

impl std::error::Error for EnclaveKeyError {}

#[derive(Clone)]
pub struct EnclaveKeyService {
    inner: Arc<Mutex<Authority>>,
}

#[derive(Clone)]
struct Authority {
    secret: String,
    place_id: String,
    devices: AccountDevices,
    records: Vec<EpochRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceClient {
    pub account: String,
    pub device: String,
    keys: BTreeMap<u64, Vec<u8>>,
    pub observed_accounts: BTreeSet<String>,
    pub observed_devices: AccountDevices,
    pub observed_key_hash: String,
    pub allowances: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ciphertext {
    pub content_id: String,
    pub epoch: u64,
    pub attachment: bool,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayConsumerRow {
    pub consumer: String,
    pub access: String,
}

/// A delivery courier. `retained` is private and is only populated by the
/// test-only hostile relay mutation used to prove the audit goes red.
#[derive(Clone, Debug, Default)]
pub struct Relay {
    retained: BTreeMap<String, Vec<u8>>,
    pub deliveries: usize,
}

impl EnclaveKeyService {
    pub fn new(
        place_id: impl Into<String>,
        secret: impl Into<String>,
        devices: AccountDevices,
        initial_epoch: u64,
    ) -> Result<Self, EnclaveKeyError> {
        if devices.values().any(BTreeSet::is_empty) {
            return Err(EnclaveKeyError::InvalidOperation(
                "an account has no signed current device roster".to_owned(),
            ));
        }
        let place_id = place_id.into();
        let secret = secret.into();
        let mut authority = Authority {
            secret,
            place_id,
            devices,
            records: Vec::new(),
        };
        let genesis = authority.record(initial_epoch, String::new(), "genesis")?;
        authority.records.push(genesis);
        Ok(Self {
            inner: Arc::new(Mutex::new(authority)),
        })
    }

    pub fn apply(&self, operation: MembershipOperation) -> Result<EpochRecord, EnclaveKeyError> {
        let mut authority = self
            .inner
            .lock()
            .expect("enclave key authority mutex poisoned");
        let mut candidate = authority.devices.clone();
        match &operation {
            MembershipOperation::Join { account, devices } => {
                if candidate.contains_key(account) || devices.is_empty() {
                    return Err(EnclaveKeyError::InvalidOperation(format!(
                        "join account={account} has duplicate account or empty roster"
                    )));
                }
                candidate.insert(account.clone(), devices.clone());
            }
            MembershipOperation::Leave { account } => {
                if candidate.remove(account).is_none() {
                    return Err(EnclaveKeyError::InvalidOperation(format!(
                        "leave account={account} is not a member"
                    )));
                }
            }
            MembershipOperation::RemoveDevice { account, device } => {
                let roster = candidate.get_mut(account).ok_or_else(|| {
                    EnclaveKeyError::InvalidOperation(format!(
                        "remove-device account={account} is not a member"
                    ))
                })?;
                if !roster.remove(device) {
                    return Err(EnclaveKeyError::InvalidOperation(format!(
                        "remove-device account={account} device={device} is absent"
                    )));
                }
                if roster.is_empty() {
                    return Err(EnclaveKeyError::InvalidOperation(format!(
                        "remove-device account={account} would leave no current devices"
                    )));
                }
            }
        }
        authority.devices = candidate;
        let parent_hash = record_hash(authority.records.last().expect("genesis record"));
        let next_epoch = authority.records.last().expect("genesis record").epoch + 1;
        let record = authority.record(next_epoch, parent_hash, operation.name())?;
        authority.records.push(record.clone());
        Ok(record)
    }

    pub fn records(&self) -> Vec<EpochRecord> {
        self.inner
            .lock()
            .expect("enclave key authority mutex poisoned")
            .records
            .clone()
    }

    pub fn current(&self) -> EpochRecord {
        self.inner
            .lock()
            .expect("enclave key authority mutex poisoned")
            .records
            .last()
            .expect("genesis record")
            .clone()
    }

    pub fn validate_history(&self) -> Result<(), EnclaveKeyError> {
        let authority = self
            .inner
            .lock()
            .expect("enclave key authority mutex poisoned");
        for (index, record) in authority.records.iter().enumerate() {
            if record.signature != sign(&authority.secret, record)? {
                return Err(EnclaveKeyError::InvalidEpoch(format!(
                    "invalid signature epoch={}",
                    record.epoch
                )));
            }
            if index > 0 {
                let parent = &authority.records[index - 1];
                if record.epoch != parent.epoch + 1 || record.parent_hash != record_hash(parent) {
                    return Err(EnclaveKeyError::InvalidEpoch(format!(
                        "replay/reorder/fork epoch={}",
                        record.epoch
                    )));
                }
            }
            if record.recipients != recipient_set(&record.device_rosters)
                || record.accounts != record.device_rosters.keys().cloned().collect()
            {
                return Err(EnclaveKeyError::InvalidEpoch(format!(
                    "recipient roster mismatch epoch={}",
                    record.epoch
                )));
            }
        }
        Ok(())
    }

    /// Refuses a replay, a reordered record, or an equal-epoch fork before it
    /// can become part of the canonical history.
    pub fn accept_external_epoch(&self, proposed: &EpochRecord) -> Result<(), EnclaveKeyError> {
        let authority = self
            .inner
            .lock()
            .expect("enclave key authority mutex poisoned");
        let last = authority.records.last().expect("genesis record");
        if proposed.epoch <= last.epoch
            || proposed.epoch != last.epoch + 1
            || proposed.parent_hash != record_hash(last)
        {
            return Err(EnclaveKeyError::InvalidEpoch(format!(
                "replay/reorder/fork epoch={}",
                proposed.epoch
            )));
        }
        if proposed.signature != sign(&authority.secret, proposed)? {
            return Err(EnclaveKeyError::InvalidEpoch(format!(
                "invalid signature epoch={}",
                proposed.epoch
            )));
        }
        Err(EnclaveKeyError::InvalidEpoch(format!(
            "external epoch={} is not an authority operation",
            proposed.epoch
        )))
    }

    pub fn grant(
        &self,
        epoch: u64,
        account: &str,
        device: &str,
        path: &str,
    ) -> Result<Vec<u8>, EnclaveKeyError> {
        let authority = self
            .inner
            .lock()
            .expect("enclave key authority mutex poisoned");
        let record = authority
            .records
            .iter()
            .find(|record| record.epoch == epoch)
            .ok_or_else(|| EnclaveKeyError::InvalidEpoch(format!("unknown epoch={epoch}")))?;
        let principal = principal(account, device);
        if !record.recipients.contains(&principal) {
            return Err(EnclaveKeyError::Refused {
                account: account.to_owned(),
                device: device.to_owned(),
                epoch,
                path: path.to_owned(),
            });
        }
        Ok(key_for(&authority.secret, &authority.place_id, record))
    }

    pub fn seal(
        &self,
        epoch: u64,
        content_id: impl Into<String>,
        attachment: bool,
        plaintext: &[u8],
    ) -> Result<Ciphertext, EnclaveKeyError> {
        let authority = self
            .inner
            .lock()
            .expect("enclave key authority mutex poisoned");
        let record = authority
            .records
            .iter()
            .find(|record| record.epoch == epoch)
            .ok_or_else(|| EnclaveKeyError::InvalidEpoch(format!("unknown epoch={epoch}")))?;
        Ok(Ciphertext {
            content_id: content_id.into(),
            epoch,
            attachment,
            bytes: xor(
                plaintext,
                &key_for(&authority.secret, &authority.place_id, record),
            ),
        })
    }

    pub fn roster_at(&self, epoch: u64) -> Result<EpochRecord, EnclaveKeyError> {
        self.inner
            .lock()
            .expect("enclave key authority mutex poisoned")
            .records
            .iter()
            .find(|record| record.epoch == epoch)
            .cloned()
            .ok_or_else(|| EnclaveKeyError::InvalidEpoch(format!("unknown epoch={epoch}")))
    }
}

impl Authority {
    fn record(
        &self,
        epoch: u64,
        parent_hash: String,
        operation: &str,
    ) -> Result<EpochRecord, EnclaveKeyError> {
        let mut record = EpochRecord {
            epoch,
            parent_hash,
            operation: operation.to_owned(),
            accounts: self.devices.keys().cloned().collect(),
            device_rosters: self.devices.clone(),
            recipients: recipient_set(&self.devices),
            key_hash: String::new(),
            signature: String::new(),
        };
        record.key_hash = digest_bytes(&key_for(&self.secret, &self.place_id, &record));
        record.signature = sign(&self.secret, &record)?;
        Ok(record)
    }
}

impl DeviceClient {
    pub fn new(account: impl Into<String>, device: impl Into<String>) -> Self {
        Self {
            account: account.into(),
            device: device.into(),
            keys: BTreeMap::new(),
            observed_accounts: BTreeSet::new(),
            observed_devices: BTreeMap::new(),
            observed_key_hash: String::new(),
            allowances: 0,
        }
    }

    pub fn receive(&mut self, record: &EpochRecord, key: Vec<u8>) -> Result<(), EnclaveKeyError> {
        if !record
            .recipients
            .contains(&principal(&self.account, &self.device))
            || digest_bytes(&key) != record.key_hash
        {
            return Err(EnclaveKeyError::Refused {
                account: self.account.clone(),
                device: self.device.clone(),
                epoch: record.epoch,
                path: "delivery".to_owned(),
            });
        }
        self.keys.insert(record.epoch, key);
        // Legitimate deliveries may be delayed or reordered.  A lower signed
        // epoch can add its historical key, but it must never roll a device's
        // observed current roster/key state backward.
        if self.keys.keys().next_back() == Some(&record.epoch) {
            self.observed_accounts = record.accounts.clone();
            self.observed_devices = record.device_rosters.clone();
            self.observed_key_hash = record.key_hash.clone();
        }
        self.allowances += 1;
        Ok(())
    }

    pub fn key_bytes(&self, epoch: u64) -> usize {
        self.keys.get(&epoch).map_or(0, Vec::len)
    }

    /// Test-only hostile cache injection. Shipping delivery must go through
    /// `receive`, which independently checks the signed recipient set.
    pub fn hostile_cache_for_red_proof(&mut self, epoch: u64, key: Vec<u8>) {
        self.keys.insert(epoch, key);
    }

    pub fn open(&self, ciphertext: &Ciphertext, path: &str) -> Result<Vec<u8>, EnclaveKeyError> {
        let key = self
            .keys
            .get(&ciphertext.epoch)
            .ok_or_else(|| EnclaveKeyError::Refused {
                account: self.account.clone(),
                device: self.device.clone(),
                epoch: ciphertext.epoch,
                path: path.to_owned(),
            })?;
        Ok(xor(&ciphertext.bytes, key))
    }
}

impl Relay {
    pub fn consumer_inventory() -> Vec<RelayConsumerRow> {
        vec![
            RelayConsumerRow {
                consumer: "mailbox".to_owned(),
                access: "recipient envelope".to_owned(),
            },
            RelayConsumerRow {
                consumer: "history".to_owned(),
                access: "ciphertext only".to_owned(),
            },
            RelayConsumerRow {
                consumer: "reconnect".to_owned(),
                access: "recipient envelope".to_owned(),
            },
            RelayConsumerRow {
                consumer: "restore".to_owned(),
                access: "ciphertext only".to_owned(),
            },
            RelayConsumerRow {
                consumer: "attachment".to_owned(),
                access: "ciphertext only".to_owned(),
            },
            RelayConsumerRow {
                consumer: "retry".to_owned(),
                access: "recipient envelope".to_owned(),
            },
            RelayConsumerRow {
                consumer: "alternate-endpoint".to_owned(),
                access: "recipient envelope".to_owned(),
            },
            RelayConsumerRow {
                consumer: "restart".to_owned(),
                access: "ciphertext only".to_owned(),
            },
        ]
    }

    pub fn deliver(
        &mut self,
        service: &EnclaveKeyService,
        epoch: u64,
        client: &mut DeviceClient,
        path: &str,
    ) -> Result<(), EnclaveKeyError> {
        let record = service.roster_at(epoch)?;
        let key = service.grant(epoch, &client.account, &client.device, path)?;
        client.receive(&record, key)?;
        self.deliveries += 1;
        Ok(())
    }

    /// The only unsafe API is deliberately named and used by the red proof.
    pub fn hostile_retain_raw(&mut self, sink: &str, raw: Vec<u8>) {
        self.retained.insert(sink.to_owned(), raw);
    }
    pub fn hostile_retain_derived(&mut self, sink: &str, seed: &[u8]) {
        self.retained.insert(sink.to_owned(), seed.to_vec());
    }
    pub fn hostile_key(&self, sink: &str) -> Option<&[u8]> {
        self.retained.get(sink).map(Vec::as_slice)
    }
    pub fn audit_no_key_material(&self, epoch: u64) -> Result<(), EnclaveKeyError> {
        self.retained.keys().next().map_or(Ok(()), |sink| {
            Err(EnclaveKeyError::RelayKeyMaterial {
                epoch,
                sink: sink.clone(),
            })
        })
    }
}

pub fn relay_open(ciphertext: &Ciphertext, raw_key: &[u8]) -> Vec<u8> {
    xor(&ciphertext.bytes, raw_key)
}

fn recipient_set(devices: &AccountDevices) -> BTreeSet<String> {
    devices
        .iter()
        .flat_map(|(account, roster)| roster.iter().map(move |device| principal(account, device)))
        .collect()
}
fn principal(account: &str, device: &str) -> String {
    format!("{account}/{device}")
}
fn record_hash(record: &EpochRecord) -> String {
    digest_bytes(&serde_json::to_vec(record).expect("epoch serialization"))
}
fn sign(secret: &str, record: &EpochRecord) -> Result<String, EnclaveKeyError> {
    let mut unsigned = record.clone();
    unsigned.signature.clear();
    serde_json::to_vec(&unsigned)
        .map(|bytes| digest(&[secret.as_bytes(), &bytes]))
        .map_err(|error| EnclaveKeyError::Decode(error.to_string()))
}
fn key_for(secret: &str, place_id: &str, record: &EpochRecord) -> Vec<u8> {
    digest(&[
        secret.as_bytes(),
        place_id.as_bytes(),
        record.epoch.to_string().as_bytes(),
        record.parent_hash.as_bytes(),
    ])
    .into_bytes()
}
fn digest(parts: &[&[u8]]) -> String {
    digest_bytes(&parts.concat())
}
fn digest_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn xor(bytes: &[u8], key: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect()
}
