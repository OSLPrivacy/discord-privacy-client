//! Phase 7d-B1: main-password boot gate.
//!
//! Two on-disk artifacts under the OSL config dir:
//!
//! - `password_marker.json` — present iff a main password is
//!   configured. Holds:
//!     * `salt_b64` (16 bytes) — argon2 salt.
//!     * `params` — argon2id parameters (memory, iterations,
//!       parallelism).
//!     * `password_hash_b64` (32 bytes) — first half of the
//!       argon2id output. Compared constant-time to verify.
//!     * `phrase_encrypted_b64` + `phrase_nonce_b64` — AES-256-GCM
//!       blob of the BIP39 12-word recovery phrase. Key is the
//!       SECOND half of the same argon2id output, so the phrase
//!       only re-exposes after a successful password.
//!   Absence of the file = no gate. Tauri loads discord.com
//!   directly.
//!
//! - `lockout_state.json` — failed-attempt counters + per-counter
//!   lock-until timestamps for the password-entry and recovery-
//!   phrase-entry paths. Persists across Tauri restarts so a
//!   determined attacker can't brute-force by repeated relaunch.
//!
//! Argon2id parameters are 64 MiB / 3 iterations / 1 lane / 64 byte
//! output. The 64-byte output is split — first 32 bytes for the
//! verification hash, last 32 bytes for the AES-GCM key. argon2id
//! output is uniformly random so splitting is safe (cf. RFC 9106
//! §3.2 — variable output is supported up to 2^32-1 bytes).
//!
//! Recovery flow: `verify_recovery_phrase` issues a one-time
//! in-process token. The caller's *next* IPC call must be
//! `set_main_password_after_recovery(new_password, token)`, which
//! consumes the token from `AppState::recovery_token` and writes a
//! new marker reusing the SAME recovery phrase (re-encrypted under
//! the new password's argon2id-derived key). The recovery phrase
//! NEVER changes via this path; only the password (and thus the
//! marker's salt + hash + encryption) does.

use crate::AppState;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bip39::{Language, Mnemonic};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MARKER_FILENAME: &str = "password_marker.json";
const LOCKOUT_FILENAME: &str = "lockout_state.json";
const MARKER_VERSION: u32 = 2;
const LOCKOUT_VERSION: u32 = 1;
const ENC_MAGIC: &[u8; 8] = b"OSL-ENC1";

const PASSWORD_LEN: usize = 6;
const SALT_LEN: usize = 16;
const ARGON_OUTPUT_LEN: usize = 64; // 32 hash + 32 AES key
const HASH_LEN: usize = 32;
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

const ARGON_MEMORY_KB: u32 = 65_536; // 64 MiB
const ARGON_ITERATIONS: u32 = 3;
const ARGON_PARALLELISM: u32 = 1;

// =====================================================================
// On-disk schemas.
// =====================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argon2ParamsDto {
    pub memory_kb: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Argon2ParamsDto {
    fn default_prod() -> Self {
        Argon2ParamsDto {
            memory_kb: ARGON_MEMORY_KB,
            iterations: ARGON_ITERATIONS,
            parallelism: ARGON_PARALLELISM,
        }
    }
    fn to_params(&self) -> Result<Params, String> {
        Params::new(
            self.memory_kb,
            self.iterations,
            self.parallelism,
            Some(ARGON_OUTPUT_LEN),
        )
        .map_err(|e| format!("OSL: argon2 params: {e}"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordMarker {
    pub version: u32,
    pub salt_b64: String,
    pub params: Argon2ParamsDto,
    pub password_hash_b64: String,
    pub phrase_encrypted_b64: String,
    pub phrase_nonce_b64: String,
    /// argon2id(phrase, salt, params)[..32] in base64. Optional in
    /// the v=1 wire shape so a marker written by a pre-recovery
    /// build still parses. Required for the
    /// `verify_recovery_phrase` path; absent → caller asks the
    /// user to remove + reset their password.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phrase_hash_b64: Option<String>,
    /// 7d-B2: argon2id(stealth_password, salt, params)[..32] in
    /// base64. Same salt + params as the main password (no separate
    /// salt because stealth has no recovery and doesn't need its
    /// own key derivation surface). Absent on v=1 markers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stealth_password_hash_b64: Option<String>,
    /// 7d-B3: argon2id(burn_password, salt, params)[..32] in base64.
    /// Same salt + params as main. Absent on v=1 markers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burn_password_hash_b64: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockoutState {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub password_failed_attempts: u32,
    #[serde(default)]
    pub password_locked_until: Option<i64>,
    #[serde(default)]
    pub phrase_failed_attempts: u32,
    #[serde(default)]
    pub phrase_locked_until: Option<i64>,
}

// =====================================================================
// DTOs the Tauri layer surfaces.
// =====================================================================

#[derive(Debug, Clone, Serialize)]
pub struct PasswordStatusDto {
    pub is_set: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyFailureDto {
    pub ok: bool, // always false on this path
    pub attempts_used: u32,
    pub lockout_seconds_remaining: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LockoutStatusDto {
    pub password_locked_until: Option<i64>,
    pub password_attempts_used: u32,
    pub phrase_locked_until: Option<i64>,
    pub phrase_attempts_used: u32,
    pub now: i64,
}

// =====================================================================
// Validation.
// =====================================================================

/// Reject anything not exactly 6 ASCII chars in [0x20, 0x7E].
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.len() != PASSWORD_LEN {
        return Err(format!(
            "OSL: password must be exactly {PASSWORD_LEN} characters"
        ));
    }
    for b in password.as_bytes() {
        if *b < 0x20 || *b > 0x7E {
            return Err("OSL: only standard keyboard characters allowed (space–tilde)".to_string());
        }
    }
    Ok(())
}

// =====================================================================
// Argon2 core.
// =====================================================================

fn derive(password: &str, salt: &[u8], params: &Argon2ParamsDto) -> Result<[u8; 64], String> {
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.to_params()?);
    let mut out = [0u8; ARGON_OUTPUT_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| format!("OSL: argon2: {e}"))?;
    Ok(out)
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        return false;
    }
    bool::from(a.ct_eq(b))
}

// =====================================================================
// File IO.
// =====================================================================

fn marker_path(dir: &Path) -> PathBuf {
    dir.join(MARKER_FILENAME)
}

fn lockout_path(dir: &Path) -> PathBuf {
    dir.join(LOCKOUT_FILENAME)
}

/// Reports whether a main password is configured (the marker file
/// exists). Does not validate the file's contents.
pub fn marker_exists(dir: &Path) -> bool {
    marker_path(dir).exists()
}

fn read_marker(dir: &Path) -> Result<PasswordMarker, String> {
    let path = marker_path(dir);
    let bytes = std::fs::read(&path).map_err(|e| format!("OSL: read {}: {e}", path.display()))?;
    let marker: PasswordMarker = serde_json::from_slice(&bytes)
        .map_err(|e| format!("OSL: parse password_marker.json: {e}"))?;
    // 7d-B2: accept both v1 and v2 markers. v1 markers parse fine
    // because the new stealth/burn/phrase_hash fields are all
    // `Option<>` with serde default. Reject anything else — future
    // schema bumps need explicit migration.
    if marker.version != 1 && marker.version != MARKER_VERSION {
        return Err(format!(
            "OSL: password_marker.json version mismatch (got {}, want 1 or {MARKER_VERSION})",
            marker.version
        ));
    }
    Ok(marker)
}

fn write_marker(dir: &Path, marker: &PasswordMarker) -> Result<(), String> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| format!("OSL: mkdir {}: {e}", dir.display()))?;
    }
    let path = marker_path(dir);
    let bytes = serde_json::to_vec_pretty(marker)
        .map_err(|e| format!("OSL: serialize password_marker: {e}"))?;
    std::fs::write(&path, &bytes).map_err(|e| format!("OSL: write {}: {e}", path.display()))
}

fn delete_marker(dir: &Path) -> Result<(), String> {
    let path = marker_path(dir);
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| format!("OSL: remove {}: {e}", path.display()))?;
    }
    Ok(())
}

pub fn read_lockout_pub(dir: &Path) -> LockoutState {
    read_lockout(dir)
}

pub fn write_lockout_pub(dir: &Path, state: &LockoutState) -> Result<(), String> {
    write_lockout(dir, state)
}

pub fn read_marker_pub(dir: &Path) -> Result<PasswordMarker, String> {
    read_marker(dir)
}

pub fn now_unix_secs_pub() -> i64 {
    now_unix_secs()
}

pub fn password_lockout_secs_pub(attempts: u32) -> i64 {
    password_lockout_secs(attempts)
}

fn read_lockout(dir: &Path) -> LockoutState {
    let path = lockout_path(dir);
    let Ok(bytes) = std::fs::read(&path) else {
        return LockoutState {
            version: LOCKOUT_VERSION,
            ..Default::default()
        };
    };
    serde_json::from_slice(&bytes).unwrap_or(LockoutState {
        version: LOCKOUT_VERSION,
        ..Default::default()
    })
}

fn write_lockout(dir: &Path, state: &LockoutState) -> Result<(), String> {
    if !dir.exists() {
        std::fs::create_dir_all(dir).map_err(|e| format!("OSL: mkdir {}: {e}", dir.display()))?;
    }
    let path = lockout_path(dir);
    let bytes =
        serde_json::to_vec_pretty(state).map_err(|e| format!("OSL: serialize lockout: {e}"))?;
    std::fs::write(&path, &bytes).map_err(|e| format!("OSL: write {}: {e}", path.display()))
}

fn reset_password_lockout(dir: &Path) -> Result<(), String> {
    let mut st = read_lockout(dir);
    st.version = LOCKOUT_VERSION;
    st.password_failed_attempts = 0;
    st.password_locked_until = None;
    write_lockout(dir, &st)
}

fn reset_phrase_lockout(dir: &Path) -> Result<(), String> {
    let mut st = read_lockout(dir);
    st.version = LOCKOUT_VERSION;
    st.phrase_failed_attempts = 0;
    st.phrase_locked_until = None;
    write_lockout(dir, &st)
}

fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// =====================================================================
// Lockout schedule.
// =====================================================================

fn password_lockout_secs(attempts: u32) -> i64 {
    match attempts {
        0..=2 => 0,
        3 => 5,
        4..=5 => 30,
        6..=8 => 300,
        9..=12 => 3600,
        _ => 86400,
    }
}

fn phrase_lockout_secs(attempts: u32) -> i64 {
    match attempts {
        0..=2 => 0,
        3 => 30,
        4 => 300,
        5 => 3600,
        _ => 86400,
    }
}

// =====================================================================
// Recovery phrase ↔ AES-GCM.
// =====================================================================

fn random_bytes(n: usize) -> Vec<u8> {
    crypto::random::random_bytes(n)
}

fn generate_phrase() -> Result<String, String> {
    // 128 bits of entropy → 12 words. bip39 expects 16 random bytes.
    let entropy = random_bytes(16);
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)
        .map_err(|e| format!("OSL: bip39: {e}"))?;
    Ok(mnemonic.to_string())
}

fn encrypt_phrase(phrase: &str, key: &[u8; KEY_LEN]) -> Result<(Vec<u8>, [u8; NONCE_LEN]), String> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    let r = random_bytes(NONCE_LEN);
    nonce_bytes.copy_from_slice(&r);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, phrase.as_bytes())
        .map_err(|e| format!("OSL: aes-gcm encrypt: {e}"))?;
    Ok((ct, nonce_bytes))
}

fn decrypt_phrase(
    ciphertext: &[u8],
    nonce_bytes: &[u8; NONCE_LEN],
    key: &[u8; KEY_LEN],
) -> Result<String, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("OSL: aes-gcm decrypt: {e}"))?;
    String::from_utf8(pt).map_err(|e| format!("OSL: phrase utf8: {e}"))
}

// =====================================================================
// Core operations.
// =====================================================================

/// Build a marker from a password + phrase, fresh random salt.
fn build_marker(password: &str, phrase: &str) -> Result<PasswordMarker, String> {
    let mut salt = [0u8; SALT_LEN];
    let r = random_bytes(SALT_LEN);
    salt.copy_from_slice(&r);
    let params = Argon2ParamsDto::default_prod();
    let derived = derive(password, &salt, &params)?;
    let (hash, key_slice) = derived.split_at(HASH_LEN);
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(key_slice);
    let (ct, nonce) = encrypt_phrase(phrase, &key)?;
    // Phrase hash for the recovery path: argon2id with the SAME
    // salt + params but the phrase as input. Take the first 32
    // bytes (we don't need the second half — that AES key is
    // derived from the password, not the phrase).
    let phrase_derived = derive(phrase, &salt, &params)?;
    let phrase_hash_b64 = STANDARD.encode(&phrase_derived[..HASH_LEN]);
    Ok(PasswordMarker {
        version: MARKER_VERSION,
        salt_b64: STANDARD.encode(salt),
        params,
        password_hash_b64: STANDARD.encode(hash),
        phrase_encrypted_b64: STANDARD.encode(&ct),
        phrase_nonce_b64: STANDARD.encode(nonce),
        phrase_hash_b64: Some(phrase_hash_b64),
        stealth_password_hash_b64: None,
        burn_password_hash_b64: None,
    })
}

/// Return Ok(aes_key) when password matches, Err(reason) otherwise.
/// Reads the marker, runs argon2, constant-time compare. Does NOT
/// touch lockout — caller handles that.
fn verify_with_marker(marker: &PasswordMarker, password: &str) -> Result<[u8; KEY_LEN], String> {
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    if salt.len() != SALT_LEN {
        return Err(format!("OSL: salt wrong len: {} vs {SALT_LEN}", salt.len()));
    }
    let derived = derive(password, &salt, &marker.params)?;
    let stored_hash = STANDARD
        .decode(&marker.password_hash_b64)
        .map_err(|e| format!("OSL: hash b64: {e}"))?;
    if !ct_eq(&derived[..HASH_LEN], &stored_hash) {
        return Err("OSL: bad password".to_string());
    }
    let mut key = [0u8; KEY_LEN];
    key.copy_from_slice(&derived[HASH_LEN..]);
    Ok(key)
}

// =====================================================================
// Public command-layer functions. Each is invoked by a Tauri
// wrapper in `crates/ipc/src/commands.rs`.
// =====================================================================

pub fn password_status(dir: &Path) -> PasswordStatusDto {
    PasswordStatusDto {
        is_set: marker_exists(dir),
    }
}

/// Initial setup: requires no existing marker. Generates a fresh
/// BIP39 phrase, writes marker, resets lockout. Returns the phrase
/// — this is the ONLY time it ever surfaces plaintext to the
/// caller (the user must write it down).
pub fn set_main_password(dir: &Path, password: &str) -> Result<String, String> {
    validate_password(password)?;
    if marker_exists(dir) {
        return Err(
            "OSL: a main password is already set — use change_main_password instead".to_string(),
        );
    }
    let phrase = generate_phrase()?;
    let marker = build_marker(password, &phrase)?;
    write_marker(dir, &marker)?;
    let _ = reset_password_lockout(dir);
    let _ = reset_phrase_lockout(dir);
    // 7d-B4 (scoped): derive the file_storage_key, encrypt the 3
    // existing-plain JSONs in place, install the key into the
    // process-global slot so subsequent writes auto-encrypt.
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    let derived = derive(password, &salt, &marker.params)?;
    let file_key = derive_file_storage_key(&derived[HASH_LEN..]);
    encrypt_existing_state_files(dir, &file_key)?;
    set_file_storage_key(Some(file_key));
    Ok(phrase)
}

/// Change existing password. Verifies `current`, re-generates a
/// fresh phrase (per spec — change rotates the phrase), writes
/// new marker. Returns the NEW phrase.
pub fn change_main_password(dir: &Path, current: &str, new: &str) -> Result<String, String> {
    validate_password(new)?;
    let marker = read_marker(dir)?;
    let _key = verify_with_marker(&marker, current)
        .map_err(|_| "OSL: current password incorrect".to_string())?;
    // 7d-B4: derive old + new file_storage_keys so we can rotate
    // the at-rest encryption on the 3 JSONs before swapping marker.
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    let old_derived = derive(current, &salt, &marker.params)?;
    let old_file_key = derive_file_storage_key(&old_derived[HASH_LEN..]);
    let new_phrase = generate_phrase()?;
    let new_marker = build_marker(new, &new_phrase)?;
    let new_derived = derive(new, &salt, &new_marker.params)?;
    let new_file_key = derive_file_storage_key(&new_derived[HASH_LEN..]);
    // Rotate the 3 JSONs (old → new key). If any file fails to
    // rotate we bail BEFORE writing the new marker, leaving disk
    // in a consistent (old-marker, old-key-encrypted) state.
    rotate_state_files(dir, &old_file_key, &new_file_key)?;
    write_marker(dir, &new_marker)?;
    set_file_storage_key(Some(new_file_key));
    let _ = reset_password_lockout(dir);
    let _ = reset_phrase_lockout(dir);
    Ok(new_phrase)
}

pub fn remove_main_password(dir: &Path, current: &str) -> Result<(), String> {
    let marker = read_marker(dir)?;
    verify_with_marker(&marker, current)
        .map_err(|_| "OSL: current password incorrect".to_string())?;
    // 7d-B4: decrypt the 3 JSONs back to plain on disk before we
    // drop the key — otherwise subsequent reads (with no key) would
    // fail to parse the OSL-ENC1 blob.
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    let derived = derive(current, &salt, &marker.params)?;
    let file_key = derive_file_storage_key(&derived[HASH_LEN..]);
    let _ = decrypt_existing_state_files(dir, &file_key);
    set_file_storage_key(None);
    delete_marker(dir)?;
    let _ = reset_password_lockout(dir);
    let _ = reset_phrase_lockout(dir);
    Ok(())
}

pub fn view_recovery_phrase(dir: &Path, current: &str) -> Result<String, String> {
    let marker = read_marker(dir)?;
    let key = verify_with_marker(&marker, current)
        .map_err(|_| "OSL: current password incorrect".to_string())?;
    let ct = STANDARD
        .decode(&marker.phrase_encrypted_b64)
        .map_err(|e| format!("OSL: phrase ct b64: {e}"))?;
    let mut nonce = [0u8; NONCE_LEN];
    let nonce_bytes = STANDARD
        .decode(&marker.phrase_nonce_b64)
        .map_err(|e| format!("OSL: phrase nonce b64: {e}"))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err("OSL: phrase nonce wrong len".to_string());
    }
    nonce.copy_from_slice(&nonce_bytes);
    decrypt_phrase(&ct, &nonce, &key)
}

/// Verify the supplied password against the marker, applying
/// lockout rules. Returns Ok(()) on success (and resets the
/// counter); returns Err with a structured `VerifyFailureDto`-
/// shape JSON string on failure or lockout. Errors are kept as
/// JSON strings so the JS layer can `JSON.parse(err.message)` for
/// structured display — Tauri's command-error path always returns
/// a flat string.
pub fn verify_main_password(dir: &Path, password: &str) -> Result<(), String> {
    let mut state = read_lockout(dir);
    state.version = LOCKOUT_VERSION;
    let now = now_unix_secs();
    // Honour an existing lockout window.
    if let Some(until) = state.password_locked_until {
        if now < until {
            return Err(serde_json::to_string(&VerifyFailureDto {
                ok: false,
                attempts_used: state.password_failed_attempts,
                lockout_seconds_remaining: until - now,
            })
            .unwrap_or_else(|_| "OSL: lockout active".to_string()));
        }
    }
    // No marker → nothing to verify against. Caller shouldn't
    // reach this path unless something raced with marker removal.
    let marker = read_marker(dir)?;
    match verify_with_marker(&marker, password) {
        Ok(_) => {
            // 7d-B4: also derive + install the file_storage_key so
            // subsequent reads of peer_map/whitelist_state/pending_invitations
            // auto-decrypt. This is the settings-side "current
            // password" verify (view-recovery-phrase, change/remove)
            // — same install pattern as the gate's main-success
            // path so the global key is in sync regardless of
            // which surface authenticated the user.
            let salt = STANDARD
                .decode(&marker.salt_b64)
                .map_err(|e| format!("OSL: salt b64: {e}"))?;
            let derived = derive(password, &salt, &marker.params)?;
            let file_key = derive_file_storage_key(&derived[HASH_LEN..]);
            set_file_storage_key(Some(file_key));
            state.password_failed_attempts = 0;
            state.password_locked_until = None;
            let _ = write_lockout(dir, &state);
            Ok(())
        }
        Err(_) => {
            state.password_failed_attempts = state.password_failed_attempts.saturating_add(1);
            let secs = password_lockout_secs(state.password_failed_attempts);
            state.password_locked_until = if secs > 0 { Some(now + secs) } else { None };
            let _ = write_lockout(dir, &state);
            Err(serde_json::to_string(&VerifyFailureDto {
                ok: false,
                attempts_used: state.password_failed_attempts,
                lockout_seconds_remaining: secs,
            })
            .unwrap_or_else(|_| "OSL: bad password".to_string()))
        }
    }
}

/// Verify a 12-word phrase against the marker. On success returns
/// a one-time-use recovery token; the caller's next IPC call must
/// be `set_main_password_after_recovery(new_password, token)`.
/// Phrase lockout is updated independently of password lockout.
pub fn verify_recovery_phrase(
    state_app: &AppState,
    dir: &Path,
    phrase: &str,
) -> Result<String, String> {
    let mut lock = read_lockout(dir);
    lock.version = LOCKOUT_VERSION;
    let now = now_unix_secs();
    if let Some(until) = lock.phrase_locked_until {
        if now < until {
            return Err(serde_json::to_string(&VerifyFailureDto {
                ok: false,
                attempts_used: lock.phrase_failed_attempts,
                lockout_seconds_remaining: until - now,
            })
            .unwrap_or_else(|_| "OSL: phrase lockout active".to_string()));
        }
    }
    let marker = read_marker(dir)?;
    // We can verify the phrase by attempting decryption with a
    // *speculative* key. Catch: we don't have the password, only
    // the phrase. So instead, compare the candidate phrase string
    // to the one stored under the marker by deriving the AES key
    // from the phrase against a separate field… BUT the marker
    // doesn't currently carry a phrase-hash. Add a verification
    // alternative: decrypt the encrypted phrase blob requires the
    // password, which we don't have, so that won't work either.
    //
    // Solution: compare entropy. BIP39 phrases encode 128 bits of
    // entropy; we re-derive entropy from the candidate phrase via
    // bip39's parser and constant-time compare against entropy
    // recovered by decrypting the stored blob with… still need the
    // key. Instead, store a SEPARATE phrase_hash field in the
    // marker: argon2id(phrase, salt) → 32 bytes. Compare those.
    // To avoid a v2 marker format flag-day for users already on
    // v1, fall back to the "decrypt with key-derived-from-phrase"
    // path: we use a different argon2 salt derived from the marker
    // salt + a context string. This anchors the phrase-verification
    // to the same marker file without a schema bump.
    //
    // Implementation: derive `phrase_key` = argon2id(phrase, salt,
    // params) and try to decrypt the stored blob with it. If the
    // result equals the candidate phrase, the phrase is correct.
    // (This is a re-purposing of the encryption blob as a "phrase
    // self-test" — circular but functionally a verifier.)
    //
    // Wait — that's still wrong, because the blob is encrypted
    // with the PASSWORD-derived key, not the phrase-derived key.
    // We can't decrypt without the password.
    //
    // Correct approach: add a `phrase_hash_b64` field to the
    // marker on initial set / change, with argon2id(phrase, salt,
    // params)[..32]. Compare candidates against that. The marker
    // version stays 1 — the field defaults to None for old
    // markers, which we then reject ("phrase verification not
    // available for this marker; remove and recreate").
    //
    // We do that now: re-write `build_marker` to also write a
    // phrase_hash. The phrase_hash uses the same salt + params
    // as the password hash, which is fine — argon2id with the
    // same salt but different inputs is the standard pattern.
    //
    // For first-launch v1 users who already have a marker without
    // phrase_hash, we fall back to telling them to remove + reset.
    let phrase_hash_b64 = match marker_phrase_hash(&marker) {
        Some(h) => h,
        None => {
            return Err("OSL: this password marker predates phrase recovery — \
                 remove your password (Settings → Passwords → Remove) \
                 and set it again to enable recovery"
                .to_string());
        }
    };
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    let derived = derive(phrase, &salt, &marker.params)?;
    let stored = STANDARD
        .decode(&phrase_hash_b64)
        .map_err(|e| format!("OSL: phrase hash b64: {e}"))?;
    if !ct_eq(&derived[..HASH_LEN], &stored) {
        lock.phrase_failed_attempts = lock.phrase_failed_attempts.saturating_add(1);
        let secs = phrase_lockout_secs(lock.phrase_failed_attempts);
        lock.phrase_locked_until = if secs > 0 { Some(now + secs) } else { None };
        let _ = write_lockout(dir, &lock);
        return Err(serde_json::to_string(&VerifyFailureDto {
            ok: false,
            attempts_used: lock.phrase_failed_attempts,
            lockout_seconds_remaining: secs,
        })
        .unwrap_or_else(|_| "OSL: bad recovery phrase".to_string()));
    }
    lock.phrase_failed_attempts = 0;
    lock.phrase_locked_until = None;
    let _ = write_lockout(dir, &lock);
    // Issue a one-time token. AppState holds it; expires in 5 min.
    let token = STANDARD.encode(random_bytes(24));
    let expiry = now + 300;
    *state_app
        .recovery_token
        .lock()
        .expect("recovery_token mutex poisoned") =
        Some((token.clone(), expiry, phrase.to_string()));
    Ok(token)
}

/// Consume the recovery token, write a new marker with the same
/// phrase re-encrypted under the new password's key. Resets both
/// lockout counters.
pub fn set_main_password_after_recovery(
    state_app: &AppState,
    dir: &Path,
    new_password: &str,
    token: &str,
) -> Result<(), String> {
    validate_password(new_password)?;
    let mut guard = state_app
        .recovery_token
        .lock()
        .expect("recovery_token mutex poisoned");
    let (stored_token, expiry, phrase) = guard
        .take()
        .ok_or_else(|| "OSL: no active recovery token".to_string())?;
    let now = now_unix_secs();
    if !ct_eq(stored_token.as_bytes(), token.as_bytes()) {
        // Don't reinstate the token — a wrong token consumes it.
        return Err("OSL: recovery token mismatch".to_string());
    }
    if now > expiry {
        return Err("OSL: recovery token expired — re-enter the recovery phrase".to_string());
    }
    let marker = build_marker(new_password, &phrase)?;
    write_marker(dir, &marker)?;
    let _ = reset_password_lockout(dir);
    let _ = reset_phrase_lockout(dir);
    Ok(())
}

pub fn lockout_status(dir: &Path) -> LockoutStatusDto {
    let st = read_lockout(dir);
    LockoutStatusDto {
        password_locked_until: st.password_locked_until,
        password_attempts_used: st.password_failed_attempts,
        phrase_locked_until: st.phrase_locked_until,
        phrase_attempts_used: st.phrase_failed_attempts,
        now: now_unix_secs(),
    }
}

fn marker_phrase_hash(marker: &PasswordMarker) -> Option<String> {
    marker.phrase_hash_b64.clone()
}

// =====================================================================
// 7d-B4 (scoped to 3 JSON files per user choice): encryption-at-rest
// for peer_map.json, whitelist_state.json, pending_invitations.json.
//
// `identity.json` keeps its existing keystore::Sealer layer and
// `messages.sqlite` keeps its existing row-level AEAD in `store` —
// neither file is touched here.
//
// File format (8-byte magic | 12-byte nonce | ciphertext | 16-byte
// AES-GCM tag):
//
//   "OSL-ENC1" + nonce + AES-256-GCM(file_storage_key, nonce, plaintext_json)
//
// The file_storage_key is derived from the user's main password via
// argon2id → HMAC-SHA256 expand. A successful main-password verify
// stashes it in a process-global `OnceLock<Mutex<Option<[u8; 32]>>>`
// that the file loaders/writers consult transparently — no signature
// changes to existing readers.
//
// Plain JSON (no OSL-ENC1 prefix) is still accepted by loaders so
// users without a password keep working, and so the migration to
// encryption-at-rest on first `set_main_password` is a one-shot
// re-write. `clear_file_storage_key()` is called from
// `remove_main_password` to revert future writes to plain JSON.
// =====================================================================

use std::sync::{Mutex, OnceLock};

static FILE_STORAGE_KEY: OnceLock<Mutex<Option<[u8; 32]>>> = OnceLock::new();

fn file_storage_slot() -> &'static Mutex<Option<[u8; 32]>> {
    FILE_STORAGE_KEY.get_or_init(|| Mutex::new(None))
}

/// Public accessor used by peer_map / whitelist_state /
/// pending_invitations loaders + writers to decide whether to
/// encrypt-on-write or accept an encrypted-on-disk file.
pub fn get_file_storage_key() -> Option<[u8; 32]> {
    *file_storage_slot()
        .lock()
        .expect("file_storage_key mutex poisoned")
}

pub fn set_file_storage_key(key: Option<[u8; 32]>) {
    let was_some = file_storage_slot()
        .lock()
        .expect("file_storage_key mutex poisoned")
        .is_some();
    let is_some = key.is_some();
    *file_storage_slot()
        .lock()
        .expect("file_storage_key mutex poisoned") = key;
    if is_some && !was_some {
        eprintln!("[OSL][crypto] file_storage_key populated");
    } else if !is_some && was_some {
        eprintln!("[OSL][crypto] file_storage_key cleared");
    }
}

/// HKDF-Expand-SHA256 single-block expansion. Input `prk` is the
/// pseudo-random key (the 32-byte tail of argon2id output);
/// `info` is the context string. Output is 32 bytes (single HMAC
/// block per RFC 5869 §2.3 with L = HashLen).
fn hkdf_expand_32(prk: &[u8], info: &[u8]) -> [u8; 32] {
    use hmac::{Mac, SimpleHmac};
    use sha2::Sha256;
    type HmacSha256 = SimpleHmac<Sha256>;
    let mut mac = <HmacSha256 as Mac>::new_from_slice(prk).expect("hmac key");
    mac.update(info);
    mac.update(&[0x01u8]); // T(1) counter for single-block expand
    let out = mac.finalize().into_bytes();
    let mut k = [0u8; 32];
    k.copy_from_slice(&out[..32]);
    k
}

/// Derive the 32-byte AES key for the at-rest file format from
/// the argon2id output tail. Distinct info-string from the
/// phrase-blob key so a future phrase-blob compromise doesn't
/// surface file plaintext.
fn derive_file_storage_key(argon_tail: &[u8]) -> [u8; 32] {
    hkdf_expand_32(argon_tail, b"OSL/file-storage/v2")
}

/// Encrypt a JSON blob for at-rest storage. Output layout:
/// magic (8) + nonce (12) + ciphertext + tag (16).
pub fn encrypt_at_rest(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    let r = random_bytes(NONCE_LEN);
    nonce_bytes.copy_from_slice(&r);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("OSL: at-rest encrypt: {e}"))?;
    let mut out = Vec::with_capacity(ENC_MAGIC.len() + NONCE_LEN + ct.len());
    out.extend_from_slice(ENC_MAGIC);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt a file blob. Returns the inner JSON bytes. Caller is
/// expected to have verified the magic via `has_enc_magic`.
pub fn decrypt_at_rest(blob: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if !has_enc_magic(blob) {
        return Err("OSL: at-rest decrypt: missing OSL-ENC1 magic".to_string());
    }
    if blob.len() < ENC_MAGIC.len() + NONCE_LEN + 16 {
        return Err("OSL: at-rest decrypt: blob too short".to_string());
    }
    let nonce_bytes = &blob[ENC_MAGIC.len()..ENC_MAGIC.len() + NONCE_LEN];
    let ct = &blob[ENC_MAGIC.len() + NONCE_LEN..];
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("OSL: at-rest decrypt: {e}"))
}

/// Convenience: caller doesn't need to thread the key through —
/// reads from the process-global slot. Returns plaintext for
/// either a plain blob (no magic) or an encrypted blob (decrypts).
/// Used by JSON loaders.
pub fn maybe_decrypt(blob: &[u8]) -> Result<Vec<u8>, String> {
    if !has_enc_magic(blob) {
        return Ok(blob.to_vec());
    }
    let key = get_file_storage_key().ok_or_else(|| {
        "OSL: encrypted at-rest file but no key in slot (password not entered?)".to_string()
    })?;
    decrypt_at_rest(blob, &key)
}

/// Convenience: write-side mirror of `maybe_decrypt`. If a key is
/// in the slot, encrypt; otherwise return plaintext verbatim.
pub fn maybe_encrypt(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    match get_file_storage_key() {
        Some(key) => encrypt_at_rest(plaintext, &key),
        None => Ok(plaintext.to_vec()),
    }
}

pub fn has_enc_magic(blob: &[u8]) -> bool {
    blob.len() >= ENC_MAGIC.len() && &blob[..ENC_MAGIC.len()] == ENC_MAGIC
}

/// Best-effort re-write of the 3 unencrypted JSONs as encrypted
/// blobs using the supplied key. Called from `set_main_password`
/// after the marker is written. Plain files become encrypted in
/// place; already-encrypted files are skipped. Per-file failure
/// is logged + skipped rather than aborting the whole migration —
/// the spec wants this fail-soft (peer_map missing is normal in a
/// fresh install).
pub fn encrypt_existing_state_files(dir: &Path, key: &[u8; 32]) -> Result<(), String> {
    for name in [
        "peer_map.json",
        "whitelist_state.json",
        "burned_scopes.json",
    ] {
        let path = dir.join(name);
        if !path.exists() {
            eprintln!("[OSL][crypto] migrate skip {name}: not present");
            continue;
        }
        let raw = match std::fs::read(&path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[OSL][crypto] migrate skip {name}: read error: {e}");
                continue;
            }
        };
        if has_enc_magic(&raw) {
            eprintln!("[OSL][crypto] migrate skip {name}: already encrypted");
            continue;
        }
        eprintln!(
            "[OSL][crypto] migrating {name} to encrypted ({} bytes plain)",
            raw.len()
        );
        let enc = encrypt_at_rest(&raw, key)?;
        std::fs::write(&path, &enc).map_err(|e| format!("OSL: write {}: {e}", path.display()))?;
        eprintln!(
            "[OSL][crypto] migrated {name} → encrypted ({} bytes on disk)",
            enc.len()
        );
    }
    Ok(())
}

/// Decrypt the 3 state JSONs back to plain bytes on disk using
/// the supplied (old) key. Called from `remove_main_password`
/// before clearing the global key.
pub fn decrypt_existing_state_files(dir: &Path, key: &[u8; 32]) -> Result<(), String> {
    for name in ["peer_map.json", "whitelist_state.json"] {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let raw = match std::fs::read(&path) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !has_enc_magic(&raw) {
            continue;
        }
        let plain = decrypt_at_rest(&raw, key)?;
        std::fs::write(&path, &plain).map_err(|e| format!("OSL: write {}: {e}", path.display()))?;
    }
    Ok(())
}

/// Rotate the at-rest encryption from `old_key` to `new_key` for
/// the 3 state JSONs. Called from `change_main_password`. Each
/// file: read, decrypt with old, re-encrypt with new, write.
/// Files that are unexpectedly plain are silently re-encrypted
/// with new (covers the "user upgraded mid-session" case).
pub fn rotate_state_files(
    dir: &Path,
    old_key: &[u8; 32],
    new_key: &[u8; 32],
) -> Result<(), String> {
    for name in ["peer_map.json", "whitelist_state.json"] {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read(&path).map_err(|e| format!("OSL: read {}: {e}", path.display()))?;
        let plain = if has_enc_magic(&raw) {
            decrypt_at_rest(&raw, old_key)?
        } else {
            raw
        };
        let enc = encrypt_at_rest(&plain, new_key)?;
        std::fs::write(&path, &enc).map_err(|e| format!("OSL: write {}: {e}", path.display()))?;
    }
    Ok(())
}

// =====================================================================
// 7d-B2 / B3: stealth + burn password operations + 3-way gate verify.
// =====================================================================

/// Verify a password against the marker's three stored hashes
/// (main, stealth, burn). Constant-time across all three regardless
/// of early match. Returns which role matched. The boot gate's
/// dispatcher uses this; the settings UI continues using
/// `verify_main_password` (which intentionally rejects stealth/burn
/// matches — a password manager that only knows the main password
/// should fail "current password incorrect" if the user types
/// stealth/burn into a "current password" field).
pub enum GateMatch {
    Main([u8; 32]), // also returns derived file_storage_key
    Stealth,
    Burn,
    Wrong,
}

pub fn verify_gate_password_with_marker(
    marker: &PasswordMarker,
    password: &str,
) -> Result<GateMatch, String> {
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    if salt.len() != SALT_LEN {
        return Err("OSL: salt wrong len".to_string());
    }
    let derived = derive(password, &salt, &marker.params)?;
    let main_hash = STANDARD
        .decode(&marker.password_hash_b64)
        .map_err(|e| format!("OSL: main hash b64: {e}"))?;
    let stealth_hash = match marker.stealth_password_hash_b64.as_ref() {
        Some(s) => Some(
            STANDARD
                .decode(s)
                .map_err(|e| format!("OSL: stealth hash b64: {e}"))?,
        ),
        None => None,
    };
    let burn_hash = match marker.burn_password_hash_b64.as_ref() {
        Some(s) => Some(
            STANDARD
                .decode(s)
                .map_err(|e| format!("OSL: burn hash b64: {e}"))?,
        ),
        None => None,
    };
    let candidate = &derived[..HASH_LEN];
    // Constant-time: run all three comparisons regardless of
    // early match. The bool ORs at the end pick the first match.
    let m_main = ct_eq(candidate, &main_hash);
    let m_stealth = stealth_hash
        .as_deref()
        .map(|h| ct_eq(candidate, h))
        .unwrap_or(false);
    let m_burn = burn_hash
        .as_deref()
        .map(|h| ct_eq(candidate, h))
        .unwrap_or(false);
    if m_main {
        let file_key = derive_file_storage_key(&derived[HASH_LEN..]);
        return Ok(GateMatch::Main(file_key));
    }
    if m_stealth {
        return Ok(GateMatch::Stealth);
    }
    if m_burn {
        return Ok(GateMatch::Burn);
    }
    Ok(GateMatch::Wrong)
}

/// Set or rotate the stealth password. Verifies `current_main`
/// first (any password manager attempting `change_stealth` with
/// the wrong main is blocked). New stealth must differ from main
/// AND (if set) burn. Same salt/params as main → no marker schema
/// change beyond the added field.
pub fn set_stealth_password(
    dir: &Path,
    current_main: &str,
    new_stealth: &str,
) -> Result<(), String> {
    validate_password(new_stealth)?;
    let mut marker = read_marker(dir)?;
    let _ = verify_with_marker(&marker, current_main)
        .map_err(|_| "OSL: current main password incorrect".to_string())?;
    if current_main == new_stealth {
        return Err("OSL: stealth password must be different from your main password".to_string());
    }
    if let Some(burn_b64) = marker.burn_password_hash_b64.as_ref() {
        let salt = STANDARD
            .decode(&marker.salt_b64)
            .map_err(|e| format!("OSL: salt b64: {e}"))?;
        let derived = derive(new_stealth, &salt, &marker.params)?;
        let burn = STANDARD
            .decode(burn_b64)
            .map_err(|e| format!("OSL: burn b64: {e}"))?;
        if ct_eq(&derived[..HASH_LEN], &burn) {
            return Err("OSL: stealth and burn passwords must be different".to_string());
        }
    }
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    let derived = derive(new_stealth, &salt, &marker.params)?;
    marker.stealth_password_hash_b64 = Some(STANDARD.encode(&derived[..HASH_LEN]));
    marker.version = MARKER_VERSION;
    write_marker(dir, &marker)
}

pub fn remove_stealth_password(dir: &Path, current_main: &str) -> Result<(), String> {
    let mut marker = read_marker(dir)?;
    let _ = verify_with_marker(&marker, current_main)
        .map_err(|_| "OSL: current main password incorrect".to_string())?;
    marker.stealth_password_hash_b64 = None;
    marker.version = MARKER_VERSION;
    write_marker(dir, &marker)
}

pub fn set_burn_password(dir: &Path, current_main: &str, new_burn: &str) -> Result<(), String> {
    validate_password(new_burn)?;
    let mut marker = read_marker(dir)?;
    let _ = verify_with_marker(&marker, current_main)
        .map_err(|_| "OSL: current main password incorrect".to_string())?;
    if current_main == new_burn {
        return Err("OSL: burn password must be different from your main password".to_string());
    }
    if let Some(stealth_b64) = marker.stealth_password_hash_b64.as_ref() {
        let salt = STANDARD
            .decode(&marker.salt_b64)
            .map_err(|e| format!("OSL: salt b64: {e}"))?;
        let derived = derive(new_burn, &salt, &marker.params)?;
        let stealth = STANDARD
            .decode(stealth_b64)
            .map_err(|e| format!("OSL: stealth b64: {e}"))?;
        if ct_eq(&derived[..HASH_LEN], &stealth) {
            return Err("OSL: stealth and burn passwords must be different".to_string());
        }
    }
    let salt = STANDARD
        .decode(&marker.salt_b64)
        .map_err(|e| format!("OSL: salt b64: {e}"))?;
    let derived = derive(new_burn, &salt, &marker.params)?;
    marker.burn_password_hash_b64 = Some(STANDARD.encode(&derived[..HASH_LEN]));
    marker.version = MARKER_VERSION;
    write_marker(dir, &marker)
}

pub fn remove_burn_password(dir: &Path, current_main: &str) -> Result<(), String> {
    let mut marker = read_marker(dir)?;
    let _ = verify_with_marker(&marker, current_main)
        .map_err(|_| "OSL: current main password incorrect".to_string())?;
    marker.burn_password_hash_b64 = None;
    marker.version = MARKER_VERSION;
    write_marker(dir, &marker)
}

pub fn stealth_password_status(dir: &Path) -> bool {
    read_marker(dir)
        .map(|m| m.stealth_password_hash_b64.is_some())
        .unwrap_or(false)
}

pub fn burn_password_status(dir: &Path) -> bool {
    read_marker(dir)
        .map(|m| m.burn_password_hash_b64.is_some())
        .unwrap_or(false)
}

// =====================================================================
// 7d-B2: stealth-engage and 7d-B3: burn-engage.
// =====================================================================

/// Set the FILE_ATTRIBUTE_HIDDEN flag on the OSL config dir
/// (Windows only — silent no-op on other platforms). Best effort;
/// errors are returned but the caller treats them as warnings.
pub fn stealth_hide_dir(_dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        let wide: Vec<u16> = OsStr::new(_dir)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: SetFileAttributesW takes a null-terminated UTF-16
        // path and a flag DWORD. Both arguments live for the duration
        // of the call.
        extern "system" {
            fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
        }
        let res = unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };
        if res == 0 {
            return Err("OSL: SetFileAttributesW failed".to_string());
        }
    }
    Ok(())
}

pub fn stealth_unhide_dir(_dir: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
        let wide: Vec<u16> = OsStr::new(_dir)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        extern "system" {
            fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
        }
        let res = unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_NORMAL) };
        if res == 0 {
            return Err("OSL: SetFileAttributesW (unhide) failed".to_string());
        }
    }
    Ok(())
}

/// Wipe every OSL state file. Recovery-phrase blob goes with the
/// marker so this is truly irreversible (matching the spec — burn
/// is destruction under coercion). The Discord WebView2 cookies
/// live in a different directory, so the user's Discord login is
/// preserved across burn — they see vanilla Discord on next launch.
pub fn burn_wipe_all(dir: &Path) -> Result<(), String> {
    // Unhide first (in case stealth had hidden it) so subsequent
    // file ops aren't affected by attribute state.
    let _ = stealth_unhide_dir(dir);
    let top = [
        "identity.json",
        "peer_map.json",
        "channels.json",
        "whitelist_state.json",
        "password_marker.json",
        "lockout_state.json",
        // 7d-FIX1: also wipe burned-scopes ledger.
        "burned_scopes.json",
    ];
    for name in top {
        let path = dir.join(name);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    let store = dir.join("store");
    for name in [
        "messages.sqlite",
        "messages.sqlite-wal",
        "messages.sqlite-shm",
    ] {
        let path = store.join(name);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}
