//! Short-lived, non-text appearance fingerprints for verified carrier surfaces.
//! Raw UIA names/values and capture bytes never enter `AppearanceFingerprint`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_FOREGROUND_PIXEL_INTERVAL_SECS: u64 = 5;
pub const FULL_UIA_BACKSTOP_SECS: u64 = 30;
pub const MIN_DISTINCT_RGB_COLOURS: u32 = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SurfaceKind {
    Composer,
    MessageRow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub hwnd: u64,
    pub hwnd_generation: u64,
    pub dpi: u32,
    pub theme: String,
    pub density: String,
    pub zoom_percent: u16,
    pub surface_kind: SurfaceKind,
    pub carrier_version: String,
}

/// The attributes intentionally identify UIA exposure without storing its strings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExposedTextAttributes {
    pub control_type: String,
    pub is_read_only: bool,
    pub supports_text_pattern: bool,
    pub character_count: u32,
    pub line_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WarmUiaObservation {
    pub scope: Scope,
    pub runtime_id_sha256: String,
    pub bounds: Rect,
    pub text: ExposedTextAttributes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureObservation {
    pub scope: Scope,
    pub dominant_rgb: [u8; 3],
    pub edge_rgb: [u8; 3],
    pub corner_mask: u64,
    pub edge_mask: u64,
    pub non_text_phash: u64,
    pub distinct_rgb_colours: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppearanceFingerprint {
    pub scope: Scope,
    pub uia_runtime_id_sha256: String,
    pub bounds: Rect,
    pub text: ExposedTextAttributes,
    pub dominant_rgb: [u8; 3],
    pub edge_rgb: [u8; 3],
    pub corner_mask: u64,
    pub edge_mask: u64,
    pub non_text_phash: u64,
    pub distinct_rgb_colours: u32,
    pub fingerprint_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriftEvent {
    BeforeReveal,
    FocusReturn,
    MoveResize,
    DpiThemeAccessibility,
    UiaStructureProperty,
    CarrierVersion,
    EyeComposerTransition,
    ForegroundPixelInterval,
    FullUiaBackstop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckError {
    ForegroundIntervalTooSlow,
    CaptureTooFlat,
    Disagreed,
    ScopeChanged,
    NotReady,
}

#[derive(Default)]
pub struct AppearanceGuard {
    warm_uia: Option<WarmUiaObservation>,
    warm_capture: Option<CaptureObservation>,
    fingerprint: Option<AppearanceFingerprint>,
    foreground_since_secs: Option<u64>,
    last_pixel_secs: Option<u64>,
    last_uia_secs: Option<u64>,
}

impl AppearanceGuard {
    pub fn configure(foreground_pixel_interval_secs: u64) -> Result<(), CheckError> {
        if foreground_pixel_interval_secs > MAX_FOREGROUND_PIXEL_INTERVAL_SECS {
            Err(CheckError::ForegroundIntervalTooSlow)
        } else {
            Ok(())
        }
    }
    pub fn fingerprint(&self) -> Option<&AppearanceFingerprint> {
        self.fingerprint.as_ref()
    }
    pub fn serialized_state(&self) -> Option<String> {
        self.fingerprint
            .as_ref()
            .map(|f| serde_json::to_string(f).expect("serializable"))
    }
    pub fn invalidate(&mut self, _event: DriftEvent) {
        self.fingerprint = None;
        self.warm_uia = None;
        self.warm_capture = None;
    }
    pub fn foreground(&mut self, now_secs: u64) {
        self.foreground_since_secs = Some(now_secs);
        self.last_pixel_secs = None;
        self.last_uia_secs = None;
    }
    pub fn focus_lost(&mut self) {
        self.foreground_since_secs = None;
        self.invalidate(DriftEvent::FocusReturn);
    }
    pub fn due(&self, now_secs: u64) -> Vec<DriftEvent> {
        if self.foreground_since_secs.is_none() {
            return vec![];
        }
        let pixel_due = self.last_pixel_secs.map_or(true, |t| {
            now_secs.saturating_sub(t) >= MAX_FOREGROUND_PIXEL_INTERVAL_SECS
        });
        let uia_due = self.last_uia_secs.map_or(true, |t| {
            now_secs.saturating_sub(t) >= FULL_UIA_BACKSTOP_SECS
        });
        let mut due = vec![];
        if pixel_due {
            due.push(DriftEvent::ForegroundPixelInterval);
        }
        if uia_due {
            due.push(DriftEvent::FullUiaBackstop);
        }
        due
    }
    pub fn remeasure(
        &mut self,
        event: DriftEvent,
        now_secs: u64,
        uia: WarmUiaObservation,
        capture: CaptureObservation,
    ) -> Result<(), CheckError> {
        if uia.scope != capture.scope {
            self.invalidate(event);
            return Err(CheckError::ScopeChanged);
        }
        if capture.distinct_rgb_colours < MIN_DISTINCT_RGB_COLOURS {
            self.invalidate(event);
            return Err(CheckError::CaptureTooFlat);
        }
        self.last_pixel_secs = Some(now_secs);
        self.last_uia_secs = Some(now_secs);
        if self
            .fingerprint
            .as_ref()
            .is_some_and(|old| !matches_fingerprint(old, &uia, &capture))
        {
            self.invalidate(event);
        }
        let uia_ok = self.warm_uia.as_ref().is_some_and(|old| old == &uia);
        self.warm_uia = Some(uia.clone());
        let cap_ok = self
            .warm_capture
            .as_ref()
            .is_some_and(|old| old == &capture);
        self.warm_capture = Some(capture.clone());
        if self.fingerprint.is_none() && uia_ok && cap_ok {
            self.fingerprint = Some(make_fingerprint(uia, capture));
        }
        Ok(())
    }
    pub fn reveal(
        &mut self,
        now_secs: u64,
        uia: WarmUiaObservation,
        capture: CaptureObservation,
    ) -> Result<(), CheckError> {
        self.remeasure(DriftEvent::BeforeReveal, now_secs, uia, capture)?;
        if self.fingerprint.is_some() {
            Ok(())
        } else {
            Err(CheckError::NotReady)
        }
    }
}

fn matches_fingerprint(
    old: &AppearanceFingerprint,
    u: &WarmUiaObservation,
    c: &CaptureObservation,
) -> bool {
    old.scope == u.scope
        && old.scope == c.scope
        && old.uia_runtime_id_sha256 == u.runtime_id_sha256
        && old.bounds == u.bounds
        && old.text == u.text
        && old.dominant_rgb == c.dominant_rgb
        && old.edge_rgb == c.edge_rgb
        && old.corner_mask == c.corner_mask
        && old.edge_mask == c.edge_mask
        && old.non_text_phash == c.non_text_phash
        && old.distinct_rgb_colours == c.distinct_rgb_colours
}
fn make_fingerprint(u: WarmUiaObservation, c: CaptureObservation) -> AppearanceFingerprint {
    let material = format!(
        "{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{}",
        u.scope,
        u.runtime_id_sha256,
        u.bounds,
        u.text,
        c.dominant_rgb,
        c.edge_rgb,
        c.corner_mask,
        c.edge_mask,
        c.non_text_phash,
        c.distinct_rgb_colours
    );
    AppearanceFingerprint {
        scope: u.scope,
        uia_runtime_id_sha256: u.runtime_id_sha256,
        bounds: u.bounds,
        text: u.text,
        dominant_rgb: c.dominant_rgb,
        edge_rgb: c.edge_rgb,
        corner_mask: c.corner_mask,
        edge_mask: c.edge_mask,
        non_text_phash: c.non_text_phash,
        distinct_rgb_colours: c.distinct_rgb_colours,
        fingerprint_sha256: format!("{:x}", Sha256::digest(material.as_bytes())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (WarmUiaObservation, CaptureObservation) {
        let s = Scope {
            hwnd: 17,
            hwnd_generation: 4,
            dpi: 144,
            theme: "dark".into(),
            density: "compact".into(),
            zoom_percent: 110,
            surface_kind: SurfaceKind::Composer,
            carrier_version: "1.2.3".into(),
        };
        (
            WarmUiaObservation {
                scope: s.clone(),
                runtime_id_sha256: "a".repeat(64),
                bounds: Rect {
                    left: 2,
                    top: 3,
                    right: 102,
                    bottom: 43,
                },
                text: ExposedTextAttributes {
                    control_type: "Edit".into(),
                    is_read_only: false,
                    supports_text_pattern: true,
                    character_count: 9,
                    line_count: 1,
                },
            },
            CaptureObservation {
                scope: s,
                dominant_rgb: [1, 2, 3],
                edge_rgb: [4, 5, 6],
                corner_mask: 7,
                edge_mask: 8,
                non_text_phash: 9,
                distinct_rgb_colours: 32,
            },
        )
    }
    fn warmed() -> AppearanceGuard {
        let (u, c) = pair();
        let mut g = AppearanceGuard::default();
        g.foreground(0);
        g.remeasure(DriftEvent::BeforeReveal, 0, u.clone(), c.clone())
            .unwrap();
        g.remeasure(DriftEvent::BeforeReveal, 1, u, c).unwrap();
        assert!(g.fingerprint().is_some());
        g
    }
    #[test]
    fn matrix_invalidates_every_measured_field_before_reveal() {
        for field in ["geometry", "type", "fill", "radius", "edge"] {
            let (u, mut c) = pair();
            let mut g = warmed();
            match field {
                "geometry" => {
                    let mut changed = u;
                    changed.bounds.right += 1;
                    assert_eq!(g.reveal(2, changed, c), Err(CheckError::NotReady));
                }
                "type" => {
                    let mut changed = u;
                    changed.text.control_type = "Document".into();
                    assert_eq!(g.reveal(2, changed, c), Err(CheckError::NotReady));
                }
                "fill" => {
                    c.dominant_rgb[0] += 1;
                    assert_eq!(g.reveal(2, u, c), Err(CheckError::NotReady));
                }
                "radius" => {
                    c.corner_mask += 1;
                    assert_eq!(g.reveal(2, u, c), Err(CheckError::NotReady));
                }
                "edge" => {
                    c.edge_mask += 1;
                    assert_eq!(g.reveal(2, u, c), Err(CheckError::NotReady));
                }
                _ => unreachable!(),
            }
            assert!(g.fingerprint().is_none(), "{field}");
        }
    }
    #[test]
    fn unchanged_is_stable_and_serialized_state_has_no_plaintext() {
        let (u, c) = pair();
        let mut g = warmed();
        g.reveal(2, u, c).unwrap();
        let json = g.serialized_state().unwrap();
        assert!(!json.contains("plaintext") && !json.contains("message"));
        assert!(g.fingerprint().is_some());
    }
    #[test]
    fn every_required_event_invalidates_before_reveal() {
        for event in [
            DriftEvent::BeforeReveal,
            DriftEvent::FocusReturn,
            DriftEvent::MoveResize,
            DriftEvent::DpiThemeAccessibility,
            DriftEvent::UiaStructureProperty,
            DriftEvent::CarrierVersion,
            DriftEvent::EyeComposerTransition,
            DriftEvent::ForegroundPixelInterval,
            DriftEvent::FullUiaBackstop,
        ] {
            let mut g = warmed();
            g.invalidate(event);
            assert!(g.fingerprint().is_none(), "{event:?}");
        }
    }
    #[test]
    fn silent_colour_change_is_due_within_five_seconds() {
        let (u, mut c) = pair();
        let mut g = warmed();
        assert!(g.due(6).contains(&DriftEvent::ForegroundPixelInterval));
        c.dominant_rgb[2] += 1;
        assert_eq!(
            g.remeasure(DriftEvent::ForegroundPixelInterval, 6, u, c),
            Ok(())
        );
        assert!(g.fingerprint().is_none());
    }
    #[test]
    fn bad_interval_and_blank_capture_fail_closed() {
        assert_eq!(
            AppearanceGuard::configure(6),
            Err(CheckError::ForegroundIntervalTooSlow)
        );
        let (u, mut c) = pair();
        c.distinct_rgb_colours = 31;
        let mut g = AppearanceGuard::default();
        assert_eq!(
            g.remeasure(DriftEvent::BeforeReveal, 0, u, c),
            Err(CheckError::CaptureTooFlat)
        );
    }
}
