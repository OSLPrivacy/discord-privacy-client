//! Outlook desktop control driver.
//!
//! Outlook on the web is an email service surface. This module is only for the
//! installed Windows desktop program: classic Outlook (`OUTLOOK.EXE`) and New
//! Outlook (`olk.exe`). The first live driver capability is intentionally
//! narrow and read-only: bind a real top-level Outlook window and read its
//! title through Win32.

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OutlookDesktopControlTarget {
    pub name: &'static str,
    pub scope: &'static str,
    pub control_type: &'static str,
    pub ui_names: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OutlookDesktopControlDriver {
    pub driver_id: &'static str,
    pub surface: &'static str,
    pub process_names: &'static [&'static str],
    pub window_classes: &'static [&'static str],
    pub title_window_classes: &'static [&'static str],
    pub title_reader_command: &'static str,
    pub targets: &'static [OutlookDesktopControlTarget],
}

pub const OUTLOOK_CLASSIC_PROCESS_NAME: &str = "OUTLOOK";
pub const OUTLOOK_NEW_PROCESS_NAME: &str = "olk";
pub const OUTLOOK_DESKTOP_PROCESS_NAMES: &[&str] =
    &[OUTLOOK_CLASSIC_PROCESS_NAME, OUTLOOK_NEW_PROCESS_NAME];

pub const OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS: &str = "rctrl_renwnd32";
pub const OUTLOOK_NEW_PRIMARY_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
pub const OUTLOOK_DESKTOP_WINDOW_CLASSES: &[&str] = &[
    OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS,
    OUTLOOK_NEW_PRIMARY_WINDOW_CLASS,
];
pub const OUTLOOK_CLASSIC_SETUP_DIALOG_CLASS: &str = "NUIDialog";
pub const OUTLOOK_CLASSIC_SPLASH_WINDOW_CLASS: &str = "MsoSplash";
pub const OUTLOOK_DESKTOP_TITLE_WINDOW_CLASSES: &[&str] = &[
    OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS,
    OUTLOOK_NEW_PRIMARY_WINDOW_CLASS,
    OUTLOOK_CLASSIC_SETUP_DIALOG_CLASS,
    OUTLOOK_CLASSIC_SPLASH_WINDOW_CLASS,
];

pub const OUTLOOK_DESKTOP_TITLE_READER_COMMAND: &str =
    "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/qa/outlook/read-outlook-desktop-title.ps1";

pub const OUTLOOK_DESKTOP_CONTROL_TARGETS: &[OutlookDesktopControlTarget] = &[
    OutlookDesktopControlTarget {
        name: "ribbon New Mail",
        scope: "ribbon",
        control_type: "Button",
        ui_names: &["New Mail", "New Email"],
    },
    OutlookDesktopControlTarget {
        name: "body",
        scope: "compose",
        control_type: "Document",
        ui_names: &["Message body", "Body"],
    },
    OutlookDesktopControlTarget {
        name: "Send",
        scope: "compose",
        control_type: "Button",
        ui_names: &["Send"],
    },
    OutlookDesktopControlTarget {
        name: "reading pane",
        scope: "mail",
        control_type: "Pane",
        ui_names: &["Reading Pane", "Reading pane"],
    },
    OutlookDesktopControlTarget {
        name: "folders",
        scope: "mail",
        control_type: "Tree",
        ui_names: &["Folders", "Folder Pane", "Navigation Pane"],
    },
    OutlookDesktopControlTarget {
        name: "conversation view",
        scope: "mail",
        control_type: "List",
        ui_names: &["Conversation View", "Conversation view", "Message List"],
    },
];

pub fn outlook_desktop_control_targets() -> &'static [OutlookDesktopControlTarget] {
    OUTLOOK_DESKTOP_CONTROL_TARGETS
}

pub fn outlook_desktop_control_driver() -> OutlookDesktopControlDriver {
    OutlookDesktopControlDriver {
        driver_id: "outlook-desktop-win32",
        surface: "desktop-native",
        process_names: OUTLOOK_DESKTOP_PROCESS_NAMES,
        window_classes: OUTLOOK_DESKTOP_WINDOW_CLASSES,
        title_window_classes: OUTLOOK_DESKTOP_TITLE_WINDOW_CLASSES,
        title_reader_command: OUTLOOK_DESKTOP_TITLE_READER_COMMAND,
        targets: outlook_desktop_control_targets(),
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum OutlookDesktopTitleError {
    PlatformUnsupported,
    WindowUnavailable,
    TitleUnavailable,
}

#[cfg(not(target_os = "windows"))]
pub fn read_outlook_desktop_window_title() -> Result<String, OutlookDesktopTitleError> {
    Err(OutlookDesktopTitleError::PlatformUnsupported)
}

#[cfg(target_os = "windows")]
pub fn read_outlook_desktop_window_title() -> Result<String, OutlookDesktopTitleError> {
    windows::read_outlook_desktop_window_title()
}

#[cfg(target_os = "windows")]
mod windows {
    use super::{
        OutlookDesktopTitleError, OUTLOOK_DESKTOP_PROCESS_NAMES,
        OUTLOOK_DESKTOP_TITLE_WINDOW_CLASSES,
    };
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
    type RawHandle = *mut std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit_handle: BOOL, process_id: u32) -> RawHandle;
        fn QueryFullProcessImageNameW(
            process: RawHandle,
            flags: u32,
            path: *mut u16,
            path_len: *mut u32,
        ) -> BOOL;
        fn CloseHandle(handle: RawHandle) -> BOOL;
    }

    struct Search {
        title: Option<String>,
    }

    pub fn read_outlook_desktop_window_title() -> Result<String, OutlookDesktopTitleError> {
        let mut search = Search { title: None };
        unsafe {
            EnumWindows(
                Some(enum_outlook_desktop_title),
                (&mut search as *mut Search) as LPARAM,
            );
        }
        search
            .title
            .ok_or(OutlookDesktopTitleError::WindowUnavailable)
    }

    unsafe extern "system" fn enum_outlook_desktop_title(window: HWND, parameter: LPARAM) -> BOOL {
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut class_name = [0u16; 128];
        let class_length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        if class_length <= 0 {
            return 1;
        }
        let class_name = String::from_utf16_lossy(&class_name[..class_length as usize]);
        if !OUTLOOK_DESKTOP_TITLE_WINDOW_CLASSES.contains(&class_name.as_str()) {
            return 1;
        }
        let mut process_id = 0u32;
        if GetWindowThreadProcessId(window, &mut process_id) == 0 || process_id == 0 {
            return 1;
        }
        if !process_is_outlook(process_id) {
            return 1;
        }
        let mut title = [0u16; 512];
        let title_length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        if title_length <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&title[..title_length as usize]);
        if title.trim().is_empty() {
            return 1;
        }
        (*(parameter as *mut Search)).title = Some(title);
        0
    }

    unsafe fn process_is_outlook(process_id: u32) -> bool {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id);
        if process.is_null() {
            return false;
        }
        let matched = process_image_path(process)
            .as_deref()
            .and_then(Path::file_stem)
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| {
                OUTLOOK_DESKTOP_PROCESS_NAMES
                    .iter()
                    .any(|name| stem.eq_ignore_ascii_case(name))
            });
        let _ = CloseHandle(process);
        matched
    }

    unsafe fn process_image_path(process: RawHandle) -> Option<std::path::PathBuf> {
        let mut path = vec![0u16; 32_768];
        let mut path_len = path.len() as u32;
        if QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut path_len) == 0
            || path_len == 0
        {
            return None;
        }
        path.truncate(path_len as usize);
        Some(std::path::PathBuf::from(OsString::from_wide(&path)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outlook_desktop_driver_is_a_separate_win32_title_reader() {
        let driver = outlook_desktop_control_driver();
        assert_eq!(driver.driver_id, "outlook-desktop-win32");
        assert_eq!(driver.surface, "desktop-native");
        assert_eq!(driver.process_names, &["OUTLOOK", "olk"]);
        assert_eq!(
            driver.window_classes,
            &["rctrl_renwnd32", "WinUIDesktopWin32WindowClass"]
        );
        assert_eq!(
            driver.title_window_classes,
            &[
                "rctrl_renwnd32",
                "WinUIDesktopWin32WindowClass",
                "NUIDialog",
                "MsoSplash"
            ]
        );
        assert_eq!(
            driver
                .targets
                .iter()
                .map(|target| target.name)
                .collect::<Vec<_>>(),
            vec![
                "ribbon New Mail",
                "body",
                "Send",
                "reading pane",
                "folders",
                "conversation view",
            ]
        );
        assert!(driver.title_reader_command.starts_with("powershell.exe "));
        assert!(driver
            .title_reader_command
            .contains("read-outlook-desktop-title.ps1"));
        assert!(!driver.title_reader_command.contains("outlook.live.com"));
        assert!(!driver.title_reader_command.contains("firefox"));
        assert!(!driver.title_reader_command.contains("browser"));

        println!("outlook desktop driver={}", driver.driver_id);
        println!("outlook desktop surface={}", driver.surface);
        println!(
            "outlook desktop title command={}",
            driver.title_reader_command
        );
    }
}

// Outlook desktop read-only adapter pieces.
//
// The mailbox reader is deliberately local and read-only. It adapts Outlook
// desktop message facts into the shared mailbox reader contract used by Scrub.

use crate::services::{
    open_shared_mailbox_message, read_shared_mailbox_folders, read_shared_mailbox_messages,
    MailboxFolderCandidate, MailboxMessageCandidate, MailboxReaderSnapshot, SharedMailboxFolder,
    SharedMailboxMessage, SharedMailboxMessageSummary,
};

pub const OUTLOOK_DESKTOP_MAIL_READER_ID: &str = "outlook-desktop-shared-mailbox-reader";
pub const OUTLOOK_DESKTOP_SERVICE_ID: &str = "outlook";
pub const OUTLOOK_DESKTOP_SEEDED_OWNER: &str = "osl_task_3053_owner";
pub const OUTLOOK_DESKTOP_SEEDED_ACCOUNT: &str = "outlook-desktop-scrub";
pub const OUTLOOK_DESKTOP_SEEDED_SIGNED_IN_ADDRESS: &str = "scrub.owner@example.test";
pub const OUTLOOK_DESKTOP_MINE_MARKER: &str = "SCRUB-OD-MINE";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutlookDesktopMailbox {
    owner_osl_user_id: String,
    account_id: String,
    signed_in_address: String,
    snapshot: MailboxReaderSnapshot,
}

impl OutlookDesktopMailbox {
    pub fn new(
        owner_osl_user_id: impl Into<String>,
        account_id: impl Into<String>,
        signed_in_address: impl Into<String>,
        snapshot: MailboxReaderSnapshot,
    ) -> Self {
        Self {
            owner_osl_user_id: owner_osl_user_id.into(),
            account_id: account_id.into(),
            signed_in_address: signed_in_address.into(),
            snapshot,
        }
    }

    pub fn signed_in_address(&self) -> &str {
        &self.signed_in_address
    }

    pub fn read_folders(&self) -> Result<Vec<SharedMailboxFolder>, String> {
        read_shared_mailbox_folders(
            &self.owner_osl_user_id,
            OUTLOOK_DESKTOP_SERVICE_ID,
            &self.account_id,
            &self.snapshot,
        )
    }

    pub fn read_messages(
        &self,
        folder_id: &str,
    ) -> Result<Vec<SharedMailboxMessageSummary>, String> {
        read_shared_mailbox_messages(
            &self.owner_osl_user_id,
            OUTLOOK_DESKTOP_SERVICE_ID,
            &self.account_id,
            folder_id,
            &self.snapshot,
        )
    }

    pub fn open_message(
        &self,
        folder_id: &str,
        message_id: &str,
    ) -> Result<SharedMailboxMessage, String> {
        open_shared_mailbox_message(
            &self.owner_osl_user_id,
            OUTLOOK_DESKTOP_SERVICE_ID,
            &self.account_id,
            folder_id,
            message_id,
            &self.snapshot,
        )
    }
}

pub fn seeded_outlook_desktop_scrub_mailbox() -> OutlookDesktopMailbox {
    OutlookDesktopMailbox::new(
        OUTLOOK_DESKTOP_SEEDED_OWNER,
        OUTLOOK_DESKTOP_SEEDED_ACCOUNT,
        OUTLOOK_DESKTOP_SEEDED_SIGNED_IN_ADDRESS,
        MailboxReaderSnapshot::new(
            [
                MailboxFolderCandidate::new("Inbox", "Inbox"),
                MailboxFolderCandidate::new("Sent Items", "Sent Items"),
                MailboxFolderCandidate::new("Archive", "Archive"),
                MailboxFolderCandidate::new("Deleted Items", "Deleted Items"),
            ],
            [
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "outlook-desktop-sent-001",
                    "SCRUB-OD-MINE",
                    1_786_032_000,
                    OUTLOOK_DESKTOP_SEEDED_SIGNED_IN_ADDRESS,
                    "Outlook desktop seeded owner message.",
                ),
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "outlook-desktop-sent-002",
                    "Outlook desktop cleanup receipt",
                    1_786_035_600,
                    "delegate@example.test",
                    "Second seeded Sent Items body.",
                ),
                MailboxMessageCandidate::new(
                    "Sent Items",
                    "outlook-desktop-sent-003",
                    "Outlook desktop account notice",
                    1_786_039_200,
                    "noreply@example.test",
                    "Third seeded Sent Items body.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "outlook-desktop-inbox-001",
                    "Inbox task 3053 first",
                    1_786_042_800,
                    "friend@example.test",
                    "First seeded Inbox body.",
                ),
                MailboxMessageCandidate::new(
                    "Inbox",
                    "outlook-desktop-inbox-002",
                    "Inbox task 3053 second",
                    1_786_046_400,
                    "alerts@example.test",
                    "Second seeded Inbox body.",
                ),
            ],
        ),
    )
}

pub const OUTLOOK_DESKTOP_TASK_1286_MARKER: &str = "OSL-OUTLOOK-DESKTOP-1286";
pub const OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS: &str = "OSL-OUTLOOK-DESKTOP-1286 cover message";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct OutlookDesktopMappedControl {
    pub name: &'static str,
}

pub const OUTLOOK_DESKTOP_TASK_1286_CONTROLS: &[OutlookDesktopMappedControl] = &[
    OutlookDesktopMappedControl { name: "Place" },
    OutlookDesktopMappedControl { name: "Read" },
    OutlookDesktopMappedControl { name: "Send" },
];

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OutlookDesktopTask1286Fixture {
    controls: Vec<&'static str>,
    placed_messages: Vec<String>,
    sent_messages: Vec<String>,
}

impl Default for OutlookDesktopTask1286Fixture {
    fn default() -> Self {
        Self {
            controls: OUTLOOK_DESKTOP_TASK_1286_CONTROLS
                .iter()
                .map(|control| control.name)
                .collect(),
            placed_messages: Vec::new(),
            sent_messages: Vec::new(),
        }
    }
}

impl OutlookDesktopTask1286Fixture {
    pub fn start() -> Self {
        Self::default()
    }

    pub fn control_names(&self) -> Vec<&'static str> {
        self.controls.clone()
    }

    pub fn placed_message_count(&self) -> usize {
        self.placed_messages.len()
    }

    pub fn sent_count(&self) -> usize {
        self.sent_messages.len()
    }

    pub fn place(&mut self) -> Result<&'static str, String> {
        self.require_control("Place")?;
        self.placed_messages
            .push(OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS.to_owned());
        Ok(OUTLOOK_DESKTOP_TASK_1286_COVER_WORDS)
    }

    pub fn read(&self) -> Result<&str, String> {
        self.require_control("Read")?;
        self.placed_messages
            .last()
            .map(String::as_str)
            .ok_or_else(|| "Outlook desktop fixture has no placed cover message".to_owned())
    }

    pub fn send(&mut self) -> Result<usize, String> {
        self.require_control("Send")?;
        let message = self
            .placed_messages
            .last()
            .cloned()
            .ok_or_else(|| "Outlook desktop fixture has no placed cover message".to_owned())?;
        self.sent_messages.push(message);
        Ok(self.sent_messages.len())
    }

    pub fn remove_control(&mut self, name: &str) -> Result<bool, String> {
        if name == "Send" {
            return Err("Outlook desktop fixture refused removing Send".to_owned());
        }
        let before = self.controls.len();
        self.controls.retain(|control| *control != name);
        Ok(self.controls.len() != before)
    }

    fn require_control(&self, name: &str) -> Result<(), String> {
        if self.controls.contains(&name) {
            Ok(())
        } else {
            Err(format!(
                "Outlook desktop fixture control {name} is unavailable"
            ))
        }
    }
}

pub fn fake_outlook_desktop_task_1286_fixture() -> OutlookDesktopTask1286Fixture {
    OutlookDesktopTask1286Fixture::start()
}
