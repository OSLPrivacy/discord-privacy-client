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
    drive_whatsapp_composer_placement, probe_whatsapp_composer_write_then_clear,
    WhatsAppPlacementStatus, WHATSAPP_CARRIER_PREFIX, WHATSAPP_DESKTOP_PROCESS_NAME,
    WHATSAPP_ROOT_WINDOW_CLASS, WHATSAPP_UIA2_WINDOW_PLAN,
};
#[cfg(not(task1067_direct))]
use osl_privacy_hub::native_a11y::{
    Uia2CallTimeout, Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2Syscalls, Uia2TreeRoute,
    ELECTRON_OUTER_WINDOW_CLASS, ELECTRON_RENDERER_WINDOW_CLASS, WEBVIEW2_PROCESS_NAME,
};
#[cfg(not(task1067_direct))]
use osl_privacy_hub::native_whatsapp_adapter::{
    drive_whatsapp_composer_placement, probe_whatsapp_composer_write_then_clear,
    WhatsAppPlacementStatus, WHATSAPP_CARRIER_PREFIX, WHATSAPP_DESKTOP_PROCESS_NAME,
    WHATSAPP_ROOT_WINDOW_CLASS, WHATSAPP_UIA2_WINDOW_PLAN,
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
const TASK1068_TEXT: &str = "whatsapp-text-1068";
const TASK1068_CHANGED_PLACED_BYTE: u8 = b'x';

struct WhatsAppPlacementHost {
    value: RefCell<Option<String>>,
    writes: RefCell<Vec<Vec<u8>>>,
    change_before_read_back: RefCell<bool>,
    changed_read_back: RefCell<Option<Vec<u8>>>,
    task1068_events: RefCell<Vec<&'static str>>,
}

impl WhatsAppPlacementHost {
    fn new() -> Self {
        Self::with_one_byte_read_back_change(false)
    }

    fn changing_one_placed_byte_before_read_back() -> Self {
        Self::with_one_byte_read_back_change(true)
    }

    fn with_one_byte_read_back_change(change_before_read_back: bool) -> Self {
        Self {
            value: RefCell::new(None),
            writes: RefCell::new(Vec::new()),
            change_before_read_back: RefCell::new(change_before_read_back),
            changed_read_back: RefCell::new(None),
            task1068_events: RefCell::new(Vec::new()),
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
        if !value.is_empty() {
            self.task1068_events.borrow_mut().push("placed");
        }
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
        let mut value = self.value.borrow_mut();
        let mut change_before_read_back = self.change_before_read_back.borrow_mut();
        if *change_before_read_back {
            if let Some(placed) = value.as_mut() {
                let mut changed = placed.as_bytes().to_vec();
                let index = changed
                    .iter()
                    .position(|byte| *byte != TASK1068_CHANGED_PLACED_BYTE)
                    .expect("placed WhatsApp text has a byte that can be changed to x");
                changed[index] = TASK1068_CHANGED_PLACED_BYTE;
                *placed = String::from_utf8(changed.clone())
                    .expect("changing one ASCII byte keeps the WhatsApp read-back UTF-8");
                *self.changed_read_back.borrow_mut() = Some(changed);
                self.task1068_events
                    .borrow_mut()
                    .push("changed-before-read-back");
                *change_before_read_back = false;
            }
        }
        Ok(value.clone())
    }

    fn submit_shaped_calls(&self) -> usize {
        0
    }

    fn settle(&self, _millis: u64) {}
}

fn task1068_results(host: &WhatsAppPlacementHost) -> Result<Vec<String>, WhatsAppPlacementStatus> {
    let receipt = drive_whatsapp_composer_placement(host, TASK1068_TEXT, false);
    if receipt.status == WhatsAppPlacementStatus::Placed && receipt.placed && receipt.readback_exact
    {
        Ok(vec![TASK1068_TEXT.to_owned()])
    } else {
        Err(receipt.status)
    }
}

#[test]
fn task_1068_changed_placed_byte_is_refused_and_good_whatsapp_result_is_unchanged() {
    let good_host = WhatsAppPlacementHost::new();
    let good = task1068_results(&good_host)
        .expect("good whatsapp-text-1068 must pass exact WhatsApp read-back");
    assert_eq!(good, [TASK1068_TEXT]);

    let changed_host = WhatsAppPlacementHost::changing_one_placed_byte_before_read_back();
    let refusal =
        task1068_results(&changed_host).expect_err("changed placed byte x must be refused by name");
    assert_eq!(refusal, WhatsAppPlacementStatus::ReadbackMismatch);
    assert_eq!(
        changed_host.task1068_events.borrow().as_slice(),
        ["placed", "changed-before-read-back"],
        "fault injection must occur after placement and before the returned read-back"
    );

    let writes = changed_host.writes.borrow();
    assert_eq!(writes.len(), 1, "the refused attempt places exactly once");
    {
        let changed = changed_host.changed_read_back.borrow();
        let changed = changed
            .as_deref()
            .expect("the placed WhatsApp byte was changed before read-back");
        assert_eq!(changed, b"xSL1.WA.whatsapp-text-1068");
        assert_eq!(
            changed
                .iter()
                .zip(writes[0].iter())
                .filter(|(actual, placed)| actual != placed)
                .count(),
            1,
            "fault injection must alter exactly one placed byte"
        );
    }
    drop(writes);

    let restored_host = WhatsAppPlacementHost::new();
    let restored = task1068_results(&restored_host)
        .expect("restored whatsapp-text-1068 must pass exact WhatsApp read-back");
    assert_eq!(restored, good);

    println!(
        "TASK1068 text={TASK1068_TEXT} result_count={} result_name={}",
        good.len(),
        good[0]
    );
    println!(
        "TASK1068 changed_placed_byte={} changed_readback=xSL1.WA.whatsapp-text-1068 result=refused refusal_name={refusal:?}",
        char::from(TASK1068_CHANGED_PLACED_BYTE)
    );
    println!("TASK1068 mutation_order=placed,changed-before-read-back changed_byte_count=1");
    println!(
        "TASK1068 restored_text={TASK1068_TEXT} result_count={} result_name={}",
        restored.len(),
        restored[0]
    );
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
