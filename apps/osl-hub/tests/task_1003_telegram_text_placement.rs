#![cfg_attr(task1003_direct, allow(dead_code))]

use std::cell::RefCell;

#[cfg(task1003_direct)]
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

#[cfg(task1003_direct)]
#[path = "../src/native_a11y.rs"]
mod native_a11y;
#[cfg(task1003_direct)]
#[path = "../src/native_telegram_adapter.rs"]
mod native_telegram_adapter;

#[cfg(task1003_direct)]
use crate::native_a11y::{
    Uia2CallTimeout, Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2Syscalls, Uia2TreeRoute,
};
#[cfg(task1003_direct)]
use crate::native_telegram_adapter::{
    probe_telegram_composer_write_then_clear, TelegramLivePlacementRequest,
    TelegramPlacementStatus, TELEGRAM_COMPOSER_MEASURED_NAME, TELEGRAM_OUTER_WINDOW_CLASS,
    TELEGRAM_UIA2_MEASURED_ELEMENTS,
};
#[cfg(not(task1003_direct))]
use osl_privacy_hub::native_a11y::{
    Uia2CallTimeout, Uia2Deadline, Uia2Editable, Uia2OwnedWindow, Uia2Syscalls, Uia2TreeRoute,
};
#[cfg(not(task1003_direct))]
use osl_privacy_hub::native_telegram_adapter::{
    probe_telegram_composer_write_then_clear, TelegramLivePlacementRequest,
    TelegramPlacementStatus, TELEGRAM_COMPOSER_MEASURED_NAME, TELEGRAM_OUTER_WINDOW_CLASS,
    TELEGRAM_UIA2_MEASURED_ELEMENTS,
};

mod fixture {
    include!("fixtures/task_1003_marked_bytes.rs");
}

const EXPECTED_ORIGINAL_FIXTURE_BYTES: &[u8] = b"OSL-MARKED-BYTES-1003";

struct TelegramPlacementHost {
    value: RefCell<Option<String>>,
    writes: RefCell<Vec<Vec<u8>>>,
}

impl TelegramPlacementHost {
    fn new() -> Self {
        Self {
            value: RefCell::new(None),
            writes: RefCell::new(Vec::new()),
        }
    }

    fn editable(runtime_id: i32, name: &str) -> Uia2Editable {
        Uia2Editable {
            runtime_id: vec![1003, runtime_id],
            name: name.to_owned(),
            value_pattern: true,
            enabled: true,
            keyboard_focusable: true,
            read_only: false,
        }
    }
}

impl Uia2Syscalls for TelegramPlacementHost {
    fn enumerate_windows(
        &self,
        _deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2OwnedWindow>, Uia2CallTimeout> {
        Ok(vec![Uia2OwnedWindow {
            hwnd: 1003,
            parent_hwnd: None,
            associated_app_hwnd: None,
            process_id: 1003,
            process_name: "Telegram.exe".to_owned(),
            parent_process_id: 0,
            host_exe_name: None,
            class_name: TELEGRAM_OUTER_WINDOW_CLASS.to_owned(),
            visible: true,
            area: 1_000_000,
        }])
    }

    fn wake_chromium(
        &self,
        _hwnd: isize,
        _deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout> {
        panic!("Telegram's Qt tree must not use the Chromium wake action")
    }

    fn element_count(
        &self,
        _hwnd: isize,
        _route: Uia2TreeRoute,
        _deadline: Uia2Deadline,
    ) -> Result<usize, Uia2CallTimeout> {
        Ok(TELEGRAM_UIA2_MEASURED_ELEMENTS)
    }

    fn editable_elements(
        &self,
        _hwnd: isize,
        _route: Uia2TreeRoute,
        _deadline: Uia2Deadline,
    ) -> Result<Vec<Uia2Editable>, Uia2CallTimeout> {
        Ok(vec![
            Self::editable(1, "Search"),
            Self::editable(2, TELEGRAM_COMPOSER_MEASURED_NAME),
        ])
    }

    fn set_value(
        &self,
        _hwnd: isize,
        _route: Uia2TreeRoute,
        element: &Uia2Editable,
        value: &str,
        _deadline: Uia2Deadline,
    ) -> Result<bool, Uia2CallTimeout> {
        assert_eq!(element.name, TELEGRAM_COMPOSER_MEASURED_NAME);
        self.writes.borrow_mut().push(value.as_bytes().to_vec());
        *self.value.borrow_mut() = (!value.is_empty()).then(|| value.to_owned());
        Ok(true)
    }

    fn value_of(
        &self,
        _hwnd: isize,
        _route: Uia2TreeRoute,
        element: &Uia2Editable,
        _deadline: Uia2Deadline,
    ) -> Result<Option<String>, Uia2CallTimeout> {
        assert_eq!(element.name, TELEGRAM_COMPOSER_MEASURED_NAME);
        Ok(self.value.borrow().clone())
    }

    fn submit_shaped_calls(&self) -> usize {
        0
    }

    fn settle(&self, millis: u64) {
        panic!("Telegram's eager Qt tree must not settle for {millis} ms")
    }
}

#[cfg_attr(not(task1003_direct), test)]
fn found_telegram_box_uses_shared_place_exact_readback_and_clear_actions() {
    assert_eq!(
        fixture::MARKED_BYTES,
        EXPECTED_ORIGINAL_FIXTURE_BYTES,
        "task 1003 fixture changed by one byte"
    );
    let marked = std::str::from_utf8(fixture::MARKED_BYTES)
        .expect("the marked byte fixture is valid composer text");
    let host = TelegramPlacementHost::new();

    let receipt = probe_telegram_composer_write_then_clear(
        &host,
        TelegramLivePlacementRequest {
            carrier: marked,
            allow_replace_existing: false,
        },
    );

    assert_eq!(receipt.status, TelegramPlacementStatus::Placed);
    assert!(receipt.placed);
    assert_eq!(receipt.writable_composer_count, 1);
    assert!(receipt.readback_exact);
    assert_eq!(receipt.readback_bytes, fixture::MARKED_BYTES.len());
    assert_eq!(receipt.bytes_after_clear, 0);

    let writes = host.writes.borrow();
    assert_eq!(writes.len(), 2, "shared place and clear each write once");
    assert_eq!(writes[0], fixture::MARKED_BYTES);
    assert_eq!(writes[1], b"");
    assert_eq!(*host.value.borrow(), None);

    println!("TASK1003_FOUND_TELEGRAM_BOX_COUNT=1");
    println!("TASK1003_FOUND_TELEGRAM_BOX_NAME={TELEGRAM_COMPOSER_MEASURED_NAME}");
    println!("TASK1003_MARKED_BYTES_PLACED={}", writes[0].len());
    println!("TASK1003_READBACK_BYTES={}", receipt.readback_bytes);
    println!("TASK1003_READBACK_EXACT={}", receipt.readback_exact);
    println!("TASK1003_BYTES_AFTER_CLEAR={}", receipt.bytes_after_clear);
}

#[cfg(task1003_direct)]
fn main() {
    found_telegram_box_uses_shared_place_exact_readback_and_clear_actions();
}
