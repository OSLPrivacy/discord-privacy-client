use osl_privacy_hub::website_driver::{
    discover_messenger_browser_conversation, RealBrowserWebsiteDriver,
    WebsiteComposerAccessibility, WebsiteConversationAccessibility,
    WebsiteConversationBrowserSnapshot, WebsiteDriver, WebsitePageRequest,
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

const FIXTURE: &str = "Messenger|private|café|🔒|1170|_37";
const PAGE: &str = r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Messenger — Ada Lovelace</title></head>
<body>
  <main data-osl-active-conversation="true">
    <div role="textbox" contenteditable="true" aria-label="Message"
         data-osl-messenger-composer="active"></div>
  </main>
</body>
</html>"#;

#[test]
fn task_1170_messenger_lock_private_box_counts_live_bytes_and_keeps_messenger_empty() {
    assert_eq!(
        FIXTURE.len(),
        37,
        "fixture must remain exactly 37 UTF-8 bytes"
    );

    let discovery = discover_messenger_browser_conversation(&WebsiteConversationBrowserSnapshot {
        url: "https://www.messenger.com/t/ada-lovelace".to_owned(),
        title: "Messenger — Ada Lovelace".to_owned(),
        accessibility: WebsiteConversationAccessibility {
            service: "messenger".to_owned(),
            active_conversation: true,
            composer: Some(WebsiteComposerAccessibility {
                role: "textbox".to_owned(),
                name: "Message".to_owned(),
                accessible: true,
            }),
        },
    })
    .expect("gate 1166 fixture must identify the Messenger composer");

    let page_server = LocalPage::spawn(PAGE);
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch fixture browser");
    let page = driver
        .find_page(WebsitePageRequest {
            url: page_server.url(),
        })
        .expect("open Messenger private-composer fixture");
    driver.read_page(&page).expect("wait for fixture page");

    let installed = driver
        .install_messenger_private_composer(&page, &discovery)
        .expect("mount private box over verified Messenger composer");
    assert!(installed.locked, "Messenger composer must be locked");
    assert!(
        installed.private_box_visible,
        "OSL private box must be visible"
    );
    assert_eq!(installed.private_bytes, 0);
    assert_eq!(installed.counter_text, "0 bytes");
    assert_eq!(installed.messenger_composer_characters, 0);

    let written = driver
        .write_messenger_private_text(&page, FIXTURE)
        .expect("type fixture into OSL private box");
    assert!(written.locked);
    assert!(written.private_box_visible);
    assert_eq!(written.private_bytes, 37);
    assert_eq!(written.counter_text, "37 bytes");
    assert_eq!(written.messenger_composer_characters, 0);

    let cleared = driver
        .clear_messenger_private_text(&page)
        .expect("clear OSL private box");
    assert!(cleared.locked);
    assert!(cleared.private_box_visible);
    assert_eq!(cleared.private_bytes, 0);
    assert_eq!(cleared.counter_text, "0 bytes");
    assert_eq!(cleared.messenger_composer_characters, 0);

    println!("TASK1170_LOCK={}", written.locked);
    println!(
        "TASK1170_PRIVATE_BOX_VISIBLE={}",
        written.private_box_visible
    );
    println!("TASK1170_FIXTURE_BYTES={}", FIXTURE.len());
    println!("TASK1170_COUNTER_AFTER_TYPE={}", written.private_bytes);
    println!("TASK1170_COUNTER_TEXT_AFTER_TYPE={}", written.counter_text);
    println!("TASK1170_COUNTER_AFTER_CLEAR={}", cleared.private_bytes);
    println!("TASK1170_COUNTER_TEXT_AFTER_CLEAR={}", cleared.counter_text);
    println!(
        "TASK1170_MESSENGER_COMPOSER_CHARACTERS={}",
        cleared.messenger_composer_characters
    );
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
        format!("http://{}/task-1170.html", self.address)
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
