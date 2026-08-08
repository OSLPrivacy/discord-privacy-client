//! Direct, hermetic fixtures for the X active-window discovery seam.
//!
//! The command intentionally has no native window calls: it exercises the
//! result boundary used by the platform driver and makes its provider
//! classification observable in CI.

use crate::adapters::AdapterRefusal;
use crate::place_text::PlaceTextActions;
use crate::web_surface_adapter::x::{
    XActiveBrowserSurface, XFoundBrowserPlaceComposer, XSurfaceDriver, XSurfaceSnapshot,
    XWebBackend,
};
use std::sync::{Arc, Mutex};

pub const TASK_1103_MARKED_TEXT: &str = "OSL|X|1103|caf\u{e9}|\u{1f512}";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparedBrowserFixture {
    XDirect,
    InstagramDirect,
    SignalDirect,
}

impl PreparedBrowserFixture {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "x-direct" => Some(Self::XDirect),
            "instagram-direct" => Some(Self::InstagramDirect),
            "signal-direct" => Some(Self::SignalDirect),
            _ => None,
        }
    }

    pub fn record(self) -> XFoundBrowserPlaceComposer {
        match self {
            // These are the exact records supplied by the prepared browser
            // fixtures.  `direct_message` is an X classification, not a
            // generic fallback for every messaging site.
            Self::XDirect => XFoundBrowserPlaceComposer {
                browser_title: "Messages / X".to_owned(),
                place_kind: "direct_message".to_owned(),
                composer: "Message".to_owned(),
            },
            Self::InstagramDirect => XFoundBrowserPlaceComposer {
                browser_title: "Messages • Instagram".to_owned(),
                place_kind: "instagram_direct".to_owned(),
                composer: "Message...".to_owned(),
            },
            Self::SignalDirect => XFoundBrowserPlaceComposer {
                browser_title: "Signal".to_owned(),
                place_kind: "signal_conversation".to_owned(),
                composer: "Send a message".to_owned(),
            },
        }
    }
}

pub fn render_prepared_browser_fixture(value: &str) -> Result<String, String> {
    let fixture = PreparedBrowserFixture::parse(value).ok_or_else(|| {
        "usage: x-window-composer <x-direct|instagram-direct|signal-direct>".to_owned()
    })?;
    let expected = fixture.record();
    // The X fixture takes the same path as the production X backend: browser
    // discovery is delegated to the driver and the backend validates the
    // origin before it releases the three records.  Other provider fixtures
    // deliberately cannot enter that X-only path.
    let record = if fixture == PreparedBrowserFixture::XDirect {
        let found = XWebBackend::new(PreparedXDriver::default())
            .find_active_browser_place_and_composer()
            .map_err(|_| "prepared X driver could not find its active browser".to_owned())?;
        if found != expected {
            return Err("prepared X driver did not match its fixture records".to_owned());
        }
        found
    } else {
        expected
    };
    Ok(format!(
        "TASK1101_BROWSER_TITLE={}\nTASK1101_PLACE_KIND={}\nTASK1101_COMPOSER={}\n",
        record.browser_title, record.place_kind, record.composer
    ))
}

#[derive(Default)]
struct PreparedXDriver {
    state: Arc<Mutex<PreparedXTextState>>,
}

#[derive(Default)]
struct PreparedXTextState {
    composer: Vec<u8>,
    last_placed: Vec<u8>,
    last_read_back: Vec<u8>,
    actions: Vec<&'static str>,
}

impl XSurfaceDriver for PreparedXDriver {
    fn capabilities(&self) -> crate::adapters::CapabilitySet {
        Default::default()
    }

    fn is_current_generation(&self, _generation: u64) -> bool {
        true
    }

    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
        Ok(())
    }

    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
        Err(AdapterRefusal::WindowGone)
    }

    fn active_browser_surface(&self) -> Result<XActiveBrowserSurface, AdapterRefusal> {
        Ok(XActiveBrowserSurface {
            browser_title: "Messages / X".to_owned(),
            origin: "https://x.com/messages".to_owned(),
            place_kind: "direct_message".to_owned(),
            composer: "Message".to_owned(),
            composer_state: "active".to_owned(),
        })
    }
}

impl PlaceTextActions for PreparedXDriver {
    type Error = AdapterRefusal;

    fn place_text(&self, text: &[u8]) -> Result<(), Self::Error> {
        let mut state = self.state.lock().unwrap();
        state.actions.push("place_text");
        state.composer = text.to_vec();
        state.last_placed = text.to_vec();
        Ok(())
    }

    fn read_back_text(&self) -> Result<Vec<u8>, Self::Error> {
        let mut state = self.state.lock().unwrap();
        state.actions.push("read_back_text");
        if !state.composer.is_empty() {
            state.last_read_back = state.composer.clone();
        }
        Ok(state.composer.clone())
    }

    fn clear_text(&self) -> Result<(), Self::Error> {
        let mut state = self.state.lock().unwrap();
        state.actions.push("clear_text");
        state.composer.clear();
        Ok(())
    }
}

/// Direct prepared-fixture proof for Task 1103.  The backend owns the X
/// connection while the provider-neutral transaction owns place/read/clear.
pub fn render_prepared_x_text_placement() -> Result<String, String> {
    let state = Arc::new(Mutex::new(PreparedXTextState::default()));
    let backend = XWebBackend::new(PreparedXDriver {
        state: Arc::clone(&state),
    });
    let fixture = TASK_1103_MARKED_TEXT.as_bytes();
    let receipt = backend
        .place_text_exact_read_back_and_clear(fixture)
        .map_err(|error| format!("prepared X text placement failed: {error:?}"))?;
    let state = state.lock().unwrap();
    let original_hex = fixture
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let read_back_hex = state
        .last_read_back
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if receipt.placed_bytes != state.last_placed.len()
        || receipt.read_back_bytes != state.last_read_back.len()
    {
        return Err("shared receipt byte counts did not match the prepared X driver".to_owned());
    }
    Ok(format!(
        "TASK1103_SHARED_ACTIONS={}\n\
         TASK1103_FIXTURE_BYTES={}\n\
         TASK1103_PLACED_BYTES={}\n\
         TASK1103_READBACK_BYTES={}\n\
         TASK1103_ORIGINAL_HEX={}\n\
         TASK1103_READBACK_HEX={}\n\
         TASK1103_CLEARED_BYTES={}\n",
        state.actions.join(","),
        fixture.len(),
        state.last_placed.len(),
        state.last_read_back.len(),
        original_hex,
        read_back_hex,
        receipt.cleared_bytes,
    ))
}
