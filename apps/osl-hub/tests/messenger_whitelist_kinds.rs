use std::collections::HashSet;

use osl_privacy_hub::messenger_whitelist_kinds::{
    messenger_whitelist_kind_names, MESSENGER_DIRECT_MESSAGE_KIND, MESSENGER_GROUP_CHAT_KIND,
};

#[test]
fn messenger_whitelist_kinds_are_exactly_direct_message_and_group_chat() {
    let kinds = messenger_whitelist_kind_names();

    assert_eq!(
        kinds.len(),
        2,
        "the Messenger whitelist kind command must return exactly two kinds"
    );
    assert_eq!(
        kinds,
        &[MESSENGER_DIRECT_MESSAGE_KIND, MESSENGER_GROUP_CHAT_KIND],
        "Messenger kinds must stay named and ordered for command evidence"
    );
    assert_eq!(
        kinds.iter().collect::<HashSet<_>>().len(),
        kinds.len(),
        "Messenger kind names must be unique"
    );

    println!("MESSENGER WHITELIST KIND COUNT: {}", kinds.len());
    for kind in kinds {
        println!("MESSENGER WHITELIST KIND: {kind}");
    }
}
