//! Pre-generated, single-use carrier capabilities.
//!
//! Entries are generated and scored off the send path.  Consuming an entry is
//! a pop: it never performs inference and an entry can never be returned.

use std::collections::VecDeque;
use zeroize::Zeroize;

pub const MAX_POOL_ENTRIES_PER_SCOPE: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PoolScope(pub [u8; 32]);

/// Hash/version of the cover-only history against which a carrier was made.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverContextVersion(pub [u8; 32]);

#[derive(Debug)]
pub struct PoolEntry {
    pub(crate) capability: Vec<u8>,
    pub cover: String,
    pub context: CoverContextVersion,
    pub created_at_seconds: u64,
}

impl PoolEntry {
    pub fn new(capability: Vec<u8>, cover: String, context: CoverContextVersion, created_at_seconds: u64) -> Option<Self> {
        if capability.is_empty() || cover.is_empty() || cover.len() > 1024 { return None; }
        Some(Self { capability, cover, context, created_at_seconds })
    }

    /// Capability bytes are deliberately exposed only at the instant of send.
    pub fn into_capability(mut self) -> Vec<u8> { std::mem::take(&mut self.capability) }
}

impl Drop for PoolEntry { fn drop(&mut self) { self.capability.zeroize(); } }

pub struct CarrierPool { entries: VecDeque<PoolEntry> }

impl CarrierPool {
    pub fn new() -> Self { Self { entries: VecDeque::new() } }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Background producers must not evict a live bearer capability to refill.
    pub fn push(&mut self, entry: PoolEntry) -> Result<(), PoolEntry> {
        if self.entries.len() >= MAX_POOL_ENTRIES_PER_SCOPE { Err(entry) } else { self.entries.push_back(entry); Ok(()) }
    }

    /// The complete fast path for a send: consume one ready capability.
    pub fn take_ready(&mut self, context: CoverContextVersion, now_seconds: u64, max_age_seconds: u64) -> Option<PoolEntry> {
        self.discard_stale(context, now_seconds, max_age_seconds);
        self.entries.pop_front()
    }

    /// Context coherence changes, scope burn, and age all destroy entries.
    pub fn discard_stale(&mut self, context: CoverContextVersion, now_seconds: u64, max_age_seconds: u64) {
        self.entries.retain(|entry| entry.context == context && now_seconds.saturating_sub(entry.created_at_seconds) <= max_age_seconds);
    }
    pub fn invalidate_for_context_change(&mut self) { self.entries.clear(); }
    pub fn invalidate_for_scope_burn(&mut self) { self.entries.clear(); }
}

impl Default for CarrierPool { fn default() -> Self { Self::new() } }
