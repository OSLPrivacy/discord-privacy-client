//! TASK 5180 - unpack protected archives within hard quarantine limits.
//!
//! A protected download that turns out to be a *container* must not leave the
//! TASK 5166 quarantine on the strength of one whole-file antimalware verdict:
//! the bytes a user actually opens are the entries, and the whole-file verdict
//! says nothing about them. So a container is expanded - only inside an
//! `archive-expansion-*` workspace below the same OSL-private quarantine root,
//! only after the download has been decrypted into that quarantine - and every
//! entry gets its own local receipt from the same
//! [`ProtectedDownloadQuarantine::scan`] rules as the whole file. Only when
//! every entry has scanned clean does the original archive take the 5166
//! atomic release.
//!
//! Expansion is bounded four ways, and every bound is enforced *while* bytes
//! are being written, never after:
//!
//! * expanded bytes  - checked against the next chunk before that chunk is
//!   written, so the workspace never holds more than the ceiling;
//! * entry count     - checked before an entry's bytes are written at all;
//! * nesting depth   - checked on entry to a container, before it is walked;
//! * scan time       - one wall-clock deadline, re-checked before every
//!   container, before every entry, on every 64 KiB chunk, and after every
//!   entry receipt.
//!
//! Nothing extracted here is ever the thing that is released. Each entry file
//! is removed as soon as it has been scanned and recursed into, and the whole
//! workspace is destroyed on every exit path, refusal or not. The only way
//! bytes leave quarantine is still 5166's single atomic `rename` of the
//! original archive.
//!
//! TASK 5180a adds the hostile-entry rejections on top of those bounds. An
//! archive entry is refused outright - inside quarantine, before it is counted
//! and before a single byte of it is written - when it is
//!
//! * an absolute path (`/etc/cron.d/x`, or a Windows drive prefix);
//! * a parent traversal (`../../x`, at any position in the name);
//! * a symbolic link or a hard link, whatever it points at;
//! * a special file - character or block device, FIFO, socket, or any other
//!   archive entry type that is not a plain file or a plain directory.
//!
//! None of these is normalised, sanitised or rewritten into the workspace: a
//! hostile name is a refusal, and the refusal names the entry, the kind and how
//! much had been unpacked when it fired (always zero for that entry). The
//! destination path is *built* from validated components, so there is no code
//! path that could write a hostile entry and check afterwards.
//!
//! Scope note: password-protected or otherwise encrypted archives belong to
//! TASK 5180c. This module fails closed on them today - a container this build
//! cannot walk is refused as unsupported - so that open task cannot be reached
//! by releasing bytes.
//!
//! This module deliberately keeps every bound behind one guard line, marked
//! by a `TASK5180-BOUND-*` or `TASK5180A-GUARD-*` comment directly above it, so
//! the TASK 5180b starvation harness can compile these exact sources, disable
//! one guard at a time, and watch the check go red.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::protected_download_quarantine::{
    AmsiProvider, ProtectedDownloadQuarantine, QuarantineReason, QuarantinedDownload,
};

/// Most plaintext one protected archive may ever expand to inside quarantine.
pub const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
/// Most entries one protected archive may ever expand to, counted across every
/// nesting level.
pub const MAX_ENTRIES: u32 = 4096;
/// Deepest container nesting that is walked. The outermost archive is level 1.
pub const MAX_DEPTH: u32 = 3;
/// Longest wall-clock budget for expanding and scanning one archive.
pub const MAX_SCAN_TIME_MS: u64 = 120_000;

/// Copy granularity. Also the granularity at which the byte ceiling and the
/// deadline are enforced mid-entry.
const COPY_CHUNK_BYTES: usize = 64 * 1024;
/// Enough of a file's head to recognise every container format below, and to
/// reach the ustar magic at offset 257.
const SNIFF_BYTES: usize = 512;

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

/// The four hard bounds. A caller may only ever *tighten* them: see
/// [`ArchiveLimits::clamped`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub max_expanded_bytes: u64,
    pub max_entries: u32,
    pub max_depth: u32,
    pub max_scan_time: Duration,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self::shipping()
    }
}

impl ArchiveLimits {
    /// The ceilings this build ships with.
    pub fn shipping() -> Self {
        Self {
            max_expanded_bytes: MAX_EXPANDED_BYTES,
            max_entries: MAX_ENTRIES,
            max_depth: MAX_DEPTH,
            max_scan_time: Duration::from_millis(MAX_SCAN_TIME_MS),
        }
    }

    /// Every field pinned at or below the shipping ceiling. A caller asking for
    /// a wider bound gets the shipping one back, so no configuration path can
    /// raise a limit.
    pub fn clamped(self) -> Self {
        let shipping = Self::shipping();
        Self {
            max_expanded_bytes: self.max_expanded_bytes.min(shipping.max_expanded_bytes),
            max_entries: self.max_entries.min(shipping.max_entries),
            max_depth: self.max_depth.min(shipping.max_depth),
            max_scan_time: self.max_scan_time.min(shipping.max_scan_time),
        }
    }

    pub fn with_max_expanded_bytes(mut self, bytes: u64) -> Self {
        self.max_expanded_bytes = bytes;
        self
    }

    pub fn with_max_entries(mut self, entries: u32) -> Self {
        self.max_entries = entries;
        self
    }

    pub fn with_max_depth(mut self, depth: u32) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn with_max_scan_time(mut self, scan_time: Duration) -> Self {
        self.max_scan_time = scan_time;
        self
    }

    /// One line naming every ceiling in force, for the owner-facing log.
    pub fn describe(&self) -> String {
        format!(
            "expanded-bytes:{},entry-count:{},nesting-depth:{},scan-time-ms:{}",
            self.max_expanded_bytes,
            self.max_entries,
            self.max_depth,
            self.max_scan_time.as_millis()
        )
    }
}

// ---------------------------------------------------------------------------
// Format detection - from content, never from the sender's file name
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    Gzip,
    /// A container this build recognises but cannot walk. Fail closed: it is
    /// never treated as an ordinary file.
    Unsupported(&'static str),
    NotAnArchive,
}

impl ArchiveFormat {
    pub fn name(&self) -> String {
        match self {
            ArchiveFormat::Zip => "zip".to_owned(),
            ArchiveFormat::Tar => "tar".to_owned(),
            ArchiveFormat::Gzip => "gzip".to_owned(),
            ArchiveFormat::Unsupported(name) => (*name).to_owned(),
            ArchiveFormat::NotAnArchive => "not-an-archive".to_owned(),
        }
    }

    /// Whether this file has to be expanded and scanned entry by entry before
    /// anything may be released.
    pub fn needs_inspection(&self) -> bool {
        !matches!(self, ArchiveFormat::NotAnArchive)
    }
}

/// Recognise a container from its leading bytes. The sender's file name is
/// never consulted: a `.txt` that is really a zip is still a zip.
pub fn detect_archive_format(prefix: &[u8]) -> ArchiveFormat {
    if prefix.starts_with(b"PK\x03\x04")
        || prefix.starts_with(b"PK\x05\x06")
        || prefix.starts_with(b"PK\x07\x08")
    {
        return ArchiveFormat::Zip;
    }
    if prefix.starts_with(&[0x1f, 0x8b]) {
        return ArchiveFormat::Gzip;
    }
    if prefix.len() >= 262 && &prefix[257..262] == b"ustar" {
        return ArchiveFormat::Tar;
    }
    if prefix.starts_with(&[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]) {
        return ArchiveFormat::Unsupported("7-Zip");
    }
    if prefix.starts_with(b"Rar!\x1a\x07") {
        return ArchiveFormat::Unsupported("RAR");
    }
    if prefix.starts_with(&[0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00]) {
        return ArchiveFormat::Unsupported("XZ");
    }
    if prefix.starts_with(b"BZh") {
        return ArchiveFormat::Unsupported("bzip2");
    }
    ArchiveFormat::NotAnArchive
}

/// Read only the head of a quarantined file and classify it.
pub fn sniff_archive_format(path: &Path) -> Result<ArchiveFormat, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut head = vec![0u8; SNIFF_BYTES];
    let mut filled = 0usize;
    while filled < head.len() {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) => return Err(error.to_string()),
        }
    }
    head.truncate(filled);
    Ok(detect_archive_format(&head))
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// The hostile archive-entry shapes TASK 5180a refuses outright. Every one of
/// them is refused inside quarantine, before the entry is counted and before a
/// single byte of it is written anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostileEntryKind {
    /// `/etc/cron.d/x`, `C:\Windows\x` - a name that would leave the workspace
    /// by starting somewhere else entirely.
    AbsolutePath,
    /// `../x`, `a/../../x` - a name that would climb out of the workspace.
    ParentTraversal,
    /// A symbolic link entry, whatever it points at.
    SymbolicLink,
    /// A hard link entry, whatever it points at.
    HardLink,
    /// A character or block device, a FIFO, a socket, or any other entry type
    /// that is neither a plain file nor a plain directory.
    SpecialFile,
    /// A name that is not usable at all: empty, control characters, or naming
    /// no file once `.` components are dropped.
    MalformedName,
}

impl HostileEntryKind {
    /// Machine-readable name, used as the `limit=` tag of the refusal.
    pub fn name(self) -> &'static str {
        match self {
            HostileEntryKind::AbsolutePath => "absolute-path",
            HostileEntryKind::ParentTraversal => "parent-traversal",
            HostileEntryKind::SymbolicLink => "symbolic-link",
            HostileEntryKind::HardLink => "hard-link",
            HostileEntryKind::SpecialFile => "special-file",
            HostileEntryKind::MalformedName => "malformed-name",
        }
    }

    /// How the 5166 quarantine records this rejection.
    pub fn quarantine_reason(self) -> QuarantineReason {
        match self {
            HostileEntryKind::AbsolutePath => QuarantineReason::ArchiveAbsolutePath,
            HostileEntryKind::ParentTraversal => QuarantineReason::ArchiveParentTraversal,
            HostileEntryKind::SymbolicLink => QuarantineReason::ArchiveSymbolicLink,
            HostileEntryKind::HardLink => QuarantineReason::ArchiveHardLink,
            HostileEntryKind::SpecialFile => QuarantineReason::ArchiveSpecialFile,
            HostileEntryKind::MalformedName => QuarantineReason::ArchiveUnsafeEntry,
        }
    }
}

/// Why an archive stayed in quarantine. Every variant names the bound or the
/// entry that stopped it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveRefusal {
    ExpandedBytes {
        limit: u64,
        expanded_before_stop: u64,
        entry: String,
    },
    EntryCount {
        limit: u32,
        entries_seen: u32,
        entry: String,
    },
    NestingDepth {
        limit: u32,
        level: u32,
        container: String,
    },
    ScanTime {
        limit_ms: u128,
        elapsed_ms: u128,
    },
    /// TASK 5180a. The entry is an absolute path, a parent traversal, a link or
    /// a special file. `entries_unpacked` and `bytes_unpacked` are what the
    /// expansion had already done when the guard fired - the refused entry
    /// itself contributes nothing to either, because the guard runs before the
    /// entry is counted and before its first byte is written.
    UnsafeEntry {
        kind: HostileEntryKind,
        entry: String,
        /// Where the entry pointed - the link target, or empty when it named
        /// no target.
        target: String,
        detail: String,
        entries_unpacked: u32,
        bytes_unpacked: u64,
    },
    /// A container this build cannot walk, including encrypted ones (TASK
    /// 5180c). Unable to verify, never released.
    UnsupportedFormat {
        format: String,
    },
    /// The archive, or an entry of it, could not be read at all.
    Unreadable {
        detail: String,
    },
    /// An entry did not earn its own clean local receipt.
    EntryNotClean {
        entry: String,
        detail: String,
    },
    /// The expansion workspace is not inside the OSL quarantine. Structural:
    /// the boundary refuses to expand anything anywhere else.
    OutsideQuarantine {
        workspace: String,
        quarantine_root: String,
    },
}

impl ArchiveRefusal {
    /// Machine-readable name of the bound that stopped the archive.
    pub fn limit_name(&self) -> &'static str {
        match self {
            ArchiveRefusal::ExpandedBytes { .. } => "expanded-bytes",
            ArchiveRefusal::EntryCount { .. } => "entry-count",
            ArchiveRefusal::NestingDepth { .. } => "nesting-depth",
            ArchiveRefusal::ScanTime { .. } => "scan-time",
            ArchiveRefusal::UnsafeEntry { kind, .. } => kind.name(),
            ArchiveRefusal::UnsupportedFormat { .. } => "unsupported-format",
            ArchiveRefusal::Unreadable { .. } => "unreadable-archive",
            ArchiveRefusal::EntryNotClean { .. } => "entry-not-clean",
            ArchiveRefusal::OutsideQuarantine { .. } => "quarantine-root",
        }
    }

    /// Owner-facing sentence. It names the limit and the number behind it, so a
    /// refusal is never a shrug.
    pub fn reason(&self) -> String {
        match self {
            ArchiveRefusal::ExpandedBytes {
                limit,
                expanded_before_stop,
                entry,
            } => format!(
                "limit=expanded-bytes the archive exceeded the expanded-byte limit of {limit} \
                 bytes while unpacking '{entry}' (stopped at {expanded_before_stop} bytes)"
            ),
            ArchiveRefusal::EntryCount {
                limit,
                entries_seen,
                entry,
            } => format!(
                "limit=entry-count the archive exceeded the entry-count limit of {limit} entries \
                 at '{entry}' ({entries_seen} entries already unpacked)"
            ),
            ArchiveRefusal::NestingDepth {
                limit,
                level,
                container,
            } => format!(
                "limit=nesting-depth the archive exceeded the nesting-depth limit of {limit} \
                 levels at '{container}' (level {level})"
            ),
            ArchiveRefusal::ScanTime {
                limit_ms,
                elapsed_ms,
            } => format!(
                "limit=scan-time the archive exceeded the scan-time limit of {limit_ms} ms \
                 (stopped at {elapsed_ms} ms)"
            ),
            ArchiveRefusal::UnsafeEntry {
                kind,
                entry,
                target,
                detail,
                entries_unpacked,
                bytes_unpacked,
            } => {
                let pointed = if target.is_empty() {
                    String::new()
                } else {
                    format!(" -> '{target}'")
                };
                format!(
                    "limit={} archive entry '{entry}'{pointed} was refused before any of its \
                     bytes were unpacked: {detail} (entries-unpacked-before-refusal \
                     {entries_unpacked}, bytes-unpacked-before-refusal {bytes_unpacked})",
                    kind.name()
                )
            }
            ArchiveRefusal::UnsupportedFormat { format } => format!(
                "limit=unsupported-format unable to verify a {format} container: this build \
                 cannot walk its entries"
            ),
            ArchiveRefusal::Unreadable { detail } => {
                format!("limit=unreadable-archive the archive could not be read: {detail}")
            }
            ArchiveRefusal::EntryNotClean { entry, detail } => format!(
                "limit=entry-not-clean archive entry '{entry}' did not scan clean: {detail}"
            ),
            ArchiveRefusal::OutsideQuarantine {
                workspace,
                quarantine_root,
            } => format!(
                "limit=quarantine-root the expansion workspace {workspace} is not inside the OSL \
                 quarantine {quarantine_root}"
            ),
        }
    }

    /// How the 5166 quarantine records this refusal.
    pub fn quarantine_reason(&self) -> QuarantineReason {
        match self {
            ArchiveRefusal::ExpandedBytes { .. } => QuarantineReason::ArchiveExpandedBytes,
            ArchiveRefusal::EntryCount { .. } => QuarantineReason::ArchiveEntryCount,
            ArchiveRefusal::NestingDepth { .. } => QuarantineReason::ArchiveNestingDepth,
            ArchiveRefusal::ScanTime { .. } => QuarantineReason::ArchiveScanTime,
            ArchiveRefusal::UnsafeEntry { kind, .. } => kind.quarantine_reason(),
            ArchiveRefusal::UnsupportedFormat { .. } => QuarantineReason::ArchiveUnsupported,
            ArchiveRefusal::Unreadable { .. } => QuarantineReason::ArchiveUnreadable,
            ArchiveRefusal::EntryNotClean { .. } => QuarantineReason::ArchiveEntryNotClean,
            ArchiveRefusal::OutsideQuarantine { .. } => QuarantineReason::ArchiveOutsideQuarantine,
        }
    }
}

// ---------------------------------------------------------------------------
// What an inspection measured
// ---------------------------------------------------------------------------

/// Everything the expansion actually did. Counts are kept as the work happens,
/// so a release can be checked against them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveInspection {
    pub format: String,
    pub entries_seen: u32,
    pub entries_scanned: u32,
    pub expanded_bytes: u64,
    pub scanned_bytes: u64,
    pub deepest_level: u32,
    pub entry_scan_calls: u32,
    pub entry_order: Vec<String>,
    /// The directory the expansion actually used. Always inside quarantine.
    pub workspace_root: PathBuf,
    pub workspace_removed: bool,
    pub elapsed_ms: u128,
    pub limits: ArchiveLimits,
}

impl ArchiveInspection {
    /// Every entry that was unpacked also earned its own receipt.
    pub fn every_entry_scanned(&self) -> bool {
        self.entries_seen == self.entries_scanned && self.entries_scanned == self.entry_scan_calls
    }
}

// ---------------------------------------------------------------------------
// The boundary
// ---------------------------------------------------------------------------

/// Expand and scan a quarantined download if - and only if - its *content*
/// says it is a container. Returns `Ok(None)` for an ordinary file, which the
/// whole-file 5166 verdict already covers.
pub fn inspect_if_archive(
    quarantine: &ProtectedDownloadQuarantine,
    held: &QuarantinedDownload,
    provider: &dyn AmsiProvider,
    limits: ArchiveLimits,
    now_unix: u64,
) -> Result<Option<ArchiveInspection>, ArchiveRefusal> {
    let format = sniff_archive_format(held.path())
        .map_err(|detail| ArchiveRefusal::Unreadable { detail })?;
    if !format.needs_inspection() {
        return Ok(None);
    }
    inspect_archive_in_quarantine(quarantine, held, provider, limits, now_unix).map(Some)
}

/// Expand a quarantined archive inside the quarantine and scan every entry.
pub fn inspect_archive_in_quarantine(
    quarantine: &ProtectedDownloadQuarantine,
    held: &QuarantinedDownload,
    provider: &dyn AmsiProvider,
    limits: ArchiveLimits,
    now_unix: u64,
) -> Result<ArchiveInspection, ArchiveRefusal> {
    let limits = limits.clamped();
    let quarantine_root = fs::canonicalize(quarantine.quarantine_root()).map_err(|error| {
        ArchiveRefusal::Unreadable {
            detail: format!("the OSL quarantine could not be checked: {error}"),
        }
    })?;
    let archive_path =
        fs::canonicalize(held.path()).map_err(|error| ArchiveRefusal::Unreadable {
            detail: format!("the quarantined archive could not be checked: {error}"),
        })?;
    // The archive itself has to be in quarantine: this boundary expands
    // decrypted plaintext that is already held, never a file somewhere else.
    if !archive_path.starts_with(&quarantine_root) {
        return Err(ArchiveRefusal::OutsideQuarantine {
            workspace: archive_path.display().to_string(),
            quarantine_root: quarantine_root.display().to_string(),
        });
    }

    // TASK5180-BOUND-QUARANTINE-ROOT
    let workspace_parent = quarantine.quarantine_root().to_path_buf();
    let workspace = workspace_parent.join(format!(
        "archive-expansion-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    create_private_workspace(&workspace).map_err(|detail| ArchiveRefusal::Unreadable { detail })?;
    // Checked after creation, from the real path on disk, so a workspace that
    // is not inside quarantine is refused before a single entry byte is
    // written into it.
    let canonical_workspace = match fs::canonicalize(&workspace) {
        Ok(path) => path,
        Err(error) => {
            let _ = fs::remove_dir_all(&workspace);
            return Err(ArchiveRefusal::Unreadable {
                detail: format!("the expansion workspace could not be checked: {error}"),
            });
        }
    };
    if !canonical_workspace.starts_with(&quarantine_root) {
        let _ = fs::remove_dir_all(&workspace);
        return Err(ArchiveRefusal::OutsideQuarantine {
            workspace: canonical_workspace.display().to_string(),
            quarantine_root: quarantine_root.display().to_string(),
        });
    }

    let started = Instant::now();
    let mut expansion = Expansion {
        quarantine,
        provider,
        limits,
        now_unix,
        started,
        deadline: started + limits.max_scan_time,
        workspace: canonical_workspace.clone(),
        entries_seen: 0,
        entries_scanned: 0,
        expanded_bytes: 0,
        scanned_bytes: 0,
        deepest_level: 0,
        entry_scan_calls: 0,
        entry_order: Vec::new(),
        format: String::new(),
    };

    let outcome = expansion.walk_container(&archive_path, "<archive>", 1);
    // The expansion is thrown away whatever happened. Nothing that came out of
    // a container is ever the thing that is released.
    let removed = fs::remove_dir_all(&canonical_workspace).is_ok() && !canonical_workspace.exists();
    let elapsed_ms = started.elapsed().as_millis();
    outcome?;
    Ok(ArchiveInspection {
        format: expansion.format.clone(),
        entries_seen: expansion.entries_seen,
        entries_scanned: expansion.entries_scanned,
        expanded_bytes: expansion.expanded_bytes,
        scanned_bytes: expansion.scanned_bytes,
        deepest_level: expansion.deepest_level,
        entry_scan_calls: expansion.entry_scan_calls,
        entry_order: expansion.entry_order.clone(),
        workspace_root: canonical_workspace,
        workspace_removed: removed,
        elapsed_ms,
        limits,
    })
}

struct Expansion<'a> {
    quarantine: &'a ProtectedDownloadQuarantine,
    provider: &'a dyn AmsiProvider,
    limits: ArchiveLimits,
    now_unix: u64,
    started: Instant,
    deadline: Instant,
    workspace: PathBuf,
    entries_seen: u32,
    entries_scanned: u32,
    expanded_bytes: u64,
    scanned_bytes: u64,
    deepest_level: u32,
    entry_scan_calls: u32,
    entry_order: Vec<String>,
    format: String,
}

impl Expansion<'_> {
    /// The one wall-clock guard. Called before every container, before every
    /// entry, on every copy chunk and after every entry receipt, so a single
    /// slow entry cannot stall the boundary between checks.
    fn check_deadline(&self) -> Result<(), ArchiveRefusal> {
        // TASK5180-BOUND-SCAN-TIME
        if Instant::now() >= self.deadline {
            return Err(ArchiveRefusal::ScanTime {
                limit_ms: self.limits.max_scan_time.as_millis(),
                elapsed_ms: self.started.elapsed().as_millis(),
            });
        }
        Ok(())
    }

    fn walk_container(
        &mut self,
        path: &Path,
        label: &str,
        level: u32,
    ) -> Result<(), ArchiveRefusal> {
        self.check_deadline()?;
        // TASK5180-BOUND-NESTING-DEPTH
        if level > self.limits.max_depth {
            return Err(ArchiveRefusal::NestingDepth {
                limit: self.limits.max_depth,
                level,
                container: label.to_owned(),
            });
        }
        if level > self.deepest_level {
            self.deepest_level = level;
        }
        let format = sniff_archive_format(path)
            .map_err(|detail| ArchiveRefusal::Unreadable { detail })?;
        if level == 1 {
            self.format = format.name();
        }
        match format {
            ArchiveFormat::Zip => self.walk_zip(path, level),
            ArchiveFormat::Tar => self.walk_tar(path, level),
            ArchiveFormat::Gzip => self.walk_gzip(path, label, level),
            ArchiveFormat::Unsupported(name) => Err(ArchiveRefusal::UnsupportedFormat {
                format: name.to_owned(),
            }),
            // Only containers reach this function.
            ArchiveFormat::NotAnArchive => Ok(()),
        }
    }

    fn walk_zip(&mut self, path: &Path, level: u32) -> Result<(), ArchiveRefusal> {
        let file = File::open(path).map_err(|error| ArchiveRefusal::Unreadable {
            detail: error.to_string(),
        })?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|error| ArchiveRefusal::Unreadable {
                detail: format!("zip: {error}"),
            })?;
        for index in 0..archive.len() {
            self.check_deadline()?;
            let mut entry = archive
                .by_index(index)
                .map_err(|error| ArchiveRefusal::Unreadable {
                    detail: format!("zip entry {index}: {error}"),
                })?;
            let name = entry.name().to_owned();
            // Every zip entry - file *and* directory - has its name validated
            // before anything else happens to it.
            let entry_path = self.entry_path(&name)?;
            let mode = entry.unix_mode();
            // TASK5180A-GUARD-SYMBOLIC-LINK
            if entry.is_symlink() {
                let target = zip_link_target(&mut entry);
                return Err(self.at_current_progress(symbolic_link_entry(&name, &target)));
            }
            let special = special_unix_file_kind(mode);
            // TASK5180A-GUARD-SPECIAL-FILE
            if special.is_some() {
                let kind_text = special.unwrap_or("special archive entry");
                return Err(self.at_current_progress(special_file_entry(&name, kind_text)));
            }
            if entry.is_dir() {
                continue;
            }
            self.count_entry(&name)?;
            self.copy_entry_bounded(&mut entry, &entry_path, &name)?;
            drop(entry);
            self.scan_and_recurse(&entry_path, &name, level)?;
        }
        Ok(())
    }

    fn walk_tar(&mut self, path: &Path, level: u32) -> Result<(), ArchiveRefusal> {
        let file = File::open(path).map_err(|error| ArchiveRefusal::Unreadable {
            detail: error.to_string(),
        })?;
        let mut archive = tar::Archive::new(file);
        let entries = archive
            .entries()
            .map_err(|error| ArchiveRefusal::Unreadable {
                detail: format!("tar: {error}"),
            })?;
        for entry in entries {
            self.check_deadline()?;
            let mut entry = entry.map_err(|error| ArchiveRefusal::Unreadable {
                detail: format!("tar entry: {error}"),
            })?;
            let name = entry
                .path()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            let kind = entry.header().entry_type();
            let target = entry
                .link_name()
                .ok()
                .flatten()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            // Every tar entry - file, directory, link and special file alike -
            // has its name validated before anything else happens to it.
            let entry_path = self.entry_path(&name)?;
            // TASK5180A-GUARD-SYMBOLIC-LINK
            if kind.is_symlink() {
                return Err(self.at_current_progress(symbolic_link_entry(&name, &target)));
            }
            // TASK5180A-GUARD-HARD-LINK
            if kind.is_hard_link() {
                return Err(self.at_current_progress(hard_link_entry(&name, &target)));
            }
            // TASK5180A-GUARD-SPECIAL-FILE
            if is_special_tar_entry(kind) {
                let kind_text = tar_special_kind_name(kind);
                return Err(self.at_current_progress(special_file_entry(&name, kind_text)));
            }
            if kind.is_dir() {
                continue;
            }
            self.count_entry(&name)?;
            self.copy_entry_bounded(&mut entry, &entry_path, &name)?;
            self.scan_and_recurse(&entry_path, &name, level)?;
        }
        Ok(())
    }

    fn walk_gzip(&mut self, path: &Path, label: &str, level: u32) -> Result<(), ArchiveRefusal> {
        let file = File::open(path).map_err(|error| ArchiveRefusal::Unreadable {
            detail: error.to_string(),
        })?;
        let stem = Path::new(label)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("member");
        let name = format!("{stem}.gunzip");
        let entry_path = self.entry_path(&name)?;
        self.count_entry(&name)?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        self.copy_entry_bounded(&mut decoder, &entry_path, &name)?;
        self.scan_and_recurse(&entry_path, &name, level)
    }

    /// Where an entry's bytes are written. The path is *built* from validated
    /// `Component::Normal` parts pushed onto the workspace, never taken from
    /// the archive, so a hostile name is a refusal and never a normalisation.
    ///
    /// Called before the entry is counted and before its first byte is
    /// written, at every nesting level, for files *and* directories.
    fn entry_path(&self, name: &str) -> Result<PathBuf, ArchiveRefusal> {
        match safe_relative_entry_path(name) {
            Ok(relative) => Ok(self.workspace.join(relative)),
            Err(refusal) => Err(self.at_current_progress(refusal)),
        }
    }

    /// Stamp a hostile-entry refusal with what the expansion had already done
    /// when the guard fired. The refused entry contributes nothing to either
    /// number: it is refused before `count_entry` and before
    /// `copy_entry_bounded`.
    fn at_current_progress(&self, refusal: ArchiveRefusal) -> ArchiveRefusal {
        match refusal {
            ArchiveRefusal::UnsafeEntry {
                kind,
                entry,
                target,
                detail,
                ..
            } => ArchiveRefusal::UnsafeEntry {
                kind,
                entry,
                target,
                detail,
                entries_unpacked: self.entries_seen,
                bytes_unpacked: self.expanded_bytes,
            },
            other => other,
        }
    }

    /// Counted *before* an entry's bytes are written, so the ceiling is a bound
    /// on what is unpacked, not a report on what already was.
    fn count_entry(&mut self, name: &str) -> Result<(), ArchiveRefusal> {
        // TASK5180-BOUND-ENTRY-COUNT
        if self.entries_seen >= self.limits.max_entries {
            return Err(ArchiveRefusal::EntryCount {
                limit: self.limits.max_entries,
                entries_seen: self.entries_seen,
                entry: name.to_owned(),
            });
        }
        self.entries_seen = self.entries_seen.saturating_add(1);
        self.entry_order.push(name.to_owned());
        Ok(())
    }

    /// Stream one entry into the workspace, checking the byte ceiling and the
    /// deadline before every chunk is written. A refusal here leaves nothing
    /// behind: the partial file is removed on the way out.
    fn copy_entry_bounded(
        &mut self,
        reader: &mut dyn Read,
        entry_path: &Path,
        name: &str,
    ) -> Result<(), ArchiveRefusal> {
        if let Some(parent) = entry_path.parent() {
            fs::create_dir_all(parent).map_err(|error| ArchiveRefusal::Unreadable {
                detail: format!("entry directory could not be created: {error}"),
            })?;
        }
        let mut output = create_private_file(entry_path).map_err(|detail| {
            ArchiveRefusal::Unreadable { detail }
        })?;
        let mut buffer = vec![0u8; COPY_CHUNK_BYTES];
        let result = (|| -> Result<(), ArchiveRefusal> {
            loop {
                self.check_deadline()?;
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| ArchiveRefusal::Unreadable {
                        detail: format!("entry '{name}' could not be read: {error}"),
                    })?;
                if read == 0 {
                    return Ok(());
                }
                let next = self.expanded_bytes.saturating_add(read as u64);
                // TASK5180-BOUND-EXPANDED-BYTES
                if next > self.limits.max_expanded_bytes {
                    return Err(ArchiveRefusal::ExpandedBytes {
                        limit: self.limits.max_expanded_bytes,
                        expanded_before_stop: self.expanded_bytes,
                        entry: name.to_owned(),
                    });
                }
                output
                    .write_all(&buffer[..read])
                    .map_err(|error| ArchiveRefusal::Unreadable {
                        detail: format!("entry '{name}' could not be written: {error}"),
                    })?;
                self.expanded_bytes = next;
            }
        })();
        if result.is_err() {
            drop(output);
            let _ = fs::remove_file(entry_path);
            return result;
        }
        output.sync_all().map_err(|error| ArchiveRefusal::Unreadable {
            detail: format!("entry '{name}' could not be synchronized: {error}"),
        })?;
        Ok(())
    }

    /// Give the unpacked entry its own local receipt under exactly the 5166
    /// rules, walk it if it is itself a container, then destroy it.
    fn scan_and_recurse(
        &mut self,
        entry_path: &Path,
        name: &str,
        level: u32,
    ) -> Result<(), ArchiveRefusal> {
        // TASK5180-BOUND-CLEAN-CONTROL
        self.scan_entry(entry_path, name)?;
        self.check_deadline()?;
        let format = sniff_archive_format(entry_path)
            .map_err(|detail| ArchiveRefusal::Unreadable { detail })?;
        if format.needs_inspection() {
            self.walk_container(entry_path, name, level + 1)?;
        }
        let _ = fs::remove_file(entry_path);
        Ok(())
    }

    fn scan_entry(&mut self, entry_path: &Path, name: &str) -> Result<(), ArchiveRefusal> {
        // `adopt` re-checks that the bytes are really inside the quarantine
        // root before anything is submitted, so an entry that somehow landed
        // elsewhere is never scanned and never counted.
        let held = self
            .quarantine
            .adopt(entry_path.to_path_buf())
            .map_err(|detail| ArchiveRefusal::OutsideQuarantine {
                workspace: format!("{} ({detail})", entry_path.display()),
                quarantine_root: self.quarantine.quarantine_root().display().to_string(),
            })?;
        self.entry_scan_calls = self.entry_scan_calls.saturating_add(1);
        let scan = self
            .quarantine
            .scan(&held, self.provider, self.now_unix)
            .map_err(|withheld| ArchiveRefusal::EntryNotClean {
                entry: name.to_owned(),
                detail: withheld.local_reason_text,
            })?;
        self.entries_scanned = self.entries_scanned.saturating_add(1);
        self.scanned_bytes = self.scanned_bytes.saturating_add(scan.content_len);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// TASK 5180a - hostile entries
// ---------------------------------------------------------------------------

/// Build a hostile-entry refusal. The two progress counters are filled in by
/// [`Expansion::at_current_progress`] at the call site that has them.
fn hostile_entry(
    kind: HostileEntryKind,
    entry: &str,
    target: &str,
    detail: &str,
) -> ArchiveRefusal {
    ArchiveRefusal::UnsafeEntry {
        kind,
        entry: entry.to_owned(),
        target: target.to_owned(),
        detail: detail.to_owned(),
        entries_unpacked: 0,
        bytes_unpacked: 0,
    }
}

fn absolute_path_entry(name: &str) -> ArchiveRefusal {
    let detail = "the entry is an absolute path and would be written outside the expansion \
                  workspace";
    hostile_entry(HostileEntryKind::AbsolutePath, name, "", detail)
}

fn parent_traversal_entry(name: &str) -> ArchiveRefusal {
    let detail = "the entry climbs out of the expansion workspace by parent traversal";
    hostile_entry(HostileEntryKind::ParentTraversal, name, "", detail)
}

fn symbolic_link_entry(name: &str, target: &str) -> ArchiveRefusal {
    let detail = "the entry is a symbolic link, which could redirect a later entry outside \
                  quarantine";
    hostile_entry(HostileEntryKind::SymbolicLink, name, target, detail)
}

fn hard_link_entry(name: &str, target: &str) -> ArchiveRefusal {
    let detail = "the entry is a hard link, which could alias a file outside quarantine";
    hostile_entry(HostileEntryKind::HardLink, name, target, detail)
}

fn special_file_entry(name: &str, kind_text: &str) -> ArchiveRefusal {
    let detail =
        format!("the entry is a {kind_text}, not a plain file, and is never materialised");
    hostile_entry(HostileEntryKind::SpecialFile, name, "", &detail)
}

fn malformed_entry(name: &str, detail: &str) -> ArchiveRefusal {
    hostile_entry(HostileEntryKind::MalformedName, name, "", detail)
}

/// Validate one archive entry name into a plain relative path, or refuse it.
///
/// Nothing here rewrites a hostile name into something safe: an absolute path
/// and a parent traversal are each a refusal, so the only names that survive
/// are the ones already made of ordinary components.
fn safe_relative_entry_path(name: &str) -> Result<PathBuf, ArchiveRefusal> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(malformed_entry(name, "the entry has no name"));
    }
    if trimmed.contains('\0') || trimmed.chars().any(char::is_control) {
        return Err(malformed_entry(
            name,
            "the entry name contains control characters",
        ));
    }
    // A Windows-style separator is a path separator on the machines this ships
    // to, so it is treated as one here whatever this build runs on.
    if trimmed.contains('\\') {
        return Err(malformed_entry(
            name,
            "the entry name contains a backslash path separator",
        ));
    }
    let mut built = PathBuf::new();
    let mut parts = 0usize;
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => {
                built.push(part);
                parts += 1;
            }
            Component::RootDir | Component::Prefix(_) => {
                // TASK5180A-GUARD-ABSOLUTE-PATH
                return Err(absolute_path_entry(name));
            }
            Component::ParentDir => {
                // TASK5180A-GUARD-PARENT-TRAVERSAL
                return Err(parent_traversal_entry(name));
            }
            Component::CurDir => {}
        }
    }
    if parts == 0 {
        return Err(malformed_entry(name, "the entry names no file"));
    }
    Ok(built)
}

/// The unix file-type bits, as they appear in a zip entry's external
/// attributes.
const UNIX_TYPE_MASK: u32 = 0o170_000;
const UNIX_TYPE_FIFO: u32 = 0o010_000;
const UNIX_TYPE_CHAR_DEVICE: u32 = 0o020_000;
const UNIX_TYPE_BLOCK_DEVICE: u32 = 0o060_000;
const UNIX_TYPE_SOCKET: u32 = 0o140_000;

/// Name the special-file kind a unix mode describes, if it describes one. A
/// mode with no file-type bits at all - which is what an ordinary zip entry
/// written on Windows carries - is not a special file.
fn special_unix_file_kind(mode: Option<u32>) -> Option<&'static str> {
    match mode? & UNIX_TYPE_MASK {
        UNIX_TYPE_FIFO => Some("FIFO"),
        UNIX_TYPE_CHAR_DEVICE => Some("character device"),
        UNIX_TYPE_BLOCK_DEVICE => Some("block device"),
        UNIX_TYPE_SOCKET => Some("socket"),
        _ => None,
    }
}

/// A tar entry that is neither a plain file, a plain directory, a symbolic
/// link nor a hard link. Links have their own guards, so this stays independent
/// of them - and anything unrecognised lands here, which is the fail-closed
/// side.
fn is_special_tar_entry(kind: tar::EntryType) -> bool {
    !(kind.is_file() || kind.is_dir() || kind.is_symlink() || kind.is_hard_link())
}

/// Owner-facing name for a tar entry type that is not a plain file.
fn tar_special_kind_name(kind: tar::EntryType) -> &'static str {
    match kind {
        tar::EntryType::Char => "character device",
        tar::EntryType::Block => "block device",
        tar::EntryType::Fifo => "FIFO",
        tar::EntryType::Continuous => "contiguous file",
        _ => "special archive entry",
    }
}

/// Read a zip symbolic link's target, which zip stores as the entry's content.
/// Bounded, held in memory, never written to disk.
fn zip_link_target(entry: &mut dyn Read) -> String {
    let mut target = Vec::new();
    let _ = entry.take(4096).read_to_end(&mut target);
    String::from_utf8_lossy(&target).into_owned()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_private_workspace(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("the expansion workspace could not be created: {error}"))?;
    harden_workspace(path)
}

#[cfg(unix)]
fn harden_workspace(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("the expansion workspace could not be locked down: {error}"))?;
    Ok(())
}

#[cfg(not(unix))]
fn harden_workspace(_path: &Path) -> Result<(), String> {
    // The workspace is created inside the quarantine root, which the 5166
    // boundary already locked to this account.
    Ok(())
}

#[cfg(unix)]
fn create_private_file(path: &Path) -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| format!("entry file could not be created: {error}"))
}

#[cfg(not(unix))]
fn create_private_file(path: &Path) -> Result<File, String> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("entry file could not be created: {error}"))
}
