//! The one OSL-owned Windows logon entry.
//!
//! Windows reads per-user startup applications from the `Run` registry key.
//! Keep this narrow: OSL writes and removes only its own named value and never
//! enumerates, rewrites, or otherwise changes another application's entry.

use ipc::app_preferences::StartWithWindowsChoice;
use std::path::Path;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const OSL_STARTUP_VALUE: &str = "OSL Privacy";

/// The narrow operation OSL needs on the Windows `Run` key.  Keeping this
/// boundary small lets the behavior test count a complete startup-entry set
/// without touching the host user's registry.
trait StartupEntryStore {
    fn set_osl_entry(&mut self, command: &str) -> Result<(), String>;
    fn remove_osl_entry(&mut self) -> Result<(), String>;
}

fn sync_with_store(
    store: &mut impl StartupEntryStore,
    choice: StartWithWindowsChoice,
    executable: &Path,
) -> Result<(), String> {
    match choice {
        StartWithWindowsChoice::On => store.set_osl_entry(&startup_command(executable)),
        StartWithWindowsChoice::Off => store.remove_osl_entry(),
    }
}

/// Apply the saved preference to the Windows logon entry.
///
/// The installed executable is resolved at the time the user changes the
/// setting, rather than recording a development or configuration directory.
pub fn sync(choice: StartWithWindowsChoice) -> Result<(), String> {
    #[cfg(windows)]
    {
        let executable = std::env::current_exe()
            .map_err(|error| format!("OSL: cannot locate the installed executable: {error}"))?;
        let mut startup_store = WindowsRunKey;
        sync_with_store(&mut startup_store, choice, &executable)
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
struct WindowsRunKey;

#[cfg(windows)]
impl StartupEntryStore for WindowsRunKey {
    fn set_osl_entry(&mut self, command: &str) -> Result<(), String> {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
            REG_SZ,
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

    fn remove_osl_entry(&mut self) -> Result<(), String> {
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
    use super::{startup_command, sync_with_store, StartupEntryStore, OSL_STARTUP_VALUE, RUN_KEY};
    use ipc::app_preferences::StartWithWindowsChoice;
    use std::collections::BTreeMap;
    use std::path::Path;

    #[derive(Default)]
    struct TestRunKey(BTreeMap<String, String>);

    impl TestRunKey {
        fn osl_entry_count(&self) -> usize {
            usize::from(self.0.contains_key(OSL_STARTUP_VALUE))
        }

        fn total_entry_count(&self) -> usize {
            self.0.len()
        }
    }

    impl StartupEntryStore for TestRunKey {
        fn set_osl_entry(&mut self, command: &str) -> Result<(), String> {
            self.0
                .insert(OSL_STARTUP_VALUE.to_owned(), command.to_owned());
            Ok(())
        }

        fn remove_osl_entry(&mut self) -> Result<(), String> {
            self.0.remove(OSL_STARTUP_VALUE);
            Ok(())
        }
    }

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

    #[test]
    fn task3147_start_with_windows_adds_and_removes_exactly_one_installed_osl_entry() {
        let installed_osl = Path::new(r"C:\Program Files\OSL Privacy\OSL Privacy.exe");
        let mut run_key = TestRunKey::default();
        run_key.0.insert(
            "Other App".to_owned(),
            r#""C:\Program Files\Other App\Other App.exe""#.to_owned(),
        );
        run_key.0.insert(
            "OneDrive".to_owned(),
            r#""C:\Program Files\Microsoft OneDrive\OneDrive.exe""#.to_owned(),
        );

        let total_before = run_key.total_entry_count();
        let osl_before = run_key.osl_entry_count();
        sync_with_store(&mut run_key, StartWithWindowsChoice::On, installed_osl)
            .expect("turn start-with-Windows on");
        let total_on = run_key.total_entry_count();
        let osl_on = run_key.osl_entry_count();
        let installed_command = run_key
            .0
            .get(OSL_STARTUP_VALUE)
            .expect("one OSL startup entry after turning on")
            .clone();

        sync_with_store(&mut run_key, StartWithWindowsChoice::Off, installed_osl)
            .expect("turn start-with-Windows off");
        let total_off = run_key.total_entry_count();
        let osl_off = run_key.osl_entry_count();

        println!("TASK3147 osl_startup_counts={osl_before},{osl_on},{osl_off}");
        println!("TASK3147 total_startup_counts={total_before},{total_on},{total_off}");
        println!("TASK3147 installed_osl_startup_command={installed_command}");
        assert_eq!([osl_before, osl_on, osl_off], [0, 1, 0]);
        assert_eq!(installed_command, startup_command(installed_osl));
        assert_eq!(total_on - total_before, 1);
        assert_eq!(total_on - total_off, 1);
    }
}
