//! Authenticated at-rest representation for pre-drawn bearer capabilities.
//!
//! This intentionally delegates keys and the cipher to the application's store
//! (which already owns that hierarchy). A pool entry is never serialized or
//! cached as plaintext by this module.

use crate::pool::{CoverContextVersion, PoolEntry};
use zeroize::Zeroize;

const AAD: &[u8] = b"osl-cover-ai-pool-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedPoolEntry { pub nonce: Vec<u8>, pub ciphertext: Vec<u8> }

pub trait PoolAead {
    type Error;
    fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<SealedPoolEntry, Self::Error>;
    fn open(&self, aad: &[u8], sealed: &SealedPoolEntry) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Debug, Eq, PartialEq)]
pub enum PoolAtRestError { Aead, Malformed }

pub fn seal<A: PoolAead>(aead: &A, entry: &PoolEntry) -> Result<SealedPoolEntry, PoolAtRestError> {
    let mut plaintext = encode(entry).ok_or(PoolAtRestError::Malformed)?;
    let sealed = aead.seal(AAD, &plaintext).map_err(|_| PoolAtRestError::Aead);
    plaintext.zeroize();
    sealed
}

pub fn open<A: PoolAead>(aead: &A, sealed: &SealedPoolEntry) -> Result<PoolEntry, PoolAtRestError> {
    let mut plaintext = aead.open(AAD, sealed).map_err(|_| PoolAtRestError::Aead)?;
    let entry = decode(&plaintext);
    plaintext.zeroize();
    entry
}

fn encode(entry: &PoolEntry) -> Option<Vec<u8>> {
    let cover = entry.cover.as_bytes();
    if entry.capability.len() > u16::MAX as usize || cover.len() > u16::MAX as usize { return None; }
    let mut out = Vec::with_capacity(2 + entry.capability.len() + 2 + cover.len() + 32 + 8);
    out.extend_from_slice(&(entry.capability.len() as u16).to_be_bytes()); out.extend_from_slice(&entry.capability);
    out.extend_from_slice(&(cover.len() as u16).to_be_bytes()); out.extend_from_slice(cover);
    out.extend_from_slice(&entry.context.0); out.extend_from_slice(&entry.created_at_seconds.to_be_bytes()); Some(out)
}

fn decode(bytes: &[u8]) -> Result<PoolEntry, PoolAtRestError> {
    let mut at = 0usize;
    let take = |n: usize, at: &mut usize| -> Option<&[u8]> { let end = at.checked_add(n)?; let part = bytes.get(*at..end)?; *at=end; Some(part) };
    let cap_len = u16::from_be_bytes(take(2, &mut at).ok_or(PoolAtRestError::Malformed)?.try_into().unwrap()) as usize;
    let capability = take(cap_len, &mut at).ok_or(PoolAtRestError::Malformed)?.to_vec();
    let cover_len = u16::from_be_bytes(take(2, &mut at).ok_or(PoolAtRestError::Malformed)?.try_into().unwrap()) as usize;
    let cover = String::from_utf8(take(cover_len, &mut at).ok_or(PoolAtRestError::Malformed)?.to_vec()).map_err(|_| PoolAtRestError::Malformed)?;
    let context = CoverContextVersion(take(32, &mut at).ok_or(PoolAtRestError::Malformed)?.try_into().unwrap());
    let created_at_seconds = u64::from_be_bytes(take(8, &mut at).ok_or(PoolAtRestError::Malformed)?.try_into().unwrap());
    if at != bytes.len() { return Err(PoolAtRestError::Malformed); }
    PoolEntry::new(capability, cover, context, created_at_seconds).ok_or(PoolAtRestError::Malformed)
}
