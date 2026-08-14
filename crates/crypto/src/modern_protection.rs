//! Unified modern protection for recovery, exports, backups and carrier objects.
//!
//! The deliberately small surface leaves no algorithm selector or raw-key
//! constructor. Every object receives a fresh OS-CSPRNG 256-bit data key and
//! XChaCha20-Poly1305-IETF nonce. Password protection uses Argon2id only to
//! encrypt that random data key; it never turns a low-entropy password into a
//! replacement for the random data key.

use crate::{aead, ed25519, hkdf, random, Error, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const FORMAT_VERSION: &str = "1";
pub const AEAD_ALGORITHM: &str = "XChaCha20-Poly1305-IETF";
pub const AEAD_LIBRARY: &str = "RustCrypto chacha20poly1305 0.10.1";
pub const SIGNATURE_ALGORITHM: &str = "Ed25519";
pub const SIGNATURE_LIBRARY: &str = "ed25519-dalek 2";
pub const KEY_BITS: usize = 256;
pub const SIGNATURE_SECURITY_BITS: usize = 128;
pub const NONCE_BITS: usize = 192;
pub const ARGON2_SALT_BITS: usize = 128;
pub const ARGON2_MEMORY_KIB: u32 = 65_536;
pub const ARGON2_ITERATIONS: u32 = 3;
pub const ARGON2_PARALLELISM: u32 = 1;
const KEY_DOMAIN_PREFIX: &[u8] = b"OSL/6140/object-key/v1/";
const PASSWORD_WRAP_PURPOSE: &[u8] = b"OSL/6140/password-data-key-wrap/v1";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtectionDomain {
    Recovery,
    EnclaveExport,
    PersonalExport,
    Backup,
    Carrier,
}

impl ProtectionDomain {
    pub const ALL: [Self; 5] = [
        Self::Recovery,
        Self::EnclaveExport,
        Self::PersonalExport,
        Self::Backup,
        Self::Carrier,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recovery => "recovery",
            Self::EnclaveExport => "enclave-export",
            Self::PersonalExport => "personal-export",
            Self::Backup => "backup",
            Self::Carrier => "carrier",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectContext {
    pub version: String,
    pub purpose: String,
    pub owner_account: String,
    pub object_id: String,
    pub generation: u64,
    pub chunk_index: u64,
    pub chunk_count: u64,
}

impl ObjectContext {
    pub fn v1(
        purpose: impl Into<String>,
        owner_account: impl Into<String>,
        object_id: impl Into<String>,
        generation: u64,
        chunk_index: u64,
        chunk_count: u64,
    ) -> Self {
        Self {
            version: FORMAT_VERSION.to_owned(),
            purpose: purpose.into(),
            owner_account: owner_account.into(),
            object_id: object_id.into(),
            generation,
            chunk_index,
            chunk_count,
        }
    }
}

#[derive(ZeroizeOnDrop)]
pub struct ObjectKey([u8; 32]);

impl ObjectKey {
    fn generate() -> Self {
        let generated = random::random_aead_key();
        Self(*generated.as_bytes())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasswordKdf {
    pub algorithm: String,
    pub salt: [u8; 16],
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub wrap_nonce: [u8; aead::NONCE_SIZE],
    pub wrapped_data_key: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedObject {
    pub algorithm: String,
    pub version: String,
    pub domain: ProtectionDomain,
    pub key_salt: [u8; 32],
    pub nonce: [u8; aead::NONCE_SIZE],
    pub ciphertext_and_tag: Vec<u8>,
    pub password_kdf: Option<PasswordKdf>,
}

impl ProtectedObject {
    /// Stable public bytes used by no-plaintext capture checks.
    pub fn public_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        append(&mut out, b"algorithm", self.algorithm.as_bytes());
        append(&mut out, b"version", self.version.as_bytes());
        append(&mut out, b"domain", self.domain.as_str().as_bytes());
        append(&mut out, b"key-salt", &self.key_salt);
        append(&mut out, b"nonce", &self.nonce);
        append(&mut out, b"ciphertext-and-tag", &self.ciphertext_and_tag);
        if let Some(kdf) = &self.password_kdf {
            append(&mut out, b"kdf", kdf.algorithm.as_bytes());
            append(&mut out, b"password-salt", &kdf.salt);
            append(&mut out, b"memory-kib", &kdf.memory_kib.to_be_bytes());
            append(&mut out, b"iterations", &kdf.iterations.to_be_bytes());
            append(&mut out, b"parallelism", &kdf.parallelism.to_be_bytes());
            append(&mut out, b"wrap-nonce", &kdf.wrap_nonce);
            append(&mut out, b"wrapped-data-key", &kdf.wrapped_data_key);
        }
        out
    }
}

fn append(out: &mut Vec<u8>, label: &[u8], value: &[u8]) {
    out.extend_from_slice(&(label.len() as u32).to_be_bytes());
    out.extend_from_slice(label);
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

fn validate_context(context: &ObjectContext) -> Result<()> {
    if context.version != FORMAT_VERSION
        || context.purpose.is_empty()
        || context.owner_account.is_empty()
        || context.object_id.is_empty()
        || context.generation == 0
        || context.chunk_count == 0
        || context.chunk_index >= context.chunk_count
    {
        return Err(Error::AeadFailure);
    }
    Ok(())
}

fn associated_data(domain: ProtectionDomain, context: &ObjectContext) -> Result<Vec<u8>> {
    validate_context(context)?;
    let mut aad = Vec::new();
    append(&mut aad, b"algorithm", AEAD_ALGORITHM.as_bytes());
    append(&mut aad, b"domain", domain.as_str().as_bytes());
    append(&mut aad, b"version", context.version.as_bytes());
    append(&mut aad, b"purpose", context.purpose.as_bytes());
    append(&mut aad, b"owner-account", context.owner_account.as_bytes());
    append(&mut aad, b"object-id", context.object_id.as_bytes());
    append(&mut aad, b"generation", &context.generation.to_be_bytes());
    append(&mut aad, b"chunk-index", &context.chunk_index.to_be_bytes());
    append(&mut aad, b"chunk-count", &context.chunk_count.to_be_bytes());
    Ok(aad)
}

fn domain_key(domain: ProtectionDomain, key_salt: &[u8; 32], key: &ObjectKey) -> Result<aead::Key> {
    let mut info = KEY_DOMAIN_PREFIX.to_vec();
    info.extend_from_slice(domain.as_str().as_bytes());
    Ok(aead::Key::from_bytes(hkdf::derive_32(
        key_salt, &key.0, &info,
    )?))
}

fn seal_with_key(
    domain: ProtectionDomain,
    context: &ObjectContext,
    plaintext: &[u8],
    key: &ObjectKey,
) -> Result<ProtectedObject> {
    if plaintext.is_empty() {
        return Err(Error::AeadFailure);
    }
    let mut key_salt = [0u8; 32];
    key_salt.copy_from_slice(&random::random_bytes(32));
    let nonce = random::random_nonce();
    let cipher_key = domain_key(domain, &key_salt, key)?;
    let aad = associated_data(domain, context)?;
    let ciphertext_and_tag = aead::seal(&cipher_key, &nonce, &aad, plaintext)?;
    Ok(ProtectedObject {
        algorithm: AEAD_ALGORITHM.to_owned(),
        version: FORMAT_VERSION.to_owned(),
        domain,
        key_salt,
        nonce: *nonce.as_bytes(),
        ciphertext_and_tag,
        password_kdf: None,
    })
}

fn validate_envelope(domain: ProtectionDomain, object: &ProtectedObject) -> Result<()> {
    if object.algorithm != AEAD_ALGORITHM
        || object.version != FORMAT_VERSION
        || object.domain != domain
    {
        return Err(Error::AeadFailure);
    }
    Ok(())
}

/// Protect with an independently generated OS-CSPRNG data key.
pub fn seal_random(
    domain: ProtectionDomain,
    context: &ObjectContext,
    plaintext: &[u8],
) -> Result<(ProtectedObject, ObjectKey)> {
    let key = ObjectKey::generate();
    let object = seal_with_key(domain, context, plaintext, &key)?;
    Ok((object, key))
}

pub fn open_random(
    domain: ProtectionDomain,
    context: &ObjectContext,
    object: &ProtectedObject,
    key: &ObjectKey,
) -> Result<Vec<u8>> {
    validate_envelope(domain, object)?;
    if object.password_kdf.is_some() {
        return Err(Error::AeadFailure);
    }
    let cipher_key = domain_key(domain, &object.key_salt, key)?;
    let aad = associated_data(domain, context)?;
    aead::open(
        &cipher_key,
        &aead::Nonce::from_bytes(object.nonce),
        &aad,
        &object.ciphertext_and_tag,
    )
}

fn derive_password_key(password: &[u8], kdf: &PasswordKdf) -> Result<aead::Key> {
    if password.is_empty()
        || kdf.algorithm != "Argon2id"
        || kdf.memory_kib < ARGON2_MEMORY_KIB
        || kdf.iterations < ARGON2_ITERATIONS
        || kdf.parallelism < ARGON2_PARALLELISM
    {
        return Err(Error::AeadFailure);
    }
    let params = Params::new(kdf.memory_kib, kdf.iterations, kdf.parallelism, Some(32))
        .map_err(|_| Error::AeadFailure)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut output = [0u8; 32];
    argon
        .hash_password_into(password, &kdf.salt, &mut output)
        .map_err(|_| Error::AeadFailure)?;
    let key = aead::Key::from_bytes(output);
    output.zeroize();
    Ok(key)
}

fn password_wrap_aad(domain: ProtectionDomain, context: &ObjectContext) -> Result<Vec<u8>> {
    let mut aad = associated_data(domain, context)?;
    append(&mut aad, b"key-purpose", PASSWORD_WRAP_PURPOSE);
    Ok(aad)
}

/// Protect an object with a fresh random data key wrapped by an Argon2id key.
pub fn seal_password(
    domain: ProtectionDomain,
    context: &ObjectContext,
    plaintext: &[u8],
    password: &[u8],
) -> Result<ProtectedObject> {
    if password.is_empty() {
        return Err(Error::AeadFailure);
    }
    let data_key = ObjectKey::generate();
    let mut object = seal_with_key(domain, context, plaintext, &data_key)?;
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&random::random_bytes(16));
    let wrap_nonce = random::random_nonce();
    let mut kdf = PasswordKdf {
        algorithm: "Argon2id".to_owned(),
        salt,
        memory_kib: ARGON2_MEMORY_KIB,
        iterations: ARGON2_ITERATIONS,
        parallelism: ARGON2_PARALLELISM,
        wrap_nonce: *wrap_nonce.as_bytes(),
        wrapped_data_key: Vec::new(),
    };
    let wrapping_key = derive_password_key(password, &kdf)?;
    kdf.wrapped_data_key = aead::seal(
        &wrapping_key,
        &wrap_nonce,
        &password_wrap_aad(domain, context)?,
        &data_key.0,
    )?;
    object.password_kdf = Some(kdf);
    Ok(object)
}

pub fn open_password(
    domain: ProtectionDomain,
    context: &ObjectContext,
    object: &ProtectedObject,
    password: &[u8],
) -> Result<Vec<u8>> {
    validate_envelope(domain, object)?;
    let kdf = object.password_kdf.as_ref().ok_or(Error::AeadFailure)?;
    let wrapping_key = derive_password_key(password, kdf)?;
    let bytes = aead::open(
        &wrapping_key,
        &aead::Nonce::from_bytes(kdf.wrap_nonce),
        &password_wrap_aad(domain, context)?,
        &kdf.wrapped_data_key,
    )?;
    let mut raw: [u8; 32] = bytes.try_into().map_err(|_| Error::AeadFailure)?;
    let data_key = ObjectKey(raw);
    raw.zeroize();
    let cipher_key = domain_key(domain, &object.key_salt, &data_key)?;
    let aad = associated_data(domain, context)?;
    aead::open(
        &cipher_key,
        &aead::Nonce::from_bytes(object.nonce),
        &aad,
        &object.ciphertext_and_tag,
    )
}

#[derive(ZeroizeOnDrop)]
pub struct RecoverySigner(ed25519::SecretKey);

impl RecoverySigner {
    /// Generates an independent 256-bit Ed25519 seed with the OS CSPRNG.
    pub fn generate() -> Self {
        let (secret, _) = ed25519::generate_keypair();
        Self(secret)
    }

    pub fn public_key(&self) -> ed25519::PublicKey {
        ed25519::derive_public(&self.0)
    }

    pub fn sign(
        &self,
        context: &ObjectContext,
        declaration: &[u8],
    ) -> Result<SignedRecoveryDeclaration> {
        if declaration.is_empty() {
            return Err(Error::AeadFailure);
        }
        let mut signed_bytes = associated_data(ProtectionDomain::Recovery, context)?;
        append(&mut signed_bytes, b"declaration", declaration);
        Ok(SignedRecoveryDeclaration {
            algorithm: SIGNATURE_ALGORITHM.to_owned(),
            context: context.clone(),
            declaration: declaration.to_vec(),
            public_key: self.public_key(),
            signature: ed25519::sign(&self.0, &signed_bytes),
        })
    }
}

#[derive(Clone, Debug)]
pub struct SignedRecoveryDeclaration {
    pub algorithm: String,
    pub context: ObjectContext,
    pub declaration: Vec<u8>,
    pub public_key: ed25519::PublicKey,
    pub signature: ed25519::Signature,
}

pub fn verify_recovery(declaration: &SignedRecoveryDeclaration) -> Result<bool> {
    if declaration.algorithm != SIGNATURE_ALGORITHM || declaration.declaration.is_empty() {
        return Ok(false);
    }
    let mut signed_bytes = associated_data(ProtectionDomain::Recovery, &declaration.context)?;
    append(&mut signed_bytes, b"declaration", &declaration.declaration);
    ed25519::verify(
        &declaration.public_key,
        &signed_bytes,
        &declaration.signature,
    )
}
