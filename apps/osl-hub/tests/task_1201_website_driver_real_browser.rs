use osl_privacy_hub::website_driver::{
    RealBrowserWebsiteDriver, WebsiteDriver, WebsiteNamedControl, WebsitePageRequest,
    WebsiteSendCommand, WebsiteTextPlacement,
    RealBrowserWebsiteDriver, WebsiteDriver, WebsiteDriverError, WebsiteNamedControl,
    WebsitePageRequest, WebsiteSendCommand, WebsiteTextPlacement,
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
const TASK_1216_TITLE: &str = "OSL Task 1216 Press Named Button";
const TASK_1217_TITLE: &str = "OSL Task 1217 Send After Proof";
const TASK_1217_MESSAGE: &str = "MAPLE-1217";
const TASK_1218_TITLE: &str = "OSL Task 1218 Disabled Send";
const TASK_1218_MESSAGE: &str = "MAPLE-4172";

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
fn task_1216_named_send_command_makes_fixture_send_counter_one() {
    let server = LocalTestPage::spawn_body(
        TASK_1216_TITLE,
        r#"
            <main>
              <p id="counter" aria-live="polite">Send counter: <span id="send-count">0</span></p>
              <button type="button" onclick="
                const node = document.getElementById('send-count');
                node.textContent = String(Number(node.textContent) + 1);
              ">Send</button>
              <button type="button" hidden onclick="
                document.getElementById('send-count').textContent = '99';
              ">Send</button>
              <button type="button" disabled onclick="
                document.getElementById('send-count').textContent = '99';
              ">Send</button>
              <button type="button" onclick="
                document.getElementById('send-count').textContent = '42';
              ">Archive</button>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local test page through real browser");
    let before = driver
        .read_page(&page)
        .expect("read fixture before pressing Send");
    assert_eq!(fixture_send_counter(&before.text), Some(0));

    driver
        .press_named_control(WebsiteNamedControl {
            page: page.clone(),
            name: "Send".to_owned(),
        })
        .expect("press the one named visible enabled Send button");

    let after = driver
        .read_page(&page)
        .expect("read fixture after pressing Send");
    assert_eq!(fixture_send_counter(&after.text), Some(1));

    println!("TASK1216 direct_driver_command=press_named_control");
    println!("TASK1216 named_command=Send");
    println!(
        "TASK1216 fixture_send_counter_before={}",
        fixture_send_counter(&before.text).expect("counter before")
    );
    println!(
        "TASK1216 fixture_send_counter_after={}",
        fixture_send_counter(&after.text).expect("counter after")
    );
}

#[test]
fn task_1217_direct_send_command_places_once_then_presses_send_once() {
    let server = LocalTestPage::spawn_body(
        TASK_1217_TITLE,
        r#"
            <main>
              <label id="body-label" for="body">Body</label>
              <textarea id="body" aria-labelledby="body-label"></textarea>
              <p aria-live="polite">Placement proof count: <span id="proof-count">0</span></p>
              <p aria-live="polite">Send press count: <span id="send-count">0</span></p>
              <p aria-live="polite">Event log: <span id="event-log"></span></p>
              <p aria-live="polite">Sent message: <span id="sent-message"></span></p>
              <p>Fixture end</p>
              <button type="button" onclick="
                const count = document.getElementById('send-count');
                const log = document.getElementById('event-log');
                const body = document.getElementById('body');
                count.textContent = String(Number(count.textContent) + 1);
                log.textContent = log.textContent ? log.textContent + '>Send press' : 'Send press';
                document.getElementById('sent-message').textContent = body.value;
              ">Send</button>
              <script>
                document.getElementById('body').addEventListener('input', () => {
                  const body = document.getElementById('body');
                  if (body.value !== 'MAPLE-1217') return;
                  const count = document.getElementById('proof-count');
                  const log = document.getElementById('event-log');
                  count.textContent = String(Number(count.textContent) + 1);
                  log.textContent = log.textContent ? log.textContent + '>placement proof' : 'placement proof';
                });
              </script>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local test page through real browser");
    let before = driver
        .read_page(&page)
        .expect("read fixture before direct send command");
    assert_eq!(
        count_after_label(&before.text, "Placement proof count: "),
        Some(0)
    );
    assert_eq!(
        count_after_label(&before.text, "Send press count: "),
        Some(0)
    );

    let receipt = driver
        .send_after_successful_placement(WebsiteSendCommand {
            placement: WebsiteTextPlacement {
                page: page.clone(),
                editable_box_name: "Body".to_owned(),
                text: TASK_1217_MESSAGE.to_owned(),
            },
            send_control_name: "Send".to_owned(),
        })
        .expect("direct send places text before pressing Send");

    let after = driver
        .read_page(&page)
        .expect("read fixture after direct send command");
    let placement_proofs =
        count_after_label(&after.text, "Placement proof count: ").expect("placement proof count");
    let send_presses = count_after_label(&after.text, "Send press count: ").expect("send count");
    let event_log = value_after_label(&after.text, "Event log: ").expect("event log");
    let sent_message = value_after_label(&after.text, "Sent message: ").expect("sent message");

    assert_eq!(receipt.placement_proof.editable_box_name, "Body");
    assert_eq!(receipt.placement_proof.utf16_units, TASK_1217_MESSAGE.len());
    assert_eq!(receipt.send_control_name, "Send");
    assert!(receipt.send_pressed);
    assert_eq!(placement_proofs, 1);
    assert_eq!(send_presses, 1);
    assert_eq!(event_log, "placement proof>Send press");
    assert_eq!(sent_message, TASK_1217_MESSAGE);

    println!("TASK1217 direct_send_command=send_after_successful_placement");
    println!("TASK1217 placement_proof_count={placement_proofs}");
    println!("TASK1217 placement_proof_editable=Body");
    println!("TASK1217 named_send_control=Send");
    println!("TASK1217 send_press_count={send_presses}");
    println!("TASK1217 event_order={event_log}");
    println!("TASK1217 sent_message={sent_message}");
}

#[test]
fn task_1218_disabled_send_is_not_bypassed_after_first_send() {
    let server = LocalTestPage::spawn_body(
        TASK_1218_TITLE,
        r#"
            <main>
              <label id="body-label" for="body">Body</label>
              <textarea id="body" aria-labelledby="body-label"></textarea>
              <p aria-live="polite">Send enabled: <span id="send-enabled">yes</span></p>
              <p aria-live="polite">Sent-message count: <span id="sent-message-count">0</span></p>
              <p aria-live="polite">Result name: <span id="result-name"></span></p>
              <p aria-live="polite">First message: <span id="first-message"></span></p>
              <p>Fixture end</p>
              <button id="send" type="button" onclick="
                const body = document.getElementById('body');
                const count = document.getElementById('sent-message-count');
                count.textContent = String(Number(count.textContent) + 1);
                document.getElementById('result-name').textContent = 'sent ' + body.value;
                const first = document.getElementById('first-message');
                if (!first.textContent) first.textContent = body.value;
                this.disabled = true;
                document.getElementById('send-enabled').textContent = 'no';
              ">Send</button>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local test page through real browser");
    let before = driver
        .read_page(&page)
        .expect("read fixture before the first send");
    let sent_before = count_after_label(&before.text, "Sent-message count: ").expect("sent before");
    assert_eq!(sent_before, 0);
    assert_eq!(
        value_after_label(&before.text, "Send enabled: ").expect("send enabled before"),
        "yes"
    );

    let first_receipt = driver
        .send_after_successful_placement(WebsiteSendCommand {
            placement: WebsiteTextPlacement {
                page: page.clone(),
                editable_box_name: "Body".to_owned(),
                text: TASK_1218_MESSAGE.to_owned(),
            },
            send_control_name: "Send".to_owned(),
        })
        .expect("enabled Send accepts the first message");
    assert!(first_receipt.send_pressed);

    let after_first = driver
        .read_page(&page)
        .expect("read fixture after the enabled send");
    let sent_after_first =
        count_after_label(&after_first.text, "Sent-message count: ").expect("sent after first");
    let first_result = value_after_label(&after_first.text, "Result name: ").expect("result name");
    let send_enabled_after_first =
        value_after_label(&after_first.text, "Send enabled: ").expect("send enabled after first");
    assert_eq!(sent_after_first, 1);
    assert_eq!(first_result, "sent MAPLE-4172");
    assert_eq!(send_enabled_after_first, "no");

    let disabled = driver
        .send_after_successful_placement(WebsiteSendCommand {
            placement: WebsiteTextPlacement {
                page: page.clone(),
                editable_box_name: "Body".to_owned(),
                text: TASK_1218_MESSAGE.to_owned(),
            },
            send_control_name: "Send".to_owned(),
        })
        .expect_err("disabled Send refuses the retry");
    assert_eq!(disabled, WebsiteDriverError::NamedControlDisabled);
    assert_eq!(disabled.to_string(), "Send disabled");

    let after_disabled = driver
        .read_page(&page)
        .expect("read fixture after disabled Send refusal");
    let sent_after_disabled = count_after_label(&after_disabled.text, "Sent-message count: ")
        .expect("sent after disabled");
    let first_message =
        value_after_label(&after_disabled.text, "First message: ").expect("first message");
    assert_eq!(sent_after_disabled, 1);
    assert_eq!(first_message, TASK_1218_MESSAGE);
    assert_eq!(
        value_after_label(&after_disabled.text, "Result name: ").expect("result stays"),
        "sent MAPLE-4172"
    );

    println!("TASK1218 sent_message_count_before={sent_before}");
    println!("TASK1218 first_result={first_result}");
    println!("TASK1218 sent_message_count_after_first={sent_after_first}");
    println!("TASK1218 send_enabled_after_first={send_enabled_after_first}");
    println!("TASK1218 disabled_refusal={disabled}");
    println!("TASK1218 first_message_after_refusal={first_message}");
    println!("TASK1218 sent_message_count_after_refusal={sent_after_disabled}");
}

fn fixture_send_counter(text: &str) -> Option<u32> {
    text.split("Send counter: ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn count_after_label(text: &str, label: &str) -> Option<u32> {
    text.split(label)
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn value_after_label(text: &str, label: &str) -> Option<String> {
    let value = text.split(label).nth(1)?.trim();
    let next_label = value
        .find("Send counter: ")
        .or_else(|| value.find("Placement proof count: "))
        .or_else(|| value.find("Send press count: "))
        .or_else(|| value.find("Send enabled: "))
        .or_else(|| value.find("Sent-message count: "))
        .or_else(|| value.find("Result name: "))
        .or_else(|| value.find("First message: "))
        .or_else(|| value.find("Event log: "))
        .or_else(|| value.find("Sent message: "))
        .or_else(|| value.find("Fixture end"))
        .unwrap_or(value.len());
    Some(value[..next_label].trim().to_owned())
}

struct LocalTestPage {
    listener_addr: String,
    running: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
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
