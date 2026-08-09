#![cfg_attr(task1067_direct, allow(dead_code))]

use std::cell::RefCell;

#[cfg(task1067_direct)]
mod adapters {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Bounds {
        pub x: i32,
        pub y: i32,
        pub width: i32,
        pub height: i32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PaintConfidence {
        Exact,
        Approximate,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PaintTarget {
        pub carrier_sha256: String,
        pub rect: Bounds,
        pub clipped_by: Option<Bounds>,
        pub confidence: PaintConfidence,
    }
}

#[cfg(task1067_direct)]
#[path = "../src/native_a11y.rs"]
mod native_a11y;

#[cfg(task1067_direct)]
mod native_apps {
    pub fn whatsapp_store_package_family_name() -> &'static str {
        "5319275A.WhatsAppDesktop_cv1g1gvanyjgm"
    }
}

#[cfg(task1067_direct)]
#[path = "../src/native_whatsapp_adapter.rs"]
mod native_whatsapp_adapter;

#[cfg(task1067_direct)]
use crate::native_a11y::{
    Uia2CallTimeout, Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2Syscalls, Uia2TreeRoute,
    ELECTRON_OUTER_WINDOW_CLASS, ELECTRON_RENDERER_WINDOW_CLASS, WEBVIEW2_PROCESS_NAME,
};
#[cfg(task1067_direct)]
use crate::native_whatsapp_adapter::{
    probe_whatsapp_composer_write_then_clear, WhatsAppPlacementStatus, WHATSAPP_CARRIER_PREFIX,
    WHATSAPP_DESKTOP_PROCESS_NAME, WHATSAPP_ROOT_WINDOW_CLASS, WHATSAPP_UIA2_WINDOW_PLAN,
};
#[cfg(not(task1067_direct))]
use osl_privacy_hub::native_a11y::{
    Uia2CallTimeout, Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2Syscalls, Uia2TreeRoute,
    ELECTRON_OUTER_WINDOW_CLASS, ELECTRON_RENDERER_WINDOW_CLASS, WEBVIEW2_PROCESS_NAME,
};
#[cfg(not(task1067_direct))]
use osl_privacy_hub::native_whatsapp_adapter::{
    probe_whatsapp_composer_write_then_clear, WhatsAppPlacementStatus, WHATSAPP_CARRIER_PREFIX,
    WHATSAPP_DESKTOP_PROCESS_NAME, WHATSAPP_ROOT_WINDOW_CLASS, WHATSAPP_UIA2_WINDOW_PLAN,
};

mod fixture {
    include!("fixtures/task_1067_marked_bytes.rs");
}

const EXPECTED_ORIGINAL_FIXTURE_BYTES: &[u8] = b"OSL1.WA.WA1067-MARKED-BYTES-5D92";
const SHELL_HWND: isize = 10_670;
const WEBVIEW_HWND: isize = 10_671;
const RENDERER_HWND: isize = 10_672;
const SHELL_PROCESS_ID: u32 = 10_670;
const WEBVIEW_PROCESS_ID: u32 = 10_671;

struct WhatsAppPlacementHost {
    value: RefCell<Option<String>>,
    writes: RefCell<Vec<Vec<u8>>>,
}

impl WhatsAppPlacementHost {
    fn new() -> Self {
        Self {
            value: RefCell::new(None),
            writes: RefCell::new(Vec::new()),
        }
    }

    fn window(
        hwnd: isize,
        parent_hwnd: Option<isize>,
        associated_app_hwnd: Option<isize>,
        process_id: u32,
        process_name: &str,
        class_name: &str,
    ) -> Uia2OwnedWindow {
        Uia2OwnedWindow {
            hwnd,
            parent_hwnd,
            associated_app_hwnd,
            process_id,
            process_name: process_name.to_owned(),
            parent_process_id: 0,
            host_exe_name: None,
            class_name: class_name.to_owned(),
            visible: true,
            area: 1_000_000,
        }
    }

    fn composer() -> Uia2Editable {
        Uia2Editable {
            runtime_id: vec![1067, 1],
            name: "Type a message".to_owned(),
            value_pattern: true,
            enabled: true,
            keyboard_focusable: true,
            read_only: false,
        }
    }
}

impl Uia2Syscalls for WhatsAppPlacementHost {
    fn enumerate_windows(
        &self,
        _deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
        Ok(vec![
            Self::window(
                SHELL_HWND,
                None,
                None,
                SHELL_PROCESS_ID,
                WHATSAPP_DESKTOP_PROCESS_NAME,
                WHATSAPP_ROOT_WINDOW_CLASS,
            ),
            Self::window(
                WEBVIEW_HWND,
                None,
                Some(SHELL_HWND),
                WEBVIEW_PROCESS_ID,
                WEBVIEW2_PROCESS_NAME,
                ELECTRON_OUTER_WINDOW_CLASS,
            ),
            Self::window(
                RENDERER_HWND,
                Some(WEBVIEW_HWND),
                Some(SHELL_HWND),
                WEBVIEW_PROCESS_ID,
                WEBVIEW2_PROCESS_NAME,
                ELECTRON_RENDERER_WINDOW_CLASS,
            ),
        ])
    }

    fn wake_chromium(&self, hwnd: isize, _deadline: Uia2Deadline) -> Result<bool, Uia2CallTimeout> {
        assert_eq!(hwnd, RENDERER_HWND);
        Ok(true)
    }

    fn element_count(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        _deadline: Uia2Deadline,
    ) -> Result<usize, Uia2CallTimeout> {
        assert_eq!(hwnd, RENDERER_HWND);
        assert_eq!(route, Uia2TreeRoute::UiaNative);
        Ok(WHATSAPP_UIA2_WINDOW_PLAN.populated_min_elements)
    }

    fn editable_elements(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        _deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
        assert_eq!(hwnd, RENDERER_HWND);
        assert_eq!(route, Uia2TreeRoute::UiaNative);
        Ok(vec![Self::composer()])
    }

    fn set_value(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        value: &str,
        _deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout> {
        assert_eq!(hwnd, RENDERER_HWND);
        assert_eq!(route, Uia2TreeRoute::UiaNative);
        assert_eq!(element.name, "Type a message");
        self.writes.borrow_mut().push(value.as_bytes().to_vec());
        *self.value.borrow_mut() = Some(value.to_owned());
        Ok(true)
    }

    fn value_of(
        &self,
        hwnd: isize,
        route: Uia2TreeRoute,
        element: &Uia2Editable,
        _deadline: Uia2Deadline,
    ) -> Result<Option<String>, Uia2CallTimeout> {
        assert_eq!(hwnd, RENDERER_HWND);
        assert_eq!(route, Uia2TreeRoute::UiaNative);
        assert_eq!(element.name, "Type a message");
        Ok(self.value.borrow().clone())
    }

    fn submit_shaped_calls(&self) -> usize {
        0
    }

    fn settle(&self, _millis: u64) {}
}

#[test]
fn confirmed_whatsapp_box_places_reads_exactly_then_clears_through_shared_actions() {
    assert_eq!(
        fixture::MARKED_BYTES,
        EXPECTED_ORIGINAL_FIXTURE_BYTES,
        "task 1067 fixture changed by one byte"
    );
    let fixture_text = std::str::from_utf8(fixture::MARKED_BYTES)
        .expect("the marked byte fixture is valid composer text");
    let marked = fixture_text
        .strip_prefix(WHATSAPP_CARRIER_PREFIX)
        .expect("the fixture contains the production WhatsApp carrier prefix exactly once");
    let host = WhatsAppPlacementHost::new();

    let receipt = probe_whatsapp_composer_write_then_clear(&host, marked, false);

    assert_eq!(receipt.status, WhatsAppPlacementStatus::Placed);
    assert!(receipt.placed);
    assert!(receipt.cleared);
    assert!(receipt.readback_exact);
    assert_eq!(receipt.readback_bytes, fixture::MARKED_BYTES.len());
    assert_eq!(receipt.bytes_after_clear, 0);

    let writes = host.writes.borrow();
    assert_eq!(writes.len(), 2, "shared place and clear each write once");
    assert_eq!(writes[0], fixture::MARKED_BYTES);
    assert_eq!(writes[1], b"");
    let final_value = host.value.borrow().clone();
    let final_box_bytes = final_value.as_deref().map_or(0, str::len);
    assert_eq!(final_value.as_deref(), Some(""));
    assert_eq!(final_box_bytes, 0);

    println!("TASK1067_FIXTURE_BYTES={}", fixture::MARKED_BYTES.len());
    println!("TASK1067_MARKED_BYTES_PLACED={}", writes[0].len());
    println!("TASK1067_READBACK_BYTES={}", receipt.readback_bytes);
    println!("TASK1067_READBACK_EXACT={}", receipt.readback_exact);
    println!("TASK1067_BYTES_AFTER_CLEAR={}", receipt.bytes_after_clear);
    println!("TASK1067_FINAL_BOX_BYTE_COUNT={final_box_bytes}");
    println!("TASK1067_SHARED_WRITE_COUNT={}", writes.len());

    let adapter_source = include_str!("../src/native_whatsapp_adapter.rs");
    assert!(adapter_source.contains("place_uia2_carrier(host, acquired, &composer, &carrier"));
    assert!(adapter_source.contains("clear_uia2_composer(host, acquired, &composer)"));
    assert!(adapter_source.contains("read_uia2_composer_value(host, acquired, &composer)"));
}
