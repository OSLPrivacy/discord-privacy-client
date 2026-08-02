//! Cover-only conversation starters for the no-history path.
//!
//! A cold start deliberately accepts no conversation identifier, contact, or
//! plaintext. It produces a harmless, plausible visible exchange so the cover
//! selector has context even when no prior carrier renders exist.

use rand::{rngs::OsRng, RngCore};

const OPENERS: [&str; 10] = [
    "hey, did you get a chance to take a break today?",
    "i finally made some tea and it helped a lot.",
    "the weather changed so quickly this afternoon.",
    "i was thinking about that little cafe near the station.",
    "did your package ever turn up?",
    "i took the long way home for once.",
    "today felt much longer than it needed to.",
    "i found a playlist that is surprisingly good for working.",
    "i keep meaning to sort out my desk this week.",
    "the train was quieter than usual this morning.",
];

const REPLIES: [&str; 10] = [
    "yeah, a short walk made the afternoon easier.",
    "that sounds nice, I should make one too.",
    "i noticed that too, it caught me off guard.",
    "oh right, I have not been there in ages.",
    "it did, just later than I expected.",
    "sometimes that is the best part of the day.",
    "same here, but at least it is winding down now.",
    "send it over when you have a minute.",
    "a clean desk would be a miracle at this point.",
    "that must have been a nice change for once.",
];

const FOLLOW_UPS: [&str; 10] = [
    "glad you got a little breathing room.",
    "i hope the evening stays calm too.",
    "at least it gave the day a different mood.",
    "we should go again sometime soon.",
    "good, I was hoping it had not gone missing.",
    "it made everything feel less rushed.",
    "tomorrow will probably feel lighter.",
    "i will put it on after dinner.",
    "one small corner at a time is probably enough.",
    "maybe everyone was taking the day slowly.",
];

const CONVERSATION_COUNT: usize = OPENERS.len() * REPLIES.len() * FOLLOW_UPS.len();

/// A fabricated, visible-only exchange used before any cover history exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColdStartConversation(String);

impl ColdStartConversation {
    /// The synthetic exchange that can be supplied to a local cover selector.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Generates a random permutation of cold-start conversations.
///
/// Consecutive calls cannot repeat until every template combination has been
/// used once. This makes the empty-history path varied without consulting any
/// user, platform, or plaintext state.
pub struct ColdStartGenerator {
    offset: usize,
    stride: usize,
    position: usize,
}

impl ColdStartGenerator {
    /// Seed from the operating system CSPRNG.
    pub fn new() -> Self {
        let mut rng = OsRng;
        Self::from_rng(&mut rng)
    }

    /// Construct from a caller-provided CSPRNG, primarily for application
    /// ownership of randomness and deterministic tests.
    pub fn from_rng(rng: &mut impl RngCore) -> Self {
        let offset = (rng.next_u64() as usize) % CONVERSATION_COUNT;
        let mut stride = ((rng.next_u64() as usize) % (CONVERSATION_COUNT - 1)) + 1;
        while gcd(stride, CONVERSATION_COUNT) != 1 {
            stride = (stride % (CONVERSATION_COUNT - 1)) + 1;
        }
        Self {
            offset,
            stride,
            position: 0,
        }
    }

    /// Produce the next fabricated three-turn conversation.
    pub fn next_conversation(&mut self) -> ColdStartConversation {
        let index = (self.offset + self.position * self.stride) % CONVERSATION_COUNT;
        self.position = (self.position + 1) % CONVERSATION_COUNT;
        render(index)
    }
}

impl Default for ColdStartGenerator {
    fn default() -> Self {
        Self::new()
    }
}

fn render(index: usize) -> ColdStartConversation {
    let opener_index = index / (REPLIES.len() * FOLLOW_UPS.len());
    let reply_index = (index / FOLLOW_UPS.len()) % REPLIES.len();
    let follow_up_index = index % FOLLOW_UPS.len();
    ColdStartConversation(format!(
        "{opener}\n{reply}\n{follow_up}",
        opener = OPENERS[opener_index],
        reply = REPLIES[reply_index],
        follow_up = FOLLOW_UPS[follow_up_index],
    ))
}

fn gcd(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}
