//! Task 0079 — measure every adjustable-layer combination.
//!
//! The expected ordered table was recorded before this test runs.  This makes
//! a change in cover length visible, while checking that every emitted cover
//! still addresses the exact same 200-character message.

use std::collections::HashMap;

use stego::{
    decode_layered_cover, encode_layered_cover, CoverLayerSettings, LayerStrength,
    LayeredCoverInput, SHRUNK_TOKEN_ID_BYTES, TOKEN_ID_BYTES,
};

const EXPECTED_WORD_COUNTS: [usize; 81] = [
    32, 30, 28, 28, 26, 24, 24, 23, 22, 30, 28, 26, 26, 24, 23, 23, 22, 21, 28, 26,
    24, 24, 23, 22, 22, 21, 20, 30, 27, 26, 26, 24, 22, 22, 21, 20, 27, 25, 24, 24,
    22, 21, 21, 20, 19, 26, 24, 22, 22, 21, 20, 20, 19, 18, 14, 13, 12, 12, 11, 10,
    10, 10, 9, 13, 12, 11, 11, 10, 10, 10, 9, 9, 12, 11, 10, 10, 10, 9, 9, 9, 8,
];

fn original_message() -> String {
    let mut message = String::new();
    while message.chars().count() < 200 {
        message.push_str(
            "private task 0078 message: the original stays in the paired store while every cover layer can be reviewed at a gentler setting. ",
        );
    }
    message.chars().take(200).collect()
}

#[test]
fn task_0079_measure_all_layer_word_counts_and_exact_recovery() {
    let key = b"task-0078-paired-shared-key";
    let original = original_message();
    let seed: [u8; TOKEN_ID_BYTES] = *b"task0078-seed-000001";
    let handle: [u8; SHRUNK_TOKEN_ID_BYTES] = *b"msg0078a";
    let mut seed_store = HashMap::new();
    seed_store.insert(seed, original.clone());
    let mut handle_store = HashMap::new();
    handle_store.insert(handle, original.clone());

    assert_eq!(original.chars().count(), 200);
    let mut row = 0usize;
    let mut word_mismatches = 0usize;
    let mut recovery_mismatches = 0usize;
    let mut best_words = usize::MAX;
    let mut best_settings = None;

    for pointer in LayerStrength::ALL {
        for capitalisation in LayerStrength::ALL {
            for vocabulary in LayerStrength::ALL {
                for spelling in LayerStrength::ALL {
                    let settings =
                        CoverLayerSettings::new(pointer, capitalisation, vocabulary, spelling);
                    let input = match pointer {
                        LayerStrength::High => LayeredCoverInput::SharedHandle(handle),
                        LayerStrength::Off | LayerStrength::Low => LayeredCoverInput::Seed(seed),
                    };
                    let cover = encode_layered_cover(key, input, settings).unwrap();
                    let decoded = decode_layered_cover(key, settings, &cover).unwrap();
                    let recovered = match decoded {
                        LayeredCoverInput::Seed(found_seed) => seed_store.get(&found_seed),
                        LayeredCoverInput::SharedHandle(found_handle) => handle_store.get(&found_handle),
                    };
                    let actual = cover.split_ascii_whitespace().count();
                    let expected = EXPECTED_WORD_COUNTS[row];
                    let recovered_exact = recovered == Some(&original);
                    word_mismatches += usize::from(actual != expected);
                    recovery_mismatches += usize::from(!recovered_exact);
                    if actual < best_words {
                        best_words = actual;
                        best_settings = Some(settings);
                    }
                    println!(
                        "TASK0079 pointer={} capitalisation={} vocabulary={} spelling={} expected_words={} actual_words={} recovered_exact={}",
                        pointer.label(), capitalisation.label(), vocabulary.label(), spelling.label(),
                        expected, actual, recovered_exact,
                    );
                    row += 1;
                }
            }
        }
    }

    let best_settings = best_settings.unwrap();
    println!(
        "TASK0079 message_characters={} settings_checked={} word_count_mismatches={} recovery_mismatches={} best_words={} best_pointer={} best_capitalisation={} best_vocabulary={} best_spelling={}",
        original.chars().count(), row, word_mismatches, recovery_mismatches, best_words,
        best_settings.pointer.label(), best_settings.capitalisation.label(),
        best_settings.vocabulary.label(), best_settings.spelling.label(),
    );
    assert_eq!(row, 81);
    assert_eq!(word_mismatches, 0);
    assert_eq!(recovery_mismatches, 0);
    assert!(best_words <= 10);
}
