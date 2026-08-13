//! TASK 5181 - preserve Windows download-zone protection after a clean scan.
//!
//! After the TASK 5166 quarantine has scanned a protected download and released
//! it, the final save is handed to **Windows Attachment Services** through
//! `IAttachmentExecute` (`SetSource` / `SetReferrer` / `SetFileName` /
//! `SetLocalPath` / `CheckPolicy` / `Save`). `Save()` is what writes the
//! `Zone.Identifier` mark, so the file the user ends up with still says it came
//! from the Internet zone and Windows can keep applying its own reputation
//! (SmartScreen) and antivirus defenses to it later.
//!
//! This is **defense in depth**. It never substitutes for the OSL quarantine
//! scan: nothing reaches this module that the quarantine has not already
//! cleared, and a zone handoff cannot turn a detection into a release.
//!
//! Two rules keep the report honest:
//!
//! 1. **The mark is verified, not assumed.** The helper reads the
//!    `Zone.Identifier` stream back off the destination and this module parses
//!    `ZoneId` out of it. The `Save()` HRESULT is never treated as proof: on
//!    exFAT and on the 9P WSL volume `Save()` returns `S_OK` and no usable mark
//!    exists afterwards.
//! 2. **A filesystem that cannot hold the mark is reported as a platform
//!    limitation, and the handoff is not attempted there.** The outcome says so
//!    in words, `zone_marked()` is `false`, and no `ZoneId` is invented.
//!
//! The module deliberately depends on nothing but `std` and `base64`, so the
//! TASK 5181b starvation harness can compile this exact source file, starve the
//! handoff, and watch the check go red.

use std::fmt;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine as _;

/// The Internet zone (`URLZONE_INTERNET`). A download that came off the network
/// must end up marked with this.
pub const INTERNET_ZONE_ID: u32 = 3;

/// The one `IAttachmentExecute::Save` call a delivered download rests on.
pub const REQUIRED_SAVE_CALLS: u32 = 1;

/// Client identity handed to Attachment Services, so the mark and any later
/// Windows prompt name OSL rather than an anonymous process.
pub const CLIENT_TITLE: &str = "OSL Privacy protected download";

/// Stable per-application GUID for `IAttachmentExecute::SetClientGuid`.
pub const CLIENT_GUID: &str = "9b0fe1c1-2e4b-4b9b-9e45-5a5b5f4a1d10";

/// Filesystems that can actually retain an alternate data stream.
pub const ZONE_CAPABLE_FILESYSTEMS: [&str; 2] = ["NTFS", "ReFS"];

/// Default ceiling on one Attachment Services round trip.
pub const DEFAULT_HANDOFF_TIMEOUT_SECONDS: u64 = 30;

/// The zone-handoff helper, embedded rather than dropped on disk so no other
/// process can swap it out from under the boundary.
pub const ZONE_HANDOFF_HELPER_SCRIPT: &str = include_str!("download_zone_handoff.ps1");

// ---------------------------------------------------------------------------
// Request and outcome
// ---------------------------------------------------------------------------

/// One final save to hand to Windows Attachment Services.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneHandoffRequest {
    /// The path the file already occupies, as this process sees it.
    pub local_path: PathBuf,
    /// The name the user sees, handed to `SetFileName`.
    pub file_name: String,
    /// Where the download came from, handed to `SetSource`. This is what makes
    /// the mark say Internet zone.
    pub source_url: String,
    /// Handed to `SetReferrer`.
    pub referrer_url: String,
}

/// A verified `Zone.Identifier` mark: these fields were read back off the
/// destination after the save, not predicted before it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZoneMark {
    pub zone_id: u32,
    pub host_url: String,
    pub referrer_url: String,
    pub zone_identifier_text: String,
    pub filesystem: String,
    pub windows_path: String,
    pub save_calls: u32,
    pub save_hresult: String,
    pub check_policy_hresult: String,
    pub bytes_at_destination: u64,
}

/// Why this destination cannot carry the mark. Never a claim that it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneLimitation {
    /// The destination volume's filesystem does not carry alternate data
    /// streams (exFAT, FAT32, the 9P WSL volume, ...).
    FilesystemCannotRetainMark,
    /// The destination is not on a volume this Windows session can see.
    DestinationNotWindowsVisible,
}

impl ZoneLimitation {
    pub fn name(self) -> &'static str {
        match self {
            ZoneLimitation::FilesystemCannotRetainMark => "filesystem_cannot_retain_mark",
            ZoneLimitation::DestinationNotWindowsVisible => "destination_not_windows_visible",
        }
    }
}

impl fmt::Display for ZoneLimitation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A destination that kept the file but could not keep the mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformLimitation {
    pub reason: ZoneLimitation,
    pub filesystem: String,
    pub windows_path: String,
    pub save_calls: u32,
    pub message: String,
}

impl PlatformLimitation {
    fn new(
        reason: ZoneLimitation,
        filesystem: &str,
        windows_path: &str,
        detail: &str,
    ) -> Self {
        Self {
            reason,
            filesystem: filesystem.to_owned(),
            windows_path: windows_path.to_owned(),
            save_calls: 0,
            message: format!(
                "This download was saved, but Windows cannot keep an Internet-zone mark on it \
                 here: {detail}. No mark exists on this file, so Windows SmartScreen will not \
                 warn about it later. OSL still scanned it in quarantine before saving it."
            ),
        }
    }

    /// Structurally false. A limitation never reports a mark.
    pub fn zone_marked(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZoneHandoffOutcome {
    /// The mark was written and read back.
    Marked(ZoneMark),
    /// The file is at the destination; the platform cannot mark it.
    PlatformLimited(PlatformLimitation),
}

impl ZoneHandoffOutcome {
    pub fn zone_marked(&self) -> bool {
        matches!(self, ZoneHandoffOutcome::Marked(_))
    }

    pub fn zone_id(&self) -> Option<u32> {
        match self {
            ZoneHandoffOutcome::Marked(mark) => Some(mark.zone_id),
            ZoneHandoffOutcome::PlatformLimited(_) => None,
        }
    }

    pub fn save_calls(&self) -> u32 {
        match self {
            ZoneHandoffOutcome::Marked(mark) => mark.save_calls,
            ZoneHandoffOutcome::PlatformLimited(limited) => limited.save_calls,
        }
    }

    pub fn filesystem(&self) -> &str {
        match self {
            ZoneHandoffOutcome::Marked(mark) => &mark.filesystem,
            ZoneHandoffOutcome::PlatformLimited(limited) => &limited.filesystem,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ZoneHandoffOutcome::Marked(mark) => format!(
                "Saved with the Windows Internet-zone mark (ZoneId={}) from {}.",
                mark.zone_id, mark.host_url
            ),
            ZoneHandoffOutcome::PlatformLimited(limited) => limited.message.clone(),
        }
    }
}

/// Every way the handoff can fail to happen. All of them mean the file must not
/// be left sitting at the destination pretending to be a marked download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZoneHandoffFailure {
    /// Attachment Services could not be reached at all.
    HandoffAbsent(String),
    /// The helper ran and reported an error.
    HandoffError(String),
    /// The helper did not answer in time.
    Timeout(String),
    /// `IAttachmentExecute::Save` returned a failing HRESULT.
    SaveRefused(String),
    /// The number of `Save()` calls behind the mark is not exactly one.
    SaveCallCount(u32),
    /// The save reported success and no mark is on the file.
    MarkMissing(String),
    /// A mark is there, but not the Internet zone.
    WrongZone(u32),
    /// The file is no longer at the destination after the handoff (Windows
    /// removed it) - a real outcome, and not a delivery.
    DestinationGone(String),
}

impl ZoneHandoffFailure {
    pub fn name(&self) -> &'static str {
        match self {
            ZoneHandoffFailure::HandoffAbsent(_) => "zone_handoff_absent",
            ZoneHandoffFailure::HandoffError(_) => "zone_handoff_error",
            ZoneHandoffFailure::Timeout(_) => "zone_handoff_timeout",
            ZoneHandoffFailure::SaveRefused(_) => "zone_handoff_save_refused",
            ZoneHandoffFailure::SaveCallCount(_) => "zone_handoff_save_call_count",
            ZoneHandoffFailure::MarkMissing(_) => "zone_handoff_mark_missing",
            ZoneHandoffFailure::WrongZone(_) => "zone_handoff_wrong_zone",
            ZoneHandoffFailure::DestinationGone(_) => "zone_handoff_destination_gone",
        }
    }

    /// Text a person can read. Every variant names the missing zone handoff, so
    /// a starved handoff can never be mistaken for a delivered download.
    pub fn message(&self) -> String {
        let detail = match self {
            ZoneHandoffFailure::HandoffAbsent(detail)
            | ZoneHandoffFailure::HandoffError(detail)
            | ZoneHandoffFailure::Timeout(detail)
            | ZoneHandoffFailure::SaveRefused(detail)
            | ZoneHandoffFailure::MarkMissing(detail)
            | ZoneHandoffFailure::DestinationGone(detail) => detail.clone(),
            ZoneHandoffFailure::SaveCallCount(count) => format!(
                "the mark rests on {count} Attachment Services save calls, not the required \
                 {REQUIRED_SAVE_CALLS}"
            ),
            ZoneHandoffFailure::WrongZone(zone) => {
                format!("the mark says ZoneId={zone}, not the Internet zone {INTERNET_ZONE_ID}")
            }
        };
        format!(
            "The Windows zone handoff is missing, so this download was not saved. \
             Local reason: {} ({detail}). OSL will not leave a downloaded file on disk without \
             the Internet-zone mark Windows needs to keep defending it.",
            self.name()
        )
    }
}

// ---------------------------------------------------------------------------
// The boundary
// ---------------------------------------------------------------------------

/// Anything that can hand a saved file to Windows Attachment Services. The
/// shipping implementation is [`WindowsAttachmentServicesSaver`]; the checks
/// wrap it to count calls.
pub trait AttachmentServicesSaver {
    fn save(&self, request: &ZoneHandoffRequest)
        -> Result<ZoneHandoffOutcome, ZoneHandoffFailure>;
}

/// Calls the real Windows Attachment Services COM server through a
/// `powershell.exe` host running the embedded helper.
pub struct WindowsAttachmentServicesSaver {
    powershell: PathBuf,
    timeout: Duration,
}

impl Default for WindowsAttachmentServicesSaver {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsAttachmentServicesSaver {
    /// Absolute System32 path, so a hijacked `PATH` cannot substitute the host.
    pub fn new() -> Self {
        Self {
            powershell: default_powershell_path(),
            timeout: Duration::from_secs(DEFAULT_HANDOFF_TIMEOUT_SECONDS),
        }
    }

    pub fn with_powershell(mut self, powershell: PathBuf) -> Self {
        self.powershell = powershell;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn powershell_path(&self) -> &Path {
        &self.powershell
    }

    fn encoded_command() -> String {
        let mut utf16 = Vec::with_capacity(ZONE_HANDOFF_HELPER_SCRIPT.len() * 2);
        for unit in ZONE_HANDOFF_HELPER_SCRIPT.encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        base64::engine::general_purpose::STANDARD.encode(utf16)
    }
}

impl AttachmentServicesSaver for WindowsAttachmentServicesSaver {
    fn save(
        &self,
        request: &ZoneHandoffRequest,
    ) -> Result<ZoneHandoffOutcome, ZoneHandoffFailure> {
        if !self.powershell.exists() {
            return Err(ZoneHandoffFailure::HandoffAbsent(format!(
                "the Windows Attachment Services host {} is not present",
                self.powershell.display()
            )));
        }
        let windows_path = match windows_path_for(&request.local_path) {
            Ok(path) => path,
            Err(detail) => {
                return Ok(ZoneHandoffOutcome::PlatformLimited(PlatformLimitation::new(
                    ZoneLimitation::DestinationNotWindowsVisible,
                    "",
                    &request.local_path.display().to_string(),
                    &detail,
                )))
            }
        };

        let mut document = String::new();
        document.push_str(&format!("LOCAL_PATH={windows_path}\n"));
        document.push_str(&format!("FILE_NAME={}\n", request.file_name));
        document.push_str(&format!("SOURCE_URL={}\n", request.source_url));
        document.push_str(&format!("REFERRER_URL={}\n", request.referrer_url));
        document.push_str(&format!("CLIENT_TITLE={CLIENT_TITLE}\n"));
        document.push_str(&format!("CLIENT_GUID={CLIENT_GUID}\n"));
        let payload = base64::engine::general_purpose::STANDARD.encode(document.as_bytes());

        let mut child = Command::new(&self.powershell)
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-ExecutionPolicy")
            .arg("Bypass")
            .arg("-EncodedCommand")
            .arg(Self::encoded_command())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                ZoneHandoffFailure::HandoffAbsent(format!(
                    "the Windows Attachment Services host would not start: {error}"
                ))
            })?;

        let mut stdin = child.stdin.take().ok_or_else(|| {
            ZoneHandoffFailure::HandoffError("Attachment Services stdin was unavailable".to_owned())
        })?;
        let writer = std::thread::spawn(move || {
            let _ = stdin.write_all(payload.as_bytes());
            let _ = stdin.flush();
            drop(stdin);
        });
        let mut stdout = child.stdout.take().ok_or_else(|| {
            ZoneHandoffFailure::HandoffError("Attachment Services stdout was unavailable".to_owned())
        })?;
        let reader = std::thread::spawn(move || {
            let mut buffer = String::new();
            let _ = stdout.read_to_string(&mut buffer);
            buffer
        });

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    return Err(ZoneHandoffFailure::HandoffError(format!(
                        "the Attachment Services host could not be waited on: {error}"
                    )));
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ZoneHandoffFailure::Timeout(format!(
                    "Windows Attachment Services did not answer within {} ms",
                    self.timeout.as_millis()
                )));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let output = reader.join().unwrap_or_default();
        let _ = writer.join();
        if !status.success() {
            return Err(ZoneHandoffFailure::HandoffError(format!(
                "the Attachment Services host exited with {status}"
            )));
        }
        parse_zone_handoff_output(&output)
    }
}

/// Parse the embedded helper's `OSL5181_KEY=value` lines into an outcome.
pub fn parse_zone_handoff_output(
    output: &str,
) -> Result<ZoneHandoffOutcome, ZoneHandoffFailure> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\r').trim();
        if let Some(rest) = line.strip_prefix("OSL5181_") {
            if let Some((key, value)) = rest.split_once('=') {
                fields.push((key.to_owned(), value.to_owned()));
            }
        }
    }
    let get = |key: &str| -> Option<&str> {
        fields
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    let filesystem = get("FILESYSTEM").unwrap_or("").to_owned();
    let windows_path = get("WINDOWS_PATH").unwrap_or("").to_owned();
    let detail = get("DETAIL").unwrap_or("no detail reported").to_owned();

    match get("STATUS") {
        Some("handed_off") => {}
        Some("platform_unsupported") => {
            return Ok(ZoneHandoffOutcome::PlatformLimited(PlatformLimitation::new(
                ZoneLimitation::FilesystemCannotRetainMark,
                &filesystem,
                &windows_path,
                &detail,
            )))
        }
        Some("destination_missing") => {
            return Err(ZoneHandoffFailure::DestinationGone(detail))
        }
        Some("handoff_error") => return Err(ZoneHandoffFailure::HandoffError(detail)),
        Some(other) => {
            return Err(ZoneHandoffFailure::HandoffError(format!(
                "the Attachment Services helper reported an unknown status {other}"
            )))
        }
        None => {
            return Err(ZoneHandoffFailure::HandoffAbsent(
                "the Attachment Services helper reported no status".to_owned(),
            ))
        }
    }

    if let Some(absent) = get("HANDOFF_ABSENT") {
        return Err(ZoneHandoffFailure::HandoffAbsent(absent.to_owned()));
    }

    let save_calls = get("SAVE_CALLS")
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0);
    if save_calls != REQUIRED_SAVE_CALLS {
        return Err(ZoneHandoffFailure::SaveCallCount(save_calls));
    }
    let save_hresult = get("HR_SAVE").unwrap_or("").trim().to_owned();
    if save_hresult.is_empty() {
        return Err(ZoneHandoffFailure::HandoffError(
            "Attachment Services reported no Save() HRESULT".to_owned(),
        ));
    }
    if !is_success_hresult(&save_hresult) {
        return Err(ZoneHandoffFailure::SaveRefused(format!(
            "IAttachmentExecute::Save returned hr={save_hresult}"
        )));
    }
    if get("FILE_EXISTS_AFTER") != Some("true") {
        return Err(ZoneHandoffFailure::DestinationGone(format!(
            "Windows removed {windows_path} during the Attachment Services save"
        )));
    }

    // The mark is whatever is really in the stream. An empty stream is no mark:
    // on the 9P WSL volume the write is accepted and reads back empty.
    let zone_text = match get("ZONE_B64") {
        Some(encoded) if !encoded.is_empty() => {
            match base64::engine::general_purpose::STANDARD.decode(encoded) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                Err(error) => {
                    return Err(ZoneHandoffFailure::MarkMissing(format!(
                        "the Zone.Identifier stream could not be decoded: {error}"
                    )))
                }
            }
        }
        _ => String::new(),
    };
    if zone_text.trim().is_empty() {
        let read_error = get("ZONE_READ_ERROR").unwrap_or("the stream is empty");
        return Err(ZoneHandoffFailure::MarkMissing(format!(
            "no Zone.Identifier mark is on {windows_path} after the save ({read_error})"
        )));
    }
    let zone_id = match zone_identifier_value(&zone_text, "ZoneId")
        .and_then(|value| value.trim().parse::<u32>().ok())
    {
        Some(zone) => zone,
        None => {
            return Err(ZoneHandoffFailure::MarkMissing(format!(
                "the Zone.Identifier stream on {windows_path} carries no ZoneId"
            )))
        }
    };
    if zone_id != INTERNET_ZONE_ID {
        return Err(ZoneHandoffFailure::WrongZone(zone_id));
    }

    Ok(ZoneHandoffOutcome::Marked(ZoneMark {
        zone_id,
        host_url: zone_identifier_value(&zone_text, "HostUrl").unwrap_or_default(),
        referrer_url: zone_identifier_value(&zone_text, "ReferrerUrl").unwrap_or_default(),
        zone_identifier_text: zone_text,
        filesystem,
        windows_path,
        save_calls,
        save_hresult,
        check_policy_hresult: get("HR_CHECK_POLICY").unwrap_or("").trim().to_owned(),
        bytes_at_destination: get("FILE_LEN_AFTER")
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0),
    }))
}

/// Pull one `Key=Value` out of a `Zone.Identifier` stream body.
pub fn zone_identifier_value(zone_text: &str, key: &str) -> Option<String> {
    for line in zone_text.lines() {
        let line = line.trim_end_matches('\r').trim();
        if let Some((name, value)) = line.split_once('=') {
            if name.trim().eq_ignore_ascii_case(key) {
                return Some(value.trim().to_owned());
            }
        }
    }
    None
}

/// `SUCCEEDED(hr)`: the top bit clear. `S_FALSE` (`0x00000001`) is a success.
pub fn is_success_hresult(hresult: &str) -> bool {
    let text = hresult.trim().trim_start_matches("0x");
    match u32::from_str_radix(text, 16) {
        Ok(value) => value & 0x8000_0000 == 0,
        Err(_) => false,
    }
}

/// True when a filesystem can retain an alternate data stream.
pub fn filesystem_can_retain_mark(filesystem: &str) -> bool {
    ZONE_CAPABLE_FILESYSTEMS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(filesystem.trim()))
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// The path as the interactive Windows session sees it.
///
/// On Windows this is the path itself. From WSL a DrvFs mount (`/mnt/c/...`)
/// becomes its drive-letter path, and anything else becomes the `\\wsl.localhost`
/// UNC path for this distribution - which Windows really can open, and which
/// really cannot hold an alternate data stream, so it is reported as a platform
/// limitation rather than marked.
pub fn windows_path_for(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err(format!(
            "{} is not an absolute path",
            path.display()
        ));
    }
    if cfg!(windows) {
        return Ok(path.display().to_string());
    }
    let components: Vec<String> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if components.len() >= 2 && components[0] == "mnt" && components[1].len() == 1 {
        let letter = components[1].to_uppercase();
        let rest = components[2..].join("\\");
        if rest.is_empty() {
            return Ok(format!("{letter}:\\"));
        }
        return Ok(format!("{letter}:\\{rest}"));
    }
    if let Ok(distro) = std::env::var("WSL_DISTRO_NAME") {
        if !distro.is_empty() {
            return Ok(format!(
                "\\\\wsl.localhost\\{distro}\\{}",
                components.join("\\")
            ));
        }
    }
    match Command::new("wslpath").arg("-w").arg(path).output() {
        Ok(output) if output.status.success() => {
            let translated = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if translated.is_empty() {
                Err(format!(
                    "{} has no path in the Windows session",
                    path.display()
                ))
            } else {
                Ok(translated)
            }
        }
        _ => Err(format!(
            "{} has no path in the Windows session",
            path.display()
        )),
    }
}

fn default_powershell_path() -> PathBuf {
    // From WSL the same interactive Windows session is reached through the
    // DrvFs mount; on Windows itself the drive-letter path is the real one.
    let mounted = Path::new("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
    if mounted.exists() {
        return mounted.to_owned();
    }
    PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
}
