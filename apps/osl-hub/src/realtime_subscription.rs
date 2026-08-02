//! The ephemeral subscription window for the realtime transport.
//!
//! A tick may carry at most 92 tags, but this manager deliberately sends 32 at
//! a time.  Thirty-two keeps the server's live, unlinkable tag bag modest while
//! still revisiting every tag in a 92-tag set in at most three four-second
//! ticks.  Undelivered blobs remain live for seven days, so that short rotation
//! delay is preferable to making every connection expose the maximum bag.
//!
//! This module accepts only opaque delivery tags.  It has no account,
//! conversation, pointer, or capability input: callers must derive a tag
//! locally before it reaches this boundary.

use std::collections::BTreeSet;

/// Width of a delivery tag in the frozen realtime frame contract.
pub const DELIVERY_TAG_BYTES: usize = 16;
/// Absolute number of tag slots in one `TICK` frame.
pub const MAX_TAGS_PER_TICK: usize = 92;
/// The deliberately smaller rotating window used by this client.
pub const SUBSCRIPTION_WINDOW_SIZE: usize = 32;

/// An opaque locally-derived routing tag.  It cannot represent an account.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeliveryTag([u8; DELIVERY_TAG_BYTES]);

impl DeliveryTag {
    /// Reject the all-zero value, which the wire contract reserves for padding.
    pub fn try_from_bytes(bytes: [u8; DELIVERY_TAG_BYTES]) -> Result<Self, InvalidDeliveryTag> {
        if bytes == [0; DELIVERY_TAG_BYTES] {
            return Err(InvalidDeliveryTag::Zero);
        }
        Ok(Self(bytes))
    }

    pub const fn as_bytes(self) -> [u8; DELIVERY_TAG_BYTES] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidDeliveryTag {
    Zero,
}

/// Maintains the complete locally-known tag set and exposes one rotating,
/// complete-for-this-tick subscription window at a time.
#[derive(Clone, Debug)]
pub struct SubscriptionWindowManager {
    tags: BTreeSet<DeliveryTag>,
    next_start: usize,
    window_size: usize,
}

impl Default for SubscriptionWindowManager {
    fn default() -> Self {
        Self::new(SUBSCRIPTION_WINDOW_SIZE)
    }
}

impl SubscriptionWindowManager {
    /// Refuse a size outside the protocol's fixed 1..=92 tag capacity.
    pub const fn new(window_size: usize) -> Self {
        assert!(window_size > 0, "a subscription window must contain a tag slot");
        assert!(window_size <= MAX_TAGS_PER_TICK, "window exceeds TICK tag capacity");
        Self {
            tags: BTreeSet::new(),
            next_start: 0,
            window_size,
        }
    }

    /// Atomically replace the local tag set.  No previous window is retained.
    pub fn replace_tags(&mut self, tags: impl IntoIterator<Item = DeliveryTag>) {
        self.tags = tags.into_iter().collect();
        self.next_start = 0;
    }

    /// Return the full subscription set for the next tick and advance the
    /// rotation.  Each returned vector contains only raw delivery tags.
    pub fn next_window(&mut self) -> Vec<DeliveryTag> {
        let tags: Vec<_> = self.tags.iter().copied().collect();
        if tags.is_empty() {
            return Vec::new();
        }

        let start = self.next_start % tags.len();
        let count = self.window_size.min(tags.len());
        let window = (0..count)
            .map(|offset| tags[(start + offset) % tags.len()])
            .collect();
        self.next_start = (start + count) % tags.len();
        window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(byte: u8) -> DeliveryTag {
        DeliveryTag::try_from_bytes([byte; DELIVERY_TAG_BYTES]).expect("non-zero fixture tag")
    }

    #[test]
    fn t1_t55_rotates_only_opaque_tags_and_replaces_the_live_window() {
        let mut manager = SubscriptionWindowManager::default();
        manager.replace_tags((1..=33).map(tag));

        let first = manager.next_window();
        let second = manager.next_window();

        assert_eq!(first.len(), SUBSCRIPTION_WINDOW_SIZE);
        assert_eq!(second.len(), SUBSCRIPTION_WINDOW_SIZE);
        assert_eq!(first.first().copied(), Some(tag(1)));
        assert_eq!(first.last().copied(), Some(tag(32)));
        assert_eq!(second.first().copied(), Some(tag(33)));
        assert!(second.contains(&tag(1)));
        assert!(first.iter().chain(&second).all(|tag| tag.as_bytes() != [0; DELIVERY_TAG_BYTES]));

        manager.replace_tags([tag(77)]);
        assert_eq!(manager.next_window(), vec![tag(77)]);
    }

    #[test]
    fn zero_is_reserved_for_frame_padding_not_an_active_subscription() {
        assert_eq!(
            DeliveryTag::try_from_bytes([0; DELIVERY_TAG_BYTES]),
            Err(InvalidDeliveryTag::Zero)
        );
    }
}
