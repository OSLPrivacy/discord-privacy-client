pub const MESSENGER_DIRECT_MESSAGE_KIND: &str = "direct message";
pub const MESSENGER_GROUP_CHAT_KIND: &str = "group chat";
pub const MESSENGER_ROOM_KIND: &str = "room";

pub const MESSENGER_WHITELIST_KIND_NAMES: [&str; 3] = [
    MESSENGER_DIRECT_MESSAGE_KIND,
    MESSENGER_GROUP_CHAT_KIND,
    MESSENGER_ROOM_KIND,
];

pub fn messenger_whitelist_kind_names() -> &'static [&'static str] {
    &MESSENGER_WHITELIST_KIND_NAMES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_4200_messenger_finish_line_names_the_full_kind_list() {
        let kinds = messenger_whitelist_kind_names();
        println!(
            "TASK4200_MESSENGER count={} names={}",
            kinds.len(),
            kinds.join(", ")
        );
        assert_eq!(kinds, &["direct message", "group chat", "room"]);
        assert_eq!(kinds.len(), MESSENGER_WHITELIST_KIND_NAMES.len());
    }
}
