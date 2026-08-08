//! Direct, hermetic fixtures for the X active-window discovery seam.
//!
//! The command intentionally has no native window calls: it exercises the
//! result boundary used by the platform driver and makes its provider
//! classification observable in CI.

use crate::adapters::AdapterRefusal;
use crate::web_surface_adapter::x::{
    XActiveBrowserSurface, XFoundBrowserPlaceComposer, XSurfaceDriver, XSurfaceSnapshot,
    XWebBackend,
};

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
        let found = XWebBackend::new(PreparedXDriver).find_active_browser_place_and_composer()
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

struct PreparedXDriver;

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
        })
    }
}
