//! Deterministic synthetic-account fixture backend. No content strings are
//! placed in either the manifest or logs.

use crate::*;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub const VERIFIED_HWND: u64 = 0x5102;
const GENERATION: u64 = 17;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixtureCase {
    Valid,
    WrongHwnd,
    Occluded,
    ObservationDisagrees,
    FrameDisagrees,
    TooWide,
    Starved,
    DesktopSource,
}

impl FixtureCase {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "valid" => Some(Self::Valid),
            "wrong-hwnd" => Some(Self::WrongHwnd),
            "occluded" => Some(Self::Occluded),
            "observation-disagrees" => Some(Self::ObservationDisagrees),
            "frame-disagrees" => Some(Self::FrameDisagrees),
            "too-wide" => Some(Self::TooWide),
            "starved" => Some(Self::Starved),
            "desktop-source" => Some(Self::DesktopSource),
            _ => None,
        }
    }
}

pub struct FixtureBackend {
    case: FixtureCase,
    observations: usize,
    captures: usize,
    zeroize_audit: Arc<AtomicUsize>,
    capture_bounds: Vec<PhysicalRect>,
}

impl FixtureBackend {
    pub fn new(case: FixtureCase) -> Self {
        Self {
            case,
            observations: 0,
            captures: 0,
            zeroize_audit: Arc::new(AtomicUsize::new(0)),
            capture_bounds: Vec::new(),
        }
    }

    pub fn captures(&self) -> usize {
        self.captures
    }

    pub fn zeroized_sources(&self) -> usize {
        self.zeroize_audit.load(Ordering::SeqCst)
    }

    pub fn capture_bounds(&self) -> &[PhysicalRect] {
        &self.capture_bounds
    }
}

impl CarrierBackend for FixtureBackend {
    fn observe(&mut self, _expected_hwnd: u64) -> Result<VerifiedObservation, CaptureError> {
        if self.case == FixtureCase::Starved {
            return Err(CaptureError::MissingVerifiedHwnd);
        }
        self.observations += 1;
        let mut observation = fixture_observation();
        if self.case == FixtureCase::WrongHwnd {
            observation.hwnd += 1;
        }
        if self.case == FixtureCase::Occluded {
            observation.occluded = true;
        }
        if self.case == FixtureCase::ObservationDisagrees && self.observations == 2 {
            observation.uia_runtime_id_sha256 = "b".repeat(64);
        }
        Ok(observation)
    }

    fn capture_bounded(
        &mut self,
        expected_hwnd: u64,
        bounds: PhysicalRect,
    ) -> Result<SensitiveFrame, CaptureError> {
        self.captures += 1;
        self.capture_bounds.push(bounds);
        let actual_bounds = if self.case == FixtureCase::DesktopSource {
            PhysicalRect {
                left: 0,
                top: 0,
                right: 5_760,
                bottom: 1_200,
            }
        } else {
            bounds
        };
        let width = usize::try_from(actual_bounds.width().ok_or(CaptureError::InvalidFrame)?)
            .map_err(|_| CaptureError::InvalidFrame)?;
        let height = usize::try_from(actual_bounds.height().ok_or(CaptureError::InvalidFrame)?)
            .map_err(|_| CaptureError::InvalidFrame)?;
        let mut pixels = vec![0u8; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let offset = (y * width + x) * 3;
                pixels[offset] = 41 + u8::try_from(x % 13).unwrap_or(0);
                pixels[offset + 1] = 43 + u8::try_from(y % 11).unwrap_or(0);
                pixels[offset + 2] = 48;
            }
        }
        if self.case == FixtureCase::FrameDisagrees && self.captures == 2 {
            pixels[0] ^= 0xff;
        }
        if self.case == FixtureCase::DesktopSource {
            let marker = b"PERSONAL_CONVERSATION_MARKER_5102";
            pixels[..marker.len()].copy_from_slice(marker);
        }
        Ok(SensitiveFrame::new_audited(
            expected_hwnd,
            GENERATION,
            actual_bounds,
            PixelMode::Rgb,
            pixels,
            Arc::clone(&self.zeroize_audit),
        ))
    }
}

pub fn fixture_request(output_dir: PathBuf, case: FixtureCase) -> CaptureRequest {
    let requested_roi = if case == FixtureCase::TooWide {
        PhysicalRect {
            left: 96,
            top: 220,
            right: 902,
            bottom: 260,
        }
    } else {
        fixture_observation().uia_bounds
    };
    CaptureRequest {
        expected_hwnd: if case == FixtureCase::Starved {
            0
        } else {
            VERIFIED_HWND
        },
        surface_kind: SurfaceKind::Composer,
        requested_roi,
        state: "focused-empty".to_owned(),
        scenario: "synthetic-composer-reference".to_owned(),
        synthetic_test_account: true,
        captured_at_utc: "2026-08-10T12:34:56Z".to_owned(),
        output_dir,
    }
}

pub fn fixture_observation() -> VerifiedObservation {
    VerifiedObservation {
        hwnd: VERIFIED_HWND,
        hwnd_generation: GENERATION,
        visible: true,
        foreground: true,
        occluded: false,
        surface_kind: SurfaceKind::Composer,
        uia_runtime_id_sha256: "a".repeat(64),
        uia_bounds: PhysicalRect {
            left: 120,
            top: 220,
            right: 300,
            bottom: 260,
        },
        environment: CarrierEnvironment {
            carrier: "Discord".to_owned(),
            channel: "stable".to_owned(),
            identity: CarrierIdentity::SignedExecutable {
                executable_name: "Discord.exe".to_owned(),
                executable_sha256: "c".repeat(64),
                signer: "Discord Inc.".to_owned(),
                signature_status: "valid".to_owned(),
                version: "1.0.9251".to_owned(),
            },
            windows_build: "10.0.26100.4770".to_owned(),
            window_physical_geometry: PhysicalRect {
                left: 100,
                top: 100,
                right: 900,
                bottom: 1_122,
            },
            dpi: 96,
            monitor: MonitorMode {
                monitor_id: "DISPLAY1".to_owned(),
                colour_mode: "SDR 8-bit RGB".to_owned(),
                colour_profile_sha256: Some("d".repeat(64)),
            },
            appearance: AppearanceState {
                theme: "dark".to_owned(),
                density: "cozy".to_owned(),
                zoom_percent: 100,
                locale: "en-US".to_owned(),
                row_state: "composer-focused-empty".to_owned(),
                pointer_state: "outside-capture".to_owned(),
                caret_state: "hidden".to_owned(),
                hover_state: "none".to_owned(),
            },
        },
    }
}
