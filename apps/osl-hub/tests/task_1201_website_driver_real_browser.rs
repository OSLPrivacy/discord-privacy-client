use osl_privacy_hub::{
    hub_command_surface::{
        read_icloud_mailbox_for_scrub_with_driver, read_icloud_mailbox_pages_for_scrub_with_driver,
        read_ordinary_send_progress, read_protected_email_open_message_with_driver,
        read_proton_mailbox_for_scrub_with_driver, send_ordinary_message_with_progress,
        IcloudMailboxForScrubReadRequest, IcloudMailboxPagingReadRequest, MailPagingStopReason,
        OrdinarySendProgress, OrdinarySendProgressRequest, ProtectedEmailOpenMessageReadRequest,
        ProtonMailboxForScrubReadRequest,
    },
    website_driver::{
        RealBrowserWebsiteDriver, WebsiteDriver, WebsiteLiveRunProgress, WebsiteNamedControl,
        WebsitePageRequest,
    },
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

const TASK_1201_TITLE: &str = "OSL Task 1201 Real Browser Title";
const TASK_1204_TITLE: &str = "OSL Task 1204 Read Page Controls";
const TASK_1213_TITLE: &str = "OSL Task 1213 Read Open Message Pane";
const TASK_1213_BODY: &str =
    "Fixture selected email body for task 1213. It stays inside the open message pane.";
const TASK_1213_THREAD_ID: &str = "email-thread-1213-stable";
const TASK_1214_TITLE: &str = "OSL Task 1214 Protected Email Reader";
const TASK_1214_COVER_MESSAGE: &str =
    "Fixture cover message for task 1214. The protected email reader got it through the driver.";
const TASK_1214_THREAD_ID: &str = "email-thread-1214-stable";
const TASK_3056_TITLE: &str = "OSL Task 3056 Seeded Proton Mailbox";
const TASK_3056_FOLDERS: [&str; 4] = ["Inbox", "Sent", "Archive", "Trash"];
const TASK_3072_TITLE: &str = "OSL Task 3072 Seeded iCloud Mailbox Paging";
const TASK_3072_FOLDERS: [&str; 4] = ["Inbox", "Sent", "Archive", "Trash"];
const TASK_3072_MESSAGE_COUNT: usize = 120;
const TASK_3072_PAGE_SIZE: usize = 30;
const TASK_3072_SET_PAUSE_MS: u64 = 5;
const TASK_1426_TITLE: &str = "OSL Task 1426 Live Run Progress";
const TASK_1426_ACCOUNT: &str = "fixture-account-1426@example.invalid";
const TASK_3603_TITLE: &str = "OSL Task 3603 Ordinary Send Progress";
const TASK_3603_DRAFT: &str = "ordinary send progress fixture draft";
const TASK_3603_STEPS: [&str; 5] = [
    "private_save",
    "service_acceptance",
    "local_save",
    "receiver_publish",
    "final_confirmation",
];

#[test]
fn task_1201_direct_driver_command_opens_local_test_page_and_reads_title() {
    let server = LocalTestPage::spawn(TASK_1201_TITLE);
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local test page through real browser");
    let read = driver
        .read_page(&page)
        .expect("read local test page title through real browser");

    assert_eq!(page.url, server.url());
    assert_eq!(read.title, TASK_1201_TITLE);

    println!("TASK1201 direct_driver_command=find_page");
    println!("TASK1201 direct_driver_command=read_page");
    println!(
        "TASK1201 browser_executable={}",
        driver.browser_executable().display()
    );
    println!("TASK1201 local_test_page_url={}", page.url);
    println!("TASK1201 read_title={}", read.title);
}

#[test]
fn task_1204_fixture_page_returns_compose_send_and_reading_pane_names() {
    let server = LocalTestPage::spawn_body(
        TASK_1204_TITLE,
        r#"
            <main>
              <section role="log" aria-label="Reading pane">
                <article>Existing visible message</article>
              </section>
              <label id="compose-label" for="compose">Compose</label>
              <textarea id="compose" aria-labelledby="compose-label"></textarea>
              <button type="button">Send</button>
              <button type="button" hidden>Hidden send</button>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local test page through real browser");
    let read = driver
        .read_page(&page)
        .expect("read local test page controls through real browser");

    assert_eq!(read.title, TASK_1204_TITLE);
    assert_eq!(read.controls.editable_boxes, vec!["Compose"]);
    assert_eq!(read.controls.buttons, vec!["Send"]);
    assert_eq!(read.controls.visible_message_areas, vec!["Reading pane"]);

    println!("TASK1204 direct_driver_command=read_page");
    println!(
        "TASK1204 editable_box_count={}",
        read.controls.editable_boxes.len()
    );
    for name in &read.controls.editable_boxes {
        println!("TASK1204 editable_box_name={name}");
    }
    println!("TASK1204 button_count={}", read.controls.buttons.len());
    for name in &read.controls.buttons {
        println!("TASK1204 button_name={name}");
    }
    println!(
        "TASK1204 visible_message_area_count={}",
        read.controls.visible_message_areas.len()
    );
    for name in &read.controls.visible_message_areas {
        println!("TASK1204 visible_message_area_name={name}");
    }
}

#[test]
fn task_1213_fixture_message_returns_body_and_stable_thread_identity() {
    let server = LocalTestPage::spawn_body(
        TASK_1213_TITLE,
        r#"
            <main>
              <section role="log" aria-label="Reading pane">
                <article data-osl-open-email="true" data-osl-thread-id="email-thread-1213-stable">
                  <header>
                    <h2>Task 1213 fixture message</h2>
                  </header>
                  <div data-osl-email-body>
                    Fixture selected email body for task 1213.
                    It stays inside the open message pane.
                  </div>
                </article>
              </section>
              <article data-osl-open-email="false" data-osl-thread-id="other-thread">
                <div data-osl-email-body>Wrong body</div>
              </article>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local email fixture page through real browser");
    let selected = driver
        .read_selected_email(&page)
        .expect("read selected fixture email through real browser");

    assert_eq!(selected.body, TASK_1213_BODY);
    assert_eq!(selected.conversation_identity, TASK_1213_THREAD_ID);

    println!("TASK1213 direct_driver_command=read_selected_email");
    println!("TASK1213 fixture_message_body={}", selected.body);
    println!(
        "TASK1213 stable_thread_identity={}",
        selected.conversation_identity
    );
}

#[test]
fn task_1214_direct_reader_command_returns_fixture_cover_message() {
    let server = LocalTestPage::spawn_body(
        TASK_1214_TITLE,
        r#"
            <main>
              <section role="log" aria-label="Reading pane">
                <article data-osl-open-email="true" data-osl-thread-id="email-thread-1214-stable">
                  <header>
                    <h2>Task 1214 fixture cover</h2>
                  </header>
                  <p data-osl-email-body>
                    Fixture cover message for task 1214.
                    The protected email reader got it through the driver.
                  </p>
                </article>
              </section>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let read = read_protected_email_open_message_with_driver(
        &mut driver,
        ProtectedEmailOpenMessageReadRequest {
            page_url: server.url(),
        },
    )
    .expect("direct reader command reads the open email through the driver");

    assert_eq!(read.cover_message, TASK_1214_COVER_MESSAGE);
    assert_eq!(read.conversation_identity, TASK_1214_THREAD_ID);

    println!("TASK1214 direct_reader_command=read_protected_email_open_message");
    println!("TASK1214 driver_command=read_selected_email");
    println!("TASK1214 fixture_cover_message={}", read.cover_message);
    println!(
        "TASK1214 stable_thread_identity={}",
        read.conversation_identity
    );
}

#[test]
fn task_3056_seeded_proton_mailbox_returns_folders_sent_messages_and_ownership() {
    let server = LocalTestPage::spawn_body(
        TASK_3056_TITLE,
        r#"
            <main>
              <nav aria-label="Proton folders">
                <button type="button" data-osl-mail-folder="Inbox">Inbox</button>
                <button type="button" data-osl-mail-folder="Sent">Sent</button>
                <button type="button" data-osl-mail-folder="Archive">Archive</button>
                <button type="button" data-osl-mail-folder="Trash">Trash</button>
              </nav>
              <section aria-label="Seeded Proton messages">
                <article
                  data-osl-mail-message
                  data-osl-folder="Sent"
                  data-osl-subject="SCRUB-PR-MINE"
                  data-osl-time="2026-08-06 08:15"
                  data-osl-sender="scrub.owner@proton.test"
                  data-osl-scrub-owner-marker="SCRUB-PR-MINE">
                  <h2 data-osl-mail-subject>SCRUB-PR-MINE</h2>
                  <span data-osl-mail-time>2026-08-06 08:15</span>
                  <span data-osl-mail-sender>scrub.owner@proton.test</span>
                </article>
                <article
                  data-osl-mail-message
                  data-osl-folder="Sent"
                  data-osl-subject="Scrub export request"
                  data-osl-time="2026-08-06 08:20"
                  data-osl-sender="scrub.owner@proton.test">
                  <h2 data-osl-mail-subject>Scrub export request</h2>
                  <span data-osl-mail-time>2026-08-06 08:20</span>
                  <span data-osl-mail-sender>scrub.owner@proton.test</span>
                </article>
                <article
                  data-osl-mail-message
                  data-osl-folder="Sent"
                  data-osl-subject="Scrub confirmation note"
                  data-osl-time="2026-08-06 08:25"
                  data-osl-sender="scrub.owner@proton.test">
                  <h2 data-osl-mail-subject>Scrub confirmation note</h2>
                  <span data-osl-mail-time>2026-08-06 08:25</span>
                  <span data-osl-mail-sender>scrub.owner@proton.test</span>
                </article>
                <article
                  data-osl-mail-message
                  data-osl-folder="Inbox"
                  data-osl-subject="Provider reply one"
                  data-osl-time="2026-08-06 09:10"
                  data-osl-sender="privacy-team@example.test">
                  <h2 data-osl-mail-subject>Provider reply one</h2>
                  <span data-osl-mail-time>2026-08-06 09:10</span>
                  <span data-osl-mail-sender>privacy-team@example.test</span>
                </article>
                <article
                  data-osl-mail-message
                  data-osl-folder="Inbox"
                  data-osl-subject="Provider reply two"
                  data-osl-time="2026-08-06 09:25"
                  data-osl-sender="support@example.test">
                  <h2 data-osl-mail-subject>Provider reply two</h2>
                  <span data-osl-mail-time>2026-08-06 09:25</span>
                  <span data-osl-mail-sender>support@example.test</span>
                </article>
                <article
                  hidden
                  data-osl-mail-message
                  data-osl-folder="Sent"
                  data-osl-subject="Hidden decoy"
                  data-osl-time="2026-08-06 10:00"
                  data-osl-sender="decoy@example.test">
                  Hidden decoy
                </article>
              </section>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let read = read_proton_mailbox_for_scrub_with_driver(
        &mut driver,
        ProtonMailboxForScrubReadRequest {
            page_url: server.url(),
        },
    )
    .expect("read seeded Proton mailbox through real browser");

    assert_eq!(read.folders, TASK_3056_FOLDERS);
    assert_eq!(read.sent.len(), 3);
    assert_eq!(read.inbox.len(), 2);
    assert!(read
        .sent
        .iter()
        .any(|message| message.owner_marker == "SCRUB-PR-MINE" && message.yours));
    assert!(read.inbox.iter().all(|message| !message.yours));
    for message in &read.sent {
        assert!(!message.subject.is_empty());
        assert!(!message.time.is_empty());
        assert!(!message.sender.is_empty());
    }

    println!("TASK3056 direct_reader_command=read_proton_mailbox_for_scrub");
    println!("TASK3056 provider=Proton Mail");
    println!("TASK3056 folder_count={}", read.folders.len());
    for folder in &read.folders {
        println!("TASK3056 folder={folder}");
    }
    println!("TASK3056 sent_count={}", read.sent.len());
    for (index, message) in read.sent.iter().enumerate() {
        println!(
            "TASK3056 sent_message_{} subject={} time={} sender={} owner_marker={} owner_label={}",
            index + 1,
            message.subject,
            message.time,
            message.sender,
            message.owner_marker,
            if message.yours { "yours" } else { "not_yours" }
        );
    }
    println!("TASK3056 inbox_count={}", read.inbox.len());
    for (index, message) in read.inbox.iter().enumerate() {
        println!(
            "TASK3056 inbox_message_{} subject={} owner_label={}",
            index + 1,
            message.subject,
            if message.yours { "yours" } else { "not_yours" }
        );
    }
}

#[test]
fn task_3072_icloud_mailbox_pages_with_shared_pause_and_stop() {
    let server = LocalTestPage::spawn_body(TASK_3072_TITLE, task_3072_icloud_fixture_body());

    let mut full_driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
    let seeded = read_icloud_mailbox_for_scrub_with_driver(
        &mut full_driver,
        IcloudMailboxForScrubReadRequest {
            page_url: server.url(),
        },
    )
    .expect("direct iCloud mailbox reader returns the first seeded page");
    assert_eq!(seeded.folders, TASK_3072_FOLDERS);
    assert_eq!(seeded.sent.len(), TASK_3072_PAGE_SIZE);
    assert!(seeded
        .sent
        .iter()
        .any(|message| message.owner_marker == "SCRUB-IC-MINE" && message.yours));

    let full = read_icloud_mailbox_pages_for_scrub_with_driver(
        &mut full_driver,
        IcloudMailboxPagingReadRequest {
            page_url: server.url(),
            folder_id: "Sent".to_owned(),
            set_pause_ms: TASK_3072_SET_PAUSE_MS,
            stop_during_page: None,
        },
    )
    .expect("direct iCloud page reader reads the seeded folder");
    assert_eq!(full.folder_id, "Sent");
    assert_eq!(full.message_count, TASK_3072_MESSAGE_COUNT);
    assert!(full.page_count >= 3);
    assert_eq!(full.stop_reason, MailPagingStopReason::EndOfPlace);
    assert!(full
        .inter_action_gaps_ms
        .iter()
        .all(|gap| *gap >= TASK_3072_SET_PAUSE_MS));
    assert_eq!(full.one_screen_scrolls, full.page_count - 1);

    let mut stopped_driver =
        RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
    let stopped = read_icloud_mailbox_pages_for_scrub_with_driver(
        &mut stopped_driver,
        IcloudMailboxPagingReadRequest {
            page_url: server.url(),
            folder_id: "Sent".to_owned(),
            set_pause_ms: TASK_3072_SET_PAUSE_MS,
            stop_during_page: Some(2),
        },
    )
    .expect("direct iCloud page reader stops after page two");
    assert_eq!(stopped.stop_reason, MailPagingStopReason::StopRequested);
    assert_eq!(stopped.stop_requested_during_page, Some(2));
    assert_eq!(stopped.stopped_on_page_number, Some(2));
    assert!(stopped.stop_requested_during_run);
    assert!(
        (40..=80).contains(&stopped.message_count),
        "stop read {} messages",
        stopped.message_count
    );

    println!("TASK3072 direct_reader=read_icloud_mailbox_pages_for_scrub");
    println!("TASK3072 shared_reader=shared_mail_folder_page_reader");
    println!("TASK3072 provider=iCloud Mail");
    println!("TASK3072 folder_id={}", full.folder_id);
    println!("TASK3072 folder_message_count={TASK_3072_MESSAGE_COUNT}");
    println!("TASK3072 first_page_sent_count={}", seeded.sent.len());
    println!("TASK3072 first_page_owner_marker=SCRUB-IC-MINE");
    println!(
        "TASK3072 first_page_owner_label={}",
        seeded
            .sent
            .iter()
            .find(|message| message.owner_marker == "SCRUB-IC-MINE")
            .map(|message| if message.yours { "yours" } else { "not_yours" })
            .unwrap_or("missing")
    );
    println!("TASK3072 set_pause_ms={}", full.set_pause_ms);
    println!("TASK3072 full_read_message_count={}", full.message_count);
    println!("TASK3072 full_page_count={}", full.page_count);
    println!("TASK3072 full_stop_reason={:?}", full.stop_reason);
    println!(
        "TASK3072 full_one_screen_scrolls={}",
        full.one_screen_scrolls
    );
    println!("TASK3072 full_action_count={}", full.action_names.len());
    println!("TASK3072 full_action_names={}", full.action_names.join(","));
    println!(
        "TASK3072 full_inter_action_gaps_ms={:?}",
        full.inter_action_gaps_ms
    );
    println!(
        "TASK3072 full_every_inter_action_gap_at_least_set_pause={}",
        full.inter_action_gaps_ms
            .iter()
            .all(|gap| *gap >= TASK_3072_SET_PAUSE_MS)
    );
    println!(
        "TASK3072 stop_requested_during_page={}",
        stopped.stop_requested_during_page.unwrap_or_default()
    );
    println!(
        "TASK3072 stop_requested_during_run={}",
        stopped.stop_requested_during_run
    );
    println!("TASK3072 stop_reason={:?}", stopped.stop_reason);
    println!(
        "TASK3072 stopped_on_page_number={}",
        stopped.stopped_on_page_number.unwrap_or_default()
    );
    println!("TASK3072 stop_read_message_count={}", stopped.message_count);
    println!("TASK3072 stop_page_count={}", stopped.page_count);
    println!(
        "TASK3072 stop_message_count_between_40_and_80={}",
        (40..=80).contains(&stopped.message_count)
    );
}

#[test]
fn task_1426_direct_progress_command_changes_after_each_fixture_action() {
    let server = LocalTestPage::spawn_body(
        TASK_1426_TITLE,
        r#"
            <main>
              <section
                data-osl-live-run-progress
                data-osl-active-account="fixture-account-1426@example.invalid"
                data-osl-current-place="Inbox"
                data-osl-messages-checked="0"
                data-osl-matches="0"
                data-osl-scrolls="0"
                data-osl-waits="0"
                data-osl-changes="0">
                <button type="button" id="check-message">Check message</button>
                <button type="button" id="match-protected">Match protected</button>
                <button type="button" id="scroll-history">Scroll history</button>
                <button type="button" id="wait-settle">Wait settle</button>
              </section>
              <script>
                const progress = document.querySelector('[data-osl-live-run-progress]');
                const number = (name) => Number.parseInt(progress.dataset[name] || '0', 10) || 0;
                const setProgress = (place, patch) => {
                  progress.dataset.oslCurrentPlace = place;
                  progress.dataset.oslChanges = String(number('oslChanges') + 1);
                  for (const [key, value] of Object.entries(patch)) {
                    progress.dataset[key] = String(value);
                  }
                };
                document.getElementById('check-message').addEventListener('click', () => {
                  setProgress('Read pane', {
                    oslMessagesChecked: number('oslMessagesChecked') + 1,
                    oslWaits: number('oslWaits') + 1
                  });
                });
                document.getElementById('match-protected').addEventListener('click', () => {
                  setProgress('Review matches', {
                    oslMatches: number('oslMatches') + 1
                  });
                });
                document.getElementById('scroll-history').addEventListener('click', () => {
                  setProgress('Older messages', {
                    oslScrolls: number('oslScrolls') + 1
                  });
                });
                document.getElementById('wait-settle').addEventListener('click', () => {
                  setProgress('Settled wait', {
                    oslWaits: number('oslWaits') + 1
                  });
                });
              </script>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local live-progress fixture page through real browser");

    let opened = driver
        .read_live_run_progress(&page)
        .expect("read initial live run progress");
    assert_progress(
        &opened,
        "Inbox",
        (0, 0, 0, 0, 0),
        "initial progress comes from the fixture page",
    );
    print_task_1426_progress("after_open", &opened);

    press_fixture_action(&mut driver, &page, "Check message");
    let checked = driver
        .read_live_run_progress(&page)
        .expect("read live run progress after checking one message");
    assert_ne!(opened, checked);
    assert_progress(
        &checked,
        "Read pane",
        (1, 0, 0, 1, 1),
        "checking a message changes messages checked, waits, and changes",
    );
    print_task_1426_progress("after_check_message", &checked);

    press_fixture_action(&mut driver, &page, "Match protected");
    let matched = driver
        .read_live_run_progress(&page)
        .expect("read live run progress after matching a message");
    assert_ne!(checked, matched);
    assert_progress(
        &matched,
        "Review matches",
        (1, 1, 0, 1, 2),
        "matching changes matches and changes",
    );
    print_task_1426_progress("after_match_protected", &matched);

    press_fixture_action(&mut driver, &page, "Scroll history");
    let scrolled = driver
        .read_live_run_progress(&page)
        .expect("read live run progress after scrolling");
    assert_ne!(matched, scrolled);
    assert_progress(
        &scrolled,
        "Older messages",
        (1, 1, 1, 1, 3),
        "scrolling changes scrolls and changes",
    );
    print_task_1426_progress("after_scroll_history", &scrolled);

    press_fixture_action(&mut driver, &page, "Wait settle");
    let waited = driver
        .read_live_run_progress(&page)
        .expect("read live run progress after waiting");
    assert_ne!(scrolled, waited);
    assert_progress(
        &waited,
        "Settled wait",
        (1, 1, 1, 2, 4),
        "waiting changes waits and changes",
    );
    print_task_1426_progress("after_wait_settle", &waited);

    println!("TASK1426 direct_progress_command=read_live_run_progress");
    println!("TASK1426 active_account={TASK_1426_ACCOUNT}");
    println!("TASK1426 current_place={}", waited.current_place);
    println!("TASK1426 messages_checked={}", waited.messages_checked);
    println!("TASK1426 matches={}", waited.matches);
    println!("TASK1426 scrolls={}", waited.scrolls);
    println!("TASK1426 waits={}", waited.waits);
    println!("TASK1426 changes={}", waited.changes);
}

#[test]
fn task_3603_direct_interrupted_send_saves_one_id_and_five_step_progress() {
    let server = LocalTestPage::spawn_body(
        TASK_3603_TITLE,
        r#"
            <main>
              <label id="compose-label" for="compose">Compose</label>
              <textarea id="compose" aria-labelledby="compose-label"></textarea>
              <section
                data-osl-live-run-progress
                data-osl-active-account="ordinary-send@example.invalid"
                data-osl-current-place="Compose"
                data-osl-messages-checked="0"
                data-osl-matches="0"
                data-osl-scrolls="0"
                data-osl-waits="0"
                data-osl-changes="0">
                <button type="button" id="save-local">Save local</button>
                <button type="button" id="publish">Publish to receiver</button>
                <button type="button" id="confirm">Confirm final</button>
              </section>
              <script>
                const progress = document.querySelector('[data-osl-live-run-progress]');
                const changed = (place) => {
                  progress.dataset.oslCurrentPlace = place;
                  progress.dataset.oslChanges = String(Number(progress.dataset.oslChanges || '0') + 1);
                };
                document.getElementById('compose').addEventListener('input', () => changed('Service accepted'));
                document.getElementById('save-local').addEventListener('click', () => changed('Local saved'));
                document.getElementById('publish').addEventListener('click', () => changed('Receiver published'));
                document.getElementById('confirm').addEventListener('click', () => changed('Final confirmed'));
              </script>
            </main>
        "#,
    );
    let directory = tempfile::tempdir().expect("task 3603 progress directory");
    let progress_path = directory.path().join("ordinary-send-progress.json");

    let interrupted = run_task_3603_send(&server, &progress_path, Some(3));
    assert_task_3603_shape(&interrupted, false);
    assert_completed_steps(&interrupted, &TASK_3603_STEPS[..3]);
    print_task_3603_progress("interrupted", &interrupted);

    let restarted = read_ordinary_send_progress(&task_3603_request(&server, &progress_path, None))
        .expect("restart reads saved ordinary-send progress");
    assert_eq!(restarted.send_id, interrupted.send_id);
    assert_task_3603_shape(&restarted, false);
    assert_completed_steps(&restarted, &TASK_3603_STEPS[..3]);
    print_task_3603_progress("after_restart", &restarted);

    let four_steps = run_task_3603_send(&server, &progress_path, Some(4));
    assert_eq!(four_steps.send_id, interrupted.send_id);
    assert_task_3603_shape(&four_steps, false);
    assert_completed_steps(&four_steps, &TASK_3603_STEPS[..4]);
    print_task_3603_progress("after_four_steps", &four_steps);

    let finished = run_task_3603_send(&server, &progress_path, Some(5));
    assert_eq!(finished.send_id, interrupted.send_id);
    assert_task_3603_shape(&finished, true);
    assert_completed_steps(&finished, &TASK_3603_STEPS);
    print_task_3603_progress("finished", &finished);

    println!("TASK3603 direct_interrupted_send=send_ordinary_message_with_progress");
    println!("TASK3603 stable_send_id={}", interrupted.send_id);
    println!("TASK3603 restart_send_id={}", restarted.send_id);
    println!(
        "TASK3603 same_id_after_restart={}",
        restarted.send_id == interrupted.send_id
    );
    println!("TASK3603 named_step_count={}", interrupted.steps.len());
    for step in &TASK_3603_STEPS {
        println!("TASK3603 named_step={step}");
    }
    println!(
        "TASK3603 completed_after_interruption={}",
        interrupted.completed_step_names().len()
    );
    println!(
        "TASK3603 completed_after_restart={}",
        restarted.completed_step_names().len()
    );
    println!(
        "TASK3603 completed_after_finish={}",
        finished.completed_step_names().len()
    );
    println!(
        "TASK3603 final_confirmation_after_interruption={}",
        interrupted.final_confirmation
    );
    println!(
        "TASK3603 final_confirmation_after_four_steps={}",
        four_steps.final_confirmation
    );
    println!(
        "TASK3603 final_confirmation_after_finish={}",
        finished.final_confirmation
    );
}

struct LocalTestPage {
    listener_addr: String,
    running: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

fn task_3072_icloud_fixture_body() -> String {
    let mut rows = String::new();
    for index in 0..TASK_3072_MESSAGE_COUNT {
        let marker = if index == 0 {
            r#" data-osl-scrub-owner-marker="SCRUB-IC-MINE""#
        } else {
            ""
        };
        rows.push_str(&format!(
            r#"<article
                  data-osl-mail-message
                  data-osl-folder="Sent"
                  data-osl-subject="iCloud scrub fixture message {number:03}"
                  data-osl-time="2026-08-06 12:{minute:02}"
                  data-osl-sender="scrub-owner@icloud.test"{marker}>
                  <h2 data-osl-mail-subject>iCloud scrub fixture message {number:03}</h2>
                  <span data-osl-mail-time>2026-08-06 12:{minute:02}</span>
                  <span data-osl-mail-sender>scrub-owner@icloud.test</span>
                </article>"#,
            number = index + 1,
            minute = index % 60,
            marker = marker,
        ));
    }

    format!(
        r#"
            <main>
              <nav aria-label="iCloud folders">
                <button type="button" data-osl-mail-folder="Inbox">Inbox</button>
                <button type="button" data-osl-mail-folder="Sent">Sent</button>
                <button type="button" data-osl-mail-folder="Archive">Archive</button>
                <button type="button" data-osl-mail-folder="Trash">Trash</button>
              </nav>
              <section id="mailbox" aria-label="Seeded iCloud messages">
                {rows}
              </section>
              <button type="button" id="next-page">Next page</button>
              <script>
                const pageSize = {page_size};
                let page = 0;
                const rows = Array.from(document.querySelectorAll('[data-osl-mail-message]'));
                const next = document.getElementById('next-page');
                const render = () => {{
                  rows.forEach((row, index) => {{
                    row.hidden = index < page * pageSize || index >= (page + 1) * pageSize;
                  }});
                  next.hidden = (page + 1) * pageSize >= rows.length;
                }};
                next.addEventListener('click', () => {{
                  page += 1;
                  render();
                }});
                render();
              </script>
            </main>
        "#,
        rows = rows,
        page_size = TASK_3072_PAGE_SIZE,
    )
}

fn run_task_3603_send(
    server: &LocalTestPage,
    progress_path: &std::path::Path,
    max_steps: Option<usize>,
) -> OrdinarySendProgress {
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
    send_ordinary_message_with_progress(
        &mut driver,
        task_3603_request(server, progress_path, max_steps),
    )
    .expect("direct ordinary send command records progress")
}

fn task_3603_request(
    server: &LocalTestPage,
    progress_path: &std::path::Path,
    max_steps: Option<usize>,
) -> OrdinarySendProgressRequest {
    OrdinarySendProgressRequest {
        page_url: server.url(),
        draft_text: TASK_3603_DRAFT.to_owned(),
        progress_path: progress_path.to_path_buf(),
        max_steps,
    }
}

fn assert_task_3603_shape(progress: &OrdinarySendProgress, final_confirmation: bool) {
    let names = progress
        .steps
        .iter()
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, TASK_3603_STEPS);
    assert_eq!(progress.steps.len(), 5);
    assert_eq!(progress.final_confirmation, final_confirmation);
}

fn assert_completed_steps(progress: &OrdinarySendProgress, expected: &[&str]) {
    assert_eq!(progress.completed_step_names(), expected);
}

fn print_task_3603_progress(stage: &str, progress: &OrdinarySendProgress) {
    println!(
        "TASK3603 {stage} send_id={} completed={} final_confirmation={}",
        progress.send_id,
        progress.completed_step_names().len(),
        progress.final_confirmation
    );
    for step in &progress.steps {
        println!(
            "TASK3603 {stage} step={} completed={}",
            step.name, step.completed
        );
    }
}

fn press_fixture_action(
    driver: &mut RealBrowserWebsiteDriver,
    page: &osl_privacy_hub::website_driver::WebsitePage,
    name: &str,
) {
    driver
        .press_named_control(WebsiteNamedControl {
            page: page.clone(),
            name: name.to_owned(),
        })
        .unwrap_or_else(|error| panic!("press fixture action {name}: {error}"));
}

fn assert_progress(
    progress: &WebsiteLiveRunProgress,
    place: &str,
    counts: (usize, usize, usize, usize, usize),
    context: &str,
) {
    assert_eq!(progress.active_account, TASK_1426_ACCOUNT, "{context}");
    assert_eq!(progress.current_place, place, "{context}");
    assert_eq!(progress.messages_checked, counts.0, "{context}");
    assert_eq!(progress.matches, counts.1, "{context}");
    assert_eq!(progress.scrolls, counts.2, "{context}");
    assert_eq!(progress.waits, counts.3, "{context}");
    assert_eq!(progress.changes, counts.4, "{context}");
}

fn print_task_1426_progress(stage: &str, progress: &WebsiteLiveRunProgress) {
    println!(
        "TASK1426 {stage} active_account={} current_place={} messages_checked={} matches={} scrolls={} waits={} changes={}",
        progress.active_account,
        progress.current_place,
        progress.messages_checked,
        progress.matches,
        progress.scrolls,
        progress.waits,
        progress.changes
    );
}

impl LocalTestPage {
    fn spawn(title: &'static str) -> Self {
        Self::spawn_body(title, "")
    }

    fn spawn_body(title: impl Into<String>, body: impl Into<String>) -> Self {
        let title = title.into();
        let body = body.into();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local test page");
        listener
            .set_nonblocking(true)
            .expect("make local test page listener nonblocking");
        let listener_addr = listener
            .local_addr()
            .expect("local test page address")
            .to_string();
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let worker = thread::spawn(move || {
            while worker_running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => serve_page(stream, &title, &body),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            listener_addr,
            running,
            worker: Some(worker),
        }
    }

    fn url(&self) -> String {
        format!("http://{}/task-1201.html", self.listener_addr)
    }
}

impl Drop for LocalTestPage {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.listener_addr);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_page(mut stream: TcpStream, title: &str, body: &str) {
    let mut request = [0_u8; 1024];
    let _ = stream.read(&mut request);
    let body = format!("<!doctype html><title>{title}</title><h1>{title}</h1>{body}");
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}
