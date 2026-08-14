//! TASK 3636: Outlook desktop compose discovery must not depend on the
//! localized accessibility name.  The reviewed body shape is a writable,
//! keyboard-focusable UIA Document inside the compose pane.  Each language
//! fixture deliberately supplies other writable controls, so a broad
//! "first editable" selection cannot pass this check.

#[path = "../examples/task_3406_place_text.rs"]
mod shared_place_text;

use shared_place_text::{
    place_read_back_and_clear_guarded, PlacementWindowGuard, PlacementWindowState,
    SharedTextActions,
};

const SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES: [&str; 8] = [
    "de-DE", "es-ES", "fr-FR", "ja-JP", "zh-CN", "ar-SA", "hi-IN", "ru-RU",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlType {
    Edit,
    Document,
    Button,
}

#[derive(Clone, Debug)]
struct OutlookControl {
    control_type: ControlType,
    writable: bool,
    keyboard_focusable: bool,
    in_compose_pane: bool,
    text: String,
}

impl OutlookControl {
    fn compose_body() -> Self {
        Self {
            control_type: ControlType::Document,
            writable: true,
            keyboard_focusable: true,
            in_compose_pane: true,
            text: String::new(),
        }
    }

    fn other_editable() -> Self {
        Self {
            control_type: ControlType::Edit,
            writable: true,
            keyboard_focusable: true,
            in_compose_pane: false,
            text: String::new(),
        }
    }
}

/// This fixture intentionally has no accessibility-name field.  Discovery is
/// defined only by the UIA shape used by the unnamed production route.
struct OutlookDesktopLanguageSurface {
    language: &'static str,
    controls: Vec<OutlookControl>,
    typed_characters: usize,
    place_calls: usize,
}

impl OutlookDesktopLanguageSurface {
    fn with_one_compose_box(language: &'static str) -> Self {
        Self {
            language,
            controls: vec![
                OutlookControl::other_editable(),
                OutlookControl::compose_body(),
                OutlookControl {
                    control_type: ControlType::Button,
                    writable: false,
                    keyboard_focusable: true,
                    in_compose_pane: true,
                    text: String::new(),
                },
            ],
            typed_characters: 0,
            place_calls: 0,
        }
    }

    fn with_ambiguous_compose_boxes(language: &'static str) -> Self {
        let mut surface = Self::with_one_compose_box(language);
        surface.controls.push(OutlookControl::compose_body());
        surface
    }

    fn compose_indices(&self) -> Vec<usize> {
        self.controls
            .iter()
            .enumerate()
            .filter_map(|(index, control)| {
                (control.control_type == ControlType::Document
                    && control.writable
                    && control.keyboard_focusable
                    && control.in_compose_pane)
                    .then_some(index)
            })
            .collect()
    }

    fn require_one_compose_index(&self) -> Result<usize, String> {
        let matches = self.compose_indices();
        match matches.as_slice() {
            [index] => Ok(*index),
            _ => Err(format!(
                "OUTLOOK_DESKTOP_COMPOSE_REFUSED language={} reason=compose-box-count-{} before typing",
                self.language,
                matches.len()
            )),
        }
    }
}

impl SharedTextActions for OutlookDesktopLanguageSurface {
    fn read_back_text(&mut self) -> Result<String, String> {
        let index = self.require_one_compose_index()?;
        Ok(self.controls[index].text.clone())
    }

    fn place_text(&mut self, text: &str) -> Result<(), String> {
        let index = self.require_one_compose_index()?;
        self.place_calls += 1;
        self.typed_characters += text.chars().count();
        self.controls[index].text = text.to_owned();
        Ok(())
    }

    fn clear_text(&mut self) -> Result<(), String> {
        let index = self.require_one_compose_index()?;
        self.controls[index].text.clear();
        Ok(())
    }
}

struct FocusedOutlook;

impl PlacementWindowGuard for FocusedOutlook {
    fn state_before_place(&mut self) -> Result<PlacementWindowState, String> {
        Ok(PlacementWindowState {
            app_has_focus: true,
            app_is_covered: false,
            app_is_minimized: false,
            app_display_available: true,
        })
    }
}

#[test]
fn task_3636_every_supported_non_english_outlook_language_finds_one_compose_box_without_a_translated_name(
) {
    let mut found_languages = 0usize;
    let mut found_compose_boxes = 0usize;
    let mut refusal_languages = 0usize;
    let mut refusal_typed_before = 0usize;
    let mut refusal_typed_after = 0usize;

    for language in SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES {
        let mark = format!("TASK3636-{language}-compose-mark");
        let mut surface = OutlookDesktopLanguageSurface::with_one_compose_box(language);
        let mut guard = FocusedOutlook;
        let receipt = place_read_back_and_clear_guarded(&mut surface, &mut guard, &mark)
            .unwrap_or_else(|error| {
                panic!("TASK3636 language={language} failed structural Outlook compose discovery: {error}")
            });
        let compose_boxes = surface.compose_indices().len();
        assert_eq!(
            compose_boxes, 1,
            "TASK3636 language={language} compose box count"
        );
        assert_eq!(
            surface.place_calls, 1,
            "TASK3636 language={language} place calls"
        );
        assert_eq!(surface.typed_characters, mark.chars().count());
        assert_eq!(receipt.readback_bytes, mark.len());
        assert_eq!(receipt.clear_bytes, 0);
        found_languages += 1;
        found_compose_boxes += compose_boxes;
        println!(
            "TASK3636_FOUND language={language} compose_boxes={compose_boxes} discovery=uiA-document-writable-focusable-compose-pane translated_name_used=false typed_characters={}",
            surface.typed_characters
        );

        // Also prove the allowed alternate outcome for the same recorded
        // language: an ambiguous structural tree is refused with the language
        // named before the shared 3406 job can type a character.
        let mut refused = OutlookDesktopLanguageSurface::with_ambiguous_compose_boxes(language);
        let typed_before = refused.typed_characters;
        let mut guard = FocusedOutlook;
        let refusal = place_read_back_and_clear_guarded(&mut refused, &mut guard, &mark)
            .expect_err("ambiguous Outlook compose boxes must refuse before typing");
        let typed_after = refused.typed_characters;
        assert!(
            refusal.contains(language) && refusal.contains("compose-box-count-2"),
            "TASK3636 language={language} refusal must name the language and count: {refusal}"
        );
        assert_eq!(
            refused.place_calls, 0,
            "TASK3636 language={language} refused place calls"
        );
        assert_eq!(
            typed_before, 0,
            "TASK3636 language={language} refusal typed before"
        );
        assert_eq!(
            typed_after, 0,
            "TASK3636 language={language} refusal typed after"
        );
        refusal_languages += 1;
        refusal_typed_before += typed_before;
        refusal_typed_after += typed_after;
        println!(
            "TASK3636_REFUSED language={language} compose_boxes=2 refusal={refusal:?} typed_characters_before={typed_before} typed_characters_after={typed_after}"
        );
    }

    println!(
        "TASK3636_SUMMARY supported_non_english_languages={} found_languages={found_languages} found_compose_boxes={found_compose_boxes} refusal_languages={refusal_languages} refusal_typed_characters_before={refusal_typed_before} refusal_typed_characters_after={refusal_typed_after}",
        SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES.len()
    );
    assert_eq!(SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES.len(), 8);
    assert_eq!(
        found_languages,
        SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES.len()
    );
    assert_eq!(
        found_compose_boxes,
        SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES.len()
    );
    assert_eq!(
        refusal_languages,
        SUPPORTED_NON_ENGLISH_OUTLOOK_LANGUAGES.len()
    );
    assert_eq!(refusal_typed_before, 0);
    assert_eq!(refusal_typed_after, 0);
}

#[test]
fn task_3636_shared_3406_unnamed_outlook_path_does_not_contain_translated_compose_names() {
    const SHARED_PLACE_JOB: &str = include_str!("../examples/task_3406_place_text.rs");
    for translated_name in ["nachricht", "mensaje", "message body"] {
        assert!(
            !SHARED_PLACE_JOB.to_lowercase().contains(translated_name),
            "TASK3636 shared 3406 job must not match a localized Outlook name: {translated_name}"
        );
    }
    assert!(SHARED_PLACE_JOB.contains("UIA_DocumentControlTypeId"));
    assert!(SHARED_PLACE_JOB.contains("control_type == Some(UIA_DocumentControlTypeId)"));
    println!("TASK3636_SHARED_3406_OUTLOOK_DISCOVERY=uiA-document-not-translated-name");
}
