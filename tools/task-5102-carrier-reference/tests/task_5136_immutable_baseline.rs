use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::process::Command;
use task_5102_carrier_reference::baseline::{
    review_payload, signature_hex, BaselineError, BaselineStore, CandidateManifest, ReviewApproval,
    ReviewTrustRoot, UpdateActor,
};

const ROOT_ID: &str = "fixture-release-review-root";
const AUTHOR: &str = "capture-author-alice";
const REVIEWER: &str = "reviewer-bob";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn capture(version: &str, marker: u8) -> (Vec<u8>, Vec<u8>) {
    let png = vec![marker; 128];
    let manifest = serde_json::to_vec_pretty(&json!({
        "schema": "osl-carrier-reference-v1",
        "carrier": "Discord",
        "channel": "stable",
        "version": version,
        "state": "composer-focused-empty",
        "png_sha256": sha(&png)
    }))
    .unwrap();
    (manifest, png)
}

fn approval(
    candidate: &CandidateManifest,
    reviewer: &str,
    key_id: &str,
    key: &SigningKey,
) -> ReviewApproval {
    let payload = review_payload(candidate, reviewer, key_id).unwrap();
    ReviewApproval {
        reviewer: reviewer.to_owned(),
        reviewer_key_id: key_id.to_owned(),
        signature_hex: signature_hex(&key.sign(&payload).to_bytes()),
    }
}

fn tree_bytes(path: &Path) -> Vec<(String, Vec<u8>)> {
    fn visit(root: &Path, at: &Path, output: &mut Vec<(String, Vec<u8>)>) {
        if !at.exists() {
            return;
        }
        let mut entries: Vec<_> = fs::read_dir(at)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                visit(root, &path, output);
            } else {
                output.push((
                    path.strip_prefix(root).unwrap().display().to_string(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }
    let mut output = Vec::new();
    visit(path, path, &mut output);
    output
}

fn assert_zero_baseline_bytes_changed(
    label: &str,
    baselines: &Path,
    operation: impl FnOnce() -> Result<(), BaselineError>,
) {
    let before = tree_bytes(baselines);
    assert!(
        operation().is_err(),
        "{label} unexpectedly advanced the baseline"
    );
    let after = tree_bytes(baselines);
    assert_eq!(after, before, "{label} changed accepted baseline bytes");
    println!("{label}_baseline_bytes_changed=0");
}

#[test]
fn distinct_authorized_signature_advances_once_and_every_automatic_path_changes_zero_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let store = BaselineStore::new(temporary.path());
    let reviewer_key = SigningKey::from_bytes(&[7u8; 32]);
    let wrong_key = SigningKey::from_bytes(&[8u8; 32]);
    let author_key = SigningKey::from_bytes(&[9u8; 32]);
    let trust = ReviewTrustRoot::configured(
        ROOT_ID,
        [(REVIEWER.to_owned(), reviewer_key.verifying_key().to_bytes())],
    );

    // Establish a reviewed parent so the update proves old -> new linkage.
    let (initial_manifest, initial_png) = capture("1.0.9251", 0x25);
    let initial = store
        .create_candidate(&initial_manifest, &initial_png, AUTHOR)
        .unwrap();
    let initial_approval = approval(&initial, REVIEWER, ROOT_ID, &reviewer_key);
    store
        .advance(
            &initial.candidate_hash,
            UpdateActor::ReleaseReviewer,
            Some(&initial_approval),
            &trust,
        )
        .unwrap();
    let old_hash = initial.candidate_hash.clone();

    let (next_manifest, next_png) = capture("1.0.9252", 0x51);
    let candidate = store
        .create_candidate(&next_manifest, &next_png, AUTHOR)
        .unwrap();
    assert_eq!(candidate.parent_hash.as_deref(), Some(old_hash.as_str()));
    let good = approval(&candidate, REVIEWER, ROOT_ID, &reviewer_key);
    let baselines = temporary.path().join("baselines");

    assert_zero_baseline_bytes_changed("unreviewed", &baselines, || {
        store
            .advance(
                &candidate.candidate_hash,
                UpdateActor::ReleaseReviewer,
                None,
                &trust,
            )
            .map(|_| ())
    });
    let self_review = approval(&candidate, AUTHOR, ROOT_ID, &author_key);
    assert_zero_baseline_bytes_changed("self_reviewed", &baselines, || {
        store
            .advance(
                &candidate.candidate_hash,
                UpdateActor::ReleaseReviewer,
                Some(&self_review),
                &trust,
            )
            .map(|_| ())
    });
    let bad_signature = approval(&candidate, REVIEWER, ROOT_ID, &wrong_key);
    assert_zero_baseline_bytes_changed("wrong_key", &baselines, || {
        store
            .advance(
                &candidate.candidate_hash,
                UpdateActor::ReleaseReviewer,
                Some(&bad_signature),
                &trust,
            )
            .map(|_| ())
    });
    for (label, actor) in [
        ("runtime", UpdateActor::RuntimeAdaptation),
        ("diff", UpdateActor::PassingDiff),
        ("capture_author", UpdateActor::CaptureAuthor),
    ] {
        assert_zero_baseline_bytes_changed(label, &baselines, || {
            store
                .advance(&candidate.candidate_hash, actor, Some(&good), &trust)
                .map(|_| ())
        });
    }

    let receipt = store
        .advance(
            &candidate.candidate_hash,
            UpdateActor::ReleaseReviewer,
            Some(&good),
            &trust,
        )
        .unwrap();
    assert_eq!(receipt.old_hash.as_deref(), Some(old_hash.as_str()));
    assert_eq!(receipt.new_hash, candidate.candidate_hash);
    assert_eq!(receipt.capture_author, AUTHOR);
    assert_eq!(receipt.reviewer, REVIEWER);
    assert_eq!(receipt.signature_hex, good.signature_hex);
    assert_eq!(
        store.current_hash(&candidate.name).unwrap(),
        Some(candidate.candidate_hash.clone())
    );

    let release_path = baselines
        .join("objects")
        .join(&candidate.candidate_hash)
        .join("release.manifest.json");
    let release: Value = serde_json::from_slice(&fs::read(release_path).unwrap()).unwrap();
    assert_eq!(release["schema"], "osl-carrier-release-baseline-v1");
    assert_eq!(release["name"]["carrier"], "Discord");
    assert_eq!(release["name"]["channel"], "stable");
    assert_eq!(release["name"]["version"], "1.0.9252");
    assert_eq!(release["name"]["state"], "composer-focused-empty");
    assert_eq!(release["parent_hash"], old_hash);
    assert_eq!(release["baseline_hash"], candidate.candidate_hash);
    assert_eq!(release["capture_author"], AUTHOR);
    assert_eq!(release["reviewer"], REVIEWER);
    assert_eq!(release["reviewer_key_id"], ROOT_ID);
    assert_eq!(release["signature_hex"], good.signature_hex);

    let after_once = tree_bytes(&baselines);
    assert_eq!(
        store.advance(
            &candidate.candidate_hash,
            UpdateActor::ReleaseReviewer,
            Some(&good),
            &trust
        ),
        Err(BaselineError::AlreadyReleased)
    );
    assert_eq!(tree_bytes(&baselines), after_once);
    println!("baseline_advancement_count=1");
    println!("old_hash={}", receipt.old_hash.unwrap());
    println!("new_hash={}", receipt.new_hash);
    println!("capture_author={}", receipt.capture_author);
    println!("reviewer={}", receipt.reviewer);
    println!("signature_hex_length={}", receipt.signature_hex.len());

    // A third candidate invokes the actual command with no reviewer signature.
    let (starved_manifest, starved_png) = capture("1.0.9253", 0x67);
    let starved = store
        .create_candidate(&starved_manifest, &starved_png, "capture-author-carol")
        .unwrap();
    let prior_hash = store.current_hash(&starved.name).unwrap().unwrap();
    let prior_bytes = tree_bytes(&baselines);
    let output = Command::new(env!("CARGO_BIN_EXE_carrier-baseline-review-fixture"))
        .arg(temporary.path())
        .arg(&starved.candidate_hash)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("authorized reviewer signature is required"));
    assert_eq!(
        store.current_hash(&starved.name).unwrap().as_deref(),
        Some(prior_hash.as_str())
    );
    assert_eq!(tree_bytes(&baselines), prior_bytes);
    println!("starved_command_exit=1");
    println!("starved_baseline_bytes_changed=0");
    println!("prior_hash_remains_current={prior_hash}");
}
