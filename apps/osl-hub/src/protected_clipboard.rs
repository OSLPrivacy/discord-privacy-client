//! Clipboard writes for protected-message placement.
//!
//! The private draft is never an argument to this module. Callers must finish
//! encryption/cover generation first, then hand over only the public text that
//! is safe for the host clipboard and its history.

use crate::invite_clipboard;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinishedCoverText(String);

impl FinishedCoverText {
    pub fn new(value: String) -> Result<Self, String> {
        if value.trim().is_empty() {
            return Err("protected clipboard cover text is empty".to_owned());
        }
        if value.len() > 2_000 {
            return Err("protected clipboard cover text exceeds the placement limit".to_owned());
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedClipboardPlacement {
    pub placed_clipboard: String,
    pub restored_clipboard: Option<String>,
    pub cover_history_entries_removed: usize,
}

/// Copy a completed protected-message carrier. This function deliberately does
/// not accept a plaintext/draft parameter.
pub fn write_finished_cover_text_to_clipboard(
    cover_text: &FinishedCoverText,
) -> Result<(), String> {
    invite_clipboard::write_desktop_clipboard_text(cover_text.as_str())
}

/// Temporarily stage a completed carrier for placement, then put the previous
/// clipboard text back and remove the carrier from Windows clipboard history.
pub fn place_finished_cover_text_with_clipboard<F>(
    cover_text: &FinishedCoverText,
    place: F,
) -> Result<ProtectedClipboardPlacement, String>
where
    F: FnOnce() -> Result<(), String>,
{
    #[cfg(windows)]
    {
        let mut backend = WindowsProtectedClipboard;
        place_finished_cover_text_with_backend(&mut backend, cover_text.as_str(), place)
    }
    #[cfg(not(windows))]
    {
        write_finished_cover_text_to_clipboard(cover_text)?;
        place()?;
        Ok(ProtectedClipboardPlacement {
            placed_clipboard: cover_text.as_str().to_owned(),
            restored_clipboard: None,
            cover_history_entries_removed: 0,
        })
    }
}

trait ProtectedClipboardBackend {
    fn read_text(&mut self) -> Result<String, String>;
    fn write_text(&mut self, value: &str) -> Result<(), String>;
    fn clear(&mut self) -> Result<(), String>;
    fn delete_history_text(&mut self, value: &str) -> Result<usize, String>;
}

fn place_finished_cover_text_with_backend<F>(
    backend: &mut dyn ProtectedClipboardBackend,
    cover_text: &str,
    place: F,
) -> Result<ProtectedClipboardPlacement, String>
where
    F: FnOnce() -> Result<(), String>,
{
    let saved_text = backend.read_text()?;
    backend.write_text(cover_text)?;

    let place_result = place();
    let restore_result = if saved_text.is_empty() {
        backend.clear()
    } else {
        backend.write_text(&saved_text)
    };
    let history_result = backend.delete_history_text(cover_text);

    place_result?;
    restore_result?;
    let removed = history_result?;

    Ok(ProtectedClipboardPlacement {
        placed_clipboard: cover_text.to_owned(),
        restored_clipboard: (!saved_text.is_empty()).then_some(saved_text),
        cover_history_entries_removed: removed,
    })
}

#[cfg(windows)]
struct WindowsProtectedClipboard;

#[cfg(windows)]
impl ProtectedClipboardBackend for WindowsProtectedClipboard {
    fn read_text(&mut self) -> Result<String, String> {
        windows_clipboard_text()
    }

    fn write_text(&mut self, value: &str) -> Result<(), String> {
        set_windows_clipboard_text(value)
    }

    fn clear(&mut self) -> Result<(), String> {
        windows::ApplicationModel::DataTransfer::Clipboard::Clear()
            .map_err(|error| format!("The Windows clipboard could not be cleared: {error}"))
    }

    fn delete_history_text(&mut self, value: &str) -> Result<usize, String> {
        delete_windows_clipboard_history_text(value)
    }
}

#[cfg(windows)]
fn windows_clipboard_text() -> Result<String, String> {
    use windows::ApplicationModel::DataTransfer::{Clipboard, StandardDataFormats};

    let content = Clipboard::GetContent()
        .map_err(|error| format!("The Windows clipboard is unavailable: {error}"))?;
    let text_format = StandardDataFormats::Text()
        .map_err(|error| format!("The Windows clipboard text format is unavailable: {error}"))?;
    if !content
        .Contains(&text_format)
        .map_err(|error| format!("The Windows clipboard formats could not be read: {error}"))?
    {
        return Ok(String::new());
    }
    content
        .GetTextAsync()
        .map_err(|error| format!("The Windows clipboard text could not be requested: {error}"))?
        .get()
        .map(|text| text.to_string())
        .map_err(|error| format!("The Windows clipboard text could not be read: {error}"))
}

#[cfg(windows)]
fn set_windows_clipboard_text(value: &str) -> Result<(), String> {
    use std::{ptr, thread, time::Duration};
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::CF_UNICODETEXT,
        },
    };

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            // SAFETY: the guard is created only after this thread opens the
            // clipboard and closes that same open operation.
            unsafe { CloseClipboard() };
        }
    }

    let mut opened = false;
    for _ in 0..8 {
        // SAFETY: a null owner is explicitly supported by OpenClipboard.
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        thread::sleep(Duration::from_millis(8));
    }
    if !opened {
        return Err("The Windows clipboard is busy".to_owned());
    }
    let _clipboard = ClipboardGuard;

    let utf16 = value
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let byte_len = utf16
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| "The protected cover text is too large for the clipboard".to_owned())?;
    // SAFETY: byte_len is checked above and ownership remains here until
    // SetClipboardData succeeds.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) };
    if memory.is_null() {
        return Err("The Windows clipboard could not allocate memory".to_owned());
    }
    // SAFETY: memory is a valid allocation from GlobalAlloc.
    let destination = unsafe { GlobalLock(memory) }.cast::<u16>();
    if destination.is_null() {
        // SAFETY: ownership has not been transferred to the clipboard.
        unsafe { GlobalFree(memory) };
        return Err("The Windows clipboard memory could not be locked".to_owned());
    }
    // SAFETY: destination is byte_len bytes and utf16 has exactly that many
    // bytes including the trailing NUL.
    unsafe {
        ptr::copy_nonoverlapping(utf16.as_ptr(), destination, utf16.len());
        GlobalUnlock(memory);
    }
    // SAFETY: this thread owns the open clipboard.
    if unsafe { EmptyClipboard() } == 0 {
        // SAFETY: ownership has not been transferred to the clipboard.
        unsafe { GlobalFree(memory) };
        return Err("The Windows clipboard could not be cleared".to_owned());
    }
    // SAFETY: after success Windows owns memory; after failure we free it.
    if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null() {
        unsafe { GlobalFree(memory) };
        return Err("The protected cover text could not be copied".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
fn delete_windows_clipboard_history_text(value: &str) -> Result<usize, String> {
    use windows::ApplicationModel::DataTransfer::{
        Clipboard, ClipboardHistoryItemsResultStatus, StandardDataFormats,
    };

    if !Clipboard::IsHistoryEnabled().map_err(|error| {
        format!("The Windows clipboard history state could not be read: {error}")
    })? {
        return Ok(0);
    }

    let result = Clipboard::GetHistoryItemsAsync()
        .map_err(|error| format!("The Windows clipboard history could not be requested: {error}"))?
        .get()
        .map_err(|error| format!("The Windows clipboard history could not be read: {error}"))?;
    let status = result.Status().map_err(|error| {
        format!("The Windows clipboard history status could not be read: {error}")
    })?;
    if status != ClipboardHistoryItemsResultStatus::Success {
        return Err(format!(
            "The Windows clipboard history read did not succeed: {status:?}"
        ));
    }

    let items = result.Items().map_err(|error| {
        format!("The Windows clipboard history items could not be read: {error}")
    })?;
    let text_format = StandardDataFormats::Text()
        .map_err(|error| format!("The Windows clipboard text format is unavailable: {error}"))?;
    let mut removed = 0usize;
    for index in 0..items.Size().map_err(|error| {
        format!("The Windows clipboard history count could not be read: {error}")
    })? {
        let item = items.GetAt(index).map_err(|error| {
            format!("The Windows clipboard history item could not be read: {error}")
        })?;
        let content = item.Content().map_err(|error| {
            format!("The Windows clipboard history item content could not be read: {error}")
        })?;
        if !content.Contains(&text_format).map_err(|error| {
            format!("The Windows clipboard history item formats could not be read: {error}")
        })? {
            continue;
        }
        let text = content
            .GetTextAsync()
            .map_err(|error| {
                format!("The Windows clipboard history item text could not be requested: {error}")
            })?
            .get()
            .map_err(|error| {
                format!("The Windows clipboard history item text could not be read: {error}")
            })?
            .to_string();
        if text == value
            && Clipboard::DeleteItemFromHistory(&item).map_err(|error| {
                format!("The Windows clipboard history item could not be deleted: {error}")
            })?
        {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_cover_text_refuses_empty_clipboard_payloads() {
        assert!(FinishedCoverText::new("".to_owned()).is_err());
        assert!(FinishedCoverText::new(" \n\t ".to_owned()).is_err());
    }

    #[test]
    fn finished_cover_text_bounds_clipboard_payloads_to_one_message() {
        assert!(FinishedCoverText::new("a".repeat(2_000)).is_ok());
        assert!(FinishedCoverText::new("a".repeat(2_001)).is_err());
    }

    #[test]
    fn placement_restores_previous_clipboard_and_removes_cover_history() {
        let mut backend = FakeProtectedClipboard {
            clipboard: "PERSON3405".to_owned(),
            history_removed: 1,
            events: Vec::new(),
        };

        let report =
            place_finished_cover_text_with_backend(&mut backend, "COVER3410", || Ok(())).unwrap();

        assert_eq!(backend.clipboard, "PERSON3405");
        assert_eq!(
            report,
            ProtectedClipboardPlacement {
                placed_clipboard: "COVER3410".to_owned(),
                restored_clipboard: Some("PERSON3405".to_owned()),
                cover_history_entries_removed: 1,
            }
        );
        assert_eq!(
            backend.events,
            [
                "read",
                "write:COVER3410",
                "write:PERSON3405",
                "delete_history:COVER3410"
            ]
        );
    }

    #[test]
    fn failed_placement_still_restores_and_removes_cover_history() {
        let mut backend = FakeProtectedClipboard {
            clipboard: "PERSON3405".to_owned(),
            history_removed: 1,
            events: Vec::new(),
        };
        let error = place_finished_cover_text_with_backend(&mut backend, "COVER3410", || {
            Err("placement failed".to_owned())
        })
        .unwrap_err();

        assert_eq!(error, "placement failed");
        assert_eq!(backend.clipboard, "PERSON3405");
        assert_eq!(
            backend.events,
            [
                "read",
                "write:COVER3410",
                "write:PERSON3405",
                "delete_history:COVER3410"
            ]
        );
    }

    struct FakeProtectedClipboard {
        clipboard: String,
        history_removed: usize,
        events: Vec<&'static str>,
    }

    impl ProtectedClipboardBackend for FakeProtectedClipboard {
        fn read_text(&mut self) -> Result<String, String> {
            self.events.push("read");
            Ok(self.clipboard.clone())
        }

        fn write_text(&mut self, value: &str) -> Result<(), String> {
            self.clipboard = value.to_owned();
            self.events.push(match value {
                "COVER3410" => "write:COVER3410",
                "PERSON3405" => "write:PERSON3405",
                _ => "write:other",
            });
            Ok(())
        }

        fn clear(&mut self) -> Result<(), String> {
            self.clipboard.clear();
            self.events.push("clear");
            Ok(())
        }

        fn delete_history_text(&mut self, value: &str) -> Result<usize, String> {
            self.events.push(match value {
                "COVER3410" => "delete_history:COVER3410",
                _ => "delete_history:other",
            });
            Ok(self.history_removed)
        }
    }
}
