use osl_privacy_hub::{
    adapters::{AdapterRefusal, CapabilitySet},
    web_surface_adapter::x::{
        XNamedControl, XSurfaceDriver, XSurfaceSnapshot, XWebBackend,
    },
};
use std::sync::{Arc, Mutex};

const X_COMPOSER: &str = "x-box-1128";

struct PreparedXDriver {
    requests: Arc<Mutex<Vec<Vec<String>>>>,
}

impl PreparedXDriver {
    fn new(requests: Arc<Mutex<Vec<Vec<String>>>>) -> Self {
        Self { requests }
    }
}

impl XSurfaceDriver for PreparedXDriver {
    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }

    fn is_current_generation(&self, _: u64) -> bool {
        true
    }

    fn wake_accessibility(&self) -> Result<(), AdapterRefusal> {
        Ok(())
    }

    fn snapshot(&self) -> Result<XSurfaceSnapshot, AdapterRefusal> {
        Err(AdapterRefusal::WindowGone)
    }

    fn named_controls(&self, names: &[&str]) -> Result<Vec<XNamedControl>, AdapterRefusal> {
        self.requests.lock().expect("requests lock").push(
            names.iter().map(|name| (*name).to_owned()).collect(),
        );
        if names == [X_COMPOSER] {
            Ok(vec![XNamedControl {
                name: X_COMPOSER.to_owned(),
                label: "X composer".to_owned(),
            }])
        } else {
            Err(AdapterRefusal::WindowGone)
        }
    }
}

#[test]
fn task_1128_x_driver_has_one_composer_and_refuses_story_controls_by_name() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let driver = PreparedXDriver::new(Arc::clone(&requests));
    let backend = XWebBackend::new(driver);

    let first = backend
        .find_named_controls(&[X_COMPOSER])
        .expect("the prepared X composer is found");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].label, "X composer");

    for name in ["story-composer", "story-timer", "story-burn"] {
        assert_eq!(
            backend.find_named_controls(&[name]),
            Err(AdapterRefusal::WindowGone),
            "{name} must be refused by exact name"
        );
    }

    let second = backend
        .find_named_controls(&[X_COMPOSER])
        .expect("the X composer result remains unchanged");
    assert_eq!(second, first);

    let requests = requests.lock().expect("requests lock").clone();
    assert_eq!(
        requests,
        vec![
            vec![X_COMPOSER.to_owned()],
            vec!["story-composer".to_owned()],
            vec!["story-timer".to_owned()],
            vec!["story-burn".to_owned()],
            vec![X_COMPOSER.to_owned()],
        ]
    );

    println!("TASK1128 first_result_count={}", first.len());
    println!("TASK1128 first_result_label={}", first[0].label);
    for name in ["story-composer", "story-timer", "story-burn"] {
        println!("TASK1128 refused_name={name}");
    }
    println!("TASK1128 second_result_count={}", second.len());
    println!("TASK1128 second_result_label={}", second[0].label);
}
