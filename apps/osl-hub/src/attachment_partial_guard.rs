//! Fail-closed ownership of an inbound attachment's partial staging file.
//!
//! A download is never a renderable or persistent attachment while this guard
//! owns it. Every terminal path drops the guard and removes the staging file;
//! only authenticated decryption may borrow the path before it is discarded.

use std::path::{Path, PathBuf};

/// Owns a partial sealed download until the receive path reaches a terminal
/// outcome. Cleanup is deliberately idempotent because explicit failure
/// handling and `Drop` are independent backstops.
pub struct AttachmentPartialGuard<'a> {
    root: &'a Path,
    path: Option<PathBuf>,
    remove: fn(&Path, &Path) -> Result<(), String>,
}

impl<'a> AttachmentPartialGuard<'a> {
    pub fn new(
        root: &'a Path,
        path: PathBuf,
        remove: fn(&Path, &Path) -> Result<(), String>,
    ) -> Self {
        Self {
            root,
            path: Some(path),
            remove,
        }
    }

    /// The sealed bytes may be authenticated and decrypted, but must never be
    /// handed to a renderer directly.
    pub fn path(&self) -> &Path {
        self.path
            .as_deref()
            .expect("partial attachment guard must own a path until discarded")
    }

    /// Remove the partial now, preserving a cleanup failure for the caller.
    pub fn discard(&mut self) -> Result<(), String> {
        let path = self
            .path
            .as_deref()
            .expect("partial attachment guard must own a path until discarded");
        (self.remove)(self.root, path)?;
        self.path.take();
        Ok(())
    }
}

impl Drop for AttachmentPartialGuard<'_> {
    fn drop(&mut self) {
        // Terminal cleanup must survive new `?` returns added to the receive
        // path. Explicit cleanup still reports failures where that is useful.
        if let Some(path) = self.path.take() {
            let _ = (self.remove)(self.root, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

    static REMOVALS: AtomicUsize = AtomicUsize::new(0);
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn remove_for_test(_root: &Path, _path: &Path) -> Result<(), String> {
        REMOVALS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    #[test]
    fn burn_mid_download_discards_partial_before_any_render_can_start() {
        let _lock = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        REMOVALS.store(0, Ordering::SeqCst);
        let root = Path::new("/trusted-staging-root");
        let partial = AttachmentPartialGuard::new(
            root,
            root.join("download-partial.oslatt"),
            remove_for_test,
        );

        // A terminal burn drops the guard instead of exposing its path to a
        // viewer or persistence layer.
        drop(partial);

        assert_eq!(REMOVALS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn explicit_discard_removes_the_partial() {
        let _lock = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        REMOVALS.store(0, Ordering::SeqCst);
        let root = Path::new("/trusted-staging-root");
        let partial = AttachmentPartialGuard::new(
            root,
            root.join("download-partial.oslatt"),
            remove_for_test,
        );

        let mut partial = partial;
        partial.discard().unwrap();

        assert_eq!(REMOVALS.load(Ordering::SeqCst), 1);
    }
}
