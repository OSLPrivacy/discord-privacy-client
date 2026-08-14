use osl_privacy_hub::{
    hub_command_surface::{
        read_ordinary_send_progress, read_protected_email_open_message_with_driver,
        send_ordinary_message_with_progress, OrdinarySendProgress, OrdinarySendProgressRequest,
        ProtectedEmailOpenMessageReadRequest,
    },
    website_driver::{
        RealBrowserWebsiteDriver, WebsiteDriver, WebsiteDriverError, WebsiteLiveRunProgress,
        WebsiteNamedControl, WebsitePage, WebsitePageControls, WebsitePageRequest, WebsitePageText,
        WebsitePlacementProof, WebsiteSelectedEmail, WebsiteTextPlacement,
    },
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
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
const TASK_1426_TITLE: &str = "OSL Task 1426 Live Run Progress";
const TASK_1426_ACCOUNT: &str = "fixture-account-1426@example.invalid";
const TASK_3418_TITLE: &str = "OSL Task 3418 Email Compose Placement";
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
fn task_3418_real_browser_places_random_marked_message_in_empty_email_compose_box() {
    let server = LocalTestPage::spawn_body(
        TASK_3418_TITLE,
        r#"
            <main>
              <label id="body-label" for="body">Body</label>
              <textarea id="body" aria-labelledby="body-label"></textarea>
              <p>Box empty: <span id="box-empty">true</span></p>
              <p>Box value: <span id="box-value"></span></p>
              <script>
                const body = document.getElementById('body');
                const empty = document.getElementById('box-empty');
                const value = document.getElementById('box-value');
                const sync = () => {
                  empty.textContent = body.value === '' ? 'true' : 'false';
                  value.textContent = body.value;
                };
                body.addEventListener('input', sync);
                body.addEventListener('change', sync);
                sync();
              </script>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
    let mark = random_task_3418_mark();

    let checked = run_task_3418_place_mark(&mut driver, server.url(), &mark)
        .expect("shared place_text job writes the mark into the compose box");

    assert!(checked.before_empty);
    assert_eq!(checked.after_value, mark);
    assert_eq!(checked.placement_editable, "Body");

    println!("TASK3418 direct_driver_command=place_text");
    println!(
        "TASK3418 browser_executable={}",
        driver.browser_executable().display()
    );
    println!("TASK3418 before_box_empty={}", checked.before_empty);
    println!("TASK3418 random_mark={mark}");
    println!("TASK3418 after_box_value={}", checked.after_value);
    println!("TASK3418 placement_editable={}", checked.placement_editable);
}

#[test]
fn task_3418_noop_placing_job_makes_check_fail() {
    let mark = random_task_3418_mark();
    let mut driver = NoopTask3418Driver::new();

    let failure = run_task_3418_place_mark(
        &mut driver,
        "https://example.invalid/task-3418-noop".to_owned(),
        &mark,
    )
    .expect_err("stubbed placing job must leave the box without the mark");

    assert_eq!(
        failure,
        "compose box did not hold the exact mark after placement"
    );
    println!("TASK3418_STUB placing_job_stubbed=noop");
    println!("TASK3418_STUB random_mark={mark}");
    println!("TASK3418_STUB check_failed=true");
    println!("TASK3418_STUB failure={failure}");
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

#[test]
fn task_3604_restarts_retry_only_unfinished_ordinary_send_steps() {
    let directory = tempfile::tempdir().expect("task 3604 progress directory");
    let progress_path = directory.path().join("ordinary-send-progress.json");
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut send_ids = Vec::new();
    let mut success_before_receiver_publish = 0usize;
    let mut success_before_final_confirmation = 0usize;
    let mut success_first_seen_after_step = None::<String>;
    let mut finished = None::<OrdinarySendProgress>;

    for stop_after in 1..=TASK_3603_STEPS.len() {
        let mut driver = CountingOrdinarySendDriver::new(Arc::clone(&events));
        let progress = send_ordinary_message_with_progress(
            &mut driver,
            OrdinarySendProgressRequest {
                page_url: "https://ordinary-send.example.invalid/thread".to_owned(),
                draft_text: TASK_3603_DRAFT.to_owned(),
                progress_path: progress_path.clone(),
                max_steps: Some(stop_after),
            },
        )
        .expect("ordinary send restart run records progress");
        assert_task_3603_shape(&progress, stop_after == TASK_3603_STEPS.len());
        assert_completed_steps(&progress, &TASK_3603_STEPS[..stop_after]);

        let restarted = read_ordinary_send_progress(&OrdinarySendProgressRequest {
            page_url: "https://ordinary-send.example.invalid/thread".to_owned(),
            draft_text: TASK_3603_DRAFT.to_owned(),
            progress_path: progress_path.clone(),
            max_steps: None,
        })
        .expect("restart reads ordinary-send progress");
        assert_eq!(restarted, progress);

        let receiver_published = step_completed(&progress, "receiver_publish");
        if progress.final_confirmation && !receiver_published {
            success_before_receiver_publish += 1;
        }
        if progress.final_confirmation && !step_completed(&progress, "final_confirmation") {
            success_before_final_confirmation += 1;
        }
        if progress.final_confirmation && success_first_seen_after_step.is_none() {
            success_first_seen_after_step = progress
                .completed_step_names()
                .last()
                .map(|s| (*s).to_owned());
        }

        println!(
            "TASK3604 stop_after={} restart_send_id={} completed={} success={} receiver_publish_completed={}",
            stop_after,
            restarted.send_id,
            restarted.completed_step_names().len(),
            restarted.final_confirmation,
            receiver_published
        );
        send_ids.push(restarted.send_id.clone());
        finished = Some(restarted);
    }

    let finished = finished.expect("finished progress exists");
    let unique_send_ids = send_ids
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let events = events.lock().expect("counting driver events lock").clone();
    assert_eq!(unique_send_ids, 1);
    assert_eq!(
        events,
        [
            "service_acceptance",
            "local_save",
            "receiver_publish",
            "final_confirmation"
        ]
    );
    for step in &finished.steps {
        assert_eq!(step.run_count, 1, "{} must run exactly once", step.name);
    }
    assert_eq!(success_before_receiver_publish, 0);
    assert_eq!(success_before_final_confirmation, 0);
    assert_eq!(
        success_first_seen_after_step.as_deref(),
        Some("final_confirmation")
    );

    println!("TASK3604 stop_restart_count={}", TASK_3603_STEPS.len());
    println!("TASK3604 unique_send_id_count={unique_send_ids}");
    println!("TASK3604 send_id={}", finished.send_id);
    for step in &finished.steps {
        println!(
            "TASK3604 step={} completed={} run_count={}",
            step.name, step.completed, step.run_count
        );
    }
    println!("TASK3604 driver_event_count={}", events.len());
    for event in &events {
        println!("TASK3604 driver_event={event}");
    }
    println!("TASK3604 success_before_receiver_publish_count={success_before_receiver_publish}");
    println!(
        "TASK3604 success_before_final_confirmation_count={success_before_final_confirmation}"
    );
    println!(
        "TASK3604 success_first_seen_after_step={}",
        success_first_seen_after_step.expect("success appears after final confirmation")
    );
    println!(
        "TASK3604 success_appears_only_after_receiver_publish_finishes={}",
        step_completed(&finished, "receiver_publish")
    );
}

#[test]
fn task_3605_crash_after_every_ordinary_send_step_keeps_receiver_marks_once() {
    let directory = tempfile::tempdir().expect("task 3605 progress directory");
    let receiver = Arc::new(Mutex::new(Vec::<ReceiverMark>::new()));
    let control_mark = "TASK3605_CONTROL_MARK".to_owned();
    let control_before = receiver_count(&receiver);
    let control_progress_path = directory.path().join("ordinary-send-control.json");
    let control_progress = run_task_3605_marked_send(
        Arc::clone(&receiver),
        control_mark.clone(),
        &control_progress_path,
        None,
    )
    .expect("control ordinary send completes");
    let control_after = receiver_count(&receiver);
    assert_eq!(control_before, 0);
    assert_eq!(control_after, 1);
    assert_eq!(receiver_mark_count(&receiver, &control_mark), 1);
    assert!(receiver_marks_complete(&receiver));
    assert!(control_progress.final_confirmation);

    println!(
        "TASK3605 control receiver_before={} receiver_after={} mark={} mark_count={} success={}",
        control_before,
        control_after,
        control_mark,
        receiver_mark_count(&receiver, &control_mark),
        control_progress.final_confirmation
    );

    let mut crash_success_count = 0usize;
    let mut crash_readable_mark_count = 0usize;
    let mut crash_invariant_count = 0usize;

    for (index, step) in TASK_3603_STEPS.iter().enumerate() {
        let mark = format!("TASK3605_MARK_{}", step.to_ascii_uppercase());
        let progress_path = directory
            .path()
            .join(format!("ordinary-send-crash-{step}.json"));
        let before_crash_total = receiver_count(&receiver);
        let after_crash = run_task_3605_marked_send(
            Arc::clone(&receiver),
            mark.clone(),
            &progress_path,
            Some(index + 1),
        )
        .expect("ordinary send crash run saves bounded progress");
        assert_completed_steps(&after_crash, &TASK_3603_STEPS[..=index]);

        let after_crash_total = receiver_count(&receiver);
        let after_crash_mark_count = receiver_mark_count(&receiver, &mark);
        let added_once = after_crash_mark_count == 1 && after_crash_total == before_crash_total + 1;
        let unchanged = after_crash_mark_count == 0 && after_crash_total == before_crash_total;
        assert!(
            added_once || unchanged,
            "crash after {step} must either add its mark once or leave receiver count unchanged"
        );
        assert!(receiver_marks_complete(&receiver));
        crash_invariant_count += 1;

        let after_retry =
            run_task_3605_marked_send(Arc::clone(&receiver), mark.clone(), &progress_path, None)
                .expect("ordinary send retry completes after restart");
        assert_task_3603_shape(&after_retry, true);
        assert_completed_steps(&after_retry, &TASK_3603_STEPS);
        assert_eq!(receiver_mark_count(&receiver, &mark), 1);
        assert!(receiver_marks_complete(&receiver));
        if after_retry.final_confirmation {
            crash_success_count += 1;
        }
        crash_readable_mark_count += receiver_mark_count(&receiver, &mark);

        println!(
            "TASK3605 crash_step={} mark={} before_total={} after_crash_total={} after_crash_mark_count={} added_once={} unchanged={} after_retry_total={} final_mark_count={} readable_complete={} success={}",
            step,
            mark,
            before_crash_total,
            after_crash_total,
            after_crash_mark_count,
            added_once,
            unchanged,
            receiver_count(&receiver),
            receiver_mark_count(&receiver, &mark),
            receiver_marks_complete(&receiver),
            after_retry.final_confirmation
        );
    }

    let total_success_count =
        crash_success_count + usize::from(control_progress.final_confirmation);
    let total_readable_mark_count = receiver_count(&receiver);
    assert_eq!(crash_invariant_count, TASK_3603_STEPS.len());
    assert_eq!(crash_success_count, crash_readable_mark_count);
    assert_eq!(total_success_count, total_readable_mark_count);
    assert!(receiver_marks_complete(&receiver));

    println!("TASK3605 crash_step_count={crash_invariant_count}");
    println!("TASK3605 crash_success_count={crash_success_count}");
    println!("TASK3605 crash_readable_mark_count={crash_readable_mark_count}");
    println!("TASK3605 total_success_count={total_success_count}");
    println!("TASK3605 total_readable_mark_count={total_readable_mark_count}");
    println!(
        "TASK3605 every_readable_mark_complete={}",
        receiver_marks_complete(&receiver)
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Task3418Check {
    before_empty: bool,
    after_value: String,
    placement_editable: String,
}

fn run_task_3418_place_mark<D>(
    driver: &mut D,
    page_url: String,
    mark: &str,
) -> Result<Task3418Check, String>
where
    D: WebsiteDriver,
{
    let page = driver
        .find_page(WebsitePageRequest { url: page_url })
        .map_err(|error| error.to_string())?;
    let before = driver.read_page(&page).map_err(|error| error.to_string())?;
    let before_empty = task_3418_box_empty(&before.text)?;
    if !before_empty {
        return Err("compose box was not empty before placement".to_owned());
    }

    let placement = driver
        .place_text(WebsiteTextPlacement {
            page: page.clone(),
            editable_box_name: "Body".to_owned(),
            text: mark.to_owned(),
        })
        .map_err(|error| error.to_string())?;
    let after = driver.read_page(&page).map_err(|error| error.to_string())?;
    let after_value = task_3418_box_value(&after.text)?;
    if after_value != mark {
        return Err("compose box did not hold the exact mark after placement".to_owned());
    }

    Ok(Task3418Check {
        before_empty,
        after_value,
        placement_editable: placement.editable_box_name,
    })
}

fn task_3418_box_empty(text: &str) -> Result<bool, String> {
    match text
        .split("Box empty: ")
        .nth(1)
        .and_then(|value| value.split_whitespace().next())
    {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err("compose box empty state was not readable".to_owned()),
    }
}

fn task_3418_box_value(text: &str) -> Result<String, String> {
    text.split("Box value: ")
        .nth(1)
        .map(str::trim)
        .map(str::to_owned)
        .ok_or_else(|| "compose box value was not readable".to_owned())
}

fn random_task_3418_mark() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos();
    format!("TASK3418-MARK-{nanos}")
}

struct NoopTask3418Driver {
    page: Option<WebsitePage>,
}

impl NoopTask3418Driver {
    fn new() -> Self {
        Self { page: None }
    }
}

impl WebsiteDriver for NoopTask3418Driver {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        let page = WebsitePage::synthetic(request.url);
        self.page = Some(page.clone());
        Ok(page)
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        Ok(WebsitePageText {
            page: page.clone(),
            title: TASK_3418_TITLE.to_owned(),
            text: "Box empty: true Box value: ".to_owned(),
            controls: WebsitePageControls {
                editable_boxes: vec!["Body".to_owned()],
                buttons: Vec::new(),
                visible_message_areas: Vec::new(),
            },
        })
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: String::new(),
        })
    }

    fn press_named_control(
        &mut self,
        _control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        Err(WebsiteDriverError::NamedControlNotFound)
    }
}

struct LocalTestPage {
    listener_addr: String,
    running: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReceiverMark {
    mark: String,
    complete: bool,
}

struct CountingOrdinarySendDriver {
    events: Arc<Mutex<Vec<String>>>,
}

impl CountingOrdinarySendDriver {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }

    fn record(&self, event: &str) {
        self.events
            .lock()
            .expect("counting driver events lock")
            .push(event.to_owned());
    }
}

impl WebsiteDriver for CountingOrdinarySendDriver {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        Ok(WebsitePage::synthetic(request.url))
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        Ok(WebsitePageText {
            page: page.clone(),
            title: TASK_3603_TITLE.to_owned(),
            text: "ordinary send counting fixture".to_owned(),
            controls: WebsitePageControls::default(),
        })
    }

    fn read_selected_email(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn read_live_run_progress(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        assert_eq!(placement.text, TASK_3603_DRAFT);
        self.record("service_acceptance");
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: String::new(),
        })
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        let event = match control.name.as_str() {
            "Save local" => "local_save",
            "Publish to receiver" => "receiver_publish",
            "Confirm final" => "final_confirmation",
            _ => return Err(WebsiteDriverError::NamedControlNotFound),
        };
        self.record(event);
        Ok(())
    }
}

struct MarkingOrdinarySendDriver {
    receiver: Arc<Mutex<Vec<ReceiverMark>>>,
    mark: String,
}

impl MarkingOrdinarySendDriver {
    fn new(receiver: Arc<Mutex<Vec<ReceiverMark>>>, mark: String) -> Self {
        Self { receiver, mark }
    }
}

impl WebsiteDriver for MarkingOrdinarySendDriver {
    fn find_page(
        &mut self,
        request: WebsitePageRequest,
    ) -> Result<WebsitePage, WebsiteDriverError> {
        Ok(WebsitePage::synthetic(request.url))
    }

    fn read_page(&mut self, page: &WebsitePage) -> Result<WebsitePageText, WebsiteDriverError> {
        Ok(WebsitePageText {
            page: page.clone(),
            title: TASK_3603_TITLE.to_owned(),
            text: "ordinary send crash fixture".to_owned(),
            controls: WebsitePageControls::default(),
        })
    }

    fn read_selected_email(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteSelectedEmail, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn read_live_run_progress(
        &mut self,
        _page: &WebsitePage,
    ) -> Result<WebsiteLiveRunProgress, WebsiteDriverError> {
        Err(WebsiteDriverError::ReadFailed)
    }

    fn place_text(
        &mut self,
        placement: WebsiteTextPlacement,
    ) -> Result<WebsitePlacementProof, WebsiteDriverError> {
        assert_eq!(placement.text, self.mark);
        Ok(WebsitePlacementProof {
            page: placement.page,
            editable_box_name: placement.editable_box_name,
            utf16_units: placement.text.encode_utf16().count(),
            placed_sha256: String::new(),
        })
    }

    fn press_named_control(
        &mut self,
        control: WebsiteNamedControl,
    ) -> Result<(), WebsiteDriverError> {
        match control.name.as_str() {
            "Save local" => Ok(()),
            "Publish to receiver" => {
                self.receiver
                    .lock()
                    .expect("task 3605 receiver lock")
                    .push(ReceiverMark {
                        mark: self.mark.clone(),
                        complete: true,
                    });
                Ok(())
            }
            "Confirm final" => Ok(()),
            _ => Err(WebsiteDriverError::NamedControlNotFound),
        }
    }
}

fn run_task_3605_marked_send(
    receiver: Arc<Mutex<Vec<ReceiverMark>>>,
    mark: String,
    progress_path: &std::path::Path,
    max_steps: Option<usize>,
) -> Result<OrdinarySendProgress, String> {
    let mut driver = MarkingOrdinarySendDriver::new(receiver, mark.clone());
    send_ordinary_message_with_progress(
        &mut driver,
        OrdinarySendProgressRequest {
            page_url: "https://ordinary-send.example.invalid/task-3605".to_owned(),
            draft_text: mark,
            progress_path: progress_path.to_path_buf(),
            max_steps,
        },
    )
}

fn receiver_count(receiver: &Arc<Mutex<Vec<ReceiverMark>>>) -> usize {
    receiver.lock().expect("task 3605 receiver lock").len()
}

fn receiver_mark_count(receiver: &Arc<Mutex<Vec<ReceiverMark>>>, mark: &str) -> usize {
    receiver
        .lock()
        .expect("task 3605 receiver lock")
        .iter()
        .filter(|entry| entry.mark == mark)
        .count()
}

fn receiver_marks_complete(receiver: &Arc<Mutex<Vec<ReceiverMark>>>) -> bool {
    receiver
        .lock()
        .expect("task 3605 receiver lock")
        .iter()
        .all(|entry| entry.complete)
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

fn step_completed(progress: &OrdinarySendProgress, name: &str) -> bool {
    progress
        .steps
        .iter()
        .any(|step| step.name == name && step.completed)
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

    fn spawn_body(title: &'static str, body: &'static str) -> Self {
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
                    Ok((stream, _)) => serve_page(stream, title, body),
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
