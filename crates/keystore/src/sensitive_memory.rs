//! Best-effort OS page locking for short-lived plaintext buffers.
//!
//! This module is intentionally small: callers hand it plaintext buffers that
//! currently hold identity material and message contents, and the returned guard
//! keeps their backing pages locked until it drops. Failure to lock is a
//! refusal, not permission to continue with pageable plaintext.

use std::fmt;
use std::marker::PhantomData;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SensitivePlaintextKind {
    Identity,
    Message,
}

impl SensitivePlaintextKind {
    const fn buffer_kind(self) -> SensitiveBufferKind {
        match self {
            Self::Identity => SensitiveBufferKind::IdentityPlaintext,
            Self::Message => SensitiveBufferKind::MessagePlaintext,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensitiveBufferKind {
    IdentityPlaintext,
    MessagePlaintext,
}

#[derive(Debug, Eq, PartialEq)]
pub enum SensitiveMemoryError {
    EmptyBuffer(SensitiveBufferKind),
    InvalidPageSize,
    AddressOverflow,
    LockFailed {
        kind: SensitiveBufferKind,
        os_code: Option<i32>,
    },
}

impl fmt::Display for SensitiveMemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensitiveMemoryError::EmptyBuffer(kind) => {
                write!(f, "sensitive page lock refused empty {kind:?} buffer")
            }
            SensitiveMemoryError::InvalidPageSize => {
                f.write_str("sensitive page lock failed: invalid OS page size")
            }
            SensitiveMemoryError::AddressOverflow => {
                f.write_str("sensitive page lock failed: address range overflow")
            }
            SensitiveMemoryError::LockFailed { kind, os_code } => {
                write!(
                    f,
                    "sensitive page lock failed for {kind:?} buffer (os_code={os_code:?})"
                )
            }
        }
    }
}

impl std::error::Error for SensitiveMemoryError {}

pub struct LockedSensitivePlaintext<G = PlatformPlaintextPageLockGuard> {
    kind: SensitivePlaintextKind,
    bytes: Zeroizing<Vec<u8>>,
    _guard: G,
}

impl<G> LockedSensitivePlaintext<G> {
    pub fn kind(&self) -> SensitivePlaintextKind {
        self.kind
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.bytes.as_mut_slice()
    }
}

pub fn lock_sensitive_plaintext(
    kind: SensitivePlaintextKind,
    plaintext: Vec<u8>,
) -> Result<LockedSensitivePlaintext, SensitiveMemoryError> {
    lock_sensitive_plaintext_with(&PlatformSingleBufferPageLocker, kind, plaintext)
}

trait SensitivePageLocker {
    type Guard<'a>
    where
        Self: 'a;

    fn lock<'a>(
        &'a self,
        kind: SensitivePlaintextKind,
        bytes: &mut [u8],
    ) -> Result<Self::Guard<'a>, SensitiveMemoryError>;
}

fn lock_sensitive_plaintext_with<L>(
    locker: &L,
    kind: SensitivePlaintextKind,
    plaintext: Vec<u8>,
) -> Result<LockedSensitivePlaintext<L::Guard<'_>>, SensitiveMemoryError>
where
    L: SensitivePageLocker,
{
    let mut bytes = Zeroizing::new(plaintext);
    if bytes.is_empty() {
        return Err(SensitiveMemoryError::EmptyBuffer(kind.buffer_kind()));
    }
    let guard = match locker.lock(kind, bytes.as_mut_slice()) {
        Ok(guard) => guard,
        Err(error) => {
            bytes.zeroize();
            return Err(error);
        }
    };
    Ok(LockedSensitivePlaintext {
        kind,
        bytes,
        _guard: guard,
    })
}

struct PlatformSingleBufferPageLocker;

impl SensitivePageLocker for PlatformSingleBufferPageLocker {
    type Guard<'a> = PlatformPlaintextPageLockGuard;

    fn lock<'a>(
        &'a self,
        kind: SensitivePlaintextKind,
        bytes: &mut [u8],
    ) -> Result<Self::Guard<'a>, SensitiveMemoryError> {
        let span = PageSpan::covering(
            kind.buffer_kind(),
            bytes.as_ptr(),
            bytes.len(),
            platform_page_size(),
        )?;
        platform_lock(span.addr, span.len).map_err(|os_code| SensitiveMemoryError::LockFailed {
            kind: span.kind,
            os_code,
        })?;
        Ok(PlatformPlaintextPageLockGuard { span })
    }
}

pub struct PlatformPlaintextPageLockGuard {
    span: PageSpan,
}

impl Drop for PlatformPlaintextPageLockGuard {
    fn drop(&mut self) {
        platform_unlock(self.span.addr, self.span.len);
    }
}

/// Guard that keeps sensitive plaintext page ranges locked.
///
/// The guard borrows both input buffers for its lifetime, preventing callers
/// from resizing or moving them while the OS lock is active.
pub struct SensitivePageLock<'a> {
    inner: LockedSensitivePageRanges<'a, 'static, OsPageLocker>,
}

impl<'a> SensitivePageLock<'a> {
    pub fn locked_range_count(&self) -> usize {
        self.inner.locked_range_count()
    }
}

/// Lock the pages backing both identity and message plaintext buffers.
///
/// This is fail-closed: if either buffer cannot be locked, any earlier lock is
/// released and an error is returned.
pub fn lock_sensitive_pages<'a>(
    identity_plaintext: &'a mut [u8],
    message_plaintext: &'a mut [u8],
) -> Result<SensitivePageLock<'a>, SensitiveMemoryError> {
    lock_sensitive_pages_with_locker(identity_plaintext, message_plaintext, &OS_PAGE_LOCKER)
        .map(|inner| SensitivePageLock { inner })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PageSpan {
    addr: usize,
    len: usize,
    kind: SensitiveBufferKind,
}

impl PageSpan {
    fn covering(
        kind: SensitiveBufferKind,
        ptr: *const u8,
        len: usize,
        page_size: usize,
    ) -> Result<Self, SensitiveMemoryError> {
        if len == 0 {
            return Err(SensitiveMemoryError::EmptyBuffer(kind));
        }
        if page_size == 0 || !page_size.is_power_of_two() {
            return Err(SensitiveMemoryError::InvalidPageSize);
        }
        let start = ptr as usize;
        let end = start
            .checked_add(len)
            .ok_or(SensitiveMemoryError::AddressOverflow)?;
        let page_mask = page_size - 1;
        let aligned_start = start & !page_mask;
        let aligned_end = end
            .checked_add(page_mask)
            .ok_or(SensitiveMemoryError::AddressOverflow)?
            & !page_mask;
        let aligned_len = aligned_end
            .checked_sub(aligned_start)
            .ok_or(SensitiveMemoryError::AddressOverflow)?;
        Ok(Self {
            addr: aligned_start,
            len: aligned_len,
            kind,
        })
    }
}

trait PageLocker {
    fn page_size(&self) -> usize;
    fn lock(&self, addr: usize, len: usize) -> Result<(), Option<i32>>;
    fn unlock(&self, addr: usize, len: usize);
}

struct LockedSensitivePageRanges<'buf, 'locker, L: PageLocker + ?Sized> {
    locker: &'locker L,
    spans: Vec<PageSpan>,
    _borrowed_plaintext: PhantomData<&'buf mut [u8]>,
}

impl<'buf, 'locker, L: PageLocker + ?Sized> LockedSensitivePageRanges<'buf, 'locker, L> {
    fn locked_range_count(&self) -> usize {
        self.spans.len()
    }
}

impl<'buf, 'locker, L: PageLocker + ?Sized> Drop for LockedSensitivePageRanges<'buf, 'locker, L> {
    fn drop(&mut self) {
        for span in self.spans.iter().rev() {
            self.locker.unlock(span.addr, span.len);
        }
    }
}

fn lock_sensitive_pages_with_locker<'buf, 'locker, L: PageLocker + ?Sized>(
    identity_plaintext: &'buf mut [u8],
    message_plaintext: &'buf mut [u8],
    locker: &'locker L,
) -> Result<LockedSensitivePageRanges<'buf, 'locker, L>, SensitiveMemoryError> {
    let page_size = locker.page_size();
    let spans = [
        PageSpan::covering(
            SensitiveBufferKind::IdentityPlaintext,
            identity_plaintext.as_ptr(),
            identity_plaintext.len(),
            page_size,
        )?,
        PageSpan::covering(
            SensitiveBufferKind::MessagePlaintext,
            message_plaintext.as_ptr(),
            message_plaintext.len(),
            page_size,
        )?,
    ];

    let mut locked: Vec<PageSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Err(os_code) = locker.lock(span.addr, span.len) {
            for prior in locked.iter().rev() {
                locker.unlock(prior.addr, prior.len);
            }
            return Err(SensitiveMemoryError::LockFailed {
                kind: span.kind,
                os_code,
            });
        }
        locked.push(span);
    }

    Ok(LockedSensitivePageRanges {
        locker,
        spans: locked,
        _borrowed_plaintext: PhantomData,
    })
}

struct OsPageLocker;

static OS_PAGE_LOCKER: OsPageLocker = OsPageLocker;

impl PageLocker for OsPageLocker {
    fn page_size(&self) -> usize {
        platform_page_size()
    }

    fn lock(&self, addr: usize, len: usize) -> Result<(), Option<i32>> {
        platform_lock(addr, len)
    }

    fn unlock(&self, addr: usize, len: usize) {
        platform_unlock(addr, len)
    }
}

#[cfg(unix)]
fn platform_page_size() -> usize {
    unsafe {
        let size = sysconf(_SC_PAGESIZE);
        if size <= 0 {
            0
        } else {
            size as usize
        }
    }
}

#[cfg(windows)]
fn platform_page_size() -> usize {
    let mut info = SystemInfo::default();
    unsafe {
        GetSystemInfo(&mut info);
    }
    info.dw_page_size as usize
}

#[cfg(not(any(unix, windows)))]
fn platform_page_size() -> usize {
    0
}

#[cfg(unix)]
fn platform_lock(addr: usize, len: usize) -> Result<(), Option<i32>> {
    let rc = unsafe { mlock(addr as *const std::ffi::c_void, len) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().raw_os_error())
    }
}

#[cfg(windows)]
fn platform_lock(addr: usize, len: usize) -> Result<(), Option<i32>> {
    let ok = unsafe { VirtualLock(addr as *const std::ffi::c_void, len) };
    if ok != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().raw_os_error())
    }
}

#[cfg(not(any(unix, windows)))]
fn platform_lock(_addr: usize, _len: usize) -> Result<(), Option<i32>> {
    Err(None)
}

#[cfg(unix)]
fn platform_unlock(addr: usize, len: usize) {
    unsafe {
        let _ = munlock(addr as *const std::ffi::c_void, len);
    }
}

#[cfg(windows)]
fn platform_unlock(addr: usize, len: usize) {
    unsafe {
        let _ = VirtualUnlock(addr as *const std::ffi::c_void, len);
    }
}

#[cfg(not(any(unix, windows)))]
fn platform_unlock(_addr: usize, _len: usize) {}

#[cfg(all(
    unix,
    any(target_os = "linux", target_os = "android", target_os = "freebsd")
))]
const _SC_PAGESIZE: i32 = 30;

#[cfg(all(
    unix,
    any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    )
))]
const _SC_PAGESIZE: i32 = 29;

#[cfg(all(
    unix,
    not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos"
    ))
))]
const _SC_PAGESIZE: i32 = 30;

#[cfg(unix)]
unsafe extern "C" {
    fn sysconf(name: i32) -> isize;
    fn mlock(addr: *const std::ffi::c_void, len: usize) -> i32;
    fn munlock(addr: *const std::ffi::c_void, len: usize) -> i32;
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct SystemInfo {
    w_processor_architecture: u16,
    w_reserved: u16,
    dw_page_size: u32,
    lp_minimum_application_address: *mut std::ffi::c_void,
    lp_maximum_application_address: *mut std::ffi::c_void,
    dw_active_processor_mask: usize,
    dw_number_of_processors: u32,
    dw_processor_type: u32,
    dw_allocation_granularity: u32,
    w_processor_level: u16,
    w_processor_revision: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetSystemInfo(lpSystemInfo: *mut SystemInfo);
    fn VirtualLock(lpAddress: *const std::ffi::c_void, dwSize: usize) -> i32;
    fn VirtualUnlock(lpAddress: *const std::ffi::c_void, dwSize: usize) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Debug, PartialEq, Eq)]
    struct PlaintextLockCall {
        kind: SensitivePlaintextKind,
        len: usize,
        observed_plaintext: Vec<u8>,
    }

    #[derive(Default)]
    struct RecordingPlaintextLocker {
        locks: RefCell<Vec<PlaintextLockCall>>,
        unlocks: RefCell<Vec<SensitivePlaintextKind>>,
    }

    struct RecordingPlaintextGuard<'a> {
        locker: &'a RecordingPlaintextLocker,
        kind: SensitivePlaintextKind,
    }

    impl Drop for RecordingPlaintextGuard<'_> {
        fn drop(&mut self) {
            self.locker.unlocks.borrow_mut().push(self.kind);
        }
    }

    impl SensitivePageLocker for RecordingPlaintextLocker {
        type Guard<'a> = RecordingPlaintextGuard<'a>;

        fn lock<'a>(
            &'a self,
            kind: SensitivePlaintextKind,
            bytes: &mut [u8],
        ) -> Result<Self::Guard<'a>, SensitiveMemoryError> {
            self.locks.borrow_mut().push(PlaintextLockCall {
                kind,
                len: bytes.len(),
                observed_plaintext: bytes.to_vec(),
            });
            Ok(RecordingPlaintextGuard { locker: self, kind })
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Call {
        Lock(PageSpan),
        Unlock(PageSpan),
    }

    struct RecordingLocker {
        page_size: usize,
        fail_on_call: Option<usize>,
        calls: RefCell<Vec<Call>>,
        active: RefCell<Vec<PageSpan>>,
        lock_attempts: Cell<usize>,
    }

    impl RecordingLocker {
        fn new(page_size: usize) -> Self {
            Self {
                page_size,
                fail_on_call: None,
                calls: RefCell::new(Vec::new()),
                active: RefCell::new(Vec::new()),
                lock_attempts: Cell::new(0),
            }
        }

        fn failing_on(page_size: usize, fail_on_call: usize) -> Self {
            Self {
                page_size,
                fail_on_call: Some(fail_on_call),
                calls: RefCell::new(Vec::new()),
                active: RefCell::new(Vec::new()),
                lock_attempts: Cell::new(0),
            }
        }
    }

    impl PageLocker for RecordingLocker {
        fn page_size(&self) -> usize {
            self.page_size
        }

        fn lock(&self, addr: usize, len: usize) -> Result<(), Option<i32>> {
            let attempt = self.lock_attempts.get() + 1;
            self.lock_attempts.set(attempt);
            let kind = if attempt == 1 {
                SensitiveBufferKind::IdentityPlaintext
            } else {
                SensitiveBufferKind::MessagePlaintext
            };
            let span = PageSpan { addr, len, kind };
            self.calls.borrow_mut().push(Call::Lock(span));
            if self.fail_on_call == Some(attempt) {
                Err(Some(12))
            } else {
                self.active.borrow_mut().push(span);
                Ok(())
            }
        }

        fn unlock(&self, addr: usize, len: usize) {
            let span = self
                .active
                .borrow_mut()
                .pop()
                .expect("unlock must match a prior successful lock");
            assert_eq!(span.addr, addr);
            assert_eq!(span.len, len);
            self.calls.borrow_mut().push(Call::Unlock(span));
        }
    }

    #[test]
    fn lock_sensitive_plaintext_owns_zeroizing_identity_and_message_buffers() {
        let locker = RecordingPlaintextLocker::default();
        let identity = lock_sensitive_plaintext_with(
            &locker,
            SensitivePlaintextKind::Identity,
            b"identity plaintext a61".to_vec(),
        )
        .expect("identity plaintext pages lock");
        let message = lock_sensitive_plaintext_with(
            &locker,
            SensitivePlaintextKind::Message,
            b"message plaintext a61".to_vec(),
        )
        .expect("message plaintext pages lock");

        assert_eq!(identity.kind(), SensitivePlaintextKind::Identity);
        assert_eq!(message.kind(), SensitivePlaintextKind::Message);
        assert_eq!(identity.as_slice(), b"identity plaintext a61");
        assert_eq!(message.as_slice(), b"message plaintext a61");
        assert_eq!(
            *locker.locks.borrow(),
            vec![
                PlaintextLockCall {
                    kind: SensitivePlaintextKind::Identity,
                    len: "identity plaintext a61".len(),
                    observed_plaintext: b"identity plaintext a61".to_vec(),
                },
                PlaintextLockCall {
                    kind: SensitivePlaintextKind::Message,
                    len: "message plaintext a61".len(),
                    observed_plaintext: b"message plaintext a61".to_vec(),
                },
            ]
        );

        drop(message);
        drop(identity);
        assert_eq!(
            *locker.unlocks.borrow(),
            vec![
                SensitivePlaintextKind::Message,
                SensitivePlaintextKind::Identity,
            ]
        );
    }

    #[test]
    fn lock_sensitive_pages_prevents_identity_and_message_plaintext_swapping() {
        let locker = RecordingLocker::new(4096);
        let mut identity_plaintext = vec![0x11; 32];
        let mut message_plaintext = vec![0x22; 9000];

        let expected_identity = PageSpan::covering(
            SensitiveBufferKind::IdentityPlaintext,
            identity_plaintext.as_ptr(),
            identity_plaintext.len(),
            locker.page_size(),
        )
        .unwrap();
        let expected_message = PageSpan::covering(
            SensitiveBufferKind::MessagePlaintext,
            message_plaintext.as_ptr(),
            message_plaintext.len(),
            locker.page_size(),
        )
        .unwrap();

        {
            let guard = lock_sensitive_pages_with_locker(
                &mut identity_plaintext,
                &mut message_plaintext,
                &locker,
            )
            .expect("both plaintext buffers must be locked");

            assert_eq!(guard.locked_range_count(), 2);
            assert_eq!(
                locker.calls.borrow().clone(),
                vec![Call::Lock(expected_identity), Call::Lock(expected_message),],
                "identity and message plaintext must both be page-locked"
            );
        }

        assert_eq!(
            locker.calls.borrow().clone(),
            vec![
                Call::Lock(expected_identity),
                Call::Lock(expected_message),
                Call::Unlock(expected_message),
                Call::Unlock(expected_identity),
            ],
            "dropping the guard must release both page locks after use"
        );

        let failing = RecordingLocker::failing_on(4096, 2);
        let mut identity_plaintext = vec![0x33; 8];
        let mut message_plaintext = vec![0x44; 8];
        let err = match lock_sensitive_pages_with_locker(
            &mut identity_plaintext,
            &mut message_plaintext,
            &failing,
        ) {
            Ok(_) => panic!("message lock failure must refuse instead of continuing pageable"),
            Err(err) => err,
        };
        assert_eq!(
            err,
            SensitiveMemoryError::LockFailed {
                kind: SensitiveBufferKind::MessagePlaintext,
                os_code: Some(12),
            }
        );
        let calls = failing.calls.borrow();
        assert_eq!(calls.len(), 3);
        assert!(matches!(
            calls[0],
            Call::Lock(PageSpan {
                kind: SensitiveBufferKind::IdentityPlaintext,
                ..
            })
        ));
        assert!(matches!(
            calls[1],
            Call::Lock(PageSpan {
                kind: SensitiveBufferKind::MessagePlaintext,
                ..
            })
        ));
        assert!(matches!(
            calls[2],
            Call::Unlock(PageSpan {
                kind: SensitiveBufferKind::IdentityPlaintext,
                ..
            })
        ));
    }
}
