use std::fs;
use std::path::Path;

use ipc::commands::cmd_osl_verify_gate_password;
use ipc::main_password::{
    get_file_storage_key, read_lockout_pub, set_file_storage_key, set_main_password,
    write_lockout_pub, DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT,
};
use ipc::state::AppState;
use tempfile::TempDir;

struct KeystoreOverrideReset;

impl Drop for KeystoreOverrideReset {
    fn drop(&mut self) {
        set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
    }
}

#[derive(Debug)]
struct ProfileState {
    identity: bool,
    password_marker: bool,
    prekeys: bool,
    store: bool,
    anonymous_credentials: bool,
    boot: bool,
    injection: bool,
    opsec: bool,
}

impl ProfileState {
    fn read(base_dir: &Path, account_dir: &Path) -> Self {
        Self {
            identity: account_dir.join("identity.json").exists(),
            password_marker: base_dir.join("password_marker.json").exists(),
            prekeys: account_dir.join("prekeys.json").exists(),
            store: account_dir.join("store").exists(),
            anonymous_credentials: account_dir.join("anonymous_credentials.json").exists(),
            boot: account_dir.join("boot.js").exists(),
            injection: account_dir.join("injection.js").exists(),
            opsec: account_dir.join("opsec").exists(),
        }
    }

    fn assert_preserved(&self, attempt: u32) {
        assert!(
            self.identity,
            "identity must survive wrong attempt {attempt}"
        );
        assert!(
            self.password_marker,
            "password marker must survive wrong attempt {attempt}"
        );
        assert!(self.prekeys, "prekeys must survive wrong attempt {attempt}");
        assert!(
            self.store,
            "message store must survive wrong attempt {attempt}"
        );
        assert!(
            self.anonymous_credentials,
            "anonymous credentials must survive wrong attempt {attempt}"
        );
        assert!(self.boot, "boot.js must survive wrong attempt {attempt}");
        assert!(
            self.injection,
            "injection.js must survive wrong attempt {attempt}"
        );
        assert!(self.opsec, "opsec dir must survive wrong attempt {attempt}");
    }

    fn summary(&self) -> String {
        format!(
            "identity={},password_marker={},prekeys={},store={},anonymous_credentials={},boot={},injection={},opsec={}",
            self.identity,
            self.password_marker,
            self.prekeys,
            self.store,
            self.anonymous_credentials,
            self.boot,
            self.injection,
            self.opsec
        )
    }
}

#[test]
fn task_0469_superseded_tenth_wrong_unlock_preserves_profile_and_cools_down() {
    let temp = TempDir::new().expect("disposable profile tempdir");
    let base_dir = temp.path().join("base");
    let account_dir = temp.path().join("accounts").join("active");
    fs::create_dir_all(&base_dir).expect("create base dir");
    fs::create_dir_all(account_dir.join("store")).expect("create account store");
    fs::create_dir_all(account_dir.join("opsec")).expect("create opsec dir");

    keystore::set_base_dir_override(Some(base_dir.clone()));
    keystore::set_active_account_dir(Some(account_dir.clone()));
    let _reset = KeystoreOverrideReset;

    let recovery_phrase = set_main_password(&base_dir, "correct-password-0469")
        .expect("install disposable main password");
    assert_eq!(recovery_phrase.split_whitespace().count(), 12);
    set_file_storage_key(Some([0x46; 32]));

    fs::write(account_dir.join("identity.json"), b"disposable identity").expect("write identity");
    fs::write(account_dir.join("prekeys.json"), b"disposable prekeys").expect("write prekeys");
    fs::write(
        account_dir.join("store").join("messages.sqlite"),
        b"message history",
    )
    .expect("write message history");
    fs::write(
        account_dir.join("anonymous_credentials.json"),
        b"anonymous credentials",
    )
    .expect("write anonymous credentials");
    fs::write(account_dir.join("boot.js"), b"boot").expect("write boot.js");
    fs::write(account_dir.join("injection.js"), b"injection").expect("write injection.js");

    let state = AppState::new_with_production_duress_engine(account_dir.clone());
    let mut preserved_attempts = 0u32;
    let mut ninth_summary = String::new();

    for attempt in 1..DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT {
        let result = cmd_osl_verify_gate_password(&state, format!("wrong-password-0469-{attempt}"))
            .expect("wrong unlock attempt returns a gate result");
        assert_eq!(result.result, "wrong");
        assert_eq!(result.attempts_used, attempt);

        let profile = ProfileState::read(&base_dir, &account_dir);
        profile.assert_preserved(attempt);
        preserved_attempts += 1;
        if attempt == DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT - 1 {
            ninth_summary = profile.summary();
        }

        let mut lockout = read_lockout_pub(&base_dir);
        lockout.password_locked_until = None;
        write_lockout_pub(&base_dir, &lockout).expect("clear lockout wait between submissions");

        println!(
            "TASK0469_ATTEMPT_{attempt}=result:{} attempts_used:{} profile_preserved:true",
            result.result, result.attempts_used
        );
    }

    let tenth = cmd_osl_verify_gate_password(&state, "wrong-password-0469-10".to_owned())
        .expect("tenth wrong unlock attempt returns cooldown");
    assert_eq!(tenth.result, "wrong");
    assert_eq!(tenth.attempts_used, DURESS_WRONG_PASSWORD_ATTEMPT_LIMIT);
    assert_eq!(
        tenth.lockout_seconds_remaining,
        ipc::main_password::PASSWORD_COOLDOWN_SECONDS as i64
    );

    let preserved = ProfileState::read(&base_dir, &account_dir);
    preserved.assert_preserved(10);
    assert_eq!(get_file_storage_key(), Some([0x46; 32]));

    println!("TASK0469_DISPOSABLE_PROFILE_ROOT={}", temp.path().display());
    println!("TASK0469_PRESERVED_WRONG_ATTEMPTS={preserved_attempts}");
    println!("TASK0469_NINTH_PROFILE_STATE={ninth_summary}");
    println!(
        "TASK0469_TENTH_RESULT=result:{} attempts_used:{} lockout_seconds_remaining:{}",
        tenth.result, tenth.attempts_used, tenth.lockout_seconds_remaining
    );
    println!(
        "TASK0469_SUPERSEDED_NON_DESTRUCTIVE_RESULT={} file_storage_key=preserved",
        preserved.summary()
    );
}
