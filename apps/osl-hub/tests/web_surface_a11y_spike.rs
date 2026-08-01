//! Windows-only proof that the WebView2 surface exposes its rendered document
//! through UI Automation. Run with:
//!
//! `cargo test --features desktop --test web_surface_a11y_spike`
//!
//! The test deliberately uses a second UIA enumeration: WebView2 can expose a
//! partial tree on its first walk (WebView2Feedback #3530).

#![cfg(target_os = "windows")]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tauri::{WebviewUrl, WebviewWindowBuilder};
use url::Url;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
};

const BROWSER_ARGUMENTS: &str = concat!(
    "--force-renderer-accessibility=complete ",
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection"
);
const FIRST_SENTINEL: &str = "OSL WebView accessibility sentinel one";
const SECOND_SENTINEL: &str = "OSL WebView accessibility sentinel two";

fn reply_with_fixed_page(mut stream: TcpStream) {
    let mut request = [0_u8; 1024];
    let _ = stream.read(&mut request);
    let page = format!(
        "<!doctype html><html><body><main><h1>{FIRST_SENTINEL}</h1><p>{SECOND_SENTINEL}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("fixed accessibility page can be served");
}

fn fixed_local_page() -> (Url, thread::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener can bind");
    let address = listener
        .local_addr()
        .expect("loopback listener has an address");
    let server = thread::spawn(move || {
        for stream in listener.incoming().take(1) {
            reply_with_fixed_page(stream.expect("loopback request is readable"));
        }
    });
    (
        Url::parse(&format!("http://{address}/accessibility-spike.html"))
            .expect("loopback URL is valid"),
        server,
    )
}

fn descendant_names(automation: &IUIAutomation, hwnd: HWND) -> Result<Vec<String>, String> {
    let root: IUIAutomationElement = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|error| format!("UIA cannot find the WebView2 host window: {error}"))?;
    let condition = unsafe { automation.CreateTrueCondition() }
        .map_err(|error| format!("UIA cannot create a traversal condition: {error}"))?;
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
        .map_err(|error| format!("UIA cannot enumerate the WebView2 tree: {error}"))?;
    let count = unsafe { elements.Length() }
        .map_err(|error| format!("UIA cannot count WebView2 descendants: {error}"))?;
    if !(0..=1024).contains(&count) {
        return Err(format!(
            "WebView2 returned an unsafe UIA descendant count: {count}"
        ));
    }
    let mut names = Vec::with_capacity(count as usize);
    for index in 0..count {
        let element = unsafe { elements.GetElement(index) }
            .map_err(|error| format!("UIA cannot read WebView2 descendant {index}: {error}"))?;
        if let Ok(name) = unsafe { element.CurrentName() } {
            if !name.is_empty() {
                names.push(name.to_string());
            }
        }
    }
    Ok(names)
}

fn complete_document_names(hwnd: HWND) -> Result<Vec<String>, String> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("Windows UI Automation is unavailable: {error}"))?;

    // Warm-up is intentional. The first WebView2 enumeration is documented to
    // be partial; only the second result is evidence for this gate.
    let _ = descendant_names(&automation, hwnd)?;
    thread::sleep(Duration::from_millis(500));
    let names = descendant_names(&automation, hwnd)?;
    if initialized {
        unsafe { CoUninitialize() };
    }
    Ok(names)
}

#[test]
fn webview2_complete_accessibility_exposes_fixed_local_document_text() {
    let (url, server) = fixed_local_page();
    let (result_tx, result_rx) = mpsc::channel();
    let app = tauri::Builder::default()
        .build(tauri::generate_context!())
        .expect("Tauri test application can start");

    app.run(move |handle, event| {
        if !matches!(event, tauri::RunEvent::Ready) {
            return;
        }
        let webview = WebviewWindowBuilder::new(
            handle,
            "web-surface-a11y-spike",
            WebviewUrl::External(url.clone()),
        )
        .title("OSL WebView2 accessibility spike")
        .additional_browser_args(BROWSER_ARGUMENTS)
        .build()
        .expect("WebView2 accessibility spike can be created");
        let hwnd = webview
            .hwnd()
            .expect("spike WebView2 exposes its host HWND");
        let app = handle.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(1));
            let _ = result_tx.send(complete_document_names(hwnd));
            app.exit(0);
        });
    });

    let names = result_rx
        .recv_timeout(Duration::from_secs(12))
        .expect("WebView2 UIA probe completes before its timeout")
        .expect("WebView2 exposes a readable UIA document tree");
    server
        .join()
        .expect("fixed local-page server exits cleanly");

    assert!(
        names.iter().any(|name| name == FIRST_SENTINEL),
        "complete WebView2 UIA tree omitted first document text: {names:?}"
    );
    assert!(
        names.iter().any(|name| name == SECOND_SENTINEL),
        "complete WebView2 UIA tree omitted second document text: {names:?}"
    );
}
