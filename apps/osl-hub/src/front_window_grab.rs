use serde::Serialize;
use std::ffi::OsString;

pub const FRONT_WINDOW_GRAB_CLI_FLAG: &str = "--front-window-grab";
pub const FRONT_WINDOW_ROUTE_CLI_FLAG: &str = "--front-window-route";
pub const FRONT_WINDOW_GRAB_FALLBACK_FAILURES: u8 = 2;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontWindowGrabJson {
    ok: bool,
    command: &'static str,
    window_name: String,
    window_number: u32,
    alt_press_count: u8,
    alt_release_count: u8,
    set_foreground_window_call_count: u8,
    set_foreground_window_returned: bool,
    readback_source: &'static str,
    requested_window: WindowInfoJson,
    foreground_after: WindowInfoJson,
    foreground_matched: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrontWindowGrabErrorJson {
    ok: bool,
    command: &'static str,
    window_name: String,
    window_number: u32,
    matching_window_count: usize,
    error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfoJson {
    hwnd: isize,
    pid: u32,
    process: Option<String>,
    title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontWindowGrabCommandResult {
    pub exit_code: i32,
    pub stdout: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontWindowDirectRoute {
    Grab,
    WaitForPerson,
}

impl FrontWindowDirectRoute {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grab => "grab",
            Self::WaitForPerson => "wait_for_person",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontWindowRouteRunRecord {
    pub command: &'static str,
    pub window_name: String,
    pub route: FrontWindowDirectRoute,
    pub grab_route_attempted: bool,
    pub grab_used: bool,
    pub wait_for_person_used: bool,
    pub grab_attempts: u8,
    pub consecutive_grab_failures: u8,
    pub grab_failures: Vec<String>,
    pub switch_written: bool,
    pub screen_message: Option<String>,
    pub silent_failures: u8,
}

pub fn run_front_window_grab_cli_from_env() -> Option<i32> {
    let result = run_front_window_grab_cli(std::env::args_os())?;
    print!("{}", result.stdout);
    Some(result.exit_code)
}

pub fn run_front_window_route_cli_from_env() -> Option<i32> {
    let result = run_front_window_route_cli(std::env::args_os())?;
    print!("{}", result.stdout);
    Some(result.exit_code)
}

pub fn run_front_window_grab_cli<I>(args: I) -> Option<FrontWindowGrabCommandResult>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let flag = args
        .iter()
        .position(|arg| arg == FRONT_WINDOW_GRAB_CLI_FLAG)?;
    let parsed = ParsedGrabArgs::parse(&args[flag + 1..]);
    let rendered = match parsed.and_then(run_front_window_grab_command) {
        Ok(value) => FrontWindowGrabCommandResult {
            exit_code: 0,
            stdout: json_line(&value),
        },
        Err(error) => FrontWindowGrabCommandResult {
            exit_code: 1,
            stdout: json_line(&error),
        },
    };
    Some(rendered)
}

pub fn run_front_window_route_cli<I>(args: I) -> Option<FrontWindowGrabCommandResult>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let flag = args
        .iter()
        .position(|arg| arg == FRONT_WINDOW_ROUTE_CLI_FLAG)?;
    let parsed = ParsedGrabArgs::parse(&args[flag + 1..]);
    let rendered = match parsed {
        Ok(parsed) => {
            let record = run_direct_front_window_route(&parsed.window_name, || {
                run_front_window_grab_command(parsed.clone())
                    .map(|_| ())
                    .map_err(|error| error.error)
            });
            FrontWindowGrabCommandResult {
                exit_code: 0,
                stdout: json_line(&record),
            }
        }
        Err(error) => FrontWindowGrabCommandResult {
            exit_code: 1,
            stdout: json_line(&error),
        },
    };
    Some(rendered)
}

pub fn run_direct_front_window_route<F>(
    window_name: &str,
    mut try_grab: F,
) -> FrontWindowRouteRunRecord
where
    F: FnMut() -> Result<(), String>,
{
    let mut grab_failures = Vec::new();
    for attempt in 1..=FRONT_WINDOW_GRAB_FALLBACK_FAILURES {
        match try_grab() {
            Ok(()) => {
                return FrontWindowRouteRunRecord {
                    command: "frontWindowRouteSwitch",
                    window_name: window_name.to_owned(),
                    route: FrontWindowDirectRoute::Grab,
                    grab_route_attempted: true,
                    grab_used: true,
                    wait_for_person_used: false,
                    grab_attempts: attempt,
                    consecutive_grab_failures: 0,
                    grab_failures,
                    switch_written: false,
                    screen_message: None,
                    silent_failures: 0,
                };
            }
            Err(error) => grab_failures.push(recorded_grab_failure(error)),
        }
    }

    let screen_message = front_window_wait_screen_message(window_name);
    FrontWindowRouteRunRecord {
        command: "frontWindowRouteSwitch",
        window_name: window_name.to_owned(),
        route: FrontWindowDirectRoute::WaitForPerson,
        grab_route_attempted: true,
        grab_used: false,
        wait_for_person_used: true,
        grab_attempts: FRONT_WINDOW_GRAB_FALLBACK_FAILURES,
        consecutive_grab_failures: FRONT_WINDOW_GRAB_FALLBACK_FAILURES,
        grab_failures,
        switch_written: true,
        screen_message: Some(screen_message),
        silent_failures: 0,
    }
}

pub fn front_window_wait_screen_message(window_name: &str) -> String {
    let window_name = if window_name.trim().is_empty() {
        "the requested window"
    } else {
        window_name.trim()
    };
    format!(
        "OSL is waiting for you to bring {window_name} forward because the quick window grab failed twice. This is slower, but it keeps the run visible instead of silently failing."
    )
}

fn recorded_grab_failure(error: String) -> String {
    let error = error.trim();
    if error.is_empty() {
        "front-window grab failed without a platform reason".to_owned()
    } else {
        error.to_owned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedGrabArgs {
    window_name: String,
    window_number: u32,
}

impl ParsedGrabArgs {
    fn parse(args: &[String]) -> Result<Self, FrontWindowGrabErrorJson> {
        let mut window_name = None;
        let mut window_number = None;
        let mut index = 0usize;
        while index < args.len() {
            let key = args.get(index).cloned().unwrap_or_default();
            let value = args.get(index + 1).cloned().ok_or_else(|| {
                front_window_grab_error("", 0, 0, format!("missing value for {key}"))
            })?;
            match key.as_str() {
                "--window-name" => window_name = Some(value),
                "--window-number" => {
                    window_number = Some(value.parse::<u32>().map_err(|_| {
                        front_window_grab_error("", 0, 0, "invalid --window-number".to_owned())
                    })?)
                }
                _ => {
                    return Err(front_window_grab_error(
                        "",
                        0,
                        0,
                        "usage: --front-window-grab --window-name <name> --window-number <number>"
                            .to_owned(),
                    ))
                }
            }
            index += 2;
        }
        let window_name = window_name
            .ok_or_else(|| front_window_grab_error("", 0, 0, "missing --window-name".to_owned()))?;
        let window_number = window_number.ok_or_else(|| {
            front_window_grab_error(&window_name, 0, 0, "missing --window-number".to_owned())
        })?;
        if window_name.trim().is_empty() {
            return Err(front_window_grab_error(
                "",
                window_number,
                0,
                "blank --window-name".to_owned(),
            ));
        }
        if window_number == 0 {
            return Err(front_window_grab_error(
                &window_name,
                window_number,
                0,
                "--window-number is one-based".to_owned(),
            ));
        }
        Ok(Self {
            window_name,
            window_number,
        })
    }
}

fn run_front_window_grab_command(
    args: ParsedGrabArgs,
) -> Result<FrontWindowGrabJson, FrontWindowGrabErrorJson> {
    platform::run(args)
}

fn front_window_grab_error(
    window_name: &str,
    window_number: u32,
    matching_window_count: usize,
    error: String,
) -> FrontWindowGrabErrorJson {
    FrontWindowGrabErrorJson {
        ok: false,
        command: "frontWindowGrab",
        window_name: window_name.to_owned(),
        window_number,
        matching_window_count,
        error,
    }
}

fn json_line<T: Serialize>(value: &T) -> String {
    format!(
        "{}\n",
        serde_json::to_string(value).expect("front-window-grab JSON response serializes")
    )
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{
        front_window_grab_error, FrontWindowGrabErrorJson, FrontWindowGrabJson, ParsedGrabArgs,
        WindowInfoJson,
    };
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE, HWND, LPARAM};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_MENU,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible, SetForegroundWindow,
    };

    pub(super) fn run(
        args: ParsedGrabArgs,
    ) -> Result<FrontWindowGrabJson, FrontWindowGrabErrorJson> {
        let matches = enumerate_named_windows(&args.window_name);
        let Some(requested) = matches
            .get(args.window_number.saturating_sub(1) as usize)
            .cloned()
        else {
            return Err(front_window_grab_error(
                &args.window_name,
                args.window_number,
                matches.len(),
                format!(
                    "{} window #{} was not found",
                    args.window_name, args.window_number
                ),
            ));
        };

        let alt_accepted = send_single_alt_press_and_release();
        let hwnd = requested.hwnd as HWND;
        let set_foreground_window_returned =
            !hwnd.is_null() && unsafe { SetForegroundWindow(hwnd) } != 0;
        std::thread::sleep(std::time::Duration::from_millis(250));
        let foreground_after = window_info(unsafe { GetForegroundWindow() });
        let foreground_matched = foreground_after.hwnd == requested.hwnd;

        let value = FrontWindowGrabJson {
            ok: foreground_matched,
            command: "frontWindowGrab",
            window_name: args.window_name,
            window_number: args.window_number,
            alt_press_count: u8::from(alt_accepted),
            alt_release_count: u8::from(alt_accepted),
            set_foreground_window_call_count: 1,
            set_foreground_window_returned,
            readback_source: "GetForegroundWindow",
            requested_window: requested,
            foreground_after,
            foreground_matched,
        };
        if value.foreground_matched {
            Ok(value)
        } else {
            Err(front_window_grab_error(
                &value.window_name,
                value.window_number,
                matches.len(),
                format!(
                    "{} window #{} did not become the foreground window",
                    value.window_name, value.window_number
                ),
            ))
        }
    }

    fn enumerate_named_windows(name: &str) -> Vec<WindowInfoJson> {
        let mut windows = Vec::<WindowInfoJson>::new();
        unsafe {
            EnumWindows(
                Some(enum_windows_proc),
                (&mut windows as *mut Vec<WindowInfoJson>) as LPARAM,
            );
        }
        let process_matches = windows
            .iter()
            .filter(|window| {
                window
                    .process
                    .as_deref()
                    .is_some_and(|process| same_window_name(process, name))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !process_matches.is_empty() {
            return process_matches;
        }
        windows
            .into_iter()
            .filter(|window| contains_window_name(&window.title, name))
            .collect()
    }

    unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd) != 0 {
            let info = window_info(hwnd);
            if !info.title.trim().is_empty() || info.process.is_some() {
                let windows = &mut *(lparam as *mut Vec<WindowInfoJson>);
                windows.push(info);
            }
        }
        1
    }

    fn same_window_name(left: &str, right: &str) -> bool {
        normalize_window_name(left) == normalize_window_name(right)
    }

    fn contains_window_name(haystack: &str, needle: &str) -> bool {
        haystack
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
    }

    fn normalize_window_name(value: &str) -> String {
        value
            .trim()
            .trim_end_matches(".exe")
            .trim_end_matches(".EXE")
            .to_ascii_lowercase()
    }

    fn send_single_alt_press_and_release() -> bool {
        let inputs = [alt_keyboard_input(0), alt_keyboard_input(KEYEVENTF_KEYUP)];
        (unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        }) == inputs.len() as u32
    }

    fn alt_keyboard_input(flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU as u16,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn window_info(hwnd: HWND) -> WindowInfoJson {
        let mut pid = 0u32;
        if !hwnd.is_null() {
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        }
        WindowInfoJson {
            hwnd: hwnd as isize,
            pid,
            process: process_name(pid),
            title: window_title(hwnd),
        }
    }

    fn window_title(hwnd: HWND) -> String {
        if hwnd.is_null() {
            return String::new();
        }
        let mut buffer = vec![0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        if len <= 0 {
            return String::new();
        }
        buffer.truncate(len as usize);
        OsString::from_wide(&buffer).to_string_lossy().into_owned()
    }

    fn process_name(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let result = process_name_from_handle(handle);
        unsafe { CloseHandle(handle) };
        result
    }

    fn process_name_from_handle(handle: HANDLE) -> Option<String> {
        let mut buffer = vec![0u16; 32_768];
        let mut length = buffer.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) } == 0
            || length == 0
        {
            return None;
        }
        buffer.truncate(length as usize);
        let path = OsString::from_wide(&buffer);
        Path::new(&path)
            .file_stem()
            .map(|stem| stem.to_string_lossy().trim_end_matches(".exe").to_owned())
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::{
        front_window_grab_error, FrontWindowGrabErrorJson, FrontWindowGrabJson, ParsedGrabArgs,
    };

    pub(super) fn run(
        args: ParsedGrabArgs,
    ) -> Result<FrontWindowGrabJson, FrontWindowGrabErrorJson> {
        Err(front_window_grab_error(
            &args.window_name,
            args.window_number,
            0,
            format!(
                "{} window #{} cannot be grabbed: front-window grab is only available on Windows",
                args.window_name, args.window_number
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        run_direct_front_window_route, run_front_window_grab_cli, run_front_window_route_cli,
        FrontWindowDirectRoute,
    };
    use serde_json::Value;
    use std::ffi::OsString;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    fn json(stdout: &str) -> Value {
        serde_json::from_str(stdout.trim_end()).expect("stdout is one JSON object")
    }

    #[test]
    fn front_window_grab_cli_is_a_direct_numbered_named_window_command() {
        let result = run_front_window_grab_cli(args(&[
            "osl-privacy-hub",
            "--front-window-grab",
            "--window-name",
            "Discord",
            "--window-number",
            "9999",
        ]))
        .expect("front-window command recognized");

        assert_eq!(result.exit_code, 1);
        let value = json(&result.stdout);
        assert_eq!(value["command"], "frontWindowGrab");
        assert_eq!(value["ok"], false);
        assert_eq!(value["windowName"], "Discord");
        assert_eq!(value["windowNumber"], 9999);
        assert!(
            value["error"].as_str().unwrap().contains("Discord"),
            "failure must name the requested window"
        );
    }

    #[test]
    fn front_window_grab_cli_is_ignored_without_its_flag() {
        assert!(run_front_window_grab_cli(args(&["osl-privacy-hub"])).is_none());
    }

    #[test]
    fn task_3412_direct_command_with_working_grab_uses_grab_route() {
        let mut grab_calls = 0u8;
        let record = run_direct_front_window_route("Discord", || {
            grab_calls += 1;
            Ok(())
        });
        let json_record =
            serde_json::to_string(&record).expect("task 3412 working run record serializes");

        println!("task_3412_direct_command=front_window_route");
        println!("task_3412_working_route={}", record.route.as_str());
        println!("task_3412_working_grab_calls={grab_calls}");
        println!(
            "task_3412_working_silent_failure_count={}",
            record.silent_failures
        );
        println!("task_3412_working_run_record={json_record}");

        assert_eq!(record.route, FrontWindowDirectRoute::Grab);
        assert!(record.grab_route_attempted);
        assert!(record.grab_used);
        assert!(!record.wait_for_person_used);
        assert_eq!(record.grab_attempts, 1);
        assert_eq!(grab_calls, 1);
        assert!(!record.switch_written);
        assert_eq!(record.screen_message, None);
        assert_eq!(record.silent_failures, 0);
    }

    #[test]
    fn task_3412_grab_fails_twice_switches_to_waiting_with_visible_reason() {
        let mut grab_calls = 0u8;
        let record = run_direct_front_window_route("Discord", || {
            grab_calls += 1;
            Err(format!("forced grab failure {grab_calls}"))
        });
        let value = serde_json::to_value(&record).expect("task 3412 fallback run record value");
        let json_record =
            serde_json::to_string(&record).expect("task 3412 fallback run record serializes");
        let screen_message = record
            .screen_message
            .as_deref()
            .expect("fallback tells the person why waiting is slower");

        println!("task_3412_forced_failure_route={}", record.route.as_str());
        println!("task_3412_forced_failure_grab_calls={grab_calls}");
        println!(
            "task_3412_forced_failure_grab_attempts={}",
            record.grab_attempts
        );
        println!("task_3412_switch_written={}", record.switch_written);
        println!("task_3412_screen_message={screen_message}");
        println!("task_3412_silent_failure_count={}", record.silent_failures);
        println!("task_3412_forced_failure_run_record={json_record}");

        assert_eq!(record.route, FrontWindowDirectRoute::WaitForPerson);
        assert!(record.grab_route_attempted);
        assert!(!record.grab_used);
        assert!(record.wait_for_person_used);
        assert_eq!(record.grab_attempts, 2);
        assert_eq!(grab_calls, 2);
        assert_eq!(record.consecutive_grab_failures, 2);
        assert_eq!(record.grab_failures.len(), 2);
        assert!(record.switch_written);
        assert_eq!(value["switchWritten"], true);
        assert!(screen_message.contains("failed twice"));
        assert!(screen_message.contains("slower"));
        assert_eq!(record.silent_failures, 0);
    }

    #[test]
    fn task_3412_front_window_route_cli_writes_wait_record_after_two_platform_grab_failures() {
        let result = run_front_window_route_cli(args(&[
            "osl-privacy-hub",
            "--front-window-route",
            "--window-name",
            "Discord",
            "--window-number",
            "9999",
        ]))
        .expect("front-window route command recognized");

        assert_eq!(result.exit_code, 0);
        println!(
            "task_3412_cli_forced_failure_run_record={}",
            result.stdout.trim_end()
        );
        let value = json(&result.stdout);
        assert_eq!(value["command"], "frontWindowRouteSwitch");
        assert_eq!(value["route"], "wait_for_person");
        assert_eq!(value["grabAttempts"], 2);
        assert_eq!(value["switchWritten"], true);
        assert_eq!(value["silentFailures"], 0);
        assert!(value["screenMessage"]
            .as_str()
            .expect("screen message")
            .contains("failed twice"));
    }
}
