//! The three independently-observable effects of a burn request.
//!
//! Local destruction is a local safety action and must complete before any
//! network work is considered. Server deletion and the peer instruction are
//! durable work for reconnect; neither is evidence the other completed.

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedBurn {
    pub blob_id: String,
    pub manage_cap: [u8; 32],
    pub peer_instruction: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BurnDispatch {
    pub local_destroyed: bool,
    pub queued_server_delete: QueuedBurn,
    pub queued_peer_instruction: QueuedBurn,
}

/// Derive the server-delete capability from material still held locally.
///
/// Nothing accepted from a prior server response is retained here. A queued
/// retry can therefore be reconstructed after offline time without waiting on
/// a remembered server-minted token.
pub fn recompute_manage_cap(k_send: &[u8], blob_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"osl/manage-cap/v1");
    hasher.update((k_send.len() as u64).to_be_bytes());
    hasher.update(k_send);
    hasher.update(blob_id.as_bytes());
    hasher.finalize().into()
}

/// Destroy the local copy now, then construct the two independent queued
/// effects. `destroy_local` intentionally runs before any queue callback or
/// network transport can be introduced by a caller.
pub fn dispatch_burn<F>(k_send: &[u8], blob_id: String, peer_instruction: Vec<u8>, destroy_local: F) -> BurnDispatch
where
    F: FnOnce(),
{
    destroy_local();
    let queued = QueuedBurn {
        manage_cap: recompute_manage_cap(k_send, &blob_id),
        blob_id,
        peer_instruction,
    };
    BurnDispatch {
        local_destroyed: true,
        queued_server_delete: queued.clone(),
        queued_peer_instruction: queued,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t1_t63_burn_destroys_local_copy_while_network_is_down() {
        let mut local_destroyed = false;
        let dispatched = dispatch_burn(b"send key", "blob-1".into(), vec![7], || local_destroyed = true);

        assert!(local_destroyed, "local destruction must not wait for the network");
        assert!(dispatched.local_destroyed);
        assert_eq!(dispatched.queued_server_delete.blob_id, "blob-1");
        assert_eq!(dispatched.queued_peer_instruction.peer_instruction, vec![7]);
    }

    #[test]
    fn queued_burn_recomputes_its_capability_from_key_and_blob_not_a_token() {
        assert_eq!(
            recompute_manage_cap(b"send key", "blob-1"),
            recompute_manage_cap(b"send key", "blob-1")
        );
        assert_ne!(
            recompute_manage_cap(b"send key", "blob-1"),
            recompute_manage_cap(b"send key", "blob-2")
        );
    }
}
