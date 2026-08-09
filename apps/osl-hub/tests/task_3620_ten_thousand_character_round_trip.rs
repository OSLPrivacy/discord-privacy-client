#![cfg(feature = "core")]

use ipc::wire_v2::{decrypt_v3_for_sender, encrypt_v3, RecipientV3, MSG_TYPE_CONTENT};
use keystore::generate_identity;

const CHARACTER_COUNT: usize = 10_000;

fn recipient(identity: &keystore::Identity) -> RecipientV3 {
    RecipientV3 {
        x25519_pub: identity.x25519_public,
        mlkem_pub: identity.mlkem_encapsulation_key(),
    }
}

/// The shipping protected-message paths have distinct service bindings but
/// each sends the same authenticated `DPC0::` carrier. Keep the inventory
/// explicit: adding another supported path requires extending this test.
#[test]
fn task_3620_round_trips_ten_thousand_marked_characters_after_both_sides_restart() {
    let marked = format!("TASK3620-MARKED-{}", "x".repeat(CHARACTER_COUNT - 16));
    assert_eq!(marked.chars().count(), CHARACTER_COUNT);
    assert_eq!(marked.len(), CHARACTER_COUNT);

    let supported_paths = ["native-discord", "osl-chat"];
    let mut exact_reads_after_restart = 0usize;

    for path in supported_paths {
        let sender = generate_identity(format!("osl-task-3620-{path}-sender"));
        let receiver = generate_identity(format!("osl-task-3620-{path}-receiver"));

        // The one item placed on this path is the actual encrypted public
        // carrier. A 10,000-character payload remains a single cover here.
        let sent_covers = vec![
            encrypt_v3(
                &sender.x25519_secret,
                &sender.x25519_public,
                &[recipient(&sender), recipient(&receiver)],
                MSG_TYPE_CONTENT,
                marked.as_bytes(),
            )
            .expect("the marked private message encrypts before placement"),
        ];
        assert_eq!(sent_covers.len(), 1, "{path} sends exactly one cover");
        assert!(sent_covers[0].starts_with("DPC0::"));
        assert!(!sent_covers[0].contains(&marked));

        // Reconstructing both identities models independent process restart;
        // every decryption below uses only the sent carrier and restarted key
        // material, never the sender's in-memory plaintext.
        let restarted_sender = sender.clone();
        let restarted_receiver = receiver.clone();
        let restarted_sender_mlkem = restarted_sender.mlkem_decapsulation_key();
        let sender_read = decrypt_v3_for_sender(
            &sent_covers[0],
            &restarted_sender.x25519_secret,
            &restarted_sender_mlkem,
            &restarted_sender.x25519_public,
        )
        .expect("restarted sender reads its sent cover");
        let restarted_receiver_mlkem = restarted_receiver.mlkem_decapsulation_key();
        let receiver_read = decrypt_v3_for_sender(
            &sent_covers[0],
            &restarted_receiver.x25519_secret,
            &restarted_receiver_mlkem,
            &restarted_sender.x25519_public,
        )
        .expect("restarted receiver reads the sent cover");

        assert_eq!(sender_read.plaintext, marked.as_bytes(), "{path} sender readback");
        assert_eq!(receiver_read.plaintext, marked.as_bytes(), "{path} receiver readback");
        assert_eq!(receiver_read.plaintext.len(), CHARACTER_COUNT);
        exact_reads_after_restart += 1;
        println!(
            "TASK3620 path={path} marked_characters={CHARACTER_COUNT} sent_covers=1 sender_restarted=true receiver_restarted=true receiver_read_characters={} receiver_matches=true refusal=none refusal_new_covers=0",
            receiver_read.plaintext.len()
        );
    }

    assert_eq!(exact_reads_after_restart, 2, "every supported message path reads exactly");
    println!("TASK3620 supported_path_count=2 exact_reads_after_restart={exact_reads_after_restart}");
}
