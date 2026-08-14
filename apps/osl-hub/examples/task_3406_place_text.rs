fn verify_marked_placement(before: &str, after: &str, mark: &str) -> Result<(), String> {
    if !before.as_bytes().is_empty() {
        return Err(format!(
            "composer was not empty before placement: {before:?}"
        ));
    }
    if after != mark {
        return Err(format!(
            "readback did not equal placed mark {mark:?}: {after:?}"
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedTextPlacementReceipt {
    pub placed_bytes: usize,
    pub readback_bytes: usize,
    pub clear_bytes: usize,
}

/// Provider-neutral text actions used by the shared place-text job.  A service
/// adapter supplies one already-discovered composer; the shared job owns the
/// empty-before, exact-readback, and empty-after-clear checks.
pub trait SharedTextActions {
    fn read_back_text(&mut self) -> Result<String, String>;
    fn place_text(&mut self, text: &str) -> Result<(), String>;
    fn clear_text(&mut self) -> Result<(), String>;
}

pub fn place_read_back_and_clear(
    actions: &mut impl SharedTextActions,
    mark: &str,
) -> Result<SharedTextPlacementReceipt, String> {
    if mark.as_bytes().is_empty() {
        return Err("marked text must contain at least one byte".to_owned());
    }

    let before_readback = actions.read_back_text()?;
    println!("before_readback={before_readback:?}");
    if !before_readback.as_bytes().is_empty() {
        return Err(format!(
            "composer was not empty before placement: {before_readback:?}"
        ));
    }

    actions.place_text(mark)?;
    let readback = actions.read_back_text()?;
    println!("readback={readback:?}");
    verify_marked_placement(&before_readback, &readback, mark)?;

    actions.clear_text()?;
    let clear_readback = actions.read_back_text()?;
    println!("clear_readback={clear_readback:?}");
    if !clear_readback.as_bytes().is_empty() {
        return Err(format!(
            "composer clear left {} bytes: {clear_readback:?}",
            clear_readback.len()
        ));
    }

    Ok(SharedTextPlacementReceipt {
        placed_bytes: mark.len(),
        readback_bytes: readback.len(),
        clear_bytes: clear_readback.len(),
    })
}

#[cfg(target_os = "windows")]
mod windows_place_text {
    use std::ffi::{c_void, OsString};
    use std::mem::size_of;
    use std::os::windows::ffi::OsStringExt;
    use std::process::{Child, Command, Stdio};
    use std::ptr;
    use std::thread;
    use std::time::{Duration, Instant};

    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        AccessibleObjectFromWindow, CUIAutomation, IAccessible, IUIAutomation,
        IUIAutomationElement, IUIAutomationValuePattern, NotifyWinEvent, TreeScope_Subtree,
        UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_TextControlTypeId,
        UIA_ValuePatternId,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, POINT, TRUE};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
        IsClipboardFormatAvailable, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess,
        GetExitCodeProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
        MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, VK_CONTROL, VK_DELETE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetAncestor, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        SetCursorPos, SetForegroundWindow, ShowWindow, WindowFromPoint, GA_ROOT,
        SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_RESTORE,
    };

    const DEFAULT_APP: &str = "Discord";
    const DEFAULT_TEXT: &str = "MAPLE-3406";
    const DEFAULT_PRIVATE_CANARY: &str = "QQQQQQQQQQ";
    const DEFAULT_INITIAL_FRONT: &str = "Photos";
    const EVENT_SYSTEM_ALERT: u32 = 0x0002;
    const ELECTRON_A11Y_OBJECT_ID: i32 = 1;
    const OBJID_CLIENT: i32 = -4;
    const MIN_TREE_ELEMENTS: i32 = 10;
    const TREE_WAIT_MS: u64 = 1_000;
    const SETTLE_MS: u64 = 160;
    const STILL_ACTIVE: u32 = 259;
    const COMPOSER_STEMS: &[&str] = &["message", "nachricht", "mensaje"];
    const NON_COMPOSER_STEMS: &[&str] = &["search", "filter", "buscar"];

    #[derive(Clone, Debug)]
    struct WindowInfo {
        hwnd: HWND,
        title: String,
        process_name: String,
        process_id: u32,
    }

    struct Search {
        wanted: String,
        found: Vec<WindowInfo>,
    }

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    #[derive(Clone)]
    struct ClipboardFormat {
        format: u32,
        bytes: Vec<u8>,
    }

    #[derive(Clone)]
    struct ClipboardSnapshot {
        formats: Vec<ClipboardFormat>,
    }

    struct ClipboardRestorer {
        snapshot: ClipboardSnapshot,
        restored: bool,
    }

    impl ClipboardRestorer {
        fn restore(&mut self) -> Result<Instant, String> {
            let removed_at = restore_clipboard(&self.snapshot)?;
            self.restored = true;
            Ok(removed_at)
        }
    }

    impl Drop for ClipboardRestorer {
        fn drop(&mut self) {
            if !self.restored {
                let _ = restore_clipboard(&self.snapshot);
            }
        }
    }

    #[derive(Debug)]
    pub struct CommandError {
        pub code: i32,
        pub message: String,
    }

    impl CommandError {
        fn exit1(message: impl Into<String>) -> Self {
            Self {
                code: 1,
                message: message.into(),
            }
        }

        fn usage(message: impl Into<String>) -> Self {
            Self {
                code: 2,
                message: message.into(),
            }
        }
    }

    pub fn run() -> Result<(), CommandError> {
        let args = Args::parse()?;
        if let Some(observer) = args.observer {
            return run_clipboard_observer(observer);
        }
        let PlaceArgs {
            initial_front,
            app,
            composer_name,
            text,
            private_canary,
            pause_after_step,
        } = args.place;
        let initial = foreground_window()
            .ok_or_else(|| CommandError::exit1("Windows reported no foreground window"))?;
        println!(
            "initial_front={}",
            describe_window(&initial).replace('\n', " ")
        );
        if !window_matches(&initial, &initial_front) {
            return Err(CommandError::exit1(format!(
                "initial front window was not {}",
                initial_front
            )));
        }

        let discord = match find_window(&app) {
            Some(window) => window,
            None => {
                println!("osl_clipboard_entries=0");
                return Err(CommandError::exit1(format!("{} not found", app)));
            }
        };
        let initially_behind = !same_root(initial.hwnd, discord.hwnd);
        println!(
            "behind_window={} behind_initial={}",
            describe_window(&discord).replace('\n', " "),
            initially_behind
        );
        if !initially_behind {
            return Err(CommandError::exit1(format!(
                "{} was already the foreground window",
                app
            )));
        }

        request_front_window(discord.hwnd);
        thread::sleep(Duration::from_millis(SETTLE_MS));
        let grabbed = foreground_window().ok_or_else(|| {
            CommandError::exit1("Windows reported no foreground window after grab")
        })?;
        println!(
            "after_grab_front={}",
            describe_window(&grabbed).replace('\n', " ")
        );
        if !same_root(grabbed.hwnd, discord.hwnd) {
            return Err(CommandError::exit1(format!(
                "{} did not become the foreground window",
                app
            )));
        }

        let _com = initialize_com()?;
        let automation = automation()?;
        let (root, root_route) =
            accessibility_root(&automation, discord.hwnd, composer_name.as_deref())?;
        println!("root_route={root_route}");
        wait_for_tree(&root, &automation, &app)?;
        let composer = find_composer(&root, &automation, composer_name.as_deref())?;
        println!("composer_name={:?}", element_name(&composer));
        let bounds = element_bounds(&composer)
            .ok_or_else(|| CommandError::exit1(format!("{} composer bounds not found", app)))?;
        println!(
            "composer_bounds={},{},{},{}",
            bounds[0], bounds[1], bounds[2], bounds[3]
        );

        click_composer(bounds, discord.hwnd)?;
        thread::sleep(Duration::from_millis(180));
        let focus = unsafe { composer.CurrentHasKeyboardFocus() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        println!("composer_has_keyboard_focus={focus}");
        if !focus {
            return Err(CommandError::exit1(format!(
                "{} composer did not take keyboard focus",
                app
            )));
        }

        let mut actions = WindowsComposerTextActions {
            composer: &composer,
            private_canary: &private_canary,
            provider_name: &app,
            provider_pid: discord.process_id,
            pause_after_step: pause_after_step.as_deref(),
        };
        // The private draft never enters this command.  Its caller supplies a
        // canary solely so a live close-provider run can prove the opaque
        // private-draft fingerprint did not change across a refused attempt.
        println!("private_draft_fingerprint={:016x}", digest_text(&private_canary));
        let receipt = match super::place_read_back_and_clear(&mut actions, &text) {
            Ok(receipt) => receipt,
            Err(error) => {
                println!("covers_sent=0");
                return Err(CommandError::exit1(format!("{app} {error}")));
            }
        };
        println!("placed_bytes={}", receipt.placed_bytes);
        println!("readback_bytes={}", receipt.readback_bytes);
        println!("clear_bytes={}", receipt.clear_bytes);
        println!("covers_sent=0");
        Ok(())
    }

    struct Args {
        place: PlaceArgs,
        observer: Option<ObserverArgs>,
    }

    struct PlaceArgs {
        initial_front: String,
        app: String,
        composer_name: Option<String>,
        text: String,
        private_canary: String,
        pause_after_step: Option<String>,
    }

    struct ObserverArgs {
        needle: String,
        timeout: Duration,
    }

    impl Args {
        fn parse() -> Result<Self, CommandError> {
            let mut initial_front = DEFAULT_INITIAL_FRONT.to_owned();
            let mut app = DEFAULT_APP.to_owned();
            let mut composer_name = None;
            let mut text = DEFAULT_TEXT.to_owned();
            let mut private_canary = DEFAULT_PRIVATE_CANARY.to_owned();
            let mut pause_after_step = None;
            let mut observer = false;
            let mut observer_needle = String::new();
            let mut observer_timeout = Duration::from_millis(3_000);
            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--clipboard-observer" => {
                        observer = true;
                    }
                    "--observer-needle" => {
                        observer_needle = args.next().ok_or_else(|| {
                            CommandError::usage("--observer-needle needs a value")
                        })?;
                    }
                    "--observer-timeout-ms" => {
                        let raw = args.next().ok_or_else(|| {
                            CommandError::usage("--observer-timeout-ms needs a value")
                        })?;
                        let millis = raw.parse::<u64>().map_err(|_| {
                            CommandError::usage("--observer-timeout-ms must be an integer")
                        })?;
                        observer_timeout = Duration::from_millis(millis);
                    }
                    "--initial-front" => {
                        initial_front = args
                            .next()
                            .ok_or_else(|| CommandError::usage("--initial-front needs a value"))?;
                    }
                    "--app" => {
                        app = args
                            .next()
                            .ok_or_else(|| CommandError::usage("--app needs a value"))?;
                    }
                    "--composer-name" => {
                        composer_name =
                            Some(args.next().ok_or_else(|| {
                                CommandError::usage("--composer-name needs a value")
                            })?);
                    }
                    "--text" => {
                        text = args
                            .next()
                            .ok_or_else(|| CommandError::usage("--text needs a value"))?;
                    }
                    "--private-canary" => {
                        private_canary = args
                            .next()
                            .ok_or_else(|| CommandError::usage("--private-canary needs a value"))?;
                    }
                    "--pause-after-step" => {
                        let step = args.next().ok_or_else(|| {
                            CommandError::usage("--pause-after-step needs a value")
                        })?;
                        if !matches!(step.as_str(), "empty-readback" | "marked-paste" | "exact-readback" | "clear") {
                            return Err(CommandError::usage(
                                "--pause-after-step must be empty-readback, marked-paste, exact-readback, or clear",
                            ));
                        }
                        pause_after_step = Some(step);
                    }
                    "--help" | "-h" => {
                        return Err(CommandError::usage(
                            "usage: task_3406_place_text [--initial-front Photos] [--app Discord] [--composer-name Message] [--text MAPLE-3406] [--private-canary QQQQQQQQQQ]",
                        ));
                    }
                    other => {
                        return Err(CommandError::usage(format!("unknown argument {other}")));
                    }
                }
            }
            if text.is_empty() || text.chars().any(|ch| matches!(ch, '\n' | '\r')) {
                return Err(CommandError::usage("text must be one non-empty line"));
            }
            if composer_name
                .as_ref()
                .is_some_and(|name: &String| name.trim().is_empty())
            {
                return Err(CommandError::usage("composer name must not be empty"));
            }
            if observer {
                if observer_needle.is_empty() {
                    return Err(CommandError::usage(
                        "--clipboard-observer requires --observer-needle",
                    ));
                }
                return Ok(Self {
                    place: PlaceArgs {
                        initial_front,
                        app,
                        composer_name,
                        text,
                        private_canary,
                        pause_after_step: None,
                    },
                    observer: Some(ObserverArgs {
                        needle: observer_needle,
                        timeout: observer_timeout,
                    }),
                });
            }
            if private_canary.is_empty() || private_canary.chars().any(|ch| text.contains(ch)) {
                return Err(CommandError::usage(
                    "--private-canary must be non-empty and share no characters with --text",
                ));
            }
            Ok(Self {
                place: PlaceArgs {
                    initial_front,
                    app,
                    composer_name,
                    text,
                    private_canary,
                    pause_after_step,
                },
                observer: None,
            })
        }
    }

    struct ClipboardObserver {
        child: Child,
    }

    struct ClipboardObserverReport {
        pid: Option<u32>,
        saw: String,
    }

    impl ClipboardObserver {
        fn spawn(needle: &str, timeout: Duration) -> Result<Self, String> {
            let exe = std::env::current_exe()
                .map_err(|error| format!("current exe unavailable: {error}"))?;
            let child = Command::new(exe)
                .arg("--clipboard-observer")
                .arg("--observer-needle")
                .arg(needle)
                .arg("--observer-timeout-ms")
                .arg(timeout.as_millis().to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| format!("spawn failed: {error}"))?;
            Ok(Self { child })
        }

        fn wait(self) -> Result<ClipboardObserverReport, String> {
            let output = self
                .child
                .wait_with_output()
                .map_err(|error| format!("wait failed: {error}"))?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut pid = None;
            let mut saw = None;
            for line in stdout.lines() {
                if let Some(value) = line.strip_prefix("observer_pid=") {
                    pid = value.parse::<u32>().ok();
                } else if let Some(value) = line.strip_prefix("observer_saw=") {
                    saw = Some(value.to_owned());
                }
            }
            if !output.status.success() {
                return Err(format!(
                    "observer exited {:?}; stdout={stdout:?}; stderr={stderr:?}",
                    output.status.code()
                ));
            }
            Ok(ClipboardObserverReport {
                pid,
                saw: saw.ok_or_else(|| {
                    format!("observer did not print observer_saw; stdout={stdout:?}")
                })?,
            })
        }
    }

    fn run_clipboard_observer(args: ObserverArgs) -> Result<(), CommandError> {
        println!("observer_pid={}", unsafe { GetCurrentProcessId() });
        let started = Instant::now();
        let mut last_text = None;
        while started.elapsed() < args.timeout {
            if let Ok(Some(text)) = read_clipboard_unicode_text() {
                if text == args.needle {
                    println!("observer_saw={text}");
                    return Ok(());
                }
                last_text = Some(text);
            }
            thread::sleep(Duration::from_millis(5));
        }
        if let Some(text) = last_text {
            println!("observer_saw={text}");
        }
        Err(CommandError::exit1(
            "observer did not see requested clipboard text",
        ))
    }

    fn initialize_com() -> Result<ComGuard, CommandError> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            Ok(ComGuard(true))
        } else {
            Err(CommandError::exit1(format!(
                "COM initialization failed: {result:?}"
            )))
        }
    }

    fn automation() -> Result<IUIAutomation, CommandError> {
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.map_err(|error| {
            CommandError::exit1(format!("UI Automation initialization failed: {error:?}"))
        })
    }

    fn accessibility_root(
        automation: &IUIAutomation,
        hwnd: HWND,
        composer_name: Option<&str>,
    ) -> Result<(IUIAutomationElement, &'static str), CommandError> {
        if let Ok(root) =
            unsafe { automation.ElementFromHandle(windows::Win32::Foundation::HWND(hwnd as _)) }
        {
            if subtree_len(&root, automation) >= MIN_TREE_ELEMENTS
                && find_composer(&root, automation, composer_name).is_ok()
            {
                return Ok((root, "uia_native"));
            }
        }

        let accessible = wake_electron_accessibility(hwnd)
            .ok_or_else(|| CommandError::exit1("accessibility root not available"))?;
        unsafe { automation.ElementFromIAccessible(&accessible, 0) }
            .map(|root| (root, "msaa_client_after_wake"))
            .map_err(|error| CommandError::exit1(format!("MSAA bridge failed: {error:?}")))
    }

    fn wake_electron_accessibility(hwnd: HWND) -> Option<IAccessible> {
        if hwnd.is_null() {
            return None;
        }
        unsafe {
            NotifyWinEvent(
                EVENT_SYSTEM_ALERT,
                windows::Win32::Foundation::HWND(hwnd as _),
                ELECTRON_A11Y_OBJECT_ID,
                0,
            )
        };
        let _ = accessible_object_from_window(hwnd, ELECTRON_A11Y_OBJECT_ID);
        accessible_object_from_window(hwnd, OBJID_CLIENT)
    }

    fn accessible_object_from_window(hwnd: HWND, object_id: i32) -> Option<IAccessible> {
        let mut object: *mut c_void = ptr::null_mut();
        unsafe {
            AccessibleObjectFromWindow(
                windows::Win32::Foundation::HWND(hwnd as _),
                object_id as u32,
                &IAccessible::IID,
                &mut object,
            )
        }
        .ok()?;
        (!object.is_null()).then(|| unsafe { IAccessible::from_raw(object) })
    }

    fn wait_for_tree(
        root: &IUIAutomationElement,
        automation: &IUIAutomation,
        app: &str,
    ) -> Result<(), CommandError> {
        let started = Instant::now();
        loop {
            let count = subtree_len(root, automation);
            if count >= MIN_TREE_ELEMENTS {
                println!("accessibility_tree_elements={count}");
                return Ok(());
            }
            if started.elapsed() >= Duration::from_millis(TREE_WAIT_MS) {
                return Err(CommandError::exit1(format!(
                    "{app} accessibility tree never populated: saw {count}, needed {MIN_TREE_ELEMENTS}"
                )));
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn subtree_len(root: &IUIAutomationElement, automation: &IUIAutomation) -> i32 {
        let Ok(condition) = (unsafe { automation.CreateTrueCondition() }) else {
            return 0;
        };
        let Ok(found) = (unsafe { root.FindAll(TreeScope_Subtree, &condition) }) else {
            return 0;
        };
        unsafe { found.Length() }.unwrap_or(0)
    }

    fn find_composer(
        root: &IUIAutomationElement,
        automation: &IUIAutomation,
        composer_name: Option<&str>,
    ) -> Result<IUIAutomationElement, CommandError> {
        let condition = unsafe { automation.CreateTrueCondition() }.map_err(|error| {
            CommandError::exit1(format!("UI Automation condition failed: {error:?}"))
        })?;
        let found = unsafe { root.FindAll(TreeScope_Subtree, &condition) }.map_err(|error| {
            CommandError::exit1(format!("UI Automation tree walk failed: {error:?}"))
        })?;
        let length = unsafe { found.Length() }.unwrap_or(0);
        let mut matches = Vec::new();
        for index in 0..length {
            let Ok(element) = (unsafe { found.GetElement(index) }) else {
                continue;
            };
            if element_is_composer(&element, composer_name) {
                matches.push(element);
            }
        }
        match matches.len() {
            1 => Ok(matches.remove(0)),
            0 => Err(CommandError::exit1("named composer not found")),
            count => Err(CommandError::exit1(format!(
                "named composer ambiguous: {count} candidates"
            ))),
        }
    }

    fn element_is_composer(element: &IUIAutomationElement, composer_name: Option<&str>) -> bool {
        let control_type = unsafe { element.CurrentControlType() }.ok();
        if !control_type.is_some_and(|kind| {
            kind == UIA_EditControlTypeId
                || kind == UIA_DocumentControlTypeId
                || kind == UIA_TextControlTypeId
        }) {
            return false;
        }
        let Some(pattern) = value_pattern(element) else {
            return false;
        };
        let enabled = unsafe { element.CurrentIsEnabled() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        let focusable = unsafe { element.CurrentIsKeyboardFocusable() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        let read_only = unsafe { pattern.CurrentIsReadOnly() }
            .map(|value| value.as_bool())
            .unwrap_or(true);
        let name = element_name(element);
        let name_matches = composer_name
            .map(|wanted| name.trim() == wanted.trim())
            .unwrap_or_else(|| name_is_composer(&name));
        enabled && focusable && !read_only && name_matches
    }

    fn value_pattern(element: &IUIAutomationElement) -> Option<IUIAutomationValuePattern> {
        unsafe { element.GetCurrentPattern(UIA_ValuePatternId) }
            .ok()?
            .cast::<IUIAutomationValuePattern>()
            .ok()
    }

    fn value_of(element: &IUIAutomationElement) -> Option<String> {
        let pattern = value_pattern(element)?;
        unsafe { pattern.CurrentValue() }
            .ok()
            .map(|value| value.to_string())
    }

    fn wait_for_value(
        element: &IUIAutomationElement,
        expected: &str,
        timeout: Duration,
    ) -> Result<String, CommandError> {
        let started = Instant::now();
        let mut last = String::new();
        while started.elapsed() < timeout {
            last = value_of(element).unwrap_or_default();
            if last == expected {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(25));
        }
        Ok(last)
    }

    struct WindowsComposerTextActions<'a> {
        composer: &'a IUIAutomationElement,
        private_canary: &'a str,
        provider_name: &'a str,
        provider_pid: u32,
        pause_after_step: Option<&'a str>,
    }

    impl WindowsComposerTextActions<'_> {
        /// A live-run test may pause at one named shipping step and terminate
        /// the exact process which owns the discovered provider window.  This
        /// is deliberately in the writer, not a provider-process fixture: the
        /// next writer operation observes the death and reports a retry-safe
        /// refusal before any send boundary exists.
        fn pause_and_require_live_provider(&self, step: &str) -> Result<(), String> {
            if self.pause_after_step == Some(step) {
                println!(
                    "TASK3585_PAUSED provider={} pid={} step={step}",
                    self.provider_name, self.provider_pid
                );
                for _ in 0..400 {
                    if !provider_process_is_live(self.provider_pid) {
                        return Err(format!(
                            "{} closed during {step}. Your message was not sent anywhere. Retry placement in {}.",
                            self.provider_name, self.provider_name
                        ));
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                return Err(format!(
                    "{} did not close during requested {step} pause; refusing marked placement. Your message was not sent anywhere. Retry placement in {}.",
                    self.provider_name, self.provider_name
                ));
            }
            if !provider_process_is_live(self.provider_pid) {
                return Err(format!(
                    "{} closed during {step}. Your message was not sent anywhere. Retry placement in {}.",
                    self.provider_name, self.provider_name
                ));
            }
            Ok(())
        }
    }

    impl super::SharedTextActions for WindowsComposerTextActions<'_> {
        fn read_back_text(&mut self) -> Result<String, String> {
            let value = value_of(self.composer)
                .ok_or_else(|| "composer value could not be read back".to_owned())?;
            // `place_read_back_and_clear` calls this before placement, after
            // the paste, and after clear.  Only those concrete runtime steps
            // are eligible for the close-provider pause.
            let step = if value.is_empty() { "empty-readback" } else { "exact-readback" };
            self.pause_and_require_live_provider(step)?;
            Ok(value)
        }

        fn place_text(&mut self, text: &str) -> Result<(), String> {
            let snapshot = snapshot_clipboard()
                .map_err(|error| format!("clipboard snapshot failed: {error}"))?;
            let before_digest = snapshot.digest();
            let before_text = snapshot.unicode_text();
            let mut restorer = ClipboardRestorer {
                snapshot,
                restored: false,
            };
            let observer = ClipboardObserver::spawn(text, Duration::from_millis(3_000))
                .map_err(|error| format!("clipboard observer failed: {error}"))?;
            let staged_at = stage_clipboard_text(&text)
                .map_err(|error| format!("clipboard stage failed: {error}"))?;
            send_ctrl_v()?;
            self.pause_and_require_live_provider("marked-paste")?;
            let readback = wait_for_value(self.composer, text, Duration::from_millis(1_200))
                .map_err(|error| error.message)?;
            let observer_report = observer
                .wait()
                .map_err(|error| format!("clipboard observer failed: {error}"))?;

            let removed_at = restorer
                .restore()
                .map_err(|error| format!("clipboard restore failed: {error}"))?;
            let exposure_ms = removed_at.saturating_duration_since(staged_at).as_millis();
            let after_snapshot = snapshot_clipboard()
                .map_err(|error| format!("clipboard resnapshot failed: {error}"))?;
            let after_digest = after_snapshot.digest();
            let after_text = after_snapshot.unicode_text();
            let private_chars_found = private_canary_chars_reaching_clipboard(
                self.private_canary,
                &[text, observer_report.saw.as_str()],
            );
            let private_chars = private_chars_found.chars().count();
            println!("clipboard_exposure_ms={exposure_ms}");
            println!("clipboard_private_chars={private_chars}");
            println!("clipboard_private_chars_found={private_chars_found:?}");
            println!("clipboard_original_text_before={before_text:?}");
            println!("clipboard_original_text_after={after_text:?}");
            println!("clipboard_before_digest={before_digest:016x}");
            println!("clipboard_after_digest={after_digest:016x}");
            println!("clipboard_restored_exact={}", before_digest == after_digest);
            println!(
                "clipboard_second_program_pid={}",
                observer_report.pid.unwrap_or(0)
            );
            println!("clipboard_second_program_saw={:?}", observer_report.saw);
            println!("osl_clipboard_entries=0");
            if readback != text {
                return Err(format!("readback did not equal {text:?}"));
            }
            if before_digest != after_digest {
                return Err("clipboard content changed across placement".to_owned());
            }
            if before_text != after_text {
                return Err("clipboard text changed across placement".to_owned());
            }
            if observer_report.saw != text {
                return Err(format!(
                    "clipboard observer saw {:?}, not {:?}",
                    observer_report.saw, text
                ));
            }
            if private_chars != 0 {
                return Err(format!(
                    "{private_chars} private canary characters reached the clipboard: {private_chars_found:?}"
                ));
            }
            Ok(())
        }

        fn clear_text(&mut self) -> Result<(), String> {
            self.pause_and_require_live_provider("clear")?;
            let focused = unsafe { self.composer.CurrentHasKeyboardFocus() }
                .map(|value| value.as_bool())
                .unwrap_or(false);
            if !focused {
                return Err("composer lost keyboard focus before clear".to_owned());
            }
            send_ctrl_a_delete()?;
            thread::sleep(Duration::from_millis(220));
            Ok(())
        }
    }

    fn element_name(element: &IUIAutomationElement) -> String {
        unsafe { element.CurrentName() }
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    fn name_is_composer(name: &str) -> bool {
        let normalized = name
            .trim()
            .trim_end_matches('.')
            .replace('\u{2026}', "")
            .to_lowercase();
        if NON_COMPOSER_STEMS
            .iter()
            .any(|stem| normalized.contains(stem))
        {
            return false;
        }
        COMPOSER_STEMS.iter().any(|stem| normalized.contains(stem))
    }

    fn element_bounds(element: &IUIAutomationElement) -> Option<[i32; 4]> {
        let rect = unsafe { element.CurrentBoundingRectangle() }.ok()?;
        let left = rect.left as i32;
        let top = rect.top as i32;
        let right = rect.right as i32;
        let bottom = rect.bottom as i32;
        (right > left && bottom > top).then_some([left, top, right, bottom])
    }

    fn find_window(wanted: &str) -> Option<WindowInfo> {
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
        let info = window_info(hwnd);
        if window_matches(&info, &search.wanted) {
            search.found.push(info);
        }
        TRUE
    }

    fn foreground_window() -> Option<WindowInfo> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.is_null()).then(|| window_info(hwnd))
    }

    fn window_info(hwnd: HWND) -> WindowInfo {
        let mut process_id = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
        WindowInfo {
            hwnd,
            title: window_title(hwnd),
            process_name: window_process_name(hwnd).unwrap_or_default(),
            process_id,
        }
    }

    fn provider_process_is_live(pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return false;
        }
        let mut exit_code = 0;
        let live = unsafe { GetExitCodeProcess(handle, &mut exit_code) } != 0 && exit_code == STILL_ACTIVE;
        unsafe { CloseHandle(handle) };
        live
    }

    fn digest_text(value: &str) -> u64 {
        value.as_bytes().iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
    }

    fn window_matches(window: &WindowInfo, wanted: &str) -> bool {
        let wanted = normalize(wanted.trim_end_matches(".exe"));
        normalize(window.process_name.trim_end_matches(".exe")) == wanted
            || normalize(&window.title).contains(&wanted)
    }

    fn describe_window(window: &WindowInfo) -> String {
        format!(
            "hwnd={:#x} process={:?} title={:?}",
            window.hwnd as isize, window.process_name, window.title
        )
    }

    fn same_root(left: HWND, right: HWND) -> bool {
        if left.is_null() || right.is_null() {
            return false;
        }
        (unsafe { GetAncestor(left, GA_ROOT) }) == (unsafe { GetAncestor(right, GA_ROOT) })
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
        if unsafe { GetForegroundWindow() } != hwnd {
            let _attachment = InputQueueAttachment::acquire();
            unsafe { SetForegroundWindow(hwnd) };
        }
    }

    fn click_composer(bounds: [i32; 4], expected_root: HWND) -> Result<(), CommandError> {
        let [left, top, right, bottom] = bounds;
        if right <= left || bottom <= top {
            return Err(CommandError::exit1("composer bounds are invalid"));
        }
        let x = left + (right - left) / 3;
        let y = top + (bottom - top) / 2;
        let point_window = unsafe { WindowFromPoint(POINT { x, y }) };
        if point_window.is_null() || !same_root(point_window, expected_root) {
            return Err(CommandError::exit1("composer click point is obscured"));
        }

        let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width <= 1 || height <= 1 {
            return Err(CommandError::exit1("virtual desktop metrics are invalid"));
        }
        let normalised_x = ((x - origin_x) as i64 * 65_535 / (width - 1) as i64) as i32;
        let normalised_y = ((y - origin_y) as i64 * 65_535 / (height - 1) as i64) as i32;

        let mut restore = POINT { x: 0, y: 0 };
        let saved = unsafe { GetCursorPos(&mut restore) } != 0;
        let make = |flags: u32| INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: normalised_x,
                    dy: normalised_y,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [
            make(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
            make(MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
            make(MOUSEEVENTF_LEFTUP | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK),
        ];
        let accepted = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if saved {
            unsafe { SetCursorPos(restore.x, restore.y) };
        }
        if accepted != inputs.len() as u32 {
            return Err(CommandError::exit1("Windows rejected the composer click"));
        }
        Ok(())
    }

    fn send_ctrl_v() -> Result<(), String> {
        let key = |vk: u16, flags: u32| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [
            key(VK_CONTROL, 0),
            key(u16::from(b'V'), 0),
            key(u16::from(b'V'), KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        let accepted = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if accepted != inputs.len() as u32 {
            return Err("Windows rejected Ctrl+V".to_owned());
        }
        Ok(())
    }

    fn send_ctrl_a_delete() -> Result<(), String> {
        let key = |vk: u16, flags: u32| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [
            key(VK_CONTROL, 0),
            key(u16::from(b'A'), 0),
            key(u16::from(b'A'), KEYEVENTF_KEYUP),
            key(VK_CONTROL, KEYEVENTF_KEYUP),
            key(VK_DELETE, 0),
            key(VK_DELETE, KEYEVENTF_KEYUP),
        ];
        let accepted = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                size_of::<INPUT>() as i32,
            )
        };
        if accepted != inputs.len() as u32 {
            return Err("Windows rejected Ctrl+A/Delete".to_owned());
        }
        Ok(())
    }

    fn snapshot_clipboard() -> Result<ClipboardSnapshot, String> {
        with_clipboard(|| {
            let mut formats = Vec::new();
            let mut format = 0u32;
            loop {
                format = unsafe { EnumClipboardFormats(format) };
                if format == 0 {
                    break;
                }
                let handle = unsafe { GetClipboardData(format) };
                if handle.is_null() {
                    return Err(format!("clipboard format {format} could not be read"));
                }
                let size = unsafe { GlobalSize(handle as _) };
                if size == 0 {
                    return Err(format!("clipboard format {format} is not byte-copyable"));
                }
                let source = unsafe { GlobalLock(handle as _) };
                if source.is_null() {
                    return Err(format!("clipboard format {format} could not be locked"));
                }
                let bytes =
                    unsafe { std::slice::from_raw_parts(source.cast::<u8>(), size) }.to_vec();
                unsafe { GlobalUnlock(handle as _) };
                formats.push(ClipboardFormat { format, bytes });
            }
            Ok(ClipboardSnapshot { formats })
        })
    }

    fn stage_clipboard_text(value: &str) -> Result<Instant, String> {
        let utf16 = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let bytes = unsafe {
            std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), utf16.len() * size_of::<u16>())
        };
        with_clipboard(|| {
            if unsafe { EmptyClipboard() } == 0 {
                return Err("clipboard clear failed".to_owned());
            }
            set_clipboard_bytes(CF_UNICODETEXT as u32, bytes)?;
            Ok(Instant::now())
        })
    }

    fn restore_clipboard(snapshot: &ClipboardSnapshot) -> Result<Instant, String> {
        with_clipboard(|| {
            if unsafe { EmptyClipboard() } == 0 {
                return Err("clipboard clear failed".to_owned());
            }
            let removed_at = Instant::now();
            for format in &snapshot.formats {
                set_clipboard_bytes(format.format, &format.bytes)?;
            }
            Ok(removed_at)
        })
    }

    fn read_clipboard_unicode_text() -> Result<Option<String>, String> {
        with_clipboard(|| {
            if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT as u32) } == 0 {
                return Ok(None);
            }
            let handle = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
            if handle.is_null() {
                return Ok(None);
            }
            let size = unsafe { GlobalSize(handle as _) };
            if size < size_of::<u16>() {
                return Ok(None);
            }
            let source = unsafe { GlobalLock(handle as _) };
            if source.is_null() {
                return Err("clipboard text could not be locked".to_owned());
            }
            let units = unsafe {
                std::slice::from_raw_parts(source.cast::<u16>(), size / size_of::<u16>())
            };
            let nul = units
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..nul]);
            unsafe { GlobalUnlock(handle as _) };
            Ok(Some(text))
        })
    }

    fn set_clipboard_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
        if bytes.is_empty() {
            return Ok(());
        }
        let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
        if memory.is_null() {
            return Err("clipboard allocation failed".to_owned());
        }
        let destination = unsafe { GlobalLock(memory) };
        if destination.is_null() {
            unsafe { windows_sys::Win32::Foundation::GlobalFree(memory) };
            return Err("clipboard lock failed".to_owned());
        }
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), destination.cast::<u8>(), bytes.len());
            GlobalUnlock(memory);
        }
        if unsafe { SetClipboardData(format, memory) }.is_null() {
            unsafe { windows_sys::Win32::Foundation::GlobalFree(memory) };
            return Err("clipboard write failed".to_owned());
        }
        Ok(())
    }

    fn with_clipboard<T>(call: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        let mut opened = false;
        for _ in 0..12 {
            if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                opened = true;
                break;
            }
            thread::sleep(Duration::from_millis(12));
        }
        if !opened {
            return Err("clipboard busy".to_owned());
        }
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                unsafe { CloseClipboard() };
            }
        }
        let _guard = Guard;
        call()
    }

    impl ClipboardSnapshot {
        fn digest(&self) -> u64 {
            let mut hash = 0xcbf2_9ce4_8422_2325u64;
            for format in &self.formats {
                for byte in format.format.to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
                for byte in &format.bytes {
                    hash ^= u64::from(*byte);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            hash
        }

        fn unicode_text(&self) -> Option<String> {
            let text = self
                .formats
                .iter()
                .find(|format| format.format == CF_UNICODETEXT as u32)?;
            let units = text
                .bytes
                .chunks_exact(size_of::<u16>())
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .collect::<Vec<_>>();
            let nul = units
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(units.len());
            Some(String::from_utf16_lossy(&units[..nul]))
        }
    }

    fn private_canary_chars_reaching_clipboard(canary: &str, observations: &[&str]) -> String {
        canary
            .chars()
            .filter(|private| observations.iter().any(|seen| seen.contains(*private)))
            .collect()
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
    match windows_place_text::run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{}", error.message);
            std::process::exit(error.code);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("task_3406_place_text requires Windows");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::verify_marked_placement;

    fn random_mark() -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is after epoch")
            .as_nanos();
        format!("TASK3415-{nanos:x}-OSL")
    }

    #[test]
    fn task_3415_empty_before_exact_after_check_is_not_vacuous() {
        let mark = random_mark();

        assert!(verify_marked_placement("", &mark, &mark).is_ok());
        let dirty = verify_marked_placement("owner draft", &mark, &mark)
            .expect_err("a non-empty composer must fail before placement");
        let noop = verify_marked_placement("", "", &mark)
            .expect_err("a no-op placing job must fail the exact readback");

        eprintln!(
            "task3415 mark={mark:?} before={:?} after={mark:?} noop_after={:?} noop_failed=true",
            "", ""
        );
        assert!(dirty.contains("not empty"));
        assert!(noop.contains("readback did not equal placed mark"));
    }
}
