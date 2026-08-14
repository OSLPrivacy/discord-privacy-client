use osl_privacy_hub::website_driver::{
    MessengerSendChoice, RealBrowserWebsiteDriver, WebsiteConversationDiscovery, WebsiteDriver,
    WebsiteDriverError, WebsitePageRequest,
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

const PRIVATE_DRAFT: &str = "private Messenger draft 1172";
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
fn task_1172_all_five_choices_prepare_cover_and_unknown_choice_fails() {
    let page_server = LocalPage::spawn(PAGE);
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch fixture browser");
    let page = driver
        .find_page(WebsitePageRequest {
            url: page_server.url(),
        })
        .expect("open Messenger send-choice fixture");
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

    let expected_names = [
        "Manual",
        "Double Enter",
        "Single Enter",
        "Instant",
        "Match typing",
    ];
    assert_eq!(
        MessengerSendChoice::ALL.map(MessengerSendChoice::name),
        expected_names
    );

    let mut prepared_count = 0usize;
    for (index, choice) in expected_names.into_iter().enumerate() {
        let cover = format!("Ordinary Messenger cover {} café 🔒", index + 1);
        let state = driver
            .prepare_messenger_cover_for_choice(&page, choice, &cover)
            .unwrap_or_else(|error| panic!("{choice} must prepare a cover: {error}"));

        assert!(state.prepared, "{choice} must report prepared");
        assert_eq!(state.choice, choice);
        assert_eq!(state.cover_text.as_bytes(), cover.as_bytes());
        assert_eq!(state.cover_bytes, cover.len());
        assert_eq!(state.private_bytes, PRIVATE_DRAFT.len());
        assert_eq!(state.messenger_composer_characters, cover.chars().count());
        prepared_count += usize::from(state.prepared);
        println!(
            "TASK1172_PREPARED_{}=true choice={choice:?} cover_bytes={}",
            index + 1,
            state.cover_bytes
        );
    }

    let before_unknown = driver
        .read_messenger_prepared_cover(&page)
        .expect("read last prepared cover before unknown choice");
    let unknown = driver.prepare_messenger_cover_for_choice(
        &page,
        "Unknown choice",
        "this must never be placed",
    );
    assert_eq!(unknown, Err(WebsiteDriverError::UnknownMessengerSendChoice));
    let after_unknown = driver
        .read_messenger_prepared_cover(&page)
        .expect("read last prepared cover after unknown choice");
    assert_eq!(
        after_unknown, before_unknown,
        "unknown choice must be inert"
    );

    println!("TASK1172_PREPARED_COVERS={prepared_count}");
    println!("TASK1172_UNKNOWN_CHOICE_ERROR={}", unknown.unwrap_err());
    println!("TASK1172_UNKNOWN_CHOICE_FAILED=true");
    println!("TASK1172_UNKNOWN_CHOICE_MUTATIONS=0");

    assert_eq!(prepared_count, 5);
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
        format!("http://{}/task-1172.html", self.address)
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
