//! The installed OSL Chats client's route manifest.
//!
//! An installed client has to know which endpoints the deployed service
//! exposes, what each one binds itself to (which parent identifiers it carries
//! and whose authorship it claims) and which roles are allowed to use it. Until
//! now that knowledge was implicit in whatever call the client happened to make,
//! so there was no way to ask "is every deployed route accounted for?" — an
//! endpoint could ship with nobody classifying it.
//!
//! [`CLIENT_ROUTES`] is that classification, and `osl-chats-client --generate`
//! writes it into the install directory as `route-manifest.json`. It is the
//! client's half of the inventory; the deployed service carries its own router
//! table, and the two are required to agree one-for-one. A route that the
//! service dispatches but the manifest does not classify is exactly the hole
//! this table closes.
//!
//! `allowedRoles` is a claim, not a decision: nothing here is consulted at
//! request time. The service decides, and the manifest is judged against what
//! the service actually did.

use serde::{Deserialize, Serialize};

/// One route as the installed client knows it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatsRoute {
    pub op: String,
    pub method: String,
    pub path: String,
    /// `read`, `content-write` or `rights-write`.
    pub kind: String,
    /// The parent identifiers a request on this route must carry, in the order
    /// the service resolves them.
    pub parent_binding: String,
    /// Whose authorship the request claims, and what the service binds it to.
    pub author_binding: String,
    pub allowed_roles: Vec<String>,
}

/// The static form of a manifest row, so the table below is a compile-time
/// constant rather than something assembled at run time.
pub struct StaticRoute {
    pub op: &'static str,
    pub method: &'static str,
    pub path: &'static str,
    pub kind: &'static str,
    pub parent_binding: &'static str,
    pub author_binding: &'static str,
    pub allowed_roles: &'static [&'static str],
}

pub const KIND_READ: &str = "read";
pub const KIND_CONTENT_WRITE: &str = "content-write";
pub const KIND_RIGHTS_WRITE: &str = "rights-write";

pub const MANIFEST_VERSION: &str = "osl-chats-routes-v1";

/// Every endpoint an installed OSL Chats client can reach on the deployed
/// service. Adding a route to the service without adding it here makes the
/// deployed inventory unclassified, and the check refuses that.
pub const CLIENT_ROUTES: &[StaticRoute] = &[
    StaticRoute {
        op: "role.grant",
        method: "POST",
        path: "/v1/chats/role.grant",
        kind: KIND_RIGHTS_WRITE,
        parent_binding: "enclaveId",
        author_binding: "the signer is the granting actor; the target is named in the body",
        allowed_roles: &["owner"],
    },
    StaticRoute {
        op: "roster.list",
        method: "POST",
        path: "/v1/chats/roster.list",
        kind: KIND_READ,
        parent_binding: "enclaveId, optional channelId",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "channel.list",
        method: "POST",
        path: "/v1/chats/channel.list",
        kind: KIND_READ,
        parent_binding: "enclaveId",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "history.read",
        method: "POST",
        path: "/v1/chats/history.read",
        kind: KIND_READ,
        parent_binding: "enclaveId, channelId or threadId",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "history.search",
        method: "POST",
        path: "/v1/chats/history.search",
        kind: KIND_READ,
        parent_binding: "enclaveId, channelId",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "history.sync",
        method: "POST",
        path: "/v1/chats/history.sync",
        kind: KIND_READ,
        parent_binding: "enclaveId, channelId",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "history.subscribe",
        method: "POST",
        path: "/v1/chats/history.subscribe",
        kind: KIND_READ,
        parent_binding: "enclaveId, channelId",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "blob.fetch",
        method: "POST",
        path: "/v1/chats/blob.fetch",
        kind: KIND_READ,
        parent_binding: "enclaveId, messageId resolved to its channel or thread",
        author_binding: "none",
        allowed_roles: &["owner", "member", "limited"],
    },
    StaticRoute {
        op: "message.create",
        method: "POST",
        path: "/v1/chats/message.create",
        kind: KIND_CONTENT_WRITE,
        parent_binding: "enclaveId, channelId, optional threadId whose parent must be that channelId",
        author_binding: "the declared author must be the signer",
        allowed_roles: &["owner", "limited"],
    },
    StaticRoute {
        op: "message.edit",
        method: "POST",
        path: "/v1/chats/message.edit",
        kind: KIND_CONTENT_WRITE,
        parent_binding: "enclaveId, channelId and optional threadId must be the message's own home",
        author_binding: "only the message's original author",
        allowed_roles: &["owner", "limited"],
    },
    StaticRoute {
        op: "message.delete",
        method: "POST",
        path: "/v1/chats/message.delete",
        kind: KIND_CONTENT_WRITE,
        parent_binding: "enclaveId, channelId and optional threadId must be the message's own home",
        author_binding: "the message's original author, or a holder of remove-messages",
        allowed_roles: &["owner", "limited"],
    },
    StaticRoute {
        op: "channel.create",
        method: "POST",
        path: "/v1/chats/channel.create",
        kind: KIND_CONTENT_WRITE,
        parent_binding: "enclaveId must be this enclave",
        author_binding: "the declared creator must be the signer",
        allowed_roles: &["owner", "limited"],
    },
    StaticRoute {
        op: "thread.create",
        method: "POST",
        path: "/v1/chats/thread.create",
        kind: KIND_CONTENT_WRITE,
        parent_binding: "enclaveId must be this enclave and channelId must be a channel of it",
        author_binding: "the declared creator must be the signer",
        allowed_roles: &["owner", "limited"],
    },
];

/// The manifest rows in serialisable form.
pub fn manifest_rows() -> Vec<ChatsRoute> {
    CLIENT_ROUTES
        .iter()
        .map(|route| ChatsRoute {
            op: route.op.to_owned(),
            method: route.method.to_owned(),
            path: route.path.to_owned(),
            kind: route.kind.to_owned(),
            parent_binding: route.parent_binding.to_owned(),
            author_binding: route.author_binding.to_owned(),
            allowed_roles: route.allowed_roles.iter().map(|role| (*role).to_owned()).collect(),
        })
        .collect()
}

/// The bytes an install writes to `route-manifest.json`.
pub fn manifest_json() -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "manifestVersion": MANIFEST_VERSION,
        "routes": manifest_rows(),
    }))
    .unwrap_or_default()
}
