use ed25519_dalek::SigningKey;
use task_3096_build_proof::{
    check_build_proof, make_build_proof, sign_build_proof, verify_signed_build_proof,
    BuildProofCheck, BuildProofInput, SignedBuildProof,
};

const ORIGINAL: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const CHANGED: &str = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
const CHECKED_AT: u64 = 1_787_000_000;

fn signed_reply(fingerprint: &str, signing_seed: [u8; 32]) -> SignedBuildProof {
    let proof = make_build_proof(BuildProofInput {
        build_fingerprint: fingerprint.to_owned(),
        device_id: "device:qa-laptop-3218".to_owned(),
        person_id: "friend:river-3218".to_owned(),
        made_at_unix_seconds: 1_786_000_000,
        stops_counting_at_unix_seconds: 1_788_000_000,
    })
    .expect("make valid check reply proof");
    sign_build_proof(proof, signing_seed).expect("sign check reply proof")
}

fn friend_row_build_mark_count(answer: BuildProofCheck) -> usize {
    usize::from(answer == BuildProofCheck::Unmodified)
}

#[test]
fn attack_34_changed_or_missing_tick_reply_never_paints_the_build_mark() {
    let signing_seed = [34_u8; 32];
    let trusted_public_key = SigningKey::from_bytes(&signing_seed)
        .verifying_key()
        .to_bytes();

    // First run: a valid signed reply yields `modified`. Catch its serialized
    // bytes and change the authenticated fingerprint to the observed one --
    // the sole data change that would turn the check result into `unmodified`
    // if the signature boundary were absent.
    let authentic_modified_reply = signed_reply(CHANGED, signing_seed);
    let before_attack = check_build_proof(
        Some(&authentic_modified_reply),
        Some(trusted_public_key),
        ORIGINAL,
        CHECKED_AT,
    );
    assert_eq!(before_attack, BuildProofCheck::Modified);

    let mut intercepted_json =
        serde_json::to_value(&authentic_modified_reply).expect("serialize tick reply");
    assert_eq!(intercepted_json["proof"]["buildFingerprint"], CHANGED);
    intercepted_json["proof"]["buildFingerprint"] = ORIGINAL.into();
    let changed_reply: SignedBuildProof =
        serde_json::from_value(intercepted_json).expect("changed reply remains valid JSON");

    let changed_refusal = verify_signed_build_proof(&changed_reply, trusted_public_key)
        .expect_err("changed reply must be refused");
    let changed_answer = check_build_proof(
        Some(&changed_reply),
        Some(trusted_public_key),
        ORIGINAL,
        CHECKED_AT,
    );
    let changed_mark_count = friend_row_build_mark_count(changed_answer);
    assert_eq!(changed_refusal, "bad-signature");
    assert_eq!(changed_answer, BuildProofCheck::CannotTell);
    assert_eq!(changed_mark_count, 0);

    // Second run: the reply is removed entirely. The gate's total checker
    // reads absence as `cannot tell`, never as a positive build result.
    let missing_answer = check_build_proof(None, Some(trusted_public_key), ORIGINAL, CHECKED_AT);
    let missing_mark_count = friend_row_build_mark_count(missing_answer);
    assert_eq!(missing_answer, BuildProofCheck::CannotTell);
    assert_eq!(missing_mark_count, 0);

    // Untouched control run: a correctly signed matching reply earns exactly
    // one mark on the friend row.
    let good_reply = signed_reply(ORIGINAL, signing_seed);
    verify_signed_build_proof(&good_reply, trusted_public_key)
        .expect("good reply signature verifies");
    let good_answer = check_build_proof(
        Some(&good_reply),
        Some(trusted_public_key),
        ORIGINAL,
        CHECKED_AT,
    );
    let good_mark_count = friend_row_build_mark_count(good_answer);
    assert_eq!(good_answer, BuildProofCheck::Unmodified);
    assert_eq!(good_mark_count, 1);

    println!(
        "TASK3218_CHANGED_REPLY before_attack=\"{}\" attempted_answer=\"unmodified\" after_attack=\"{}\" refusal_reason={} friend_row_build_mark_count={}",
        before_attack, changed_answer, changed_refusal, changed_mark_count,
    );
    println!(
        "TASK3218_MISSING_REPLY reply_count=0 answer=\"{}\" friend_row_build_mark_count={}",
        missing_answer, missing_mark_count,
    );
    println!(
        "TASK3218_GOOD_REPLY reply_count=1 answer=\"{}\" friend_row_build_mark_count={}",
        good_answer, good_mark_count,
    );
}
