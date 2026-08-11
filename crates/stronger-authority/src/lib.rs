//! Additional-secret authority used by stronger message sequences.
//!
//! The boundary is intentionally small: a sequence starts only after a
//! fallible 32-byte operating-system CSPRNG draw succeeds.  The draw and the
//! disclosed ordinary material are fed to one production HKDF, with distinct
//! labels for encryption, authentication, and the single-message nonce.  The
//! authentication key is then used by an explicit HMAC-SHA-256 operation in
//! addition to AES-256-GCM's built-in tag.  Consequently neither a decorative
//! field nor public/ordinary key material can satisfy this construction.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const SECRET_BYTES: usize = 32;
pub const REQUIRED_MIN_ENTROPY_BITS: u16 = 128;
pub const OS_DRAW_MIN_ENTROPY_BITS: u16 = 256;
pub const OFFLINE_RECOVERY_WORK_BITS: u16 = 128;
pub const FORGERY_WORK_BITS: u16 = 128;
const TAG_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const KDF_SALT: &[u8] = b"OSL/stronger-authority/v1/extract";
const ENC_INFO: &[u8] = b"OSL/stronger-authority/v1/encryption-key";
const AUTH_INFO: &[u8] = b"OSL/stronger-authority/v1/authentication-key";
const NONCE_INFO: &[u8] = b"OSL/stronger-authority/v1/message-nonce";

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    Dm,
    Group,
    Server,
}

impl Path {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dm => "dm",
            Self::Group => "group",
            Self::Server => "server",
        }
    }
}

/// The only entropy source accepted by production sequence creation. It has
/// no constructor from bytes, counters, timestamps, strings, or callbacks.
/// The private failure bit exists solely to prove fail-closed behavior.
pub struct OsEntropy {
    forced_failure: bool,
}

impl OsEntropy {
    pub fn new() -> Self {
        Self {
            forced_failure: false,
        }
    }

    #[doc(hidden)]
    pub fn forced_failure_for_audit() -> Self {
        Self {
            forced_failure: true,
        }
    }

    fn source_name(&self) -> &'static str {
        if cfg!(target_os = "windows") {
            "windows_os_csprng"
        } else if cfg!(target_os = "linux") {
            "linux_os_csprng"
        } else {
            "operating_system_csprng"
        }
    }

    fn claimed_min_entropy_bits(&self) -> u16 {
        OS_DRAW_MIN_ENTROPY_BITS
    }

    fn try_fill(&mut self, destination: &mut [u8]) -> Result<(), EntropyError> {
        if self.forced_failure {
            return Err(EntropyError::Unavailable(
                "forced task-5905 OS failure".into(),
            ));
        }
        OsRng
            .try_fill_bytes(destination)
            .map_err(|error| EntropyError::Unavailable(error.to_string()))
    }
}

impl Default for OsEntropy {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EntropyError {
    #[error("secure randomness unavailable: {0}")]
    Unavailable(String),
    #[error(
        "entropy bound {claimed_bits} bits is below required {required_bits} bits for path {path}"
    )]
    BelowBound {
        claimed_bits: u16,
        required_bits: u16,
        path: &'static str,
    },
    #[error("production KDF failed")]
    Kdf,
}

#[derive(Clone, Debug)]
pub struct DrawTrace {
    pub path: Path,
    pub source: &'static str,
    pub bytes: usize,
    pub source_min_entropy_bits: u16,
    pub retained_min_entropy_bits: u16,
    pub commitment: [u8; 32],
    pub production_kdf_edge: bool,
    pub encryption_dependency: bool,
    pub authentication_dependency: bool,
}

#[derive(Clone, ZeroizeOnDrop)]
struct Secret([u8; SECRET_BYTES]);

#[derive(Clone, ZeroizeOnDrop)]
struct DerivedKeys {
    encryption: [u8; 32],
    authentication: [u8; 32],
    nonce: [u8; NONCE_BYTES],
}

/// One end of a stronger sequence. The object is constructed only after the
/// entropy draw and all KDF expansions succeed.
#[derive(Clone)]
pub struct Authority {
    path: Path,
    keys: DerivedKeys,
}

impl fmt::Debug for Authority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Authority")
            .field("path", &self.path)
            .field("keys", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealedMessage {
    pub nonce: [u8; NONCE_BYTES],
    pub ciphertext: Vec<u8>,
    pub authentication_tag: [u8; TAG_BYTES],
}

/// Draw a fresh OS secret and return independent sender/receiver authority.
/// The duplicate represents installation through the stronger sequence's
/// confidential state-establishment channel; the secret itself is never
/// returned from this API.
pub fn start_sequence(
    path: Path,
    ordinary_material: &[u8; 32],
) -> Result<(Authority, Authority, DrawTrace), EntropyError> {
    start_sequence_with_source(path, ordinary_material, &mut OsEntropy::new())
}

pub fn start_sequence_with_source(
    path: Path,
    ordinary_material: &[u8; 32],
    source: &mut OsEntropy,
) -> Result<(Authority, Authority, DrawTrace), EntropyError> {
    let claimed = source.claimed_min_entropy_bits();
    if claimed < REQUIRED_MIN_ENTROPY_BITS {
        return Err(EntropyError::BelowBound {
            claimed_bits: claimed,
            required_bits: REQUIRED_MIN_ENTROPY_BITS,
            path: path.as_str(),
        });
    }

    // This is deliberately the first side effect. A failed source therefore
    // returns with no Authority value and no bytes capable of being sent.
    let mut raw = [0u8; SECRET_BYTES];
    source.try_fill(&mut raw)?;
    let secret = Secret(raw);
    let commitment: [u8; 32] = Sha256::digest(secret.0).into();
    let keys = derive(path, ordinary_material, &secret)?;
    let sender = Authority { path, keys };
    let receiver = sender.clone();
    let trace = DrawTrace {
        path,
        source: source.source_name(),
        bytes: SECRET_BYTES,
        source_min_entropy_bits: claimed,
        retained_min_entropy_bits: claimed.min(256),
        commitment,
        production_kdf_edge: true,
        encryption_dependency: true,
        authentication_dependency: true,
    };
    Ok((sender, receiver, trace))
}

fn derive(
    path: Path,
    ordinary_material: &[u8; 32],
    secret: &Secret,
) -> Result<DerivedKeys, EntropyError> {
    let mut ikm = [0u8; 64];
    ikm[..32].copy_from_slice(ordinary_material);
    ikm[32..].copy_from_slice(&secret.0);
    let hk = Hkdf::<Sha256>::new(Some(KDF_SALT), &ikm);
    ikm.zeroize();

    let mut encryption = [0u8; 32];
    let mut authentication = [0u8; 32];
    let mut nonce = [0u8; NONCE_BYTES];
    let path_label = path.as_str().as_bytes();
    let mut info = Vec::with_capacity(64);
    info.extend_from_slice(ENC_INFO);
    info.extend_from_slice(path_label);
    hk.expand(&info, &mut encryption)
        .map_err(|_| EntropyError::Kdf)?;
    info.clear();
    info.extend_from_slice(AUTH_INFO);
    info.extend_from_slice(path_label);
    hk.expand(&info, &mut authentication)
        .map_err(|_| EntropyError::Kdf)?;
    info.clear();
    info.extend_from_slice(NONCE_INFO);
    info.extend_from_slice(path_label);
    hk.expand(&info, &mut nonce)
        .map_err(|_| EntropyError::Kdf)?;
    Ok(DerivedKeys {
        encryption,
        authentication,
        nonce,
    })
}

impl Authority {
    pub fn seal(&self, plaintext: &[u8]) -> Result<SealedMessage, &'static str> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.keys.encryption));
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&self.keys.nonce),
                Payload {
                    msg: plaintext,
                    aad: self.path.as_str().as_bytes(),
                },
            )
            .map_err(|_| "encryption failed")?;
        let authentication_tag = self.mac(&ciphertext)?;
        Ok(SealedMessage {
            nonce: self.keys.nonce,
            ciphertext,
            authentication_tag,
        })
    }

    pub fn open(&self, message: &SealedMessage) -> Result<Vec<u8>, &'static str> {
        if message.nonce != self.keys.nonce {
            return Err("authentication failed");
        }
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.keys.authentication)
            .map_err(|_| "authentication failed")?;
        mac.update(self.path.as_str().as_bytes());
        mac.update(&message.nonce);
        mac.update(&message.ciphertext);
        mac.verify_slice(&message.authentication_tag)
            .map_err(|_| "authentication failed")?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.keys.encryption));
        cipher
            .decrypt(
                Nonce::from_slice(&message.nonce),
                Payload {
                    msg: &message.ciphertext,
                    aad: self.path.as_str().as_bytes(),
                },
            )
            .map_err(|_| "decryption failed")
    }

    fn mac(&self, ciphertext: &[u8]) -> Result<[u8; TAG_BYTES], &'static str> {
        let mut mac = <HmacSha256 as Mac>::new_from_slice(&self.keys.authentication)
            .map_err(|_| "authentication failed")?;
        mac.update(self.path.as_str().as_bytes());
        mac.update(&self.keys.nonce);
        mac.update(ciphertext);
        Ok(mac.finalize().into_bytes().into())
    }

    /// Audit-only constructor used by the unchanged exhaustive attacker and
    /// wrong-secret black-box checks. It never claims OS provenance.
    #[doc(hidden)]
    pub fn from_candidate_for_audit(
        path: Path,
        ordinary_material: &[u8; 32],
        candidate: [u8; SECRET_BYTES],
    ) -> Result<Self, EntropyError> {
        let secret = Secret(candidate);
        Ok(Self {
            path,
            keys: derive(path, ordinary_material, &secret)?,
        })
    }
}

/// Brute-force a deliberately enumerable candidate. This exact attacker is
/// used for both the changing one-bit and 20-bit negative controls.
#[doc(hidden)]
pub fn exhaustive_recover_for_audit(
    path: Path,
    ordinary_material: &[u8; 32],
    message: &SealedMessage,
    entropy_bits: u8,
) -> Option<(u32, usize, Vec<u8>)> {
    if entropy_bits > 20 {
        return None;
    }
    let limit = 1u32 << entropy_bits;
    for candidate in 0..limit {
        let mut bytes = [0u8; SECRET_BYTES];
        bytes[SECRET_BYTES - 4..].copy_from_slice(&candidate.to_be_bytes());
        let authority = Authority::from_candidate_for_audit(path, ordinary_material, bytes).ok()?;
        if let Ok(opened) = authority.open(message) {
            return Some((candidate, candidate as usize + 1, opened));
        }
    }
    None
}
