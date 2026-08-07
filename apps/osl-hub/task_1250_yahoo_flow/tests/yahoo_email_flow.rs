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
use task_1250_yahoo_flow::website_driver::{
    RealBrowserWebsiteDriver, WebsiteControlKind, WebsiteDriver, WebsiteDriverError,
    WebsiteNamedControl, WebsitePage, WebsitePageRequest, WebsiteTextPlacement,
};

const TASK_1250_TITLE: &str = "OSL Task 1250 Yahoo Mail Fixture";
const TASK_1250_WORDS: &str = "OSL-YAHOO-1250 words";

#[test]
fn task_1250_fake_page_yahoo_email_flow() {
    let server = LocalYahooPage::spawn();
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local Yahoo fixture through real browser");

    let initial = driver
        .read_page(&page)
        .expect("read initial Yahoo fixture controls");
    let initial_sent = count_after_label(&initial.text, "Sent emails: ").expect("initial sent");
    assert_eq!(initial.title, TASK_1250_TITLE);
    assert_eq!(initial_sent, 0);
    assert!(initial
        .controls
        .editable_boxes
        .iter()
        .any(|name| name == "Compose"));
    for required in ["Compose", "Place", "Readback", "Send"] {
        assert!(
            initial.controls.buttons.iter().any(|name| name == required),
            "missing button {required:?} in {:?}",
            initial.controls.buttons
        );
    }

    press(&mut driver, &page, "Compose").expect("press Compose");
    let after_compose = driver.read_page(&page).expect("read after Compose");
    let compose_words =
        value_after_label(&after_compose.text, "Compose words: ").expect("compose words");
    assert_eq!(compose_words, TASK_1250_WORDS);

    let placement = driver
        .place_text(WebsiteTextPlacement {
            page: page.clone(),
            text: TASK_1250_WORDS.to_owned(),
        })
        .expect("place exact Yahoo words");
    assert_eq!(placement.editable_box_name, "Compose");

    press(&mut driver, &page, "Place").expect("press Place");
    let after_place = driver.read_page(&page).expect("read after Place");
    let placed_message =
        value_after_label(&after_place.text, "Placed message: ").expect("placed message");
    let placed_count =
        count_after_label(&after_place.text, "Placed-message count: ").expect("placed count");
    assert_eq!(placed_message, TASK_1250_WORDS);
    assert_eq!(placed_count, 1);

    press(&mut driver, &page, "Readback").expect("press Readback");
    let after_readback = driver.read_page(&page).expect("read after Readback");
    let readback_words =
        value_after_label(&after_readback.text, "Readback words: ").expect("readback words");
    assert_eq!(readback_words, TASK_1250_WORDS);

    press(&mut driver, &page, "Send").expect("press Send");
    let after_send = driver.read_page(&page).expect("read after Send");
    let sent_after_send =
        count_after_label(&after_send.text, "Sent emails: ").expect("sent after send");
    let sent_message = value_after_label(&after_send.text, "Sent message: ").expect("sent message");
    assert_eq!(sent_after_send, 1);
    assert_eq!(sent_message, TASK_1250_WORDS);

    press(&mut driver, &page, "Remove Send").expect("remove Send control");
    let before_refusal = driver.read_page(&page).expect("read before refusal");
    let placed_before_refusal =
        count_after_label(&before_refusal.text, "Placed-message count: ").expect("placed before");
    let sent_before_refusal =
        count_after_label(&before_refusal.text, "Sent emails: ").expect("sent before refusal");
    let refused = press(&mut driver, &page, "Send").expect_err("removed Send is refused");
    assert_eq!(refused, WebsiteDriverError::NamedControlNotFound);

    let after_refusal = driver.read_page(&page).expect("read after refusal");
    let placed_after_refusal =
        count_after_label(&after_refusal.text, "Placed-message count: ").expect("placed after");
    let sent_after_refusal =
        count_after_label(&after_refusal.text, "Sent emails: ").expect("sent after refusal");
    assert_eq!(placed_after_refusal, placed_before_refusal);
    assert_eq!(sent_after_refusal, sent_before_refusal);
    assert_eq!(placed_after_refusal, 1);
    assert_eq!(sent_after_refusal, 1);

    println!("TASK1250 fixture=Yahoo Mail fake page");
    println!("TASK1250 initial_sent_emails={initial_sent}");
    println!(
        "TASK1250 initial_editable_controls={}",
        initial.controls.editable_boxes.join(",")
    );
    println!(
        "TASK1250 initial_button_controls={}",
        initial.controls.buttons.join(",")
    );
    println!("TASK1250 compose_words={compose_words}");
    println!("TASK1250 placed_editable={}", placement.editable_box_name);
    println!("TASK1250 placed_message={placed_message}");
    println!("TASK1250 placed_message_count={placed_count}");
    println!("TASK1250 readback_words={readback_words}");
    println!("TASK1250 sent_message={sent_message}");
    println!("TASK1250 sent_emails_after_send={sent_after_send}");
    println!("TASK1250 remove_send_refusal={refused}");
    println!("TASK1250 placed_message_count_after_refusal={placed_after_refusal}");
    println!("TASK1250 sent_emails_after_refusal={sent_after_refusal}");
}

fn press(
    driver: &mut RealBrowserWebsiteDriver,
    page: &WebsitePage,
    name: &str,
) -> Result<(), WebsiteDriverError> {
    driver.press_named_control(WebsiteNamedControl {
        page: page.clone(),
        name: name.to_owned(),
        kind: WebsiteControlKind::Button,
    })
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
    let rest = text.split(label).nth(1)?;
    let end = rest
        .find(" | ")
        .or_else(|| rest.find('\n'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_owned())
}

struct LocalYahooPage {
    listener_addr: String,
    running: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalYahooPage {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local Yahoo fixture");
        listener
            .set_nonblocking(true)
            .expect("make Yahoo fixture listener nonblocking");
        let listener_addr = listener
            .local_addr()
            .expect("Yahoo fixture address")
            .to_string();
        let running = Arc::new(AtomicBool::new(true));
        let worker_running = Arc::clone(&running);
        let worker = thread::spawn(move || {
            while worker_running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => serve_yahoo_fixture(stream),
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
        format!("http://{}/task-1250-yahoo.html", self.listener_addr)
    }
}

impl Drop for LocalYahooPage {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.listener_addr);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_yahoo_fixture(mut stream: TcpStream) {
    let mut buffer = [0_u8; 1024];
    let _ = stream.read(&mut buffer);
    let remove_send_action = if std::env::var_os("TASK1250_BREAK_REMOVE_SEND").is_some() {
        ""
    } else {
        "document.getElementById('send-button')?.remove();"
    };
    let body = format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>{TASK_1250_TITLE}</title></head>
<body>
  <main aria-label="Yahoo Mail fake page">
    <p>Sent emails: <span id="sent-count">0</span> | Placed-message count: <span id="placed-count">0</span></p>
    <p>Compose words: <span id="compose-words"></span> | Placed message: <span id="placed-message"></span> |</p>
    <p>Readback words: <span id="readback-words"></span> | Sent message: <span id="sent-message"></span> |</p>
    <button type="button" id="compose-button">Compose</button>
    <label id="compose-label" for="compose-box">Compose</label>
    <textarea id="compose-box" aria-labelledby="compose-label"></textarea>
    <button type="button" id="place-button">Place</button>
    <button type="button" id="readback-button">Readback</button>
    <button type="button" id="send-button">Send</button>
    <button type="button" id="remove-send-button">Remove Send</button>
    <section role="log" aria-label="Yahoo sent emails" id="sent-log"></section>
  </main>
  <script>
    const words = {words_json};
    const box = document.getElementById('compose-box');
    const sentCount = document.getElementById('sent-count');
    const placedCount = document.getElementById('placed-count');
    document.getElementById('compose-button').addEventListener('click', () => {{
      box.value = words;
      box.dispatchEvent(new InputEvent('input', {{ bubbles: true, inputType: 'insertText', data: words }}));
      document.getElementById('compose-words').textContent = box.value;
    }});
    document.getElementById('place-button').addEventListener('click', () => {{
      document.getElementById('placed-message').textContent = box.value;
      placedCount.textContent = box.value === words ? '1' : '0';
    }});
    document.getElementById('readback-button').addEventListener('click', () => {{
      document.getElementById('readback-words').textContent = box.value;
    }});
    document.getElementById('send-button').addEventListener('click', () => {{
      const message = document.getElementById('readback-words').textContent;
      if (message !== words) return;
      sentCount.textContent = String(Number(sentCount.textContent) + 1);
      document.getElementById('sent-message').textContent = message;
      const row = document.createElement('article');
      row.textContent = message;
      document.getElementById('sent-log').appendChild(row);
    }});
    document.getElementById('remove-send-button').addEventListener('click', () => {{
      {remove_send_action}
    }});
  </script>
</body>
</html>"#,
        words_json = serde_json::to_string(TASK_1250_WORDS).expect("json words"),
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}
