//! Local-only names for previously known OSL Chat content.
//!
//! The chat history introduced by task 1303 and the authenticated attachment
//! notice used by task 0655 are both local state.  A renderer asking how to
//! name an old item must use that state, rather than probing a service: the
//! latter would turn a label into a fetch and could disclose whether a remote
//! object still exists.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SAVED_HERE_TEXT: &str = "saved-here text";
pub const SAVED_HERE_FILE: &str = "saved-here file";
pub const REMOTE_ONLY: &str = "remote-only";
pub const NOT_SAVED_ON_THIS_DEVICE: &str = "not saved on this device";

/// The content kind retained in local OSL Chat metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OldOslChatContentKind {
    Text,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OldOslChatContentState {
    SavedHereText { message_id: Option<String> },
    SavedHereFile { downloaded_path: Option<PathBuf> },
    RemoteOnly,
}

impl OldOslChatContentState {
    /// The stable, intentionally small name shown for this old item.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SavedHereText { .. } => SAVED_HERE_TEXT,
            Self::SavedHereFile { .. } => SAVED_HERE_FILE,
            Self::RemoteOnly => REMOTE_ONLY,
        }
    }
}

/// Exact content returned from a local saved copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OpenedOldOslChatContent {
    Text(String),
    File(Vec<u8>),
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
            (OldOslChatContentKind::Text, true) => {
                OldOslChatContentState::SavedHereText { message_id: None }
            }
            (OldOslChatContentKind::File, true) => OldOslChatContentState::SavedHereFile {
                downloaded_path: None,
            },
            (_, false) => OldOslChatContentState::RemoteOnly,
        };
        self.states.insert(item_id.into(), state);
    }

    /// Record the durable message row used to open saved text without a
    /// service request.
    pub fn record_saved_text(&mut self, item_id: impl Into<String>, message_id: impl Into<String>) {
        self.states.insert(
            item_id.into(),
            OldOslChatContentState::SavedHereText {
                message_id: Some(message_id.into()),
            },
        );
    }

    /// Record the already-downloaded file used to open saved bytes without a
    /// service request.
    pub fn record_downloaded_file(
        &mut self,
        item_id: impl Into<String>,
        downloaded_path: impl AsRef<Path>,
    ) {
        self.states.insert(
            item_id.into(),
            OldOslChatContentState::SavedHereFile {
                downloaded_path: Some(downloaded_path.as_ref().to_path_buf()),
            },
        );
    }

    /// Record authenticated metadata for an item whose content is not held by
    /// this device.
    pub fn record_remote_only(&mut self, item_id: impl Into<String>) {
        self.states
            .insert(item_id.into(), OldOslChatContentState::RemoteOnly);
    }

    /// Name a locally known old item without contacting its remote service.
    pub fn name(&self, item_id: &str) -> Result<&'static str, String> {
        self.states
            .get(item_id)
            .map(|state| state.name())
            .ok_or_else(|| format!("Unknown OSL Chat content item: {item_id}"))
    }

    /// Open only content already held by this device.
    ///
    /// There is deliberately no service client argument. Remote-only items
    /// fail from local metadata with the stable refusal below, so taking the
    /// service offline cannot turn a saved open into a network fallback.
    pub fn open_saved(
        &self,
        item_id: &str,
        message_store: &store::MessageStore,
    ) -> Result<OpenedOldOslChatContent, String> {
        match self.states.get(item_id) {
            Some(OldOslChatContentState::SavedHereText {
                message_id: Some(message_id),
            }) => message_store
                .get(message_id)
                .map_err(|error| format!("could not open saved text: {error}"))?
                .map(|message| OpenedOldOslChatContent::Text(message.plaintext))
                .ok_or_else(|| "saved text is missing from this device".to_owned()),
            Some(OldOslChatContentState::SavedHereFile {
                downloaded_path: Some(downloaded_path),
            }) => std::fs::read(downloaded_path)
                .map(OpenedOldOslChatContent::File)
                .map_err(|error| format!("could not open saved file: {error}")),
            Some(OldOslChatContentState::RemoteOnly) => Err(NOT_SAVED_ON_THIS_DEVICE.to_owned()),
            Some(OldOslChatContentState::SavedHereText { message_id: None }) => {
                Err("saved text has no local message locator".to_owned())
            }
            Some(OldOslChatContentState::SavedHereFile {
                downloaded_path: None,
            }) => Err("saved file has no local download locator".to_owned()),
            None => Err(format!("Unknown OSL Chat content item: {item_id}")),
        }
    }
}
