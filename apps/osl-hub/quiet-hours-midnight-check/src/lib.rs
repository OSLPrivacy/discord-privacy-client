//! Deliberately empty.
//!
//! This package carries no code of its own. Everything it builds lives in
//! `tests/task_0866_quiet_hours_across_midnight.rs`, which pulls the real
//! quiet hours gate sources out of `apps/osl-hub/src/` with `#[path]`. See the
//! comment in `Cargo.toml` for why the check is parked here instead of in
//! `apps/osl-hub/tests/`.
