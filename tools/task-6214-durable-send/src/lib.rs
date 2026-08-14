use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use fs2::FileExt;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub const PER_ACCOUNT_ITEMS: u64 = 2_000;
pub const PER_ACCOUNT_BYTES: u64 = 128 * 1024 * 1024;
pub const GLOBAL_ITEMS: u64 = 10_000;
pub const GLOBAL_BYTES: u64 = 1024 * 1024 * 1024;
pub const ABSOLUTE_DISK_FLOOR: u64 = 2 * 1024 * 1024 * 1024;
pub const FIXED_STORED_OVERHEAD: u64 = 4096;
pub const IN_FLIGHT_COPIES: u64 = 2;

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("state data malformed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("send {send_id} refused before reservation: {reason}; draft retained exact")]
    Refused { send_id: String, reason: String },
    #[error("send {send_id} cancellation refused: provider effect already committed")]
    CancelRefused { send_id: String },
    #[error("send {send_id} authority generation {generation} is cancelled")]
    Cancelled { send_id: String, generation: u64 },
    #[error("send {0} is missing")]
    Missing(String),
    #[error("provider protocol: {0}")]
    Provider(String),
    #[error("encryption or authentication failed")]
    Crypto,
}

pub type Result<T> = std::result::Result<T, SendError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Draft {
    pub account: String,
    pub recipient: String,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SendStatus {
    PrivateSaved,
    Pending,
    RequestObserved,
    ProviderCommitted,
    Sent,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Authority {
    Active,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendRecord {
    pub send_id: String,
    pub provider_id: String,
    pub account: String,
    pub recipient: String,
    pub payload_sha256: String,
    pub payload_plaintext_bytes: u64,
    pub reserved_worst_case_bytes: u64,
    pub status: SendStatus,
    pub authority: Authority,
    pub authority_generation: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Usage {
    pub items: u64,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub accounts: BTreeMap<String, Usage>,
    pub global: Usage,
    pub reservations: BTreeMap<String, Reservation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reservation {
    pub writer: String,
    pub account: String,
    pub items: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct VolumeSnapshot {
    pub free_bytes: u64,
    pub total_bytes: u64,
}

impl VolumeSnapshot {
    pub fn floor(self) -> u64 {
        ABSOLUTE_DISK_FLOOR.max(self.total_bytes / 10)
    }

    pub fn for_path(path: &Path) -> Result<Self> {
        Ok(Self {
            free_bytes: fs2::available_space(path)?,
            total_bytes: fs2::total_space(path)?,
        })
    }
}

pub fn measured_worst_case_bytes(plaintext_bytes: u64) -> u64 {
    // Ciphertext is nonce (24) + tag (16) + plaintext. We reserve two full
    // copies for atomic temp/rename and transport staging, plus a fixed page
    // each for indexes, journal growth, state, tombstone, and ledger growth.
    let encrypted = plaintext_bytes.saturating_add(40);
    encrypted
        .saturating_mul(IN_FLIGHT_COPIES)
        .saturating_add(FIXED_STORED_OVERHEAD)
}

pub struct Store {
    root: PathBuf,
    key: [u8; 32],
}

impl Clone for Store {
    fn clone(&self) -> Self {
        Self {
            root: self.root.clone(),
            key: self.key,
        }
    }
}

impl Store {
    pub fn open(root: impl Into<PathBuf>, key: [u8; 32]) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("sends"))?;
        fs::create_dir_all(root.join("drafts"))?;
        let lock = root.join("ledger.lock");
        if !lock.exists() {
            let _ = OpenOptions::new().create(true).write(true).open(lock)?;
        }
        let store = Self { root, key };
        store.with_lock(|this| {
            if !this.root.join("ledger.json").exists() {
                durable_json(&this.root.join("ledger.json"), &Ledger::default())?;
            }
            Ok(())
        })?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn with_lock<T>(&self, f: impl FnOnce(&Self) -> Result<T>) -> Result<T> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(self.root.join("ledger.lock"))?;
        if !mutant("stale_check_write") {
            file.lock_exclusive()?;
        }
        let result = f(self);
        if !mutant("stale_check_write") {
            file.unlock()?;
        }
        result
    }

    pub fn accept(
        &self,
        send_id: &str,
        writer: &str,
        draft: &Draft,
        volume: VolumeSnapshot,
    ) -> Result<SendRecord> {
        validate_component(send_id)?;
        self.save_draft(send_id, draft)?;
        hook("private-save", "after-effect-before-progress")?;
        let initial = SendRecord {
            send_id: send_id.to_owned(),
            provider_id: format!("osl-provider-v1-{send_id}"),
            account: draft.account.clone(),
            recipient: draft.recipient.clone(),
            payload_sha256: hex::encode(Sha256::digest(&draft.body)),
            payload_plaintext_bytes: draft.body.len() as u64,
            reserved_worst_case_bytes: measured_worst_case_bytes(draft.body.len() as u64),
            status: SendStatus::PrivateSaved,
            authority: Authority::Active,
            authority_generation: 1,
        };
        let initial_dir = self.send_dir(send_id);
        fs::create_dir_all(&initial_dir)?;
        if self.record_opt(send_id)?.is_none() {
            durable_json(&initial_dir.join("state.json"), &initial)?;
        }
        hook("private-save", "after-progress-commit")?;
        hook("service_acceptance", "before-effect")?;
        self.with_lock(|this| {
            let existing = this.record_opt(send_id)?;
            if let Some(record) = existing.as_ref() {
                if record.status != SendStatus::PrivateSaved {
                    return Ok(record.clone());
                }
            }
            let bytes = measured_worst_case_bytes(draft.body.len() as u64);
            let mut ledger = this.ledger()?;
            if mutant("stale_check_write") {
                thread::sleep(Duration::from_millis(50));
            }
            let account = ledger
                .accounts
                .get(&draft.account)
                .cloned()
                .unwrap_or_default();
            let floor = volume.floor();
            let existing_reservation = ledger.reservations.get(send_id).cloned();
            let already_reserved = existing_reservation.is_some();
            let added_items = if already_reserved { 0 } else { 1 };
            let added_bytes = if already_reserved { 0 } else { bytes };
            let over = |value: u64, limit: u64| {
                if mutant("off_by_one") {
                    value >= limit
                } else {
                    value > limit
                }
            };
            let refusal = if over(account.items.saturating_add(added_items), PER_ACCOUNT_ITEMS) {
                Some(format!("per-account item limit {PER_ACCOUNT_ITEMS}"))
            } else if over(account.bytes.saturating_add(added_bytes), PER_ACCOUNT_BYTES) {
                Some(format!("per-account byte limit {PER_ACCOUNT_BYTES}"))
            } else if over(
                ledger.global.items.saturating_add(added_items),
                GLOBAL_ITEMS,
            ) {
                Some(format!("global item limit {GLOBAL_ITEMS}"))
            } else if over(
                ledger.global.bytes.saturating_add(added_bytes),
                GLOBAL_BYTES,
            ) {
                Some(format!("global byte limit {GLOBAL_BYTES}"))
            } else if volume.free_bytes
                < floor
                    .saturating_add(ledger.global.bytes)
                    .saturating_add(added_bytes)
            {
                Some(format!("disk floor {floor}"))
            } else {
                None
            };
            if let Some(reason) = refusal {
                return Err(SendError::Refused {
                    send_id: send_id.to_owned(),
                    reason,
                });
            }
            let reservation = Reservation {
                writer: writer.to_owned(),
                account: draft.account.clone(),
                items: 1,
                bytes,
            };
            if !already_reserved {
                if mutant("accept_before_reservation") {
                    let mut premature = initial.clone();
                    premature.status = SendStatus::Pending;
                    durable_json(&this.send_dir(send_id).join("state.json"), &premature)?;
                } else {
                    ledger
                        .reservations
                        .insert(send_id.to_owned(), reservation.clone());
                    add_usage(&mut ledger, &reservation);
                    durable_json(&this.root.join("ledger.json"), &ledger)?;
                }
            }
            hook("service_acceptance", "after-effect-before-progress")?;
            let record = SendRecord {
                send_id: send_id.to_owned(),
                provider_id: format!("osl-provider-v1-{send_id}"),
                account: draft.account.clone(),
                recipient: draft.recipient.clone(),
                payload_sha256: initial.payload_sha256.clone(),
                payload_plaintext_bytes: draft.body.len() as u64,
                reserved_worst_case_bytes: bytes,
                status: SendStatus::Pending,
                authority: Authority::Active,
                authority_generation: 1,
            };
            let dir = this.send_dir(send_id);
            fs::create_dir_all(&dir)?;
            durable_write(
                &dir.join("payload.enc"),
                &seal(&this.key, send_id.as_bytes(), &draft.body)?,
            )?;
            durable_json(&dir.join("state.json"), &record)?;
            hook("service_acceptance", "after-progress-commit")?;
            Ok(record)
        })
    }

    pub fn recover_accept(
        &self,
        send_id: &str,
        writer: &str,
        volume: VolumeSnapshot,
    ) -> Result<SendRecord> {
        if let Some(record) = self.record_opt(send_id)? {
            if record.status != SendStatus::PrivateSaved {
                return Ok(record);
            }
        }
        let draft = self.load_draft(send_id)?;
        self.accept(send_id, writer, &draft, volume)
    }

    pub fn record(&self, send_id: &str) -> Result<SendRecord> {
        self.record_opt(send_id)?
            .ok_or_else(|| SendError::Missing(send_id.to_owned()))
    }

    pub fn open_payload(&self, send_id: &str) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        File::open(self.send_dir(send_id).join("payload.enc"))?.read_to_end(&mut bytes)?;
        open_sealed(&self.key, send_id.as_bytes(), &bytes)
    }

    pub fn can_cancel(&self, send_id: &str, provider_addr: &str) -> Result<bool> {
        let record = self.record(send_id)?;
        Ok(record.status != SendStatus::Sent
            && record.status != SendStatus::ProviderCommitted
            && record.authority == Authority::Active
            && !provider_query(provider_addr, &record, "EFFECT")?)
    }

    pub fn cancel(&self, send_id: &str, provider_addr: &str) -> Result<SendRecord> {
        self.with_lock(|this| {
            let mut record = this.record(send_id)?;
            if !mutant("false_cancel")
                && (record.status == SendStatus::Sent
                    || record.status == SendStatus::ProviderCommitted
                    || provider_query(provider_addr, &record, "EFFECT")?)
            {
                record.status = SendStatus::Sent;
                record.authority = Authority::Active;
                durable_json(&this.send_dir(send_id).join("state.json"), &record)?;
                this.release_usage(&record)?;
                return Err(SendError::CancelRefused {
                    send_id: send_id.to_owned(),
                });
            }
            if record.authority == Authority::Cancelled {
                return Ok(record);
            }
            record.authority_generation += 1;
            record.authority = Authority::Cancelled;
            record.status = SendStatus::Cancelled;
            // This authority file is the tombstone. It is committed and
            // fsynced before payload removal, and is never removed.
            durable_json(&this.send_dir(send_id).join("state.json"), &record)?;
            let payload = this.send_dir(send_id).join("payload.enc");
            if payload.exists() {
                fs::remove_file(payload)?;
                sync_dir(&this.send_dir(send_id))?;
            }
            this.release_usage(&record)?;
            if mutant("drop_row") {
                fs::remove_file(this.send_dir(send_id).join("state.json"))?;
            }
            Ok(record)
        })
    }

    pub fn ship(&self, send_id: &str, provider_addr: &str) -> Result<SendRecord> {
        let mut record = self.record(send_id)?;
        if record.authority == Authority::Cancelled && !mutant("replay_after_cancel") {
            return Err(SendError::Cancelled {
                send_id: send_id.to_owned(),
                generation: record.authority_generation,
            });
        }

        if provider_query(provider_addr, &record, "EFFECT")? {
            return self.confirm_sent(record);
        }

        if !provider_query(provider_addr, &record, "REQUEST")? {
            hook("local_save", "before-effect")?;
            provider_call(provider_addr, &record, "PREPARE")?;
            hook("local_save", "after-effect-before-progress")?;
            record = self.record(send_id)?;
            if record.authority == Authority::Cancelled && !mutant("replay_after_cancel") {
                return Err(SendError::Cancelled {
                    send_id: send_id.to_owned(),
                    generation: record.authority_generation,
                });
            }
            record.status = SendStatus::RequestObserved;
            durable_json(&self.send_dir(send_id).join("state.json"), &record)?;
            hook("local_save", "after-progress-commit")?;
            if mutant("success_early") {
                record.status = SendStatus::Sent;
                durable_json(&self.send_dir(send_id).join("state.json"), &record)?;
                return Ok(record);
            }
        }

        record = self.record(send_id)?;
        if record.authority == Authority::Cancelled && !mutant("replay_after_cancel") {
            return Err(SendError::Cancelled {
                send_id: send_id.to_owned(),
                generation: record.authority_generation,
            });
        }
        if !provider_query(provider_addr, &record, "EFFECT")? {
            hook("receiver_publish", "before-effect")?;
            // Mandatory immediate authority revalidation at provider commit.
            let stale = record.clone();
            let _committed = self.with_lock(|this| {
                let current = if mutant("no_revalidation") {
                    stale
                } else {
                    this.record(send_id)?
                };
                if current.authority == Authority::Cancelled && !mutant("replay_after_cancel") {
                    return Err(SendError::Cancelled {
                        send_id: send_id.to_owned(),
                        generation: current.authority_generation,
                    });
                }
                // Holding the same inter-process authority lock across the
                // final re-read and provider commit closes the last race: a
                // tombstone cannot commit between those two operations.
                provider_call(provider_addr, &current, "COMMIT")?;
                Ok(current)
            })?;
            hook("receiver_publish", "after-effect-before-progress")?;
            record = self.record(send_id)?;
            if record.status != SendStatus::Sent {
                record.status = SendStatus::ProviderCommitted;
                durable_json(&self.send_dir(send_id).join("state.json"), &record)?;
            }
            hook("receiver_publish", "after-progress-commit")?;
        }
        self.confirm_sent(record)
    }

    fn confirm_sent(&self, mut record: SendRecord) -> Result<SendRecord> {
        hook("final-confirmation", "before-effect")?;
        record.status = if mutant("leave_pending") {
            SendStatus::Pending
        } else {
            SendStatus::Sent
        };
        durable_json(&self.send_dir(&record.send_id).join("state.json"), &record)?;
        hook("final-confirmation", "after-effect-before-progress")?;
        self.release_usage(&record)?;
        hook("final-confirmation", "after-progress-commit")?;
        Ok(record)
    }

    fn save_draft(&self, send_id: &str, draft: &Draft) -> Result<()> {
        hook("private-save", "before-effect")?;
        let bytes = serde_json::to_vec(draft)?;
        durable_write(
            &self.root.join("drafts").join(format!("{send_id}.enc")),
            &seal(&self.key, format!("draft:{send_id}").as_bytes(), &bytes)?,
        )
    }

    pub fn load_draft(&self, send_id: &str) -> Result<Draft> {
        let mut bytes = Vec::new();
        File::open(self.root.join("drafts").join(format!("{send_id}.enc")))?
            .read_to_end(&mut bytes)?;
        Ok(serde_json::from_slice(&open_sealed(
            &self.key,
            format!("draft:{send_id}").as_bytes(),
            &bytes,
        )?)?)
    }

    fn release_usage(&self, record: &SendRecord) -> Result<()> {
        let mut ledger = self.ledger()?;
        if let Some(reservation) = ledger.reservations.remove(&record.send_id) {
            sub_usage(&mut ledger, &reservation);
            durable_json(&self.root.join("ledger.json"), &ledger)?;
        }
        Ok(())
    }

    pub fn ledger(&self) -> Result<Ledger> {
        read_json(&self.root.join("ledger.json"))
    }
    pub fn install_ledger_for_independent_audit(&self, ledger: &Ledger) -> Result<()> {
        self.with_lock(|this| durable_json(&this.root.join("ledger.json"), ledger))
    }
    fn record_opt(&self, send_id: &str) -> Result<Option<SendRecord>> {
        let path = self.send_dir(send_id).join("state.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json(&path)?))
    }
    fn send_dir(&self, send_id: &str) -> PathBuf {
        self.root.join("sends").join(send_id)
    }
}

fn add_usage(ledger: &mut Ledger, r: &Reservation) {
    let account = ledger.accounts.entry(r.account.clone()).or_default();
    account.items += r.items;
    account.bytes += r.bytes;
    ledger.global.items += r.items;
    ledger.global.bytes += r.bytes;
}
fn sub_usage(ledger: &mut Ledger, r: &Reservation) {
    if let Some(a) = ledger.accounts.get_mut(&r.account) {
        a.items -= r.items;
        a.bytes -= r.bytes;
    }
    ledger.global.items -= r.items;
    ledger.global.bytes -= r.bytes;
}

fn validate_component(value: &str) -> Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(SendError::Provider("invalid send id".into()));
    }
    Ok(())
}

fn mutant(name: &str) -> bool {
    std::env::var("OSL_6214_MUTANT").ok().as_deref() == Some(name)
}

fn seal(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce);
    let mut out = nonce.to_vec();
    out.extend(
        cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| SendError::Crypto)?,
    );
    Ok(out)
}
fn open_sealed(key: &[u8; 32], aad: &[u8], bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 40 {
        return Err(SendError::Crypto);
    }
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(&bytes[..24]),
            Payload {
                msg: &bytes[24..],
                aad,
            },
        )
        .map_err(|_| SendError::Crypto)
}

fn durable_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| SendError::Provider("path without parent".into()))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, path)?;
    sync_dir(parent)?;
    Ok(())
}
fn durable_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    durable_write(path, &serde_json::to_vec(value)?)
}
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_reader(File::open(path)?)?)
}
fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn hook(step: &str, position: &str) -> Result<()> {
    let spec = match std::env::var("OSL_6214_HOOK") {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    if spec != format!("{step}:{position}") {
        return Ok(());
    }
    let dir = PathBuf::from(
        std::env::var("OSL_6214_HOOK_DIR")
            .map_err(|_| SendError::Provider("hook dir missing".into()))?,
    );
    durable_write(&dir.join("reached"), spec.as_bytes())?;
    while !dir.join("release").exists() {
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

fn provider_line(addr: &str, line: &str) -> Result<String> {
    let mut stream = TcpStream::connect(addr)?;
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut out = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => out.push(byte[0]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                continue
            }
            Err(e) => return Err(e.into()),
        }
    }
    String::from_utf8(out).map_err(|_| SendError::Provider("non-utf8 reply".into()))
}
fn provider_call(addr: &str, record: &SendRecord, verb: &str) -> Result<()> {
    let reply = provider_line(
        addr,
        &format!(
            "{verb} {} {} {} {}",
            record.send_id, record.provider_id, record.recipient, record.payload_sha256
        ),
    )?;
    if reply == "OK" {
        Ok(())
    } else {
        Err(SendError::Provider(reply))
    }
}
fn provider_query(addr: &str, record: &SendRecord, kind: &str) -> Result<bool> {
    Ok(provider_line(
        addr,
        &format!("QUERY {kind} {} {}", record.send_id, record.provider_id),
    )? == "YES")
}

pub fn provider_serve(root: &Path, listener: std::net::TcpListener) -> Result<()> {
    fs::create_dir_all(root.join("requests"))?;
    fs::create_dir_all(root.join("objects"))?;
    fs::create_dir_all(root.join("receiver"))?;
    for incoming in listener.incoming() {
        let mut stream = incoming?;
        let mut line = String::new();
        loop {
            let mut b = [0u8; 1];
            if stream.read(&mut b)? == 0 || b[0] == b'\n' {
                break;
            }
            line.push(b[0] as char);
        }
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let answer = match parts.as_slice() {
            ["QUERY", kind, send, provider] => {
                let base = match *kind {
                    "REQUEST" => "requests",
                    "EFFECT" => "objects",
                    _ => "invalid",
                };
                if root.join(base).join(format!("{send}--{provider}")).exists() {
                    "YES"
                } else {
                    "NO"
                }
            }
            ["PREPARE", send, provider, recipient, hash] => {
                let body = format!(
                    "send={send}\nprovider={provider}\nrecipient={recipient}\nhash={hash}\n"
                );
                durable_write(
                    &root.join("requests").join(format!("{send}--{provider}")),
                    body.as_bytes(),
                )?;
                "OK"
            }
            ["COMMIT", send, provider, recipient, hash] => {
                let request = root.join("requests").join(format!("{send}--{provider}"));
                if !request.exists() {
                    "MISSING_REQUEST"
                } else {
                    let body = format!(
                        "send={send}\nprovider={provider}\nrecipient={recipient}\nhash={hash}\n"
                    );
                    let object = root.join("objects").join(format!("{send}--{provider}"));
                    if !object.exists() {
                        durable_write(&object, body.as_bytes())?;
                    }
                    let arrival = root.join("receiver").join(format!("{send}--{provider}"));
                    if !arrival.exists() {
                        durable_write(&arrival, body.as_bytes())?;
                    }
                    "OK"
                }
            }
            _ => "BAD_REQUEST",
        };
        // A shipping sender is deliberately SIGKILLed after external effects.
        // Its acknowledgement socket can therefore vanish; that must never
        // take the independent provider service down.
        let _ = stream.write_all(answer.as_bytes());
        let _ = stream.write_all(b"\n");
        let _ = stream.flush();
    }
    Ok(())
}
