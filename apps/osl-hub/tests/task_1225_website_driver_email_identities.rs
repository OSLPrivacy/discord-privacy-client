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

const TASK_1225_TITLE: &str = "OSL Task 1225 Email Identities";
const TASK_1225_THREAD_ID: &str = "email-thread-1225-stable";
const TASK_1225_FOLDER_ID: &str = "email-folder-1225-inbox";

#[test]
fn task_1225_fixture_returns_one_thread_identity_and_one_folder_identity() {
    let server = LocalTestPage::spawn_body(
        TASK_1225_TITLE,
        r#"
            <main>
              <nav aria-label="Folders">
                <a href="/inbox" data-osl-current-folder-id="email-folder-1225-inbox" aria-current="page">Inbox</a>
                <a href="/archive" data-osl-folder-id="email-folder-1225-archive">Archive</a>
              </nav>
              <section role="log" aria-label="Reading pane">
                <article data-osl-open-email="true" data-osl-thread-id="email-thread-1225-stable">
                  <header><h2>Task 1225 fixture message</h2></header>
                  <div data-osl-email-body>Open message identity fixture.</div>
                </article>
              </section>
              <article data-osl-open-email="false" data-osl-thread-id="wrong-thread">
                <div data-osl-email-body>Wrong message</div>
              </article>
            </main>
        "#,
    );
    let mut driver = RealBrowserWebsiteDriver::launch().expect("launch real browser driver");

    let page = driver
        .find_page(WebsitePageRequest { url: server.url() })
        .expect("open local email identity fixture through real browser");
    let identities = driver
        .read_selected_email_identity(&page)
        .expect("read selected email identities from open page");

    assert_eq!(identities.thread_identity, TASK_1225_THREAD_ID);
    assert_eq!(identities.folder_identity, TASK_1225_FOLDER_ID);

    println!("TASK1225 direct_driver_command=read_selected_email_identity");
    println!("TASK1225 thread_identity_count=1");
    println!("TASK1225 thread_identity={}", identities.thread_identity);
    println!("TASK1225 folder_identity_count=1");
    println!("TASK1225 folder_identity={}", identities.folder_identity);
    println!(
        "TASK1225 browser_executable={}",
        driver.browser_executable().display()
    );
}

struct LocalTestPage {
    listener_addr: String,
    running: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

impl LocalTestPage {
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
        format!("http://{}/task-1225.html", self.listener_addr)
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
