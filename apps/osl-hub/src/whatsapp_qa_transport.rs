//! QA-only, foreground-free placement into an already visually bound WhatsApp composer.
//!
//! This is deliberately not a generic automation surface. It accepts only the
//! exact claimed WhatsApp host and its verified composer rectangle, never
//! foregrounds a window, never synthesizes Send, and reports only whether
//! Windows accepted the bounded placement messages.

#[cfg(target_os = "windows")]
pub fn dispatch_bound_cover_text(
    host: &crate::whatsapp_qa_host::WhatsAppQaHostState,
    composer: [i32; 4],
    cover_text: &str,
) -> Result<(), String> {
    use std::{thread, time::Duration};
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, PostMessageW, WindowFromPoint, GA_ROOT, WM_KEYDOWN,
        WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    };

    let mut utf16 = cover_text
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if utf16.is_empty()
        || utf16.len() > 16 * 1024 + 1
        || utf16[..utf16.len() - 1]
            .iter()
            .any(|unit| matches!(*unit, 0 | 10 | 13))
    {
        return Err("The protected carrier is outside the bounded dispatch format".to_owned());
    }
    stage_ciphertext_clipboard(&utf16)?;
    utf16.fill(0);
    let [left, top, right, bottom] = composer;
    if right <= left || bottom <= top {
        return Err("The verified composer rectangle is invalid".to_owned());
    }
    host.with_current_accessibility_target(|target| {
        let hwnd = target.window as _;
        let foreground = unsafe { GetForegroundWindow() };
        let mut point = POINT {
            x: left.saturating_add((right - left) / 2),
            y: top.saturating_add((bottom - top) / 2),
        };
        let at_point = unsafe { WindowFromPoint(point) };
        if at_point.is_null() || unsafe { GetAncestor(at_point, GA_ROOT) } != hwnd {
            return Err("The verified WhatsApp composer is obscured".to_owned());
        }
        if unsafe { ScreenToClient(hwnd, &mut point) } == 0
            || point.x < 0
            || point.y < 0
            || point.x > u16::MAX.into()
            || point.y > u16::MAX.into()
        {
            return Err("The verified composer point could not be mapped safely".to_owned());
        }
        let mouse_position =
            (u32::from(point.x as u16) | (u32::from(point.y as u16) << 16)) as isize;
        if unsafe { PostMessageW(hwnd, WM_LBUTTONDOWN, 1, mouse_position) } == 0
            || unsafe { PostMessageW(hwnd, WM_LBUTTONUP, 0, mouse_position) } == 0
        {
            return Err("Windows rejected the bounded composer click".to_owned());
        }
        thread::sleep(Duration::from_millis(80));
        let key_up = ((1u32 << 30) | (1u32 << 31)) as isize;
        let paste = unsafe { PostMessageW(hwnd, WM_KEYDOWN, VK_CONTROL as usize, 1) } != 0
            && unsafe { PostMessageW(hwnd, WM_KEYDOWN, usize::from(b'V'), 1) } != 0
            && unsafe { PostMessageW(hwnd, WM_KEYUP, usize::from(b'V'), key_up) } != 0
            && unsafe { PostMessageW(hwnd, WM_KEYUP, VK_CONTROL as usize, key_up) } != 0;
        if !paste {
            clear_clipboard_best_effort();
            return Err("Windows rejected the protected carrier placement".to_owned());
        }
        thread::sleep(Duration::from_millis(80));
        clear_clipboard_best_effort();
        if unsafe { GetForegroundWindow() } != foreground {
            return Err("Foreground state changed during protected dispatch".to_owned());
        }
        Ok(())
    })
}

#[cfg(target_os = "windows")]
fn stage_ciphertext_clipboard(utf16: &[u16]) -> Result<(), String> {
    use std::{ptr, thread, time::Duration};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    let mut opened = false;
    for _ in 0..8 {
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        thread::sleep(Duration::from_millis(8));
    }
    if !opened {
        return Err("The ciphertext clipboard is busy".to_owned());
    }
    let byte_len = utf16
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| "The protected carrier is too large".to_owned())?;
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) };
    if memory.is_null() {
        unsafe { CloseClipboard() };
        return Err("The ciphertext clipboard allocation failed".to_owned());
    }
    let destination = unsafe { GlobalLock(memory) }.cast::<u16>();
    if destination.is_null() {
        unsafe {
            GlobalFree(memory);
            CloseClipboard();
        }
        return Err("The ciphertext clipboard lock failed".to_owned());
    }
    unsafe {
        ptr::copy_nonoverlapping(utf16.as_ptr(), destination, utf16.len());
        GlobalUnlock(memory);
    }
    if unsafe { EmptyClipboard() } == 0
        || unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null()
    {
        unsafe {
            GlobalFree(memory);
            CloseClipboard();
        }
        return Err("The ciphertext clipboard write failed".to_owned());
    }
    unsafe { CloseClipboard() };
    Ok(())
}

#[cfg(target_os = "windows")]
fn clear_clipboard_best_effort() {
    use std::{ptr, thread, time::Duration};
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
    for _ in 0..8 {
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            unsafe {
                EmptyClipboard();
                CloseClipboard();
            }
            return;
        }
        thread::sleep(Duration::from_millis(8));
    }
}

#[cfg(not(target_os = "windows"))]
pub fn dispatch_bound_cover_text(
    _host: &crate::whatsapp_qa_host::WhatsAppQaHostState,
    _composer: [i32; 4],
    _cover_text: &str,
) -> Result<(), String> {
    Err("WhatsApp QA dispatch requires Windows".to_owned())
}
