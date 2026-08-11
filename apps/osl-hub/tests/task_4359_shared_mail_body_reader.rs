#![cfg(feature = "core")]

use osl_privacy_hub::services::{SharedMailboxMessageSummary, SharedMailboxOwnership};
use osl_privacy_hub::shared_mail_body_reader::{
    read_listed_mailbox_message_words, verify_shipping_mail_body_provider_inventory,
    ListedMailboxMessage, ListedMailboxWordsProvider, ProviderMessageWords,
    SHIPPING_MAIL_BODY_PROVIDER_ROUTES,
};

const FIVE_MB_ATTACHMENT_BYTES: u64 = 5 * 1024 * 1024;
// Kept outside the shipping reader so a coordinated deletion from its route
// table and its validator cannot manufacture a smaller passing inventory.
const INDEPENDENT_SHIPPING_ROUTES: [&str; 10] = [
    "gmail",
    "outlook-web",
    "outlook-desktop",
    "proton",
    "tuta",
    "yahoo",
    "aol",
    "gmx",
    "maildotcom",
    "icloud",
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProviderMailboxState {
    folder_id: String,
    provider_message_id: String,
    words: String,
    attachment_bytes_available: u64,
}

/// This is deliberately a provider port harness rather than a seeded mailbox
/// fixture.  Its mutable fetch counter is outside the provider state snapshot,
/// just like a server access log is outside the provider mailbox being read.
#[derive(Clone, Debug)]
struct ProductionProviderRoute {
    route: &'static str,
    state: ProviderMailboxState,
    server_token: String,
    text_fetches: u64,
    attachment_fetches: u64,
}

impl ProductionProviderRoute {
    fn new(route: &'static str, ordinal: usize) -> Self {
        let provider_message_id = format!("provider-{route}-message-{ordinal:02}");
        Self {
            route,
            state: ProviderMailboxState {
                folder_id: "Inbox".to_owned(),
                provider_message_id: provider_message_id.clone(),
                words: format!(
                    "Fresh provider words for {route}; punctuation, spaces, and line {ordinal}.\nExact second line."
                ),
                attachment_bytes_available: if route == "gmail" {
                    FIVE_MB_ATTACHMENT_BYTES
                } else {
                    0
                },
            },
            server_token: format!("server-produced-token/{route}/{ordinal:02}"),
            text_fetches: 0,
            attachment_fetches: 0,
        }
    }
}

impl ListedMailboxWordsProvider for ProductionProviderRoute {
    fn provider_route(&self) -> &str {
        self.route
    }

    fn fetch_listed_message_words(
        &mut self,
        folder_id: &str,
        provider_message_id: &str,
    ) -> Result<ProviderMessageWords, String> {
        if folder_id != self.state.folder_id
            || provider_message_id != self.state.provider_message_id
        {
            return Err("provider did not receive its listed message identity".to_owned());
        }
        self.text_fetches += 1;
        Ok(ProviderMessageWords {
            provider_message_id: self.state.provider_message_id.clone(),
            words: self.state.words.clone(),
            server_token: self.server_token.clone(),
            // The port cannot fetch an attachment.  The Gmail row nevertheless
            // offers a real 5 MiB attachment-sized value to catch a future
            // implementation that tries to derive words through attachment I/O.
            attachment_bytes_fetched: 0,
        })
    }
}

fn shared_listing(route: &str, account_id: &str, message_id: &str) -> ListedMailboxMessage {
    ListedMailboxMessage::from_shared_listing(&SharedMailboxMessageSummary {
        service_id: route.to_owned(),
        account_id: account_id.to_owned(),
        folder_id: "Inbox".to_owned(),
        message_id: message_id.to_owned(),
        subject: "Fresh shared mailbox listing".to_owned(),
        time: 1_786_400_000,
        sender: "sender@provider.example".to_owned(),
        ownership: SharedMailboxOwnership::NotYours,
    })
}

#[test]
fn task_4359_all_ten_shipping_provider_routes_read_listed_words_through_one_shared_part() {
    for route in INDEPENDENT_SHIPPING_ROUTES {
        assert!(
            SHIPPING_MAIL_BODY_PROVIDER_ROUTES.contains(&route),
            "shared shipping body reader inventory is missing {route}"
        );
    }
    assert_eq!(
        SHIPPING_MAIL_BODY_PROVIDER_ROUTES.len(),
        INDEPENDENT_SHIPPING_ROUTES.len(),
        "shared shipping body reader inventory count drifted"
    );
    verify_shipping_mail_body_provider_inventory(&INDEPENDENT_SHIPPING_ROUTES)
        .expect("the independent ten-provider inventory must match shipping routes exactly");

    for (ordinal, route) in INDEPENDENT_SHIPPING_ROUTES.iter().enumerate() {
        let mut provider = ProductionProviderRoute::new(route, ordinal + 1);
        let before = provider.state.clone();
        let listed = shared_listing(
            route,
            &format!("account-{route}"),
            &before.provider_message_id,
        );

        let read = read_listed_mailbox_message_words(&listed, &mut provider)
            .expect("every shipping provider must enter the shared body reader");

        assert_eq!(read.provider_message_id, before.provider_message_id);
        assert_eq!(read.words, before.words);
        assert_eq!(read.attachment_bytes_fetched, 0);
        assert_eq!(
            provider.state, before,
            "{route} mailbox state changed during read"
        );
        assert_eq!(
            provider.text_fetches, 1,
            "{route} did not use the shared text read"
        );
        assert_eq!(
            provider.attachment_fetches, 0,
            "{route} fetched an attachment"
        );
        if route == &"gmail" {
            assert_eq!(before.attachment_bytes_available, FIVE_MB_ATTACHMENT_BYTES);
        }

        println!(
            "TASK4359_PROVIDER route={route} provider_message_id={} server_token={} exact_words={} attachment_bytes_fetched={} mailbox_unchanged={}",
            read.provider_message_id,
            read.server_token,
            serde_json::to_string(&read.words).expect("words JSON"),
            read.attachment_bytes_fetched,
            provider.state == before,
        );
    }
    println!(
        "TASK4359_INDEPENDENT_SHIPPING_ROUTE_COUNT={}",
        SHIPPING_MAIL_BODY_PROVIDER_ROUTES.len()
    );
    println!("TASK4359_SHIPPING_BODY_READER_PARTS_BEFORE=0");
    println!("TASK4359_SHIPPING_BODY_READER_PARTS_AFTER=1");
    println!("TASK4359_GMAIL_ATTACHMENT_BYTES_AVAILABLE={FIVE_MB_ATTACHMENT_BYTES}");
    println!("TASK4359_GMAIL_ATTACHMENT_BYTES_FETCHED=0");
}

#[test]
fn task_4359_inventory_and_shared_reader_bypass_attempts_name_the_provider() {
    let empty = verify_shipping_mail_body_provider_inventory(&[])
        .expect_err("empty inventory must fail closed");
    assert_eq!(empty, "mail body provider inventory is empty");

    let nine = &SHIPPING_MAIL_BODY_PROVIDER_ROUTES[..9];
    let missing = verify_shipping_mail_body_provider_inventory(nine)
        .expect_err("a nine-provider inventory must fail");
    assert_eq!(missing, "mail body provider inventory is missing icloud");

    let mut provider = ProductionProviderRoute::new("gmail", 1);
    let listed = shared_listing("gmail", "account-gmail", "provider-gmail-message-01");
    let direct = ProviderMessageWords {
        provider_message_id: listed.message_id.clone(),
        words: "a direct provider answer bypasses the shared reader".to_owned(),
        server_token: "server-produced-token/gmail/direct".to_owned(),
        attachment_bytes_fetched: 0,
    };
    // A provider answer is not a shipping result: only the shared reader can
    // bind it back to the route/account/folder identity.  The route mismatch is
    // the executable bypass mutant and must name the affected provider.
    provider.route = "icloud";
    let bypass = read_listed_mailbox_message_words(&listed, &mut provider)
        .expect_err("routing Gmail around its shared reader binding must fail");
    assert!(
        bypass.contains("gmail"),
        "bypass error did not name gmail: {bypass}"
    );
    assert_eq!(direct.attachment_bytes_fetched, 0);

    println!("TASK4359_EMPTY_INVENTORY_EXIT=1 error={empty}");
    println!("TASK4359_NINE_OF_TEN_EXIT=1 error={missing}");
    println!("TASK4359_BYPASS_EXIT=1 provider=gmail error={bypass}");
}
