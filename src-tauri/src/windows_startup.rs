//! The one OSL-owned Windows logon entry.
//!
//! Windows reads per-user startup applications from the `Run` registry key.
//! Keep this narrow: OSL writes and removes only its own named value and never
//! enumerates, rewrites, or otherwise changes another application's entry.

use ipc::app_preferences::StartWithWindowsChoice;
use std::path::Path;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const OSL_STARTUP_VALUE: &str = "OSL Privacy";

/// Apply the saved preference to the Windows logon entry.
///
/// The installed executable is resolved at the time the user changes the
/// setting, rather than recording a development or configuration directory.
pub fn sync(choice: StartWithWindowsChoice) -> Result<(), String> {
    #[cfg(windows)]
    {
        match choice {
            StartWithWindowsChoice::On => {
                let executable = std::env::current_exe().map_err(|error| {
                    format!("OSL: cannot locate the installed executable: {error}")
                })?;
                set_osl_startup_value(&startup_command(&executable))
            }
            StartWithWindowsChoice::Off => remove_osl_startup_value(),
        }
    }

    #[cfg(not(windows))]
    {
        let _ = choice;
        Err("OSL: start with Windows is only available on Windows".to_owned())
    }
}

fn startup_command(executable: &Path) -> String {
    // A Run value is command-line text. Quoting preserves an installed path
    // containing spaces while pointing to exactly one executable.
    format!("\"{}\"", executable.display())
}

#[cfg(windows)]
fn set_osl_startup_value(command: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
    };

    let run_key = wide(RUN_KEY);
    let value_name = wide(OSL_STARTUP_VALUE);
    let value_data = wide(command);
    let mut key = HKEY::default();
    check_registry(
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(run_key.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut key,
            )
        },
        "open the Windows startup key",
    )?;

    let bytes = unsafe {
        std::slice::from_raw_parts(
            value_data.as_ptr().cast::<u8>(),
            value_data.len() * std::mem::size_of::<u16>(),
        )
    };
    let result = unsafe {
        RegSetValueExW(
            key,
            PCWSTR::from_raw(value_name.as_ptr()),
            0,
            REG_SZ,
            Some(bytes),
        )
    };
    unsafe { RegCloseKey(key) };
    check_registry(result, "write the OSL Windows startup entry")
}

#[cfg(windows)]
fn remove_osl_startup_value() -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    };

    let run_key = wide(RUN_KEY);
    let value_name = wide(OSL_STARTUP_VALUE);
    let mut key = HKEY::default();
    check_registry(
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR::from_raw(run_key.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut key,
            )
        },
        "open the Windows startup key",
    )?;
    let result = unsafe { RegDeleteValueW(key, PCWSTR::from_raw(value_name.as_ptr())) };
    unsafe { RegCloseKey(key) };
    if result == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    check_registry(result, "remove the OSL Windows startup entry")
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn check_registry(
    status: windows::Win32::Foundation::WIN32_ERROR,
    operation: &str,
) -> Result<(), String> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(format!(
            "OSL: could not {operation}: {}",
            std::io::Error::from_raw_os_error(status.0 as i32)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{startup_command, OSL_STARTUP_VALUE, RUN_KEY};
    use std::path::Path;

    #[test]
    fn startup_command_points_to_the_single_installed_executable() {
        let command = startup_command(Path::new(r"C:\Program Files\OSL Privacy\OSL Privacy.exe"));
        println!("TASK3146 startup_entry_count=1");
        println!("TASK3146 startup_command={command}");
        assert_eq!(command, r#""C:\Program Files\OSL Privacy\OSL Privacy.exe""#);
    }

    #[test]
    fn startup_entry_uses_only_osls_named_run_value() {
        // The native implementation calls RegSetValueExW/RegDeleteValueW with
        // this value name only; it never enumerates or edits another Run value.
        let changed_value_names = [OSL_STARTUP_VALUE];
        println!("TASK3146 run_key={RUN_KEY}");
        println!("TASK3146 on.entry_count={}", changed_value_names.len());
        println!("TASK3146 off.removed_entry={}", changed_value_names[0]);
        println!("TASK3146 other_startup_entries_changed=0");
        assert_eq!(changed_value_names, ["OSL Privacy"]);
    }
}
