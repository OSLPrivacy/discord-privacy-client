//! Local-only names for previously known OSL Chat content.
//!
//! The chat history introduced by task 1303 and the authenticated attachment
//! notice used by task 0655 are both local state.  A renderer asking how to
//! name an old item must use that state, rather than probing a service: the
//! latter would turn a label into a fetch and could disclose whether a remote
//! object still exists.

use std::collections::BTreeMap;

pub const SAVED_HERE_TEXT: &str = "saved-here text";
pub const SAVED_HERE_FILE: &str = "saved-here file";
pub const REMOTE_ONLY: &str = "remote-only";

/// The content kind retained in local OSL Chat metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OldOslChatContentKind {
    Text,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OldOslChatContentState {
    SavedHereText,
    SavedHereFile,
    RemoteOnly,
}

impl OldOslChatContentState {
    /// The stable, intentionally small name shown for this old item.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SavedHereText => SAVED_HERE_TEXT,
            Self::SavedHereFile => SAVED_HERE_FILE,
            Self::RemoteOnly => REMOTE_ONLY,
        }
    }
}

/// An index made from local chat history and authenticated attachment state.
///
/// It intentionally contains no service client.  This makes it impossible for
/// the naming operation to fetch an old item as an incidental side effect.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OldOslChatContentIndex {
    states: BTreeMap<String, OldOslChatContentState>,
}

impl OldOslChatContentIndex {
    /// Record only the locally known facts. `saved_here == false` means the
    /// item is known from authenticated metadata but has no local content copy.
    pub fn record(
        &mut self,
        item_id: impl Into<String>,
        kind: OldOslChatContentKind,
        saved_here: bool,
    ) {
        let state = match (kind, saved_here) {
            (OldOslChatContentKind::Text, true) => OldOslChatContentState::SavedHereText,
            (OldOslChatContentKind::File, true) => OldOslChatContentState::SavedHereFile,
            (_, false) => OldOslChatContentState::RemoteOnly,
        };
        self.states.insert(item_id.into(), state);
    }

    /// Name a locally known old item without contacting its remote service.
    pub fn name(&self, item_id: &str) -> Result<&'static str, String> {
        self.states
            .get(item_id)
            .map(|state| state.name())
            .ok_or_else(|| format!("Unknown OSL Chat content item: {item_id}"))
    }
}
