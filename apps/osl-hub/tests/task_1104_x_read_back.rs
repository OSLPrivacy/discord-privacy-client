use osl_privacy_hub::adapters::{AdapterRefusal, CapabilitySet};
use osl_privacy_hub::place_text::{ExactTextPlacementError, PlaceTextActions};
use osl_privacy_hub::web_surface_adapter::x::{XSurfaceDriver, XSurfaceSnapshot, XWebBackend};
use std::sync::{Arc, Mutex};

const TEXT: &str = "x-text-1104";
const CHANGED_PLACED_BYTE: u8 = b'x';

#[derive(Default)]
struct DriverState {
    composer: Vec<u8>,
    change_before_read_back: Option<u8>,
    changed_read_back: Option<Vec<u8>>,
}

#[derive(Clone, Default)]
struct Task1104XDriver {
    state: Arc<Mutex<DriverState>>,
}

impl Task1104XDriver {
    fn change_one_placed_byte_before_read_back(&self, changed_byte: u8) {
        self.state.lock().unwrap().change_before_read_back = Some(changed_byte);
    }
}

impl PlaceTextActions for Task1104XDriver {
    type Error = AdapterRefusal;

    fn place_text(&self, text: &[u8]) -> Result<(), Self::Error> {
        self.state.lock().unwrap().composer = text.to_vec();
        Ok(())
    }

    fn read_back_text(&self) -> Result<Vec<u8>, Self::Error> {
        let mut state = self.state.lock().unwrap();
        if let Some(changed_byte) = state.change_before_read_back.take() {
            let index = state
                .composer
                .iter()
                .position(|byte| *byte != changed_byte)
                .ok_or(AdapterRefusal::ReadIncomplete)?;
            state.composer[index] = changed_byte;
            state.changed_read_back = Some(state.composer.clone());
        }
        Ok(state.composer.clone())
    }

    fn clear_text(&self) -> Result<(), Self::Error> {
        self.state.lock().unwrap().composer.clear();
        Ok(())
    }
}

impl XSurfaceDriver for Task1104XDriver {
    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
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
}

fn results(
    backend: &XWebBackend<Task1104XDriver>,
) -> Result<Vec<String>, ExactTextPlacementError<AdapterRefusal>> {
    backend
        .place_text_exact_read_back_and_clear(TEXT.as_bytes())
        .map(|_| vec![TEXT.to_owned()])
}

#[test]
fn changed_placed_byte_is_refused_and_good_x_text_result_is_unchanged() {
    let driver = Task1104XDriver::default();
    let control = driver.clone();
    let backend = XWebBackend::new(driver);

    let good = results(&backend).expect("good X text must pass exact read-back");
    assert_eq!(good, [TEXT]);

    control.change_one_placed_byte_before_read_back(CHANGED_PLACED_BYTE);
    let refusal = results(&backend).expect_err("changed placed byte x must be refused by name");
    match refusal {
        ExactTextPlacementError::ReadBackMismatch { expected, actual } => {
            assert_eq!(expected, TEXT.as_bytes());
            assert_eq!(actual, b"xxtext-1104");
        }
        other => panic!("changed placed byte x had wrong refusal: {other:?}"),
    }

    let state = control.state.lock().unwrap();
    assert_eq!(
        state.changed_read_back.as_deref(),
        Some(&b"xxtext-1104"[..])
    );
    assert!(
        state.composer.is_empty(),
        "the refused read-back must still clear X's composer"
    );
    drop(state);

    let restored = results(&backend).expect("restored X text must pass exact read-back");
    assert_eq!(restored, good);

    println!(
        "TASK1104 text={TEXT} result_count={} result_name={}",
        good.len(),
        good[0]
    );
    println!(
        "TASK1104 placed_byte={} result=refused refusal=ReadBackMismatch",
        char::from(CHANGED_PLACED_BYTE)
    );
    println!(
        "TASK1104 restored_text={TEXT} result_count={} result_name={}",
        restored.len(),
        restored[0]
    );
}
