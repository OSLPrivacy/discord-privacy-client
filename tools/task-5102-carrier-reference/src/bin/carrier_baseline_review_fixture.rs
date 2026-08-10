//! Deterministic command fixture for the fail-closed release-review boundary.

use ed25519_dalek::SigningKey;
use std::path::PathBuf;
use task_5102_carrier_reference::baseline::{BaselineStore, ReviewTrustRoot, UpdateActor};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: carrier-baseline-review-fixture ROOT CANDIDATE_HASH");
        std::process::exit(2);
    };
    let Some(candidate_hash) = args.next().and_then(|arg| arg.into_string().ok()) else {
        eprintln!("usage: carrier-baseline-review-fixture ROOT CANDIDATE_HASH");
        std::process::exit(2);
    };
    if args.next().is_some() {
        eprintln!("too many arguments");
        std::process::exit(2);
    }
    let reviewer_key = SigningKey::from_bytes(&[7u8; 32]);
    let trust = ReviewTrustRoot::configured(
        "fixture-release-review-root",
        [(
            "reviewer-bob".to_owned(),
            reviewer_key.verifying_key().to_bytes(),
        )],
    );
    let store = BaselineStore::new(root);
    match store.advance(&candidate_hash, UpdateActor::ReleaseReviewer, None, &trust) {
        Ok(_) => println!("unexpected advance"),
        Err(error) => {
            eprintln!("release baseline refused: {error}");
            std::process::exit(1);
        }
    }
}
