use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::commands::{
    cmd_osl_set_main_password, cmd_osl_set_main_password_after_recovery,
    cmd_osl_verify_main_password, cmd_osl_verify_recovery_phrase,
};
use crate::main_password::{get_file_storage_key, set_file_storage_key};
use crate::AppState;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

static PROCESS_GLOBALS: Mutex<()> = Mutex::new(());
const ENROLLED_PHRASE_REFERENCE: &str = "$enrolled_recovery_phrase";

#[derive(Debug, Deserialize)]
struct PasswordResetFixture {
    profile: String,
    old_password: String,
    new_password: String,
    reset_recovery_phrase: Option<String>,
}

struct ProcessGlobalReset;

impl Drop for ProcessGlobalReset {
    fn drop(&mut self) {
        set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

fn load_fixture() -> PasswordResetFixture {
    let (label, raw) = match std::env::var_os("TASK0465_FIXTURE") {
        Some(path) => {
            let path = PathBuf::from(path);
            let raw = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "TASK0465 fixture could not be read path={} error={error}",
                    path.display()
                )
            });
            (path.display().to_string(), raw)
        }
        None => (
            "built-in-valid-fixture".to_owned(),
            include_str!("../tests/fixtures/task_0465_password_reset.json").to_owned(),
        ),
    };

    let fixture: PasswordResetFixture = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("TASK0465 fixture is invalid label={label} error={error}"));
    let phrase_reference = fixture.reset_recovery_phrase.as_deref();
    println!(
        "TASK0465_FIXTURE label={label} reset_recovery_phrase={}",
        if phrase_reference.is_some() {
            "present"
        } else {
            "missing"
        }
    );
    assert_eq!(
        phrase_reference,
        Some(ENROLLED_PHRASE_REFERENCE),
        "TASK0465 fixture missing reset_recovery_phrase"
    );
    assert_ne!(
        fixture.old_password, fixture.new_password,
        "TASK0465 fixture old and new passwords must differ"
    );
    fixture
}

fn identity_fingerprint(identity: &keystore::Identity) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-TASK-0465-IDENTITY-FINGERPRINT-v1\0");
    hash.update((identity.user_id.len() as u64).to_be_bytes());
    hash.update(identity.user_id.as_bytes());
    hash.update(identity.ed25519_public.as_bytes());
    hash.update(identity.x25519_public.as_bytes());
    hash.update(identity.mlkem_public_bytes);
    let mut fingerprint = String::from("sha256:");
    for byte in hash.finalize() {
        use std::fmt::Write as _;
        write!(fingerprint, "{byte:02x}").expect("write fingerprint");
    }
    fingerprint
}

#[test]
fn task0465_password_reset_end_to_end_preserves_identity() {
    let _serial = PROCESS_GLOBALS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let _reset = ProcessGlobalReset;
    let fixture = load_fixture();

    let disposable_root = TempDir::new().expect("TASK0465 disposable account root");
    let account_dir = disposable_root
        .path()
        .join("accounts")
        .join(&fixture.profile);
    fs::create_dir_all(&account_dir).expect("TASK0465 disposable account directory");
    keystore::set_base_dir_override(Some(disposable_root.path().to_path_buf()));
    keystore::set_active_account_dir(Some(account_dir));
    set_file_storage_key(None);

    let state = AppState::new();
    let identity = keystore::native_identity_from_entropy([0x46; 16]);
    *state.identity.lock().expect("TASK0465 identity lock") = Some(identity);
    let fingerprint_before = {
        let guard = state.identity.lock().expect("TASK0465 identity lock");
        identity_fingerprint(guard.as_ref().expect("TASK0465 disposable identity"))
    };

    let enrolled_phrase = cmd_osl_set_main_password(fixture.old_password.clone())
        .expect("TASK0465 direct setup command enrolls old password");
    assert_eq!(
        enrolled_phrase.split_whitespace().count(),
        12,
        "TASK0465 enrolled reset recovery phrase must contain 12 words"
    );

    set_file_storage_key(None);
    let token = cmd_osl_verify_recovery_phrase(&state, enrolled_phrase)
        .expect("TASK0465 direct recovery command accepts enrolled phrase");
    cmd_osl_set_main_password_after_recovery(&state, fixture.new_password.clone(), token)
        .expect("TASK0465 direct reset command installs new password");

    set_file_storage_key(None);
    let old_error = cmd_osl_verify_main_password(fixture.old_password)
        .expect_err("TASK0465 old password must fail after reset");
    set_file_storage_key(None);
    cmd_osl_verify_main_password(fixture.new_password)
        .expect("TASK0465 new password must succeed after reset");
    let new_password_installed_key = get_file_storage_key().is_some();

    let fingerprint_after = {
        let guard = state.identity.lock().expect("TASK0465 identity lock");
        identity_fingerprint(guard.as_ref().expect("TASK0465 disposable identity"))
    };
    let fingerprint_unchanged = fingerprint_before == fingerprint_after;

    println!("TASK0465_PROFILE={}", fixture.profile);
    println!("TASK0465_RECOVERY_PHRASE_WORD_COUNT=12");
    println!("TASK0465_DIRECT_RESET_RESULT=succeeded");
    println!("TASK0465_OLD_PASSWORD_RESULT=failed error={old_error}");
    println!(
        "TASK0465_NEW_PASSWORD_RESULT=succeeded file_storage_key_installed={new_password_installed_key}"
    );
    println!("TASK0465_IDENTITY_FINGERPRINT_BEFORE={fingerprint_before}");
    println!("TASK0465_IDENTITY_FINGERPRINT_AFTER={fingerprint_after}");
    println!("TASK0465_IDENTITY_FINGERPRINT_UNCHANGED={fingerprint_unchanged}");

    assert!(
        new_password_installed_key,
        "TASK0465 successful new-password unlock must install the file storage key"
    );
    assert!(
        fingerprint_unchanged,
        "TASK0465 password reset changed the disposable account identity fingerprint"
    );
}
