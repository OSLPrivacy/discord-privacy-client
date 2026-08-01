//! The type boundary for data that may condition AI carrier generation.
//!
//! This module deliberately models plaintext separately from cover text.  An
//! [`AiContext`] can be made only from a [`VisibleChannelCover`], which the
//! connected-app adapter creates after it observes a rendered carrier in the
//! app's visible channel.  In particular, there is no `From<PlaintextMessage>`
//! implementation.

/// A user message before it has been encoded as a visible carrier.
///
/// This is public because transport and composer code need a concrete type for
/// private message material.  It intentionally has no conversion into
/// [`AiContext`].
#[derive(Debug, Eq, PartialEq)]
pub struct PlaintextMessage(String);

impl PlaintextMessage {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

/// Cover text observed in the connected application's visible channel.
///
/// The constructor is crate-private so only the connected-app adapter, after
/// it has verified a completed carrier render, can mint this capability.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleChannelCover(String);

impl VisibleChannelCover {
    pub(crate) fn from_observed_render(cover: String) -> Self {
        Self(cover)
    }
}

/// The only text type accepted by AI carrier generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiContext(VisibleChannelCover);

impl From<VisibleChannelCover> for AiContext {
    fn from(cover: VisibleChannelCover) -> Self {
        Self(cover)
    }
}

impl AiContext {
    /// Expose the prior visible cover only at the model invocation boundary.
    pub fn visible_cover(&self) -> &str {
        &self.0 .0
    }
}

#[cfg(test)]
mod tests {
    use super::{AiContext, VisibleChannelCover};

    #[test]
    fn observed_cover_is_the_only_context_input() {
        let context = AiContext::from(VisibleChannelCover::from_observed_render(
            "I missed the train this morning".to_owned(),
        ));

        assert_eq!(context.visible_cover(), "I missed the train this morning");
    }
}
