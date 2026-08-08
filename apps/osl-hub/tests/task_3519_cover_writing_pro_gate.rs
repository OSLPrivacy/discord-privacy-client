use ipc::AppState;
use keystore::{LicenseState, LicenseStateDto};
use osl_privacy_hub::cover_writing_gate::{
    press_cover_writing_button, write_ordinary_cover_message, CoverMessageWriter,
    CoverWritingButton, COVER_WRITING_PRO_REFUSAL,
};

const PRO_IDENTITY: &str = "task3519-pro-identity";
const FREE_IDENTITY: &str = "task3519-free-identity";

fn identity_with_license(license: LicenseState, raw_status: &str) -> AppState {
    let state = AppState::new();
    *state.license_state.lock().expect("license state lock") = LicenseStateDto {
        state: license,
        raw_status: raw_status.to_owned(),
        current_period_end: None,
        last_validated_at: None,
    };
    state
}

#[derive(Default)]
struct CountingWriter {
    chosen_buttons: Vec<CoverWritingButton>,
    ordinary_writes: usize,
}

impl CoverMessageWriter for CountingWriter {
    fn write_chosen_cover(&mut self, button: CoverWritingButton) -> Result<String, String> {
        self.chosen_buttons.push(button);
        Ok(match button {
            CoverWritingButton::Covertext => "word-bank chosen cover message",
            CoverWritingButton::AiCovertext => "local-AI chosen cover message",
        }
        .to_owned())
    }

    fn write_ordinary_cover(&mut self) -> Result<String, String> {
        self.ordinary_writes += 1;
        Ok("ordinary protected-send cover message".to_owned())
    }
}

#[test]
fn task_3519_both_choices_are_pro_only_while_free_keeps_ordinary_cover() {
    let pro = identity_with_license(LicenseState::Paid, "ACTIVE");
    let mut pro_writer = CountingWriter::default();
    let mut pro_messages = Vec::new();

    for button in [
        CoverWritingButton::Covertext,
        CoverWritingButton::AiCovertext,
    ] {
        let message =
            press_cover_writing_button(&pro, button, &mut pro_writer).unwrap_or_else(|error| {
                panic!("{PRO_IDENTITY} could not press {}: {error}", button.label())
            });
        assert!(!message.text.trim().is_empty());
        pro_messages.push(message.text);
    }

    assert_eq!(
        pro_writer.chosen_buttons,
        [
            CoverWritingButton::Covertext,
            CoverWritingButton::AiCovertext,
        ],
        "{PRO_IDENTITY} must reach both chosen writers",
    );
    assert_eq!(pro_messages.len(), 2);

    let free = identity_with_license(LicenseState::Free, "Unconfigured");
    let mut free_writer = CountingWriter::default();
    let mut refusal_names = Vec::new();

    for button in [
        CoverWritingButton::Covertext,
        CoverWritingButton::AiCovertext,
    ] {
        let refusal = match press_cover_writing_button(&free, button, &mut free_writer) {
            Ok(_) => {
                panic!(
                    "{FREE_IDENTITY} reached chosen setting {} without Pro",
                    button.label()
                );
            }
            Err(refusal) => refusal,
        };
        assert_eq!(refusal.code(), COVER_WRITING_PRO_REFUSAL);
        refusal_names.push(format!("{}={}", button.label(), refusal.code()));
    }

    assert_eq!(
        free_writer.chosen_buttons.len(),
        0,
        "{FREE_IDENTITY} reached a chosen cover-writing setting",
    );

    let ordinary = write_ordinary_cover_message(&mut free_writer)
        .unwrap_or_else(|error| panic!("{FREE_IDENTITY} ordinary cover failed: {error}"));
    assert!(!ordinary.text.trim().is_empty());
    assert_eq!(free_writer.ordinary_writes, 1);

    println!("TASK3519 pro_identity={PRO_IDENTITY}");
    println!("TASK3519 pro_cover_messages={}", pro_messages.len());
    println!("TASK3519 pro_buttons=Covertext|AI Covertext");
    println!("TASK3519 free_identity={FREE_IDENTITY}");
    println!("TASK3519 free_refusals={}", refusal_names.join("|"));
    println!(
        "TASK3519 free_chosen_setting_cover_messages={}",
        free_writer.chosen_buttons.len()
    );
    println!(
        "TASK3519 free_ordinary_cover_messages={}",
        free_writer.ordinary_writes
    );
}
