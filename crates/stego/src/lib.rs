//! Scaffolding placeholder. Implementation pending design-doc review.
//!
//! v1: Mode 1 template-based stego. Modes 2 (Markov) and 3 (Meteor LLM)
//! deferred to v2.6.
//!
//! ## Hard architectural requirement: message-independence
//!
//! Each stego'd message must be decodable from itself plus the shared
//! secret, **without reference to any other message**. Discord may
//! reorder, edit, or delete messages on its CDN; context-dependent
//! stego (where decoding message N depends on N-1, N-2, ...) breaks
//! unrecoverably the moment any context message is lost. Making it
//! reliable would require storing messages on our own server,
//! converting the project from "privacy layer over Discord" into
//! "Discord-skinned messenger with separate storage" — defeating
//! the project thesis. Applies to Mode 1, Mode 2, and Mode 3 alike.
//! See `docs/design/pqxdh-double-ratchet.md` "Stego encoding
//! constraint" and `docs/THREAT_MODEL.md`.
//!
//! ## Mode 1 quality bar
//!
//! Templates must pass Discord's automated scanning AND look like
//! plausible chat when read by a human scrolling history. Burned
//! messages render as their cover text (no "[deleted]" placeholder;
//! see `docs/design/group-messaging.md` and `docs/THREAT_MODEL.md`),
//! so an observer scrolling history must not be able to identify
//! burned messages from the templates' style. Templates must be
//! diverse enough that a recipient does not notice "every burned
//! message has the same vibe."
//!
//! ## Conversation-level stealth (documented limitation)
//!
//! Per-message fluency does not imply multi-message coherence — a
//! direct consequence of the message-independence requirement above.
//! A close reader of conversation history may notice encrypted
//! messages don't thread together. Mitigation lives at the user
//! level: mix encrypted (sensitive) with plaintext (small-talk).
//! See `docs/THREAT_MODEL.md` and `docs/ONBOARDING.md`.
