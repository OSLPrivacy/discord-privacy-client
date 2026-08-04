//! The at-rest key the UI uses for OSL Chat's *local* state.
//!
//! OSL Chat keeps four pieces of correspondent-shaped state on the UI side —
//! muted people, per-person unread counts, message-preview visibility and the
//! encrypted-message notification list. Those live in the webview's
//! `localStorage`, which on Linux is
//! `~/.local/share/org.oslprivacy.hub/localstorage/tauri_localhost_0.localstorage`
//! and on Windows is the WebView2 profile — a plain file either way. The UI
//! already has an AEAD store for them (`secure-local-store.ts`), and until now
//! it had no key, so nothing ever constructed it (D-108).
//!
//! Key authority is resolved exactly the way `ipc::peer_map::peer_map_write_key`
//! resolves it, because the same rule has to hold: the main-password-derived
//! file storage key when the session is unlocked, otherwise the device-bound
//! fallback key — and only when no main-password marker exists, which
//! `ensure_device_bound_fallback_file_storage_key` enforces itself. A locked,
//! password-gated profile therefore gets no key and the UI store stays absent
//! rather than degrading to plaintext.
//!
//! The file storage key itself is never handed to the webview. What crosses the
//! IPC boundary is an HKDF-SHA256 subkey under a distinct info label, so a
//! compromise of the renderer cannot open `peer_map.json`, `hub_people.json`,
//! or `messages.sqlite`.

use std::path::Path;
use zeroize::Zeroizing;

/// HKDF info label separating the UI's local-state key from every other
/// consumer of the file storage key. Changing it orphans existing encrypted
/// local state, so it is versioned.
pub const HKDF_INFO_OSL_CHAT_LOCAL_STATE: &[u8] = b"osl/ui/osl-chat-local-state/v1";

/// Derive the renderer's OSL Chat local-state key from an at-rest root key.
///
/// Pure; separated from [`osl_chat_local_state_key`] so it is provable without
/// touching the process-global storage-key slot.
pub fn derive_osl_chat_local_state_key(root: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, String> {
    crypto::hkdf::derive_32(&[], root, HKDF_INFO_OSL_CHAT_LOCAL_STATE)
        .map(Zeroizing::new)
        .map_err(|error| format!("OSL: derive OSL Chat local-state key: {error}"))
}

/// Resolve the storage-key authority for `dir` and derive the renderer subkey.
///
/// `Err` when the session is locked behind a main password, which is the
/// intended fail-closed branch: the UI leaves its secure store unconfigured and
/// persists nothing rather than writing correspondent ids in the clear.
pub fn osl_chat_local_state_key(dir: &Path) -> Result<Zeroizing<[u8; 32]>, String> {
    let root = match ipc::main_password::get_file_storage_key() {
        Some(key) => Zeroizing::new(key),
        None => Zeroizing::new(
            ipc::main_password::ensure_device_bound_fallback_file_storage_key(dir).map_err(
                |error| {
                    format!("OSL: no storage-key authority for OSL Chat local state: {error}")
                },
            )?,
        ),
    };
    derive_osl_chat_local_state_key(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_key_is_deterministic_and_not_the_root_key() {
        let root = [0x5au8; 32];
        let first = derive_osl_chat_local_state_key(&root).expect("derive");
        let second = derive_osl_chat_local_state_key(&root).expect("derive");
        assert_eq!(*first, *second);
        assert_ne!(*first, root, "the renderer must not receive the root key");
    }

    #[test]
    fn derived_key_is_domain_separated_from_other_labels() {
        let root = [0x11u8; 32];
        let ours = derive_osl_chat_local_state_key(&root).expect("derive");
        let other = crypto::hkdf::derive_32(&[], &root, b"osl/ui/something-else/v1").expect("hkdf");
        assert_ne!(*ours, other);
    }

    #[test]
    fn distinct_roots_give_distinct_keys() {
        let a = derive_osl_chat_local_state_key(&[1u8; 32]).expect("derive");
        let b = derive_osl_chat_local_state_key(&[2u8; 32]).expect("derive");
        assert_ne!(*a, *b);
    }
}
