use osl_privacy_hub::website_driver::{
    MessengerPreparedCoverState, MessengerSendButton, MessengerSendChoice, MessengerSendFields,
    RealBrowserWebsiteDriver, WebsiteConversationDiscovery, WebsiteDriver, WebsiteDriverError,
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

const MARKER: &str = "messenger-send-1174";
const COMPOSER: &str = "Message";
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
fn task_1174_empty_text_and_missing_composer_are_refused_for_every_choice() {
    let page_server = LocalPage::spawn(PAGE);
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch fixture browser");
    let page = driver
        .find_page(WebsitePageRequest {
            url: page_server.url(),
        })
        .expect("open Messenger send-field fixture");
    driver.read_page(&page).expect("wait for fixture page");
    driver
        .install_messenger_private_composer(
            &page,
            &WebsiteConversationDiscovery {
                browser_title: "Messenger — Ada Lovelace".to_owned(),
                place_kind: "messenger:direct_message".to_owned(),
                composer: COMPOSER.to_owned(),
            },
        )
        .expect("install locked Messenger private composer");
    driver
        .write_messenger_private_text(&page, MARKER)
        .expect("write good private text");

    let good_button = MessengerSendButton::for_selected_choice(MessengerSendChoice::Manual.name())
        .expect("the good fixture uses a named choice");
    let good_cover = good_button
        .prepare_selected_cover(
            &mut driver,
            &page,
            MessengerSendFields {
                private_text: MARKER,
                composer_name: Some(COMPOSER),
                cover_text: MARKER,
            },
        )
        .expect("good text and a discovered composer prepare one cover");
    let sent_covers: Vec<MessengerPreparedCoverState> = vec![good_cover];
    let sent_covers_before_refusals = sent_covers.clone();

    assert_eq!(sent_covers.len(), 1);
    assert!(sent_covers[0].prepared);
    assert_eq!(sent_covers[0].cover_text, MARKER);
    println!(
        "TASK1174 good_text={MARKER} sent_cover_count={} sent_cover={}",
        sent_covers.len(),
        sent_covers[0].cover_text
    );

    let mut empty_text_refusals = 0usize;
    let mut missing_composer_refusals = 0usize;
    for choice in MessengerSendChoice::ALL {
        let button = MessengerSendButton::for_selected_choice(choice.name())
            .expect("every rendered choice connects to the send button");

        let empty_text = button
            .prepare_selected_cover(
                &mut driver,
                &page,
                MessengerSendFields {
                    private_text: "",
                    composer_name: Some(COMPOSER),
                    cover_text: MARKER,
                },
            )
            .expect_err("empty private text must not prepare a cover");
        assert_eq!(
            empty_text,
            WebsiteDriverError::RefusedMessengerSendFieldValue("empty-text")
        );
        assert!(empty_text.to_string().contains("empty-text"));
        empty_text_refusals += 1;
        assert_eq!(
            driver
                .read_messenger_prepared_cover(&page)
                .expect("read cover after empty-text refusal"),
            sent_covers[0]
        );
        println!(
            "TASK1174 choice={} changed_send_field_value=empty-text refused_by_name=empty-text sent_cover_count={}",
            choice.name(),
            sent_covers.len()
        );

        let missing_composer = button
            .prepare_selected_cover(
                &mut driver,
                &page,
                MessengerSendFields {
                    private_text: MARKER,
                    composer_name: None,
                    cover_text: MARKER,
                },
            )
            .expect_err("a missing composer must not prepare a cover");
        assert_eq!(
            missing_composer,
            WebsiteDriverError::RefusedMessengerSendFieldValue("missing-composer")
        );
        assert!(missing_composer.to_string().contains("missing-composer"));
        missing_composer_refusals += 1;
        assert_eq!(
            driver
                .read_messenger_prepared_cover(&page)
                .expect("read cover after missing-composer refusal"),
            sent_covers[0]
        );
        println!(
            "TASK1174 choice={} changed_send_field_value=missing-composer refused_by_name=missing-composer sent_cover_count={}",
            choice.name(),
            sent_covers.len()
        );
    }

    assert_eq!(empty_text_refusals, MessengerSendChoice::ALL.len());
    assert_eq!(missing_composer_refusals, MessengerSendChoice::ALL.len());
    assert_eq!(empty_text_refusals + missing_composer_refusals, 10);
    assert_eq!(sent_covers, sent_covers_before_refusals);
    assert_eq!(sent_covers.len(), 1);
    assert_eq!(sent_covers[0].cover_text, MARKER);
    let final_cover = driver
        .read_messenger_prepared_cover(&page)
        .expect("read final unchanged cover");
    assert_eq!(final_cover, sent_covers[0]);
    println!(
        "TASK1174 empty_text_refusals={empty_text_refusals} missing_composer_refusals={missing_composer_refusals} total_refusals={}"
        , empty_text_refusals + missing_composer_refusals
    );
    println!(
        "TASK1174 final_sent_cover_count={} final_sent_cover={} unchanged=true",
        sent_covers.len(),
        sent_covers[0].cover_text
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
        format!("http://{}/task-1174.html", self.address)
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
