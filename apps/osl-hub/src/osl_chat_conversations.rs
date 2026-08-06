//! First-party OSL Chat conversation records.
//!
//! The UI can later connect this store to its conversation list, but creation
//! rules live here so direct-message membership cannot drift at the renderer
//! boundary.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Mutex;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OslChatConversationKind {
    DirectMessage,
}

impl OslChatConversationKind {
    pub fn wire_name(&self) -> &'static str {
        match self {
            Self::DirectMessage => "direct_message",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OslChatConversationRecord {
    pub conversation_id: String,
    pub kind: OslChatConversationKind,
    pub member_ids: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectMessageConversationInput {
    pub member_ids: Vec<String>,
}

#[derive(Default)]
pub struct OslChatConversationState {
    records: Mutex<Vec<OslChatConversationRecord>>,
}

impl OslChatConversationState {
    pub fn create_direct_message_conversation(
        &self,
        input: DirectMessageConversationInput,
    ) -> Result<OslChatConversationRecord, String> {
        create_direct_message_conversation_command(self, input)
    }

    pub fn conversations(&self) -> Result<Vec<OslChatConversationRecord>, String> {
        self.records
            .lock()
            .map(|records| records.clone())
            .map_err(|_| "OSL Chat conversations are unavailable".to_owned())
    }
}

pub fn create_direct_message_conversation_command(
    state: &OslChatConversationState,
    input: DirectMessageConversationInput,
) -> Result<OslChatConversationRecord, String> {
    let member_ids = normalize_direct_message_members(input.member_ids)?;
    let conversation_id = direct_message_conversation_id(&member_ids);
    let record = OslChatConversationRecord {
        conversation_id,
        kind: OslChatConversationKind::DirectMessage,
        member_ids,
    };
    let mut records = state
        .records
        .lock()
        .map_err(|_| "OSL Chat conversations are unavailable".to_owned())?;
    if let Some(existing) = records
        .iter()
        .find(|existing| existing.conversation_id == record.conversation_id)
    {
        return Ok(existing.clone());
    }
    records.push(record.clone());
    Ok(record)
}

fn normalize_direct_message_members(member_ids: Vec<String>) -> Result<Vec<String>, String> {
    if member_ids.len() != 2 {
        return Err("Direct messages require exactly two member ids".to_owned());
    }
    let normalized: Vec<String> = member_ids
        .into_iter()
        .map(|member| member.trim().to_owned())
        .collect();
    if normalized.iter().any(|member| member.is_empty()) {
        return Err("Direct-message member ids cannot be empty".to_owned());
    }
    if normalized[0] == normalized[1] {
        return Err("Direct messages require two distinct member ids".to_owned());
    }
    Ok(normalized)
}

fn direct_message_conversation_id(member_ids: &[String]) -> String {
    let mut canonical_members = member_ids.to_vec();
    canonical_members.sort();

    let mut hasher = Sha256::new();
    hasher.update(b"osl-chat-direct-message-conversation/v1\0");
    for member in canonical_members {
        hasher.update((member.len() as u32).to_be_bytes());
        hasher.update(member.as_bytes());
    }
    format!("dm-{}", hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_1307_direct_command_creates_conversation_with_exactly_two_member_ids() {
        let state = OslChatConversationState::default();
        let created = create_direct_message_conversation_command(
            &state,
            DirectMessageConversationInput {
                member_ids: vec!["Ava".to_owned(), "Ben".to_owned()],
            },
        )
        .expect("direct command creates a direct-message conversation");
        let conversations = state.conversations().expect("read conversations");

        println!("TASK1307 direct_command=create_osl_chat_direct_message_conversation");
        println!("TASK1307 conversation_count={}", conversations.len());
        println!("TASK1307 conversation_kind={}", created.kind.wire_name());
        println!("TASK1307 member_id_count={}", created.member_ids.len());
        println!("TASK1307 member_ids={}", created.member_ids.join(","));

        assert_eq!(conversations.len(), 1);
        assert_eq!(created.kind, OslChatConversationKind::DirectMessage);
        assert_eq!(created.member_ids, ["Ava", "Ben"]);
        assert_eq!(created.member_ids.len(), 2);

        let source = include_str!("main.rs");
        assert!(source
            .contains("#[tauri::command]\nasync fn create_osl_chat_direct_message_conversation("));
        assert!(source.contains("app.manage(OslChatConversationState::default());"));
        let command_surface = include_str!("hub_command_surface.rs");
        assert!(command_surface.contains("create_osl_chat_direct_message_conversation,"));
        assert!(include_str!("../permissions/hub.toml")
            .contains(r#"commands.allow = ["create_osl_chat_direct_message_conversation"]"#));
        assert!(include_str!("../capabilities/hub.json")
            .contains(r#""allow-create-osl-chat-direct-message-conversation""#));
    }

    #[test]
    fn direct_message_creation_rejects_non_two_member_counts() {
        let state = OslChatConversationState::default();
        let one = create_direct_message_conversation_command(
            &state,
            DirectMessageConversationInput {
                member_ids: vec!["Ava".to_owned()],
            },
        );
        assert_eq!(
            one.unwrap_err(),
            "Direct messages require exactly two member ids"
        );

        let three = create_direct_message_conversation_command(
            &state,
            DirectMessageConversationInput {
                member_ids: vec!["Ava".to_owned(), "Ben".to_owned(), "Cy".to_owned()],
            },
        );
        assert_eq!(
            three.unwrap_err(),
            "Direct messages require exactly two member ids"
        );
        assert!(state.conversations().unwrap().is_empty());
    }
}
