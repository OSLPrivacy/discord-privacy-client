//! Ordering for opening a locally held view-once payload.
//!
//! The payload has already been fetched and reserved at delivery. Opening is
//! deliberately local: the window is prepared hidden, capture protection is
//! verified, and only then may the sealed local payload be unsealed.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::collections::BTreeMap;

/// Platform and storage operations needed to reveal one view-once payload.
///
/// Implementations must prepare an initially-hidden viewer in
/// [`Self::open_viewer`]. `verify_protection` must perform the platform's
/// exact capture-protection readback; a successful request alone is not
/// sufficient. No network operation belongs in this interface.
pub trait ViewOnceOpenEffects {
    type Plaintext;
    type Error;

    /// Create the viewer without making plaintext pixels visible.
    fn open_viewer(&mut self) -> Result<(), Self::Error>;

    /// Confirm the newly-created viewer is capture protected.
    fn verify_protection(&mut self) -> Result<(), Self::Error>;

    /// Unseal the already-held local payload.
    fn unseal_local_payload(&mut self) -> Result<Self::Plaintext, Self::Error>;

    /// Begin rendering plaintext only into the verified viewer.
    fn render(&mut self, plaintext: &Self::Plaintext) -> Result<(), Self::Error>;

    /// Record the local Opened event immediately before rendering begins.
    fn emit_opened(&mut self) -> Result<(), Self::Error>;

    /// Destroy the sealed local payload after an Opened event commits.
    fn shred(&mut self) -> Result<(), Self::Error>;
}

/// Open a view-once payload without ever fetching it from the network.
///
/// If protection cannot be verified, this returns before unsealing, rendering,
/// emitting `Opened`, or shredding. The sealed payload is consequently held
/// unchanged and can be opened later on a protectable device.
pub fn open_view_once<E: ViewOnceOpenEffects>(effects: &mut E) -> Result<(), E::Error> {
    effects.open_viewer()?;
    effects.verify_protection()?;
    let plaintext = effects.unseal_local_payload()?;
    effects.emit_opened()?;
    let render = effects.render(&plaintext);
    let shred = effects.shred();
    render?;
    shred
}

#[derive(Default)]
struct NamedViewOnceCopiesState {
    copies: BTreeMap<String, BTreeMap<String, String>>,
    opened: BTreeSet<String>,
}

/// In-memory model for one view-once item fanned out to named local chat-machine
/// copies. Clones share the same authority so racing first opens serialize at
/// the local destruction boundary.
#[derive(Clone, Default)]
pub struct NamedViewOnceCopies {
    inner: Arc<Mutex<NamedViewOnceCopiesState>>,
}

impl NamedViewOnceCopies {
    pub fn insert_marked_item(
        &self,
        copy_name: &str,
        item_id: &str,
        mark: &str,
    ) -> Result<(), String> {
        validate_copy_name(copy_name)?;
        validate_item_id(item_id)?;
        if mark.is_empty() {
            return Err("view-once mark is empty".to_owned());
        }
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "view-once copy ledger is unavailable".to_owned())?;
        state
            .copies
            .entry(copy_name.to_owned())
            .or_default()
            .insert(item_id.to_owned(), mark.to_owned());
        state.opened.remove(item_id);
        Ok(())
    }

    pub fn exact_item_count(
        &self,
        copy_name: &str,
        item_id: &str,
        mark: &str,
    ) -> Result<usize, String> {
        validate_copy_name(copy_name)?;
        validate_item_id(item_id)?;
        let state = self
            .inner
            .lock()
            .map_err(|_| "view-once copy ledger is unavailable".to_owned())?;
        Ok(state
            .copies
            .get(copy_name)
            .and_then(|items| items.get(item_id))
            .is_some_and(|stored| stored == mark) as usize)
    }

    pub fn open_and_destroy_all(
        &self,
        copy_name: &str,
        item_id: &str,
    ) -> Result<Option<String>, String> {
        validate_copy_name(copy_name)?;
        validate_item_id(item_id)?;
        let mut state = self
            .inner
            .lock()
            .map_err(|_| "view-once copy ledger is unavailable".to_owned())?;
        if state.opened.contains(item_id) {
            return Ok(None);
        }
        let Some(mark) = state
            .copies
            .get(copy_name)
            .and_then(|items| items.get(item_id))
            .cloned()
        else {
            return Ok(None);
        };
        for items in state.copies.values_mut() {
            items.remove(item_id);
        }
        state.opened.insert(item_id.to_owned());
        Ok(Some(mark))
    }
}

fn validate_copy_name(copy_name: &str) -> Result<(), String> {
    validate_token(copy_name, "view-once copy name")
}

fn validate_item_id(item_id: &str) -> Result<(), String> {
    validate_token(item_id, "view-once item id")
}

fn validate_token(value: &str, label: &str) -> Result<(), String> {
    if !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(format!("{label} is invalid"))
    match effects.render(&plaintext) {
        Ok(()) => effects.shred(),
        Err(error) => {
            let _ = effects.shred();
            Err(error)
        }
    }
}

/// Local view-once copy ledger keyed by explicit chat-machine names.
///
/// The ledger stores only opaque marks in this layer. A successful open on one
/// machine destroys every local copy of the same item before returning the mark
/// to the caller that is allowed to render it.
#[derive(Default)]
pub struct NamedViewOnceCopies {
    copies: BTreeMap<String, BTreeMap<String, String>>,
}

impl NamedViewOnceCopies {
    }

#[derive(Debug, Eq, PartialEq)]
pub struct ViewOnceOpenRequest {
    pub exit_code: i32,
    pub content: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewOnceViewerTier {
    Free,
    Pro,
}

impl ViewOnceViewerTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Pro => "pro",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ViewOnceOpenResultRecord {
    pub message_id: String,
    pub viewer: String,
    pub viewer_tier: ViewOnceViewerTier,
    pub exit_code: i32,
    pub content: String,
}

impl NamedViewOnceCopies {
    pub fn insert_mark(
        &mut self,
        machine: impl Into<String>,
        item_id: impl Into<String>,
        mark: impl Into<String>,
    ) {
        self.copies
            .entry(machine.into())
            .or_default()
            .insert(item_id.into(), mark.into());
    }

    pub fn count(&self, machine: &str) -> usize {
        self.copies.get(machine).map(BTreeMap::len).unwrap_or(0)
    }

    pub fn read_mark(&self, machine: &str, item_id: &str) -> Option<&str> {
        self.copies
            .get(machine)
            .and_then(|copy| copy.get(item_id))
            .map(String::as_str)
    }

    pub fn open_and_destroy_all(&mut self, machine: &str, item_id: &str) -> Option<String> {
        let mark = self.read_mark(machine, item_id)?.to_owned();
        for copy in self.copies.values_mut() {
            copy.remove(item_id);
        }
        Some(mark)
    }

    pub fn request_open(&mut self, machine: &str, item_id: &str) -> ViewOnceOpenRequest {
        match self.open_and_destroy_all(machine, item_id) {
            Some(content) => ViewOnceOpenRequest {
                exit_code: 0,
                content,
            },
            None => ViewOnceOpenRequest {
                exit_code: 1,
                content: String::new(),
            },
        }
    }

    pub fn request_open_for_viewer(
        &mut self,
        viewer: &str,
        tier: ViewOnceViewerTier,
        item_id: &str,
    ) -> ViewOnceOpenResultRecord {
        let opened = self.request_open(viewer, item_id);
        ViewOnceOpenResultRecord {
            message_id: item_id.to_owned(),
            viewer: viewer.to_owned(),
            viewer_tier: tier,
            exit_code: opened.exit_code,
            content: opened.content,
        }
    }
}

/// The cooperating-client display bound for a native view-once image.
///
/// This timer bounds display only. It is deliberately separate from the server
/// single-fetch guarantee, which decides whether the encrypted blob may be
/// retrieved at all.
pub const MAX_NATIVE_IMAGE_DISPLAY_SECONDS: u64 = 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeImageDisplayDuration {
    seconds: u64,
}

const MAX_NATIVE_IMAGE_DISPLAY_DURATION_SECONDS: u64 = 60;

impl NativeImageDisplayDuration {
    pub fn from_seconds(seconds: u64) -> Result<Self, String> {
        if seconds == 0 {
            return Err("The protected image display duration must be positive".to_owned());
        }
        if seconds > MAX_NATIVE_IMAGE_DISPLAY_SECONDS {
            return Err("The protected image display duration is too long".to_owned());
        if seconds > MAX_NATIVE_IMAGE_DISPLAY_DURATION_SECONDS {
            return Err("The protected image display duration must be at most 60 seconds".to_owned());
        }
        Ok(Self { seconds })
    }

    pub fn from_signed_seconds(seconds: i64) -> Result<Self, String> {
        let seconds = u64::try_from(seconds)
            .map_err(|_| "The protected image display duration must be positive".to_owned())?;
        Self::from_seconds(seconds)
    }

    pub fn seconds(self) -> u64 {
        self.seconds
    }

    pub fn timer_millis_u32(self) -> Result<u32, String> {
        self.seconds
            .checked_mul(1_000)
            .and_then(|millis| u32::try_from(millis).ok())
            .filter(|millis| *millis > 0)
            .ok_or_else(|| "The protected image display duration is too long".to_owned())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeImageViewerEvent {
    HiddenWindowCreated,
    CaptureExclusionApplied,
    CaptureExclusionVerified,
    Revealed,
    SensitivePixelsPainted,
    DisplayTimerStarted,
    DisplayTimerExpired,
    WindowClosed,
    PixelsZeroized,
}

/// Validate the native view-once image lifecycle that source-only desktop code
/// must follow: no display timer before verified reveal, and timer expiry must
/// close the window so the retained pixel buffer is dropped and zeroized.
pub fn validate_native_image_viewer_lifecycle(
    events: &[NativeImageViewerEvent],
) -> Result<(), &'static str> {
    use NativeImageViewerEvent as Event;

    let position = |wanted| events.iter().position(|event| *event == wanted);
    let Some(hidden) = position(Event::HiddenWindowCreated) else {
        return Err("the protected image window must begin hidden");
    };
    let Some(applied) = position(Event::CaptureExclusionApplied) else {
        return Err("capture exclusion must be applied");
    };
    let Some(verified) = position(Event::CaptureExclusionVerified) else {
        return Err("capture exclusion must be read back");
    };
    let Some(revealed) = position(Event::Revealed) else {
        return Err("the protected image was never revealed");
    };
    let Some(painted) = position(Event::SensitivePixelsPainted) else {
        return Err("the protected image was never painted");
    };
    let Some(timer) = position(Event::DisplayTimerStarted) else {
        return Err("the display timer never started");
    };
    if !(hidden < applied && applied < verified && verified < revealed && revealed <= painted) {
        return Err("capture exclusion must be proven before reveal and paint");
    }
    if timer < painted {
        return Err("the display timer must start when protected pixels are visible");
    }
    if let Some(expired) = position(Event::DisplayTimerExpired) {
        let Some(closed) = position(Event::WindowClosed) else {
            return Err("timer expiry must close the protected image window");
        };
        let Some(zeroized) = position(Event::PixelsZeroized) else {
            return Err("closing the protected image window must zeroize pixels");
        };
        if !(expired < closed && closed < zeroized) {
            return Err("timer expiry must close before the retained pixels are zeroized");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        open_view_once, validate_native_image_viewer_lifecycle, NamedViewOnceCopies,
        NativeImageDisplayDuration, NativeImageViewerEvent as ImageEvent, ViewOnceOpenEffects,
        MAX_NATIVE_IMAGE_DISPLAY_SECONDS,
        ViewOnceOpenResultRecord, ViewOnceViewerTier,
    };

    #[derive(Debug, PartialEq, Eq)]
    enum Step {
        ViewerOpened,
        ProtectionVerified,
        PayloadUnsealed,
        RenderStarted,
        OpenedEmitted,
        Shredded,
    }

    struct TestEffects {
        protection_available: bool,
        sealed_payload: Option<&'static [u8]>,
        rendered: Option<Vec<u8>>,
        render_crashes: bool,
        opened_emitted: bool,
        shredded: bool,
        steps: Vec<Step>,
    }

    impl TestEffects {
        fn protected() -> Self {
            Self {
                protection_available: true,
                sealed_payload: Some(b"sealed payload"),
                rendered: None,
                render_crashes: false,
                opened_emitted: false,
                shredded: false,
                steps: Vec::new(),
            }
        }
    }

    impl ViewOnceOpenEffects for TestEffects {
        type Plaintext = Vec<u8>;
        type Error = &'static str;

        fn open_viewer(&mut self) -> Result<(), Self::Error> {
            self.steps.push(Step::ViewerOpened);
            Ok(())
        }

        fn verify_protection(&mut self) -> Result<(), Self::Error> {
            self.steps.push(Step::ProtectionVerified);
            self.protection_available
                .then_some(())
                .ok_or("capture protection is unavailable")
        }

        fn unseal_local_payload(&mut self) -> Result<Self::Plaintext, Self::Error> {
            self.steps.push(Step::PayloadUnsealed);
            self.sealed_payload
                .map(|payload| payload.to_vec())
                .ok_or("sealed payload is unavailable")
        }

        fn render(&mut self, plaintext: &Self::Plaintext) -> Result<(), Self::Error> {
            if !self.opened_emitted {
                return Err("Opened must be emitted when rendering starts");
            }
            self.steps.push(Step::RenderStarted);
            if self.render_crashes {
                return Err("viewer crashed mid-view");
            }
            self.rendered = Some(plaintext.clone());
            Ok(())
        }

        fn emit_opened(&mut self) -> Result<(), Self::Error> {
            self.steps.push(Step::OpenedEmitted);
            self.opened_emitted = true;
            Ok(())
        }

        fn shred(&mut self) -> Result<(), Self::Error> {
            self.steps.push(Step::Shredded);
            self.shredded = true;
            self.sealed_payload = None;
            Ok(())
        }
    }

    #[test]
    fn unprotectable_devices_hold_sealed_bytes_without_rendering_plaintext() {
        let mut effects = TestEffects {
            protection_available: false,
            ..TestEffects::protected()
        };

        assert_eq!(
            open_view_once(&mut effects),
            Err("capture protection is unavailable")
        );
        assert_eq!(effects.sealed_payload, Some(b"sealed payload".as_slice()));
        assert_eq!(effects.rendered, None);
        assert!(!effects.opened_emitted);
        assert!(!effects.shredded);
        assert_eq!(
            effects.steps,
            vec![Step::ViewerOpened, Step::ProtectionVerified],
        );
    }

    #[test]
    fn successful_open_follows_the_local_view_once_lifecycle() {
        let mut effects = TestEffects::protected();

        assert_eq!(open_view_once(&mut effects), Ok(()));
        assert_eq!(effects.rendered, Some(b"sealed payload".to_vec()));
        assert!(effects.opened_emitted);
        assert!(effects.shredded);
        assert_eq!(effects.sealed_payload, None);
        assert_eq!(
            effects.steps,
            vec![
                Step::ViewerOpened,
                Step::ProtectionVerified,
                Step::PayloadUnsealed,
                Step::OpenedEmitted,
                Step::RenderStarted,
                Step::Shredded,
            ],
        );
    }

    #[test]
    fn render_crash_still_reports_opened_before_the_view_can_close() {
        let mut effects = TestEffects {
            render_crashes: true,
            ..TestEffects::protected()
        };

        assert_eq!(open_view_once(&mut effects), Err("viewer crashed mid-view"));
        assert!(effects.opened_emitted);
        assert!(effects.shredded);
        assert_eq!(effects.sealed_payload, None);
        assert_eq!(
            effects.steps,
            vec![
                Step::ViewerOpened,
                Step::ProtectionVerified,
                Step::PayloadUnsealed,
                Step::OpenedEmitted,
                Step::RenderStarted,
                Step::Shredded,
            ],
        );
    }

    #[test]
    fn task_1347_open_destroys_marked_view_once_item_on_both_chat_machines() {
        let copies = NamedViewOnceCopies::default();
        let item_id = "task1347-item";
        let mark = "TASK1347-MARK";
        copies
            .insert_marked_item("machine_a", item_id, mark)
            .unwrap();
        copies
            .insert_marked_item("machine_b", item_id, mark)
            .unwrap();

        let machine_a_before = copies.exact_item_count("machine_a", item_id, mark).unwrap();
        let machine_b_before = copies.exact_item_count("machine_b", item_id, mark).unwrap();
        assert_eq!(machine_a_before, 1);
        assert_eq!(machine_b_before, 1);
        println!(
            "TASK1347 before machine_a_count={} machine_b_count={} machine_a_mark={} machine_b_mark={}",
            machine_a_before, machine_b_before, mark, mark
        );

        let opened = copies
            .open_and_destroy_all("machine_a", item_id)
            .unwrap()
            .expect("the first view-once open returns the marked item");
        assert_eq!(opened, mark);

        let machine_a_after = copies.exact_item_count("machine_a", item_id, mark).unwrap();
        let machine_b_after = copies.exact_item_count("machine_b", item_id, mark).unwrap();
        assert_eq!(machine_a_after, 0);
        assert_eq!(machine_b_after, 0);
        println!(
            "TASK1347 after opened_mark={} machine_a_count={} machine_b_count={} machine_a_mark_absent={} machine_b_mark_absent={}",
            opened,
            machine_a_after,
            machine_b_after,
            machine_a_after == 0,
            machine_b_after == 0
        );
    }

    #[test]
    fn task_3645_race_two_first_view_once_opens_from_two_copies() {
        let copies = NamedViewOnceCopies::default();
        let item_id = "task3645-item";
        let exact = "TASK3645-EXACT-VIEW-ONCE-CONTENT";
        let first_copy = "machine_a";
        let second_copy = "machine_b";
        copies
            .insert_marked_item(first_copy, item_id, exact)
            .unwrap();
        copies
            .insert_marked_item(second_copy, item_id, exact)
            .unwrap();

        let first_before = copies.exact_item_count(first_copy, item_id, exact).unwrap();
        let second_before = copies
            .exact_item_count(second_copy, item_id, exact)
            .unwrap();
        assert_eq!(first_before, 1);
        assert_eq!(second_before, 1);
        println!(
            "TASK3645_BEFORE local_store={} exact_item={} count={}",
            first_copy, exact, first_before
        );
        println!(
            "TASK3645_BEFORE local_store={} exact_item={} count={}",
            second_copy, exact, second_before
        );

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for copy_name in [first_copy, second_copy] {
            let copies = copies.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            let results = std::sync::Arc::clone(&results);
            let item_id = item_id.to_owned();
            let copy_name = copy_name.to_owned();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                let opened = copies.open_and_destroy_all(&copy_name, &item_id).unwrap();
                results.lock().unwrap().push((copy_name, opened));
            }));
        }
        barrier.wait();
        println!("TASK3645_RELEASE simultaneous_first_open_requests=2");
        for handle in handles {
            handle.join().unwrap();
        }

        let mut opened = results.lock().unwrap().clone();
        opened.sort_by(|left, right| left.0.cmp(&right.0));
        let content_winners = opened
            .iter()
            .filter(|(_, content)| content.as_deref() == Some(exact))
            .count();
        let no_content = opened
            .iter()
            .filter(|(_, content)| content.is_none())
            .count();
        assert_eq!(content_winners, 1);
        assert_eq!(no_content, 1);
        for (copy_name, content) in &opened {
            println!(
                "TASK3645_OPEN local_store={} returned_content={}",
                copy_name,
                content.as_deref().unwrap_or("<none>")
            );
        }
        println!(
            "TASK3645_OPEN_SUMMARY exact_content={} exact_content_returns={} no_content_returns={}",
            exact, content_winners, no_content
        );

        let first_after = copies.exact_item_count(first_copy, item_id, exact).unwrap();
        let second_after = copies
            .exact_item_count(second_copy, item_id, exact)
            .unwrap();
        assert_eq!(first_after, 0);
        assert_eq!(second_after, 0);
        println!(
            "TASK3645_AFTER local_store={} exact_item={} count={}",
            first_copy, exact, first_after
        );
        println!(
            "TASK3645_AFTER local_store={} exact_item={} count={}",
            second_copy, exact, second_after
        );

        let later = copies.open_and_destroy_all(first_copy, item_id).unwrap();
        assert_eq!(later, None);
        println!(
            "TASK3645_LATER local_store={} returned_content={}",
            first_copy,
            later.as_deref().unwrap_or("<none>")
        );
        let item_id = "peer-13471347134713471347134713471347";
        let mark = "TASK1347-MARK";
        let mut copies = NamedViewOnceCopies::default();
        copies.insert_mark("chat-machine-a", item_id, mark);
        copies.insert_mark("chat-machine-b", item_id, mark);

        let first_a = copies.read_mark("chat-machine-a", item_id);
        let first_b = copies.read_mark("chat-machine-b", item_id);
        let before_a = copies.count("chat-machine-a");
        let before_b = copies.count("chat-machine-b");
        println!(
            "TASK1347 before machine_a_count={before_a} machine_b_count={before_b} \
             machine_a_mark={} machine_b_mark={}",
            first_a.unwrap_or("ABSENT"),
            first_b.unwrap_or("ABSENT")
        );
        assert_eq!(first_a, Some(mark));
        assert_eq!(first_b, Some(mark));
        assert_eq!(first_a, first_b);
        assert_eq!(before_a, 1);
        assert_eq!(before_b, 1);

        let opened_mark = copies
            .open_and_destroy_all("chat-machine-b", item_id)
            .expect("one open returns the view-once mark");
        let after_a = copies.count("chat-machine-a");
        let after_b = copies.count("chat-machine-b");
        let absent_a = copies.read_mark("chat-machine-a", item_id).is_none();
        let absent_b = copies.read_mark("chat-machine-b", item_id).is_none();
        println!(
            "TASK1347 after opened_mark={opened_mark} machine_a_count={after_a} \
             machine_b_count={after_b} machine_a_mark_absent={absent_a} \
             machine_b_mark_absent={absent_b}"
        );

        assert_eq!(opened_mark, mark);
        assert_eq!(after_a, 0);
        assert_eq!(after_b, 0);
        assert!(absent_a);
        assert!(absent_b);
    }

    #[test]
    fn task_1373_view_once_second_requests_fail_on_both_machines() {
        let item_id = "peer-13731373137313731373137313731373";
        let mark = format!("TASK1373-MARK-{:016x}", rand::random::<u64>());
        let mut copies = NamedViewOnceCopies::default();
        copies.insert_mark("chat-machine-a", item_id, mark.as_str());
        copies.insert_mark("chat-machine-b", item_id, mark.as_str());

        let first_a = copies
            .read_mark("chat-machine-a", item_id)
            .expect("machine A can read the pending mark")
            .to_owned();
        let first_b = copies
            .read_mark("chat-machine-b", item_id)
            .expect("machine B can read the pending mark")
            .to_owned();
        let before_a = copies.count("chat-machine-a");
        let before_b = copies.count("chat-machine-b");
        println!(
            "TASK1373 first_read machine_a_mark={first_a} machine_b_mark={first_b} \
             machine_a_count={before_a} machine_b_count={before_b}"
        );
        assert_eq!(first_a, mark);
        assert_eq!(first_b, mark);
        assert_eq!(first_a, first_b);
        assert_eq!(before_a, 1);
        assert_eq!(before_b, 1);

        let first_open = copies.request_open("chat-machine-a", item_id);
        println!(
            "TASK1373 first_open exit_code={} content={}",
            first_open.exit_code, first_open.content
        );
        assert_eq!(first_open.exit_code, 0);
        assert_eq!(first_open.content, mark);

        let after_a = copies.count("chat-machine-a");
        let after_b = copies.count("chat-machine-b");
        println!("TASK1373 after_first_open machine_a_count={after_a} machine_b_count={after_b}");
        assert_eq!(after_a, 0);
        assert_eq!(after_b, 0);

        let second_a = copies.request_open("chat-machine-a", item_id);
        let second_b = copies.request_open("chat-machine-b", item_id);
        println!(
            "TASK1373 second_open machine_a_exit={} machine_a_content_len={} \
             machine_b_exit={} machine_b_content_len={}",
            second_a.exit_code,
            second_a.content.len(),
            second_b.exit_code,
            second_b.content.len()
        );
        assert_eq!(second_a.exit_code, 1);
        assert_eq!(second_b.exit_code, 1);
        assert!(second_a.content.is_empty());
        assert!(second_b.content.is_empty());
    }

    #[test]
    fn task_0592_free_and_pro_viewers_each_get_exactly_one_open() {
        const ITEM_ID: &str = "peer-05920592059205920592059205920592";
        const CONTENT: &str = "the exact words";

        fn exercise_viewer(
            tier: ViewOnceViewerTier,
            viewer: &'static str,
        ) -> (ViewOnceOpenResultRecord, ViewOnceOpenResultRecord) {
            let mut copies = NamedViewOnceCopies::default();
            copies.insert_mark(viewer, ITEM_ID, CONTENT);

            let first = copies.request_open_for_viewer(viewer, tier, ITEM_ID);
            let second = copies.request_open_for_viewer(viewer, tier, ITEM_ID);
            (first, second)
        }

        let (free_first, free_second) = exercise_viewer(ViewOnceViewerTier::Free, "free-viewer");
        let (pro_first, pro_second) = exercise_viewer(ViewOnceViewerTier::Pro, "pro-viewer");

        for (label, first, second) in [
            ("free", &free_first, &free_second),
            ("pro", &pro_first, &pro_second),
        ] {
            println!(
                "TASK0592 {label}_first message={} viewer={} tier={} exit_code={} content={}",
                first.message_id,
                first.viewer,
                first.viewer_tier.label(),
                first.exit_code,
                first.content
            );
            println!(
                "TASK0592 {label}_second message={} viewer={} tier={} exit_code={} content_len={}",
                second.message_id,
                second.viewer,
                second.viewer_tier.label(),
                second.exit_code,
                second.content.len()
            );

            assert_eq!(first.message_id, ITEM_ID);
            assert_eq!(second.message_id, ITEM_ID);
            assert_eq!(first.viewer, format!("{label}-viewer"));
            assert_eq!(second.viewer, format!("{label}-viewer"));
            assert_eq!(first.viewer_tier.label(), label);
            assert_eq!(second.viewer_tier.label(), label);
            assert_eq!(first.exit_code, 0);
            assert_eq!(first.content, CONTENT);
            assert_eq!(second.exit_code, 1);
            assert!(second.content.is_empty());
        }
    }

    #[test]
    fn native_image_timer_starts_only_after_verified_reveal_and_visible_pixels() {
        assert_eq!(
            validate_native_image_viewer_lifecycle(&[
                ImageEvent::HiddenWindowCreated,
                ImageEvent::CaptureExclusionApplied,
                ImageEvent::CaptureExclusionVerified,
                ImageEvent::Revealed,
                ImageEvent::SensitivePixelsPainted,
                ImageEvent::DisplayTimerStarted,
            ]),
            Ok(())
        );

        assert_eq!(
            validate_native_image_viewer_lifecycle(&[
                ImageEvent::HiddenWindowCreated,
                ImageEvent::CaptureExclusionApplied,
                ImageEvent::DisplayTimerStarted,
                ImageEvent::CaptureExclusionVerified,
                ImageEvent::Revealed,
                ImageEvent::SensitivePixelsPainted,
            ]),
            Err("the display timer must start when protected pixels are visible")
        );
    }

    #[test]
    fn native_image_timer_expiry_closes_and_reaches_zeroize_path() {
        assert_eq!(
            validate_native_image_viewer_lifecycle(&[
                ImageEvent::HiddenWindowCreated,
                ImageEvent::CaptureExclusionApplied,
                ImageEvent::CaptureExclusionVerified,
                ImageEvent::Revealed,
                ImageEvent::SensitivePixelsPainted,
                ImageEvent::DisplayTimerStarted,
                ImageEvent::DisplayTimerExpired,
                ImageEvent::WindowClosed,
                ImageEvent::PixelsZeroized,
            ]),
            Ok(())
        );

        assert_eq!(
            validate_native_image_viewer_lifecycle(&[
                ImageEvent::HiddenWindowCreated,
                ImageEvent::CaptureExclusionApplied,
                ImageEvent::CaptureExclusionVerified,
                ImageEvent::Revealed,
                ImageEvent::SensitivePixelsPainted,
                ImageEvent::DisplayTimerStarted,
                ImageEvent::DisplayTimerExpired,
            ]),
            Err("timer expiry must close the protected image window")
        );
    }

    #[test]
    fn task_0563_view_once_duration_edges() {
        let cases = [(1, true), (60, true), (0, false), (61, false)];

        for (seconds, should_succeed) in cases {
            let result = NativeImageDisplayDuration::from_seconds(seconds);
            println!(
                "TASK0563_DURATION seconds={} result={}",
                seconds,
                if result.is_ok() { "ok" } else { "err" }
            );
            assert_eq!(
                result.is_ok(),
                should_succeed,
                "duration edge {seconds} must {}",
                if should_succeed { "succeed" } else { "fail" }
            );
            if let Ok(duration) = result {
                assert_eq!(duration.seconds(), seconds);
                assert_eq!(
                    duration.timer_millis_u32().unwrap(),
                    (seconds * 1_000) as u32
                );
            }
        }

        assert_eq!(MAX_NATIVE_IMAGE_DISPLAY_SECONDS, 60);
    fn native_image_display_duration_accepts_only_one_to_sixty_seconds() {
        let attempts = [1_u64, 60, 0, 61];
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        for seconds in attempts {
            match NativeImageDisplayDuration::from_seconds(seconds) {
                Ok(duration) => {
                    println!(
                        "duration {seconds} seconds: succeeded as {} seconds",
                        duration.seconds()
                    );
                    succeeded.push(seconds);
                }
                Err(error) => {
                    println!("duration {seconds} seconds: failed: {error}");
                    failed.push(seconds);
                }
            }
        }
        println!("successful durations: {succeeded:?}");
        println!("failed durations: {failed:?}");

        assert_eq!(succeeded, vec![1, 60]);
        assert_eq!(failed, vec![0, 61]);
        assert!(NativeImageDisplayDuration::from_signed_seconds(0).is_err());
        assert!(NativeImageDisplayDuration::from_signed_seconds(-1).is_err());
    }

    #[test]
    fn native_image_viewer_source_keeps_timer_after_readback_and_paint() {
        let source = include_str!("native_image_viewer.rs");
        let prepare = source
            .split_once("pub(crate) fn prepare(")
            .and_then(|(_, tail)| tail.split_once("Ok(PreparedImageViewer"))
            .map(|(body, _)| body)
            .expect("native image viewer prepare body is present");
        let readback = prepare
            .find("GetWindowDisplayAffinity(hwnd, &mut affinity)")
            .expect("prepare reads capture exclusion back");
        assert!(
            readback < prepare.len(),
            "capture exclusion readback must happen before the prepared viewer can be returned"
        );
        assert!(
            !prepare.contains("SetTimer")
                && !prepare.contains("arm_display_timer_after_first_paint"),
            "prepare must not start the display timer while the window is hidden"
        );

        let paint = source
            .split_once("StretchDIBits(")
            .and_then(|(_, tail)| tail.split_once("EndPaint(hwnd, &paint);"))
            .map(|(body, _)| body)
            .expect("native image viewer paint body is present");
        let draw = paint.find("StretchDIBits").unwrap_or(0);
        let timer = paint
            .find("arm_display_timer_after_first_paint")
            .expect("first paint arms the display timer");
        assert!(
            draw < timer,
            "the display timer must start after protected pixels are actually painted"
        );
    }
}
