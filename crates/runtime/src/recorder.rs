//! Hourly process scan for screen-recording software.
//!
//! Spec: `docs/design/sender-keys.md` "Suspicious-event auto-rotation"
//! row "Screen-recording software detected" + the build-order Group-A
//! row "Process scanning for screen recorders". On detection the
//! caller fires a `Suspicious(ScreenRecorder)` rotation through
//! [`crate::rotation::RotationController`].
//!
//! ## Match list
//!
//! Maintained as a static set of lowercase executable basenames.
//! Future updates extend this list — new entries are additive and
//! do not break existing behaviour. Match is case-insensitive on the
//! basename only (path-stripped). Any new entry should be cross-
//! referenced against documented screen-recorder products and added
//! with a one-line provenance comment.
//!
//! Listing is conservative — entries are products with documented
//! screen / window capture functionality. Generic communication
//! tools that *can* screen-share (Discord, Teams, Slack, Zoom in
//! one-on-one) are deliberately NOT in the list, because their
//! presence is unrelated to active recording.
//!
//! ## Cadence
//!
//! Per design doc: hourly process scan. Caller drives the cadence;
//! the runtime here exposes a [`scan`] function that takes a
//! caller-provided process list and returns the matched names. The
//! actual `CreateToolhelp32Snapshot` enumeration lives in the
//! `imp_windows` Windows-only path; non-Windows builds are stubbed
//! so the rest of the workspace builds on dev environments.
//!
//! ## DoS
//!
//! Suspicious events go through [`RotationController`]'s 5-min cap.
//! A user with OBS in their startup folder won't trigger a rotation
//! on every hourly scan after the first.

use thiserror::Error;

/// Lowercase basename match list. Comments record the product
/// referenced by each entry.
pub const RECORDER_PROCESS_NAMES: &[&str] = &[
    "obs64.exe",                  // OBS Studio (64-bit)
    "obs32.exe",                  // OBS Studio (32-bit, legacy)
    "obs.exe",                    // OBS Studio (older builds)
    "bandicam.exe",               // Bandicam
    "camtasia.exe",               // Camtasia (TechSmith)
    "camtasia_studio.exe",        // Camtasia (older builds)
    "camrec.exe",                 // Camtasia recorder helper
    "sharex.exe",                 // ShareX
    "nvcontainer.exe",            // NVIDIA ShadowPlay container
    "nvidia share.exe",           // NVIDIA Share / GeForce Experience overlay
    "nvidia overlay.exe",         // NVIDIA overlay process (capture path)
    "broadcast.exe",              // NVIDIA Broadcast (uses capture)
    "gamebar.exe",                // Windows Game Bar
    "gamebarpresencewriter.exe",  // Windows Game Bar helper
    "screenrec.exe",              // Screenrec
    "fraps.exe",                  // Fraps
    "dxtory.exe",                 // Dxtory
    "actionnow.exe",              // Mirillis Action!
    "screenpresso.exe",           // Screenpresso
    "icecream screen recorder.exe", // Icecream Screen Recorder
    "snippingtool.exe",           // Windows Snipping Tool (legacy)
    "screenclippinghost.exe",     // Windows Snip & Sketch host
    "screensketch.exe",           // Windows Snip & Sketch
    "loom.exe",                   // Loom desktop
    "vokoscreenng.exe",           // vokoscreenNG
    "flashback.exe",              // FlashBack Express
    "movavi screen recorder.exe", // Movavi Screen Recorder
    "ezvid.exe",                  // Ezvid
    "fastcap.exe",                // FastCap
    "snagit32.exe",               // Snagit (TechSmith)
    "snagiteditor.exe",           // Snagit editor (record helper)
    "xsplit.core.exe",            // XSplit Broadcaster
    "xsplit.gamecaster.exe",      // XSplit Gamecaster
    "lightshot.exe",              // Lightshot (screen-region capture)
    "greenshot.exe",              // Greenshot
];

#[derive(Debug, Error)]
pub enum RecorderScanError {
    #[error("Win32 process enumeration failed: {0}")]
    Win32(String),
}

pub type Result<T> = core::result::Result<T, RecorderScanError>;

/// Run the recorder match against `process_basenames`. Each entry
/// must be the bare executable name (path-stripped). Comparison is
/// case-insensitive. Returns the matched names (deduplicated, in
/// match-list order, lowercased) so the caller can log /
/// audit-trail them.
pub fn match_recorders<S: AsRef<str>>(process_basenames: &[S]) -> Vec<&'static str> {
    let lowercase: std::collections::HashSet<String> = process_basenames
        .iter()
        .map(|s| s.as_ref().to_ascii_lowercase())
        .collect();
    RECORDER_PROCESS_NAMES
        .iter()
        .filter(|&&name| lowercase.contains(&name.to_ascii_lowercase()))
        .copied()
        .collect()
}

/// Snapshot the running processes on this OS and return their
/// basename strings. **Windows-only**; non-Windows returns
/// `Err(RecorderScanError::Win32("unsupported"))` so callers know
/// scan results are unavailable and can degrade gracefully.
pub fn snapshot_running_processes() -> Result<Vec<String>> {
    imp::snapshot()
}

/// Convenience: take a snapshot and run the match in one call. On
/// non-Windows this returns an error rather than a silent empty list.
pub fn scan() -> Result<Vec<&'static str>> {
    let names = snapshot_running_processes()?;
    Ok(match_recorders(&names))
}

/// Callback invoked when at least one recorder is detected on a
/// scan. Receives the list of matched names so the caller can log /
/// audit them. `Send + Sync + 'static` because the scanner thread
/// invokes it.
pub type DetectionCallback =
    Box<dyn Fn(&[&'static str]) + Send + Sync + 'static>;

/// Periodic recorder scanner. Spawns a dedicated thread that calls
/// [`scan`] every `interval`. The default interval is 1 hour
/// (matches `docs/design/sender-keys.md`'s "Hourly process scan").
///
/// Drop the [`RecorderScanner`] to stop the thread; the thread joins
/// promptly via a shared "stop" flag + bounded sleep ticks (default
/// tick: 1 s, configurable via [`RecorderScannerConfig`]).
///
/// Errors from `scan()` (e.g. the non-Windows
/// [`RecorderScanError::Win32`] stub) are routed to the optional
/// [`RecorderScannerConfig::on_error`] callback. The scan loop
/// continues regardless — a transient Win32 enumeration failure
/// shouldn't disable the trigger forever.
pub struct RecorderScanner {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

pub struct RecorderScannerConfig {
    pub interval: std::time::Duration,
    pub tick: std::time::Duration,
}

impl Default for RecorderScannerConfig {
    fn default() -> Self {
        RecorderScannerConfig {
            interval: std::time::Duration::from_secs(60 * 60),
            tick: std::time::Duration::from_secs(1),
        }
    }
}

impl RecorderScanner {
    /// Spawn the scanner. `on_detect` fires on each scan that finds
    /// at least one recorder; an empty match-list does NOT fire it.
    pub fn start(
        config: RecorderScannerConfig,
        on_detect: DetectionCallback,
    ) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_for_thread = stop.clone();
        let join = std::thread::Builder::new()
            .name("dpc-recorder-scan".into())
            .spawn(move || {
                let mut elapsed = std::time::Duration::ZERO;
                loop {
                    if stop_for_thread.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    if elapsed >= config.interval {
                        elapsed = std::time::Duration::ZERO;
                        match scan() {
                            Ok(matches) if !matches.is_empty() => {
                                on_detect(&matches);
                            }
                            Ok(_) => {
                                // No recorders — do nothing.
                            }
                            Err(e) => {
                                tracing::debug!(
                                    error = %e,
                                    "recorder scan failed; continuing"
                                );
                            }
                        }
                    }
                    std::thread::sleep(config.tick);
                    elapsed += config.tick;
                }
            })
            .expect("spawn recorder scanner thread");
        RecorderScanner {
            stop,
            join: Some(join),
        }
    }
}

impl Drop for RecorderScanner {
    fn drop(&mut self) {
        self.stop
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::{RecorderScanError, Result};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    pub(super) fn snapshot() -> Result<Vec<String>> {
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| {
                RecorderScanError::Win32(format!("CreateToolhelp32Snapshot: {e}"))
            })?;

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            let mut names = Vec::new();
            if Process32FirstW(snap, &mut entry).is_ok() {
                loop {
                    // szExeFile is a fixed [u16; 260] WTF-16 buffer
                    // ending in NUL.
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                    if !name.is_empty() {
                        names.push(name);
                    }
                    if Process32NextW(snap, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
            Ok(names)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{RecorderScanError, Result};

    pub(super) fn snapshot() -> Result<Vec<String>> {
        Err(RecorderScanError::Win32(
            "process enumeration unsupported on non-Windows targets".into(),
        ))
    }
}
