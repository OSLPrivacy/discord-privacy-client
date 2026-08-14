//! Task 0078 — one control surface dials every cover layer back.
//!
//! This is intentionally a command-shaped integration test: the printed
//! rows are the direct evidence an operator gets from `cargo test ... --nocapture`.

use std::collections::{HashMap, HashSet};

use stego::{
    decode_layered_cover, encode_layered_cover, CoverLayerSettings, LayerStrength,
    LayeredCoverInput, SHRUNK_TOKEN_ID_BYTES, TOKEN_ID_BYTES,
};

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
fn task_0078_direct_settings_command_dials_every_layer_and_reads_back() {
    let mac_key = b"task-0078-paired-shared-key";
    let original = original_message();
    let seed: [u8; TOKEN_ID_BYTES] = *b"task0078-seed-000001";
    let handle: [u8; SHRUNK_TOKEN_ID_BYTES] = *b"msg0078a";
    let mut seed_store = HashMap::new();
    seed_store.insert(seed, original.clone());
    let mut handle_store = HashMap::new();
    handle_store.insert(handle, original.clone());
    let mut counts = HashSet::new();
    let mut rows = 0usize;

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
                    let cover = encode_layered_cover(mac_key, input, settings)
                        .expect("settings and matching pointer representation encode");
                    let decoded = decode_layered_cover(mac_key, settings, &cover)
                        .expect("the settings-selected cover reads back");
                    let recovered = match decoded {
                        LayeredCoverInput::Seed(seed) => seed_store.get(&seed),
                        LayeredCoverInput::SharedHandle(handle) => handle_store.get(&handle),
                    }
                    .expect("decoded pointer addresses the original message");
                    let words = cover.split_ascii_whitespace().count();
                    counts.insert(words);
                    rows += 1;
                    println!(
                        "TASK0078 command pointer={} capitalisation={} vocabulary={} spelling={} word_count={} recovered_exact={}",
                        pointer.label(),
                        capitalisation.label(),
                        vocabulary.label(),
                        spelling.label(),
                        words,
                        recovered == &original,
                    );
                    assert_eq!(recovered, &original);
                }
            }
        }
    }

    let mut distinct_counts: Vec<usize> = counts.into_iter().collect();
    distinct_counts.sort_unstable();
    println!("TASK0078 original_message={original}");
    println!("TASK0078 settings_checked={rows}");
    println!("TASK0078 distinct_word_counts={distinct_counts:?}");

    assert_eq!(
        rows, 81,
        "every four-layer off/low/high setting is commandable"
    );
    assert!(
        distinct_counts.len() >= 9,
        "the settings must produce at least nine different word counts, got {distinct_counts:?}"
    );
}
