//! Strict identity-binding contract for Scrub's destructive-action gate.
//!
//! Scrub detects accounts that MIGHT belong to the operator. Before any
//! destructive action (a guided deletion, a removal request, anything that
//! is not simply read-only indexing) may target one of those accounts,
//! something has to answer a single question precisely: does this exact
//! account, for this exact scope of action, genuinely belong to the person
//! who unlocked this OSL identity?
//!
//! This module defines that contract and nothing else. There is no
//! scanning here, no deletion, no wiring into `scrub_index` or any other
//! caller — see `docs/AGENT_CONTRACT.md` style scoping for why. It exists
//! so a later unit can wire a real caller against a type that already
//! makes the wrong answers impossible to construct, instead of retrofitting
//! safety onto a scan-and-delete path after the fact.
//!
//! ## Design: caller-pinned, not artifact-trusting
//!
//! [`IdentityBindingVerifier`] is constructed from the caller's own
//! unlocked [`keystore::Identity`] key material
//! ([`PinnedOwner::from_identity`]) — never from a string, a bool, or any
//! field carried on the thing being verified. This mirrors the substitution
//! defense `BundleVerifyPolicy` uses on the prekey-bundle path: the
//! verifier owns the truth and checks candidate facts against it, so a
//! candidate binding can never talk its way into being believed by
//! re-describing itself.
//!
//! ## Forbidden identity bases (made inexpressible, not just discouraged)
//!
//! - **Legacy Friend Code keys** — there is no [`PinnedOwner`] constructor
//!   that accepts a friend-code string or a key bundle parsed out of one;
//!   the only input is a live `keystore::Identity`.
//! - **Old `osl_` routing IDs** — [`PinnedOwner::from_identity`] hashes the
//!   identity's Ed25519 public key, never `Identity::user_id`. For a
//!   native identity `user_id` literally IS an `osl_...` string
//!   (`keystore::native_user_id`); for a legacy one it's a platform
//!   snowflake. Neither is cryptographic authority, so neither is used.
//! - **Renderer-supplied values** — see [`BindingEvidence::RendererSupplied`].
//! - **Unsigned metadata carried on the artifact under verification** — see
//!   [`BindingEvidence::UnsignedMetadata`].
//! - **A bare `safety_number_verified`-shaped bool** — see
//!   [`BindingEvidence::UnverifiedFlag`]. A bool alone cannot express
//!   *which* account, owner, and scope it was verified for, so it can
//!   never become a binding no matter how it is threaded into [`bind`].
//!
//! ## Default-refuse
//!
//! [`IdentityBindingVerifier::verify`] returns
//! [`IdentityBindingError::NoBinding`] for any account/scope pair with no
//! matching entry. Absence of a binding is refusal, never permission.
//!
//! [`bind`]: IdentityBindingVerifier::bind

use std::fmt;

use sha2::{Digest, Sha256};

use keystore::Identity;

const OWNER_FINGERPRINT_DOMAIN: &[u8] = b"OSL-identity-binding-owner-v1";

/// The trusted owner a binding is judged against.
///
/// Pinned from the caller's own unlocked identity key material. This is
/// the ONLY way to produce one: there is no constructor that takes a
/// string, a boolean, or any value read off an artifact being verified.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PinnedOwner {
    /// SHA-256 of the identity's Ed25519 public key, domain-separated.
    /// A fingerprint of key material — never a platform ID, Friend Code,
    /// or routing string.
    fingerprint: [u8; 32],
}

impl PinnedOwner {
    /// Pin the owner from the caller's own unlocked identity.
    ///
    /// Takes the actual `keystore::Identity`, not a `user_id` string, so a
    /// Friend Code payload, a Discord snowflake, or a legacy `osl_...`
    /// label can never be substituted for the caller's real key material.
    pub fn from_identity(identity: &Identity) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(OWNER_FINGERPRINT_DOMAIN);
        hasher.update(identity.ed25519_public.as_bytes());
        let digest = hasher.finalize();
        let mut fingerprint = [0u8; 32];
        fingerprint.copy_from_slice(&digest);
        Self { fingerprint }
    }
}

impl fmt::Debug for PinnedOwner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PinnedOwner(<redacted>)")
    }
}

/// One account on one service.
///
/// Field names deliberately match `scrub_index::ScrubAccountSelection` so a
/// selection a caller already validated against Scrub's own account list
/// can be passed straight through without remapping.
#[derive(Clone, Eq, PartialEq)]
pub struct AccountRef {
    pub service_id: String,
    pub account_id: String,
}

impl fmt::Debug for AccountRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AccountRef(<redacted>)")
    }
}

/// The exact class of action a binding authorizes.
///
/// Deliberately closed: no `Other(String)` variant and no wildcard
/// variant, so a binding can never be widened by inventing a new scope
/// string at the call site. Holding a `ScrubIndex` binding never implies
/// `ScrubDeletion` is also authorized.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum BindingScope {
    /// Scrub's indexing/preview may read this account's already-visible
    /// data. Does not authorize any destructive action.
    ScrubIndex,
    /// Scrub may drive a destructive removal action against this account.
    ScrubDeletion,
}

/// Where a candidate "this account belongs to the owner" fact originated.
///
/// Only [`BindingEvidence::CallerAttested`] may ever produce a binding.
/// The other variants exist so the refusal is enforced by data flowing
/// through [`IdentityBindingVerifier::bind`], not by trusting every call
/// site to remember to skip a bad path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingEvidence {
    /// The pinned owner authenticated this fact directly — e.g. the
    /// unlocked local identity signed over the exact account + scope, or
    /// an explicit out-of-band confirmation was captured through OSL's own
    /// protected surface. The only evidence [`bind`] accepts.
    ///
    /// [`bind`]: IdentityBindingVerifier::bind
    CallerAttested,
    /// A value the target service's own UI/DOM reported about itself
    /// (e.g. read out of Discord's renderer). Never sufficient: the
    /// renderer is attacker-observable and attacker-influenceable content,
    /// exactly the substitution surface this contract exists to close.
    RendererSupplied,
    /// A field carried on the artifact being verified with no independent
    /// signature over it (e.g. a plain JSON field on a scan result). Never
    /// sufficient by itself — an artifact must not be able to vouch for
    /// its own binding.
    UnsignedMetadata,
    /// A boolean "verified" flag with no attached account/owner/scope data
    /// — the `safety_number_verified` shape. Never sufficient: a lone bool
    /// cannot express *what* it was verified for.
    UnverifiedFlag,
}

/// One exact account + scope binding, pinned to one owner. Private:
/// callers only ever observe bindings through
/// [`IdentityBindingVerifier::verify`], never by reading this list
/// directly.
#[derive(Clone, PartialEq)]
struct AccountBinding {
    account: AccountRef,
    scope: BindingScope,
    owner: PinnedOwner,
}

impl AccountBinding {
    fn verifies(&self, owner: PinnedOwner, account: &AccountRef, scope: BindingScope) -> bool {
        self.owner == owner && self.scope == scope && self.account == *account
    }
}

/// Strict identity-binding verifier.
///
/// Pinned to exactly one [`PinnedOwner`] at construction; every binding
/// registered against it is implicitly scoped to that owner, so a verifier
/// built for one unlocked identity can never be tricked into authorizing
/// another identity's accounts.
///
/// Contract only: this type does not scan, does not call into Scrub's
/// indexer, and does not delete anything. It answers exactly one
/// question — "is THIS account, for THIS scope, bound to THIS
/// caller-pinned owner?" — and defaults to refusal.
pub struct IdentityBindingVerifier {
    owner: PinnedOwner,
    bindings: Vec<AccountBinding>,
}

impl IdentityBindingVerifier {
    /// Start a verifier pinned to `owner`. Empty: no account is bound
    /// until [`Self::bind`] succeeds.
    pub fn new(owner: PinnedOwner) -> Self {
        Self {
            owner,
            bindings: Vec::new(),
        }
    }

    /// The pinned owner every binding on this verifier is judged against.
    pub fn owner(&self) -> PinnedOwner {
        self.owner
    }

    /// Register an exact account + scope binding for the pinned owner.
    ///
    /// Refuses — and creates nothing — unless `evidence` is
    /// [`BindingEvidence::CallerAttested`]. A renderer-supplied value,
    /// unsigned artifact metadata, or a bare verified-flag can never
    /// produce a binding no matter how it is threaded in here.
    ///
    /// Binding the same account + scope again replaces the prior entry
    /// rather than accumulating duplicates; it is still, by construction,
    /// bound to this verifier's one pinned owner.
    pub fn bind(
        &mut self,
        account: AccountRef,
        scope: BindingScope,
        evidence: BindingEvidence,
    ) -> Result<(), IdentityBindingError> {
        if evidence != BindingEvidence::CallerAttested {
            return Err(IdentityBindingError::EvidenceNotAttested);
        }
        self.bindings
            .retain(|existing| !(existing.account == account && existing.scope == scope));
        self.bindings.push(AccountBinding {
            account,
            scope,
            owner: self.owner,
        });
        Ok(())
    }

    /// Does an exact account + scope binding exist for the pinned owner?
    ///
    /// No partial match: a binding for a different account, a different
    /// scope, or (structurally impossible on one verifier, checked anyway)
    /// a different owner never satisfies this. No binding at all is
    /// refusal — the default outcome, not an edge case.
    pub fn verify(
        &self,
        account: &AccountRef,
        scope: BindingScope,
    ) -> Result<(), IdentityBindingError> {
        let bound = self
            .bindings
            .iter()
            .any(|existing| existing.verifies(self.owner, account, scope));
        if bound {
            Ok(())
        } else {
            Err(IdentityBindingError::NoBinding)
        }
    }
}

impl fmt::Debug for IdentityBindingVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IdentityBindingVerifier")
            .field("owner", &self.owner)
            .field("binding_count", &self.bindings.len())
            .finish()
    }
}

/// Errors from [`IdentityBindingVerifier`]. Deliberately data-free: no
/// variant carries an account identifier, owner fingerprint, or any other
/// value that would leak identity material through `Display`/`Debug`.
#[derive(Debug, Eq, PartialEq)]
pub enum IdentityBindingError {
    /// No binding exists for the exact account + scope requested, under
    /// the pinned owner. The default-refuse outcome: absence of a binding
    /// is refusal, never permission.
    NoBinding,
    /// [`IdentityBindingVerifier::bind`] was called with evidence weaker
    /// than [`BindingEvidence::CallerAttested`]. No binding was created;
    /// the caller must re-establish ownership through an attested path.
    EvidenceNotAttested,
}

impl fmt::Display for IdentityBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NoBinding => "no identity binding exists for the requested account and scope",
            Self::EvidenceNotAttested => "identity binding evidence is not caller-attested",
        })
    }
}

impl std::error::Error for IdentityBindingError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(seed: u8) -> PinnedOwner {
        let identity = keystore::identity_from_entropy([seed; 16], "u".into());
        PinnedOwner::from_identity(&identity)
    }

    fn account(id: &str) -> AccountRef {
        account_on("discord", id)
    }

    fn account_on(service_id: &str, account_id: &str) -> AccountRef {
        AccountRef {
            service_id: service_id.into(),
            account_id: account_id.into(),
        }
    }

    #[test]
    fn unbound_account_is_refused() {
        let verifier = IdentityBindingVerifier::new(owner(1));
        let result = verifier.verify(&account("account-a"), BindingScope::ScrubDeletion);
        assert_eq!(result, Err(IdentityBindingError::NoBinding));
    }

    #[test]
    fn binding_for_account_a_does_not_verify_account_b() {
        let mut verifier = IdentityBindingVerifier::new(owner(2));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-b"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding)
        );
    }

    #[test]
    fn binding_does_not_widen_to_a_different_scope() {
        let mut verifier = IdentityBindingVerifier::new(owner(3));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubIndex),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding),
            "an index-only binding must never also authorize deletion"
        );
    }

    #[test]
    fn deletion_binding_does_not_authorize_index_scope() {
        let mut verifier = IdentityBindingVerifier::new(owner(12));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubIndex),
            Err(IdentityBindingError::NoBinding),
            "a deletion binding is not a wildcard for every binding scope"
        );
    }

    #[test]
    fn binding_for_service_a_does_not_verify_service_b() {
        let mut verifier = IdentityBindingVerifier::new(owner(13));
        verifier
            .bind(
                account_on("discord", "account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(
                &account_on("telegram", "account-a"),
                BindingScope::ScrubDeletion
            ),
            Err(IdentityBindingError::NoBinding),
            "the service id is part of the account binding"
        );
    }

    #[test]
    fn account_ids_are_exact_not_prefix_matched() {
        let mut verifier = IdentityBindingVerifier::new(owner(14));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a-extra"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding)
        );
        assert_eq!(
            verifier.verify(&account("account-"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding)
        );
    }

    #[test]
    fn binding_from_renderer_supplied_evidence_is_refused() {
        let mut verifier = IdentityBindingVerifier::new(owner(4));
        let result = verifier.bind(
            account("account-a"),
            BindingScope::ScrubDeletion,
            BindingEvidence::RendererSupplied,
        );
        assert_eq!(result, Err(IdentityBindingError::EvidenceNotAttested));
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding),
            "a refused bind() must leave no trace to verify against"
        );
    }

    #[test]
    fn binding_from_unsigned_artifact_metadata_is_refused() {
        let mut verifier = IdentityBindingVerifier::new(owner(5));
        let result = verifier.bind(
            account("account-a"),
            BindingScope::ScrubDeletion,
            BindingEvidence::UnsignedMetadata,
        );
        assert_eq!(result, Err(IdentityBindingError::EvidenceNotAttested));
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding)
        );
    }

    #[test]
    fn binding_from_a_bare_verified_flag_is_refused() {
        let mut verifier = IdentityBindingVerifier::new(owner(6));
        let result = verifier.bind(
            account("account-a"),
            BindingScope::ScrubDeletion,
            BindingEvidence::UnverifiedFlag,
        );
        assert_eq!(result, Err(IdentityBindingError::EvidenceNotAttested));
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding)
        );
    }

    #[test]
    fn failed_rebind_does_not_remove_existing_attested_binding() {
        let mut verifier = IdentityBindingVerifier::new(owner(15));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        let result = verifier.bind(
            account("account-a"),
            BindingScope::ScrubDeletion,
            BindingEvidence::RendererSupplied,
        );

        assert_eq!(result, Err(IdentityBindingError::EvidenceNotAttested));
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Ok(()),
            "a refused replacement attempt must not erase prior caller-attested authority"
        );
    }

    #[test]
    fn failed_bind_after_index_binding_does_not_authorize_deletion() {
        let mut verifier = IdentityBindingVerifier::new(owner(16));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        let result = verifier.bind(
            account("account-a"),
            BindingScope::ScrubDeletion,
            BindingEvidence::UnsignedMetadata,
        );

        assert_eq!(result, Err(IdentityBindingError::EvidenceNotAttested));
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubIndex),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding),
            "failed destructive evidence must not be upgraded by an index-only binding"
        );
    }

    #[test]
    fn a_binding_never_crosses_to_a_different_pinned_owner() {
        let mut alice = IdentityBindingVerifier::new(owner(7));
        alice
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        let bob = IdentityBindingVerifier::new(owner(8));
        assert_eq!(
            bob.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding),
            "a's binding must not be visible from a verifier pinned to a different owner"
        );
    }

    #[test]
    fn verify_identity_binding_rejects_all_mutated_bindings() {
        let canonical = account_on("discord", "account-a");
        let mut verifier = IdentityBindingVerifier::new(owner(25));
        verifier
            .bind(
                canonical.clone(),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        assert_eq!(
            verifier.verify(&canonical, BindingScope::ScrubDeletion),
            Ok(())
        );

        let mut refusals = 0;
        let mut assert_refused = |result: Result<(), IdentityBindingError>| {
            refusals += 1;
            assert_eq!(result, Err(IdentityBindingError::NoBinding));
        };

        assert_refused(
            IdentityBindingVerifier::new(owner(25)).verify(&canonical, BindingScope::ScrubDeletion),
        );
        assert_refused(verifier.verify(
            &account_on("discord", "account-b"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("telegram", "account-a"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("telegram", "account-b"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(&account_on("", "account-a"), BindingScope::ScrubDeletion));
        assert_refused(verifier.verify(&account_on("discord", ""), BindingScope::ScrubDeletion));
        assert_refused(verifier.verify(
            &account_on("Discord", "account-a"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord ", "account-a"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", " account-a"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", "account-a "),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", "ACCOUNT-A"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", "account-a-extra"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", "account-"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", "account-a/child"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(
            &account_on("discord", "account_a"),
            BindingScope::ScrubDeletion,
        ));
        assert_refused(verifier.verify(&canonical, BindingScope::ScrubIndex));

        for evidence in [
            BindingEvidence::RendererSupplied,
            BindingEvidence::UnsignedMetadata,
            BindingEvidence::UnverifiedFlag,
        ] {
            let mut weak = IdentityBindingVerifier::new(owner(25));
            assert_eq!(
                weak.bind(canonical.clone(), BindingScope::ScrubDeletion, evidence),
                Err(IdentityBindingError::EvidenceNotAttested)
            );
            assert_refused(weak.verify(&canonical, BindingScope::ScrubDeletion));
        }

        let mut foreign_owner = IdentityBindingVerifier::new(owner(25));
        foreign_owner.bindings.push(AccountBinding {
            account: canonical.clone(),
            scope: BindingScope::ScrubDeletion,
            owner: owner(26),
        });
        assert_refused(foreign_owner.verify(&canonical, BindingScope::ScrubDeletion));

        let mut foreign_scope = IdentityBindingVerifier::new(owner(25));
        foreign_scope.bindings.push(AccountBinding {
            account: canonical.clone(),
            scope: BindingScope::ScrubIndex,
            owner: owner(25),
        });
        assert_refused(foreign_scope.verify(&canonical, BindingScope::ScrubDeletion));

        let mut foreign_service_binding = IdentityBindingVerifier::new(owner(25));
        foreign_service_binding.bindings.push(AccountBinding {
            account: account_on("telegram", "account-a"),
            scope: BindingScope::ScrubDeletion,
            owner: owner(25),
        });
        assert_refused(foreign_service_binding.verify(&canonical, BindingScope::ScrubDeletion));

        let mut foreign_account_binding = IdentityBindingVerifier::new(owner(25));
        foreign_account_binding.bindings.push(AccountBinding {
            account: account_on("discord", "account-b"),
            scope: BindingScope::ScrubDeletion,
            owner: owner(25),
        });
        assert_refused(foreign_account_binding.verify(&canonical, BindingScope::ScrubDeletion));

        let mut all_fields_mutated = IdentityBindingVerifier::new(owner(25));
        all_fields_mutated.bindings.push(AccountBinding {
            account: account_on("telegram", "account-b"),
            scope: BindingScope::ScrubIndex,
            owner: owner(26),
        });
        assert_refused(all_fields_mutated.verify(&canonical, BindingScope::ScrubDeletion));

        assert_eq!(refusals, 24);
    }

    #[test]
    fn f66_verify_identity_binding_rejects_foreign_owner_binding_on_same_verifier() {
        let mut verifier = IdentityBindingVerifier::new(owner(17));
        verifier.bindings.push(AccountBinding {
            account: account("account-a"),
            scope: BindingScope::ScrubDeletion,
            owner: owner(18),
        });

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Err(IdentityBindingError::NoBinding),
            "the owner check is part of the allow decision even for an in-memory binding"
        );
    }

    #[test]
    fn rebinding_the_same_account_and_scope_replaces_rather_than_duplicates() {
        let mut verifier = IdentityBindingVerifier::new(owner(9));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        assert_eq!(verifier.bindings.len(), 1);
    }

    #[test]
    fn multiple_accounts_can_be_bound_independently() {
        let mut verifier = IdentityBindingVerifier::new(owner(19));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        verifier
            .bind(
                account("account-b"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-b"), BindingScope::ScrubDeletion),
            Ok(())
        );
        assert_eq!(verifier.bindings.len(), 2);
    }

    #[test]
    fn different_scopes_for_the_same_account_can_coexist() {
        let mut verifier = IdentityBindingVerifier::new(owner(20));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubIndex),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Ok(())
        );
        assert_eq!(verifier.bindings.len(), 2);
    }

    #[test]
    fn rebinding_one_scope_does_not_remove_the_other_scope() {
        let mut verifier = IdentityBindingVerifier::new(owner(21));
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubIndex,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        verifier
            .bind(
                account("account-a"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();

        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubIndex),
            Ok(())
        );
        assert_eq!(
            verifier.verify(&account("account-a"), BindingScope::ScrubDeletion),
            Ok(())
        );
        assert_eq!(verifier.bindings.len(), 2);
    }

    #[test]
    fn debug_output_never_reveals_the_account_identifier() {
        let account = account("super-secret-account-id");
        let rendered = format!("{account:?}");
        assert!(!rendered.contains("super-secret-account-id"));
        assert!(!rendered.contains("discord"));
    }

    #[test]
    fn debug_output_never_reveals_the_owner_fingerprint() {
        let owner = owner(10);
        let rendered = format!("{owner:?}");
        assert_eq!(rendered, "PinnedOwner(<redacted>)");
    }

    #[test]
    fn verifier_debug_output_carries_no_account_data() {
        let mut verifier = IdentityBindingVerifier::new(owner(11));
        verifier
            .bind(
                account("super-secret-account-id"),
                BindingScope::ScrubDeletion,
                BindingEvidence::CallerAttested,
            )
            .unwrap();
        let rendered = format!("{verifier:?}");
        assert!(!rendered.contains("super-secret-account-id"));
        assert!(!rendered.contains("discord"));
    }

    #[test]
    fn same_identity_pins_the_same_owner_deterministically() {
        // Two verifiers pinned from the same underlying key material must
        // agree on bindings -- the fingerprint is a pure function of the
        // identity's public key, not of process-local state.
        let identity = keystore::identity_from_entropy([42; 16], "u".into());
        let a = IdentityBindingVerifier::new(PinnedOwner::from_identity(&identity));
        let mut b = IdentityBindingVerifier::new(PinnedOwner::from_identity(&identity));
        b.bind(
            account("account-a"),
            BindingScope::ScrubDeletion,
            BindingEvidence::CallerAttested,
        )
        .unwrap();
        assert_eq!(a.owner(), b.owner());
    }

    #[test]
    fn same_key_material_with_different_user_ids_pins_same_owner() {
        let a = keystore::identity_from_entropy([22; 16], "discord-snowflake-a".into());
        let b = keystore::identity_from_entropy([22; 16], "osl_legacy-routing-id".into());

        assert_eq!(
            PinnedOwner::from_identity(&a),
            PinnedOwner::from_identity(&b)
        );
    }

    #[test]
    fn different_key_material_with_same_user_id_pins_different_owner() {
        let a = keystore::identity_from_entropy([23; 16], "same-user-id".into());
        let b = keystore::identity_from_entropy([24; 16], "same-user-id".into());

        assert_ne!(
            PinnedOwner::from_identity(&a),
            PinnedOwner::from_identity(&b)
        );
    }

    #[test]
    fn error_display_and_debug_are_data_free() {
        for error in [
            IdentityBindingError::NoBinding,
            IdentityBindingError::EvidenceNotAttested,
        ] {
            let display = error.to_string();
            let debug = format!("{error:?}");

            assert!(!display.contains("super-secret-account-id"));
            assert!(!display.contains("discord"));
            assert!(!display.contains("osl_"));
            assert!(!debug.contains("super-secret-account-id"));
            assert!(!debug.contains("discord"));
            assert!(!debug.contains("osl_"));
        }
    }

    #[test]
    fn verify_identity_binding_rejects_all_mutated_bindings_second_path() {
        let target_owner = owner(90);
        let target_account = account_on("discord", "account-a");
        let target_scope = BindingScope::ScrubDeletion;

        let exact = AccountBinding {
            account: target_account.clone(),
            scope: target_scope,
            owner: target_owner,
        };
        assert!(exact.verifies(target_owner, &target_account, target_scope));

        let mut mutations: Vec<(&str, AccountBinding)> = Vec::new();
        for (label, foreign_owner) in [
            ("owner-1", owner(91)),
            ("owner-2", owner(92)),
            ("owner-3", owner(93)),
            ("owner-4", owner(94)),
            ("owner-5", owner(95)),
            ("owner-6", owner(96)),
        ] {
            mutations.push((
                label,
                AccountBinding {
                    account: target_account.clone(),
                    scope: target_scope,
                    owner: foreign_owner,
                },
            ));
        }
        for service_id in ["", "Discord", "discord ", "telegram", "x", "discord.com"] {
            mutations.push((
                service_id,
                AccountBinding {
                    account: account_on(service_id, "account-a"),
                    scope: target_scope,
                    owner: target_owner,
                },
            ));
        }
        for account_id in [
            "",
            "account",
            "account-a ",
            " account-a",
            "account-a-extra",
            "account-b",
        ] {
            mutations.push((
                account_id,
                AccountBinding {
                    account: account_on("discord", account_id),
                    scope: target_scope,
                    owner: target_owner,
                },
            ));
        }
        for label in [
            "scope-index-1",
            "scope-index-2",
            "scope-index-3",
            "scope-index-4",
            "scope-index-5",
            "scope-index-6",
        ] {
            mutations.push((
                label,
                AccountBinding {
                    account: target_account.clone(),
                    scope: BindingScope::ScrubIndex,
                    owner: target_owner,
                },
            ));
        }
        assert_eq!(mutations.len(), 24);

        for (label, binding) in mutations {
            let verifier = IdentityBindingVerifier {
                owner: target_owner,
                bindings: vec![binding],
            };
            assert_eq!(
                verifier.verify(&target_account, target_scope),
                Err(IdentityBindingError::NoBinding),
                "mutated binding must refuse: {label}"
            );
        }
    }
}
