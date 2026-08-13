use osl_privacy_hub::website_driver::{
    discover_messenger_browser_conversation, RealBrowserWebsiteDriver,
    WebsiteComposerAccessibility, WebsiteControlKind, WebsiteConversationAccessibility,
    WebsiteConversationBrowserSnapshot, WebsiteDriver, WebsiteDriverError, WebsiteNamedControl,
    WebsitePageRequest,
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

const MARKER: &str = "OSL-5054-renamed-composer-marker";
const PAGE: &str = r#"<!doctype html><title>Messenger — repair</title>
<main data-osl-active-conversation="true"><div id="composer" role="textbox" contenteditable="true" aria-label="Message"></div></main>
<button aria-label="Rename composer" onclick="document.querySelector('#composer').remove();document.querySelector('main').insertAdjacentHTML('beforeend','<div role=\"textbox\" contenteditable=\"true\" aria-label=\"Write a message\"></div>')">rename</button>
<button aria-label="Make composers ambiguous" onclick="document.querySelector('main').insertAdjacentHTML('beforeend','<div role=\"textbox\" contenteditable=\"true\" aria-label=\"Another message\"></div><div role=\"textbox\" contenteditable=\"true\" aria-label=\"One more message\"></div>')">ambiguous</button>"#;

#[test]
fn task_5054_repairs_a_renamed_messenger_composer_and_refuses_two_candidates() {
    let discovery = discover_messenger_browser_conversation(&WebsiteConversationBrowserSnapshot {
        url: "https://www.messenger.com/t/task-5054".to_owned(),
        title: "Messenger — repair".to_owned(),
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
    .unwrap();
    let server = LocalPage::spawn(PAGE);
    let mut driver = RealBrowserWebsiteDriver::launch().unwrap();
    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .unwrap();
    driver.read_page(&page).unwrap();
    let unchanged = driver
        .install_messenger_private_composer(&page, &discovery)
        .unwrap();
    assert_eq!(unchanged.composer_repair, "unchanged");
    driver
        .press_named_control(WebsiteNamedControl {
            page: page.clone(),
            name: "Rename composer".to_owned(),
            kind: WebsiteControlKind::Button,
        })
        .unwrap();
    let repaired = driver.write_messenger_private_text(&page, MARKER).unwrap();
    assert_eq!(repaired.composer_repair, "repaired");
    assert_eq!(repaired.private_bytes, MARKER.len());
    assert_eq!(repaired.messenger_composer_characters, 0);
    driver
        .press_named_control(WebsiteNamedControl {
            page: page.clone(),
            name: "Make composers ambiguous".to_owned(),
            kind: WebsiteControlKind::Button,
        })
        .unwrap();
    let refusal = driver
        .write_messenger_private_text(&page, "must-not-place")
        .unwrap_err();
    assert_eq!(refusal, WebsiteDriverError::MessengerComposerAmbiguous);
    println!("TASK5054_UNCHANGED={}", unchanged.composer_repair);
    println!("TASK5054_RENAMED={}", repaired.composer_repair);
    println!("TASK5054_MARKER_BYTES={}", repaired.private_bytes);
    println!("TASK5054_AMBIGUOUS_REFUSAL={refusal}");
}

struct LocalPage {
    address: SocketAddr,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl LocalPage {
    fn spawn(body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let live = Arc::clone(&running);
        let worker = std::thread::spawn(move || {
            while live.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut s, _)) => serve(&mut s, body),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10))
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
        format!("http://{}/task-5054.html", self.address)
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
    let mut request = [0_u8; 1024];
    let _ = stream.read(&mut request);
    let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    stream.write_all(response.as_bytes()).unwrap();
}
