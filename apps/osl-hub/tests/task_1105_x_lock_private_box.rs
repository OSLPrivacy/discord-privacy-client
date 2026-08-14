use osl_privacy_hub::web_surface_adapter::x::XFoundBrowserPlaceComposer;
use osl_privacy_hub::x_private_composer::XPrivateComposerBox;
use std::process::Command;

const X_PRIVATE_COMPOSER: &str = env!("CARGO_BIN_EXE_x-private-composer");
const FIXTURE: &str = "X|private|caf\u{e9}|\u{1f512}|1105|fixture|_37";

fn field(output: &str, name: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("TASK1105_{name}=")))
        .unwrap_or_else(|| panic!("missing {name} in {output:?}"))
        .to_owned()
}

#[test]
fn locked_x_private_box_counts_fixture_bytes_and_never_types_into_x() {
    let output = Command::new(X_PRIVATE_COMPOSER)
        .output()
        .expect("run X private-composer fixture");
    assert!(output.status.success(), "fixture failed: {output:?}");
    let output = String::from_utf8(output.stdout).expect("fixture stdout is UTF-8");

    assert_eq!(
        FIXTURE.len(),
        37,
        "fixture must remain a 37-byte UTF-8 probe"
    );
    assert_eq!(field(&output, "LOCK"), "true");
    assert_eq!(field(&output, "PRIVATE_BOX"), "Message");
    assert_eq!(field(&output, "FIXTURE_BYTES"), "37");
    assert_eq!(field(&output, "COUNTER_AFTER_TYPE"), "37");
    assert_eq!(field(&output, "COUNTER_AFTER_CLEAR"), "0");
    assert_eq!(field(&output, "X_COMPOSER_CHARS"), "0");

    println!("TASK1105 fixture_bytes=37 counter_after_type=37");
    println!("TASK1105 counter_after_clear=0 x_composer_chars=0");
}

#[test]
fn refuses_to_put_a_private_box_over_an_unrecognised_composer() {
    let other = XFoundBrowserPlaceComposer {
        browser_title: "Messages / X".to_owned(),
        place_kind: "direct_message".to_owned(),
        composer: "Post".to_owned(),
    };
    assert!(XPrivateComposerBox::lock_over(other).is_err());
}
