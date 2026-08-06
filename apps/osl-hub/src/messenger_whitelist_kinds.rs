pub const MESSENGER_DIRECT_MESSAGE_KIND: &str = "direct message";
pub const MESSENGER_GROUP_CHAT_KIND: &str = "group chat";

pub const MESSENGER_WHITELIST_KIND_NAMES: [&str; 2] =
    [MESSENGER_DIRECT_MESSAGE_KIND, MESSENGER_GROUP_CHAT_KIND];

pub fn messenger_whitelist_kind_names() -> &'static [&'static str] {
    &MESSENGER_WHITELIST_KIND_NAMES
}
