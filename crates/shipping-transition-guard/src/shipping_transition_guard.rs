//! Authenticated shipping boundary for authority-changing transitions.
//!
//! Sequence numbers and metadata versions establish freshness only after the
//! signer and its grant/token have been matched to the current authority.
//! This ordering is intentional: a removed or revoked signer cannot regain
//! authority by choosing a larger counter than the replacement signer.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

const ENFORCE_REMOVED_FRIEND: bool = true;
const ENFORCE_ROTATED_FRIEND_KEY: bool = true;
const ENFORCE_PRO_GRANT: bool = true;
const ENFORCE_MAILBOX_TOKEN: bool = true;
const ENFORCE_REVOKED_SIGNER: bool = true;

pub const TRANSITION_GATES: [u32; 10] =
    [3558, 3728, 3730, 3732, 3789, 3983, 4354, 5168, 5170, 5171];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AuthorityClass {
    RemovedFriend,
    RotatedFriendKey,
    ProGrant,
    MailboxToken,
    RevokedSigner,
}

impl AuthorityClass {
    fn guard_enabled(self) -> bool {
        match self {
            Self::RemovedFriend => ENFORCE_REMOVED_FRIEND,
            Self::RotatedFriendKey => ENFORCE_ROTATED_FRIEND_KEY,
            Self::ProGrant => ENFORCE_PRO_GRANT,
            Self::MailboxToken => ENFORCE_MAILBOX_TOKEN,
            Self::RevokedSigner => ENFORCE_REVOKED_SIGNER,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ShippingStateKind {
    Message,
    Roster,
    Entitlement,
    File,
    Mailbox,
    Update,
    CarrierTable,
}

impl ShippingStateKind {
    pub const ALL: [Self; 7] = [
        Self::Message,
        Self::Roster,
        Self::Entitlement,
        Self::File,
        Self::Mailbox,
        Self::Update,
        Self::CarrierTable,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Roster => "roster",
            Self::Entitlement => "entitlement",
            Self::File => "file",
            Self::Mailbox => "mailbox",
            Self::Update => "update",
            Self::CarrierTable => "carrier-table",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Authority {
    pub public_key: VerifyingKey,
    pub credential: String,
    pub epoch: u64,
}

impl Authority {
    pub fn fingerprint(&self) -> String {
        hex_digest(self.public_key.as_bytes())
    }
}

#[derive(Clone, Debug)]
pub struct SignedShippingAction {
    pub gate: u32,
    pub external_id: String,
    pub signer: VerifyingKey,
    pub credential: String,
    pub epoch: u64,
    pub sequence_or_version: u64,
    pub state_kind: ShippingStateKind,
    pub payload_digest: [u8; 32],
    signature: Signature,
}

impl SignedShippingAction {
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        gate: u32,
        external_id: impl Into<String>,
        credential: impl Into<String>,
        epoch: u64,
        sequence_or_version: u64,
        state_kind: ShippingStateKind,
        payload: &[u8],
        secret: &SigningKey,
    ) -> Self {
        let signer = secret.verifying_key();
        let mut action = Self {
            gate,
            external_id: external_id.into(),
            signer,
            credential: credential.into(),
            epoch,
            sequence_or_version,
            state_kind,
            payload_digest: Sha256::digest(payload).into(),
            signature: Signature::from_bytes(&[0; 64]),
        };
        action.signature = secret.sign(&action.signing_bytes());
        action
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = b"OSL-SHIPPING-TRANSITION-V1\0".to_vec();
        bytes.extend_from_slice(&self.gate.to_be_bytes());
        append_bounded(&mut bytes, self.external_id.as_bytes());
        bytes.extend_from_slice(self.signer.as_bytes());
        append_bounded(&mut bytes, self.credential.as_bytes());
        bytes.extend_from_slice(&self.epoch.to_be_bytes());
        bytes.extend_from_slice(&self.sequence_or_version.to_be_bytes());
        bytes.push(self.state_kind as u8);
        bytes.extend_from_slice(&self.payload_digest);
        bytes
    }

    pub fn signer_fingerprint(&self) -> String {
        hex_digest(self.signer.as_bytes())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShippingStateSnapshot {
    counts: BTreeMap<ShippingStateKind, usize>,
    digest: [u8; 32],
}

impl ShippingStateSnapshot {
    pub fn count(&self, kind: ShippingStateKind) -> usize {
        self.counts.get(&kind).copied().unwrap_or(0)
    }

    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }

    pub fn digest_hex(&self) -> String {
        hex_bytes(&self.digest)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TransitionRefusal {
    InvalidSignature {
        signer: String,
    },
    StaleAuthority {
        signer: String,
        stale_credential: String,
        current_credential: String,
    },
    UnknownAuthority {
        signer: String,
    },
    Replay {
        external_id: String,
    },
    CounterRollback {
        presented: u64,
        current: u64,
    },
    WrongShippingEntry {
        expected: ShippingStateKind,
        presented: ShippingStateKind,
    },
}

#[derive(Clone)]
struct TransitionAuthority {
    class: AuthorityClass,
    stale: Authority,
    current: Authority,
    state_kinds: BTreeSet<ShippingStateKind>,
    floor: u64,
}

/// State held by the shipping process. The only mutation route is
/// [`submit`][Self::submit], which verifies the signature and transition
/// authority before touching any domain state.
pub struct ShippingTransitionGuard {
    authorities: BTreeMap<u32, TransitionAuthority>,
    seen_external_ids: BTreeSet<String>,
    committed: BTreeMap<ShippingStateKind, Vec<(String, [u8; 32])>>,
    verifier_calls: usize,
}

impl ShippingTransitionGuard {
    pub fn new() -> Self {
        Self {
            authorities: BTreeMap::new(),
            seen_external_ids: BTreeSet::new(),
            committed: BTreeMap::new(),
            verifier_calls: 0,
        }
    }

    pub fn install_transition(
        &mut self,
        gate: u32,
        class: AuthorityClass,
        stale: Authority,
        current: Authority,
        state_kind: ShippingStateKind,
        counter_floor: u64,
    ) -> Result<(), String> {
        if !TRANSITION_GATES.contains(&gate) || stale == current {
            return Err(format!("invalid shipping transition for gate {gate}"));
        }
        if self
            .authorities
            .insert(
                gate,
                TransitionAuthority {
                    class,
                    stale,
                    current,
                    state_kinds: BTreeSet::from([state_kind]),
                    floor: counter_floor,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate shipping transition gate {gate}"));
        }
        Ok(())
    }

    pub fn allow_shipping_entry(
        &mut self,
        gate: u32,
        state_kind: ShippingStateKind,
    ) -> Result<(), String> {
        self.authorities
            .get_mut(&gate)
            .ok_or_else(|| format!("unknown shipping transition gate {gate}"))?
            .state_kinds
            .insert(state_kind);
        Ok(())
    }

    /// Records a pointer that was legitimately consumed before its actor was
    /// removed. Replaying it after removal must still be refused on stale
    /// authority before the ordinary replay check.
    pub fn record_pre_transition_external_id(&mut self, external_id: impl Into<String>) {
        self.seen_external_ids.insert(external_id.into());
    }

    pub fn submit(&mut self, action: &SignedShippingAction) -> Result<(), TransitionRefusal> {
        self.verifier_calls += 1;
        let signer = action.signer_fingerprint();
        if action
            .signer
            .verify(&action.signing_bytes(), &action.signature)
            .is_err()
        {
            return Err(TransitionRefusal::InvalidSignature { signer });
        }

        let transition = self.authorities.get(&action.gate).cloned().ok_or_else(|| {
            TransitionRefusal::UnknownAuthority {
                signer: signer.clone(),
            }
        })?;

        let current_match = authority_matches(&transition.current, action);
        let stale_match = authority_matches(&transition.stale, action);
        if stale_match && transition.class.guard_enabled() {
            return Err(TransitionRefusal::StaleAuthority {
                signer,
                stale_credential: transition.stale.credential,
                current_credential: transition.current.credential,
            });
        }
        if !current_match && !(stale_match && !transition.class.guard_enabled()) {
            return Err(TransitionRefusal::UnknownAuthority { signer });
        }
        if !transition.state_kinds.contains(&action.state_kind) {
            return Err(TransitionRefusal::WrongShippingEntry {
                expected: *transition
                    .state_kinds
                    .iter()
                    .next()
                    .expect("installed transition has a shipping entry"),
                presented: action.state_kind,
            });
        }
        if self.seen_external_ids.contains(&action.external_id) {
            return Err(TransitionRefusal::Replay {
                external_id: action.external_id.clone(),
            });
        }
        if action.sequence_or_version <= transition.floor {
            return Err(TransitionRefusal::CounterRollback {
                presented: action.sequence_or_version,
                current: transition.floor,
            });
        }

        self.seen_external_ids.insert(action.external_id.clone());
        self.committed
            .entry(action.state_kind)
            .or_default()
            .push((action.external_id.clone(), action.payload_digest));
        if let Some(authority) = self.authorities.get_mut(&action.gate) {
            authority.floor = action.sequence_or_version;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> ShippingStateSnapshot {
        let counts = ShippingStateKind::ALL
            .into_iter()
            .map(|kind| (kind, self.committed.get(&kind).map_or(0, Vec::len)))
            .collect::<BTreeMap<_, _>>();
        let mut digest = Sha256::new();
        for kind in ShippingStateKind::ALL {
            digest.update([kind as u8]);
            if let Some(entries) = self.committed.get(&kind) {
                for (external_id, payload) in entries {
                    append_bounded_digest(&mut digest, external_id.as_bytes());
                    digest.update(payload);
                }
            }
        }
        ShippingStateSnapshot {
            counts,
            digest: digest.finalize().into(),
        }
    }

    pub fn verifier_calls(&self) -> usize {
        self.verifier_calls
    }
}

impl Default for ShippingTransitionGuard {
    fn default() -> Self {
        Self::new()
    }
}

fn authority_matches(authority: &Authority, action: &SignedShippingAction) -> bool {
    authority.public_key == action.signer
        && authority.credential == action.credential
        && authority.epoch == action.epoch
}

fn append_bounded(target: &mut Vec<u8>, value: &[u8]) {
    target.extend_from_slice(&(value.len() as u64).to_be_bytes());
    target.extend_from_slice(value);
}

fn append_bounded_digest(target: &mut Sha256, value: &[u8]) {
    target.update((value.len() as u64).to_be_bytes());
    target.update(value);
}

fn hex_digest(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(HEX[(byte >> 4) as usize] as char);
        result.push(HEX[(byte & 0x0f) as usize] as char);
    }
    result
}
