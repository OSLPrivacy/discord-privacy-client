//! The Windows half: watch the capture paths the OS actually reports.
//!
//! Two signals, both documented user-mode APIs, both running only while a
//! view-once viewer is on screen:
//!
//! * `AddClipboardFormatListener` → `WM_CLIPBOARDUPDATE`. PrintScreen,
//!   Alt+PrintScreen, the Windows snip and Game Bar all deliver their result
//!   by putting a `CF_DIB` on the clipboard. This is the signal that carries
//!   evidence, because the bitmap can be measured against the live screen.
//! * `WH_KEYBOARD_LL` → `VK_SNAPSHOT`. Tells us the key was pressed, which
//!   lets a clipboard bitmap be attributed to PrintScreen rather than to a
//!   snip. It is corroboration only: a keystroke on its own proves nothing
//!   about whether a capture occurred, so it is never sufficient on its own.
//!
//! A capture that reaches neither of these — a recorder BitBlt-ing the desktop
//! into its own process, a camera, a capture card — produces no observation
//! here and no notification anywhere. That is a property of Windows, not a gap
//! in this file, and the shipped copy in `disclosure` says so.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_BITFIELDS, DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RemoveClipboardFormatListener,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_SNAPSHOT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, PostMessageW, RegisterClassW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, HWND_MESSAGE, KBDLLHOOKSTRUCT, MSG, SM_CXVIRTUALSCREEN,
    SM_CYVIRTUALSCREEN, WH_KEYBOARD_LL, WM_CLIPBOARDUPDATE, WM_CLOSE, WM_DESTROY, WM_KEYDOWN,
    WM_SYSKEYDOWN, WNDCLASSW,
};

use crate::event::{CaptureEvidence, SupportedCapturePath};

/// `CF_DIB`. Not re-exported by `windows-sys` 0.59 under any of the feature
/// sets this crate enables, and it is a stable Win32 constant.
const CF_DIB: u32 = 8;

/// A clipboard bitmap arriving within this long after a PrintScreen keypress
/// is attributed to PrintScreen; otherwise it is attributed to the snip. Long
/// enough for a 5760x1200 desktop to be encoded, short enough not to swallow
/// an unrelated later copy.
const PRINT_SCREEN_CORRELATION_MS: u64 = 3_000;

/// Roughly how many pixels are sampled when measuring a captured bitmap. The
/// measurement has to be cheap enough to run inside a window procedure.
const SAMPLE_TARGET: usize = 20_000;

/// A supported capture path fired, with the measurements taken at the time.
#[derive(Clone, Debug)]
pub struct ObservedCapture {
    pub path: SupportedCapturePath,
    pub evidence: CaptureEvidence,
    pub observed_at_ms: u64,
}

struct Shared {
    observations: Mutex<Vec<ObservedCapture>>,
    /// Clipboard updates seen that were *not* a capture of this screen — an
    /// ordinary text copy, a bitmap of the wrong size. Counted so the check
    /// can show the watcher is not simply reporting every clipboard event.
    ignored_clipboard_updates: AtomicU32,
    last_print_screen_ms: AtomicU32,
    baseline_sequence: AtomicU32,
    print_screen_presses: AtomicU32,
}

static SHARED: OnceLock<Mutex<Option<Arc<Shared>>>> = OnceLock::new();

fn shared_slot() -> &'static Mutex<Option<Arc<Shared>>> {
    SHARED.get_or_init(|| Mutex::new(None))
}

fn current_shared() -> Option<Arc<Shared>> {
    shared_slot().lock().ok().and_then(|slot| slot.clone())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Milliseconds since an arbitrary fixed point, truncated to 32 bits. Only
/// ever used for a sub-second correlation window, so wrap-around every 49 days
/// costs at most one misattributed path label and never a false observation.
fn coarse_ms() -> u32 {
    now_ms() as u32
}

fn virtual_screen() -> (i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

fn stride(width: i32, bit_count: u16) -> usize {
    (((width as usize) * (bit_count as usize) + 31) / 32) * 4
}

/// Byte offset of the pixel at (`x`, `y_from_top`) in a DIB whose `height`
/// carries the usual sign convention: positive is bottom-up.
fn dib_offset(x: usize, y_from_top: usize, width: i32, height: i32, bit_count: u16) -> usize {
    let row = if height >= 0 {
        (height as usize)
            .saturating_sub(1)
            .saturating_sub(y_from_top)
    } else {
        y_from_top
    };
    row * stride(width, bit_count) + x * (bit_count as usize / 8)
}

/// Read the desktop directly, right now, as the thing a claimed capture is
/// compared against. Returns 32-bit bottom-up BGRA.
fn live_screen_readback(width: i32, height: i32) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    if screen.is_null() {
        return None;
    }
    let memory = unsafe { CreateCompatibleDC(screen) };
    let bitmap = if memory.is_null() {
        std::ptr::null_mut()
    } else {
        unsafe { CreateCompatibleBitmap(screen, width, height) }
    };
    let mut pixels = vec![0u8; stride(width, 32) * height as usize];
    let mut ok = false;
    if !memory.is_null() && !bitmap.is_null() {
        let previous = unsafe { SelectObject(memory, bitmap) };
        let copied = unsafe { BitBlt(memory, 0, 0, width, height, screen, 0, 0, SRCCOPY) } != 0;
        if copied {
            let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let rows = unsafe {
                GetDIBits(
                    memory,
                    bitmap,
                    0,
                    height as u32,
                    pixels.as_mut_ptr().cast(),
                    &mut info,
                    DIB_RGB_COLORS,
                )
            };
            ok = rows == height;
        }
        unsafe { SelectObject(memory, previous) };
    }
    unsafe {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !memory.is_null() {
            DeleteDC(memory);
        }
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    ok.then_some(pixels)
}

/// Measure a clipboard bitmap against the desktop it claims to be a picture
/// of. Returns distinct sampled colours and parts-per-million agreement.
fn measure(
    dib: &[u8],
    pixel_offset: usize,
    width: i32,
    height: i32,
    bit_count: u16,
    live: Option<&[u8]>,
) -> (usize, u32) {
    use std::collections::HashSet;

    if width <= 0 || height == 0 || bit_count != 32 {
        return (0, 0);
    }
    let rows = height.unsigned_abs() as usize;
    let columns = width as usize;
    let total = rows.saturating_mul(columns);
    if total == 0 {
        return (0, 0);
    }
    let step = (total / SAMPLE_TARGET).max(1);
    let mut colors: HashSet<[u8; 3]> = HashSet::new();
    let mut sampled = 0usize;
    let mut agreed = 0usize;
    let mut index = 0usize;
    while index < total {
        let y = index / columns;
        let x = index % columns;
        let offset = pixel_offset + dib_offset(x, y, width, height, bit_count);
        if offset + 3 > dib.len() {
            break;
        }
        let pixel = [dib[offset], dib[offset + 1], dib[offset + 2]];
        colors.insert(pixel);
        sampled += 1;
        if let Some(live) = live {
            // The readback is always bottom-up 32-bit, whatever the clipboard
            // bitmap's own orientation is.
            let live_offset = dib_offset(x, y, width, height.abs(), 32);
            if live_offset + 3 <= live.len() {
                let diff = u16::from(pixel[0].abs_diff(live[live_offset]))
                    + u16::from(pixel[1].abs_diff(live[live_offset + 1]))
                    + u16::from(pixel[2].abs_diff(live[live_offset + 2]));
                if diff <= 8 {
                    agreed += 1;
                }
            }
        }
        index += step;
    }
    if sampled == 0 {
        return (0, 0);
    }
    let ppm = ((agreed as u64 * 1_000_000) / sampled as u64) as u32;
    (colors.len(), ppm)
}

/// Pixel data offset inside a packed DIB: the header, any bit-field masks, and
/// any palette.
fn packed_dib_pixel_offset(header: &BITMAPINFOHEADER) -> usize {
    let masks = if header.biCompression == BI_BITFIELDS {
        12
    } else {
        0
    };
    let palette = if header.biBitCount <= 8 {
        let entries = if header.biClrUsed == 0 {
            1usize << header.biBitCount
        } else {
            header.biClrUsed as usize
        };
        entries * 4
    } else {
        header.biClrUsed as usize * 4
    };
    header.biSize as usize + masks + palette
}

/// Look at whatever just landed on the clipboard and decide whether it is a
/// capture of this screen.
fn inspect_clipboard(shared: &Shared) {
    let sequence_after = unsafe { GetClipboardSequenceNumber() };
    let sequence_before = shared
        .baseline_sequence
        .swap(sequence_after, Ordering::SeqCst);

    if unsafe { IsClipboardFormatAvailable(CF_DIB) } == 0 {
        shared
            .ignored_clipboard_updates
            .fetch_add(1, Ordering::SeqCst);
        return;
    }

    // Another process may hold the clipboard for a moment after writing it.
    let mut opened = false;
    for _ in 0..20 {
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    if !opened {
        shared
            .ignored_clipboard_updates
            .fetch_add(1, Ordering::SeqCst);
        return;
    }

    let mut observation = None;
    let handle = unsafe { GetClipboardData(CF_DIB) };
    if !handle.is_null() {
        let byte_len = unsafe { GlobalSize(handle) } as u64;
        let pointer = unsafe { GlobalLock(handle) } as *const u8;
        if !pointer.is_null() && byte_len >= std::mem::size_of::<BITMAPINFOHEADER>() as u64 {
            let header = unsafe { *(pointer as *const BITMAPINFOHEADER) };
            let bytes = unsafe { std::slice::from_raw_parts(pointer, byte_len as usize) };
            let (screen_width, screen_height) = virtual_screen();
            let covers_screen =
                header.biWidth == screen_width && header.biHeight.abs() == screen_height;
            // Only a bitmap that covers the whole screen is compared against
            // the screen; anything else is an ordinary copy and is left alone.
            let (distinct, ppm) = if covers_screen {
                let live = live_screen_readback(screen_width, screen_height);
                measure(
                    bytes,
                    packed_dib_pixel_offset(&header),
                    header.biWidth,
                    header.biHeight,
                    header.biBitCount,
                    live.as_deref(),
                )
            } else {
                (0, 0)
            };
            unsafe { GlobalUnlock(handle) };

            if covers_screen {
                let pressed = shared.last_print_screen_ms.load(Ordering::SeqCst);
                let key_seen = pressed != 0
                    && coarse_ms().wrapping_sub(pressed) <= PRINT_SCREEN_CORRELATION_MS as u32;
                observation = Some(ObservedCapture {
                    path: if key_seen {
                        SupportedCapturePath::PrintScreenClipboard
                    } else {
                        SupportedCapturePath::SnipToClipboard
                    },
                    evidence: CaptureEvidence {
                        clipboard_sequence_before: sequence_before,
                        clipboard_sequence_after: sequence_after,
                        dib_width: header.biWidth,
                        dib_height: header.biHeight,
                        dib_bit_count: header.biBitCount,
                        dib_byte_len: byte_len,
                        screen_width,
                        screen_height,
                        distinct_sampled_colors: distinct,
                        live_match_ppm: ppm,
                        print_screen_key_seen: key_seen,
                    },
                    observed_at_ms: now_ms(),
                });
            }
        } else if !pointer.is_null() {
            unsafe { GlobalUnlock(handle) };
        }
    }
    unsafe { CloseClipboard() };

    match observation {
        Some(observed) => {
            if let Ok(mut list) = shared.observations.lock() {
                list.push(observed);
            }
        }
        None => {
            shared
                .ignored_clipboard_updates
                .fetch_add(1, Ordering::SeqCst);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLIPBOARDUPDATE => {
            if let Some(shared) = current_shared() {
                inspect_clipboard(&shared);
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            windows_sys::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && (wparam as u32 == WM_KEYDOWN || wparam as u32 == WM_SYSKEYDOWN) {
        let info = &*(lparam as *const KBDLLHOOKSTRUCT);
        if info.vkCode == VK_SNAPSHOT as u32 {
            if let Some(shared) = current_shared() {
                shared
                    .last_print_screen_ms
                    .store(coarse_ms().max(1), Ordering::SeqCst);
                shared.print_screen_presses.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Watches the supported capture paths for as long as it is alive.
pub struct SupportedCaptureWatcher {
    shared: Arc<Shared>,
    hwnd: usize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl SupportedCaptureWatcher {
    /// Start watching. Fails rather than silently watching nothing: a viewer
    /// that cannot install the listener must not imply detection is running.
    pub fn start() -> Result<Self, String> {
        let shared = Arc::new(Shared {
            observations: Mutex::new(Vec::new()),
            ignored_clipboard_updates: AtomicU32::new(0),
            last_print_screen_ms: AtomicU32::new(0),
            baseline_sequence: AtomicU32::new(unsafe { GetClipboardSequenceNumber() }),
            print_screen_presses: AtomicU32::new(0),
        });
        {
            let mut slot = shared_slot()
                .lock()
                .map_err(|_| "the capture watcher registry is unavailable".to_owned())?;
            if slot.is_some() {
                return Err("a capture watcher is already running".to_owned());
            }
            *slot = Some(Arc::clone(&shared));
        }

        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<usize, String>>();
        let thread = std::thread::spawn(move || {
            let started = AtomicBool::new(false);
            unsafe {
                let instance = GetModuleHandleW(std::ptr::null());
                let class_name: Vec<u16> = "OslViewOnceCaptureWatch\0".encode_utf16().collect();
                let mut class: WNDCLASSW = std::mem::zeroed();
                class.lpfnWndProc = Some(window_proc);
                class.hInstance = instance;
                class.lpszClassName = class_name.as_ptr();
                RegisterClassW(&class); // A duplicate registration is fine.

                let hwnd = CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    class_name.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                );
                if hwnd.is_null() {
                    let _ = ready_tx.send(Err("the capture watch window was refused".to_owned()));
                    return;
                }
                if AddClipboardFormatListener(hwnd) == 0 {
                    DestroyWindow(hwnd);
                    let _ = ready_tx.send(Err(
                        "Windows refused the clipboard capture listener".to_owned()
                    ));
                    return;
                }
                let hook: HHOOK =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0);
                started.store(true, Ordering::SeqCst);
                let _ = ready_tx.send(Ok(hwnd as usize));

                let mut message: MSG = std::mem::zeroed();
                while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }

                if !hook.is_null() {
                    UnhookWindowsHookEx(hook);
                }
                RemoveClipboardFormatListener(hwnd);
            }
        });

        match ready_rx.recv() {
            Ok(Ok(hwnd)) => Ok(Self {
                shared,
                hwnd,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                if let Ok(mut slot) = shared_slot().lock() {
                    *slot = None;
                }
                Err(error)
            }
            Err(_) => {
                if let Ok(mut slot) = shared_slot().lock() {
                    *slot = None;
                }
                Err("the capture watch thread stopped before it started".to_owned())
            }
        }
    }

    /// Captures observed so far, oldest first.
    pub fn observations(&self) -> Vec<ObservedCapture> {
        self.shared
            .observations
            .lock()
            .map(|list| list.clone())
            .unwrap_or_default()
    }

    /// Clipboard updates that were looked at and were not a capture of this
    /// screen.
    pub fn ignored_clipboard_updates(&self) -> u32 {
        self.shared.ignored_clipboard_updates.load(Ordering::SeqCst)
    }

    pub fn print_screen_presses(&self) -> u32 {
        self.shared.print_screen_presses.load(Ordering::SeqCst)
    }

    /// Block until a capture is observed or `timeout` elapses.
    pub fn wait_for_capture(&self, timeout: std::time::Duration) -> Option<ObservedCapture> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(observed) = self.observations().into_iter().next() {
                return Some(observed);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

impl Drop for SupportedCaptureWatcher {
    fn drop(&mut self) {
        unsafe {
            PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Ok(mut slot) = shared_slot().lock() {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_rows_flip_with_the_height_sign() {
        // Bottom-up: the top row of the image is the last row in memory.
        assert_eq!(dib_offset(0, 0, 4, 3, 32), 2 * 16);
        assert_eq!(dib_offset(0, 2, 4, 3, 32), 0);
        // Top-down: the top row of the image is the first row in memory.
        assert_eq!(dib_offset(0, 0, 4, -3, 32), 0);
        assert_eq!(dib_offset(0, 2, 4, -3, 32), 2 * 16);
    }

    #[test]
    fn stride_rounds_up_to_four_bytes() {
        assert_eq!(stride(3, 32), 12);
        assert_eq!(stride(3, 24), 12);
        assert_eq!(stride(5, 8), 8);
    }

    #[test]
    fn a_print_screen_dib_has_masks_before_its_pixels() {
        let mut header: BITMAPINFOHEADER = unsafe { std::mem::zeroed() };
        header.biSize = 40;
        header.biBitCount = 32;
        header.biCompression = BI_BITFIELDS;
        assert_eq!(packed_dib_pixel_offset(&header), 52);
        header.biCompression = 0;
        assert_eq!(packed_dib_pixel_offset(&header), 40);
    }

    #[test]
    fn a_flat_bitmap_measures_as_one_colour() {
        let width = 64i32;
        let height = 64i32;
        let pixels = vec![7u8; stride(width, 32) * height as usize];
        let (distinct, ppm) = measure(&pixels, 0, width, height, 32, Some(&pixels));
        assert_eq!(distinct, 1);
        assert_eq!(ppm, 1_000_000);
    }

    #[test]
    fn a_bitmap_of_a_different_screen_does_not_agree_with_it() {
        let width = 64i32;
        let height = 64i32;
        let mine = vec![7u8; stride(width, 32) * height as usize];
        let theirs = vec![200u8; stride(width, 32) * height as usize];
        let (_, ppm) = measure(&mine, 0, width, height, 32, Some(&theirs));
        assert_eq!(ppm, 0);
    }
}
