#[cfg(target_os = "windows")]
mod windows_front_window {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, TRUE};
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow, ShowWindow, GA_ROOT,
        SW_RESTORE,
    };

    const DEFAULT_SETTLE_MS: u64 = 120;

    struct Candidate {
        hwnd: HWND,
    }

    struct Search {
        wanted: String,
        found: Vec<Candidate>,
    }

    pub fn run() -> Result<bool, String> {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        if args.len() < 2 || args.len() > 3 {
            return Err(
                "usage: task_3404_front_window_grab <grab|check> <process-or-title> [settle-ms]"
                    .to_owned(),
            );
        }
        let settle_ms = args
            .get(2)
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| "settle-ms must be a positive integer".to_owned())
            })
            .transpose()?
            .unwrap_or(DEFAULT_SETTLE_MS);
        let candidate = find_window(&args[1])
            .ok_or_else(|| format!("no visible top-level window matched {}", args[1]))?;
        match args[0].as_str() {
            "grab" => {
                request_front_window(candidate.hwnd);
                std::thread::sleep(Duration::from_millis(settle_ms));
                Ok(front_window_is(candidate.hwnd))
            }
            "check" => Ok(front_window_is(candidate.hwnd)),
            _ => Err("first argument must be grab or check".to_owned()),
        }
    }

    fn find_window(wanted: &str) -> Option<Candidate> {
        let mut search = Search {
            wanted: normalize(wanted.trim_end_matches(".exe")),
            found: Vec::new(),
        };
        unsafe {
            EnumWindows(
                Some(collect_matching_windows),
                (&mut search as *mut Search) as LPARAM,
            );
        }
        search.found.into_iter().next()
    }

    unsafe extern "system" fn collect_matching_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if hwnd.is_null() || unsafe { IsWindowVisible(hwnd) } == 0 {
            return TRUE;
        }
        let search = unsafe { &mut *(lparam as *mut Search) };
        let title = window_title(hwnd);
        let process_name = window_process_name(hwnd).unwrap_or_default();
        if normalize(process_name.trim_end_matches(".exe")) == search.wanted
            || normalize(&title).contains(&search.wanted)
        {
            search.found.push(Candidate { hwnd });
        }
        TRUE
    }

    struct InputQueueAttachment {
        foreground_thread: u32,
        this_thread: u32,
    }

    impl InputQueueAttachment {
        fn acquire() -> Option<Self> {
            let foreground = unsafe { GetForegroundWindow() };
            if foreground.is_null() {
                return None;
            }
            let mut process_id = 0u32;
            let foreground_thread =
                unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
            let this_thread = unsafe { GetCurrentThreadId() };
            if foreground_thread == 0 || foreground_thread == this_thread {
                return None;
            }
            (unsafe { AttachThreadInput(foreground_thread, this_thread, 1) } != 0).then_some(Self {
                foreground_thread,
                this_thread,
            })
        }
    }

    impl Drop for InputQueueAttachment {
        fn drop(&mut self) {
            unsafe { AttachThreadInput(self.foreground_thread, self.this_thread, 0) };
        }
    }

    fn request_front_window(hwnd: HWND) {
        unsafe { ShowWindow(hwnd, SW_RESTORE) };
        unsafe { SetForegroundWindow(hwnd) };
        if !front_window_is(hwnd) {
            let _attachment = InputQueueAttachment::acquire();
            unsafe { SetForegroundWindow(hwnd) };
        }
    }

    fn front_window_is(hwnd: HWND) -> bool {
        if hwnd.is_null() {
            return false;
        }
        let wanted_root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        let foreground = unsafe { GetForegroundWindow() };
        if wanted_root.is_null() || foreground.is_null() {
            return false;
        }
        (unsafe { GetAncestor(foreground, GA_ROOT) }) == wanted_root
    }

    fn window_title(hwnd: HWND) -> String {
        let len = unsafe { GetWindowTextLengthW(hwnd) };
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let read = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        OsString::from_wide(&buf[..read.max(0) as usize])
            .to_string_lossy()
            .into_owned()
    }

    fn window_process_name(hwnd: HWND) -> Option<String> {
        let mut pid = 0u32;
        if unsafe { GetWindowThreadProcessId(hwnd, &mut pid) } == 0 || pid == 0 {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 32_768];
        let mut len = buf.len() as u32;
        let ok = unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) };
        unsafe { CloseHandle(process) };
        if ok == 0 || len == 0 {
            return None;
        }
        let path = OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .into_owned();
        path.rsplit(['\\', '/']).next().map(str::to_owned)
    }

    fn normalize(value: &str) -> String {
        value.to_ascii_lowercase()
    }
}

#[cfg(target_os = "windows")]
fn main() {
    match windows_front_window::run() {
        Ok(true) => println!("got-it"),
        Ok(false) => {
            println!("did-not-get-it");
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("task_3404_front_window_grab requires Windows");
    std::process::exit(2);
}
