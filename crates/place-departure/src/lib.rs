//! TASK 6818 — leaving a group or an enclave without orphaning keys or
//! ownership.
//!
//! Leaving a place is three separate obligations, and a build that meets only
//! the first is the failure this crate exists to prevent:
//!
//! 1. **The roster moves.** A departure is a signed self-removal on the
//!    place's membership ladder. Every remaining client applies the same event
//!    and converges on the same roster and epoch — including across a restart,
//!    where the roster is replayed from the signed events rather than restored
//!    from a cached snapshot.
//! 2. **The keys move.** A group place rotates every remaining member's sender
//!    chain and re-distributes it to the current roster only. An enclave place
//!    re-keys every channel the leaver could read and wraps the new key to the
//!    remaining readers only. The leaver keeps whatever key material is already
//!    on their device — that is the honest outcome, keys cannot be recalled —
//!    but it opens nothing sent afterwards.
//! 3. **Ownership does not evaporate.** Departure by the last holder of a role
//!    id carrying the place's required authority is refused until that
//!    authority is transferred. That decision reads role ids and capability
//!    sets, never a role's display label.
//!
//! And the local half: the departing client drops the place from its own list
//! while honestly keeping the historical copy it already had, unless the
//! separate deletion choice was taken alongside the departure.

pub mod authority;
pub mod client;
pub mod enclave;
pub mod group;
pub mod ids;
pub mod keys;
pub mod membership;
pub mod run;
pub mod world;

/// The scenario the shipped check runs against.
pub const FIXTURE_JSON: &str = include_str!("../scenario/task-6818-places.json");

/// Parses the shipped fixture.
pub fn shipped_fixture() -> Result<world::Fixture, serde_json::Error> {
    serde_json::from_str(FIXTURE_JSON)
}
