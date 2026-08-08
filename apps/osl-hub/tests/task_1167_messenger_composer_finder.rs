use osl_privacy_hub::website_driver::{
    discover_messenger_browser_conversation, WebsiteComposerAccessibility,
    WebsiteConversationAccessibility, WebsiteConversationBrowserSnapshot,
};

const BOX_ID: &str = "messenger-box-1167";
const COMPOSER_NAME: &str = "Messenger composer";

fn messenger_fixture(state: &str) -> WebsiteConversationBrowserSnapshot {
    WebsiteConversationBrowserSnapshot {
        url: format!("https://www.messenger.com/t/{BOX_ID}"),
        title: "Messenger — task 1167 fixture".to_owned(),
        accessibility: WebsiteConversationAccessibility {
            service: "messenger".to_owned(),
            active_conversation: true,
            composer: Some(WebsiteComposerAccessibility {
                role: "textbox".to_owned(),
                name: COMPOSER_NAME.to_owned(),
                accessible: true,
                state: state.to_owned(),
            }),
        },
    }
}

fn find(state: &str) -> Vec<String> {
    discover_messenger_browser_conversation(&messenger_fixture(state))
        .into_iter()
        .map(|result| result.composer)
        .collect()
}

#[test]
fn task_1167_messenger_composer_finder_refuses_closed_and_search_focused_states() {
    let good = find("active");
    assert_eq!(
        good,
        [COMPOSER_NAME],
        "good box must yield exactly one Messenger composer"
    );

    let closed = find("closed");
    assert!(closed.is_empty(), "closed must be refused by name");

    let search_focused = find("search-focused");
    assert!(
        search_focused.is_empty(),
        "search-focused must be refused by name"
    );

    let restored = find("active");
    assert_eq!(restored, good, "restored box must return the same result");

    println!(
        "TASK1167 box={BOX_ID} result_count={} result_name={}",
        good.len(),
        good[0]
    );
    println!("TASK1167 state=closed result=refused name={COMPOSER_NAME}");
    println!("TASK1167 state=search-focused result=refused name={COMPOSER_NAME}");
    println!(
        "TASK1167 restored_box={BOX_ID} result_count={} result_name={}",
        restored.len(),
        restored[0]
    );
}
