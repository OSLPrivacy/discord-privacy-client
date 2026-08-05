//! The process-wide `tracing` subscriber for the desktop app (D-191).
//!
//! # Why this module exists
//!
//! The desktop binary registered **no `tracing` subscriber at all**, so every
//! `tracing::warn!`/`tracing::error!` in the workspace was dispatched to a
//! `NoSubscriber` and dropped on the floor. That is a defect in its own right,
//! not missing scaffolding — three separate defects were prolonged or hidden by
//! it:
//!
//! * **D-142** — the one line naming *which* state loader refused
//!   (`crates/ipc/src/commands.rs:14761`) was emitted to nothing, and the
//!   defect was misdiagnosed twice before the real cause was found.
//! * **D-187** — the lost-key sentence is routed into a `tracing::error!`
//!   nobody subscribes to (`crates/ipc/src/commands.rs:14774`), so the user
//!   sees a hard-coded generic and the machine keeps no record of the cause.
//! * the Windows reopen lane spent hours on a send failure it describes as
//!   *"a one-line diagnosis with logs"*.
//!
//! `RUST_LOG` did nothing, because nothing was listening.
//!
//! # Why it must be a file, and why it must be in the release build
//!
//! `apps/osl-hub/src/main.rs:1` is `windows_subsystem = "windows"`, so a
//! release build has **no console**: `eprintln!` and a stderr layer reach
//! nobody on the machine that matters. The failures worth diagnosing happen on
//! the shipping build on someone else's machine, so this is wired into the
//! `core` feature — which is in `default` and pulled in by `desktop` — and is
//! **not** gated behind `debug_assertions` or any QA feature.
//!
//! Two sinks, one shared env filter:
//!
//! * **stderr** — for `cargo tauri dev`, the e2e harness (whose launcher
//!   redirects the process's stderr into `.profiles/<name>/logs/backend.log`)
//!   and anyone who starts the app from a terminal.
//! * **a capped file** at `std::env::temp_dir()/osl-diagnostics.log` — the
//!   "somewhere on the machine" half. Same location convention as
//!   `startup_breadcrumb`'s trace (`main.rs`), i.e. deliberately **outside**
//!   the encrypted profile directory, so a diagnostic write can never touch
//!   at-rest state. It is a second file rather than an extension of the
//!   breadcrumb trail on purpose: the breadcrumb is a fixed, greppable,
//!   hand-placed launch trace with its own `<elapsed_ms> <label>` format and a
//!   documented intent to be deleted, while this is the open-ended event
//!   stream. Merging them would make the breadcrumb unreadable and would put
//!   the trace it exists to preserve behind this file's rotation.
//!
//! # Level
//!
//! **Default is `warn`.** That is the whole default diagnostic surface and no
//! more: `WARN`/`ERROR` in this workspace are refusals and self-heals — the
//! sealed identity would not reopen, a state file was quarantined, the message
//! store did not come back — and they name files and salted `log_id` tokens,
//! never message content. `INFO`/`DEBUG` carry peer counts, scope tokens and
//! per-send activity, so they stay off unless the operator asks for them by
//! hand with `OSL_LOG` (preferred) or `RUST_LOG`, e.g. `OSL_LOG=ipc=debug`.
//! This is a privacy product; verbose logging is an explicit per-launch choice.
//!
//! # Secrets
//!
//! Nothing here filters or redacts. Redaction that lives in the subscriber is a
//! promise the next call site has no way to keep, so the rule is enforced at
//! the call sites instead: identifiers go through
//! `ipc::log_id::log_id`, filesystem paths through
//! `ipc::log_id::redact_path`, and no call site in the process passes a
//! password, recovery phrase, key material or message plaintext to a `tracing`
//! macro. See `tasklogs/D-191.md` for the audit that established that.
//!
//! # Bound
//!
//! The file is capped and rotated (see [`MAX_DIAGNOSTIC_BYTES`]); at most one
//! previous generation is kept, so the pair can never exceed twice the cap.
//! This project has already had disk exhaustion take the machine down, and an
//! unbounded log on a friend's machine is a defect.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

/// Hard cap on one on-disk diagnostic generation. Past the cap the current
/// file becomes `osl-diagnostics.1.log` (replacing any previous one) and a
/// fresh file starts, so the diagnostic occupies at most `2 *
/// MAX_DIAGNOSTIC_BYTES` on disk no matter how long the process runs or how
/// tightly something loops.
pub const MAX_DIAGNOSTIC_BYTES: u64 = 4 * 1024 * 1024;

/// Where the diagnostic lands: `<temp dir>/osl-diagnostics.log`.
///
/// On Windows that is `%LOCALAPPDATA%\Temp`, which is what a friend can be
/// pointed at over a chat message; on Linux/CI it follows `TMPDIR`.
pub fn diagnostic_log_path() -> PathBuf {
    std::env::temp_dir().join("osl-diagnostics.log")
}

/// The retired generation that [`MAX_DIAGNOSTIC_BYTES`] rotation produces.
fn rotated_path(path: &Path) -> PathBuf {
    let mut rotated = path.as_os_str().to_owned();
    rotated.push(".1");
    PathBuf::from(rotated)
}

/// Roll the log over if this generation has reached the cap.
///
/// Checked on every open rather than once per process: a process that wedges
/// in a retry loop is exactly the shape this exists to diagnose, and a cap
/// that is only evaluated at startup does not bound *that* process at all — it
/// bounds the next one.
fn rotate_if_over_cap(path: &Path) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_DIAGNOSTIC_BYTES) {
        let _ = std::fs::rename(path, rotated_path(path));
    }
}

/// Opened per event rather than held: `WARN`/`ERROR` are rare by construction,
/// and a handle that is opened, written, flushed and dropped survives a
/// process that wedges or is killed immediately afterwards — which is exactly
/// the failure shape this exists to diagnose.
fn writer_for(path: PathBuf) -> Box<dyn std::io::Write + Send> {
    rotate_if_over_cap(&path);
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(file) => Box::new(file),
        // A diagnostic sink must never be able to take the app down, so an
        // unwritable temp dir degrades to stderr-only.
        Err(_) => Box::new(std::io::sink()),
    }
}

/// Install the process-wide subscriber against the default path. Call this
/// before anything that can fail.
pub fn init_diagnostic_subscriber() {
    init_diagnostic_subscriber_at(diagnostic_log_path());
}

/// Install the process-wide subscriber against an explicit path.
///
/// Separate from [`init_diagnostic_subscriber`] so a test can point the file
/// somewhere hermetic and then *read it back* — proving the wiring by the
/// bytes that land on disk rather than by the fact that a builder ran.
pub fn init_diagnostic_subscriber_at(path: PathBuf) {
    let filter = tracing_subscriber::EnvFilter::try_from_env("OSL_LOG")
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));

    let for_writer = path.clone();
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(move || writer_for(for_writer.clone()));
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(true)
        .with_writer(std::io::stderr);

    // `try_init`, not `init`: a second call (a test harness, a re-entrant
    // shell) must not panic the app over logging.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .try_init();

    rotate_if_over_cap(&path);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "--- OSL diagnostics: pid {} started; set OSL_LOG to raise the level ---",
            std::process::id()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_keeps_exactly_one_previous_generation() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("osl-diagnostics.log");

        std::fs::write(&path, vec![b'a'; (MAX_DIAGNOSTIC_BYTES + 1) as usize]).expect("seed");
        rotate_if_over_cap(&path);
        assert!(!path.exists(), "over-cap generation must be rotated away");
        assert!(rotated_path(&path).exists(), "previous generation is kept");

        std::fs::write(&path, vec![b'b'; (MAX_DIAGNOSTIC_BYTES + 1) as usize]).expect("seed 2");
        rotate_if_over_cap(&path);
        let rotated = std::fs::read(rotated_path(&path)).expect("read rotated");
        assert_eq!(rotated[0], b'b', "rotation replaces the older generation");
        assert!(
            !rotated_path(&rotated_path(&path)).exists(),
            "no third generation accumulates"
        );
    }

    #[test]
    fn writer_rotates_before_appending_so_one_process_stays_bounded() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("osl-diagnostics.log");
        std::fs::write(&path, vec![b'a'; (MAX_DIAGNOSTIC_BYTES + 1) as usize]).expect("seed");

        let mut writer = writer_for(path.clone());
        writer.write_all(b"after rotation\n").expect("write");
        drop(writer);

        let live = std::fs::metadata(&path).expect("live file").len();
        assert!(
            live < MAX_DIAGNOSTIC_BYTES,
            "the live generation restarted from empty, was {live}"
        );
    }
}
