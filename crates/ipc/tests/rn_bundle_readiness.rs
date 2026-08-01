//! T19-T08: measure how many stored peer bundles cannot bootstrap OSL-RN.
//!
//! The fixture corpus is kept small and representative: legacy peers, modern
//! peers, and a mixed profile. To measure an unlocked local profile, run:
//!
//! ```text
//! OSL_RN_BUNDLE_READINESS_PEER_MAP=/path/to/peer_map.json \
//!   cargo test -p ipc --test rn_bundle_readiness -- --ignored --nocapture
//! ```
//!
//! The supplied file must be readable by `load_peer_map_from_path`; encrypted
//! profiles therefore need to be exported after unlock rather than copied from
//! disk while locked.

use ipc::peer_map::{load_peer_map_from_path, PeerMap};
use std::path::Path;

#[derive(Debug, PartialEq, Eq)]
struct BundleReadiness {
    total: usize,
    missing_ratchet_initial_pub: usize,
}

fn bundle_readiness(peer_map: &PeerMap) -> BundleReadiness {
    BundleReadiness {
        total: peer_map.len(),
        missing_ratchet_initial_pub: peer_map
            .values()
            .filter(|peer| peer.ik_ratchet_initial_pub.is_none())
            .count(),
    }
}

fn fixture_peer_map(json: &str) -> PeerMap {
    serde_json::from_str(json).expect("peer-map readiness fixture must parse")
}

#[test]
fn corpus_reports_all_stored_bundles_missing_the_ratchet_bootstrap_key() {
    let legacy = fixture_peer_map(
        r#"{
            "legacy-peer": { "osl_user_id": "legacy-peer" }
        }"#,
    );
    let mixed = fixture_peer_map(
        r#"{
            "ready-peer": { "ik_ratchet_initial_pub": "ready" },
            "not-ready-peer": { "osl_user_id": "not-ready-peer" },
            "explicitly-null-peer": { "ik_ratchet_initial_pub": null }
        }"#,
    );
    let current = fixture_peer_map(
        r#"{
            "current-a": { "ik_ratchet_initial_pub": "current-a-key" },
            "current-b": { "ik_ratchet_initial_pub": "current-b-key" }
        }"#,
    );

    let observed = [legacy, mixed, current].iter().map(bundle_readiness).fold(
        BundleReadiness {
            total: 0,
            missing_ratchet_initial_pub: 0,
        },
        |mut aggregate, readiness| {
            aggregate.total += readiness.total;
            aggregate.missing_ratchet_initial_pub += readiness.missing_ratchet_initial_pub;
            aggregate
        },
    );

    assert_eq!(
        observed,
        BundleReadiness {
            total: 6,
            missing_ratchet_initial_pub: 3,
        },
        "T19-T08 corpus measurement changed; report the new exposure to T5"
    );
}

#[test]
#[ignore = "requires an unlocked peer_map.json path in OSL_RN_BUNDLE_READINESS_PEER_MAP"]
fn reports_live_profile_bundle_readiness() {
    let path = std::env::var_os("OSL_RN_BUNDLE_READINESS_PEER_MAP")
        .expect("set OSL_RN_BUNDLE_READINESS_PEER_MAP to an unlocked peer_map.json");
    let path = Path::new(&path);
    let readiness = bundle_readiness(
        &load_peer_map_from_path(path)
            .expect("live peer_map must load after the profile is unlocked"),
    );

    println!(
        "T19-T08 live profile: {}/{} stored peer bundles have ratchet_initial_pub = None ({})",
        readiness.missing_ratchet_initial_pub,
        readiness.total,
        path.display()
    );
}
