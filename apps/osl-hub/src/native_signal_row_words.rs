//! The TASK 4088-winning live route for filling Signal transcript rows with
//! their exact message words.
//!
//! TASK 4088 measured four independent live Windows Automation routes for
//! reading a Signal transcript row's words and found exactly one that
//! returned live text for every row: `AutomationElement.Current.Name` on the
//! row's `ListItem`. This module is the shipping route built on that winner.
//! It reads only `Name`, `ControlType`, `IsOffscreen` and
//! `BoundingRectangle` -- never focus, click, type, submit, or scroll -- and
//! it persists no UI tree.

use crate::signal_message_reader::{
    SignalOpenScreenMessage, SignalOpenScreenSnapshot, SignalOpenScreenSource,
    SignalScreenReadAction,
};

/// One transcript row exactly as read by the TASK 4088 winning live route.
/// Nothing about the row other than its live accessible name is read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalLiveRowCapture {
    pub row_index: usize,
    pub name: String,
}

impl SignalLiveRowCapture {
    pub fn new(row_index: usize, name: impl Into<String>) -> Self {
        Self {
            row_index,
            name: name.into(),
        }
    }
}

/// A row's live accessible name is the only per-row identity this route has;
/// it carries a row's message words, not a distinguishable per-sender
/// identity. Every row handed to the shipping reader therefore gets this one
/// constant, valid sender id, and downstream ownership attribution is left to
/// whatever later gate needs a real one.
pub const SIGNAL_LIVE_ROW_SENDER_ID: &str = "signal-live-transcript-row";

/// Turns rows captured by the live route into the shipping reader's wire
/// type. This is the exact mapping the shipping route ships: every row's
/// trimmed live name becomes that row's message text. A row whose name is
/// empty after trimming carries no words and is dropped rather than handed
/// back as an empty message -- removing this mapping (for example, blanking
/// every row's name before this point) starves every row of its words.
pub fn signal_open_screen_messages_from_live_rows(
    rows: &[SignalLiveRowCapture],
) -> Vec<SignalOpenScreenMessage> {
    rows.iter()
        .filter_map(|row| {
            let text = row.name.trim();
            if text.is_empty() {
                return None;
            }
            Some(SignalOpenScreenMessage::new(
                format!("signal-live-row-{:06}", row.row_index),
                text.to_owned(),
                row.row_index as i64,
                SIGNAL_LIVE_ROW_SENDER_ID,
            ))
        })
        .collect()
}

/// A source of live-captured transcript rows, independent of how those rows
/// were captured. The live Windows implementation below is one such source;
/// tests and the QA harness supply their own.
pub trait SignalLiveRowCaptureSource {
    fn capture_rows(&mut self) -> Result<Vec<SignalLiveRowCapture>, String>;
}

/// The shipping `SignalOpenScreenSource`: one bounded live row capture per
/// read, mapped through the TASK 4088 winning route, and nothing else.
pub struct LiveSignalTranscriptRowWords<Capture> {
    place_id: String,
    capture: Capture,
    action_log: Vec<SignalScreenReadAction>,
}

impl<Capture: SignalLiveRowCaptureSource> LiveSignalTranscriptRowWords<Capture> {
    pub fn new(place_id: impl Into<String>, capture: Capture) -> Self {
        Self {
            place_id: place_id.into(),
            capture,
            action_log: Vec::new(),
        }
    }
}

impl<Capture: SignalLiveRowCaptureSource> SignalOpenScreenSource
    for LiveSignalTranscriptRowWords<Capture>
{
    fn read_open_screen(&mut self) -> Result<SignalOpenScreenSnapshot, String> {
        let rows = self.capture.capture_rows()?;
        let messages = signal_open_screen_messages_from_live_rows(&rows);
        self.action_log.push(SignalScreenReadAction::ReadOpenScreen);
        Ok(SignalOpenScreenSnapshot::new(
            self.place_id.clone(),
            messages,
        ))
    }

    fn action_log(&self) -> &[SignalScreenReadAction] {
        &self.action_log
    }
}

#[cfg(target_os = "windows")]
pub use windows::LiveSignalWindowRowCapture;

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        UIA_ListItemControlTypeId,
    };

    const MAX_ROW_WORDS_NODES: usize = 4_096;
    const MAX_ROW_WORDS_DEPTH: usize = 64;

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// An already-claimed live Signal window this route is allowed to read
    /// transcript rows from.
    pub struct LiveSignalWindowRowCapture {
        window: isize,
        process_id: u32,
        process_is_trusted: Box<dyn Fn(u32) -> bool + Send>,
    }

    impl LiveSignalWindowRowCapture {
        pub fn new(
            window: isize,
            process_id: u32,
            process_is_trusted: impl Fn(u32) -> bool + Send + 'static,
        ) -> Self {
            Self {
                window,
                process_id,
                process_is_trusted: Box::new(process_is_trusted),
            }
        }
    }

    impl SignalLiveRowCaptureSource for LiveSignalWindowRowCapture {
        fn capture_rows(&mut self) -> Result<Vec<SignalLiveRowCapture>, String> {
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let _com = ComGuard(initialized.is_ok());
            let automation: IUIAutomation =
                unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                    .map_err(|error| format!("Signal UIA automation unavailable: {error}"))?;
            let root = unsafe { automation.ElementFromHandle(HWND(self.window as _)) }
                .map_err(|error| format!("Signal UIA window root unavailable: {error}"))?;
            let root_process_id = unsafe { root.CurrentProcessId() }
                .map_err(|error| format!("Signal UIA root process id unavailable: {error}"))?;
            if root_process_id <= 0
                || root_process_id as u32 != self.process_id
                || !(self.process_is_trusted)(root_process_id as u32)
            {
                return Err("Signal UIA window root is not the trusted claimed process".to_owned());
            }
            let walker = unsafe { automation.RawViewWalker() }
                .map_err(|error| format!("Signal UIA tree walker unavailable: {error}"))?;
            let mut rows = Vec::new();
            let mut stack = vec![(root, 0usize)];
            let mut visited = 0usize;
            while let Some((element, depth)) = stack.pop() {
                visited += 1;
                if visited > MAX_ROW_WORDS_NODES {
                    return Err("Signal transcript row walk exceeded its node budget".to_owned());
                }
                if depth >= MAX_ROW_WORDS_DEPTH {
                    return Err("Signal transcript row walk exceeded its depth budget".to_owned());
                }
                let control_type = unsafe { element.CurrentControlType() }
                    .map_err(|error| format!("Signal UIA control type unavailable: {error}"))?;
                if control_type == UIA_ListItemControlTypeId {
                    let is_offscreen = unsafe { element.CurrentIsOffscreen() }
                        .map_err(|error| format!("Signal UIA offscreen flag unavailable: {error}"))?
                        .as_bool();
                    let bounds = unsafe { element.CurrentBoundingRectangle() }
                        .map_err(|error| format!("Signal UIA row bounds unavailable: {error}"))?;
                    if !is_offscreen && bounds.right > bounds.left && bounds.bottom > bounds.top {
                        let name = unsafe { element.CurrentName() }
                            .map_err(|error| format!("Signal UIA row name unavailable: {error}"))?;
                        rows.push(SignalLiveRowCapture::new(rows.len(), name.to_string()));
                    }
                }
                for child in child_elements(&walker, &element)? {
                    stack.push((child, depth + 1));
                }
            }
            Ok(rows)
        }
    }

    fn child_elements(
        walker: &IUIAutomationTreeWalker,
        parent: &IUIAutomationElement,
    ) -> Result<Vec<IUIAutomationElement>, String> {
        let mut children = Vec::new();
        // UI Automation denotes the end of a sibling list with a successful
        // call and a null COM pointer, which the `windows` 0.56 projection
        // surfaces as an empty (S_OK) error rather than `Option<..>`.
        let mut current = match unsafe { walker.GetFirstChildElement(parent) } {
            Ok(element) => Some(element),
            Err(error) if error.code().0 == 0 => None,
            Err(error) => return Err(format!("Signal UIA first child unavailable: {error}")),
        };
        while let Some(element) = current {
            if children.len() >= MAX_ROW_WORDS_NODES {
                return Err("Signal transcript row walk exceeded its sibling budget".to_owned());
            }
            current = match unsafe { walker.GetNextSiblingElement(&element) } {
                Ok(next) => Some(next),
                Err(error) if error.code().0 == 0 => None,
                Err(error) => return Err(format!("Signal UIA next sibling unavailable: {error}")),
            };
            children.push(element);
        }
        Ok(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedRowCapture(Vec<SignalLiveRowCapture>);

    impl SignalLiveRowCaptureSource for FixedRowCapture {
        fn capture_rows(&mut self) -> Result<Vec<SignalLiveRowCapture>, String> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn fills_every_row_with_its_trimmed_live_name() {
        let rows = (0..10)
            .map(|index| SignalLiveRowCapture::new(index, format!("  word-{index}  ")))
            .collect::<Vec<_>>();
        let messages = signal_open_screen_messages_from_live_rows(&rows);
        assert_eq!(messages.len(), 10);
        assert!(messages.iter().all(|message| !message.text.is_empty()));
        for (index, message) in messages.iter().enumerate() {
            assert_eq!(message.text, format!("word-{index}"));
        }
    }

    #[test]
    fn drops_rows_whose_live_name_is_blank_rather_than_hand_back_empty_text() {
        let rows = vec![
            SignalLiveRowCapture::new(0, "hello"),
            SignalLiveRowCapture::new(1, "   "),
            SignalLiveRowCapture::new(2, ""),
            SignalLiveRowCapture::new(3, "world"),
        ];
        let messages = signal_open_screen_messages_from_live_rows(&rows);
        assert_eq!(messages.len(), 2);
        assert!(messages.iter().all(|message| !message.text.is_empty()));
    }

    #[test]
    fn removing_the_winning_route_starves_every_row_of_its_words() {
        // Simulates removing the shipping winning text route: every row's
        // name arrives blank, as it would if `row.name` were no longer read.
        let starved_rows = (0..10)
            .map(|index| SignalLiveRowCapture::new(index, ""))
            .collect::<Vec<_>>();
        let messages = signal_open_screen_messages_from_live_rows(&starved_rows);
        assert!(messages.is_empty());
    }

    #[test]
    fn read_open_screen_reports_only_one_read_action_and_no_key_presses() {
        let rows = (0..3)
            .map(|index| SignalLiveRowCapture::new(index, format!("row {index}")))
            .collect::<Vec<_>>();
        let mut source = LiveSignalTranscriptRowWords::new("place-1", FixedRowCapture(rows));
        let snapshot = source.read_open_screen().expect("read open screen");
        assert_eq!(snapshot.messages.len(), 3);
        assert_eq!(source.action_log(), [SignalScreenReadAction::ReadOpenScreen]);
        assert_eq!(
            source
                .action_log()
                .iter()
                .filter(|action| matches!(action, SignalScreenReadAction::KeyPress { .. }))
                .count(),
            0
        );
    }
}
