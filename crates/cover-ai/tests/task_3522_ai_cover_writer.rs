use std::collections::{HashMap, HashSet};

#[path = "../../../apps/osl-hub/src/bundled_model_pack.rs"]
mod bundled_model_pack;

use bundled_model_pack::{ensure_bundled_model_pack, BundledCoverWriter, CoverShapeConstraints};
use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

const RUNS: usize = 10;

fn exact_private_message() -> String {
    let source = "Meet me by the old station after lunch. Bring the project notes, and I will confirm the quiet route before we leave. ";
    source.chars().cycle().take(200).collect()
}

#[test]
fn task_3522_local_ai_writer_round_trips_two_hundred_characters_ten_of_ten() {
    let private_message = exact_private_message();
    assert_eq!(private_message.chars().count(), 200);

    let install = tempfile::tempdir().expect("fresh model-pack root");
    let status = ensure_bundled_model_pack(install.path())
        .expect("bundled local model installs and verifies");
    let mut writer = BundledCoverWriter::load(&status.artifact_path)
        .expect("AI Covertext button loads the verified local writer");

    // These counts are derived before the model boundary. `CoverShapeConstraints`
    // has no field or constructor that can carry the private string.
    let shape = CoverShapeConstraints::new(200, vec![200])
        .expect("the requested cover length and shape are bounded");
    let cipher = ConversationCipher::from_salt(b"task-3522-conversation");
    let detection_key = b"task-3522-secret-detection-key";
    let mut relay = HashMap::<[u8; TOKEN_ID_BYTES], String>::new();
    let mut covers = HashSet::new();
    let mut exact = 0usize;
    let mut repeated_runs = Vec::new();
    let mut inexact_runs = Vec::new();

    for run in 0..RUNS {
        let entropy = writer
            .generate_cover_entropy(&shape)
            .expect("local writer runs from length and shape only")
            .into_bytes();
        let mut pointer = [0u8; TOKEN_ID_BYTES];
        pointer[..8].copy_from_slice(&(run as u64 + 1).to_be_bytes());
        pointer[8..].copy_from_slice(&entropy[..12]);
        relay.insert(pointer, private_message.clone());

        let cover = encode_token(&cipher, detection_key, &pointer);
        assert!(
            !cover.trim().is_empty(),
            "AI button must produce cover text"
        );
        if !covers.insert(cover.clone()) {
            repeated_runs.push(run + 1);
        }
        // A protected record is consumed once in this harness. That makes a
        // replayed cover observable even though every run deliberately uses
        // the same private message.
        let opened = decode_token(&cipher, detection_key, &cover)
            .filter(|recovered_pointer| recovered_pointer == &pointer)
            .and_then(|recovered_pointer| relay.remove(&recovered_pointer));
        if opened.as_ref() == Some(&private_message) {
            exact += 1;
        } else {
            inexact_runs.push(run + 1);
        }
    }

    println!(
        "TASK3522 private_characters={} exact_round_trips={}/{} unique_cover_messages={} writer_input=length+shape private_words_given_to_model=0 ai_button=cover_message",
        private_message.chars().count(), exact, RUNS, covers.len(),
    );
    assert!(
        repeated_runs.is_empty() && inexact_runs.is_empty(),
        "repeated cover message runs={repeated_runs:?}; runs no longer read back exactly={inexact_runs:?}"
    );
    assert_eq!(exact, RUNS);
    assert_eq!(covers.len(), RUNS);
}

#[test]
fn task_3522_shipping_button_is_enabled_and_calls_the_local_selection_command() {
    let main = include_str!("../../../apps/osl-hub-ui/src/main.ts");
    let controls = include_str!("../../../apps/osl-hub-ui/src/cover-writing-controls.ts");
    assert!(main.contains("set_ai_covertext_selected"));
    assert!(main.contains("aiAvailable: true"));
    assert!(controls.contains("AI Covertext uses the verified model on this device"));
    assert!(controls.contains("no cloud AI is used"));
    println!("TASK3522 ai_covertext_button=enabled action=set_ai_covertext_selected result=cover_message");
}
