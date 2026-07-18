use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, Uri},
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng, Payload},
    XChaCha20Poly1305, XNonce,
};
use ed25519_dalek::{Signer, SigningKey};
use hmac::{Hmac, Mac};
use rand::RngCore;
use reqwest::StatusCode;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;
use url::Url;

type HmacSha256 = Hmac<Sha256>;
const MAX_INVOICE_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("configuration rejected: {0}")]
    Config(String),
    #[error("wallet RPC failed: {0}")]
    Rpc(String),
    #[error("storage failed: {0}")]
    Store(String),
    #[error("callback failed: {0}")]
    Callback(String),
    #[error("request rejected: {0}")]
    Request(String),
}

#[derive(Clone)]
pub struct Config {
    pub bitcoin_rpc_url: Url,
    pub bitcoin_cookie_file: String,
    pub bitcoin_wallet: String,
    pub monero_wallet_rpc_url: Url,
    pub monero_account_index: u32,
    pub monero_primary_address: String,
    pub callback_url: Url,
    pub request_secret: Vec<u8>,
    pub settlement_signing_key: SigningKey,
    pub btc_confirmations: u32,
    pub xmr_confirmations: u32,
    pub invoice_retention_seconds: i64,
}

impl Config {
    pub fn validate(&self) -> Result<(), WatcherError> {
        require_loopback_http(&self.bitcoin_rpc_url)?;
        require_loopback_http(&self.monero_wallet_rpc_url)?;
        if self.monero_account_index != 0 {
            return Err(WatcherError::Config(
                "Monero account index must be 0 for primary-address pinning".into(),
            ));
        }
        validate_monero_address(&self.monero_primary_address)?;
        if self.callback_url.scheme() != "https" || self.callback_url.host_str().is_none() {
            return Err(WatcherError::Config("callback URL must be HTTPS".into()));
        }
        if self.request_secret.len() < 32 {
            return Err(WatcherError::Config(
                "request secret must be at least 32 bytes".into(),
            ));
        }
        if self.btc_confirmations == 0 || self.xmr_confirmations == 0 {
            return Err(WatcherError::Config(
                "confirmations must be positive".into(),
            ));
        }
        if !(1..=MAX_INVOICE_RETENTION_SECONDS).contains(&self.invoice_retention_seconds) {
            return Err(WatcherError::Config(
                "invoice retention must be between 1 second and 7 days".into(),
            ));
        }
        Ok(())
    }
}

fn require_loopback_http(url: &Url) -> Result<(), WatcherError> {
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "http" || !matches!(host, "127.0.0.1" | "::1" | "localhost") {
        return Err(WatcherError::Config(
            "wallet RPC must be loopback HTTP".into(),
        ));
    }
    Ok(())
}

fn validate_monero_address(address: &str) -> Result<(), WatcherError> {
    const MONERO_BASE58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    if address.len() != 95 || !address.chars().all(|ch| MONERO_BASE58.contains(ch)) {
        return Err(WatcherError::Config(
            "pinned Monero primary address is malformed".into(),
        ));
    }
    Ok(())
}

fn verify_monero_wallet_identity(expected: &str, actual: &str) -> Result<(), WatcherError> {
    if expected != actual {
        return Err(WatcherError::Rpc(
            "Monero wallet does not match the pinned view-only wallet".into(),
        ));
    }
    Ok(())
}

/// Read a small, non-symlinked credential file without exposing its contents.
/// On Unix, group/other permission bits are rejected before reading.
pub fn read_secret_file(path: &Path) -> Result<String, WatcherError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| WatcherError::Config("credential file is unavailable".into()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 16 * 1024 {
        return Err(WatcherError::Config(
            "credential path must be a small regular file".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(WatcherError::Config(
                "credential file must not be accessible by group or others".into(),
            ));
        }
    }
    let value = fs::read_to_string(path)
        .map_err(|_| WatcherError::Config("credential file is unreadable".into()))?;
    let value = value.trim().to_owned();
    if value.is_empty() {
        return Err(WatcherError::Config("credential file is empty".into()));
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Asset {
    Btc,
    Xmr,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateInvoiceRequest {
    pub invoice_id: String,
    pub payment_method: Asset,
    pub amount_atomic: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateInvoiceResponse {
    pub invoice_id: String,
    pub address: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StoredInvoice {
    invoice_id: String,
    payment_method: Asset,
    amount_atomic: String,
    address: String,
    subaddress_index: Option<u32>,
    expires_at: i64,
    observed_at: Option<i64>,
    locked_payment_refs: Vec<PaymentReference>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaymentReference {
    pub txid: String,
    pub amount_atomic: u128,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PaymentObservation {
    pub txid: String,
    pub amount_atomic: u128,
    pub confirmations: u32,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Observation {
    pub payments: Vec<PaymentObservation>,
}

#[async_trait]
pub trait WalletRpc: Send + Sync {
    async fn validate_watch_only(&self) -> Result<(), WatcherError>;
    async fn allocate(&self, asset: Asset) -> Result<(String, Option<u32>), WatcherError>;
    async fn observe(
        &self,
        invoice: &StoredInvoice,
        confirmations: u32,
    ) -> Result<Observation, WatcherError>;
}

#[derive(Clone)]
pub struct CoreWalletRpc {
    client: reqwest::Client,
    config: Arc<Config>,
}

impl CoreWalletRpc {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(8))
                .build()
                .expect("static client config"),
            config,
        }
    }

    async fn bitcoin<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, WatcherError> {
        let cookie = std::fs::read_to_string(&self.config.bitcoin_cookie_file)
            .map_err(|e| WatcherError::Rpc(format!("Bitcoin cookie unavailable: {e}")))?;
        let (user, password) = cookie
            .trim()
            .split_once(':')
            .ok_or_else(|| WatcherError::Rpc("Bitcoin cookie malformed".into()))?;
        let url = self
            .config
            .bitcoin_rpc_url
            .join(&format!("wallet/{}", self.config.bitcoin_wallet))
            .map_err(|e| WatcherError::Rpc(e.to_string()))?;
        rpc_call(
            self.client.post(url).basic_auth(user, Some(password)),
            method,
            params,
        )
        .await
    }

    async fn monero<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, WatcherError> {
        let url = self
            .config
            .monero_wallet_rpc_url
            .join("json_rpc")
            .map_err(|e| WatcherError::Rpc(e.to_string()))?;
        rpc_call(self.client.post(url), method, params).await
    }
}

async fn rpc_call<T: DeserializeOwned>(
    builder: reqwest::RequestBuilder,
    method: &str,
    params: Value,
) -> Result<T, WatcherError> {
    let response = builder
        .json(&json!({"jsonrpc":"2.0","id":"osl","method":method,"params":params}))
        .send()
        .await
        .map_err(|e| WatcherError::Rpc(e.to_string()))?;
    if !response.status().is_success() {
        return Err(WatcherError::Rpc(format!("HTTP {}", response.status())));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|e| WatcherError::Rpc(e.to_string()))?;
    if let Some(error) = value.get("error") {
        return Err(WatcherError::Rpc(error.to_string()));
    }
    serde_json::from_value(
        value
            .get("result")
            .cloned()
            .ok_or_else(|| WatcherError::Rpc("missing result".into()))?,
    )
    .map_err(|e| WatcherError::Rpc(e.to_string()))
}

#[derive(Deserialize)]
struct BitcoinWalletInfo {
    private_keys_enabled: bool,
    descriptors: bool,
}
#[derive(Deserialize)]
struct BitcoinReceivedAddress {
    #[serde(default)]
    txids: Vec<String>,
}
#[derive(Deserialize)]
struct BitcoinTransaction {
    confirmations: i64,
    #[serde(default)]
    walletconflicts: Vec<String>,
    #[serde(default)]
    details: Vec<BitcoinTransactionDetail>,
}
#[derive(Deserialize)]
struct BitcoinTransactionDetail {
    address: Option<String>,
    category: String,
    amount: Value,
    #[serde(default)]
    abandoned: bool,
}
#[derive(Deserialize)]
struct MoneroAddress {
    address: String,
    address_index: u32,
}
#[derive(Deserialize)]
struct MoneroAccountAddress {
    address: String,
}
#[derive(Deserialize)]
struct MoneroTransfers {
    #[serde(default)]
    r#in: Vec<MoneroTransfer>,
    #[serde(default)]
    pool: Vec<MoneroTransfer>,
}
#[derive(Deserialize)]
struct MoneroTransfer {
    txid: String,
    amount: u64,
    confirmations: u32,
    double_spend_seen: bool,
    locked: bool,
    unlock_time: u64,
    subaddr_index: SubaddressIndex,
}
#[derive(Deserialize)]
struct SubaddressIndex {
    major: u32,
    minor: u32,
}

#[async_trait]
impl WalletRpc for CoreWalletRpc {
    async fn validate_watch_only(&self) -> Result<(), WatcherError> {
        let info: BitcoinWalletInfo = self.bitcoin("getwalletinfo", json!([])).await?;
        if info.private_keys_enabled || !info.descriptors {
            return Err(WatcherError::Rpc(
                "Bitcoin wallet is not a descriptor watch-only wallet".into(),
            ));
        }
        let _: Value = self.monero("get_version", json!({})).await?;
        let monero: MoneroAccountAddress = self
            .monero(
                "get_address",
                json!({"account_index": self.config.monero_account_index}),
            )
            .await?;
        verify_monero_wallet_identity(&self.config.monero_primary_address, &monero.address)?;
        Ok(())
    }

    async fn allocate(&self, asset: Asset) -> Result<(String, Option<u32>), WatcherError> {
        match asset {
            Asset::Btc => {
                let address: String = self.bitcoin("getnewaddress", json!(["", "bech32"])).await?;
                Ok((address, None))
            }
            Asset::Xmr => {
                let result: MoneroAddress = self.monero("create_address", json!({"account_index":self.config.monero_account_index,"label":"","count":1})).await?;
                Ok((result.address, Some(result.address_index)))
            }
        }
    }

    async fn observe(
        &self,
        invoice: &StoredInvoice,
        _required: u32,
    ) -> Result<Observation, WatcherError> {
        match invoice.payment_method {
            Asset::Btc => {
                let received: Vec<BitcoinReceivedAddress> = self
                    .bitcoin(
                        "listreceivedbyaddress",
                        json!([0, true, true, invoice.address]),
                    )
                    .await?;
                let txids: BTreeSet<String> =
                    received.into_iter().flat_map(|entry| entry.txids).collect();
                let mut payments = Vec::with_capacity(txids.len());
                for txid in txids {
                    validate_txid(&txid)?;
                    let transaction: BitcoinTransaction = self
                        .bitcoin("gettransaction", json!([&txid, true, false]))
                        .await?;
                    if transaction.confirmations < 0 || !transaction.walletconflicts.is_empty() {
                        continue;
                    }
                    let amount_atomic = transaction
                        .details
                        .iter()
                        .filter(|detail| {
                            detail.category == "receive"
                                && detail.address.as_deref() == Some(invoice.address.as_str())
                                && !detail.abandoned
                        })
                        .try_fold(0_u128, |total, detail| {
                            total
                                .checked_add(btc_value_to_sats(&detail.amount)?)
                                .ok_or_else(|| WatcherError::Rpc("Bitcoin amount overflow".into()))
                        })?;
                    if amount_atomic == 0 {
                        continue;
                    }
                    payments.push(PaymentObservation {
                        txid,
                        amount_atomic,
                        confirmations: u32::try_from(transaction.confirmations).unwrap_or(u32::MAX),
                    });
                }
                Ok(Observation { payments })
            }
            Asset::Xmr => {
                let minor = invoice
                    .subaddress_index
                    .ok_or_else(|| WatcherError::Store("missing Monero subaddress index".into()))?;
                let transfers: MoneroTransfers = self.monero("get_transfers", json!({"in":true,"pool":true,"account_index":self.config.monero_account_index,"subaddr_indices":[minor]})).await?;
                Ok(Observation {
                    payments: monero_payment_observations(
                        transfers,
                        self.config.monero_account_index,
                        minor,
                    )?,
                })
            }
        }
    }
}

fn validate_txid(txid: &str) -> Result<(), WatcherError> {
    if txid.len() != 64
        || !txid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(WatcherError::Rpc(
            "wallet returned a malformed transaction id".into(),
        ));
    }
    Ok(())
}

fn monero_payment_observations(
    transfers: MoneroTransfers,
    account_index: u32,
    subaddress_index: u32,
) -> Result<Vec<PaymentObservation>, WatcherError> {
    fn aggregate(
        transfers: Vec<MoneroTransfer>,
        account_index: u32,
        subaddress_index: u32,
    ) -> Result<BTreeMap<String, PaymentObservation>, WatcherError> {
        let mut payments: BTreeMap<String, PaymentObservation> = BTreeMap::new();
        for transfer in transfers.into_iter().filter(|transfer| {
            transfer.subaddr_index.major == account_index
                && transfer.subaddr_index.minor == subaddress_index
                && !transfer.double_spend_seen
        }) {
            validate_txid(&transfer.txid)?;
            let entry = payments
                .entry(transfer.txid.clone())
                .or_insert(PaymentObservation {
                    txid: transfer.txid,
                    amount_atomic: 0,
                    confirmations: transfer.confirmations,
                });
            entry.amount_atomic = entry
                .amount_atomic
                .checked_add(u128::from(transfer.amount))
                .ok_or_else(|| WatcherError::Rpc("Monero amount overflow".into()))?;
            entry.confirmations = entry.confirmations.min(transfer.confirmations);
            if transfer.locked || transfer.unlock_time != 0 {
                entry.confirmations = 0;
            }
        }
        payments.retain(|_, payment| payment.amount_atomic > 0);
        Ok(payments)
    }

    let mut confirmed = aggregate(transfers.r#in, account_index, subaddress_index)?;
    for (txid, payment) in aggregate(transfers.pool, account_index, subaddress_index)? {
        confirmed.entry(txid).or_insert(payment);
    }
    Ok(confirmed.into_values().collect())
}

fn indexed_payments(
    observation: &Observation,
) -> Result<BTreeMap<&str, &PaymentObservation>, WatcherError> {
    let mut payments = BTreeMap::new();
    for payment in &observation.payments {
        validate_txid(&payment.txid)?;
        if payment.amount_atomic == 0 || payments.insert(payment.txid.as_str(), payment).is_some() {
            return Err(WatcherError::Rpc(
                "wallet returned duplicate or zero-value payment references".into(),
            ));
        }
    }
    Ok(payments)
}

fn select_payment_refs(
    observation: &Observation,
    required_amount: u128,
) -> Result<Option<Vec<PaymentReference>>, WatcherError> {
    let payments = indexed_payments(observation)?;
    let mut candidates: Vec<_> = payments.into_values().collect();
    candidates.sort_by(|left, right| {
        right
            .amount_atomic
            .cmp(&left.amount_atomic)
            .then_with(|| left.txid.cmp(&right.txid))
    });
    let mut selected = Vec::new();
    let mut total = 0_u128;
    for payment in candidates {
        total = total
            .checked_add(payment.amount_atomic)
            .ok_or_else(|| WatcherError::Rpc("payment amount overflow".into()))?;
        selected.push(PaymentReference {
            txid: payment.txid.clone(),
            amount_atomic: payment.amount_atomic,
        });
        if total >= required_amount {
            return Ok(Some(selected));
        }
    }
    Ok(None)
}

fn confirmed_locked_observation(
    observation: &Observation,
    locked: &[PaymentReference],
    required_confirmations: u32,
    required_amount: u128,
) -> Result<Option<Observation>, WatcherError> {
    if locked.is_empty() {
        return Ok(None);
    }
    let current = indexed_payments(observation)?;
    let mut payments = Vec::with_capacity(locked.len());
    let mut amount = 0_u128;
    for reference in locked {
        validate_txid(&reference.txid)?;
        let Some(payment) = current.get(reference.txid.as_str()) else {
            return Ok(None);
        };
        if payment.amount_atomic != reference.amount_atomic
            || payment.confirmations < required_confirmations
        {
            return Ok(None);
        }
        amount = amount
            .checked_add(payment.amount_atomic)
            .ok_or_else(|| WatcherError::Rpc("payment amount overflow".into()))?;
        payments.push((*payment).clone());
    }
    if amount < required_amount {
        return Ok(None);
    }
    Ok(Some(Observation { payments }))
}

fn observation_totals(observation: &Observation) -> Result<(u128, u32), WatcherError> {
    let payments = indexed_payments(observation)?;
    let amount = payments.values().try_fold(0_u128, |total, payment| {
        total
            .checked_add(payment.amount_atomic)
            .ok_or_else(|| WatcherError::Rpc("payment amount overflow".into()))
    })?;
    let confirmations = payments
        .values()
        .map(|payment| payment.confirmations)
        .min()
        .unwrap_or(0);
    Ok((amount, confirmations))
}

fn btc_value_to_sats(value: &Value) -> Result<u128, WatcherError> {
    let text = match value {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => return Err(WatcherError::Rpc("Bitcoin amount is not numeric".into())),
    };
    decimal_to_atomic(&text, 8)
}

fn decimal_to_atomic(value: &str, decimals: usize) -> Result<u128, WatcherError> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > decimals
    {
        return Err(WatcherError::Rpc(
            "asset amount precision is invalid".into(),
        ));
    }
    format!("{whole}{fraction:0<decimals$}")
        .parse()
        .map_err(|_| WatcherError::Rpc("asset amount overflow".into()))
}

pub struct InvoiceStore {
    connection: Mutex<Connection>,
    cipher: XChaCha20Poly1305,
    index_key: [u8; 32],
}

impl InvoiceStore {
    pub fn open(path: &Path, key: &[u8; 32]) -> Result<Self, WatcherError> {
        let connection = Connection::open(path).map_err(store_error)?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS invoices (id_hash BLOB PRIMARY KEY, address_hash BLOB NOT NULL UNIQUE, nonce BLOB NOT NULL, ciphertext BLOB NOT NULL, status TEXT NOT NULL CHECK(status IN ('pending','settled')), expires_at INTEGER NOT NULL, cleanup_at INTEGER NOT NULL);").map_err(store_error)?;
        let mut derivation = <HmacSha256 as Mac>::new_from_slice(key)
            .map_err(|_| WatcherError::Store("invoice index key derivation failed".into()))?;
        derivation.update(b"osl-watcher-index-key-v1");
        let mut index_key = [0_u8; 32];
        index_key.copy_from_slice(&derivation.finalize().into_bytes());
        Ok(Self {
            connection: Mutex::new(connection),
            cipher: XChaCha20Poly1305::new(key.into()),
            index_key,
        })
    }

    fn hash(&self, value: &str) -> Vec<u8> {
        let mut mac =
            <HmacSha256 as Mac>::new_from_slice(&self.index_key).expect("fixed-size HMAC key");
        mac.update(b"osl-watcher-index-v1\0");
        mac.update(value.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    fn encode(&self, invoice: &StoredInvoice) -> Result<(Vec<u8>, Vec<u8>), WatcherError> {
        let mut nonce = [0_u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let plaintext =
            serde_json::to_vec(invoice).map_err(|e| WatcherError::Store(e.to_string()))?;
        let id_hash = self.hash(&invoice.invoice_id);
        let ciphertext = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: &id_hash,
                },
            )
            .map_err(|_| WatcherError::Store("invoice encryption failed".into()))?;
        Ok((nonce.to_vec(), ciphertext))
    }

    fn decode(
        &self,
        nonce: &[u8],
        ciphertext: &[u8],
        id_hash: &[u8],
    ) -> Result<StoredInvoice, WatcherError> {
        let plaintext = self
            .cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: id_hash,
                },
            )
            .map_err(|_| WatcherError::Store("invoice decryption failed".into()))?;
        serde_json::from_slice(&plaintext).map_err(|e| WatcherError::Store(e.to_string()))
    }

    pub fn insert(&self, invoice: &StoredInvoice, cleanup_at: i64) -> Result<(), WatcherError> {
        let (nonce, ciphertext) = self.encode(invoice)?;
        self.connection
            .lock()
            .map_err(|_| WatcherError::Store("store lock poisoned".into()))?
            .execute(
                "INSERT INTO invoices VALUES (?1,?2,?3,?4,'pending',?5,?6)",
                params![
                    self.hash(&invoice.invoice_id),
                    self.hash(&invoice.address),
                    nonce,
                    ciphertext,
                    invoice.expires_at,
                    cleanup_at
                ],
            )
            .map_err(store_error)?;
        Ok(())
    }

    pub fn get(&self, invoice_id: &str) -> Result<Option<StoredInvoice>, WatcherError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WatcherError::Store("store lock poisoned".into()))?;
        let row = connection
            .query_row(
                "SELECT nonce,ciphertext FROM invoices WHERE id_hash=?",
                [self.hash(invoice_id)],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(store_error)?;
        let id_hash = self.hash(invoice_id);
        row.map(|(nonce, ciphertext)| self.decode(&nonce, &ciphertext, &id_hash))
            .transpose()
    }

    pub fn pending(&self, now: i64) -> Result<Vec<StoredInvoice>, WatcherError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WatcherError::Store("store lock poisoned".into()))?;
        let mut statement = connection
            .prepare(
                "SELECT id_hash,nonce,ciphertext FROM invoices WHERE status='pending' AND cleanup_at>=?",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map([now], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(store_error)?;
        rows.map(|row| {
            let (id_hash, nonce, ciphertext) = row.map_err(store_error)?;
            self.decode(&nonce, &ciphertext, &id_hash)
        })
        .collect()
    }

    pub fn update(&self, invoice: &StoredInvoice) -> Result<(), WatcherError> {
        let (nonce, ciphertext) = self.encode(invoice)?;
        self.connection
            .lock()
            .map_err(|_| WatcherError::Store("store lock poisoned".into()))?
            .execute(
                "UPDATE invoices SET nonce=?,ciphertext=? WHERE id_hash=?",
                params![nonce, ciphertext, self.hash(&invoice.invoice_id)],
            )
            .map_err(store_error)?;
        Ok(())
    }

    pub fn settle(&self, invoice_id: &str, cleanup_at: i64) -> Result<(), WatcherError> {
        self.connection
            .lock()
            .map_err(|_| WatcherError::Store("store lock poisoned".into()))?
            .execute(
                "UPDATE invoices SET status='settled',cleanup_at=? WHERE id_hash=?",
                params![cleanup_at, self.hash(invoice_id)],
            )
            .map_err(store_error)?;
        Ok(())
    }

    pub fn cleanup(&self, now: i64) -> Result<usize, WatcherError> {
        self.connection
            .lock()
            .map_err(|_| WatcherError::Store("store lock poisoned".into()))?
            .execute("DELETE FROM invoices WHERE cleanup_at<?", [now])
            .map_err(store_error)
    }
}

fn store_error(error: rusqlite::Error) -> WatcherError {
    WatcherError::Store(error.to_string())
}

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub wallets: Arc<dyn WalletRpc>,
    pub store: Arc<InvoiceStore>,
    pub client: reqwest::Client,
}

impl AppState {
    pub async fn create_invoice(
        &self,
        request: CreateInvoiceRequest,
    ) -> Result<CreateInvoiceResponse, WatcherError> {
        validate_invoice_request(&request)?;
        if let Some(existing) = self.store.get(&request.invoice_id)? {
            if existing.payment_method == request.payment_method
                && existing.amount_atomic == request.amount_atomic
                && existing.expires_at == request.expires_at
            {
                return Ok(CreateInvoiceResponse {
                    invoice_id: existing.invoice_id,
                    address: existing.address,
                });
            }
            return Err(WatcherError::Request(
                "invoice id was reused with different terms".into(),
            ));
        }
        let (address, subaddress_index) = self.wallets.allocate(request.payment_method).await?;
        let invoice = StoredInvoice {
            invoice_id: request.invoice_id.clone(),
            payment_method: request.payment_method,
            amount_atomic: request.amount_atomic,
            address: address.clone(),
            subaddress_index,
            expires_at: request.expires_at,
            observed_at: None,
            locked_payment_refs: Vec::new(),
        };
        self.store.insert(
            &invoice,
            request.expires_at + self.config.invoice_retention_seconds,
        )?;
        Ok(CreateInvoiceResponse {
            invoice_id: request.invoice_id,
            address,
        })
    }

    pub async fn poll_once(&self, now: i64) -> Result<usize, WatcherError> {
        // Retention is a privacy boundary, not a best-effort side effect of a
        // healthy wallet. Delete expired records before any RPC/callback work
        // that may fail and short-circuit this polling pass.
        let _ = self.store.cleanup(now)?;
        let mut settled = 0;
        for mut invoice in self.store.pending(now)? {
            let required = match invoice.payment_method {
                Asset::Btc => self.config.btc_confirmations,
                Asset::Xmr => self.config.xmr_confirmations,
            };
            let observation = self.wallets.observe(&invoice, required).await?;
            let required_amount: u128 = invoice
                .amount_atomic
                .parse()
                .map_err(|_| WatcherError::Store("stored amount malformed".into()))?;
            if invoice.locked_payment_refs.is_empty() && now <= invoice.expires_at {
                if let Some(payment_refs) = select_payment_refs(&observation, required_amount)? {
                    invoice.observed_at = Some(now);
                    invoice.locked_payment_refs = payment_refs;
                    self.store.update(&invoice)?;
                }
            }
            let Some(confirmed) = confirmed_locked_observation(
                &observation,
                &invoice.locked_payment_refs,
                required,
                required_amount,
            )?
            else {
                continue;
            };
            let observed_at = invoice
                .observed_at
                .ok_or_else(|| WatcherError::Store("locked payment time missing".into()))?;
            self.callback(&invoice, &confirmed, observed_at, now)
                .await?;
            self.store.settle(
                &invoice.invoice_id,
                now + self.config.invoice_retention_seconds,
            )?;
            settled += 1;
        }
        Ok(settled)
    }

    async fn callback(
        &self,
        invoice: &StoredInvoice,
        observation: &Observation,
        observed_at: i64,
        now: i64,
    ) -> Result<(), WatcherError> {
        let (amount_atomic, confirmations) = observation_totals(observation)?;
        let payment_reference_commitment =
            payment_reference_commitment(&invoice.locked_payment_refs)?;
        let event_hash = hex::encode(Sha256::digest(
            format!(
                "{}:{}:{}",
                invoice.invoice_id,
                asset_name(invoice.payment_method),
                payment_reference_commitment,
            )
            .as_bytes(),
        ));
        let evidence = SettlementEvidence {
            event_id: format!("evt_{event_hash}"),
            invoice_id: invoice.invoice_id.clone(),
            payment_method: invoice.payment_method,
            amount_atomic: amount_atomic.to_string(),
            confirmations,
            observed_at,
            payment_reference_commitment,
        };
        let body = serde_json::to_string(&evidence)
            .map_err(|error| WatcherError::Callback(error.to_string()))?;
        let signature = sign_settlement(
            &self.config.settlement_signing_key,
            "POST",
            self.config.callback_url.path(),
            now,
            &evidence,
        );
        let response = self
            .client
            .post(self.config.callback_url.clone())
            .header("content-type", "application/json")
            .header("x-osl-timestamp", now.to_string())
            .header("x-osl-settlement-signature", signature)
            .body(body)
            .send()
            .await
            .map_err(|e| WatcherError::Callback(e.to_string()))?;
        if !response.status().is_success() {
            return Err(WatcherError::Callback(format!(
                "HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
struct SettlementEvidence {
    event_id: String,
    invoice_id: String,
    payment_method: Asset,
    amount_atomic: String,
    confirmations: u32,
    observed_at: i64,
    payment_reference_commitment: String,
}

fn asset_name(asset: Asset) -> &'static str {
    match asset {
        Asset::Btc => "btc",
        Asset::Xmr => "xmr",
    }
}

fn payment_reference_commitment(payment_refs: &[PaymentReference]) -> Result<String, WatcherError> {
    if payment_refs.is_empty() {
        return Err(WatcherError::Store("payment references are missing".into()));
    }
    let mut sorted = payment_refs.to_vec();
    sorted.sort_by(|left, right| left.txid.cmp(&right.txid));
    let mut canonical = String::new();
    for reference in sorted {
        validate_txid(&reference.txid)?;
        canonical.push_str(&reference.txid);
        canonical.push(':');
        canonical.push_str(&reference.amount_atomic.to_string());
        canonical.push('\n');
    }
    Ok(hex::encode(Sha256::digest(canonical.as_bytes())))
}

fn settlement_canonical(
    method: &str,
    path: &str,
    timestamp: i64,
    evidence: &SettlementEvidence,
) -> String {
    [
        "osl-crypto-settlement-v1".to_owned(),
        method.to_owned(),
        path.to_owned(),
        timestamp.to_string(),
        evidence.event_id.clone(),
        evidence.invoice_id.clone(),
        asset_name(evidence.payment_method).to_owned(),
        evidence.amount_atomic.clone(),
        evidence.confirmations.to_string(),
        evidence.observed_at.to_string(),
        evidence.payment_reference_commitment.clone(),
    ]
    .join("\n")
}

fn sign_settlement(
    key: &SigningKey,
    method: &str,
    path: &str,
    timestamp: i64,
    evidence: &SettlementEvidence,
) -> String {
    let signature = key.sign(settlement_canonical(method, path, timestamp, evidence).as_bytes());
    BASE64.encode(signature.to_bytes())
}

fn validate_invoice_request(request: &CreateInvoiceRequest) -> Result<(), WatcherError> {
    let now = unix_now();
    if !request
        .invoice_id
        .strip_prefix("cpay_")
        .is_some_and(|rest| {
            rest.len() == 32
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })
        || !request.amount_atomic.bytes().all(|b| b.is_ascii_digit())
        || request.amount_atomic.starts_with('0')
        || request.amount_atomic.len() > 31
        || request.expires_at <= now
        || request.expires_at > now.saturating_add(60 * 60)
    {
        return Err(WatcherError::Request("invoice fields malformed".into()));
    }
    Ok(())
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn request_canonical(method: &str, path: &str, timestamp: i64, body: &[u8]) -> String {
    let body_hash = hex::encode(Sha256::digest(body));
    format!("osl-watcher-request-v1\n{method}\n{path}\n{timestamp}\n{body_hash}")
}

#[cfg(test)]
fn sign_request(
    secret: &[u8],
    method: &str,
    path: &str,
    timestamp: i64,
    body: &[u8],
) -> Result<String, WatcherError> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret)
        .map_err(|_| WatcherError::Config("HMAC secret invalid".into()))?;
    mac.update(request_canonical(method, path, timestamp, body).as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn verify_request(
    headers: &HeaderMap,
    secret: &[u8],
    method: &str,
    path: &str,
    body: &[u8],
    now: i64,
) -> bool {
    let Some(timestamp) = headers
        .get("x-osl-timestamp")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
    else {
        return false;
    };
    let Some(signature) = headers
        .get("x-osl-request-signature")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| hex::decode(v).ok())
    else {
        return false;
    };
    if (now - timestamp).abs() > 300 {
        return false;
    }
    let Ok(mut mac) = <HmacSha256 as Mac>::new_from_slice(secret) else {
        return false;
    };
    mac.update(request_canonical(method, path, timestamp, body).as_bytes());
    mac.verify_slice(&signature).is_ok()
}

pub async fn create_invoice_handler(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !verify_request(
        &headers,
        &state.config.request_secret,
        method.as_str(),
        uri.path(),
        &body,
        unix_now(),
    ) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"unauthorized"})),
        );
    }
    let request: CreateInvoiceRequest = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"malformed request"})),
            )
        }
    };
    match state.create_invoice(request).await {
        Ok(invoice) => (StatusCode::CREATED, Json(json!(invoice))),
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":error.to_string()})),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router};
    use tempfile::tempdir;

    #[test]
    fn packaged_systemd_unit_uses_the_runtime_credential_names() {
        let unit = include_str!("../osl-crypto-watcher.service");
        assert!(unit
            .contains("CRYPTO_WATCHER_REQUEST_SECRET_FILE=/etc/osl-crypto/watcher-request-secret"));
        assert!(unit.contains(
            "CRYPTO_WATCHER_SETTLEMENT_SIGNING_KEY_FILE=/etc/osl-crypto/watcher-settlement-key.pem"
        ));
        assert!(unit.contains("CRYPTO_WATCHER_DB_KEY_FILE=/etc/osl-crypto/watcher-db-key"));
        assert!(!unit.contains("CRYPTO_WATCHER_SHARED_SECRET_FILE"));
    }

    struct MockWallet {
        allocations: Mutex<Vec<(String, Option<u32>)>>,
        observation: Mutex<Observation>,
    }

    struct FailingWallet;

    #[async_trait]
    impl WalletRpc for FailingWallet {
        async fn validate_watch_only(&self) -> Result<(), WatcherError> {
            Ok(())
        }

        async fn allocate(&self, _: Asset) -> Result<(String, Option<u32>), WatcherError> {
            Err(WatcherError::Rpc("allocation unavailable".into()))
        }

        async fn observe(&self, _: &StoredInvoice, _: u32) -> Result<Observation, WatcherError> {
            Err(WatcherError::Rpc("wallet unavailable".into()))
        }
    }

    fn payment(txid_byte: char, amount_atomic: u128, confirmations: u32) -> PaymentObservation {
        PaymentObservation {
            txid: txid_byte.to_string().repeat(64),
            amount_atomic,
            confirmations,
        }
    }

    async fn callback_url() -> Url {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/settle", post(|| async { StatusCode::NO_CONTENT })),
            )
            .await
            .unwrap();
        });
        Url::parse(&format!("http://{address}/settle")).unwrap()
    }
    #[async_trait]
    impl WalletRpc for MockWallet {
        async fn validate_watch_only(&self) -> Result<(), WatcherError> {
            Ok(())
        }
        async fn allocate(&self, _: Asset) -> Result<(String, Option<u32>), WatcherError> {
            Ok(self.allocations.lock().unwrap().remove(0))
        }
        async fn observe(&self, _: &StoredInvoice, _: u32) -> Result<Observation, WatcherError> {
            Ok(self.observation.lock().unwrap().clone())
        }
    }

    fn config() -> Arc<Config> {
        Arc::new(Config {
            bitcoin_rpc_url: Url::parse("http://127.0.0.1:8332/").unwrap(),
            bitcoin_cookie_file: "cookie".into(),
            bitcoin_wallet: "osl-watch".into(),
            monero_wallet_rpc_url: Url::parse("http://127.0.0.1:18088/").unwrap(),
            monero_account_index: 0,
            monero_primary_address: "4".repeat(95),
            callback_url: Url::parse("https://keyserver.example/v1/internal/crypto/settle")
                .unwrap(),
            request_secret: vec![7; 32],
            settlement_signing_key: SigningKey::from_bytes(&[8; 32]),
            btc_confirmations: 2,
            xmr_confirmations: 10,
            invoice_retention_seconds: 60,
        })
    }

    #[test]
    fn amounts_are_parsed_without_floating_point() {
        assert_eq!(decimal_to_atomic("0.00008333", 8).unwrap(), 8333);
        assert_eq!(decimal_to_atomic("1.25", 12).unwrap(), 1_250_000_000_000);
        assert!(decimal_to_atomic("0.000000001", 8).is_err());
    }
    #[test]
    fn invoice_expiry_is_bounded_to_one_hour() {
        let request = CreateInvoiceRequest {
            invoice_id: format!("cpay_{}", "a".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "1".into(),
            expires_at: unix_now() + 60 * 60 + 1,
        };
        assert!(validate_invoice_request(&request).is_err());
    }
    #[test]
    fn wallet_rpc_endpoints_must_be_loopback() {
        let mut c = (*config()).clone();
        c.bitcoin_rpc_url = Url::parse("http://node.example/").unwrap();
        assert!(c.validate().is_err());
        c.bitcoin_rpc_url = Url::parse("http://127.0.0.1:8332/").unwrap();
        c.invoice_retention_seconds = 0;
        assert!(c.validate().is_err());
        c.invoice_retention_seconds = -1;
        assert!(c.validate().is_err());
        c.invoice_retention_seconds = 1;
        assert!(c.validate().is_ok());
        c.invoice_retention_seconds = MAX_INVOICE_RETENTION_SECONDS;
        assert!(c.validate().is_ok());
        c.invoice_retention_seconds = MAX_INVOICE_RETENTION_SECONDS + 1;
        assert!(c.validate().is_err());
    }

    #[tokio::test]
    async fn cleanup_runs_before_a_wallet_poll_failure() {
        let dir = tempdir().unwrap();
        let store = Arc::new(InvoiceStore::open(&dir.path().join("db"), &[3; 32]).unwrap());
        let now = unix_now();
        let stale = StoredInvoice {
            invoice_id: format!("cpay_{}", "1".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "1".into(),
            address: "bc1stale".into(),
            subaddress_index: None,
            expires_at: now - 60,
            observed_at: None,
            locked_payment_refs: Vec::new(),
        };
        let active = StoredInvoice {
            invoice_id: format!("cpay_{}", "2".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "1".into(),
            address: "bc1active".into(),
            subaddress_index: None,
            expires_at: now + 60,
            observed_at: None,
            locked_payment_refs: Vec::new(),
        };
        store.insert(&stale, now - 1).unwrap();
        store.insert(&active, now + 120).unwrap();
        let state = AppState {
            config: config(),
            wallets: Arc::new(FailingWallet),
            store: store.clone(),
            client: reqwest::Client::new(),
        };

        assert!(state.poll_once(now).await.is_err());
        assert!(store.get(&stale.invoice_id).unwrap().is_none());
        assert!(store.get(&active.invoice_id).unwrap().is_some());
    }
    #[test]
    fn monero_primary_address_must_be_operator_pinned_and_well_formed() {
        let mut c = (*config()).clone();
        c.monero_primary_address.clear();
        assert!(c.validate().is_err());
        c.monero_primary_address = "0".repeat(95);
        assert!(c.validate().is_err());
        c.monero_primary_address = "4".repeat(95);
        assert!(c.validate().is_ok());
        c.monero_account_index = 1;
        assert!(c.validate().is_err());
        c.monero_account_index = 0;
        assert!(verify_monero_wallet_identity(
            &c.monero_primary_address,
            &c.monero_primary_address
        )
        .is_ok());
        assert!(verify_monero_wallet_identity(&c.monero_primary_address, &"5".repeat(95)).is_err());
    }
    #[test]
    fn credential_files_must_be_private_regular_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("credential");
        std::fs::write(&path, "  private-value\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(read_secret_file(&path).unwrap(), "private-value");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read_secret_file(&path).is_err());
        }
    }
    #[test]
    fn encrypted_store_hides_invoice_and_address_and_round_trips() {
        let dir = tempdir().unwrap();
        let store = InvoiceStore::open(&dir.path().join("db"), &[9; 32]).unwrap();
        let invoice = StoredInvoice {
            invoice_id: format!("cpay_{}", "a".repeat(32)),
            payment_method: Asset::Xmr,
            amount_atomic: "100".into(),
            address: "secret-subaddress".into(),
            subaddress_index: Some(4),
            expires_at: unix_now() + 60,
            observed_at: Some(unix_now()),
            locked_payment_refs: vec![PaymentReference {
                txid: "f".repeat(64),
                amount_atomic: 100,
            }],
        };
        store.insert(&invoice, unix_now() + 120).unwrap();
        let raw = std::fs::read(dir.path().join("db")).unwrap();
        assert!(!raw
            .windows(b"secret-subaddress".len())
            .any(|w| w == b"secret-subaddress"));
        assert!(!raw
            .windows(64)
            .any(|window| window == "f".repeat(64).as_bytes()));
        assert_eq!(
            store.get(&invoice.invoice_id).unwrap().unwrap().address,
            invoice.address
        );
    }
    #[tokio::test]
    async fn unique_allocations_and_idempotent_replay_use_mock_wallet_rpc() {
        let dir = tempdir().unwrap();
        let store = Arc::new(InvoiceStore::open(&dir.path().join("db"), &[3; 32]).unwrap());
        let wallet = Arc::new(MockWallet {
            allocations: Mutex::new(vec![("bc1qunique".into(), None)]),
            observation: Mutex::new(Observation { payments: vec![] }),
        });
        let state = AppState {
            config: config(),
            wallets: wallet,
            store,
            client: reqwest::Client::new(),
        };
        let request = CreateInvoiceRequest {
            invoice_id: format!("cpay_{}", "b".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "8333".into(),
            expires_at: unix_now() + 60,
        };
        let first = state.create_invoice(request.clone()).await.unwrap();
        let second = state.create_invoice(request).await.unwrap();
        assert_eq!(first.address, "bc1qunique");
        assert_eq!(first.address, second.address);
    }
    #[test]
    fn signed_requests_reject_tampering_and_stale_time() {
        let now = unix_now();
        let body = b"{}";
        let sig = sign_request(&[4; 32], "POST", "/v1/invoices", now, body).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-osl-timestamp", now.to_string().parse().unwrap());
        headers.insert("x-osl-request-signature", sig.parse().unwrap());
        assert!(verify_request(
            &headers,
            &[4; 32],
            "POST",
            "/v1/invoices",
            body,
            now
        ));
        assert!(!verify_request(
            &headers,
            &[4; 32],
            "POST",
            "/v1/invoices",
            b"{x}",
            now
        ));
        assert!(!verify_request(
            &headers,
            &[4; 32],
            "GET",
            "/v1/invoices",
            body,
            now
        ));
        assert!(!verify_request(
            &headers,
            &[4; 32],
            "POST",
            "/v1/invoices",
            body,
            now + 301
        ));
    }

    #[test]
    fn settlement_signature_binds_every_payment_field_and_reference_commitment() {
        use ed25519_dalek::{Signature, Verifier};

        let signing_key = SigningKey::from_bytes(
            &hex::decode("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
                .unwrap()
                .try_into()
                .unwrap(),
        );
        let evidence = SettlementEvidence {
            event_id: format!("evt_{}", "a".repeat(64)),
            invoice_id: format!("cpay_{}", "b".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "8333".into(),
            confirmations: 2,
            observed_at: 1_750_000_000,
            payment_reference_commitment: "c".repeat(64),
        };
        let encoded = sign_settlement(
            &signing_key,
            "POST",
            "/v1/internal/crypto/settle",
            1_750_000_100,
            &evidence,
        );
        assert_eq!(
            encoded,
            "Z/Xa1d1xDhadUdNpiQC6Um29kaUEgzeziin/qbqx0iQ8m8ZUmcHLcQ31b6simvSYbQ81J8wlGca1Ua8ino9cBw=="
        );
        let signature = Signature::from_slice(&BASE64.decode(encoded).unwrap()).unwrap();
        signing_key
            .verifying_key()
            .verify(
                settlement_canonical(
                    "POST",
                    "/v1/internal/crypto/settle",
                    1_750_000_100,
                    &evidence,
                )
                .as_bytes(),
                &signature,
            )
            .unwrap();
        let mut tampered = evidence.clone();
        tampered.confirmations += 1;
        assert!(signing_key
            .verifying_key()
            .verify(
                settlement_canonical(
                    "POST",
                    "/v1/internal/crypto/settle",
                    1_750_000_100,
                    &tampered,
                )
                .as_bytes(),
                &signature,
            )
            .is_err());
    }

    #[tokio::test]
    async fn underpayment_does_not_lock_in_quote() {
        let dir = tempdir().unwrap();
        let store = Arc::new(InvoiceStore::open(&dir.path().join("db"), &[3; 32]).unwrap());
        let wallet = Arc::new(MockWallet {
            allocations: Mutex::new(vec![("bc1qunique".into(), None)]),
            observation: Mutex::new(Observation {
                payments: vec![payment('1', 1, 2)],
            }),
        });
        let state = AppState {
            config: config(),
            wallets: wallet,
            store: store.clone(),
            client: reqwest::Client::new(),
        };
        let expires_at = unix_now() + 60;
        let request = CreateInvoiceRequest {
            invoice_id: format!("cpay_{}", "c".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "8333".into(),
            expires_at,
        };
        state.create_invoice(request.clone()).await.unwrap();
        assert_eq!(state.poll_once(expires_at - 1).await.unwrap(), 0);
        assert_eq!(
            store.get(&request.invoice_id).unwrap().unwrap().observed_at,
            None
        );
    }

    #[test]
    fn payment_ref_selection_preserves_partials_and_ignores_dust() {
        let observation = Observation {
            payments: vec![
                payment('1', 1, 0),
                payment('2', 8_000, 0),
                payment('3', 333, 0),
            ],
        };
        let selected = select_payment_refs(&observation, 8_333).unwrap().unwrap();
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|reference| reference.amount_atomic > 1));
        assert_eq!(
            selected
                .iter()
                .map(|reference| reference.amount_atomic)
                .sum::<u128>(),
            8_333
        );
    }

    #[tokio::test]
    async fn replacement_or_late_payment_ref_cannot_settle_expired_quote() {
        let dir = tempdir().unwrap();
        let store = Arc::new(InvoiceStore::open(&dir.path().join("db"), &[3; 32]).unwrap());
        let wallet = Arc::new(MockWallet {
            allocations: Mutex::new(vec![("bc1qunique".into(), None)]),
            observation: Mutex::new(Observation {
                payments: vec![payment('a', 8_333, 0)],
            }),
        });
        let mut test_config = (*config()).clone();
        test_config.callback_url = callback_url().await;
        let state = AppState {
            config: Arc::new(test_config),
            wallets: wallet.clone(),
            store: store.clone(),
            client: reqwest::Client::new(),
        };
        let expires_at = unix_now() + 60;
        let request = CreateInvoiceRequest {
            invoice_id: format!("cpay_{}", "d".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "8333".into(),
            expires_at,
        };
        state.create_invoice(request.clone()).await.unwrap();
        assert_eq!(state.poll_once(expires_at - 1).await.unwrap(), 0);
        let locked = store.get(&request.invoice_id).unwrap().unwrap();
        assert_eq!(locked.locked_payment_refs[0].txid, "a".repeat(64));

        *wallet.observation.lock().unwrap() = Observation {
            payments: vec![payment('b', 8_333, 2)],
        };
        assert_eq!(state.poll_once(expires_at + 1).await.unwrap(), 0);
        let still_locked = store.get(&request.invoice_id).unwrap().unwrap();
        assert_eq!(still_locked.locked_payment_refs[0].txid, "a".repeat(64));
    }

    #[tokio::test]
    async fn payment_first_seen_after_expiry_cannot_lock_quote() {
        let dir = tempdir().unwrap();
        let store = Arc::new(InvoiceStore::open(&dir.path().join("db"), &[3; 32]).unwrap());
        let wallet = Arc::new(MockWallet {
            allocations: Mutex::new(vec![("bc1qunique".into(), None)]),
            observation: Mutex::new(Observation { payments: vec![] }),
        });
        let state = AppState {
            config: config(),
            wallets: wallet.clone(),
            store: store.clone(),
            client: reqwest::Client::new(),
        };
        let expires_at = unix_now() + 60;
        let request = CreateInvoiceRequest {
            invoice_id: format!("cpay_{}", "9".repeat(32)),
            payment_method: Asset::Btc,
            amount_atomic: "8333".into(),
            expires_at,
        };
        state.create_invoice(request.clone()).await.unwrap();
        *wallet.observation.lock().unwrap() = Observation {
            payments: vec![payment('d', 8_333, 2)],
        };
        assert_eq!(state.poll_once(expires_at + 1).await.unwrap(), 0);
        let invoice = store.get(&request.invoice_id).unwrap().unwrap();
        assert!(invoice.locked_payment_refs.is_empty());
        assert_eq!(invoice.observed_at, None);
    }

    #[tokio::test]
    async fn same_payment_ref_may_confirm_after_quote_expiry() {
        let dir = tempdir().unwrap();
        let store = Arc::new(InvoiceStore::open(&dir.path().join("db"), &[3; 32]).unwrap());
        let wallet = Arc::new(MockWallet {
            allocations: Mutex::new(vec![("bc1qunique".into(), None)]),
            observation: Mutex::new(Observation {
                payments: vec![payment('c', 8_333, 0)],
            }),
        });
        let mut test_config = (*config()).clone();
        test_config.callback_url = callback_url().await;
        let state = AppState {
            config: Arc::new(test_config),
            wallets: wallet.clone(),
            store,
            client: reqwest::Client::new(),
        };
        let expires_at = unix_now() + 60;
        state
            .create_invoice(CreateInvoiceRequest {
                invoice_id: format!("cpay_{}", "e".repeat(32)),
                payment_method: Asset::Btc,
                amount_atomic: "8333".into(),
                expires_at,
            })
            .await
            .unwrap();
        assert_eq!(state.poll_once(expires_at - 1).await.unwrap(), 0);
        *wallet.observation.lock().unwrap() = Observation {
            payments: vec![payment('c', 8_333, 2)],
        };
        assert_eq!(state.poll_once(expires_at + 1).await.unwrap(), 1);
    }

    #[test]
    fn monero_observations_exclude_double_spends_and_other_subaddresses() {
        let transfer = |txid: char, amount, major, minor, double_spend_seen| MoneroTransfer {
            txid: txid.to_string().repeat(64),
            amount,
            confirmations: 12,
            double_spend_seen,
            locked: false,
            unlock_time: 0,
            subaddr_index: SubaddressIndex { major, minor },
        };
        let payments = monero_payment_observations(
            MoneroTransfers {
                r#in: vec![
                    transfer('1', 100, 0, 4, false),
                    transfer('2', 200, 0, 5, false),
                    transfer('3', 300, 0, 4, true),
                ],
                pool: vec![],
            },
            0,
            4,
        )
        .unwrap();
        assert_eq!(payments, vec![payment('1', 100, 12)]);
    }
}
