//! Isolated worker credential lifetime guard.
//!
//! The guard has one job: credentials leased to an isolated worker are revoked
//! after the worker exits, regardless of whether the worker reports success,
//! reports failure, or unwinds.

use std::fmt;
use std::panic::{self, AssertUnwindSafe};

pub trait RevocableWorkerCredentials {
    fn revoke(&mut self) -> Result<(), CredentialRevocationError>;
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum CredentialRevocationError {
    Refused,
}

impl fmt::Debug for CredentialRevocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialRevocationError::Refused")
    }
}

impl fmt::Display for CredentialRevocationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("credential revocation failed")
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum IsolatedWorkerError {
    WorkerFailed,
    WorkerPanicked,
    CredentialRevocationFailed,
}

impl fmt::Debug for IsolatedWorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WorkerFailed => "IsolatedWorkerError::WorkerFailed",
            Self::WorkerPanicked => "IsolatedWorkerError::WorkerPanicked",
            Self::CredentialRevocationFailed => "IsolatedWorkerError::CredentialRevocationFailed",
        })
    }
}

impl fmt::Display for IsolatedWorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WorkerFailed => "isolated worker failed",
            Self::WorkerPanicked => "isolated worker panicked",
            Self::CredentialRevocationFailed => "isolated worker credential revocation failed",
        })
    }
}

impl std::error::Error for IsolatedWorkerError {}

pub fn run_isolated_worker<C, F, T>(
    credentials: &mut C,
    worker: F,
) -> Result<T, IsolatedWorkerError>
where
    C: RevocableWorkerCredentials,
    F: FnOnce(&mut C) -> Result<T, ()>,
{
    let worker_result = panic::catch_unwind(AssertUnwindSafe(|| worker(credentials)));
    let revoke_result = credentials.revoke();
    if revoke_result.is_err() {
        return Err(IsolatedWorkerError::CredentialRevocationFailed);
    }
    match worker_result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(())) => Err(IsolatedWorkerError::WorkerFailed),
        Err(_) => Err(IsolatedWorkerError::WorkerPanicked),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct ProbeCredentials {
        revoked: bool,
        revoke_count: usize,
    }

    impl RevocableWorkerCredentials for ProbeCredentials {
        fn revoke(&mut self) -> Result<(), CredentialRevocationError> {
            self.revoked = true;
            self.revoke_count += 1;
            Ok(())
        }
    }

    #[test]
    fn isolated_worker_revokes_credentials_on_completion_or_failure() {
        let mut success = ProbeCredentials::default();
        let value = run_isolated_worker(&mut success, |credentials| {
            assert!(!credentials.revoked);
            Ok(42)
        });
        assert_eq!(value, Ok(42));
        assert!(success.revoked);
        assert_eq!(success.revoke_count, 1);

        let mut failure = ProbeCredentials::default();
        let result: Result<(), IsolatedWorkerError> =
            run_isolated_worker(&mut failure, |credentials| {
                assert!(!credentials.revoked);
                Err(())
            });
        assert_eq!(result, Err(IsolatedWorkerError::WorkerFailed));
        assert!(failure.revoked);
        assert_eq!(failure.revoke_count, 1);

        let mut panic_case = ProbeCredentials::default();
        let result = run_isolated_worker::<_, _, ()>(&mut panic_case, |_| {
            panic!("worker stopped after taking its lease")
        });
        assert_eq!(result, Err(IsolatedWorkerError::WorkerPanicked));
        assert!(panic_case.revoked);
        assert_eq!(panic_case.revoke_count, 1);
    }
}
