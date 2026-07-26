//! Windows executable identity boundary.
//!
//! Callers provide only a reviewed publisher enum. On Windows, the file is
//! opened without write/delete sharing, its Authenticode chain is validated by
//! Windows, and the leaf certificate's organization is matched exactly. The
//! returned object keeps the verified file handle open so a caller can retain
//! it until after process creation.

use std::path::Path;
use std::time::Duration;

#[cfg(any(target_os = "windows", test))]
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::fs::File;
#[cfg(any(target_os = "windows", test))]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::{Mutex, OnceLock};
#[cfg(any(target_os = "windows", test))]
use std::time::Instant;

const POSITIVE_CACHE_TTL: Duration = Duration::from_secs(30);
const NEGATIVE_CACHE_TTL: Duration = Duration::from_millis(250);
const MAX_CACHE_ENTRIES: usize = 32;
#[cfg(target_os = "windows")]
const MAX_ORGANIZATION_UTF16_UNITS: u32 = 256;

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) enum ExecutablePublisher {
    Discord,
    Chrome,
    Edge,
    Firefox,
    Brave,
    Opera,
    DuckDuckGo,
    Microsoft,
    Telegram,
    Signal,
}

impl ExecutablePublisher {
    fn allowed_organizations(self) -> &'static [&'static str] {
        match self {
            Self::Discord => &["Discord Inc."],
            Self::Chrome => &["Google LLC"],
            Self::Edge => &["Microsoft Corporation"],
            Self::Firefox => &["Mozilla Corporation"],
            Self::Brave => &["Brave Software, Inc."],
            Self::Opera => &["Opera Norway AS"],
            Self::DuckDuckGo => &["Duck Duck Go, Inc."],
            Self::Microsoft => &["Microsoft Corporation"],
            Self::Telegram => &["Telegram FZ-LLC"],
            Self::Signal => &["Signal Messenger, LLC"],
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ExecutableTrustError {
    #[cfg(not(target_os = "windows"))]
    UnsupportedPlatform,
    #[cfg(target_os = "windows")]
    OpenFailed,
    #[cfg(target_os = "windows")]
    NotRegularFile,
    #[cfg(target_os = "windows")]
    FileIdentityUnavailable,
    #[cfg(target_os = "windows")]
    SignatureOrPublisherRejected,
    #[cfg(target_os = "windows")]
    ChangedDuringVerification,
}

#[derive(Debug)]
pub(crate) struct TrustedExecutable {
    #[cfg(target_os = "windows")]
    path: PathBuf,
    #[cfg(target_os = "windows")]
    _locked_file: File,
}

impl TrustedExecutable {
    /// The reviewed path to pass directly to process creation while this
    /// object—and therefore its restrictive file handle—remains alive.
    #[cfg(target_os = "windows")]
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn verify_executable(
    path: &Path,
    publisher: ExecutablePublisher,
) -> Result<TrustedExecutable, ExecutableTrustError> {
    verify_executable_platform(path, publisher)
}

fn organization_allowed(publisher: ExecutablePublisher, organization: &str) -> bool {
    publisher
        .allowed_organizations()
        .iter()
        .any(|expected| organization.eq_ignore_ascii_case(expected))
}

#[cfg(target_os = "windows")]
fn verify_executable_platform(
    path: &Path,
    publisher: ExecutablePublisher,
) -> Result<TrustedExecutable, ExecutableTrustError> {
    use std::os::windows::fs::OpenOptionsExt;

    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let canonical_path = path
        .canonicalize()
        .map_err(|_| ExecutableTrustError::OpenFailed)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&canonical_path)
        .map_err(|_| ExecutableTrustError::OpenFailed)?;
    let before = file
        .metadata()
        .map_err(|_| ExecutableTrustError::FileIdentityUnavailable)?;
    if !before.is_file() {
        return Err(ExecutableTrustError::NotRegularFile);
    }
    let before_identity = FileIdentity::from_file(&file)?;
    let key = TrustCacheKey {
        canonical_path: canonical_path.clone(),
        identity: before_identity,
        publisher,
    };
    let now = Instant::now();
    let cached = cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .lookup(&key, now);
    let trusted = cached.unwrap_or_else(|| {
        let trusted = authenticode_matches(&file, &canonical_path, publisher);
        cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.clone(), trusted, Instant::now());
        trusted
    });
    if !trusted {
        return Err(ExecutableTrustError::SignatureOrPublisherRejected);
    }

    let after_identity = FileIdentity::from_file(&file)
        .map_err(|_| ExecutableTrustError::ChangedDuringVerification)?;
    if before_identity != after_identity {
        return Err(ExecutableTrustError::ChangedDuringVerification);
    }
    Ok(TrustedExecutable {
        path: canonical_path,
        _locked_file: file,
    })
}

#[cfg(not(target_os = "windows"))]
fn verify_executable_platform(
    _path: &Path,
    _publisher: ExecutablePublisher,
) -> Result<TrustedExecutable, ExecutableTrustError> {
    Err(ExecutableTrustError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn authenticode_matches(file: &File, path: &Path, publisher: ExecutablePublisher) -> bool {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Security::Cryptography::{
        szOID_ORGANIZATION_NAME, CertGetNameStringW, CERT_NAME_ATTR_TYPE,
    };
    use windows_sys::Win32::Security::WinTrust::{
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
        WTHelperProvDataFromStateData, WinVerifyTrustEx, WINTRUST_ACTION_GENERIC_VERIFY_V2,
        WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
    };

    let wide_path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut file_info: WINTRUST_FILE_INFO = unsafe { std::mem::zeroed() };
    file_info.cbStruct = std::mem::size_of::<WINTRUST_FILE_INFO>() as u32;
    file_info.pcwszFilePath = wide_path.as_ptr();
    file_info.hFile = file.as_raw_handle().cast();

    let mut trust_data: WINTRUST_DATA = unsafe { std::mem::zeroed() };
    trust_data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    trust_data.dwUIChoice = WTD_UI_NONE;
    trust_data.fdwRevocationChecks = WTD_REVOKE_NONE;
    trust_data.dwUnionChoice = WTD_CHOICE_FILE;
    trust_data.Anonymous = WINTRUST_DATA_0 {
        pFile: &mut file_info,
    };
    trust_data.dwStateAction = WTD_STATEACTION_VERIFY;
    trust_data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
    trust_data.dwUIContext = WTD_UICONTEXT_EXECUTE;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    let status = unsafe { WinVerifyTrustEx(std::ptr::null_mut(), &mut action, &mut trust_data) };
    let organization = if status == 0 && !trust_data.hWVTStateData.is_null() {
        unsafe {
            let provider = WTHelperProvDataFromStateData(trust_data.hWVTStateData);
            let signer = (!provider.is_null())
                .then(|| WTHelperGetProvSignerFromChain(provider, 0, 0, 0))
                .filter(|signer| !signer.is_null());
            let certificate = signer
                .map(|signer| WTHelperGetProvCertFromChain(signer, 0))
                .filter(|certificate| !certificate.is_null());
            certificate.and_then(|certificate| {
                let context = (*certificate).pCert;
                if context.is_null() {
                    return None;
                }
                let required = CertGetNameStringW(
                    context,
                    CERT_NAME_ATTR_TYPE,
                    0,
                    szOID_ORGANIZATION_NAME.cast::<c_void>(),
                    std::ptr::null_mut(),
                    0,
                );
                if !(2..=MAX_ORGANIZATION_UTF16_UNITS).contains(&required) {
                    return None;
                }
                let mut value = vec![0u16; required as usize];
                let written = CertGetNameStringW(
                    context,
                    CERT_NAME_ATTR_TYPE,
                    0,
                    szOID_ORGANIZATION_NAME.cast::<c_void>(),
                    value.as_mut_ptr(),
                    required,
                );
                if written < 2 || written > required {
                    return None;
                }
                value.truncate(written as usize - 1);
                (!value.contains(&0))
                    .then(|| String::from_utf16(&value).ok())
                    .flatten()
            })
        }
    } else {
        None
    };

    // WinTrust owns all provider/certificate state above. Close it for every
    // result, including signature failure and publisher mismatch.
    if !trust_data.hWVTStateData.is_null() {
        trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
        unsafe {
            let _ = WinVerifyTrustEx(std::ptr::null_mut(), &mut action, &mut trust_data);
        }
    }
    organization
        .as_deref()
        .is_some_and(|organization| organization_allowed(publisher, organization))
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
struct FileIdentity {
    volume_serial: u32,
    file_index: u64,
    file_size: u64,
    last_write_time: u64,
}

#[cfg(target_os = "windows")]
impl FileIdentity {
    fn from_file(file: &std::fs::File) -> Result<Self, ExecutableTrustError> {
        use std::os::windows::io::AsRawHandle;

        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };

        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) } == 0
        {
            return Err(ExecutableTrustError::FileIdentityUnavailable);
        }
        Ok(Self {
            volume_serial: information.dwVolumeSerialNumber,
            file_index: u64::from(information.nFileIndexHigh) << 32
                | u64::from(information.nFileIndexLow),
            file_size: u64::from(information.nFileSizeHigh) << 32
                | u64::from(information.nFileSizeLow),
            last_write_time: u64::from(information.ftLastWriteTime.dwHighDateTime) << 32
                | u64::from(information.ftLastWriteTime.dwLowDateTime),
        })
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct TrustCacheKey {
    canonical_path: PathBuf,
    identity: FileIdentity,
    publisher: ExecutablePublisher,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Clone, Copy)]
struct TrustCacheEntry {
    trusted: bool,
    checked_at: Instant,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Default)]
struct TrustCache {
    entries: HashMap<TrustCacheKey, TrustCacheEntry>,
}

#[cfg(any(target_os = "windows", test))]
impl TrustCache {
    fn lookup(&mut self, key: &TrustCacheKey, now: Instant) -> Option<bool> {
        let entry = self.entries.get(key).copied()?;
        let ttl = if entry.trusted {
            POSITIVE_CACHE_TTL
        } else {
            NEGATIVE_CACHE_TTL
        };
        if now.saturating_duration_since(entry.checked_at) >= ttl {
            self.entries.remove(key);
            return None;
        }
        Some(entry.trusted)
    }

    fn insert(&mut self, key: TrustCacheKey, trusted: bool, now: Instant) {
        self.entries.retain(|_, entry| {
            let ttl = if entry.trusted {
                POSITIVE_CACHE_TTL
            } else {
                NEGATIVE_CACHE_TTL
            };
            now.saturating_duration_since(entry.checked_at) < ttl
        });
        if self.entries.len() >= MAX_CACHE_ENTRIES && !self.entries.contains_key(&key) {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.checked_at)
                .map(|(key, _)| key.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.entries.insert(
            key,
            TrustCacheEntry {
                trusted,
                checked_at: now,
            },
        );
    }
}

#[cfg(target_os = "windows")]
fn cache() -> &'static Mutex<TrustCache> {
    static CACHE: OnceLock<Mutex<TrustCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(TrustCache::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(index: u64, publisher: ExecutablePublisher) -> TrustCacheKey {
        TrustCacheKey {
            canonical_path: PathBuf::from(format!("C:/reviewed/app-{index}.exe")),
            identity: FileIdentity {
                volume_serial: 7,
                file_index: index,
                file_size: 100 + index,
                last_write_time: 1_000 + index,
            },
            publisher,
        }
    }

    #[test]
    fn publisher_matching_is_exact_except_for_ascii_case() {
        assert!(organization_allowed(
            ExecutablePublisher::Discord,
            "Discord Inc."
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Chrome,
            "Google LLC"
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Edge,
            "Microsoft Corporation"
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Firefox,
            "Mozilla Corporation"
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Brave,
            "Brave Software, Inc."
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Opera,
            "Opera Norway AS"
        ));
        assert!(organization_allowed(
            ExecutablePublisher::DuckDuckGo,
            "Duck Duck Go, Inc."
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Telegram,
            "telegram fz-llc"
        ));
        assert!(organization_allowed(
            ExecutablePublisher::Signal,
            "Signal Messenger, LLC"
        ));
        for rejected in [
            "Mozilla Corporation ",
            "Fake Mozilla Corporation",
            "Discord, Inc.",
            "Google LLC ",
            "Microsoft Corporation Ltd.",
            "Brave Software Inc.",
            "Telegram Messenger LLP",
            "Signal Messenger LLC",
            "",
        ] {
            assert!(!organization_allowed(
                ExecutablePublisher::Discord,
                rejected
            ));
            assert!(!organization_allowed(ExecutablePublisher::Chrome, rejected));
            assert!(!organization_allowed(ExecutablePublisher::Edge, rejected));
            assert!(!organization_allowed(
                ExecutablePublisher::Firefox,
                rejected
            ));
            assert!(!organization_allowed(ExecutablePublisher::Brave, rejected));
            assert!(!organization_allowed(ExecutablePublisher::Opera, rejected));
            assert!(!organization_allowed(
                ExecutablePublisher::DuckDuckGo,
                rejected
            ));
            assert!(!organization_allowed(
                ExecutablePublisher::Telegram,
                rejected
            ));
            assert!(!organization_allowed(ExecutablePublisher::Signal, rejected));
        }
    }

    #[test]
    fn cache_keys_include_file_identity_and_expected_publisher() {
        let now = Instant::now();
        let mut cache = TrustCache::default();
        let firefox = key(1, ExecutablePublisher::Firefox);
        cache.insert(firefox.clone(), true, now);
        assert_eq!(cache.lookup(&firefox, now), Some(true));
        assert_eq!(
            cache.lookup(&key(2, ExecutablePublisher::Firefox), now),
            None
        );
        assert_eq!(
            cache.lookup(&key(1, ExecutablePublisher::Signal), now),
            None
        );
    }

    #[test]
    fn positive_and_negative_cache_entries_use_bounded_ttls() {
        let now = Instant::now();
        let mut cache = TrustCache::default();
        let positive = key(1, ExecutablePublisher::Firefox);
        let negative = key(2, ExecutablePublisher::Firefox);
        cache.insert(positive.clone(), true, now);
        cache.insert(negative.clone(), false, now);
        assert_eq!(cache.lookup(&negative, now + NEGATIVE_CACHE_TTL), None);
        assert_eq!(
            cache.lookup(&positive, now + NEGATIVE_CACHE_TTL),
            Some(true)
        );
        assert_eq!(cache.lookup(&positive, now + POSITIVE_CACHE_TTL), None);
    }

    #[test]
    fn cache_never_exceeds_its_fixed_entry_bound() {
        let now = Instant::now();
        let mut cache = TrustCache::default();
        for index in 0..(MAX_CACHE_ENTRIES as u64 + 8) {
            cache.insert(
                key(index, ExecutablePublisher::Firefox),
                true,
                now + Duration::from_millis(index),
            );
        }
        assert_eq!(cache.entries.len(), MAX_CACHE_ENTRIES);
        assert_eq!(
            cache.lookup(&key(0, ExecutablePublisher::Firefox), now),
            None
        );
        assert_eq!(
            cache.lookup(
                &key(MAX_CACHE_ENTRIES as u64 + 7, ExecutablePublisher::Firefox),
                now + Duration::from_millis(MAX_CACHE_ENTRIES as u64 + 7)
            ),
            Some(true)
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn executable_verification_fails_closed_off_windows() {
        assert_eq!(
            verify_executable(
                Path::new("/tmp/not-a-windows-executable"),
                ExecutablePublisher::Firefox
            )
            .unwrap_err(),
            ExecutableTrustError::UnsupportedPlatform
        );
    }
}
