use std::time::Instant;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedTextPlacementReceipt {
    pub placed_bytes: usize,
    pub readback_bytes: usize,
    pub clear_bytes: usize,
}

/// Provider-neutral composer actions for an already-discovered text box.
pub trait SharedTextActions {
    fn read_back_text(&mut self) -> Result<String, String>;
    fn place_text(&mut self, text: &str) -> Result<(), String>;
    fn clear_text(&mut self) -> Result<(), String>;
}

/// Place exact text, prove the read-back byte-for-byte, and clear it without
/// invoking any provider send control.
pub fn place_read_back_and_clear(
    actions: &mut impl SharedTextActions,
    mark: &str,
) -> Result<SharedTextPlacementReceipt, String> {
    if mark.is_empty() {
        return Err("marked text must contain at least one byte".to_owned());
    }
    let before_readback = actions.read_back_text()?;
    if !before_readback.is_empty() {
        return Err(format!("composer was not empty before placement: {before_readback:?}"));
    }
    actions.place_text(mark)?;
    let readback = actions.read_back_text()?;
    if readback.as_bytes() != mark.as_bytes() {
        return Err(format!("readback did not equal placed mark {mark:?}: {readback:?}"));
    }
    actions.clear_text()?;
    let clear_readback = actions.read_back_text()?;
    if !clear_readback.is_empty() {
        return Err(format!("composer clear left {} bytes: {clear_readback:?}", clear_readback.len()));
    }
    Ok(SharedTextPlacementReceipt {
        placed_bytes: mark.len(),
        readback_bytes: readback.len(),
        clear_bytes: clear_readback.len(),
    })
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, PartialEq, Eq)]
enum ReadbackComparison {
    Matched,
    DidNotMatch {
        position: usize,
        expected: Option<char>,
        actual: Option<char>,
    },
}

#[cfg(any(target_os = "windows", test))]
impl ReadbackComparison {
    fn status_line(&self) -> String {
        match self {
            Self::Matched => "matched".to_owned(),
            Self::DidNotMatch {
                position,
                expected,
                actual,
            } => format!(
                "did-not-match first_differing_character_position={position} expected={} actual={}",
                printable_char(*expected),
                printable_char(*actual)
            ),
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn compare_readback(expected: &str, actual: &str) -> ReadbackComparison {
    let mut expected_chars = expected.chars();
    let mut actual_chars = actual.chars();
    for position in 1.. {
        match (expected_chars.next(), actual_chars.next()) {
            (Some(left), Some(right)) if left == right => {}
            (None, None) => return ReadbackComparison::Matched,
            (expected, actual) => {
                return ReadbackComparison::DidNotMatch {
                    position,
                    expected,
                    actual,
                }
            }
        }
    }
    unreachable!("unbounded loop returns on match or first differing character")
}

#[cfg(any(target_os = "windows", test))]
fn printable_char(value: Option<char>) -> String {
    match value {
        Some(ch) => format!("{ch:?}"),
        None => "<end>".to_owned(),
    }
}

#[cfg(test)]
mod task_3407_tests {
    use super::{compare_readback, ReadbackComparison};

    #[test]
    fn clean_readback_returns_matched() {
        assert_eq!(
            compare_readback("MAPLE-3407", "MAPLE-3407"),
            ReadbackComparison::Matched
        );
        assert_eq!(
            compare_readback("MAPLE-3407", "MAPLE-3407").status_line(),
            "matched"
        );
    }

    #[test]
    fn changed_character_names_the_first_differing_character_position() {
        let comparison = compare_readback("MAPLE-3407", "MAPLE-X407");
        assert_eq!(
            comparison.status_line(),
            "did-not-match first_differing_character_position=7 expected='3' actual='X'"
        );
    }
}

/// Provider-neutral actions used by the one shared place-text job.
///
/// A provider may locate controls and report readiness in its own read-only
/// adapter, but it cannot replace `paste_text`: every rich editor is driven by
/// this same editor-visible paste action.
pub trait SharedPlaceTextActions {
    fn read_back_text(&mut self) -> Result<String, String>;
    fn paste_text(&mut self, text: &str) -> Result<(), String>;
    fn editor_accepts_message(&mut self) -> Result<bool, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedPlaceTextReceipt {
    pub placed_bytes: usize,
    pub readback_bytes: usize,
    pub readback_exact: bool,
    pub editor_accepts_message: bool,
}

/// Tracks only the interval in which this command's staged text can be read
/// from the system clipboard. A command that exits before staging has no such
/// interval and consequently reports zero.
#[derive(Debug, Default)]
pub struct ClipboardExposure {
    staged_at: Option<Instant>,
    restored_at: Option<Instant>,
}

impl ClipboardExposure {
    pub fn staged(&mut self, staged_at: Instant) {
        self.staged_at = Some(staged_at);
        self.restored_at = None;
    }

    pub fn restored(&mut self, restored_at: Instant) {
        if self.staged_at.is_some() {
            self.restored_at = Some(restored_at);
        }
    }

    pub fn milliseconds(&self) -> u128 {
        match (self.staged_at, self.restored_at) {
            (Some(staged_at), Some(restored_at)) => {
                restored_at.saturating_duration_since(staged_at).as_millis()
            }
            (Some(staged_at), None) => staged_at.elapsed().as_millis(),
            (None, _) => 0,
        }
    }
}

/// A clipboard payload proven not to contain any character from the private
/// canary. Its private field prevents the clipboard writer from accepting an
/// unchecked command-line string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClipboardCoverText<'a>(&'a str);

impl<'a> ClipboardCoverText<'a> {
    pub fn new(cover_text: &'a str, private_canary: &str) -> Result<Self, String> {
        if cover_text.is_empty()
            || cover_text
                .chars()
                .any(|character| matches!(character, '\r' | '\n'))
        {
            return Err("clipboard cover text must be one non-empty line".to_owned());
        }
        if private_canary.is_empty() {
            return Err("private clipboard canary must not be empty".to_owned());
        }
        let private_chars_found =
            private_canary_chars_reaching_clipboard(private_canary, &[cover_text]);
        if !private_chars_found.is_empty() {
            return Err(format!(
                "{} private canary characters reached the clipboard payload: {private_chars_found:?}",
                private_chars_found.chars().count()
            ));
        }
        Ok(Self(cover_text))
    }

    pub fn as_str(self) -> &'a str {
        self.0
    }
}

fn private_canary_chars_reaching_clipboard(canary: &str, observations: &[&str]) -> String {
    canary
        .chars()
        .filter(|private| observations.iter().any(|seen| seen.contains(*private)))
        .collect()
}

/// Put one marked string through the shared editor-visible action, read it back
/// byte-for-byte, and require the provider's own readiness state to agree.
pub fn place_text_through_shared_job(
    actions: &mut impl SharedPlaceTextActions,
    marked: &str,
) -> Result<SharedPlaceTextReceipt, String> {
    if marked.is_empty()
        || marked
            .chars()
            .any(|character| matches!(character, '\r' | '\n'))
    {
        return Err("shared place-text requires one non-empty line".to_owned());
    }
    if !actions.read_back_text()?.is_empty() {
        return Err("shared place-text requires an empty message box".to_owned());
    }

    actions.paste_text(marked)?;
    let readback = actions.read_back_text()?;
    let readback_exact = readback.as_bytes() == marked.as_bytes();
    if !readback_exact {
        return Err("shared place-text read-back changed the marked bytes".to_owned());
    }
    let editor_accepts_message = actions.editor_accepts_message()?;
    if !editor_accepts_message {
        return Err("provider editor did not make its send control available".to_owned());
    }

    Ok(SharedPlaceTextReceipt {
        placed_bytes: marked.len(),
        readback_bytes: readback.len(),
        readback_exact,
        editor_accepts_message,
    })
}

#[cfg(target_os = "windows")]
mod windows_place_text {
    use super::{compare_readback, printable_char, ReadbackComparison};

    use std::ffi::{c_void, OsString};
    use std::mem::size_of;
    use std::os::windows::ffi::OsStringExt;
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
        UIA_ButtonControlTypeId, UIA_DocumentControlTypeId, UIA_EditControlTypeId,
        UIA_TextControlTypeId, UIA_ValuePatternId,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, POINT, RECT, TRUE};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, OpenClipboard,
        SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
        MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE,
        MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT, VK_CONTROL,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetAncestor, GetClassNameW, GetCursorPos,
        GetForegroundWindow, GetSystemMetrics, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsWindowVisible, SetCursorPos, SetForegroundWindow, ShowWindow,
        WindowFromPoint, GA_ROOT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN, SW_RESTORE,
    };

    const DEFAULT_APP: &str = "Discord";
    const DEFAULT_TEXT: &str = "MAPLE-3406";
    const DEFAULT_INITIAL_FRONT: &str = "Photos";
    const DEFAULT_WAIT_BEFORE_READ_MS: u64 = 0;
    const EVENT_SYSTEM_ALERT: u32 = 0x0002;
    const ELECTRON_A11Y_OBJECT_ID: i32 = 1;
    const OBJID_CLIENT: i32 = -4;
    const MIN_TREE_ELEMENTS: i32 = 10;
    const TREE_WAIT_MS: u64 = 1_000;
    const SETTLE_MS: u64 = 160;
    const DEFAULT_WAIT_TIMEOUT_SECONDS: u64 = 120;
    // lane/g: the standalone clipboard observer.
    const DEFAULT_PRIVATE_CANARY: &str = "QQQQQQQQQQ";
    const DEFAULT_OBSERVER_TIMEOUT_MS: u64 = 3_000;
    const SIGNAL_RENDERER_CLASS: &str = "Chrome_RenderWidgetHostHWND";
    const COMPOSER_STEMS: &[&str] = &["message", "nachricht", "mensaje"];
    const NON_COMPOSER_STEMS: &[&str] = &["search", "filter", "buscar"];

    #[derive(Clone, Debug)]
    struct WindowInfo {
        hwnd: HWND,
        title: String,
        process_name: String,
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
        fn restore(&mut self) -> Result<(), String> {
            restore_clipboard(&self.snapshot)?;
            self.restored = true;
            Ok(())
        }
    }

    impl Drop for ClipboardRestorer {
        fn drop(&mut self) {
            if !self.restored {
                let _ = restore_clipboard(&self.snapshot);
            }
        }
    }

    /// Reports at scope exit so every command path has one result. It is
    /// declared before `ClipboardRestorer`, so unwinding restores the clipboard
    /// before this report is emitted.
    struct ClipboardExposureReporter {
        exposure: super::ClipboardExposure,
    }

    impl ClipboardExposureReporter {
        fn new() -> Self {
            Self {
                exposure: super::ClipboardExposure::default(),
            }
        }
    }

    impl Drop for ClipboardExposureReporter {
        fn drop(&mut self) {
            println!("clipboard_exposure_ms={}", self.exposure.milliseconds());
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
        let mut clipboard_exposure = ClipboardExposureReporter::new();
        let args = Args::parse()?;
        // lane/g: --clipboard-observer watches the clipboard instead of placing
        // text, so it returns before any window work below.
        if let Some(observer) = args.observer {
            return run_clipboard_observer(observer);
        }
        let signal_mode = app_is_signal(&args.app);
        let initial = foreground_window()
            .ok_or_else(|| CommandError::exit1("Windows reported no foreground window"))?;
        println!(
            "initial_front={}",
            describe_window(&initial).replace('\n', " ")
        );
        if !window_matches(&initial, &args.initial_front) {
            return Err(CommandError::exit1(format!(
                "initial front window was not {}",
                args.initial_front
            )));
        }

        let discord = match find_window(&args.app) {
            Some(window) => window,
            None => {
                println!("osl_clipboard_entries=0");
                return Err(CommandError::exit1(format!("{} not found", args.app)));
            }
        };
        verify_target_onscreen(discord.hwnd, &args.app)?;
        let initially_behind = !same_root(initial.hwnd, discord.hwnd);
        println!(
            "behind_window={} behind_initial={}",
            describe_window(&discord).replace('\n', " "),
            initially_behind
        );
        if !initially_behind && !args.allow_already_front {
            return Err(CommandError::exit1(format!(
                "{} was already the foreground window",
                args.app
            )));
        }

        if args.wait_for_person {
            println!("front_window_grab=off");
            println!("placement_waiting_for_person=true");
            println!("placement_attempted_before_front=false");
            wait_for_person_to_front(
                &discord,
                Duration::from_secs(args.wait_timeout_seconds),
                &args.app,
            )?;
        } else if initially_behind {
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
                    args.app
                )));
            }
        } else {
            println!(
                "after_grab_front={}",
                describe_window(&initial).replace('\n', " ")
            );
        }
        verify_target_onscreen(discord.hwnd, &args.app)?;

        let _com = initialize_com()?;
        let automation = automation()?;
        let root = app_accessibility_root(&automation, &args.app, discord.hwnd)?;
        wait_for_tree(&root, &automation)?;
        let composer = find_composer(&root, &automation, &args.app)?;
        println!("composer_name={:?}", element_name(&composer));
        let bounds = element_bounds(&composer).ok_or_else(|| {
            CommandError::exit1(format!("{} composer bounds not found", args.app))
        })?;
        println!(
            "composer_bounds={},{},{},{}",
            bounds[0], bounds[1], bounds[2], bounds[3]
        );

        verify_focused_typing_point(&automation, &composer, discord.hwnd, &args.app)?;
        let before_readback = value_of(&composer).unwrap_or_default();
        println!("before_readback={before_readback:?}");
        if !before_readback.is_empty() {
            return Err(CommandError::exit1(format!(
                "{} composer was not empty before placement",
                args.app
            )));
        }
        if signal_mode {
            let send_before = send_button_available(&root, &automation)?;
            println!("TASK1033_SIGNAL_SEND_BEFORE_AVAILABLE={send_before}");
            if send_before {
                return Err(CommandError::exit1(
                    "Signal send button was available before placement",
                ));
            }
        }

        let snapshot = snapshot_clipboard()
            .map_err(|error| CommandError::exit1(format!("clipboard snapshot failed: {error}")))?;
        let before_digest = snapshot.digest();
        let mut restorer = ClipboardRestorer {
            snapshot,
            restored: false,
        };
        // Validate separation before opening or changing the clipboard. `text`
        // is the finished public cover; the private canary is only a refusal
        // oracle and is never accepted by the clipboard writer.
        let text = super::ClipboardCoverText::new(&args.text, &args.private_canary)
            .map_err(CommandError::exit1)?;
        let staged_at = stage_clipboard_text(&text)
            .map_err(|error| CommandError::exit1(format!("clipboard stage failed: {error}")))?;
        clipboard_exposure.exposure.staged(staged_at);
        let shared_receipt = {
            let mut actions = WindowsSharedPlaceTextActions {
                composer: &composer,
                root: &root,
                automation: &automation,
                require_send_button: signal_mode,
            };
            super::place_text_through_shared_job(&mut actions, &args.text)
                .map_err(|error| CommandError::exit1(format!("{} {error}", args.app)))?
        };

        let readback = value_of(&composer).unwrap_or_default();
        println!("readback={readback:?}");
        if signal_mode {
            println!(
                "TASK1033_SIGNAL_MARKED_BYTES={}",
                shared_receipt.placed_bytes
            );
            println!(
                "TASK1033_SIGNAL_READBACK_BYTES={}",
                shared_receipt.readback_bytes
            );
            println!(
                "TASK1033_SIGNAL_READBACK_EXACT={}",
                shared_receipt.readback_exact
            );
            println!(
                "TASK1033_SIGNAL_SEND_AFTER_AVAILABLE={}",
                shared_receipt.editor_accepts_message
            );
        }
        if args.wait_before_read_ms > 0 {
            println!("waiting_before_read_ms={}", args.wait_before_read_ms);
            thread::sleep(Duration::from_millis(args.wait_before_read_ms));
        }
        let readback = value_of(&composer).unwrap_or_default();
        let comparison = compare_readback(&args.text, &readback);
        println!("{}", comparison.status_line());
        restorer
            .restore()
            .map_err(|error| CommandError::exit1(format!("clipboard restore failed: {error}")))?;
        clipboard_exposure.exposure.restored(Instant::now());
        let after_digest = snapshot_clipboard()
            .map_err(|error| CommandError::exit1(format!("clipboard resnapshot failed: {error}")))?
            .digest();
        println!("clipboard_before_digest={before_digest:016x}");
        println!("clipboard_after_digest={after_digest:016x}");
        println!("clipboard_restored_exact={}", before_digest == after_digest);
        println!("osl_clipboard_entries=0");
        println!("placed_count=1");

        if let ReadbackComparison::DidNotMatch {
            position,
            expected,
            actual,
        } = comparison
        {
            return Err(CommandError::exit1(format!(
                "did-not-match first_differing_character_position={position} expected={} actual={}",
                printable_char(expected),
                printable_char(actual)
            )));
        }
        if before_digest != after_digest {
            return Err(CommandError::exit1(
                "clipboard content changed across placement",
            ));
        }
        Ok(())
    }

    fn verify_target_onscreen(hwnd: HWND, app: &str) -> Result<(), CommandError> {
        let rect = window_rect(hwnd)
            .ok_or_else(|| CommandError::exit1(format!("{app} window bounds could not be read")))?;
        let origin_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let origin_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if width <= 1 || height <= 1 {
            return Err(CommandError::exit1("virtual desktop metrics are invalid"));
        }
        let desktop = RECT {
            left: origin_x,
            top: origin_y,
            right: origin_x + width,
            bottom: origin_y + height,
        };
        let intersects = rect.right > desktop.left
            && rect.left < desktop.right
            && rect.bottom > desktop.top
            && rect.top < desktop.bottom;
        println!("target_onscreen={intersects}");
        if !intersects {
            return Err(CommandError::exit1(format!("{app} window is off screen")));
        }
        Ok(())
    }

    fn verify_focused_typing_point(
        automation: &IUIAutomation,
        composer: &IUIAutomationElement,
        expected_root: HWND,
        app: &str,
    ) -> Result<(), CommandError> {
        let front = foreground_window().ok_or_else(|| {
            CommandError::exit1("Windows reported no foreground window before paste")
        })?;
        if !same_root(front.hwnd, expected_root) {
            println!(
                "placement_refused_focused_app={}",
                describe_window(&front).replace('\n', " ")
            );
            return Err(CommandError::exit1(format!(
                "{app} placement refused: focused app was not {app}"
            )));
        }

        let focused = unsafe { automation.GetFocusedElement() }.map_err(|error| {
            CommandError::exit1(format!("focused typing point could not be read: {error:?}"))
        })?;
        let focused_name = element_name(&focused);
        println!("focused_box_name={focused_name:?}");
        let composer_has_focus = unsafe { composer.CurrentHasKeyboardFocus() }
            .map(|value| value.as_bool())
            .unwrap_or(false);
        let focused_is_message_box = composer_has_focus && element_is_composer(&focused);
        println!("typing_point_in_message_box={focused_is_message_box}");
        if !focused_is_message_box {
            println!("placement_refused_focused_box={focused_name:?}");
            return Err(CommandError::exit1(format!(
                "{app} placement refused: focused box was {focused_name:?}, not the conversation message box"
            )));
        }
        Ok(())
    }

    struct Args {
        initial_front: String,
        app: String,
        text: String,
        private_canary: String,
        wait_for_person: bool,
        wait_timeout_seconds: u64,
        allow_already_front: bool,
        observer: Option<ObserverArgs>,
        wait_before_read_ms: u64,
    }

    impl Args {
        fn parse() -> Result<Self, CommandError> {
            let mut initial_front = DEFAULT_INITIAL_FRONT.to_owned();
            let mut app = DEFAULT_APP.to_owned();
            let mut text = DEFAULT_TEXT.to_owned();
            let mut private_canary = DEFAULT_PRIVATE_CANARY.to_owned();
            let mut wait_for_person = false;
            let mut wait_timeout_seconds = DEFAULT_WAIT_TIMEOUT_SECONDS;
            let mut allow_already_front = false;
            let mut observer_requested = false;
            let mut observer_needle: Option<String> = None;
            let mut observer_timeout_ms = DEFAULT_OBSERVER_TIMEOUT_MS;
            let mut wait_before_read_ms = DEFAULT_WAIT_BEFORE_READ_MS;
            let mut args = std::env::args().skip(1);
            while let Some(arg) = args.next() {
                match arg.as_str() {
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
                    "--wait-for-person" => {
                        wait_for_person = true;
                    }
                    "--allow-already-front" => {
                        allow_already_front = true;
                    }
                    "--wait-timeout-seconds" => {
                        let value = args.next().ok_or_else(|| {
                            CommandError::usage("--wait-timeout-seconds needs a value")
                        })?;
                        wait_timeout_seconds = value.parse().map_err(|_| {
                            CommandError::usage("--wait-timeout-seconds must be a positive integer")
                        })?;
                    }
                    "--clipboard-observer" => {
                        observer_requested = true;
                    }
                    "--observer-needle" => {
                        observer_needle = Some(args.next().ok_or_else(|| {
                            CommandError::usage("--observer-needle needs a value")
                        })?);
                    }
                    "--observer-timeout-ms" => {
                        let raw = args.next().ok_or_else(|| {
                            CommandError::usage("--observer-timeout-ms needs a value")
                        })?;
                        observer_timeout_ms = raw.parse().map_err(|_| {
                            CommandError::usage("--observer-timeout-ms must be a whole number")
                        })?;
                    }
                    "--wait-before-read-ms" => {
                        let value = args.next().ok_or_else(|| {
                            CommandError::usage("--wait-before-read-ms needs a value")
                        })?;
                        wait_before_read_ms = value.parse().map_err(|_| {
                            CommandError::usage(
                                "--wait-before-read-ms must be a non-negative integer",
                            )
                        })?;
                    }
                    "--help" | "-h" => {
                        return Err(CommandError::usage(
                            "usage: task_3406_place_text [--initial-front Photos] [--app Discord] [--text MAPLE-3406] [--private-canary QQQQQQQQQQ] [--wait-for-person] [--wait-timeout-seconds 120] [--allow-already-front] [--wait-before-read-ms 0] [--clipboard-observer] [--observer-needle TEXT] [--observer-timeout-ms 3000]",
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
            if wait_timeout_seconds == 0 {
                return Err(CommandError::usage(
                    "--wait-timeout-seconds must be a positive integer",
                ));
            }
            // Refuse before any UI or clipboard action. The typed clipboard
            // boundary repeats this check immediately before staging.
            super::ClipboardCoverText::new(&text, &private_canary).map_err(CommandError::usage)?;
            Ok(Self {
                initial_front,
                app,
                text,
                private_canary,
                wait_for_person,
                wait_timeout_seconds,
                allow_already_front,
                observer: if observer_requested {
                    Some(ObserverArgs {
                        needle: observer_needle
                            .unwrap_or_else(|| DEFAULT_PRIVATE_CANARY.to_owned()),
                        timeout: Duration::from_millis(observer_timeout_ms),
                    })
                } else {
                    None
                },
                wait_before_read_ms,
            })
        }
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

    struct WindowsSharedPlaceTextActions<'a> {
        composer: &'a IUIAutomationElement,
        root: &'a IUIAutomationElement,
        automation: &'a IUIAutomation,
        require_send_button: bool,
    }

    impl super::SharedPlaceTextActions for WindowsSharedPlaceTextActions<'_> {
        fn read_back_text(&mut self) -> Result<String, String> {
            value_of(self.composer)
                .ok_or_else(|| "message box read-back was unavailable".to_owned())
        }

        fn paste_text(&mut self, _text: &str) -> Result<(), String> {
            // The caller staged `text` on the clipboard. This is the one shared
            // editor-visible placement action for every supported native app.
            send_ctrl_v()?;
            thread::sleep(Duration::from_millis(320));
            Ok(())
        }

        fn editor_accepts_message(&mut self) -> Result<bool, String> {
            if self.require_send_button {
                return send_button_available(self.root, self.automation)
                    .map_err(|error| error.message);
            }
            Ok(value_of(self.composer).is_some_and(|value| !value.is_empty()))
        }
    }

    fn app_accessibility_root(
        automation: &IUIAutomation,
        app: &str,
        hwnd: HWND,
    ) -> Result<IUIAutomationElement, CommandError> {
        if app_is_signal(app) {
            let renderer =
                find_child_window_by_class(hwnd, SIGNAL_RENDERER_CLASS).ok_or_else(|| {
                    CommandError::exit1("Signal renderer accessibility window not found")
                })?;
            let _ = wake_electron_accessibility(renderer);
            return unsafe {
                automation.ElementFromHandle(windows::Win32::Foundation::HWND(renderer as _))
            }
            .map_err(|error| CommandError::exit1(format!("Signal UIA root failed: {error:?}")));
        }
        let accessible = wake_electron_accessibility(hwnd)
            .ok_or_else(|| CommandError::exit1(format!("{app} accessibility wake failed")))?;
        unsafe { automation.ElementFromIAccessible(&accessible, 0) }
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
                    "Discord accessibility tree never populated: saw {count}, needed {MIN_TREE_ELEMENTS}"
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
        app: &str,
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
            if element_is_composer(&element) {
                matches.push(element);
            }
        }
        match matches.len() {
            1 => Ok(matches.remove(0)),
            0 => Err(CommandError::exit1(format!("{app} composer not found"))),
            count => Err(CommandError::exit1(format!(
                "{app} composer ambiguous: {count} candidates"
            ))),
        }
    }

    /// Read-only readiness proof. Placement remains the shared Ctrl+V action;
    /// this query merely proves the rich editor accepted that paste into its
    /// own private message state.
    fn send_button_available(
        root: &IUIAutomationElement,
        automation: &IUIAutomation,
    ) -> Result<bool, CommandError> {
        let condition = unsafe { automation.CreateTrueCondition() }.map_err(|error| {
            CommandError::exit1(format!("UI Automation condition failed: {error:?}"))
        })?;
        let found = unsafe { root.FindAll(TreeScope_Subtree, &condition) }.map_err(|error| {
            CommandError::exit1(format!("UI Automation tree walk failed: {error:?}"))
        })?;
        let length = unsafe { found.Length() }.unwrap_or(0);
        for index in 0..length {
            let Ok(element) = (unsafe { found.GetElement(index) }) else {
                continue;
            };
            if unsafe { element.CurrentControlType() }.ok() != Some(UIA_ButtonControlTypeId)
                || !name_is_send_button(&element_name(&element))
            {
                continue;
            }
            return Ok(unsafe { element.CurrentIsEnabled() }
                .map(|value| value.as_bool())
                .unwrap_or(false));
        }
        Ok(false)
    }

    fn element_is_composer(element: &IUIAutomationElement) -> bool {
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
        enabled && focusable && !read_only && name_is_composer(&element_name(element))
    }

    fn value_pattern(element: &IUIAutomationElement) -> Option<IUIAutomationValuePattern> {
        unsafe { element.GetCurrentPattern(UIA_ValuePatternId) }
            .ok()?
            .cast::<IUIAutomationValuePattern>()
            .ok()
    }

    fn value_of(element: &IUIAutomationElement) -> Option<String> {
        let pattern = value_pattern(element)?;
        let raw = unsafe { pattern.CurrentValue() }
            .ok()
            .map(|value| value.to_string())?;
        Some(normalize_discord_composer_value(&raw))
    }

    fn normalize_discord_composer_value(raw: &str) -> String {
        let value = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        value
            .strip_prefix("\r\n")
            .or_else(|| value.strip_prefix('\n'))
            .unwrap_or(value)
            .to_owned()
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

    fn name_is_send_button(name: &str) -> bool {
        let normalized = name
            .trim()
            .trim_end_matches('.')
            .replace('\u{2026}', "")
            .to_lowercase();
        normalized == "send" || normalized == "send message" || normalized.contains("send message")
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

    struct ChildWindowSearch {
        class_name: String,
        found: HWND,
    }

    fn find_child_window_by_class(parent: HWND, class_name: &str) -> Option<HWND> {
        let mut search = ChildWindowSearch {
            class_name: class_name.to_owned(),
            found: ptr::null_mut(),
        };
        unsafe {
            EnumChildWindows(
                parent,
                Some(collect_child_window_by_class),
                (&mut search as *mut ChildWindowSearch) as LPARAM,
            );
        }
        (!search.found.is_null()).then_some(search.found)
    }

    unsafe extern "system" fn collect_child_window_by_class(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam as *mut ChildWindowSearch) };
        if class_name(hwnd) == search.class_name {
            search.found = hwnd;
            return 0;
        }
        TRUE
    }

    fn foreground_window() -> Option<WindowInfo> {
        let hwnd = unsafe { GetForegroundWindow() };
        (!hwnd.is_null()).then(|| window_info(hwnd))
    }

    fn wait_for_person_to_front(
        target: &WindowInfo,
        timeout: Duration,
        app: &str,
    ) -> Result<(), CommandError> {
        let started = Instant::now();
        loop {
            if let Some(front) = foreground_window() {
                if same_root(front.hwnd, target.hwnd) {
                    println!(
                        "after_person_front={}",
                        describe_window(&front).replace('\n', " ")
                    );
                    return Ok(());
                }
            }
            if started.elapsed() >= timeout {
                return Err(CommandError::exit1(format!(
                    "{app} was not brought to the foreground by the person"
                )));
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    fn window_info(hwnd: HWND) -> WindowInfo {
        WindowInfo {
            hwnd,
            title: window_title(hwnd),
            process_name: window_process_name(hwnd).unwrap_or_default(),
        }
    }

    fn window_rect(hwnd: HWND) -> Option<RECT> {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        (unsafe { GetWindowRect(hwnd, &mut rect) } != 0
            && rect.right > rect.left
            && rect.bottom > rect.top)
            .then_some(rect)
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

    fn stage_clipboard_text(value: &super::ClipboardCoverText<'_>) -> Result<Instant, String> {
        let utf16 = value
            .as_str()
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
            set_clipboard_bytes(CF_UNICODETEXT as u32, bytes)
        })?;
        Ok(Instant::now())
    }

    fn restore_clipboard(snapshot: &ClipboardSnapshot) -> Result<(), String> {
        with_clipboard(|| {
            if unsafe { EmptyClipboard() } == 0 {
                return Err("clipboard clear failed".to_owned());
            }
            for format in &snapshot.formats {
                set_clipboard_bytes(format.format, &format.bytes)?;
            }
            Ok(())
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

    fn class_name(hwnd: HWND) -> String {
        let mut buffer = [0u16; 256];
        let length = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
        if length <= 0 {
            return String::new();
        }
        OsString::from_wide(&buffer[..length as usize])
            .to_string_lossy()
            .into_owned()
    }

    fn normalize(value: &str) -> String {
        value.to_ascii_lowercase()
    }

    struct ObserverArgs {
        needle: String,
        timeout: Duration,
    }
    struct ClipboardObserver {
        child: Child,
    }
    struct ClipboardObserverReport {
        pid: Option<u32>,
        saw: String,
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
    fn private_canary_chars_reaching_clipboard(canary: &str, observations: &[&str]) -> String {
        canary
            .chars()
            .filter(|private| observations.iter().any(|seen| seen.contains(*private)))
            .collect()
    }

    fn app_is_signal(app: &str) -> bool {
        normalize(app.trim_end_matches(".exe")) == "signal"
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
    println!("clipboard_exposure_ms=0");
    eprintln!("task_3406_place_text requires Windows");
    std::process::exit(2);
}

enum ReadbackComparison {
    Matched,
    DidNotMatch {
        position: usize,
        expected: Option<char>,
        actual: Option<char>,
    },
}

fn compare_readback(expected: &str, actual: &str) -> ReadbackComparison {
    let mut expected_chars = expected.chars();
    let mut actual_chars = actual.chars();
    for position in 1.. {
        match (expected_chars.next(), actual_chars.next()) {
            (Some(left), Some(right)) if left == right => {}
            (None, None) => return ReadbackComparison::Matched,
            (expected, actual) => {
                return ReadbackComparison::DidNotMatch {
                    position,
                    expected,
                    actual,
                }
            }
        }
    }
    unreachable!("unbounded loop returns on match or first differing character")
}

fn printable_char(value: Option<char>) -> String {
    match value {
        Some(ch) => format!("{ch:?}"),
        None => "<end>".to_owned(),
    }
}

fn verify_marked_placement(before: &str, after: &str, mark: &str) -> Result<(), String> {
    if !before.trim().is_empty() {
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
