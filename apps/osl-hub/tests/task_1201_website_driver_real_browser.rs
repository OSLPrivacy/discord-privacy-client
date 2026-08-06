use osl_privacy_hub::website_driver::{
    RealBrowserWebsiteDriver, WebsiteDriver, WebsitePageRequest,
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
