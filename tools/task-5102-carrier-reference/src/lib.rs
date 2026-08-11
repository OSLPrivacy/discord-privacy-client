//! Fail-closed carrier-reference capture state machine.
//!
//! A backend may observe only one explicitly selected carrier HWND and may
//! capture only the exact UIA surface plus its four-physical-pixel seam ring.
//! The state machine does not expose an API for desktop or arbitrary rectangles.

pub mod baseline;
pub mod messenger;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use zeroize::Zeroize;

pub mod fidelity;

pub const SEAM_RING_PX: i32 = 4;
const MAX_CAPTURE_PIXELS: u64 = 8_388_608;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceKind {
    Composer,
    MessageRow,
}

impl SurfaceKind {
    fn file_component(self) -> &'static str {
        match self {
            Self::Composer => "composer",
            Self::MessageRow => "message-row",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PhysicalRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PhysicalRect {
    pub fn width(self) -> Option<u32> {
        u32::try_from(self.right.checked_sub(self.left)?)
            .ok()
            .filter(|v| *v > 0)
    }

    pub fn height(self) -> Option<u32> {
        u32::try_from(self.bottom.checked_sub(self.top)?)
            .ok()
            .filter(|v| *v > 0)
    }

    pub fn contains(self, inner: Self) -> bool {
        self.width().is_some()
            && inner.width().is_some()
            && self.height().is_some()
            && inner.height().is_some()
            && inner.left >= self.left
            && inner.top >= self.top
            && inner.right <= self.right
            && inner.bottom <= self.bottom
    }

    fn expand(self, pixels: i32) -> Option<Self> {
        Some(Self {
            left: self.left.checked_sub(pixels)?,
            top: self.top.checked_sub(pixels)?,
            right: self.right.checked_add(pixels)?,
            bottom: self.bottom.checked_add(pixels)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CarrierIdentity {
    SignedExecutable {
        executable_name: String,
        executable_sha256: String,
        signer: String,
        signature_status: String,
        version: String,
    },
    BrowserOrigin {
        browser_name: String,
        browser_executable_sha256: String,
        signer: String,
        signature_status: String,
        version: String,
        origin: String,
    },
}

impl CarrierIdentity {
    fn is_verified(&self) -> bool {
        match self {
            Self::SignedExecutable {
                executable_name,
                executable_sha256,
                signer,
                signature_status,
                version,
            } => {
                !executable_name.is_empty()
                    && is_sha256(executable_sha256)
                    && !signer.is_empty()
                    && signature_status == "valid"
                    && !version.is_empty()
            }
            Self::BrowserOrigin {
                browser_name,
                browser_executable_sha256,
                signer,
                signature_status,
                version,
                origin,
            } => {
                !browser_name.is_empty()
                    && is_sha256(browser_executable_sha256)
                    && !signer.is_empty()
                    && signature_status == "valid"
                    && !version.is_empty()
                    && (origin.starts_with("https://") || origin.starts_with("http://localhost"))
            }
        }
    }

    fn version(&self) -> &str {
        match self {
            Self::SignedExecutable { version, .. } | Self::BrowserOrigin { version, .. } => version,
        }
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MonitorMode {
    pub monitor_id: String,
    pub colour_mode: String,
    pub colour_profile_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppearanceState {
    pub theme: String,
    pub density: String,
    pub zoom_percent: u16,
    pub locale: String,
    pub row_state: String,
    pub pointer_state: String,
    pub caret_state: String,
    pub hover_state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CarrierEnvironment {
    pub carrier: String,
    pub channel: String,
    pub identity: CarrierIdentity,
    pub windows_build: String,
    pub window_physical_geometry: PhysicalRect,
    pub dpi: u32,
    pub monitor: MonitorMode,
    pub appearance: AppearanceState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerifiedObservation {
    pub hwnd: u64,
    pub hwnd_generation: u64,
    pub visible: bool,
    pub foreground: bool,
    pub occluded: bool,
    pub surface_kind: SurfaceKind,
    pub uia_runtime_id_sha256: String,
    pub uia_bounds: PhysicalRect,
    pub environment: CarrierEnvironment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelMode {
    Rgb,
    Rgba,
}

impl PixelMode {
    fn channels(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    fn manifest_name(self) -> &'static str {
        match self {
            Self::Rgb => "RGB",
            Self::Rgba => "RGBA",
        }
    }
}

/// Raw capture bytes are zeroized both explicitly and on every drop path.
pub struct SensitiveFrame {
    pub hwnd: u64,
    pub hwnd_generation: u64,
    pub bounds: PhysicalRect,
    pub mode: PixelMode,
    pixels: Vec<u8>,
    zeroize_audit: Option<Arc<AtomicUsize>>,
}

impl SensitiveFrame {
    pub fn new(
        hwnd: u64,
        hwnd_generation: u64,
        bounds: PhysicalRect,
        mode: PixelMode,
        pixels: Vec<u8>,
    ) -> Self {
        Self {
            hwnd,
            hwnd_generation,
            bounds,
            mode,
            pixels,
            zeroize_audit: None,
        }
    }

    pub fn new_audited(
        hwnd: u64,
        hwnd_generation: u64,
        bounds: PhysicalRect,
        mode: PixelMode,
        pixels: Vec<u8>,
        zeroize_audit: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            hwnd,
            hwnd_generation,
            bounds,
            mode,
            pixels,
            zeroize_audit: Some(zeroize_audit),
        }
    }

    fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    fn zero(&mut self) {
        self.pixels.zeroize();
    }
}

impl Drop for SensitiveFrame {
    fn drop(&mut self) {
        self.zero();
        if self.pixels.iter().all(|byte| *byte == 0) {
            if let Some(audit) = &self.zeroize_audit {
                audit.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

/// Implementations must bind both operations to `expected_hwnd`. `capture`
/// receives only the already-validated bounded surface+ring rectangle.
pub trait CarrierBackend {
    fn observe(&mut self, expected_hwnd: u64) -> Result<VerifiedObservation, CaptureError>;
    fn capture_bounded(
        &mut self,
        expected_hwnd: u64,
        bounds: PhysicalRect,
    ) -> Result<SensitiveFrame, CaptureError>;
}

#[derive(Clone, Debug)]
pub struct CaptureRequest {
    pub expected_hwnd: u64,
    pub surface_kind: SurfaceKind,
    pub requested_roi: PhysicalRect,
    pub state: String,
    pub scenario: String,
    pub synthetic_test_account: bool,
    pub captured_at_utc: String,
    pub output_dir: PathBuf,
}

#[derive(Debug)]
pub struct CaptureResult {
    pub png_path: PathBuf,
    pub manifest_path: PathBuf,
    pub capture_bounds: PhysicalRect,
    pub png_sha256: String,
}

#[derive(Debug)]
pub enum CaptureError {
    MissingVerifiedHwnd,
    InvalidRequest(&'static str),
    ObservationDisagreed,
    WrongHwnd,
    NotVisible,
    NotForeground,
    Occluded,
    InvalidIdentity,
    UiaSurfaceMismatch,
    FrameDisagreed,
    InvalidFrame,
    OutputExists,
    Io(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVerifiedHwnd => write!(f, "verified HWND unavailable"),
            Self::InvalidRequest(reason) => write!(f, "invalid request: {reason}"),
            Self::ObservationDisagreed => write!(f, "two UIA observations did not agree"),
            Self::WrongHwnd => write!(f, "observation was bound to the wrong HWND"),
            Self::NotVisible => write!(f, "carrier window is not visible"),
            Self::NotForeground => write!(f, "carrier window is not foreground"),
            Self::Occluded => write!(f, "carrier window is occluded"),
            Self::InvalidIdentity => write!(f, "carrier identity is not verified"),
            Self::UiaSurfaceMismatch => write!(f, "requested ROI is not the exact UIA surface"),
            Self::FrameDisagreed => write!(f, "two bounded frames did not agree"),
            Self::InvalidFrame => write!(f, "bounded frame is invalid or unbound"),
            Self::OutputExists => write!(f, "capture output already exists"),
            Self::Io(reason) => write!(f, "output failed: {reason}"),
        }
    }
}

impl std::error::Error for CaptureError {}

#[derive(Serialize)]
struct Manifest<'a> {
    schema: &'static str,
    scenario: &'a str,
    state: &'a str,
    synthetic_test_account: bool,
    carrier: &'a str,
    channel: &'a str,
    identity: &'a CarrierIdentity,
    version: &'a str,
    hwnd_generation: HwndGeneration,
    windows_build: &'a str,
    physical_geometry: PhysicalGeometry,
    dpi: u32,
    monitor: &'a MonitorMode,
    appearance: &'a AppearanceState,
    row_state: &'a str,
    pointer_state: &'a str,
    caret_state: &'a str,
    hover_state: &'a str,
    uia_bounds: PhysicalRect,
    uia_runtime_id_sha256: &'a str,
    observations_agreeing: u8,
    frames_agreeing: u8,
    seam_ring_physical_px: i32,
    png_mode: &'static str,
    png_width: u32,
    png_height: u32,
    captured_at_utc: &'a str,
    png_sha256: &'a str,
}

#[derive(Serialize)]
struct HwndGeneration {
    hwnd: u64,
    generation: u64,
}

#[derive(Serialize)]
struct PhysicalGeometry {
    window: PhysicalRect,
    roi: PhysicalRect,
    capture_with_seam_ring: PhysicalRect,
}

pub fn run_capture<B: CarrierBackend>(
    backend: &mut B,
    request: &CaptureRequest,
) -> Result<CaptureResult, CaptureError> {
    validate_request(request)?;

    // Pixels cannot be requested until two complete, equal UIA/window samples
    // independently prove the same visible, foreground, unobscured target.
    let first_observation = backend.observe(request.expected_hwnd)?;
    validate_observation(&first_observation, request)?;
    // PRIVACY_GUARD_SECOND_UIA_OBSERVATION: the second sample is independent.
    let second_observation = backend.observe(request.expected_hwnd)?;
    validate_observation(&second_observation, request)?;
    if first_observation != second_observation {
        return Err(CaptureError::ObservationDisagreed);
    }

    let capture_bounds = request
        .requested_roi
        .expand(SEAM_RING_PX)
        .ok_or(CaptureError::InvalidRequest("ROI seam ring overflow"))?;
    if !first_observation
        .environment
        .window_physical_geometry
        .contains(capture_bounds)
    {
        return Err(CaptureError::InvalidRequest(
            "ROI plus seam ring exceeds verified window",
        ));
    }
    let pixels = u64::from(capture_bounds.width().ok_or(CaptureError::InvalidFrame)?)
        .checked_mul(u64::from(
            capture_bounds.height().ok_or(CaptureError::InvalidFrame)?,
        ))
        .filter(|count| *count <= MAX_CAPTURE_PIXELS)
        .ok_or(CaptureError::InvalidRequest("bounded ROI is too large"))?;
    if pixels == 0 {
        return Err(CaptureError::InvalidFrame);
    }

    let mut first_frame = backend.capture_bounded(request.expected_hwnd, capture_bounds)?;
    // PRIVACY_GUARD_VERIFIED_HWND_CROP: reject broad or unbound source frames.
    validate_frame(&first_frame, &first_observation, capture_bounds)?;
    let mut second_frame = backend.capture_bounded(request.expected_hwnd, capture_bounds)?;
    validate_frame(&second_frame, &first_observation, capture_bounds)?;
    if first_frame.mode != second_frame.mode || first_frame.pixels() != second_frame.pixels() {
        first_frame.zero();
        second_frame.zero();
        return Err(CaptureError::FrameDisagreed);
    }

    // Both persistent artifacts are formed in memory before any output path is
    // opened. Raw source frames are zeroized before either artifact is written.
    let png = encode_png(&second_frame)?;
    first_frame.zero();
    second_frame.zero();
    drop(first_frame);
    drop(second_frame);

    let png_sha256 = hex_sha256(&png);
    let base = format!(
        "{}-{}-{}-{}",
        safe_component(&first_observation.environment.carrier),
        safe_component(&first_observation.environment.channel),
        request.surface_kind.file_component(),
        safe_component(&request.state),
    );
    let png_path = request.output_dir.join(format!("{base}.png"));
    let manifest_path = request.output_dir.join(format!("{base}.manifest.json"));
    if png_path.exists() || manifest_path.exists() {
        return Err(CaptureError::OutputExists);
    }

    let env = &first_observation.environment;
    let manifest = Manifest {
        schema: "osl-carrier-reference-v1",
        scenario: &request.scenario,
        state: &request.state,
        synthetic_test_account: true,
        carrier: &env.carrier,
        channel: &env.channel,
        identity: &env.identity,
        version: env.identity.version(),
        hwnd_generation: HwndGeneration {
            hwnd: first_observation.hwnd,
            generation: first_observation.hwnd_generation,
        },
        windows_build: &env.windows_build,
        physical_geometry: PhysicalGeometry {
            window: env.window_physical_geometry,
            roi: request.requested_roi,
            capture_with_seam_ring: capture_bounds,
        },
        dpi: env.dpi,
        monitor: &env.monitor,
        appearance: &env.appearance,
        row_state: &env.appearance.row_state,
        pointer_state: &env.appearance.pointer_state,
        caret_state: &env.appearance.caret_state,
        hover_state: &env.appearance.hover_state,
        uia_bounds: first_observation.uia_bounds,
        uia_runtime_id_sha256: &first_observation.uia_runtime_id_sha256,
        observations_agreeing: 2,
        frames_agreeing: 2,
        seam_ring_physical_px: SEAM_RING_PX,
        png_mode: second_observation_mode(&png)?,
        png_width: capture_bounds.width().ok_or(CaptureError::InvalidFrame)?,
        png_height: capture_bounds.height().ok_or(CaptureError::InvalidFrame)?,
        captured_at_utc: &request.captured_at_utc,
        png_sha256: &png_sha256,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| CaptureError::Io(error.to_string()))?;

    fs::create_dir_all(&request.output_dir).map_err(io_error)?;
    write_new(&manifest_path, &manifest_bytes)?;
    if let Err(error) = write_new(&png_path, &png) {
        let _ = fs::remove_file(&manifest_path);
        return Err(error);
    }

    Ok(CaptureResult {
        png_path,
        manifest_path,
        capture_bounds,
        png_sha256,
    })
}

fn validate_request(request: &CaptureRequest) -> Result<(), CaptureError> {
    if request.expected_hwnd == 0 {
        return Err(CaptureError::MissingVerifiedHwnd);
    }
    if !request.synthetic_test_account {
        return Err(CaptureError::InvalidRequest(
            "only synthetic test accounts may be captured",
        ));
    }
    if request.state.is_empty() || request.scenario.is_empty() {
        return Err(CaptureError::InvalidRequest(
            "state and scenario are required",
        ));
    }
    if !request.captured_at_utc.ends_with('Z') {
        return Err(CaptureError::InvalidRequest("UTC capture time is required"));
    }
    request
        .requested_roi
        .width()
        .zip(request.requested_roi.height())
        .ok_or(CaptureError::InvalidRequest("ROI is degenerate"))?;
    Ok(())
}

fn validate_observation(
    observation: &VerifiedObservation,
    request: &CaptureRequest,
) -> Result<(), CaptureError> {
    if observation.hwnd != request.expected_hwnd {
        return Err(CaptureError::WrongHwnd);
    }
    if observation.hwnd_generation == 0 {
        return Err(CaptureError::WrongHwnd);
    }
    if !observation.visible {
        return Err(CaptureError::NotVisible);
    }
    if !observation.foreground {
        return Err(CaptureError::NotForeground);
    }
    if observation.occluded {
        return Err(CaptureError::Occluded);
    }
    if observation.surface_kind != request.surface_kind
        || observation.uia_bounds != request.requested_roi
        || !is_sha256(&observation.uia_runtime_id_sha256)
    {
        return Err(CaptureError::UiaSurfaceMismatch);
    }
    if !observation.environment.identity.is_verified()
        || observation.environment.carrier.is_empty()
        || observation.environment.channel.is_empty()
        || observation.environment.windows_build.is_empty()
        || observation.environment.dpi == 0
        || observation.environment.monitor.monitor_id.is_empty()
        || observation.environment.monitor.colour_mode.is_empty()
        || observation.environment.appearance.theme.is_empty()
        || observation.environment.appearance.density.is_empty()
        || observation.environment.appearance.zoom_percent == 0
        || observation.environment.appearance.locale.is_empty()
    {
        return Err(CaptureError::InvalidIdentity);
    }
    if !observation
        .environment
        .window_physical_geometry
        .contains(observation.uia_bounds)
    {
        return Err(CaptureError::UiaSurfaceMismatch);
    }
    Ok(())
}

fn validate_frame(
    frame: &SensitiveFrame,
    observation: &VerifiedObservation,
    expected_bounds: PhysicalRect,
) -> Result<(), CaptureError> {
    if frame.hwnd != observation.hwnd
        || frame.hwnd_generation != observation.hwnd_generation
        || frame.bounds != expected_bounds
    {
        return Err(CaptureError::InvalidFrame);
    }
    let expected_len = usize::try_from(expected_bounds.width().ok_or(CaptureError::InvalidFrame)?)
        .ok()
        .and_then(|width| {
            usize::try_from(expected_bounds.height()?)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(frame.mode.channels()))
        .ok_or(CaptureError::InvalidFrame)?;
    if frame.pixels().len() != expected_len {
        return Err(CaptureError::InvalidFrame);
    }
    Ok(())
}

fn encode_png(frame: &SensitiveFrame) -> Result<Vec<u8>, CaptureError> {
    let width = frame.bounds.width().ok_or(CaptureError::InvalidFrame)?;
    let height = frame.bounds.height().ok_or(CaptureError::InvalidFrame)?;
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_color(match frame.mode {
            PixelMode::Rgb => png::ColorType::Rgb,
            PixelMode::Rgba => png::ColorType::Rgba,
        });
        encoder.set_compression(png::Compression::Best);
        let mut writer = encoder
            .write_header()
            .map_err(|error| CaptureError::Io(error.to_string()))?;
        writer
            .write_image_data(frame.pixels())
            .map_err(|error| CaptureError::Io(error.to_string()))?;
    }
    Ok(output)
}

fn second_observation_mode(png: &[u8]) -> Result<&'static str, CaptureError> {
    // PNG IHDR colour-type byte: 2=RGB, 6=RGBA. Reading the encoded artifact
    // avoids retaining raw pixels merely to populate the manifest.
    match png.get(25) {
        Some(2) => Ok(PixelMode::Rgb.manifest_name()),
        Some(6) => Ok(PixelMode::Rgba.manifest_name()),
        _ => Err(CaptureError::InvalidFrame),
    }
}

fn safe_component(value: &str) -> String {
    let safe: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    safe.trim_matches('-').to_owned()
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CaptureError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes).map_err(io_error)?;
    writer.flush().map_err(io_error)
}

fn io_error(error: std::io::Error) -> CaptureError {
    CaptureError::Io(error.to_string())
}

pub mod fixture;
