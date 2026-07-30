//! Page-locking helpers for plaintext that should not be swapped to disk.
//!
//! This is deliberately a small API: callers hand over a plaintext `Vec<u8>`,
//! and get back a zeroizing buffer whose backing pages have been locked by the
//! platform. If the lock fails, the plaintext is wiped before the error is
//! returned.

use std::fmt;
use std::ptr::NonNull;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum SensitivePlaintextKind {
    Identity,
    Message,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SensitiveMemoryError {
    EmptyBuffer,
    PlatformLockFailed,
}

impl fmt::Display for SensitiveMemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyBuffer => "sensitive plaintext buffer is empty",
            Self::PlatformLockFailed => "sensitive plaintext pages could not be locked",
        })
    }
}

impl std::error::Error for SensitiveMemoryError {}

pub struct LockedSensitivePages<G = PlatformPageLockGuard> {
    kind: SensitivePlaintextKind,
    bytes: Zeroizing<Vec<u8>>,
    _guard: G,
}

impl<G> LockedSensitivePages<G> {
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

pub fn lock_sensitive_pages(
    kind: SensitivePlaintextKind,
    plaintext: Vec<u8>,
) -> Result<LockedSensitivePages, SensitiveMemoryError> {
    lock_sensitive_pages_with(&PlatformPageLocker, kind, plaintext)
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

fn lock_sensitive_pages_with<L>(
    locker: &L,
    kind: SensitivePlaintextKind,
    plaintext: Vec<u8>,
) -> Result<LockedSensitivePages<L::Guard<'_>>, SensitiveMemoryError>
where
    L: SensitivePageLocker,
{
    let mut bytes = Zeroizing::new(plaintext);
    if bytes.is_empty() {
        return Err(SensitiveMemoryError::EmptyBuffer);
    }
    let guard = match locker.lock(kind, bytes.as_mut_slice()) {
        Ok(guard) => guard,
        Err(error) => {
            bytes.zeroize();
            return Err(error);
        }
    };
    Ok(LockedSensitivePages {
        kind,
        bytes,
        _guard: guard,
    })
}

struct PlatformPageLocker;

impl SensitivePageLocker for PlatformPageLocker {
    type Guard<'a> = PlatformPageLockGuard;

    fn lock<'a>(
        &'a self,
        _kind: SensitivePlaintextKind,
        bytes: &mut [u8],
    ) -> Result<Self::Guard<'a>, SensitiveMemoryError> {
        let ptr = NonNull::new(bytes.as_mut_ptr()).ok_or(SensitiveMemoryError::EmptyBuffer)?;
        platform_lock(ptr, bytes.len())?;
        Ok(PlatformPageLockGuard {
            ptr,
            len: bytes.len(),
        })
    }
}

pub struct PlatformPageLockGuard {
    ptr: NonNull<u8>,
    len: usize,
}

impl Drop for PlatformPageLockGuard {
    fn drop(&mut self) {
        platform_unlock(self.ptr, self.len);
    }
}

#[cfg(unix)]
fn platform_lock(ptr: NonNull<u8>, len: usize) -> Result<(), SensitiveMemoryError> {
    unsafe extern "C" {
        fn mlock(addr: *const std::ffi::c_void, len: usize) -> i32;
    }
    let rc = unsafe { mlock(ptr.as_ptr().cast(), len) };
    if rc == 0 {
        Ok(())
    } else {
        Err(SensitiveMemoryError::PlatformLockFailed)
    }
}

#[cfg(unix)]
fn platform_unlock(ptr: NonNull<u8>, len: usize) {
    unsafe extern "C" {
        fn munlock(addr: *const std::ffi::c_void, len: usize) -> i32;
    }
    let _ = unsafe { munlock(ptr.as_ptr().cast(), len) };
}

#[cfg(windows)]
fn platform_lock(ptr: NonNull<u8>, len: usize) -> Result<(), SensitiveMemoryError> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualLock(lpaddress: *mut std::ffi::c_void, dwsize: usize) -> i32;
    }
    let rc = unsafe { VirtualLock(ptr.as_ptr().cast(), len) };
    if rc != 0 {
        Ok(())
    } else {
        Err(SensitiveMemoryError::PlatformLockFailed)
    }
}

#[cfg(windows)]
fn platform_unlock(ptr: NonNull<u8>, len: usize) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn VirtualUnlock(lpaddress: *mut std::ffi::c_void, dwsize: usize) -> i32;
    }
    let _ = unsafe { VirtualUnlock(ptr.as_ptr().cast(), len) };
}

#[cfg(not(any(unix, windows)))]
fn platform_lock(_ptr: NonNull<u8>, _len: usize) -> Result<(), SensitiveMemoryError> {
    Err(SensitiveMemoryError::PlatformLockFailed)
}

#[cfg(not(any(unix, windows)))]
fn platform_unlock(_ptr: NonNull<u8>, _len: usize) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Debug, PartialEq, Eq)]
    struct LockCall {
        kind: SensitivePlaintextKind,
        len: usize,
        observed_plaintext: Vec<u8>,
    }

    #[derive(Default)]
    struct RecordingLocker {
        locks: RefCell<Vec<LockCall>>,
        unlocks: RefCell<Vec<SensitivePlaintextKind>>,
    }

    struct RecordingGuard<'a> {
        locker: &'a RecordingLocker,
        kind: SensitivePlaintextKind,
    }

    impl Drop for RecordingGuard<'_> {
        fn drop(&mut self) {
            self.locker.unlocks.borrow_mut().push(self.kind);
        }
    }

    impl SensitivePageLocker for RecordingLocker {
        type Guard<'a> = RecordingGuard<'a>;

        fn lock<'a>(
            &'a self,
            kind: SensitivePlaintextKind,
            bytes: &mut [u8],
        ) -> Result<Self::Guard<'a>, SensitiveMemoryError> {
            self.locks.borrow_mut().push(LockCall {
                kind,
                len: bytes.len(),
                observed_plaintext: bytes.to_vec(),
            });
            Ok(RecordingGuard { locker: self, kind })
        }
    }

    #[test]
    fn lock_sensitive_pages_prevents_identity_and_message_plaintext_swapping() {
        let locker = RecordingLocker::default();
        let identity = lock_sensitive_pages_with(
            &locker,
            SensitivePlaintextKind::Identity,
            b"identity plaintext a61".to_vec(),
        )
        .expect("identity plaintext pages lock");
        let message = lock_sensitive_pages_with(
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
                LockCall {
                    kind: SensitivePlaintextKind::Identity,
                    len: "identity plaintext a61".len(),
                    observed_plaintext: b"identity plaintext a61".to_vec(),
                },
                LockCall {
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
}
