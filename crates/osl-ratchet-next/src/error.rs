//! Error type.
//!
//! Every fallible path in this crate returns [`Error`]. No function
//! in the library panics on attacker-controlled input: all wire
//! parsing goes through [`crate::codec::Reader`], which bounds-checks
//! every read, and every fixed-size copy is guarded by an explicit
//! length check first.
//!
//! ## Error granularity is deliberately coarse for AEAD failures
//!
//! `Error::AuthFailed` is returned for *every* authentication failure:
//! wrong header key, tampered header, tampered body, wrong message
//! key, and "this is not for me". Splitting those apart would hand a
//! network attacker an oracle that distinguishes "you guessed the
//! header key but not the body key" from "you guessed neither". The
//! variant carries no data.

use core::fmt;

/// Crate-wide result alias.
pub type Result<T> = core::result::Result<T, Error>;

/// All failures surfaced by this crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// AEAD authentication failed, or no candidate key opened the
    /// header. Deliberately indistinguishable across causes.
    AuthFailed,

    /// The wire blob did not start with the expected `DPC0::` prefix.
    BadPrefix,

    /// Base64 decoding failed.
    Base64,

    /// The version byte is not this protocol's.
    ///
    /// A version router uses this to fall through to the v=2..v=5
    /// decoders rather than treating the message as corrupt.
    WrongVersion { got: u8, expected: u8 },

    /// Ran out of bytes while parsing, or trailing bytes remained.
    Malformed(&'static str),

    /// A counter, epoch or length field exceeded a hard policy bound.
    /// Never derived from a secret; safe to report.
    PolicyBound(&'static str),

    /// The receiver would have had to derive more skipped message
    /// keys than policy allows to reach this message. The message is
    /// dropped; the session is *not* poisoned.
    SkipLimitExceeded { requested: u64, limit: u64 },

    /// The peer announced a PQ epoch mix we cannot perform because we
    /// do not hold that epoch's shared secret. Indicates a protocol
    /// violation or state corruption, not ordinary loss.
    MissingPqEpoch { epoch: u32 },

    /// A low-order / all-zero X25519 result was rejected
    /// (contributory-behaviour check, RFC 7748 §6.1).
    DegenerateDh,

    /// Serialized session state was produced by a different state
    /// format version, or is corrupt.
    BadStateFormat,

    /// Internal invariant violated. Reaching this from network input
    /// is a bug; it is returned rather than panicking so that a
    /// hostile peer can at worst kill one session, never the process.
    Internal(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AuthFailed => write!(f, "authentication failed"),
            Error::BadPrefix => write!(f, "missing DPC0:: prefix"),
            Error::Base64 => write!(f, "base64 decode failed"),
            Error::WrongVersion { got, expected } => {
                write!(f, "wire version 0x{got:02x} != 0x{expected:02x}")
            }
            Error::Malformed(what) => write!(f, "malformed wire: {what}"),
            Error::PolicyBound(what) => write!(f, "policy bound exceeded: {what}"),
            Error::SkipLimitExceeded { requested, limit } => write!(
                f,
                "skip of {requested} message keys exceeds limit {limit}"
            ),
            Error::MissingPqEpoch { epoch } => {
                write!(f, "peer announced PQ epoch {epoch} we do not hold")
            }
            Error::DegenerateDh => write!(f, "degenerate X25519 shared secret rejected"),
            Error::BadStateFormat => write!(f, "unsupported or corrupt session state"),
            Error::Internal(what) => write!(f, "internal invariant: {what}"),
        }
    }
}

impl std::error::Error for Error {}
