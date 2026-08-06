use serde::Serialize;

use crate::core_bridge::HubCoreState;

pub const OSL_CHAT_FREE_FILE_LIMIT_BYTES: u64 = 25 * 1024 * 1024;
pub const OSL_CHAT_PRO_FILE_LIMIT_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OslChatFileSizeLimitDto {
    pub tier: &'static str,
    pub label: &'static str,
    pub max_bytes: u64,
}

pub fn osl_chat_file_size_limits() -> [OslChatFileSizeLimitDto; 2] {
    [
        OslChatFileSizeLimitDto {
            tier: "free",
            label: "25 MB",
            max_bytes: OSL_CHAT_FREE_FILE_LIMIT_BYTES,
        },
        OslChatFileSizeLimitDto {
            tier: "pro",
            label: "1 GB",
            max_bytes: OSL_CHAT_PRO_FILE_LIMIT_BYTES,
        },
    ]
}

pub fn current_osl_chat_file_size_limit(core: &HubCoreState) -> OslChatFileSizeLimitDto {
    if ipc::tier_gate::is_paid_equivalent(&core.osl) {
        osl_chat_file_size_limits()[1]
    } else {
        osl_chat_file_size_limits()[0]
    }
}
