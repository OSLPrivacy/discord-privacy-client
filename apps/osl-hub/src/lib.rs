// Module declarations are split by product area. Add a new declaration to the
// area file that owns its boundary; crate-root paths remain unchanged because
// `include!` expands each declaration here.
include!("module_declarations_accounts_and_attachments.rs");
include!("module_declarations_automation_and_burn.rs");
include!("module_declarations_carrier_cloud_and_control.rs");
include!("module_declarations_messaging_and_hosted.rs");
include!("module_declarations_native_and_osl.rs");
include!("module_declarations_protected_and_scrub.rs");
include!("module_declarations_services_and_surfaces.rs");
include!("module_declarations_runtime_and_identity.rs");
include!("module_declarations_lifecycle_and_security.rs");
include!("module_declarations_bootstrap.rs");

// Native executable verification is exercised only by Windows callers. Keep
// its fail-closed types available to cross-platform manifests and tests.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) mod windows_executable_trust;

// This private implementation module is genuine crate-root plumbing.
mod atomic_file;

// The keystore/ipc crates deliberately keep the active-account dir, base-dir
// override, and unlocked main-password key in process-wide statics (see
// crates/keystore/src/recipients.rs and crates/ipc/src/main_password.rs) —
// correct for a single-instance desktop app, but `cargo test` runs many test
// functions concurrently on separate OS threads inside one process. Any test
// (in any module) that mutates those globals must hold this lock for its
// entire critical section, or it can race a sibling test running in another
// module and silently read back the wrong key or directory. Deliberately
// crate-wide: a lock private to one module is no protection at all when a
// sibling module's test mutates the same global concurrently.
#[cfg(test)]
pub(crate) static GLOBAL_KEYSTORE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Take the crate-wide keystore/globals lock, recovering from poisoning.
///
/// Every caller used to `.lock().unwrap()`, which meant one genuinely failing
/// test poisoned the mutex and every later test that touched these globals
/// died with `PoisonError` instead of running. On Windows CI that turned a
/// single real assertion failure in `services` into 11 reported failures and
/// buried the one that mattered. Poisoning tells us nothing useful here: the
/// lock guards process globals that each test sets up for itself, not an
/// invariant that a panic could leave half-applied.
#[cfg(test)]
pub(crate) fn global_keystore_test_lock() -> std::sync::MutexGuard<'static, ()> {
    GLOBAL_KEYSTORE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
