//! Bounded skipped-message-key store.
//!
//! # Why this is a memory-exhaustion vector in the first place
//!
//! Out-of-order delivery forces a receiver to keep message keys for
//! messages it has not yet seen. The Double Ratchet specification says
//! only "an implementation SHOULD set a limit"; the naive reading — a
//! map that grows with every gap — lets one peer pin arbitrary memory.
//!
//! # The three bounds, and what each one stops
//!
//! 1. **Per-message derivation cap** ([`SkipParams::max_skip_per_message`]).
//!    A single message may never cause more than this many chain-key
//!    derivations. This is the bound that stops a header claiming
//!    `counter = 2^32 - 1` from turning into a four-billion-iteration
//!    loop. Crucially it is checked *before* any state mutation, so a
//!    rejected message leaves the session exactly as it was.
//!
//! 2. **Per-chain and global key caps** ([`SkipParams::max_keys_per_chain`],
//!    [`SkipParams::max_total_keys`]). Total resident keys are hard
//!    capped, so worst-case memory is
//!    `max_total_keys * (32 + 32 + overhead)` — a few hundred KiB at
//!    the defaults, regardless of peer behaviour.
//!
//! 3. **Chain-count cap** ([`SkipParams::max_chains`]). Bounds the
//!    number of *trial header decryptions* an inbound message can
//!    cost. With header encryption the receiver must try candidate
//!    header keys; without this cap that trial set would grow without
//!    limit and turn header encryption itself into the DoS vector.
//!
//! Plus a logical-clock age bound ([`SkipParams::max_age`]) measured in
//! *messages accepted by this session*, not wall-clock seconds. A
//! logical clock is used deliberately: it is deterministic (so the
//! bound is testable), it cannot be manipulated by clock skew, and it
//! degrades correctly on a device that was offline for a month.
//!
//! # The real improvement over plain Signal
//!
//! In the non-header-encrypted Double Ratchet that Signal ships, the
//! ratchet header is **plaintext**. Anyone who can inject a message
//! can therefore name a counter and make the receiver derive that many
//! chain keys *before* the AEAD tag check tells it the message was
//! forged. The caps above are the only thing standing between the
//! receiver and that work.
//!
//! Here the header is itself AEAD-authenticated under a header key. An
//! attacker who does not hold the header key cannot get past the trial
//! decryptions at all: **zero** chain-key derivations, zero insertions.
//! The caps still exist, but they now only constrain a genuine —
//! possibly buggy or hostile — *peer*, not an arbitrary network
//! attacker. That is a strictly smaller attack surface than the
//! shipped design.
//!
//! Eviction is FIFO by insertion order within the oldest chain, which
//! is deterministic and therefore testable; it is not claimed to be
//! optimal.

use crate::codec::{Reader, Writer};
use crate::error::{Error, Result};
use crate::primitives::{Secret32, AEAD_KEY};
use std::collections::BTreeMap;
use std::collections::VecDeque;

/// Storage policy. Defaults are documented in `DESIGN.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkipParams {
    pub max_skip_per_message: u32,
    pub max_keys_per_chain: usize,
    pub max_total_keys: usize,
    pub max_chains: usize,
    /// Age bound in messages accepted by this session.
    pub max_age: u64,
}

impl Default for SkipParams {
    fn default() -> Self {
        SkipParams {
            max_skip_per_message: 512,
            max_keys_per_chain: 512,
            max_total_keys: 2048,
            max_chains: 5,
            max_age: 100_000,
        }
    }
}

impl SkipParams {
    fn validated(self) -> Result<Self> {
        if self.max_chains == 0
            || self.max_keys_per_chain == 0
            || self.max_total_keys == 0
            || self.max_skip_per_message == 0
        {
            return Err(Error::PolicyBound("skip params must be non-zero"));
        }
        if self.max_keys_per_chain > self.max_total_keys {
            return Err(Error::PolicyBound("per-chain cap exceeds global cap"));
        }
        Ok(self)
    }
}

#[derive(Clone)]
struct ChainBucket {
    header_key: [u8; AEAD_KEY],
    /// counter -> (message key, insertion tick)
    keys: BTreeMap<u32, (Secret32, u64)>,
}

/// Bounded store of `(header_key, counter) -> message_key`.
#[derive(Clone)]
pub struct SkippedKeys {
    params: SkipParams,
    chains: VecDeque<ChainBucket>,
    total: usize,
    tick: u64,
}

impl SkippedKeys {
    pub fn new(params: SkipParams) -> Result<Self> {
        Ok(SkippedKeys {
            params: params.validated()?,
            chains: VecDeque::new(),
            total: 0,
            tick: 0,
        })
    }

    pub fn params(&self) -> SkipParams {
        self.params
    }

    pub fn total_keys(&self) -> usize {
        self.total
    }

    pub fn chain_count(&self) -> usize {
        self.chains.len()
    }

    /// Advance the logical clock and expire aged-out entries. Called
    /// once per successfully authenticated inbound message.
    pub fn tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        let cutoff = self.tick.saturating_sub(self.params.max_age);
        if cutoff == 0 {
            return;
        }
        for chain in self.chains.iter_mut() {
            let stale: Vec<u32> = chain
                .keys
                .iter()
                .filter(|(_, (_, t))| *t < cutoff)
                .map(|(c, _)| *c)
                .collect();
            for c in stale {
                if chain.keys.remove(&c).is_some() {
                    self.total = self.total.saturating_sub(1);
                }
            }
        }
        self.chains.retain(|c| !c.keys.is_empty());
    }

    /// Candidate header keys to trial-decrypt against, oldest first.
    /// Length is bounded by `max_chains`.
    pub fn header_keys(&self) -> Vec<[u8; AEAD_KEY]> {
        self.chains.iter().map(|c| c.header_key).collect()
    }

    /// Check whether `count` derivations are permitted. Called before
    /// any mutation so a refusal leaves state untouched.
    pub fn check_skip(&self, count: u64) -> Result<()> {
        if count > u64::from(self.params.max_skip_per_message) {
            return Err(Error::SkipLimitExceeded {
                requested: count,
                limit: u64::from(self.params.max_skip_per_message),
            });
        }
        Ok(())
    }

    /// Insert one skipped key. Evicts under pressure; never grows past
    /// the caps.
    pub fn insert(&mut self, header_key: &[u8; AEAD_KEY], counter: u32, mk: Secret32) {
        let idx = self.chains.iter().position(|c| &c.header_key == header_key);
        let idx = match idx {
            Some(i) => i,
            None => {
                while self.chains.len() >= self.params.max_chains {
                    if let Some(dropped) = self.chains.pop_front() {
                        self.total = self.total.saturating_sub(dropped.keys.len());
                    } else {
                        break;
                    }
                }
                self.chains.push_back(ChainBucket {
                    header_key: *header_key,
                    keys: BTreeMap::new(),
                });
                self.chains.len().saturating_sub(1)
            }
        };

        // Per-chain cap: drop this chain's lowest counter (the oldest
        // gap, least likely to still be in flight).
        if let Some(chain) = self.chains.get_mut(idx) {
            while chain.keys.len() >= self.params.max_keys_per_chain {
                let Some(&lowest) = chain.keys.keys().next() else {
                    break;
                };
                if chain.keys.remove(&lowest).is_some() {
                    self.total = self.total.saturating_sub(1);
                }
            }
        }

        // Global cap: evict from the oldest chain.
        while self.total >= self.params.max_total_keys {
            let mut evicted = false;
            for i in 0..self.chains.len() {
                let Some(chain) = self.chains.get_mut(i) else {
                    break;
                };
                let Some(&lowest) = chain.keys.keys().next() else {
                    continue;
                };
                if chain.keys.remove(&lowest).is_some() {
                    self.total = self.total.saturating_sub(1);
                    evicted = true;
                }
                break;
            }
            self.chains.retain(|c| !c.keys.is_empty());
            if !evicted {
                break;
            }
        }

        // The chain may have been dropped by the retain above.
        let idx = match self.chains.iter().position(|c| &c.header_key == header_key) {
            Some(i) => i,
            None => {
                self.chains.push_back(ChainBucket {
                    header_key: *header_key,
                    keys: BTreeMap::new(),
                });
                self.chains.len().saturating_sub(1)
            }
        };
        if let Some(chain) = self.chains.get_mut(idx) {
            if chain.keys.insert(counter, (mk, self.tick)).is_none() {
                self.total = self.total.saturating_add(1);
            }
        }
    }

    /// Take (and remove) a skipped key. Removal on use is what makes
    /// replay of an out-of-order message fail.
    pub fn take(&mut self, header_key: &[u8; AEAD_KEY], counter: u32) -> Option<Secret32> {
        let idx = self.chains.iter().position(|c| &c.header_key == header_key)?;
        let chain = self.chains.get_mut(idx)?;
        let (mk, _) = chain.keys.remove(&counter)?;
        self.total = self.total.saturating_sub(1);
        if chain.keys.is_empty() {
            self.chains.remove(idx);
        }
        Some(mk)
    }

    pub(crate) fn export(&self, w: &mut Writer) -> Result<()> {
        w.varint(self.params.max_skip_per_message);
        w.varint(u32::try_from(self.params.max_keys_per_chain).unwrap_or(u32::MAX));
        w.varint(u32::try_from(self.params.max_total_keys).unwrap_or(u32::MAX));
        w.varint(u32::try_from(self.params.max_chains).unwrap_or(u32::MAX));
        w.varint(u32::try_from(self.params.max_age).unwrap_or(u32::MAX));
        w.varint(u32::try_from(self.tick).unwrap_or(u32::MAX));
        w.varint(u32::try_from(self.chains.len()).unwrap_or(u32::MAX));
        for chain in &self.chains {
            w.bytes(&chain.header_key);
            w.varint(u32::try_from(chain.keys.len()).unwrap_or(u32::MAX));
            for (counter, (mk, t)) in &chain.keys {
                w.varint(*counter);
                w.varint(u32::try_from(*t).unwrap_or(u32::MAX));
                w.bytes(mk.as_bytes());
            }
        }
        Ok(())
    }

    pub(crate) fn import(r: &mut Reader<'_>) -> Result<Self> {
        let params = SkipParams {
            max_skip_per_message: r.varint()?,
            max_keys_per_chain: r.varint()? as usize,
            max_total_keys: r.varint()? as usize,
            max_chains: r.varint()? as usize,
            max_age: u64::from(r.varint()?),
        }
        .validated()?;
        let tick = u64::from(r.varint()?);
        let n_chains = r.varint()? as usize;
        if n_chains > params.max_chains {
            return Err(Error::BadStateFormat);
        }
        let mut chains = VecDeque::with_capacity(n_chains);
        let mut total = 0usize;
        for _ in 0..n_chains {
            let header_key = r.array::<AEAD_KEY>()?;
            let n_keys = r.varint()? as usize;
            if n_keys > params.max_keys_per_chain {
                return Err(Error::BadStateFormat);
            }
            let mut keys = BTreeMap::new();
            for _ in 0..n_keys {
                let counter = r.varint()?;
                let t = u64::from(r.varint()?);
                let mk = Secret32::from_bytes(r.array::<32>()?);
                if keys.insert(counter, (mk, t)).is_none() {
                    total += 1;
                }
            }
            chains.push_back(ChainBucket { header_key, keys });
        }
        if total > params.max_total_keys {
            return Err(Error::BadStateFormat);
        }
        Ok(SkippedKeys {
            params,
            chains,
            total,
            tick,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(params: SkipParams) -> SkippedKeys {
        SkippedKeys::new(params).expect("params")
    }

    #[test]
    fn insert_and_take_roundtrip() {
        let mut s = store(SkipParams::default());
        let hk = [1u8; 32];
        s.insert(&hk, 5, Secret32::from_bytes([7u8; 32]));
        assert_eq!(s.total_keys(), 1);
        assert_eq!(
            s.take(&hk, 5),
            Some(Secret32::from_bytes([7u8; 32]))
        );
        assert_eq!(s.total_keys(), 0);
        // Removal on use: a second take fails, which is what makes
        // replay of a skipped message fail.
        assert_eq!(s.take(&hk, 5), None);
        assert_eq!(s.chain_count(), 0);
    }

    #[test]
    fn global_cap_is_never_exceeded() {
        let p = SkipParams {
            max_skip_per_message: 10_000,
            max_keys_per_chain: 50,
            max_total_keys: 100,
            max_chains: 4,
            max_age: 1_000_000,
        };
        let mut s = store(p);
        for chain in 0..4u8 {
            let hk = [chain; 32];
            for c in 0..1000u32 {
                s.insert(&hk, c, Secret32::from_bytes([chain; 32]));
                assert!(
                    s.total_keys() <= p.max_total_keys,
                    "global cap breached: {}",
                    s.total_keys()
                );
            }
        }
        assert!(s.chain_count() <= p.max_chains);
    }

    #[test]
    fn per_chain_cap_is_never_exceeded() {
        let p = SkipParams {
            max_skip_per_message: 10_000,
            max_keys_per_chain: 8,
            max_total_keys: 1000,
            max_chains: 4,
            max_age: 1_000_000,
        };
        let mut s = store(p);
        let hk = [3u8; 32];
        for c in 0..500u32 {
            s.insert(&hk, c, Secret32::from_bytes([1u8; 32]));
        }
        assert_eq!(s.total_keys(), 8);
        // The most recent 8 counters survive; the oldest gaps were
        // evicted first.
        for c in 492..500u32 {
            assert!(s.take(&hk, c).is_some(), "expected counter {c}");
        }
    }

    #[test]
    fn chain_cap_bounds_trial_decryption_cost() {
        let p = SkipParams {
            max_chains: 3,
            ..SkipParams::default()
        };
        let mut s = store(p);
        for chain in 0..50u8 {
            s.insert(&[chain; 32], 1, Secret32::from_bytes([0u8; 32]));
            assert!(s.header_keys().len() <= 3);
        }
        assert_eq!(s.chain_count(), 3);
    }

    #[test]
    fn check_skip_refuses_absurd_counters() {
        let s = store(SkipParams::default());
        assert!(s.check_skip(1).is_ok());
        assert!(s.check_skip(512).is_ok());
        assert_eq!(
            s.check_skip(u64::from(u32::MAX)),
            Err(Error::SkipLimitExceeded {
                requested: u64::from(u32::MAX),
                limit: 512
            })
        );
    }

    #[test]
    fn age_bound_expires_entries() {
        let p = SkipParams {
            max_age: 10,
            ..SkipParams::default()
        };
        let mut s = store(p);
        s.insert(&[1u8; 32], 0, Secret32::from_bytes([0u8; 32]));
        for _ in 0..11 {
            s.tick();
        }
        assert_eq!(s.total_keys(), 0, "aged entry should have been swept");
    }

    #[test]
    fn export_import_roundtrips() {
        let mut s = store(SkipParams::default());
        s.insert(&[1u8; 32], 3, Secret32::from_bytes([9u8; 32]));
        s.insert(&[2u8; 32], 4, Secret32::from_bytes([8u8; 32]));
        let mut w = Writer::default();
        s.export(&mut w).expect("export");
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes);
        let mut back = SkippedKeys::import(&mut r).expect("import");
        r.finish().expect("no trailing bytes");
        assert_eq!(back.total_keys(), 2);
        assert_eq!(
            back.take(&[1u8; 32], 3),
            Some(Secret32::from_bytes([9u8; 32]))
        );
    }

    #[test]
    fn import_rejects_over_cap_state() {
        // A hand-built state claiming more chains than its own policy.
        let mut w = Writer::default();
        w.varint(512).varint(512).varint(2048).varint(2).varint(100);
        w.varint(0);
        w.varint(9); // 9 chains, cap is 2
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes);
        assert!(matches!(
            SkippedKeys::import(&mut r),
            Err(Error::BadStateFormat)
        ));
    }

    #[test]
    fn invalid_params_are_rejected() {
        assert!(SkippedKeys::new(SkipParams {
            max_chains: 0,
            ..SkipParams::default()
        })
        .is_err());
        assert!(SkippedKeys::new(SkipParams {
            max_keys_per_chain: 10_000,
            max_total_keys: 10,
            ..SkipParams::default()
        })
        .is_err());
    }
}
