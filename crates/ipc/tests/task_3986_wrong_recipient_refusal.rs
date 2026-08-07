use ipc::wire_v2::{decrypt_v3_for_sender, encrypt_v3, RecipientV3, V2Error, MSG_TYPE_CONTENT};

const FIXED_REFUSAL: &str = "This encrypted message could not be opened";
const EXACT_PRIVATE_WORDS: &[u8] = b"TASK 3986 exact private words for Bob only";

fn recipient(identity: &keystore::Identity) -> RecipientV3 {
    RecipientV3 {
        x25519_pub: identity.x25519_public,
        mlkem_pub: identity.mlkem_encapsulation_key(),
    }
}

#[test]
fn task_3986_message_addressed_to_somebody_else_is_refused() {
    let alice = keystore::generate_identity("task-3986-alice".to_owned());
    let bob = keystore::generate_identity("task-3986-real-recipient-bob".to_owned());
    let charlie = keystore::generate_identity("task-3986-wrong-recipient-charlie".to_owned());

    let sealed_for_bob = encrypt_v3(
        &alice.x25519_secret,
        &alice.x25519_public,
        &[recipient(&alice), recipient(&bob)],
        MSG_TYPE_CONTENT,
        EXACT_PRIVATE_WORDS,
    )
    .expect("produce a real sealed message addressed to Bob");

    let wrong_recipient = decrypt_v3_for_sender(
        &sealed_for_bob,
        &charlie.x25519_secret,
        &charlie.mlkem_decapsulation_key(),
        &alice.x25519_public,
    );
    assert!(
        matches!(wrong_recipient, Err(V2Error::NoMatchingSlot)),
        "Charlie must not have a recipient slot in Bob's sealed message: {wrong_recipient:?}"
    );
    let opened_private_messages = usize::from(wrong_recipient.is_ok());
    let refusal = if wrong_recipient.is_ok() {
        "UNEXPECTED_OPEN"
    } else {
        FIXED_REFUSAL
    };
    assert_eq!(opened_private_messages, 0);
    assert_eq!(refusal, FIXED_REFUSAL);
    println!(
        "TASK_3986 wrong_recipient delivered_to=charlie real_recipient=bob opened_private_messages={} refusal=\"{}\"",
        opened_private_messages, refusal
    );

    let opened_by_real_recipient = decrypt_v3_for_sender(
        &sealed_for_bob,
        &bob.x25519_secret,
        &bob.mlkem_decapsulation_key(),
        &alice.x25519_public,
    )
    .expect("Bob opens the same sealed message");
    assert_eq!(opened_by_real_recipient.msg_type, MSG_TYPE_CONTENT);
    let exact_matches = usize::from(opened_by_real_recipient.plaintext == EXACT_PRIVATE_WORDS);
    assert_eq!(exact_matches, 1);
    println!(
        "TASK_3986 real_recipient delivered_to=bob exact_matches={} exact_plaintext=\"{}\" same_message=true",
        exact_matches,
        std::str::from_utf8(&opened_by_real_recipient.plaintext)
            .expect("task 3986 fixture plaintext is UTF-8")
    );

    println!(
        "TASK_3986 summary opened_private_messages={} refusal=\"{}\" real_recipient_exact_matches={}",
        opened_private_messages, refusal, exact_matches
    );
}
