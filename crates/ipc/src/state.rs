//! Shared application state held in Tauri's `State<>` slot.
//!
//! v1 alpha holds:
//! - The currently-loaded [`keystore::Identity`] (if any).
//! - The configured [`keystore::KeyServerClient`] (if any).
//!
//! Both live behind a `Mutex` to allow blocking access from sync
//! command handlers (the keystore HTTP client is sync; tauri command
//! handlers wrap it in `spawn_blocking`).
//!
//! v1 stable extends this with: ratchet state per peer, sender-keys
//! state per group, wrapped-key cache, manifest cache, etc.

use keystore::{Identity, KeyServerClient};
use std::sync::Mutex;

#[derive(Default)]
pub struct AppState {
    pub identity: Mutex<Option<Identity>>,
    pub keyserver: Mutex<Option<KeyServerClient>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState::default()
    }

    pub fn has_identity(&self) -> bool {
        self.identity.lock().expect("identity mutex poisoned").is_some()
    }

    pub fn has_keyserver(&self) -> bool {
        self.keyserver.lock().expect("keyserver mutex poisoned").is_some()
    }
}
