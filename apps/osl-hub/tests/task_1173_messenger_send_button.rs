use osl_privacy_hub::website_driver::{
    MessengerSendButton, MessengerSendChoice, MessengerSendFields, RealBrowserWebsiteDriver,
    WebsiteConversationDiscovery, WebsiteDriver, WebsitePageRequest,
};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::Duration,
};

const PRIVATE_DRAFT: &str = "private Messenger draft 1173";
const COVER: &str = "messenger-prepared-cover-1173";
const POSTED_MARKER: &str = "posted-count=0";
const PAGE: &str = r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Messenger — Ada Lovelace</title></head>
<body>
  <main data-osl-active-conversation="true">
    <div role="textbox" contenteditable="true" aria-label="Message"
         data-osl-messenger-composer="active"></div>
    <button id="messenger-send" type="button" aria-label="Send">Send</button>
    <output id="posts">posted-count=0</output>
  </main>
  <script>
    document.getElementById('messenger-send').addEventListener('click', () => {
      document.getElementById('posts').textContent = 'posted-count=1';
    });
  </script>
</body>
</html>"#;

#[test]
fn task_1173_each_messenger_send_button_choice_prepares_without_posting() {
    let page_server = LocalPage::spawn(PAGE);
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch fixture browser");
    let page = driver
        .find_page(WebsitePageRequest {
            url: page_server.url(),
        })
        .expect("open Messenger send-button fixture");
    driver.read_page(&page).expect("wait for fixture page");
    driver
        .install_messenger_private_composer(
            &page,
            &WebsiteConversationDiscovery {
                browser_title: "Messenger — Ada Lovelace".to_owned(),
                place_kind: "messenger:direct_message".to_owned(),
                composer: "Message".to_owned(),
            },
        )
        .expect("install locked Messenger private composer");
    driver
        .write_messenger_private_text(&page, PRIVATE_DRAFT)
        .expect("write the private draft");

    let mut prepared_count = 0usize;
    for choice in MessengerSendChoice::ALL {
        let button = MessengerSendButton::for_selected_choice(choice.name())
            .expect("every rendered choice connects to the send button");
        let prepared = button
            .prepare_selected_cover(
                &mut driver,
                &page,
                MessengerSendFields {
                    private_text: PRIVATE_DRAFT,
                    composer_name: Some("Message"),
                    cover_text: COVER,
                },
            )
            .expect("pressing the OSL button prepares the selected cover");

        assert!(prepared.prepared);
        assert_eq!(prepared.choice, choice.name());
        assert_eq!(prepared.cover_text, COVER);
        assert_eq!(prepared.cover_bytes, COVER.len());
        assert_eq!(prepared.private_bytes, PRIVATE_DRAFT.len());

        let provider_page = driver
            .read_page(&page)
            .expect("read provider Send trap after preparation");
        assert_eq!(provider_page.text.matches(POSTED_MARKER).count(), 1);
        assert!(!provider_page.text.contains("posted-count=1"));

        prepared_count += 1;
        println!(
            "TASK1173 choice={} prepared_cover_bytes={} posted=false",
            choice.name(),
            prepared.cover_bytes
        );
    }

    assert_eq!(prepared_count, MessengerSendChoice::ALL.len());
    let provider_page = driver
        .read_page(&page)
        .expect("read final provider Send trap");
    let posted_count = usize::from(provider_page.text.contains("posted-count=1"));
    assert_eq!(posted_count, 0);
    println!("TASK1173 prepared_cover_count={prepared_count} posted_count={posted_count}");
}

struct LocalPage {
    address: SocketAddr,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl LocalPage {
    fn spawn(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture page");
        let address = listener.local_addr().expect("fixture address");
        listener
            .set_nonblocking(true)
            .expect("make fixture listener nonblocking");
        let running = Arc::new(AtomicBool::new(true));
        let running_worker = Arc::clone(&running);
        let worker = std::thread::spawn(move || {
            while running_worker.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => serve(&mut stream, body),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            address,
            running,
            worker: Some(worker),
        }
    }

    fn url(&self) -> String {
        format!("http://{}/task-1173.html", self.address)
    }
}

impl Drop for LocalPage {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(stream: &mut TcpStream, body: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let mut request = [0_u8; 4096];
    let _ = stream.read(&mut request);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .expect("serve fixture page");
}
