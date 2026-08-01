//! Local-only policy and category mapping for the sensitive-content warning.
//!
//! This crate deliberately does not inspect text. Detection remains in the
//! shipping app's `privacy_scan` module; this crate only maps its findings to
//! the warning's stable, user-facing categories.

pub mod categories;
pub mod policy;
