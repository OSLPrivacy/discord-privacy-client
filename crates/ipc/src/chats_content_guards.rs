//! The deployed OSL Chats content-write guards.
//!
//! `chats_service_authority` already refuses forbidden *reads* and a
//! self-permission raise. Nothing there stood in front of a content write: a
//! signed client could create, edit or delete a message, or create a channel or
//! a thread, and no deployed rule looked at whether it was allowed to.
//!
//! This module is the enforcement point for those five endpoints. Every write
//! endpoint answers four independent questions, and each answer is a separate
//! one-line guard here, so a throwaway build can bypass exactly one of them:
//!
//! * **membership** — is the actor a current participant of the container this
//!   write targets? For a channel or thread write that is the channel's
//!   effective reader set (the shipping open-vs-limited rule), so an ordinary
//!   member is not a participant of a limited channel they are not in, and a
//!   removed member is not a participant of anything. For an enclave-level
//!   write it is the roster.
//! * **role** — does the actor hold the right this endpoint needs, per the
//!   shipping `ServerPermissionStore`? This is deliberately *not* intersected
//!   with membership: a stale grant that survives removal must be stopped by
//!   the membership guard, not by the accident of the grant being cleared.
//! * **author** — is the authorship the request claims the actor's own? A
//!   created message or channel is attributed to the signer, an edit is the
//!   original author's alone, and a delete is the author's or a
//!   `remove-messages` holder's.
//! * **parent binding** — do the enclave, channel and thread identifiers the
//!   request carries describe the place it says they do? A thread never belongs
//!   to a channel other than its own, and a write never lands in another
//!   server.
//!
//! Each guard takes one already-computed boolean fact and returns whether the
//! deployed service may proceed. Facts are computed from the shipping
//! permission types at the call site; the guards themselves are the decision.
//! Bypassing one bypasses one rule for one endpoint and nothing else.

/// One production guard, named by the endpoint it protects and the binding it
/// enforces. The mutant ladder is derived from this table, so a guard that
/// nothing bypasses is a guard nobody proved.
pub struct ContentGuard {
    pub endpoint: &'static str,
    pub kind: &'static str,
    pub marker: &'static str,
}

pub const GUARD_MEMBERSHIP: &str = "membership";
pub const GUARD_ROLE: &str = "role";
pub const GUARD_AUTHOR: &str = "author";
pub const GUARD_PARENT: &str = "parent-binding";

/// Every production content-write guard the deployed service applies.
pub const GUARD_INVENTORY: &[ContentGuard] = &[
    ContentGuard { endpoint: "message.create", kind: GUARD_MEMBERSHIP, marker: "TASK6206-GUARD-MESSAGE-CREATE-MEMBERSHIP" },
    ContentGuard { endpoint: "message.create", kind: GUARD_ROLE, marker: "TASK6206-GUARD-MESSAGE-CREATE-ROLE" },
    ContentGuard { endpoint: "message.create", kind: GUARD_AUTHOR, marker: "TASK6206-GUARD-MESSAGE-CREATE-AUTHOR" },
    ContentGuard { endpoint: "message.create", kind: GUARD_PARENT, marker: "TASK6206-GUARD-MESSAGE-CREATE-PARENT" },
    ContentGuard { endpoint: "message.edit", kind: GUARD_MEMBERSHIP, marker: "TASK6206-GUARD-MESSAGE-EDIT-MEMBERSHIP" },
    ContentGuard { endpoint: "message.edit", kind: GUARD_ROLE, marker: "TASK6206-GUARD-MESSAGE-EDIT-ROLE" },
    ContentGuard { endpoint: "message.edit", kind: GUARD_AUTHOR, marker: "TASK6206-GUARD-MESSAGE-EDIT-AUTHOR" },
    ContentGuard { endpoint: "message.edit", kind: GUARD_PARENT, marker: "TASK6206-GUARD-MESSAGE-EDIT-PARENT" },
    ContentGuard { endpoint: "message.delete", kind: GUARD_MEMBERSHIP, marker: "TASK6206-GUARD-MESSAGE-DELETE-MEMBERSHIP" },
    ContentGuard { endpoint: "message.delete", kind: GUARD_ROLE, marker: "TASK6206-GUARD-MESSAGE-DELETE-ROLE" },
    ContentGuard { endpoint: "message.delete", kind: GUARD_AUTHOR, marker: "TASK6206-GUARD-MESSAGE-DELETE-AUTHOR" },
    ContentGuard { endpoint: "message.delete", kind: GUARD_PARENT, marker: "TASK6206-GUARD-MESSAGE-DELETE-PARENT" },
    ContentGuard { endpoint: "channel.create", kind: GUARD_MEMBERSHIP, marker: "TASK6206-GUARD-CHANNEL-CREATE-MEMBERSHIP" },
    ContentGuard { endpoint: "channel.create", kind: GUARD_ROLE, marker: "TASK6206-GUARD-CHANNEL-CREATE-ROLE" },
    ContentGuard { endpoint: "channel.create", kind: GUARD_AUTHOR, marker: "TASK6206-GUARD-CHANNEL-CREATE-AUTHOR" },
    ContentGuard { endpoint: "channel.create", kind: GUARD_PARENT, marker: "TASK6206-GUARD-CHANNEL-CREATE-PARENT" },
    ContentGuard { endpoint: "thread.create", kind: GUARD_MEMBERSHIP, marker: "TASK6206-GUARD-THREAD-CREATE-MEMBERSHIP" },
    ContentGuard { endpoint: "thread.create", kind: GUARD_ROLE, marker: "TASK6206-GUARD-THREAD-CREATE-ROLE" },
    ContentGuard { endpoint: "thread.create", kind: GUARD_AUTHOR, marker: "TASK6206-GUARD-THREAD-CREATE-AUTHOR" },
    ContentGuard { endpoint: "thread.create", kind: GUARD_PARENT, marker: "TASK6206-GUARD-THREAD-CREATE-PARENT" },
];

// ---------------------------------------------------------------------------
// message.create — posting a message into a channel or one of its threads.
// ---------------------------------------------------------------------------

/// Only a current participant of the named channel may post into it. A removed
/// member is not one, and neither is a member of the enclave who is not in a
/// limited channel — not even the owner.
#[allow(unused_variables)]
pub fn message_create_membership_allowed(actor_participates_in_target: bool) -> bool {
    actor_participates_in_target // TASK6206-GUARD-MESSAGE-CREATE-MEMBERSHIP
}

/// Posting needs the `send` right.
#[allow(unused_variables)]
pub fn message_create_role_allowed(actor_holds_send: bool) -> bool {
    actor_holds_send // TASK6206-GUARD-MESSAGE-CREATE-ROLE
}

/// A new message is attributed to whoever signed the request; a client may not
/// post under somebody else's name.
#[allow(unused_variables)]
pub fn message_create_author_allowed(declared_author_is_signer: bool) -> bool {
    declared_author_is_signer // TASK6206-GUARD-MESSAGE-CREATE-AUTHOR
}

/// The enclave, channel and (if given) thread the request carries must be a
/// real place: this enclave, one of its channels, and a thread of that channel.
#[allow(unused_variables)]
pub fn message_create_parent_allowed(parent_binding_holds: bool) -> bool {
    parent_binding_holds // TASK6206-GUARD-MESSAGE-CREATE-PARENT
}

// ---------------------------------------------------------------------------
// message.edit — replacing the ciphertext of an existing message.
// ---------------------------------------------------------------------------

/// Only a current participant of the channel the edit names may edit in it.
#[allow(unused_variables)]
pub fn message_edit_membership_allowed(actor_participates_in_target: bool) -> bool {
    actor_participates_in_target // TASK6206-GUARD-MESSAGE-EDIT-MEMBERSHIP
}

/// Editing needs the `send` right: a read-only member cannot rewrite history.
#[allow(unused_variables)]
pub fn message_edit_role_allowed(actor_holds_send: bool) -> bool {
    actor_holds_send // TASK6206-GUARD-MESSAGE-EDIT-ROLE
}

/// Only the message's original author may replace its content. This holds for
/// the enclave owner too: an owner may remove a message, never forge one.
#[allow(unused_variables)]
pub fn message_edit_author_allowed(actor_is_original_author: bool) -> bool {
    actor_is_original_author // TASK6206-GUARD-MESSAGE-EDIT-AUTHOR
}

/// The enclave, channel and thread the edit carries must be the message's own
/// home, so a message cannot be dragged into another channel by editing it.
#[allow(unused_variables)]
pub fn message_edit_parent_allowed(parent_binding_holds: bool) -> bool {
    parent_binding_holds // TASK6206-GUARD-MESSAGE-EDIT-PARENT
}

// ---------------------------------------------------------------------------
// message.delete — retiring an existing message.
// ---------------------------------------------------------------------------

/// Only a current participant of the channel the delete names may delete in it.
#[allow(unused_variables)]
pub fn message_delete_membership_allowed(actor_participates_in_target: bool) -> bool {
    actor_participates_in_target // TASK6206-GUARD-MESSAGE-DELETE-MEMBERSHIP
}

/// Deleting needs the `send` right; a read-only member cannot retire anything.
#[allow(unused_variables)]
pub fn message_delete_role_allowed(actor_holds_send: bool) -> bool {
    actor_holds_send // TASK6206-GUARD-MESSAGE-DELETE-ROLE
}

/// A message is the author's to retire; anybody else needs `remove-messages`.
#[allow(unused_variables)]
pub fn message_delete_author_allowed(actor_is_author_or_moderator: bool) -> bool {
    actor_is_author_or_moderator // TASK6206-GUARD-MESSAGE-DELETE-AUTHOR
}

/// The enclave, channel and thread the delete carries must be the message's own
/// home.
#[allow(unused_variables)]
pub fn message_delete_parent_allowed(parent_binding_holds: bool) -> bool {
    parent_binding_holds // TASK6206-GUARD-MESSAGE-DELETE-PARENT
}

// ---------------------------------------------------------------------------
// channel.create — adding a channel to the enclave.
// ---------------------------------------------------------------------------

/// Only somebody currently on the roster may add a channel to the enclave.
#[allow(unused_variables)]
pub fn channel_create_membership_allowed(actor_is_current_member: bool) -> bool {
    actor_is_current_member // TASK6206-GUARD-CHANNEL-CREATE-MEMBERSHIP
}

/// Adding a channel needs the `make-channels` right.
#[allow(unused_variables)]
pub fn channel_create_role_allowed(actor_holds_make_channels: bool) -> bool {
    actor_holds_make_channels // TASK6206-GUARD-CHANNEL-CREATE-ROLE
}

/// A new channel is attributed to whoever signed the request.
#[allow(unused_variables)]
pub fn channel_create_author_allowed(declared_creator_is_signer: bool) -> bool {
    declared_creator_is_signer // TASK6206-GUARD-CHANNEL-CREATE-AUTHOR
}

/// The enclave the request carries must be this enclave: a client may not
/// create a channel in another server through this one.
#[allow(unused_variables)]
pub fn channel_create_parent_allowed(parent_binding_holds: bool) -> bool {
    parent_binding_holds // TASK6206-GUARD-CHANNEL-CREATE-PARENT
}

// ---------------------------------------------------------------------------
// thread.create — hanging a thread under a channel.
// ---------------------------------------------------------------------------

/// Only a current participant of the parent channel may open a thread in it.
#[allow(unused_variables)]
pub fn thread_create_membership_allowed(actor_participates_in_target: bool) -> bool {
    actor_participates_in_target // TASK6206-GUARD-THREAD-CREATE-MEMBERSHIP
}

/// Opening a thread needs the `make-channels` right.
#[allow(unused_variables)]
pub fn thread_create_role_allowed(actor_holds_make_channels: bool) -> bool {
    actor_holds_make_channels // TASK6206-GUARD-THREAD-CREATE-ROLE
}

/// A new thread is attributed to whoever signed the request.
#[allow(unused_variables)]
pub fn thread_create_author_allowed(declared_creator_is_signer: bool) -> bool {
    declared_creator_is_signer // TASK6206-GUARD-THREAD-CREATE-AUTHOR
}

/// The enclave and parent channel the request carries must be this enclave and
/// a channel of it.
#[allow(unused_variables)]
pub fn thread_create_parent_allowed(parent_binding_holds: bool) -> bool {
    parent_binding_holds // TASK6206-GUARD-THREAD-CREATE-PARENT
}
