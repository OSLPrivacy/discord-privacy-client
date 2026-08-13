//! Read-only live Discord direct-message composer check (TASK 0902).
//!
//! This is deliberately a shipping-binary command, not a DOM fixture.  On
//! Windows it enumerates only visible `Discord.exe` top-level windows and asks
//! UI Automation for exactly one enabled, visible Edit whose accessible name is
//! Discord's direct-message form, `Message @<person>`.  A group channel's
//! `Message #<channel>` surface, a sign-in/search edit, ambiguity, and a closed
//! Discord process all fail closed with exit status 1.

use serde::Serialize;
use std::ffi::OsString;

pub const DISCORD_TYPING_BOX_CHECK_CLI_FLAG: &str = "--check-discord-typing-box";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TypingBoxCheckOk {
    ok: bool,
    command: &'static str,
    carrier: &'static str,
    conversation: String,
    typing_boxes: usize,
    source: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TypingBoxCheckError {
    ok: bool,
    command: &'static str,
    carrier: &'static str,
    typing_boxes: usize,
    error: String,
}

pub fn run_discord_typing_box_check_cli_from_env() -> Option<i32> {
    let result = run_discord_typing_box_check_cli(std::env::args_os())?;
    print!("{}", result.stdout);
    Some(result.exit_code)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordTypingBoxCheckResult {
    pub exit_code: i32,
    pub stdout: String,
}

pub fn run_discord_typing_box_check_cli<I>(args: I) -> Option<DiscordTypingBoxCheckResult>
where
    I: IntoIterator<Item = OsString>,
{
    if !args
        .into_iter()
        .any(|arg| arg == DISCORD_TYPING_BOX_CHECK_CLI_FLAG)
    {
        return None;
    }
    let (exit_code, stdout) = match platform::find_active_direct_message_typing_boxes() {
        Ok(conversation) => (
            0,
            json_line(&TypingBoxCheckOk {
                ok: true,
                command: "checkDiscordTypingBox",
                carrier: "Discord",
                conversation,
                typing_boxes: 1,
                source: "Win32.Discord.exe+UIA.Edit.Name",
            }),
        ),
        Err(error) => (
            1,
            json_line(&TypingBoxCheckError {
                ok: false,
                command: "checkDiscordTypingBox",
                carrier: "Discord",
                typing_boxes: 0,
                error,
            }),
        ),
    };
    Some(DiscordTypingBoxCheckResult { exit_code, stdout })
}

fn json_line<T: Serialize>(value: &T) -> String {
    format!(
        "{}\n",
        serde_json::to_string(value).expect("check JSON serializes")
    )
}

/// Returns the direct-message peer only for Discord's accessible composer
/// wording.  `#` is intentionally not accepted: it names a channel/group.
#[cfg(any(target_os = "windows", test))]
pub fn direct_message_from_composer_name(name: &str) -> Option<&str> {
    let peer = name.trim().strip_prefix("Message @")?.trim();
    (!peer.is_empty()
        && peer.len() <= 512
        && peer.trim() == peer
        && !peer.chars().any(char::is_control))
    .then_some(peer)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::direct_message_from_composer_name;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::core::VARIANT;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_ControlTypePropertyId,
        UIA_EditControlTypeId,
    };
    use windows_sys::Win32::Foundation::{BOOL, HWND as RawHwnd, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) fn find_active_direct_message_typing_boxes() -> Result<String, String> {
        let mut windows = Vec::new();
        unsafe {
            EnumWindows(
                Some(collect_discord_window),
                (&mut windows as *mut Vec<RawHwnd>) as LPARAM,
            )
        };
        let foreground = unsafe { GetForegroundWindow() };
        let window = windows
            .into_iter()
            .find(|hwnd| *hwnd == foreground)
            .ok_or_else(|| "Discord carrier window is unavailable or not foreground".to_owned())?;
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| "Discord carrier accessibility is unavailable".to_owned())?;
        let root = unsafe { automation.ElementFromHandle(HWND(window as _)) }
            .map_err(|_| "Discord carrier window surface is unavailable".to_owned())?;
        let condition = unsafe {
            automation.CreatePropertyCondition(
                UIA_ControlTypePropertyId,
                &VARIANT::from(UIA_EditControlTypeId.0),
            )
        }
        .map_err(|_| "Discord composer selector is unavailable".to_owned())?;
        let elements = unsafe { root.FindAll(TreeScope_Descendants, &condition) }
            .map_err(|_| "Discord composer surface is unavailable".to_owned())?;
        let mut peers = Vec::new();
        for index in 0..unsafe { elements.Length() }.unwrap_or(0) {
            let Ok(element) = (unsafe { elements.GetElement(index) }) else {
                continue;
            };
            if !unsafe { element.CurrentIsEnabled() }.unwrap_or(false)
                || unsafe { element.CurrentIsOffscreen() }.unwrap_or(true)
            {
                continue;
            }
            let Ok(name) = (unsafe { element.CurrentName() }) else {
                continue;
            };
            if let Some(peer) = direct_message_from_composer_name(&name.to_string()) {
                if !peers.iter().any(|known: &String| known == peer) {
                    peers.push(peer.to_owned());
                }
            }
        }
        match peers.as_slice() {
            [peer] => Ok(peer.clone()),
            [] => Err("Discord direct-message typing box is unavailable".to_owned()),
            _ => Err("Discord direct-message typing box is ambiguous".to_owned()),
        }
    }

    unsafe extern "system" fn collect_discord_window(hwnd: RawHwnd, parameter: LPARAM) -> BOOL {
        if unsafe { IsWindowVisible(hwnd) } == 0 || !is_discord_process(hwnd) {
            return 1;
        }
        unsafe { (*(parameter as *mut Vec<RawHwnd>)).push(hwnd) };
        1
    }

    fn is_discord_process(hwnd: RawHwnd) -> bool {
        let mut pid = 0u32;
        if unsafe { GetWindowThreadProcessId(hwnd, &mut pid) } == 0 || pid == 0 {
            return false;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process == 0 {
            return false;
        }
        let mut path = vec![0u16; 32_768];
        let mut length = path.len() as u32;
        let ok =
            unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) } != 0;
        unsafe { windows_sys::Win32::Foundation::CloseHandle(process) };
        ok && std::path::Path::new(&OsString::from_wide(&path[..length as usize]))
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("Discord.exe"))
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    pub(super) fn find_active_direct_message_typing_boxes() -> Result<String, String> {
        Err("Discord typing-box discovery is only available on Windows".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{direct_message_from_composer_name, run_discord_typing_box_check_cli};
    use serde_json::Value;
    use std::ffi::OsString;

    #[test]
    fn direct_message_composer_accepts_one_and_rejects_group_and_login_edits() {
        assert_eq!(
            direct_message_from_composer_name("Message @OSL QA Peer"),
            Some("OSL QA Peer")
        );
        assert_eq!(
            direct_message_from_composer_name("Message #OSL group"),
            None
        );
        assert_eq!(
            direct_message_from_composer_name("Email or Phone Number"),
            None
        );
    }

    #[test]
    fn direct_command_refuses_without_a_live_windows_discord_window() {
        let result = run_discord_typing_box_check_cli([
            OsString::from("osl-hub"),
            OsString::from("--check-discord-typing-box"),
        ])
        .expect("recognized");
        let value: Value = serde_json::from_str(result.stdout.trim()).expect("JSON");
        assert_eq!(value["command"], "checkDiscordTypingBox");
        assert_eq!(value["carrier"], "Discord");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(result.exit_code, 1);
    }
}
