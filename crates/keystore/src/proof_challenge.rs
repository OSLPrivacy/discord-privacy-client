//! `ProofChallenge` — a server-issued nonce bound to an account-ownership
//! proof.
//!
//! Part of checklist item **A4 · proven platform-account registration**
//! (`docs/design/osl-internal-build-checklist.md`): nobody may pre-register
//! another owner's public service ID. A signature alone proves nothing —
//! the client has to sign *something*, and if that something isn't tied to
//! the exact account, the exact claiming owner, and a bounded time window,
//! a signature captured for one registration can be replayed against a
//! different account, a different owner, or replayed later after the
//! window it was meant for has passed. Binding the nonce to all three
//! closes that off structurally instead of relying on a caller to
//! remember to re-check them.
//!
//! This module defines the value type only: issuance, transport, and
//! signature verification live in separate client/server units. Checking a
//! client's signed response against `nonce` is deliberately separate from
//! constructing and carrying the challenge.
//!
//! A sibling unit (`a26`) is concurrently defining `AccountOwnershipError`
//! in this crate, which is expected to eventually wrap the failure modes a
//! caller hits against this type (account mismatch, owner mismatch,
//! expired, already spent). This module does not define that enum and
//! does not depend on it; reconcile the two once `a26` lands.

use core::fmt;

/// Byte length of the challenge nonce. Matches the 32-byte nonce
/// convention used elsewhere in this crate (see [`crate::prekeys`]'s
/// `SpkEntry`/`OpkEntry` key material and its `byte_array_b64::array_32`
/// serde helper).
pub const PROOF_CHALLENGE_NONCE_BYTES: usize = 32;

/// A server-issued, single-use nonce bound to one account-ownership proof
/// attempt.
///
/// Binding is the entire point: a valid signature over [`Self::nonce`]
/// proves nothing on its own unless the verifier also confirms the request
/// being authorized matches `service_account_id`, `owner_user_id`, and
/// `expires_at_unix_seconds`. [`Self::binds`] and [`Self::is_expired`]
/// expose exactly that check; there is deliberately no all-in-one
/// `verify()` here, since checking a client's *signature* over the nonce
/// is a different unit's job.
///
/// Single-use is expressed as state on the value itself, not as a
/// convention callers have to remember: [`Self::spend`] is the only way to
/// transition `spent` from `false` to `true`, it reports whether *this*
/// call was the one that did so, and there is no setter that can move
/// `spent` back to `false`. A caller that ignores the return value and
/// spends twice still only ever gets a live grant once — the second call
/// observes `spent == true` and structurally refuses by returning `false`
/// rather than silently succeeding again.
///
/// Persisting spent-nonce state across process restarts (e.g. a
/// server-side table keyed on the nonce) is a caller concern outside this
/// type; this type only guarantees that a single in-memory value cannot be
/// spent more than once.
pub struct ProofChallenge {
    nonce: [u8; PROOF_CHALLENGE_NONCE_BYTES],
    /// The target platform account this proof is for. Never printed by
    /// `Debug`/`Display` — see the hand-written impls below.
    service_account_id: String,
    /// The OSL identity (see [`crate::identity::Identity::user_id`])
    /// claiming ownership of `service_account_id`. Never printed by
    /// `Debug`/`Display`.
    owner_user_id: String,
    issued_at_unix_seconds: u64,
    expires_at_unix_seconds: u64,
    spent: bool,
}

impl ProofChallenge {
    /// Construct a fresh, unspent challenge. `expires_at_unix_seconds`
    /// must be strictly greater than `issued_at_unix_seconds` — a
    /// challenge that is already expired at issuance is refused rather
    /// than silently constructed, since a caller that got the arithmetic
    /// wrong upstream deserves a loud failure here, not a challenge nobody
    /// can ever satisfy.
    pub fn new(
        nonce: [u8; PROOF_CHALLENGE_NONCE_BYTES],
        service_account_id: impl Into<String>,
        owner_user_id: impl Into<String>,
        issued_at_unix_seconds: u64,
        expires_at_unix_seconds: u64,
    ) -> Option<Self> {
        if expires_at_unix_seconds <= issued_at_unix_seconds {
            return None;
        }
        Some(ProofChallenge {
            nonce,
            service_account_id: service_account_id.into(),
            owner_user_id: owner_user_id.into(),
            issued_at_unix_seconds,
            expires_at_unix_seconds,
            spent: false,
        })
    }

    pub fn nonce(&self) -> &[u8; PROOF_CHALLENGE_NONCE_BYTES] {
        &self.nonce
    }

    pub fn service_account_id(&self) -> &str {
        &self.service_account_id
    }

    pub fn owner_user_id(&self) -> &str {
        &self.owner_user_id
    }

    pub fn issued_at_unix_seconds(&self) -> u64 {
        self.issued_at_unix_seconds
    }

    pub fn expires_at_unix_seconds(&self) -> u64 {
        self.expires_at_unix_seconds
    }

    pub fn is_spent(&self) -> bool {
        self.spent
    }

    /// True iff `service_account_id` and `owner_user_id` both match this
    /// challenge's binding. This checks binding only — a caller must
    /// separately check [`Self::is_expired`] and [`Self::is_spent`], and
    /// (elsewhere) the client's signature over [`Self::nonce`], before
    /// treating a proof as valid.
    pub fn binds(&self, service_account_id: &str, owner_user_id: &str) -> bool {
        self.service_account_id == service_account_id && self.owner_user_id == owner_user_id
    }

    /// True iff `now_unix_seconds` is at or past the expiry. Expiry is
    /// inclusive of the boundary instant: a challenge is not usable at
    /// exactly `expires_at_unix_seconds`.
    pub fn is_expired(&self, now_unix_seconds: u64) -> bool {
        now_unix_seconds >= self.expires_at_unix_seconds
    }

    /// Attempt to spend this challenge. Returns `true` if this call is the
    /// one that transitioned it from unspent to spent; returns `false` if
    /// it was already spent, refusing the replay. There is no way to
    /// un-spend a challenge, and no way to read `spent` as `false` again
    /// once a `spend()` call has returned `true`.
    pub fn spend(&mut self) -> bool {
        if self.spent {
            false
        } else {
            self.spent = true;
            true
        }
    }
}

/// Hand-written: the derived impl would print `service_account_id` and
/// `owner_user_id`, and this type must never leak an account identifier
/// through `Debug`. `nonce` is redacted too, out of caution — although the
/// server hands it to the client anyway, a captured value bound to a live
/// window is exactly the credential a replay needs, so it does not belong
/// in a log line either.
impl fmt::Debug for ProofChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProofChallenge")
            .field("nonce", &"[REDACTED]")
            .field("service_account_id", &"[REDACTED]")
            .field("owner_user_id", &"[REDACTED]")
            .field("issued_at_unix_seconds", &self.issued_at_unix_seconds)
            .field("expires_at_unix_seconds", &self.expires_at_unix_seconds)
            .field("spent", &self.spent)
            .finish()
    }
}

/// Hand-written for the same reason as `Debug`: no identifier, no nonce.
impl fmt::Display for ProofChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ProofChallenge(expires_at={}, spent={})",
            self.expires_at_unix_seconds, self.spent
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge(
        service_account_id: &str,
        owner_user_id: &str,
        issued_at: u64,
        expires_at: u64,
    ) -> ProofChallenge {
        ProofChallenge::new(
            [7u8; PROOF_CHALLENGE_NONCE_BYTES],
            service_account_id,
            owner_user_id,
            issued_at,
            expires_at,
        )
        .expect("valid issued/expiry ordering")
    }

    #[test]
    fn new_refuses_non_positive_lifetime() {
        // expires_at == issued_at
        assert!(ProofChallenge::new([0u8; 32], "acct-a", "owner-1", 100, 100).is_none());
        // expires_at < issued_at
        assert!(ProofChallenge::new([0u8; 32], "acct-a", "owner-1", 100, 50).is_none());
    }

    #[test]
    fn binds_rejects_mismatched_account() {
        let c = challenge("acct-a", "owner-1", 0, 100);
        assert!(c.binds("acct-a", "owner-1"));
        // Same owner, different target account: must not satisfy account B.
        assert!(!c.binds("acct-b", "owner-1"));
    }

    #[test]
    fn binds_rejects_mismatched_owner() {
        let c = challenge("acct-a", "owner-1", 0, 100);
        // Same account, different claiming owner: must not satisfy it either.
        assert!(!c.binds("acct-a", "owner-2"));
    }

    #[test]
    fn expired_challenge_is_refused() {
        let c = challenge("acct-a", "owner-1", 0, 100);
        assert!(!c.is_expired(99));
        // Inclusive boundary: exactly at expiry is already refused.
        assert!(c.is_expired(100));
        assert!(c.is_expired(101));
    }

    #[test]
    fn spent_challenge_cannot_be_reused() {
        let mut c = challenge("acct-a", "owner-1", 0, 100);
        assert!(!c.is_spent());
        // First spend succeeds.
        assert!(c.spend());
        assert!(c.is_spent());
        // Replay: a second spend on the same value is structurally refused.
        assert!(!c.spend());
        // Still spent, not un-spent by the failed attempt.
        assert!(c.is_spent());
    }

    #[test]
    fn debug_and_display_omit_identifiers_and_nonce() {
        let c = challenge("super-secret-account-id", "super-secret-owner-id", 0, 100);
        let debug_str = format!("{c:?}");
        let display_str = format!("{c}");
        assert!(!debug_str.contains("super-secret-account-id"));
        assert!(!debug_str.contains("super-secret-owner-id"));
        assert!(
            !debug_str.contains("[7"),
            "nonce bytes leaked into Debug: {debug_str}"
        );
        assert!(!display_str.contains("super-secret-account-id"));
        assert!(!display_str.contains("super-secret-owner-id"));
    }
}
