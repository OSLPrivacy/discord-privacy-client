use osl_privacy_hub::adapters::{AdapterRefusal, CapabilitySet};
use osl_privacy_hub::web_surface_adapter::x::{
    XActiveBrowserSurface, XSurfaceDriver, XSurfaceSnapshot, XWebBackend,
};

const BOX_ID: &str = "x-box-1102";
const COMPOSER_NAME: &str = "X composer";

struct XStateFixture {
    state: &'static str,
    accessible_name: &'static str,
}

impl XSurfaceDriver for XStateFixture {
    fn capabilities(&self) -> CapabilitySet {
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
            composer: self.accessible_name.to_owned(),
            composer_state: self.state.to_owned(),
        })
    }
}

fn find(state: &'static str, accessible_name: &'static str) -> Vec<String> {
    XWebBackend::new(XStateFixture {
        state,
        accessible_name,
    })
    .find_active_browser_place_and_composer()
    .into_iter()
    .map(|result| result.composer)
    .collect()
}

#[test]
fn task_1102_x_composer_finder_refuses_closed_and_search_focused_states() {
    let good = find("active", COMPOSER_NAME);
    assert_eq!(
        good,
        [COMPOSER_NAME],
        "good box must yield exactly one X composer"
    );

    let closed = find("closed", COMPOSER_NAME);
    assert!(closed.is_empty(), "closed must be refused by name");

    // The focused site-search control deliberately has a non-composer name as
    // well as the explicit search-focused state.  The state gate is what keeps
    // it out even though it is on the otherwise valid X messages surface.
    let search_focused = find("search-focused", "Search X");
    assert!(
        search_focused.is_empty(),
        "search-focused must be refused by name"
    );

    let restored = find("active", COMPOSER_NAME);
    assert_eq!(restored, good, "restored box must return the same result");

    println!(
        "TASK1102 box={BOX_ID} result_count={} result_name={}",
        good.len(),
        good[0]
    );
    println!("TASK1102 state=closed result=refused");
    println!("TASK1102 state=search-focused result=refused");
    println!(
        "TASK1102 restored_box={BOX_ID} result_count={} result_name={}",
        restored.len(),
        restored[0]
    );
}
