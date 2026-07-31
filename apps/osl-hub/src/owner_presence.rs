//! Native owner-presence sampling.
//!
//! The only signal modeled here is OS idle time: how long it has been since
//! the operating system observed local input. Non-Windows builds do not fake a
//! value, because an invented idle observation would be permission-by-absence.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NativeIdleObservation {
    /// Native idle was sampled successfully.
    Observed { idle_ms: u64 },
    /// This platform has no native idle sampler in this build.
    Unsupported,
    /// The platform should support native idle sampling, but the OS call failed.
    Unavailable,
}

pub fn sample_owner_idle() -> NativeIdleObservation {
    platform_sample()
}

#[cfg(windows)]
pub fn platform_sample() -> NativeIdleObservation {
    use std::mem::size_of;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if ok == 0 {
        return NativeIdleObservation::Unavailable;
    }
    let now = unsafe { GetTickCount() };
    NativeIdleObservation::Observed {
        idle_ms: now.wrapping_sub(info.dwTime) as u64,
    }
}

#[cfg(not(windows))]
pub fn platform_sample() -> NativeIdleObservation {
    NativeIdleObservation::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_owner_idle_reports_native_idle_observation() {
        let observed = sample_owner_idle();

        #[cfg(windows)]
        assert!(
            matches!(
                observed,
                NativeIdleObservation::Observed { .. } | NativeIdleObservation::Unavailable
            ),
            "Windows must use the native idle sampler result"
        );

        #[cfg(not(windows))]
        assert_eq!(observed, NativeIdleObservation::Unsupported);
    }

    #[test]
    fn platform_sample_reports_native_idle_on_windows_and_non_windows() {
        let observed = platform_sample();

        #[cfg(windows)]
        assert!(
            !matches!(observed, NativeIdleObservation::Unsupported),
            "Windows has a native idle implementation and must not report unsupported"
        );

        #[cfg(not(windows))]
        assert_eq!(
            observed,
            NativeIdleObservation::Unsupported,
            "non-Windows builds must not synthesize an idle duration"
        );
    }
}
