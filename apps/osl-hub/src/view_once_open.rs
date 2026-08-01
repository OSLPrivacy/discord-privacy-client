//! Ordering for opening a locally held view-once payload.
//!
//! The payload has already been fetched and reserved at delivery. Opening is
//! deliberately local: the window is prepared hidden, capture protection is
//! verified, and only then may the sealed local payload be unsealed.

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

    /// Render plaintext only into the verified viewer.
    fn render(&mut self, plaintext: &Self::Plaintext) -> Result<(), Self::Error>;

    /// Record the local Opened event after the payload was rendered.
    fn emit_opened(&mut self) -> Result<(), Self::Error>;

    /// Destroy the sealed local payload after a successful open.
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
    effects.render(&plaintext)?;
    effects.emit_opened()?;
    effects.shred()
}

#[cfg(test)]
mod tests {
    use super::{open_view_once, ViewOnceOpenEffects};

    #[derive(Debug, PartialEq, Eq)]
    enum Step {
        ViewerOpened,
        ProtectionVerified,
        PayloadUnsealed,
        Rendered,
        OpenedEmitted,
        Shredded,
    }

    struct TestEffects {
        protection_available: bool,
        sealed_payload: Option<&'static [u8]>,
        rendered: Option<Vec<u8>>,
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
            self.steps.push(Step::Rendered);
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
                Step::Rendered,
                Step::OpenedEmitted,
                Step::Shredded,
            ],
        );
    }
}
