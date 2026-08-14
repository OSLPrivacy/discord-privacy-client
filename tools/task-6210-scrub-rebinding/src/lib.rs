//! Isolated executable carrier for TASK 6210's production consent guard.
//!
//! The Hub currently has unrelated merge damage.  Including the shipping
//! module by path ensures this verifier compiles and executes the exact file
//! exposed by `apps/osl-hub/src/lib.rs`, without copying a test-only model.

#[path = "../../../apps/osl-hub/src/scrub_account_rebinding.rs"]
pub mod scrub_account_rebinding;
