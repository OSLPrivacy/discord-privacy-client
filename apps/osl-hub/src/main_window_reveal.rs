//! When the main window may stop being hidden.
//!
//! `setup()` hides the main window unconditionally and nothing shows it again
//! until this says so. That is deliberate on Windows: `SetWindowDisplayAffinity`
//! is the only capture protection OSL has, it is applied per-HWND, and the
//! window must not be on screen for a single frame before an exact readback on
//! *that* HWND confirms it took. Reveal is therefore gated on the readback, not
//! on the call returning `Ok`.
//!
//! Off Windows that primitive does not exist at all — every non-Windows
//! `apply_to_hwnd` is a no-op — so there is nothing to read back and nothing the
//! reveal can be gated on. The gate was nevertheless the only path to `show()`,
//! which meant a Linux build started, loaded, ran, and never mapped its window:
//! a live process, 28 threads, and an `IsUnMapped` window rendering nothing.
//! Launching a second copy fixed it only because the single-instance callback
//! happens to call `show()`.
//!
//! Both platforms route through the one function below so that "the window is
//! revealed after a successful load" is a property of a decision this crate
//! tests, rather than of which `cfg` block happened to contain a `show()` call.
//! The Windows condition is unchanged and is not weakened by anything here.

/// Whether the platform has a per-window capture-protection primitive that the
/// reveal can be gated on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureAffinity {
    /// Windows. Display affinity exists; reveal waits for the readback.
    Gated,
    /// Everywhere else. No primitive exists, so nothing can gate the reveal.
    Absent,
}

impl CaptureAffinity {
    /// What the running platform actually offers.
    pub fn for_this_platform() -> Self {
        if cfg!(windows) {
            Self::Gated
        } else {
            Self::Absent
        }
    }
}

/// Which of the two `on_page_load` firings this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLoadPhase {
    Started,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainWindowReveal {
    /// Stay hidden.
    Hold,
    /// Show the window.
    Show,
}

/// The single decision behind every `show()` of the main window at page load.
///
/// `capture_protected` is the result of the affinity readback and is consulted
/// only where an affinity primitive exists; passing `false` on a platform that
/// has none cannot hold the window back, because there is no protection there
/// to wait for and holding would just be a permanently blank app.
pub fn main_window_reveal(
    is_main_window: bool,
    phase: PageLoadPhase,
    affinity: CaptureAffinity,
    capture_protected: bool,
) -> MainWindowReveal {
    if !is_main_window {
        return MainWindowReveal::Hold;
    }
    if phase != PageLoadPhase::Finished {
        return MainWindowReveal::Hold;
    }
    match affinity {
        CaptureAffinity::Gated if !capture_protected => MainWindowReveal::Hold,
        _ => MainWindowReveal::Show,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHASES: [PageLoadPhase; 2] = [PageLoadPhase::Started, PageLoadPhase::Finished];
    const AFFINITIES: [CaptureAffinity; 2] = [CaptureAffinity::Gated, CaptureAffinity::Absent];

    #[test]
    fn a_platform_without_the_primitive_maps_its_window_after_the_page_loads() {
        // The defect: this was `Hold` for every input off Windows, so the app
        // ran with an unmapped window and rendered nothing at all.
        assert_eq!(
            main_window_reveal(
                true,
                PageLoadPhase::Finished,
                CaptureAffinity::Absent,
                false,
            ),
            MainWindowReveal::Show,
        );
    }

    #[test]
    fn no_unprotected_readback_can_strand_a_platform_that_has_no_readback() {
        for protected in [true, false] {
            assert_eq!(
                main_window_reveal(
                    true,
                    PageLoadPhase::Finished,
                    CaptureAffinity::Absent,
                    protected,
                ),
                MainWindowReveal::Show,
                "an absent primitive must not be able to hold the window back",
            );
        }
    }

    #[test]
    fn windows_still_waits_for_the_affinity_readback() {
        assert_eq!(
            main_window_reveal(true, PageLoadPhase::Finished, CaptureAffinity::Gated, false),
            MainWindowReveal::Hold,
        );
        assert_eq!(
            main_window_reveal(true, PageLoadPhase::Finished, CaptureAffinity::Gated, true),
            MainWindowReveal::Show,
        );
    }

    #[test]
    fn nothing_is_revealed_before_the_load_finishes() {
        for affinity in AFFINITIES {
            for protected in [true, false] {
                assert_eq!(
                    main_window_reveal(true, PageLoadPhase::Started, affinity, protected),
                    MainWindowReveal::Hold,
                );
            }
        }
    }

    #[test]
    fn a_foreign_webview_never_reveals_the_main_window() {
        for phase in PHASES {
            for affinity in AFFINITIES {
                for protected in [true, false] {
                    assert_eq!(
                        main_window_reveal(false, phase, affinity, protected),
                        MainWindowReveal::Hold,
                    );
                }
            }
        }
    }

    #[test]
    fn this_platform_reports_the_primitive_it_actually_has() {
        let expected = if cfg!(windows) {
            CaptureAffinity::Gated
        } else {
            CaptureAffinity::Absent
        };
        assert_eq!(CaptureAffinity::for_this_platform(), expected);
    }
}
