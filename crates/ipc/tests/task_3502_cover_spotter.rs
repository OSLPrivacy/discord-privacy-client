use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ipc::commands::{decrypt_osl_phase4_cover, DecodeError};
use keystore::generate_identity;
use stego::{decode_token, encode_token, ConversationCipher, TOKEN_ID_BYTES};

fn pointer(seed: u8) -> [u8; TOKEN_ID_BYTES] {
    let mut value = [0u8; TOKEN_ID_BYTES];
    for (index, byte) in value.iter_mut().enumerate() {
        *byte = seed.wrapping_mul(17).wrapping_add(index as u8);
    }
    value
}

fn ordinary_messages() -> [&'static str; 20] {
    [
        "running five minutes late but i am on my way",
        "can you send the notes after the meeting",
        "coffee machine is working again",
        "i moved the call to three",
        "that build finished without the fixture server",
        "we should pick this up after lunch",
        "the draft is in the shared folder",
        "please ignore the typo in the first line",
        "i will check the logs before i leave",
        "the hallway lights are still on",
        "ship the smaller patch first",
        "that screenshot is from yesterday",
        "i need the invoice number again",
        "the keyboard shortcut changed in this build",
        "let me know when the upload starts",
        "the calendar invite has the wrong room",
        "i found the receipt in downloads",
        "the local server is on another port",
        "we can delete the old branch now",
        "thanks, i will review it tonight",
    ]
}

#[test]
fn task_3502_proves_one_plain_text_cover_spotter() {
    let cipher = ConversationCipher::from_salt(b"task-3502-shipping-cover-setting");
    let detection_key = b"task-3502-detection-key";

    let covers: Vec<([u8; TOKEN_ID_BYTES], String)> = (0..20)
        .map(|seed| {
            let id = pointer(seed);
            (id, encode_token(&cipher, detection_key, &id))
        })
        .collect();

    let pointer_found_count = covers
        .iter()
        .filter(|(expected, text)| {
            decode_token(&cipher, detection_key, text).as_ref() == Some(expected)
        })
        .count();

    let ordinary = ordinary_messages();
    let ordinary_false_positives: Vec<&str> = ordinary
        .iter()
        .copied()
        .filter(|text| decode_token(&cipher, detection_key, text).is_some())
        .collect();
    let ordinary_not_cover_count = ordinary
        .iter()
        .filter(|text| decode_token(&cipher, detection_key, text).is_none())
        .count();

    let mut texts = Vec::with_capacity(40);
    let mut expected_answers = Vec::with_capacity(40);
    for (_, cover) in &covers {
        texts.push(cover.as_str());
        expected_answers.push(true);
    }
    for ordinary in ordinary {
        texts.push(ordinary);
        expected_answers.push(false);
    }

    let app_free_same_answers = texts
        .iter()
        .zip(expected_answers.iter().copied())
        .filter(|(text, expected)| {
            decode_token(&cipher, detection_key, text).is_some() == *expected
        })
        .count();

    println!("TASK3502 shipping_cover_format=marker-free prose-token");
    println!("TASK3502 pointer_found_count={pointer_found_count}");
    println!("TASK3502 ordinary_not_cover_count={ordinary_not_cover_count}");
    println!("TASK3502 app_free_same_answers={app_free_same_answers}");

    assert_eq!(pointer_found_count, 20);
    assert!(
        ordinary_false_positives.is_empty(),
        "ordinary messages wrongly called covers: {:?}",
        ordinary_false_positives
    );
    assert_eq!(ordinary_not_cover_count, 20);
    assert_eq!(app_free_same_answers, 40);

    let recipient = generate_identity("task-3502-bob".to_owned());
    let sender = generate_identity("task-3502-alice".to_owned());
    let mut unknown_version_wire = vec![0u8; 42];
    unknown_version_wire[0] = 0x99;
    let unknown_version_cover = format!("DPC0::{}", STANDARD.encode(unknown_version_wire));

    let error = decrypt_osl_phase4_cover(
        &recipient.x25519_secret,
        &sender.x25519_public,
        &unknown_version_cover,
    )
    .expect_err("an unknown cover wire version must be refused");
    match error {
        DecodeError::UnsupportedVersion { got, expected } => {
            println!(
                "TASK3502 unknown_version_refused_by_name=UnsupportedVersion got=0x{got:02x} expected=0x{expected:02x}"
            );
        }
        other => panic!("unknown version must be refused by name, got {other:?}"),
    }
}
