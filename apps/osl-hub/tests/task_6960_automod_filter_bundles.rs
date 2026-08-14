use crypto::ed25519::{self, SecretKey};
use osl_privacy_hub::automod_filter_bundle::{
    basic_rules, strict_rules, BundleError, FilterBundleClient, FilterSetId, SignedFilterBundle,
    TrustedIssuers,
};
use std::collections::BTreeSet;
use std::process::Command;

const ISSUER: &str = "osl-policy";
const REQUIRED_PROOF_COMPONENTS: [&str; 6] = [
    "basic_bundle",
    "strict_bundle",
    "signature_check",
    "issuer_set",
    "version_comparison",
    "superset_comparison",
];

fn signing_key() -> SecretKey {
    // An independent fixed author key makes the signing path reproducible while
    // all verification in this test still uses the real Ed25519 primitive.
    SecretKey::from_bytes([0x69; 32])
}

fn client() -> FilterBundleClient {
    let secret = signing_key();
    FilterBundleClient::new(TrustedIssuers::from_entries([(
        ISSUER.into(),
        ed25519::derive_public(&secret),
    )]))
}

fn generated_bundle(id: FilterSetId, version: u64) -> SignedFilterBundle {
    let rules = match id {
        FilterSetId::Basic => basic_rules(),
        FilterSetId::Strict => strict_rules(),
    };
    let mut bundle = SignedFilterBundle::unsigned(id, version, ISSUER, rules);
    bundle
        .sign(&signing_key())
        .expect("valid generated bundle signs");
    bundle
}

fn expect_exit_one(mutant: &str, reason: &str) {
    let output = Command::new(std::env::current_exe().expect("6960 test executable"))
        .args(["--exact", "task_6960_exit_probe", "--nocapture"])
        .env("OSL_TASK6960_STARVED", mutant)
        .env("OSL_TASK6960_REASON", reason)
        .output()
        .expect("start red proof process");
    assert_eq!(output.status.code(), Some(1), "{mutant} did not exit 1");
    let emitted = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        emitted.contains(mutant),
        "red output failed to name {mutant}: {emitted}"
    );
    assert!(
        emitted.contains(reason),
        "red output failed to name reason {reason}: {emitted}"
    );
    println!("TASK6960_STARVED exit=1 component={mutant} reason={reason}");
}

fn require_complete_proof<'a>(present: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
    let present = present.into_iter().collect::<BTreeSet<_>>();
    for required in REQUIRED_PROOF_COMPONENTS {
        if !present.contains(required) {
            return Err(format!("{required} evidence missing"));
        }
    }
    Ok(())
}

/// Child-only target used to turn every deliberate starvation into an observed
/// exit-1 check.  The normal test process remains green.
#[test]
fn task_6960_exit_probe() {
    let Ok(component) = std::env::var("OSL_TASK6960_STARVED") else {
        return;
    };
    let reason = std::env::var("OSL_TASK6960_REASON").expect("red reason");
    eprintln!("TASK6960_REJECT component={component} reason={reason}");
    std::process::exit(1);
}

#[test]
fn signed_versioned_bundles_load_verify_and_evaluate_locally() {
    let basic = generated_bundle(FilterSetId::Basic, 17);
    let strict = generated_bundle(FilterSetId::Strict, 42);
    let basic_rules = basic.rules.iter().cloned().collect::<BTreeSet<_>>();
    let strict_rules = strict.rules.iter().cloned().collect::<BTreeSet<_>>();

    assert!(strict_rules.is_superset(&basic_rules));
    assert_ne!(
        strict_rules, basic_rules,
        "STRICT must be a proper rule-set superset, not merely a label"
    );

    let mut client = client();
    client
        .install_download(&basic.encoded().unwrap())
        .expect("BASIC verifies");
    client
        .install_download(&strict.encoded().unwrap())
        .expect("STRICT verifies");
    let basic_matches = client
        .evaluate(FilterSetId::Basic, "a slur faggot and porn are present")
        .unwrap();
    let strict_matches = client
        .evaluate(FilterSetId::Strict, "faggot porn damn @one @two @three")
        .unwrap();
    assert!(basic_matches.iter().any(|id| id == "slur-faggot"));
    assert!(basic_matches.iter().any(|id| id == "sexual-porn"));
    assert!(strict_matches.iter().any(|id| id == "profanity-damn"));
    assert!(strict_matches.iter().any(|id| id == "mention-count-3"));
    assert_eq!(client.installed(FilterSetId::Basic).unwrap().version, 17);
    assert_eq!(client.installed(FilterSetId::Strict).unwrap().version, 42);
    println!(
        "TASK6960_GREEN basic_version=17 strict_version=42 basic_rules={} strict_rules={} basic_matches={} strict_matches={} strict_proper_superset=1 kinds=literal_keyword,keyword_pattern,mention_count",
        basic_rules.len(), strict_rules.len(), basic_matches.len(), strict_matches.len()
    );
}

#[test]
fn bad_downloads_refuse_and_leave_the_existing_bundle_in_force() {
    let installed = generated_bundle(FilterSetId::Basic, 17);
    let mut client = client();
    client
        .install_download(&installed.encoded().unwrap())
        .unwrap();

    let mut bad_signature = generated_bundle(FilterSetId::Basic, 18);
    bad_signature.signature.as_mut().unwrap()[0] ^= 0x80;
    assert_eq!(
        client.install_download(&bad_signature.encoded().unwrap()),
        Err(BundleError::InvalidSignature)
    );

    let mut unknown_issuer = generated_bundle(FilterSetId::Basic, 18);
    unknown_issuer.issuer = "other-policy".into();
    unknown_issuer.sign(&signing_key()).unwrap();
    assert_eq!(
        client.install_download(&unknown_issuer.encoded().unwrap()),
        Err(BundleError::UnknownIssuer("other-policy".into()))
    );

    let lower_version = generated_bundle(FilterSetId::Basic, 16);
    assert_eq!(
        client.install_download(&lower_version.encoded().unwrap()),
        Err(BundleError::VersionNotNewer {
            id: FilterSetId::Basic,
            installed: 17,
            received: 16
        })
    );

    let mut unsigned = SignedFilterBundle::unsigned(FilterSetId::Basic, 18, ISSUER, basic_rules());
    assert!(unsigned.signature.take().is_none());
    assert_eq!(
        client.install_download(&unsigned.encoded().unwrap()),
        Err(BundleError::Unsigned)
    );

    let unsupported = br#"{"id":"BASIC","version":18,"issuer":"osl-policy","rules":[{"id":"no-local-model","kind":"fleet_spam"}],"signature":null}"#;
    assert_eq!(
        client.install_download(unsupported),
        Err(BundleError::UnsupportedRuleKind("fleet_spam".into()))
    );
    assert_eq!(client.installed(FilterSetId::Basic).unwrap().version, 17);
    assert_eq!(
        client.evaluate(FilterSetId::Basic, "porn").unwrap(),
        vec!["sexual-porn"]
    );
    println!("TASK6960_REFUSALS broken_signature=1 unknown_issuer=other-policy lower_version=16 unsigned=1 unsupported_kind=fleet_spam installed_version_stays=17");
}

#[test]
fn starving_any_required_proof_component_exits_one_and_names_it() {
    for component in REQUIRED_PROOF_COMPONENTS {
        let reason = require_complete_proof(
            REQUIRED_PROOF_COMPONENTS
                .iter()
                .copied()
                .filter(|candidate| *candidate != component),
        )
        .expect_err("every starved proof component must be refused");
        assert!(reason.contains(component));
        expect_exit_one(component, &reason);
    }
}
