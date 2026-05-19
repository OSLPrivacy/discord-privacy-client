//! W1: durable scope-membership accrual store.
//!
//! The server/channel whitelist model (Option B) resolves recipients
//! *dynamically* — "encrypt to all OSL members of this server/channel,
//! including future ones." Discord never hands us a full server roster
//! (the "channel roster not loaded" cause), so membership is accrued
//! over time from the gateway taps the client already has: whenever a
//! peer is observed in a channel/GC, we record `(scope → peer)` here.
//!
//! Best-effort by design (locked decision #3): a peer never observed
//! in any gateway event is simply not yet a known member; the moment
//! they're seen they're added, and the auto-recovery layer heals any
//! messages they couldn't read in the gap. This store is the membership
//! oracle for [`crate::whitelist`]'s precedence resolver and for
//! dynamic recipient enumeration.
//!
//! Keying mirrors [`crate::scope::Scope::storage_key`] plus a synthetic
//! `server:<id>` roll-up so a server-header whitelist (which spans every
//! channel) can enumerate the union of everyone seen anywhere in the
//! server:
//!
//! | key                                  | populated from                |
//! |--------------------------------------|-------------------------------|
//! | `server:<sid>`                       | any observation in server sid |
//! | `server_channel:<sid>:<cid>`         | observations in that channel  |
//! | `gc:<id>`                            | observations in that GC       |
//!
//! In-memory; mirrored to `membership.json`. A lost file just means
//! re-accrual on the next gateway events (safe — never grants access,
//! only ever narrows the known-member set).

use crate::scope::{Scope, ScopeKind};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Roll-up key for "seen anywhere in this server".
pub fn server_key(server_id: &str) -> String {
    format!("server:{server_id}")
}

/// Per-channel key. Matches `Scope::storage_key()` for a
/// `ServerChannel` scope (`server_channel:<server>:<channel>`).
pub fn channel_key(server_id: &str, channel_id: &str) -> String {
    format!("server_channel:{server_id}:{channel_id}")
}

/// GC key. Matches `Scope::storage_key()` for a `Gc` scope.
pub fn gc_key(gc_id: &str) -> String {
    format!("gc:{gc_id}")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeMembership {
    /// membership key → set of member Discord snowflakes.
    #[serde(default)]
    map: HashMap<String, HashSet<String>>,
}

impl ScopeMembership {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `discord_id` as observed in a server text channel. Adds
    /// both the per-channel key and the server roll-up so a later
    /// server-header whitelist sees them.
    pub fn note_server_channel_member(
        &mut self,
        server_id: &str,
        channel_id: &str,
        discord_id: &str,
    ) {
        self.map
            .entry(channel_key(server_id, channel_id))
            .or_default()
            .insert(discord_id.to_string());
        self.map
            .entry(server_key(server_id))
            .or_default()
            .insert(discord_id.to_string());
    }

    /// Record `discord_id` as observed in a GC.
    pub fn note_gc_member(&mut self, gc_id: &str, discord_id: &str) {
        self.map
            .entry(gc_key(gc_id))
            .or_default()
            .insert(discord_id.to_string());
    }

    /// Bulk form: note every id in `members` for a server channel.
    pub fn note_server_channel_members<I, S>(
        &mut self,
        server_id: &str,
        channel_id: &str,
        members: I,
    ) where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for m in members {
            self.note_server_channel_member(server_id, channel_id, m.as_ref());
        }
    }

    /// Bulk form: note every id in `members` for a GC.
    pub fn note_gc_members<I, S>(&mut self, gc_id: &str, members: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for m in members {
            self.note_gc_member(gc_id, m.as_ref());
        }
    }

    /// Is `discord_id` a known member anywhere in server `server_id`?
    pub fn is_server_member(&self, server_id: &str, discord_id: &str) -> bool {
        self.map
            .get(&server_key(server_id))
            .is_some_and(|s| s.contains(discord_id))
    }

    /// Is `discord_id` a known member of this exact server channel?
    pub fn is_channel_member(
        &self,
        server_id: &str,
        channel_id: &str,
        discord_id: &str,
    ) -> bool {
        self.map
            .get(&channel_key(server_id, channel_id))
            .is_some_and(|s| s.contains(discord_id))
    }

    /// Is `discord_id` a known member of GC `gc_id`?
    pub fn is_gc_member(&self, gc_id: &str, discord_id: &str) -> bool {
        self.map
            .get(&gc_key(gc_id))
            .is_some_and(|s| s.contains(discord_id))
    }

    /// Members observed for an arbitrary key (server / channel / gc).
    /// Empty slice semantics: returns an empty Vec for unknown keys.
    pub fn members_for_key(&self, key: &str) -> Vec<String> {
        self.map
            .get(key)
            .map(|s| {
                let mut v: Vec<String> = s.iter().cloned().collect();
                v.sort(); // deterministic order for callers/tests
                v
            })
            .unwrap_or_default()
    }

    /// Decision #2 gate: is `discord_id` a member of the *specific*
    /// scope? Used to member-gate the DM-bleed cross-grant so a
    /// DM-whitelisted peer only reads server/channel traffic in
    /// servers/channels they're actually in. DM scope: trivially true
    /// (the DM peer is the scope). ServerFull: server-level. Gc /
    /// ServerChannel: that exact GC / channel.
    pub fn is_member_of_scope(&self, scope: &Scope, discord_id: &str) -> bool {
        match scope.kind {
            ScopeKind::Dm => true,
            ScopeKind::Gc => self.is_gc_member(&scope.id, discord_id),
            ScopeKind::ServerChannel => match (&scope.server_id, &scope.channel_id) {
                (Some(s), Some(c)) => self.is_channel_member(s, c, discord_id),
                _ => false,
            },
            ScopeKind::ServerFull => match &scope.server_id {
                Some(s) => self.is_server_member(s, discord_id),
                None => false,
            },
        }
    }

    /// Candidate member set for dynamic recipient resolution of a
    /// `ServerChannel` scope. `server_header_on` selects the server
    /// roll-up (every OSL member of the server — header REPLACE
    /// semantics) vs the single channel's members.
    pub fn server_channel_candidates(
        &self,
        server_id: &str,
        channel_id: &str,
        server_header_on: bool,
    ) -> Vec<String> {
        if server_header_on {
            self.members_for_key(&server_key(server_id))
        } else {
            self.members_for_key(&channel_key(server_id, channel_id))
        }
    }
}

// ---- Persistence (membership.json) ----
//
// Mirrors the whitelist_state writer: atomic tempfile+rename, at-rest
// encryption via `maybe_encrypt` when a main password is set. A
// missing file is non-fatal (NotFound) — the store just starts empty
// and re-accrues from gateway events.

#[derive(Debug, thiserror::Error)]
pub enum ScopeMembershipError {
    #[error("membership.json not found at {0}")]
    NotFound(String),
    #[error("membership.json read failed at {path}: {source}")]
    ReadFailed {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("membership.json parse failed at {path}: {reason}")]
    ParseFailed { path: String, reason: String },
}

/// Load `ScopeMembership` from `path`. `NotFound` on a fresh install
/// is the common, non-fatal case.
pub fn load_scope_membership_from_path(
    path: &Path,
) -> Result<ScopeMembership, ScopeMembershipError> {
    let blob = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(ScopeMembershipError::NotFound(path.display().to_string()));
        }
        Err(source) => {
            return Err(ScopeMembershipError::ReadFailed {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let plain = crate::main_password::maybe_decrypt(&blob).map_err(|e| {
        ScopeMembershipError::ParseFailed {
            path: path.display().to_string(),
            reason: e,
        }
    })?;
    serde_json::from_slice(&plain).map_err(|e| ScopeMembershipError::ParseFailed {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

/// Serialize + atomically write `m` to `path` (tempfile + rename;
/// at-rest-encrypted when a main password key is in the slot).
pub fn write_scope_membership(path: &Path, m: &ScopeMembership) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(m).map_err(std::io::Error::other)?;
    let out_bytes = crate::main_password::maybe_encrypt(&body)
        .map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &out_bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::Scope;

    const A: &str = "111111111111111111";
    const B: &str = "222222222222222222";
    const C: &str = "333333333333333333";
    const SRV: &str = "900000000000000001";
    const CH1: &str = "900000000000000010";
    const CH2: &str = "900000000000000011";
    const GC: &str = "900000000000000099";

    #[test]
    fn channel_observation_rolls_up_to_server() {
        let mut m = ScopeMembership::new();
        m.note_server_channel_member(SRV, CH1, A);
        assert!(m.is_channel_member(SRV, CH1, A));
        assert!(m.is_server_member(SRV, A));
        // Not a member of a different channel in the same server…
        assert!(!m.is_channel_member(SRV, CH2, A));
        // …but the server roll-up still knows them.
        assert!(m.is_server_member(SRV, A));
    }

    #[test]
    fn server_header_candidates_union_all_channels() {
        let mut m = ScopeMembership::new();
        m.note_server_channel_member(SRV, CH1, A);
        m.note_server_channel_member(SRV, CH2, B);
        // header ON → server-wide union of every channel seen.
        let mut server_wide = m.server_channel_candidates(SRV, CH1, true);
        server_wide.sort();
        assert_eq!(server_wide, vec![A.to_string(), B.to_string()]);
        // header OFF, channel CH1 → only CH1's members.
        assert_eq!(
            m.server_channel_candidates(SRV, CH1, false),
            vec![A.to_string()]
        );
        assert_eq!(
            m.server_channel_candidates(SRV, CH2, false),
            vec![B.to_string()]
        );
    }

    #[test]
    fn gc_membership_isolated_from_servers() {
        let mut m = ScopeMembership::new();
        m.note_gc_member(GC, A);
        assert!(m.is_gc_member(GC, A));
        assert!(!m.is_server_member(SRV, A));
        assert!(!m.is_channel_member(SRV, CH1, A));
    }

    #[test]
    fn is_member_of_scope_maps_each_kind() {
        let mut m = ScopeMembership::new();
        m.note_server_channel_member(SRV, CH1, A);
        m.note_gc_member(GC, B);

        let sc = Scope::server_channel(SRV, CH1);
        assert!(m.is_member_of_scope(&sc, A));
        assert!(!m.is_member_of_scope(&sc, B));

        let gc = Scope::gc(GC);
        assert!(m.is_member_of_scope(&gc, B));
        assert!(!m.is_member_of_scope(&gc, A));

        // DM scope: trivially true (the peer IS the scope).
        let dm = Scope::dm(C);
        assert!(m.is_member_of_scope(&dm, C));
    }

    #[test]
    fn unknown_keys_are_empty_not_panicking() {
        let m = ScopeMembership::new();
        assert!(m.members_for_key("server:nope").is_empty());
        assert!(!m.is_server_member("nope", A));
        assert!(!m.is_channel_member("nope", "nada", A));
        assert!(!m.is_gc_member("nope", A));
    }

    #[test]
    fn serde_round_trip() {
        let mut m = ScopeMembership::new();
        m.note_server_channel_member(SRV, CH1, A);
        m.note_gc_member(GC, B);
        let json = serde_json::to_string(&m).unwrap();
        let back: ScopeMembership = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
        assert!(back.is_channel_member(SRV, CH1, A));
        assert!(back.is_gc_member(GC, B));
    }

    #[test]
    fn file_round_trip_and_missing_is_notfound() {
        let dir = std::env::temp_dir().join(format!(
            "osl_mem_test_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("membership.json");
        let _ = std::fs::remove_file(&path);
        // Missing → NotFound (non-fatal).
        assert!(matches!(
            load_scope_membership_from_path(&path),
            Err(ScopeMembershipError::NotFound(_))
        ));
        let mut m = ScopeMembership::new();
        m.note_server_channel_member(SRV, CH1, A);
        m.note_gc_member(GC, B);
        write_scope_membership(&path, &m).unwrap();
        let back = load_scope_membership_from_path(&path).unwrap();
        assert_eq!(m, back);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bulk_note_helpers() {
        let mut m = ScopeMembership::new();
        m.note_server_channel_members(SRV, CH1, [A, B, C]);
        m.note_gc_members(GC, vec![A.to_string(), B.to_string()]);
        assert!(m.is_channel_member(SRV, CH1, A));
        assert!(m.is_channel_member(SRV, CH1, C));
        assert!(m.is_server_member(SRV, B));
        assert!(m.is_gc_member(GC, A));
        assert!(!m.is_gc_member(GC, C));
    }
}
