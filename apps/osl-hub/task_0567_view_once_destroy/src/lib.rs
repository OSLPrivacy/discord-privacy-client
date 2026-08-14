//! Focused build surface for TASK 0567.
//!
//! This is the shipping hub implementation, included by path rather than
//! copied, so the test exercises the same `NamedViewOnceCopies` methods used by
//! `apps/osl-hub/src/lib.rs`.

#[path = "../../src/view_once_open.rs"]
pub mod view_once_open;
