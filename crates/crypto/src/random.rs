//! Secure random key and nonce generation.
//!
//! Backed by `rand::rngs::OsRng`, which uses the OS getrandom facility
//! (Windows: `BCryptGenRandom`; Linux: `getrandom(2)` or `/dev/urandom`).

use crate::aead::{Key, Nonce, KEY_SIZE, NONCE_SIZE};
use rand::rngs::OsRng;
use rand::RngCore;

pub fn random_aead_key() -> Key {
    let mut bytes = [0u8; KEY_SIZE];
    OsRng.fill_bytes(&mut bytes);
    Key::from_bytes(bytes)
}

pub fn random_nonce() -> Nonce {
    let mut bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut bytes);
    Nonce::from_bytes(bytes)
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

/// Random `u32` from the OS RNG. Used by Mode 1 chunking to stamp
/// each multi-message session with a unique id.
pub fn random_u32() -> u32 {
    let mut bytes = [0u8; 4];
    OsRng.fill_bytes(&mut bytes);
    u32::from_be_bytes(bytes)
}
