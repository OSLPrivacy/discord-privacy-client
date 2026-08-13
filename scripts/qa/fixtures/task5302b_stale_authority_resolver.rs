//! Deliberately permissive fixture-only resolver for TASK 5302b.
//!
//! The break harness copies this unreferenced module next to the shipping
//! source. If merely adding a fixture helper changes the shipping executable,
//! the negative control fails. Production accepts actions only through
//! `ShippingTransitionGuard::submit`; this file is never compiled or called.

#[allow(dead_code)]
fn fixture_accepts_every_stale_authority() -> bool {
    true
}
