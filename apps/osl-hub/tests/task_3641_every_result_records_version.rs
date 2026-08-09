//! TASK 3641: persist and inspect one automated and one live result for every
//! supported provider.  The test's "live" rows model the live QA boundary's
//! saved verdict shape; no third-party account is contacted from Linux CI.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use osl_privacy_hub::qa_selftest_request::{
    inspect_saved_provider_result, ProviderTestMode, ProviderVersionedTestResult,
    REFUSAL_PROVIDER_NAME_MISSING, REFUSAL_PROVIDER_VERSION_MISSING,
};

const SUPPORTED_PROVIDERS: [(&str, &str); 17] = [
    ("Discord", "1.0.9168"),
    ("Telegram", "5.14.3"),
    ("Signal", "7.60.0"),
    ("WhatsApp", "2.2531.5.0"),
    ("Outlook", "1.2026.707.300"),
    ("Gmail", "Firefox 141.0.3"),
    ("Proton Mail", "Firefox 141.0.3"),
    ("Tuta Mail", "Firefox 141.0.3"),
    ("Yahoo Mail", "Firefox 141.0.3"),
    ("AOL Mail", "Firefox 141.0.3"),
    ("GMX Mail", "Firefox 141.0.3"),
    ("mail.com", "Firefox 141.0.3"),
    ("iCloud Mail", "Firefox 141.0.3"),
    ("Chrome", "127.0.6533.120"),
    ("Edge", "127.0.2651.105"),
    ("Firefox", "141.0.3"),
    ("Brave", "1.68.141"),
];

fn result_filename(provider: &str, mode: ProviderTestMode) -> String {
    let provider = provider
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let mode = match mode {
        ProviderTestMode::Automated => "automated",
        ProviderTestMode::Live => "live",
    };
    format!("{provider}-{mode}.json")
}

#[test]
fn task_3641_every_saved_automated_and_live_result_has_an_exact_provider_version() {
    let directory = tempfile::tempdir().expect("result directory");
    let mut saved_paths = Vec::new();

    for (provider, version) in SUPPORTED_PROVIDERS {
        for mode in [ProviderTestMode::Automated, ProviderTestMode::Live] {
            let result = ProviderVersionedTestResult::new(
                mode,
                format!(
                    "task-3641-{}",
                    match mode {
                        ProviderTestMode::Automated => "automated",
                        ProviderTestMode::Live => "live",
                    }
                ),
                provider,
                version,
                true,
            )
            .unwrap_or_else(|refusal| {
                panic!("TASK3641 provider={provider} mode={mode:?} refused: {refusal}")
            });
            let path = directory.path().join(result_filename(provider, mode));
            fs::write(
                &path,
                serde_json::to_vec(&result).expect("serialize result"),
            )
            .unwrap_or_else(|error| {
                panic!("TASK3641 provider={provider} mode={mode:?} save failed: {error}")
            });
            saved_paths.push((path, provider, version, mode));
        }
    }

    let mut inspected_provider_modes = BTreeSet::new();
    let mut versions_by_provider = BTreeMap::<&str, BTreeSet<&str>>::new();
    for (path, expected_provider, expected_version, mode) in &saved_paths {
        let saved = fs::read_to_string(path).expect("read saved result");
        let inspected = inspect_saved_provider_result(&saved).unwrap_or_else(|refusal| {
            panic!(
                "TASK3641 saved_result={} provider={} mode={mode:?} refusal={refusal}",
                path.display(),
                expected_provider,
            )
        });
        assert_eq!(inspected.provider_name, *expected_provider);
        assert_eq!(inspected.exact_version, *expected_version);
        assert!(
            inspected_provider_modes.insert((*expected_provider, *mode)),
            "duplicate saved result for {expected_provider} {mode:?}"
        );
        versions_by_provider
            .entry(expected_provider)
            .or_default()
            .insert(expected_version);
        println!(
            "TASK3641_RESULT provider={} mode={mode:?} exact_version={} saved_result={}",
            inspected.provider_name,
            inspected.exact_version,
            path.file_name()
                .and_then(|name| name.to_str())
                .expect("UTF-8 filename"),
        );
    }

    assert_eq!(SUPPORTED_PROVIDERS.len(), 17);
    assert_eq!(saved_paths.len(), 34);
    assert_eq!(inspected_provider_modes.len(), 34);
    assert_eq!(versions_by_provider.len(), 17);
    assert!(versions_by_provider
        .values()
        .all(|versions| versions.len() == 1));
    println!(
        "TASK3641_SUPPORTED_PROVIDER_COUNT={}",
        SUPPORTED_PROVIDERS.len()
    );
    println!(
        "TASK3641_AUTOMATED_RESULT_COUNT={}",
        SUPPORTED_PROVIDERS.len()
    );
    println!("TASK3641_LIVE_RESULT_COUNT={}", SUPPORTED_PROVIDERS.len());
    println!("TASK3641_SAVED_RESULT_COUNT={}", saved_paths.len());
    println!(
        "TASK3641_INSPECTED_RESULT_COUNT={}",
        inspected_provider_modes.len()
    );
}

#[test]
fn task_3641_saved_result_missing_provider_or_version_fails_the_inspection() {
    for (saved, expected_refusal) in [
        (r#"{"exactVersion":"1.0.0"}"#, REFUSAL_PROVIDER_NAME_MISSING),
        (
            r#"{"providerName":"Discord"}"#,
            REFUSAL_PROVIDER_VERSION_MISSING,
        ),
        (
            r#"{"providerName":"Discord","exactVersion":"   "}"#,
            REFUSAL_PROVIDER_VERSION_MISSING,
        ),
    ] {
        assert_eq!(inspect_saved_provider_result(saved), Err(expected_refusal));
        println!("TASK3641_REFUSAL={expected_refusal}");
    }
}
