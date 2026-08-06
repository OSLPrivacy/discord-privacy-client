//! Ordering for opening a locally held view-once payload.
//!
//! The payload has already been fetched and reserved at delivery. Opening is
//! deliberately local: the window is prepared hidden, capture protection is
//! verified, and only then may the sealed local payload be unsealed.

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
}

/// The cooperating-client display bound for a native view-once image.
///
/// This timer bounds display only. It is deliberately separate from the server
/// single-fetch guarantee, which decides whether the encrypted blob may be
/// retrieved at all.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeImageDisplayDuration {
    seconds: u64,
}

impl NativeImageDisplayDuration {
    pub fn from_seconds(seconds: u64) -> Result<Self, String> {
        if seconds == 0 {
            return Err("The protected image display duration must be positive".to_owned());
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
    fn native_image_display_duration_refuses_zero_and_negative() {
        assert_eq!(
            NativeImageDisplayDuration::from_seconds(3)
                .unwrap()
                .seconds(),
            3
        );
        assert!(NativeImageDisplayDuration::from_seconds(0).is_err());
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
