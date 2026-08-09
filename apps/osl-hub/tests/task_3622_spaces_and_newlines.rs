//! TASK 3622: whitespace-only input must either round-trip byte-for-byte or
//! stop before the shared native placement job creates a cover.
//!
//! Discord is the single supported outside message path.  The test compiles
//! the shared TASK 3406 primitive directly, so its counters are the actual
//! read/place/clear boundary rather than a second test-only validator.

#[path = "../examples/task_3406_place_text.rs"]
mod task_3406_place_text;

use task_3406_place_text::{place_read_back_and_clear, SharedTextActions};

const SUPPORTED_OUTSIDE_APPS: [&str; 1] = ["Discord"];
const SPACES_ONLY: &str = "   ";
const NEWLINE_ONLY: &str = "\n";

#[derive(Default)]
struct RecordedComposer {
    current_text: String,
    readbacks: Vec<String>,
    covers: Vec<String>,
    clear_calls: usize,
}

impl SharedTextActions for RecordedComposer {
    fn read_back_text(&mut self) -> Result<String, String> {
        let readback = self.current_text.clone();
        self.readbacks.push(readback.clone());
        Ok(readback)
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        self.current_text = text.to_owned();
        self.covers.push(text.to_owned());
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        self.current_text.clear();
        self.clear_calls += 1;
        Ok(())
    }
}

#[test]
fn task_3622_spaces_and_newlines_are_exact_or_refused_before_placement() {
    let ui_source = include_str!("../../osl-hub-ui/src/main.ts");
    assert!(
        ui_source.contains("const supportedNativeAppIds = new Set<NativeAppId>([\"discord\"]);")
    );

    let mut cases_run = 0usize;
    let mut refused_cases = 0usize;
    for app in SUPPORTED_OUTSIDE_APPS {
        for (case_name, text, expected_refusal) in [
            ("spaces_only", SPACES_ONLY, None),
            (
                "newline_only",
                NEWLINE_ONLY,
                Some("marked text must be one non-empty line"),
            ),
        ] {
            let mut composer = RecordedComposer::default();
            let covers_before = composer.covers.len();
            let result = place_read_back_and_clear(&mut composer, text);
            cases_run += 1;

            match expected_refusal {
                None => {
                    let receipt = result.expect("spaces-only text must be placed and read exactly");
                    assert_eq!(composer.covers.as_slice(), [text]);
                    assert_eq!(composer.readbacks.as_slice(), ["", text, ""]);
                    assert_eq!(receipt.placed_bytes, text.len());
                    assert_eq!(receipt.readback_bytes, text.len());
                    assert_eq!(receipt.clear_bytes, 0);
                    assert_eq!(composer.clear_calls, 1);
                    println!(
                        "TASK3622_CASE app={app} case={case_name} input={text:?} outcome=read_exact readback={text:?} covers_before={covers_before} covers_after={} placed_bytes={} readback_bytes={} clear_bytes={}",
                        composer.covers.len(),
                        receipt.placed_bytes,
                        receipt.readback_bytes,
                        receipt.clear_bytes,
                    );
                }
                Some(expected_refusal) => {
                    let refusal =
                        result.expect_err("newline-only text must refuse before placement");
                    refused_cases += 1;
                    assert_eq!(refusal, expected_refusal);
                    assert!(composer.readbacks.is_empty());
                    assert_eq!(composer.clear_calls, 0);
                    assert_eq!(composer.covers.len(), 0);
                    println!(
                        "TASK3622_CASE app={app} case={case_name} input={text:?} outcome=refused refusal={refusal:?} covers_before={covers_before} covers_after={} read_calls={} clear_calls={}",
                        composer.covers.len(),
                        composer.readbacks.len(),
                        composer.clear_calls,
                    );
                }
            }
        }
    }

    println!(
        "TASK3622_SUMMARY supported_message_paths={} paths={} cases_run={cases_run} refused_cases={refused_cases} spaces_only={SPACES_ONLY:?} newline_only={NEWLINE_ONLY:?}",
        SUPPORTED_OUTSIDE_APPS.len(),
        SUPPORTED_OUTSIDE_APPS.join("|"),
    );
    assert_eq!(cases_run, 2);
    assert_eq!(refused_cases, 1);
}
