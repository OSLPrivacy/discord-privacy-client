use osl_privacy_hub::website_driver::{
    RealBrowserWebsiteDriver, WebsiteDriver, WebsiteDriverError, WebsiteEmailDraft,
    WebsitePageRequest,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

const TASK_1221_BODY: &str = "MAPLE-4172";
const VALID_RECIPIENT: &str = "lee@example.com";
const MALFORMED_RECIPIENT: &str = "lee@";

#[derive(Clone, Debug)]
struct SentMessage {
    recipient: String,
    body: String,
}

#[test]
fn task_1221_malformed_recipient_stops_sending() {
    let server = LocalMailPage::spawn();
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");
    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local mail compose fixture through real browser");

    let before = server.messages_for(VALID_RECIPIENT);
    println!("TASK1221 before_sent_message_count={}", before.len());
    assert_eq!(before.len(), 0);

    let first_receipt = driver
        .send_email_draft(
            &page,
            WebsiteEmailDraft {
                recipient: VALID_RECIPIENT.to_owned(),
                body: Some(TASK_1221_BODY.to_owned()),
            },
        )
        .expect("send MAPLE-4172 to lee@example.com");
    assert_eq!(first_receipt.recipient, VALID_RECIPIENT);

    let after_valid = server.messages_for(VALID_RECIPIENT);
    let received = after_valid
        .first()
        .map(|message| message.body.as_str())
        .unwrap_or("");
    println!(
        "TASK1221 after_valid_recipient={} sent_message_count={} received_body={:?}",
        VALID_RECIPIENT,
        after_valid.len(),
        received
    );
    assert_eq!(after_valid.len(), 1);
    assert_eq!(received, TASK_1221_BODY);

    let malformed = driver.send_email_draft(
        &page,
        WebsiteEmailDraft {
            recipient: MALFORMED_RECIPIENT.to_owned(),
            body: None,
        },
    );
    println!(
        "TASK1221 changed_only_to={} refused_as={}",
        MALFORMED_RECIPIENT,
        WebsiteDriverError::MalformedRecipient
    );
    assert_eq!(malformed, Err(WebsiteDriverError::MalformedRecipient));

    let final_messages = server.messages_for(VALID_RECIPIENT);
    let final_body = final_messages
        .first()
        .map(|message| message.body.as_str())
        .unwrap_or("");
    println!(
        "TASK1221 final_recipient={} sent_message_count={} received_body={:?}",
        VALID_RECIPIENT,
        final_messages.len(),
        final_body
    );
    assert_eq!(final_messages.len(), 1);
    assert_eq!(final_body, TASK_1221_BODY);
}

struct LocalMailPage {
    listener_addr: String,
    running: Arc<AtomicBool>,
    messages: Arc<Mutex<Vec<SentMessage>>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalMailPage {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind local mail page");
        listener
            .set_nonblocking(true)
            .expect("make local mail page listener nonblocking");
        let listener_addr = listener
            .local_addr()
            .expect("local mail page address")
            .to_string();
        let running = Arc::new(AtomicBool::new(true));
        let messages = Arc::new(Mutex::new(Vec::new()));
        let worker_running = Arc::clone(&running);
        let worker_messages = Arc::clone(&messages);
        let worker = thread::spawn(move || {
            while worker_running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => serve_mail_page(stream, &worker_messages),
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
            messages,
            worker: Some(worker),
        }
    }

    fn url(&self) -> String {
        format!("http://{}/task-1221.html", self.listener_addr)
    }

    fn messages_for(&self, recipient: &str) -> Vec<SentMessage> {
        self.messages
            .lock()
            .expect("mail messages lock")
            .iter()
            .filter(|message| message.recipient == recipient)
            .cloned()
            .collect()
    }
}

impl Drop for LocalMailPage {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(&self.listener_addr);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve_mail_page(mut stream: TcpStream, messages: &Arc<Mutex<Vec<SentMessage>>>) {
    let Some((method, path, body)) = read_http_request(&mut stream) else {
        return;
    };
    if method == "POST" && path == "/send" {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&body) {
            if let (Some(recipient), Some(body)) = (
                value.get("to").and_then(serde_json::Value::as_str),
                value.get("body").and_then(serde_json::Value::as_str),
            ) {
                messages
                    .lock()
                    .expect("mail messages lock")
                    .push(SentMessage {
                        recipient: recipient.to_owned(),
                        body: body.to_owned(),
                    });
            }
        }
        write_response(&mut stream, "application/json", r#"{"ok":true}"#);
        return;
    }

    write_response(&mut stream, "text/html; charset=utf-8", compose_page());
}

fn read_http_request(stream: &mut TcpStream) -> Option<(String, String, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut buf = [0_u8; 4096];
    let header_end;
    loop {
        let read = stream.read(&mut buf).ok()?;
        if read == 0 {
            return None;
        }
        bytes.extend_from_slice(&buf[..read]);
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
    }

    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = headers.lines();
    let request_line = lines.next()?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next()?.to_owned();
    let path = request_parts.next()?.to_owned();
    let content_length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..read]);
    }

    Some((
        method,
        path,
        bytes[header_end..bytes.len().min(header_end + content_length)].to_vec(),
    ))
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn compose_page() -> &'static str {
    r#"<!doctype html>
<title>OSL Task 1221 Mail Fixture</title>
<main>
  <label>To <input data-osl-email-to name="to" aria-label="To" autocomplete="off"></label>
  <label>Body <textarea data-osl-email-body-input name="body" aria-label="Body"></textarea></label>
  <button data-osl-email-send type="button" aria-label="Send">Send</button>
</main>
<script>
document.querySelector('[data-osl-email-send]').addEventListener('click', () => {
  const to = document.querySelector('[data-osl-email-to]').value;
  const body = document.querySelector('[data-osl-email-body-input]').value;
  window.__oslLastSendPromise = fetch('/send', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ to, body })
  });
});
</script>
"#
}
