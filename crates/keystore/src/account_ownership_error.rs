//! Refusal vocabulary for platform-account ownership proofs (checklist A4).
//!
//! A4 — proven platform-account registration: nobody may pre-register
//! another owner's public service ID (e.g. claim someone else's Discord
//! account) against OSL. Before a registration is accepted, the caller
//! must present a proof that the presenting identity actually controls
//! the claimed platform account. [`AccountOwnershipError`] is the closed
//! set of reasons that proof can be refused.
//!
//! This module is error vocabulary only: no verification logic, no
//! network calls, no callers. The proof-checking code that will
//! eventually return these variants lives (or will live) alongside this
//! file.
//!
//! ## No identifiers in error output
//!
//! A prior unit found a real leak: a derived `Debug` impl on
//! `browser_footprint.rs` printed the account identifier it wrapped.
//! [`AccountOwnershipError`] closes that class of bug structurally, not
//! just by convention: **no variant carries an account identifier,
//! handle, or credential field**, so there is nothing for a derived (or
//! hand-written) `Display`/`Debug` impl to print, however it is called.
//! [`AccountOwnershipError::UnsupportedService`] carries a `&'static
//! str` service tag (e.g. `"discord"`), which names a supported-platform
//! kind, not an account — that is not identifying information and is
//! safe to surface in both impls.
//!
//! `Display` and `Debug` are both written by hand below rather than
//! derived, so that adding a field to a variant in the future cannot
//! silently start leaking it through an auto-generated impl; a new field
//! has to be deliberately threaded through the match arms here.

use std::fmt;

/// Why a platform-account ownership proof was refused.
///
/// Every variant is deliberately identifier-free; see the module docs
/// for why.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AccountOwnershipError {
    /// The registration request carried no ownership proof at all.
    NoProofPresented,

    /// A proof was presented, but it attests ownership of a platform
    /// account other than the one this registration is claiming.
    ProofForDifferentAccount,

    /// A proof was presented for the right platform account, but it was
    /// not signed by (or otherwise bound to) the identity presenting it
    /// — i.e. it proves someone owns the account, just not the caller.
    ProofForDifferentOwner,

    /// The proof was valid when issued but has aged past its allowed
    /// freshness window.
    ProofStale,

    /// The proof's nonce (or equivalent single-use token) has already
    /// been consumed by an earlier registration attempt.
    ProofReplayed,

    /// The proof could not be parsed / decoded into a recognizable
    /// shape.
    ProofMalformed,

    /// The claimed platform is not one OSL currently accepts ownership
    /// proofs for. Carries a static service tag (e.g. `"discord"`) —
    /// this names a platform *kind*, never an account.
    UnsupportedService { service: &'static str },
}

impl fmt::Display for AccountOwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProofPresented => {
                write!(f, "no platform-account ownership proof was presented")
            }
            Self::ProofForDifferentAccount => write!(
                f,
                "ownership proof does not attest the account being registered"
            ),
            Self::ProofForDifferentOwner => write!(
                f,
                "ownership proof is not bound to the identity presenting it"
            ),
            Self::ProofStale => write!(f, "ownership proof has expired"),
            Self::ProofReplayed => write!(f, "ownership proof has already been consumed"),
            Self::ProofMalformed => {
                write!(f, "ownership proof is malformed and could not be parsed")
            }
            Self::UnsupportedService { service } => write!(
                f,
                "ownership proofs are not supported for service kind '{service}'"
            ),
        }
    }
}

impl fmt::Debug for AccountOwnershipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Hand-written, not derived: every arm prints only the variant
        // name (plus, for `UnsupportedService`, the static service
        // kind). No arm can ever print an account identifier because no
        // arm holds one.
        match self {
            Self::NoProofPresented => f.write_str("AccountOwnershipError::NoProofPresented"),
            Self::ProofForDifferentAccount => {
                f.write_str("AccountOwnershipError::ProofForDifferentAccount")
            }
            Self::ProofForDifferentOwner => {
                f.write_str("AccountOwnershipError::ProofForDifferentOwner")
            }
            Self::ProofStale => f.write_str("AccountOwnershipError::ProofStale"),
            Self::ProofReplayed => f.write_str("AccountOwnershipError::ProofReplayed"),
            Self::ProofMalformed => f.write_str("AccountOwnershipError::ProofMalformed"),
            Self::UnsupportedService { service } => f
                .debug_struct("AccountOwnershipError::UnsupportedService")
                .field("service", service)
                .finish(),
        }
    }
}

impl std::error::Error for AccountOwnershipError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the kind of value that must never reach a variant's
    /// output. If any of these substrings ever showed up in a formatted
    /// error, that would be exactly the `browser_footprint.rs`-class leak
    /// this module is designed to make structurally impossible.
    const FORBIDDEN: &[&str] = &[
        "victim#1234",
        "discord_user_id_881122334455",
        "@handle_example",
        "secret_credential_token",
    ];

    fn assert_no_leak(rendered: &str) {
        for needle in FORBIDDEN {
            assert!(
                !rendered.contains(needle),
                "leaked identifier-shaped content {needle:?} in {rendered:?}"
            );
        }
    }

    #[test]
    fn no_proof_presented_redacts_identifiers() {
        let err = AccountOwnershipError::NoProofPresented;
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
        assert_eq!(
            err.to_string(),
            "no platform-account ownership proof was presented"
        );
        assert_eq!(
            format!("{err:?}"),
            "AccountOwnershipError::NoProofPresented"
        );
    }

    #[test]
    fn proof_for_different_account_redacts_identifiers() {
        let err = AccountOwnershipError::ProofForDifferentAccount;
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
        assert_eq!(
            format!("{err:?}"),
            "AccountOwnershipError::ProofForDifferentAccount"
        );
    }

    #[test]
    fn proof_for_different_owner_redacts_identifiers() {
        let err = AccountOwnershipError::ProofForDifferentOwner;
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
        assert_eq!(
            format!("{err:?}"),
            "AccountOwnershipError::ProofForDifferentOwner"
        );
    }

    #[test]
    fn proof_stale_redacts_identifiers() {
        let err = AccountOwnershipError::ProofStale;
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
        assert_eq!(err.to_string(), "ownership proof has expired");
    }

    #[test]
    fn proof_replayed_redacts_identifiers() {
        let err = AccountOwnershipError::ProofReplayed;
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
        assert_eq!(err.to_string(), "ownership proof has already been consumed");
    }

    #[test]
    fn proof_malformed_redacts_identifiers() {
        let err = AccountOwnershipError::ProofMalformed;
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
    }

    #[test]
    fn unsupported_service_redacts_identifiers_but_names_the_service_kind() {
        let err = AccountOwnershipError::UnsupportedService { service: "discord" };
        // The service *kind* is expected and fine to see...
        assert!(err.to_string().contains("discord"));
        assert!(format!("{err:?}").contains("discord"));
        // ...but nothing identifier-, handle-, or credential-shaped ever is.
        assert_no_leak(&err.to_string());
        assert_no_leak(&format!("{err:?}"));
    }

    #[test]
    fn all_variants_implement_std_error() {
        fn assert_error<E: std::error::Error>(_: &E) {}
        assert_error(&AccountOwnershipError::NoProofPresented);
    }

    #[test]
    fn variants_are_copy_and_comparable() {
        // Deliberately cheap: nothing heap-allocated or identifier-bearing
        // needs cloning, which is itself part of the redaction guarantee.
        let a = AccountOwnershipError::ProofStale;
        let b = a;
        assert!(a == b);
    }
}
