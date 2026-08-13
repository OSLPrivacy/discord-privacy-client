#![cfg(feature = "core")]

//! TASK 6804 — the native half of "load a recovery-kit file on both recovery
//! pages", run against real accounts and real files on disk.
//!
//! Two phases, because the kit that gets loaded has to be exported by something
//! other than the code that loads it. `phase1_export_source_material` creates a
//! real OSL account and hands its two recovery phrases to the check script;
//! the check script writes the `.oslkit` files with its *own* encoder (see
//! `apps/osl-hub-ui/scripts/check-recovery-kit-upload-6804.ts`), and
//! `phase2_journeys_and_refusals` then loads those files the way the chip does
//! and runs each page's real journey to the end.
//!
//! What is real here and what is not, stated plainly so the evidence does not
//! overclaim:
//!
//! * Real: the account, both recovery phrases, the exported files, the single
//!   frozen read, the SHA-256 of the selected bytes, every validation and
//!   refusal, the identity comparison, `import_native_identity_phrase` on a
//!   profile that has never seen the account, `reset_main_password_after_recovery`
//!   on the profile that holds it, and a before/after byte census of that
//!   profile across every refusal.
//! * Not real on this host: the *Windows* dialog. `blocking_pick_file` needs a
//!   parented Win32 window and a `desktop`-feature build, and `src/main.rs`
//!   cannot be compiled on Linux at all. The picker is therefore a port
//!   (`RecoveryKitPicker`); the production implementation is
//!   `src/recovery_kit_picker.rs`, and the check asserts by source that the
//!   chip reaches it and that it reaches the installed dialog. That assertion
//!   is a wiring proof, not an execution proof, and is labelled as such.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::password_lifecycle;
use osl_privacy_hub::recovery_kit_file::{
    derived_account_id, load_frozen_recovery_kit, load_recovery_kit_through_picker,
    FrozenKitSelection, LoadedRecoveryKit, RecoveryKitPage, RecoveryKitPicker, RecoveryKitRefusal,
    RECOVERY_KIT_WORDS,
};

const SOURCE_PASSWORD: &str = "aB3!z9-task-6804-source-password";
const REPLACEMENT_PASSWORD: &str = "aB3!z9-task-6804-replacement";

/// A valid BIP-39 mnemonic that is not the source account's. Used to build the
/// "wrong identity" kit without installing a second account.
const FOREIGN_IDENTITY_PHRASE: &str = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
/// Twelve lowercase words. A kit's password phrase only has to be that.
const FOREIGN_PASSWORD_PHRASE: &str =
    "legal winner thank year wave sausage worth useful legal winner thank yellow";

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(
        std::env::var(name)
            .unwrap_or_else(|_| panic!("{name} must name a path; run this through the 6804 check")),
    )
}

fn starve(stage: &str) -> bool {
    std::env::var("OSL6804_STARVE").ok().as_deref() == Some(stage)
}

fn page_starved(page: RecoveryKitPage) -> bool {
    starve("page") || starve(page.id())
}

/// The picker port, standing in for the Windows dialog. `None` is a cancel.
struct StubPicker(Option<PathBuf>);

impl RecoveryKitPicker for StubPicker {
    fn pick(&self) -> Result<Option<PathBuf>, RecoveryKitRefusal> {
        Ok(self.0.clone())
    }
}

/// Every regular file under `root`, with its SHA-256. The "account bytes" a
/// refusal is forbidden to change.
fn byte_census(root: &Path) -> BTreeMap<String, String> {
    let mut census = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(entry) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&entry) else {
            continue;
        };
        for child in read.flatten() {
            let path = child.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(bytes) = std::fs::read(&path) {
                census.insert(
                    path.strip_prefix(root).unwrap_or(&path).display().to_string(),
                    osl_privacy_hub::recovery_kit_file::sha256_hex(&bytes),
                );
            }
        }
    }
    census
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Phase 1. Build the source account and publish exactly what an independent
/// exporter needs. Nothing here reads a kit.
#[test]
fn phase1_export_source_material() {
    let fixtures = env_path("OSL6804_FIXTURES");
    std::fs::create_dir_all(&fixtures).expect("create the fixture directory");
    let profile = fixtures.join("profile-source");
    std::fs::create_dir_all(&profile).expect("create the source profile directory");

    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(profile.clone()));
    let state = HubCoreState::bootstrap_from_disk();
    // Bootstrap must see an actually empty profile; a keyserver user id is a
    // legacy identity seed. Install the dead offline route only after that
    // first read, exactly as `first_run_account_creation` does.
    std::fs::write(
        profile.join("keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:1","user_id":"task-6804-probe"}"#,
    )
    .expect("write the refused keyserver route");
    let created = password_lifecycle::create_native_identity(
        &state,
        Some(password_lifecycle::IdentityCreationOwnerAuthorization::ExplicitOwnerSignoff),
    )
    .expect("the source account can be created on an empty profile");
    let identity_phrase = created
        .identity_recovery_phrase
        .clone()
        .expect("account creation hands back the identity phrase once");
    let setup = password_lifecycle::setup_main_password(&state, SOURCE_PASSWORD.to_owned())
        .expect("the source account's first password can be set");
    let password_phrase = setup.password_recovery_phrase.clone();

    for phrase in [&identity_phrase, &password_phrase] {
        assert_eq!(
            phrase.split(' ').count(),
            RECOVERY_KIT_WORDS,
            "a kit holds twelve numbered words per phrase"
        );
        assert!(
            phrase.split(' ').all(|word| !word.is_empty()
                && word.bytes().all(|byte| byte.is_ascii_lowercase())),
            "kit phrases are lowercase words"
        );
    }
    assert_eq!(
        derived_account_id(&identity_phrase).expect("the source phrase derives an account id"),
        created.user_id,
        "the account id in a kit must be the one its identity phrase derives to"
    );

    let foreign_user_id =
        derived_account_id(FOREIGN_IDENTITY_PHRASE).expect("the foreign phrase derives an id");
    assert_ne!(foreign_user_id, created.user_id);

    let document = format!(
        "{{\"userId\":{},\"identityPhrase\":{},\"passwordPhrase\":{},\"foreignUserId\":{},\"foreignIdentityPhrase\":{},\"foreignPasswordPhrase\":{},\"sourceProfile\":{},\"sourcePassword\":{}}}\n",
        json_string(&created.user_id),
        json_string(&identity_phrase),
        json_string(&password_phrase),
        json_string(&foreign_user_id),
        json_string(FOREIGN_IDENTITY_PHRASE),
        json_string(FOREIGN_PASSWORD_PHRASE),
        json_string(&profile.display().to_string()),
        json_string(SOURCE_PASSWORD),
    );
    std::fs::write(fixtures.join("source-material.json"), document)
        .expect("publish the source material for the independent exporter");

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(None);
    keystore::set_active_account_dir(None);
    println!("TASK6804_PHASE1 account_created=1 phrases=2");
}

struct Outcome {
    tag: String,
    path: String,
    selected_sha256: Option<String>,
    validated_sha256: Option<String>,
    account_id: Option<String>,
    words: Vec<String>,
    refusal: Option<&'static str>,
    refusal_message: Option<&'static str>,
}

fn run_selection(
    page: RecoveryKitPage,
    tag: &str,
    selection: Option<PathBuf>,
    expected_account_id: Option<&str>,
) -> Outcome {
    if page_starved(page) || starve("picker") {
        // A disconnected picker is indistinguishable from cancellation at the
        // page boundary. The acceptance runner still expects the valid kit to
        // arrive, so this makes the starvation red without reading any file.
        return Outcome {
            tag: tag.to_owned(),
            path: String::new(),
            selected_sha256: None,
            validated_sha256: None,
            account_id: None,
            words: Vec::new(),
            refusal: None,
            refusal_message: None,
        };
    }
    if starve("identity-comparison") && tag == "wrong-identity" {
        // Test-only removal of the identity-comparison report. The runner
        // treats a kit accepted without this explicit refusal as red.
        return Outcome {
            tag: tag.to_owned(),
            path: selection
                .as_ref()
                .map(|value| value.display().to_string())
                .unwrap_or_default(),
            selected_sha256: None,
            validated_sha256: None,
            account_id: None,
            words: Vec::new(),
            refusal: None,
            refusal_message: None,
        };
    }
    let picker = StubPicker(selection.clone());
    let path = selection
        .as_ref()
        .map(|value| value.display().to_string())
        .unwrap_or_default();
    let expected_account_id = if starve("identity-comparison") {
        // Test-only starvation: remove the page's existing-account binding.
        // The independently written wrong-identity kit then exposes that the
        // check really consumes the comparison outcome.
        None
    } else {
        expected_account_id
    };
    match load_recovery_kit_through_picker(page, &picker, expected_account_id) {
        Ok(None) => Outcome {
            tag: tag.to_owned(),
            path,
            selected_sha256: None,
            validated_sha256: None,
            account_id: None,
            words: Vec::new(),
            refusal: None,
            refusal_message: None,
        },
        Ok(Some(loaded)) => Outcome {
            tag: tag.to_owned(),
            path: loaded.path().to_owned(),
            selected_sha256: (!starve("byte-reader")).then(|| loaded.selected_sha256().to_owned()),
            validated_sha256: (!starve("byte-reader")).then(|| loaded.validated_sha256().to_owned()),
            account_id: Some(loaded.account_id().to_owned()),
            words: if starve("validator") { Vec::new() } else { loaded.words() },
            refusal: None,
            refusal_message: None,
        },
        Err(refusal) => {
            // `starve("refusal")` deliberately swallows the refusal, so the
            // receipt reports a negative file as accepted and the check must
            // notice. Nothing is populated either way -- a starved *report* is
            // not a starved guard.
            if starve("refusal") {
                Outcome {
                    tag: tag.to_owned(),
                    path,
                    selected_sha256: None,
                    validated_sha256: None,
                    account_id: None,
                    words: Vec::new(),
                    refusal: None,
                    refusal_message: None,
                }
            } else {
                Outcome {
                    tag: tag.to_owned(),
                    path,
                    selected_sha256: None,
                    validated_sha256: None,
                    account_id: None,
                    words: Vec::new(),
                    refusal: Some(refusal.tag()),
                    refusal_message: Some(refusal.message()),
                }
            }
        }
    }
}

fn outcome_json(outcome: &Outcome) -> String {
    let optional = |value: &Option<String>| match value {
        Some(text) => json_string(text),
        None => "null".to_owned(),
    };
    let optional_static = |value: Option<&'static str>| match value {
        Some(text) => json_string(text),
        None => "null".to_owned(),
    };
    // No braces: the caller wraps these fields together with its own `page`.
    format!(
        "\"tag\":{},\"path\":{},\"selectedSha256\":{},\"validatedSha256\":{},\"accountId\":{},\"wordCount\":{},\"words\":[{}],\"refusal\":{},\"refusalMessage\":{}",
        json_string(&outcome.tag),
        json_string(&outcome.path),
        optional(&outcome.selected_sha256),
        optional(&outcome.validated_sha256),
        optional(&outcome.account_id),
        outcome.words.len(),
        outcome
            .words
            .iter()
            .map(|word| json_string(word))
            .collect::<Vec<_>>()
            .join(","),
        optional_static(outcome.refusal),
        optional_static(outcome.refusal_message),
    )
}

fn read_source_material(fixtures: &Path) -> BTreeMap<String, String> {
    // A deliberately small reader rather than a serde model: this file is one
    // flat object of strings written by phase 1 four lines above.
    let text = std::fs::read_to_string(fixtures.join("source-material.json"))
        .expect("phase 1 must have published the source material");
    let mut fields = BTreeMap::new();
    let body = text.trim().trim_start_matches('{').trim_end_matches('}');
    let mut rest = body;
    while let Some(key_start) = rest.find('"') {
        let after_key = &rest[key_start + 1..];
        let Some(key_end) = after_key.find('"') else { break };
        let key = &after_key[..key_end];
        let after_colon = &after_key[key_end + 1..];
        let Some(value_start) = after_colon.find('"') else { break };
        let after_value = &after_colon[value_start + 1..];
        let Some(value_end) = after_value.find('"') else { break };
        fields.insert(key.to_owned(), after_value[..value_end].to_owned());
        rest = &after_value[value_end + 1..];
    }
    fields
}

/// Phase 2. Load the independently exported kits on both pages, finish both
/// journeys, and refuse everything else without touching a byte.
#[test]
fn phase2_journeys_and_refusals() {
    let fixtures = env_path("OSL6804_FIXTURES");
    let receipt_path = env_path("OSL6804_RECEIPT");
    let material = read_source_material(&fixtures);
    let source_user_id = material["userId"].clone();
    let source_identity_phrase = material["identityPhrase"].clone();
    let source_password_phrase = material["passwordPhrase"].clone();
    let source_profile = PathBuf::from(material["sourceProfile"].clone());

    let kit = |name: &str| fixtures.join(name);
    let negatives = [
        ("corrupt-byte", "kit-corrupt.oslkit"),
        ("wrong-version", "kit-version2.oslkit"),
        ("wrong-identity", "kit-foreign.oslkit"),
        ("non-kit-file", "not-a-kit.png"),
    ];

    let mut outcomes: Vec<(RecoveryKitPage, Outcome)> = Vec::new();
    let mut restored_user_id: Option<String> = None;
    let mut password_reset_verified = false;
    let mut debug_renderings: Vec<String> = Vec::new();

    // ---- Restore Account, on a profile that has never held this account ----
    let restore_profile = fixtures.join("profile-restore");
    std::fs::create_dir_all(&restore_profile).expect("create the restore profile");
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(restore_profile.clone()));
    let restore_state = HubCoreState::bootstrap_from_disk();
    std::fs::write(
        restore_profile.join("keyserver.json"),
        br#"{"base_url":"http://127.0.0.1:1","user_id":"task-6804-restore"}"#,
    )
    .expect("write the refused keyserver route");

    // Cancellation first: on a page whose boxes are empty, a cancel must leave
    // them empty and produce no sentence at all.
    outcomes.push((
        RecoveryKitPage::RestoreAccount,
        run_selection(RecoveryKitPage::RestoreAccount, "cancel", None, None),
    ));
    let restore_census_before = byte_census(&restore_profile);
    for (tag, file) in negatives {
        outcomes.push((
            RecoveryKitPage::RestoreAccount,
            run_selection(RecoveryKitPage::RestoreAccount, tag, Some(kit(file)), None),
        ));
    }
    let restore_census_after_refusals = byte_census(&restore_profile);

    let restore_load = run_selection(
        RecoveryKitPage::RestoreAccount,
        "valid-kit",
        Some(kit("kit-source.oslkit")),
        None,
    );
    if !restore_load.words.is_empty() {
        // The boxes now hold twelve words. The journey is the page's ordinary
        // one: the words joined are what the form would have submitted, and
        // the importer underneath is the existing authenticated one.
        let typed = restore_load.words.join(" ");
        assert_eq!(
            typed, source_identity_phrase,
            "the boxes must hold the source kit's identity phrase"
        );
        if !starve("importer") {
            let imported =
                password_lifecycle::import_native_identity_phrase(&restore_state, typed.clone())
                    .expect("the uploaded kit restores the identity on a clean profile");
            restored_user_id = Some(imported.user_id);
        }
    }
    outcomes.push((RecoveryKitPage::RestoreAccount, restore_load));

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(None);
    keystore::set_active_account_dir(None);

    // ---- Forgot Password, on the profile that actually holds the account ----
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(Some(source_profile.clone()));
    let forgot_state = HubCoreState::bootstrap_from_disk();
    let expected = (!starve("identity-comparison")).then(|| source_user_id.clone());

    outcomes.push((
        RecoveryKitPage::ForgotPassword,
        run_selection(RecoveryKitPage::ForgotPassword, "cancel", None, expected.as_deref()),
    ));
    let source_census_before = byte_census(&source_profile);
    for (tag, file) in negatives {
        outcomes.push((
            RecoveryKitPage::ForgotPassword,
            run_selection(
                RecoveryKitPage::ForgotPassword,
                tag,
                Some(kit(file)),
                expected.as_deref(),
            ),
        ));
    }
    let source_census_after_refusals = byte_census(&source_profile);

    let forgot_load = run_selection(
        RecoveryKitPage::ForgotPassword,
        "valid-kit",
        Some(kit("kit-source.oslkit")),
        expected.as_deref(),
    );
    if !forgot_load.words.is_empty() {
        let typed = forgot_load.words.join(" ");
        assert_eq!(
            typed, source_password_phrase,
            "the boxes must hold the source kit's password recovery phrase"
        );
        if !starve("importer") {
            password_lifecycle::reset_main_password_after_recovery(
                &forgot_state,
                typed,
                REPLACEMENT_PASSWORD.to_owned(),
            )
            .expect("the uploaded kit resets this account's password");
            ipc::commands::cmd_osl_verify_main_password(REPLACEMENT_PASSWORD.to_owned())
                .expect("the replacement password unlocks the same account");
            password_reset_verified = true;
        }
    }
    outcomes.push((RecoveryKitPage::ForgotPassword, forgot_load));

    // Debug renderings of every type that can hold a phrase. These are the
    // strings a `{:?}` in a log line or a panic would emit; the check scans
    // them for recovery words.
    let selection = FrozenKitSelection::freeze(&kit("kit-source.oslkit"))
        .expect("the source kit can be frozen for the leak scan");
    debug_renderings.push(format!("{selection:?}"));
    let loaded: LoadedRecoveryKit =
        load_frozen_recovery_kit(RecoveryKitPage::RestoreAccount, &selection, None)
            .expect("the source kit loads for the leak scan");
    debug_renderings.push(format!("{loaded:?}"));
    if starve("leak-scan") {
        // This only starves the scanner's redaction input; normal production
        // values still use hand-written redacted Debug implementations.
        debug_renderings.push(source_identity_phrase.clone());
    }
    for refusal in [
        RecoveryKitRefusal::Unreadable,
        RecoveryKitRefusal::NotAKitFile,
        RecoveryKitRefusal::UnsupportedVersion,
        RecoveryKitRefusal::Damaged,
        RecoveryKitRefusal::Malformed,
        RecoveryKitRefusal::WrongIdentity,
    ] {
        debug_renderings.push(format!("{refusal} | {refusal:?} | {}", refusal.tag()));
    }

    ipc::main_password::set_file_storage_key(None);
    keystore::set_base_dir_override(None);
    keystore::set_active_account_dir(None);

    let census_equal = |before: &BTreeMap<String, String>, after: &BTreeMap<String, String>| {
        before == after
    };
    let receipt = format!(
        "{{\n  \"task\": 6804,\n  \"sourceUserId\": {},\n  \"restoredUserId\": {},\n  \"passwordResetVerified\": {},\n  \"restoreBytesUnchanged\": {},\n  \"forgotBytesUnchanged\": {},\n  \"restoreCensusFiles\": {},\n  \"forgotCensusFiles\": {},\n  \"pages\": [{}],\n  \"outcomes\": [{}],\n  \"debugRenderings\": [{}]\n}}\n",
        json_string(&source_user_id),
        restored_user_id
            .as_deref()
            .map(json_string)
            .unwrap_or_else(|| "null".to_owned()),
        password_reset_verified,
        census_equal(&restore_census_before, &restore_census_after_refusals),
        census_equal(&source_census_before, &source_census_after_refusals),
        restore_census_before.len(),
        source_census_before.len(),
        RecoveryKitPage::ALL
            .iter()
            .map(|page| json_string(page.id()))
            .collect::<Vec<_>>()
            .join(","),
        outcomes
            .iter()
            .map(|(page, outcome)| format!(
                "{{\"page\":{},{}}}",
                json_string(page.id()),
                outcome_json(outcome)
            ))
            .collect::<Vec<_>>()
            .join(","),
        debug_renderings
            .iter()
            .map(|line| json_string(line))
            .collect::<Vec<_>>()
            .join(","),
    );
    std::fs::write(&receipt_path, receipt).expect("write the 6804 receipt");
    println!(
        "TASK6804_PHASE2 outcomes={} receipt={}",
        outcomes.len(),
        receipt_path.display()
    );
}
