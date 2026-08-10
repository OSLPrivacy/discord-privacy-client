//! Policy for proving services accepted at the public-name proof boundary.
//!
//! Gate 3118 permits no outside proving services, so public names require no
//! outside account proofs.

use core::fmt;

const ALLOWED_PROVING_SERVICES: &[&str] = &[];
const REQUIRED_OUTSIDE_PROOF_COUNT: usize = 0;

/// Returns the proving services allowed for public-name account proofs.
pub fn allowed_proving_services() -> &'static [&'static str] {
    ALLOWED_PROVING_SERVICES
}

/// Returns the number of outside proofs required for a public name.
pub const fn required_outside_proof_count() -> usize {
    REQUIRED_OUTSIDE_PROOF_COUNT
}

/// Rejects a proving service that is not allowed for public-name proofs.
pub fn validate_proving_service(service: &str) -> Result<(), ProvingServiceError> {
    if allowed_proving_services().contains(&service) {
        Ok(())
    } else {
        Err(ProvingServiceError {
            service: service.to_owned(),
        })
    }
}

/// A proving service submitted where Gate 3118 permits none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvingServiceError {
    service: String,
}

impl ProvingServiceError {
    /// Returns the rejected proving service.
    pub fn service(&self) -> &str {
        &self.service
    }
}

impl fmt::Display for ProvingServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "proving service {:?} not allowed", self.service)
    }
}

impl std::error::Error for ProvingServiceError {}
