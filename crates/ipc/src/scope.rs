//! Scope identifiers for Phase 7 whitelist + burn semantics.
//!
//! Spec: `docs/phase-7-design.md` §2.1.
//!
//! ## Scope model
//!
//! Every whitelist / burn / encryption-toggle decision is keyed
//! by a *scope*. The scope hierarchy is:
//!
//! | Kind            | `id`                | Means                                 |
//! |-----------------|---------------------|---------------------------------------|
//! | `Dm`            | peer's discord_id    | One-to-one DM with that peer          |
//! | `Gc`            | GC channel id        | Group chat (multi-recipient DM)       |
//! | `ServerChannel` | `<server>:<channel>` | One channel inside a server           |
//! | `ServerFull`    | server_id            | Every channel in a server             |
//!
//! ## Storage-key wire shape
//!
//! Used as the key in `whitelist_state.json` and
//! `peer_map.incoming_decrypt_accepted`:
//!
//! ```text
//! "dm:<discord_id>"
//! "gc:<gc_id>"
//! "server_channel:<server_id>:<channel_id>"
//! "server_full:<server_id>"
//! ```
//!
//! `parse` is the inverse of `storage_key`; both are stable and
//! must not change without bumping the file-schema version.
//!
//! ## `ScopeInput` (DTO)
//!
//! Tauri commands accept [`ScopeInput`] (camelCase-friendly JS
//! shape) and convert to [`Scope`] via `TryFrom`. The DTO is
//! permissive about which `server_id` / `channel_id` fields are
//! populated; conversion validates that the required fields for
//! the named kind are present.

use serde::{Deserialize, Serialize};

/// The four scope kinds. Tagged as snake_case strings in the
/// `ScopeInput` JSON shape so the JS side stays readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Dm,
    Gc,
    ServerChannel,
    ServerFull,
}

/// Validated scope used by all whitelist + burn logic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    pub kind: ScopeKind,
    /// Canonical id for this scope. Shape depends on `kind` — see
    /// the table in the module docs. For `ServerChannel` this is
    /// the combined `"<server_id>:<channel_id>"` form so
    /// `storage_key` is a clean append.
    pub id: String,
    /// Populated for `ServerChannel` and `ServerFull`.
    pub server_id: Option<String>,
    /// Populated for `Dm`, `Gc`, `ServerChannel`.
    pub channel_id: Option<String>,
}

impl Scope {
    /// Build a DM scope from a peer discord id.
    pub fn dm(peer_discord_id: impl Into<String>) -> Self {
        let id: String = peer_discord_id.into();
        Scope {
            kind: ScopeKind::Dm,
            channel_id: Some(id.clone()),
            id,
            server_id: None,
        }
    }

    /// Build a GC scope from a GC channel id.
    pub fn gc(gc_channel_id: impl Into<String>) -> Self {
        let id: String = gc_channel_id.into();
        Scope {
            kind: ScopeKind::Gc,
            channel_id: Some(id.clone()),
            id,
            server_id: None,
        }
    }

    /// Build a server-channel scope.
    pub fn server_channel(server_id: impl Into<String>, channel_id: impl Into<String>) -> Self {
        let server_id: String = server_id.into();
        let channel_id: String = channel_id.into();
        Scope {
            kind: ScopeKind::ServerChannel,
            id: format!("{server_id}:{channel_id}"),
            server_id: Some(server_id),
            channel_id: Some(channel_id),
        }
    }

    /// Build an entire-server scope.
    pub fn server_full(server_id: impl Into<String>) -> Self {
        let server_id: String = server_id.into();
        Scope {
            kind: ScopeKind::ServerFull,
            id: server_id.clone(),
            server_id: Some(server_id),
            channel_id: None,
        }
    }

    /// Canonical storage_key for `whitelist_state.json` and the
    /// `incoming_decrypt_accepted` map on `PeerEntry`. Stable
    /// wire shape: don't change without schema bump.
    pub fn storage_key(&self) -> String {
        match self.kind {
            ScopeKind::Dm => format!("dm:{}", self.id),
            ScopeKind::Gc => format!("gc:{}", self.id),
            ScopeKind::ServerChannel => format!("server_channel:{}", self.id),
            ScopeKind::ServerFull => format!("server_full:{}", self.id),
        }
    }

    /// Inverse of [`storage_key`]. Returns `None` for malformed
    /// input rather than erroring — callers (e.g. config loaders
    /// recovering from a hand-edit) want to skip and log rather
    /// than abort the whole load.
    pub fn parse(key: &str) -> Option<Scope> {
        if let Some(rest) = key.strip_prefix("dm:") {
            if rest.is_empty() {
                return None;
            }
            return Some(Scope::dm(rest));
        }
        if let Some(rest) = key.strip_prefix("gc:") {
            if rest.is_empty() {
                return None;
            }
            return Some(Scope::gc(rest));
        }
        if let Some(rest) = key.strip_prefix("server_channel:") {
            let (server_id, channel_id) = rest.split_once(':')?;
            if server_id.is_empty() || channel_id.is_empty() {
                return None;
            }
            return Some(Scope::server_channel(server_id, channel_id));
        }
        if let Some(rest) = key.strip_prefix("server_full:") {
            if rest.is_empty() {
                return None;
            }
            return Some(Scope::server_full(rest));
        }
        None
    }
}

/// Tauri-facing serializable scope shape. Boot.js builds this from
/// Discord channel context (`oslDetectScope`) and Rust converts to
/// the validated [`Scope`] via `TryFrom`.
///
/// Field shape matches the JS DTO produced by `oslDetectScope`:
/// `kind` is snake_case (`"dm"`, `"gc"`, `"server_channel"`,
/// `"server_full"`); `id` carries the canonical id for the named
/// kind; `server_id`/`channel_id` are present only where
/// applicable.
///
/// Tauri's camelCase argument convention applies to top-level
/// command arguments — within a serialized struct payload, we
/// keep snake_case so the on-disk and on-wire shapes stay
/// uniform with the rest of the v=7 schemas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeInput {
    pub kind: ScopeKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
}

/// Validation error for [`ScopeInput`] → [`Scope`] conversion.
#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error("scope.{0} missing required field {1}")]
    MissingField(&'static str, &'static str),
    #[error("scope.id empty")]
    EmptyId,
}

impl TryFrom<ScopeInput> for Scope {
    type Error = ScopeError;

    fn try_from(v: ScopeInput) -> Result<Self, Self::Error> {
        if v.id.is_empty() {
            return Err(ScopeError::EmptyId);
        }
        Ok(match v.kind {
            ScopeKind::Dm => Scope {
                kind: ScopeKind::Dm,
                id: v.id.clone(),
                server_id: None,
                channel_id: v.channel_id.or(Some(v.id)),
            },
            ScopeKind::Gc => Scope {
                kind: ScopeKind::Gc,
                id: v.id.clone(),
                server_id: None,
                channel_id: v.channel_id.or(Some(v.id)),
            },
            ScopeKind::ServerChannel => {
                let server_id = v
                    .server_id
                    .clone()
                    .ok_or(ScopeError::MissingField("server_channel", "server_id"))?;
                let channel_id = v
                    .channel_id
                    .clone()
                    .ok_or(ScopeError::MissingField("server_channel", "channel_id"))?;
                Scope::server_channel(server_id, channel_id)
            }
            ScopeKind::ServerFull => {
                let server_id = v.server_id.clone().unwrap_or_else(|| v.id.clone());
                Scope {
                    kind: ScopeKind::ServerFull,
                    id: server_id.clone(),
                    server_id: Some(server_id),
                    channel_id: None,
                }
            }
        })
    }
}

impl From<&Scope> for ScopeInput {
    fn from(s: &Scope) -> Self {
        ScopeInput {
            kind: s.kind,
            id: s.id.clone(),
            server_id: s.server_id.clone(),
            channel_id: s.channel_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_key_round_trip() {
        let cases = [
            Scope::dm("900000000000000003"),
            Scope::gc("1234567890"),
            Scope::server_channel("9876", "5432"),
            Scope::server_full("9876"),
        ];
        for s in &cases {
            let k = s.storage_key();
            let parsed = Scope::parse(&k).unwrap();
            assert_eq!(&parsed, s, "round trip failed for {k}");
        }
    }

    #[test]
    fn storage_key_format() {
        assert_eq!(Scope::dm("henry_id").storage_key(), "dm:henry_id");
        assert_eq!(Scope::gc("1234").storage_key(), "gc:1234");
        assert_eq!(
            Scope::server_channel("9876", "5432").storage_key(),
            "server_channel:9876:5432"
        );
        assert_eq!(Scope::server_full("9876").storage_key(), "server_full:9876");
    }

    #[test]
    fn parse_rejects_malformed() {
        assert!(Scope::parse("").is_none());
        assert!(Scope::parse("dm:").is_none());
        assert!(Scope::parse("unknown:foo").is_none());
        assert!(Scope::parse("server_channel:onlyone").is_none());
        assert!(Scope::parse("server_channel::missing").is_none());
        assert!(Scope::parse("server_channel:missing:").is_none());
    }

    #[test]
    fn scope_input_try_from_dm() {
        let input = ScopeInput {
            kind: ScopeKind::Dm,
            id: "henry_id".to_string(),
            server_id: None,
            channel_id: None,
        };
        let s = Scope::try_from(input).unwrap();
        assert_eq!(s, Scope::dm("henry_id"));
    }

    #[test]
    fn scope_input_server_channel_requires_both_fields() {
        let missing_channel = ScopeInput {
            kind: ScopeKind::ServerChannel,
            id: "9876:5432".to_string(),
            server_id: Some("9876".to_string()),
            channel_id: None,
        };
        assert!(matches!(
            Scope::try_from(missing_channel),
            Err(ScopeError::MissingField("server_channel", "channel_id"))
        ));
    }
}
