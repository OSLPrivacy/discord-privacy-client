//! TASK 3663: prove setup secrets stay out of the native Windows UIA tree.
//!
//! This launches the four fixed source fields in installed Chrome app
//! mode and reads the resulting Windows UI Automation tree through COM. It is
//! intentionally Windows-only; a Linux Chromium DevTools tree is not evidence
//! for this gate.

#![cfg(target_os = "windows")]

use std::fs;
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use tempfile::TempDir;
use url::Url;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
    TreeScope_Descendants, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

const WINDOW_TITLE: &str = "OSL TASK 3663 setup secrets";

// TASK 3663b changes this one type to `text` in a throwaway worktree. A real
// ValuePattern leak then names MARK-RECOVERY-3663 and fails the gate.
const RECOVERY_INPUT_TYPE: &str = "password";
// The committed control must retain the password-backed recovery source.

#[derive(Clone, Copy)]
struct MarkedSecret {
    id: &'static str,
    label: &'static str,
    marker: &'static str,
    input_type: &'static str,
}

const MARKED_SECRETS: [MarkedSecret; 4] = [
    MarkedSecret {
        id: "recovery",
        label: "Recovery words",
        marker: "MARK-RECOVERY-3663",
        input_type: RECOVERY_INPUT_TYPE,
    },
    MarkedSecret {
        id: "normal-password",
        label: "Normal password",
        marker: "MARK-NORMAL-PASSWORD-3663",
        input_type: "password",
    },
    MarkedSecret {
        id: "stealth-password",
        label: "Stealth password",
        marker: "MARK-STEALTH-PASSWORD-3663",
        input_type: "password",
    },
    MarkedSecret {
        id: "burn-password",
        label: "Burn password",
        marker: "MARK-BURN-PASSWORD-3663",
        input_type: "password",
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UiaNode {
    control_type: i32,
    localized_control_type: String,
    name: String,
    automation_id: String,
    class_name: String,
    help_text: String,
    item_status: String,
    aria_role: String,
    aria_properties: String,
    is_password: bool,
    value_pattern: Option<String>,
}

struct BrowserProcess(Child);

impl Drop for BrowserProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn setup_page(window_title: &str) -> String {
    let fields = MARKED_SECRETS
        .iter()
        .map(|secret| {
            format!(
                r#"<input id="{}" type="{}" aria-label="{}" value="{}" autocomplete="off">"#,
                secret.id, secret.input_type, secret.label, secret.marker
            )
        })
        .collect::<Vec<_>>()
        .join("");
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{window_title}</title></head><body><main><h1>Setup secrets</h1>{fields}</main></body></html>"
    )
}

fn browser_path() -> &'static Path {
    [
        Path::new(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        Path::new(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .expect("TASK 3663 requires installed Google Chrome")
}

fn launch_page(page: &str) -> (TempDir, BrowserProcess) {
    let temporary = tempfile::Builder::new()
        .prefix("osl-task-3663-")
        .tempdir()
        .expect("TASK 3663 temporary fixture directory can be created");
    let page_path = temporary.path().join("setup-secrets.html");
    let profile_path = temporary.path().join("chrome-profile");
    fs::write(&page_path, page).expect("TASK 3663 setup-secret fixture can be written");
    let page_url = Url::from_file_path(&page_path).expect("TASK 3663 fixture has a file URL");
    let child = Command::new(browser_path())
        .arg(format!("--app={page_url}"))
        .arg(format!("--user-data-dir={}", profile_path.display()))
        .args([
            "--force-renderer-accessibility=complete",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-sync",
            "--disable-background-mode",
        ])
        .spawn()
        .expect("TASK 3663 Chrome fixture can launch");
    (temporary, BrowserProcess(child))
}

fn fixture_window(window_title: &str) -> HWND {
    let deadline = Instant::now() + Duration::from_secs(15);
    let window_title = HSTRING::from(window_title);
    loop {
        let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(window_title.as_ptr())) };
        if hwnd.0 != 0 {
            return hwnd;
        }
        assert!(
            Instant::now() < deadline,
            "TASK 3663 Chrome fixture window did not appear"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn bstr_or_empty(result: windows::core::Result<windows::core::BSTR>) -> String {
    result.map(|value| value.to_string()).unwrap_or_default()
}

fn read_node(element: &IUIAutomationElement) -> UiaNode {
    let value_pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
            .ok()
            .and_then(|pattern| unsafe { pattern.CurrentValue() }.ok())
            .map(|value| value.to_string());
    UiaNode {
        control_type: unsafe { element.CurrentControlType() }
            .map(|value| value.0)
            .unwrap_or_default(),
        localized_control_type: bstr_or_empty(unsafe { element.CurrentLocalizedControlType() }),
        name: bstr_or_empty(unsafe { element.CurrentName() }),
        automation_id: bstr_or_empty(unsafe { element.CurrentAutomationId() }),
        class_name: bstr_or_empty(unsafe { element.CurrentClassName() }),
        help_text: bstr_or_empty(unsafe { element.CurrentHelpText() }),
        item_status: bstr_or_empty(unsafe { element.CurrentItemStatus() }),
        aria_role: bstr_or_empty(unsafe { element.CurrentAriaRole() }),
        aria_properties: bstr_or_empty(unsafe { element.CurrentAriaProperties() }),
        is_password: unsafe { element.CurrentIsPassword() }
            .map(|value| value.as_bool())
            .unwrap_or(false),
        value_pattern,
    }
}

fn descendant_nodes(automation: &IUIAutomation, hwnd: HWND) -> Result<Vec<UiaNode>, String> {
    let root: IUIAutomationElement = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|error| format!("UIA cannot find the Chrome host window: {error}"))?;
    let condition = unsafe { automation.CreateTrueCondition() }
        .map_err(|error| format!("UIA cannot create a traversal condition: {error}"))?;
    let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
        .map_err(|error| format!("UIA cannot enumerate the Chrome tree: {error}"))?;
    let count = unsafe { elements.Length() }
        .map_err(|error| format!("UIA cannot count Chrome descendants: {error}"))?;
    if !(1..=1024).contains(&count) {
        return Err(format!(
            "Chrome returned an unsafe UIA descendant count: {count}"
        ));
    }
    let mut nodes = Vec::with_capacity(count as usize);
    for index in 0..count {
        let element = unsafe { elements.GetElement(index) }
            .map_err(|error| format!("UIA cannot read Chrome descendant {index}: {error}"))?;
        nodes.push(read_node(&element));
    }
    Ok(nodes)
}

fn complete_document_nodes(hwnd: HWND) -> Result<Vec<UiaNode>, String> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let automation: IUIAutomation =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
            .map_err(|error| format!("Windows UI Automation is unavailable: {error}"))?;
    let _ = descendant_nodes(&automation, hwnd)?;
    thread::sleep(Duration::from_millis(500));
    let nodes = descendant_nodes(&automation, hwnd)?;
    // Release every COM interface while the apartment is still initialized.
    // Dropping the automation object after CoUninitialize is undefined and can
    // terminate the test process instead of returning an assertion result.
    drop(automation);
    if initialized {
        unsafe { CoUninitialize() };
    }
    Ok(nodes)
}

#[test]
fn setup_secrets_are_present_once_at_source_and_absent_from_windows_uia() {
    // A process-specific title prevents a previous failed browser run from being
    // mistaken for this fixture while keeping the semantic field tree stable.
    let window_title = format!("{WINDOW_TITLE} {}", std::process::id());
    let page = setup_page(&window_title);
    let source_counts = MARKED_SECRETS.map(|secret| page.matches(secret.marker).count());
    let (_temporary, _browser) = launch_page(&page);
    let hwnd = fixture_window(&window_title);
    thread::sleep(Duration::from_millis(700));
    let nodes = complete_document_nodes(hwnd)
        .expect("TASK 3663 Chrome exposes a readable Windows UIA tree");
    let tree_dump = serde_json::to_string(&nodes).expect("TASK 3663 UIA tree serializes");
    let password_field_count = nodes.iter().filter(|node| node.is_password).count();

    println!("TASK3663_PLATFORM=windows-chrome-uia");
    println!("TASK3663_WINDOWS_TREE={tree_dump}");
    println!("TASK3663_PASSWORD_FIELD_COUNT={password_field_count}");

    let mut failures = Vec::new();
    for (index, secret) in MARKED_SECRETS.iter().enumerate() {
        let source_count = source_counts[index];
        let tree_count = tree_dump.matches(secret.marker).count();
        let label_count = nodes
            .iter()
            .filter(|node| {
                [
                    node.name.as_str(),
                    node.automation_id.as_str(),
                    node.class_name.as_str(),
                    node.help_text.as_str(),
                    node.item_status.as_str(),
                    node.aria_role.as_str(),
                    node.aria_properties.as_str(),
                    node.value_pattern.as_deref().unwrap_or_default(),
                ]
                .iter()
                .any(|value| *value == secret.label)
            })
            .count();
        println!(
            "TASK3663_SECRET id={} marker={} source_count={} tree_count={} label=\"{}\" label_count={}",
            secret.id, secret.marker, source_count, tree_count, secret.label, label_count
        );
        if source_count != 1 {
            failures.push(format!(
                "{} source count was {source_count}, expected 1",
                secret.marker
            ));
        }
        if tree_count != 0 {
            failures.push(format!(
                "{} leaked into the Windows UIA tree {tree_count} time(s)",
                secret.marker
            ));
        }
        if label_count != 1 {
            failures.push(format!(
                "{} label {:?} appeared {label_count} time(s), expected 1",
                secret.marker, secret.label
            ));
        }
    }
    if password_field_count != MARKED_SECRETS.len() {
        failures.push(format!(
            "Windows UIA marked {password_field_count} fields as password fields, expected {}",
            MARKED_SECRETS.len()
        ));
    }

    assert!(failures.is_empty(), "TASK3663_FAIL {}", failures.join("; "));
    println!("TASK3663_PASS secrets=4 source_count_each=1 tree_count_each=0 label_count_each=1");
}
