use keystore::{
    load_password_record, save_password_record, validate_password, validate_setup_pair,
    verify_against_record, Argon2Params, Error, InactivityTimer, MemorySealer, NoOpSealer,
    PasswordError, PasswordHash, PasswordRecord, VerifyOutcome, DEFAULT_FAILED_ATTEMPT_THRESHOLD,
    DEFAULT_INACTIVITY_SECONDS, MIN_PASSWORD_LENGTH,
};
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn fast() -> Argon2Params {
    Argon2Params::fast_for_tests()
}

// ---- policy ----

#[test]
fn validate_password_rejects_too_short() {
    for short in ["", "1", "12", "12345"] {
        assert!(matches!(
            validate_password(short),
            Err(PasswordError::TooShort { .. })
        ));
    }
}

#[test]
fn validate_password_accepts_six_digits() {
    validate_password("123456").unwrap();
}

#[test]
fn validate_password_accepts_longer_alphanumeric() {
    validate_password("correct horse battery staple").unwrap();
}

#[test]
fn validate_setup_pair_rejects_identical() {
    let res = validate_setup_pair("123456", Some("123456"));
    assert!(matches!(res, Err(PasswordError::PasswordsIdentical)));
}

#[test]
fn validate_setup_pair_accepts_different_pair() {
    validate_setup_pair("111111", Some("222222")).unwrap();
}

#[test]
fn validate_setup_pair_unlock_only() {
    validate_setup_pair("111111", None).unwrap();
}

#[test]
fn min_password_length_constant_matches_design_doc() {
    assert_eq!(MIN_PASSWORD_LENGTH, 6);
}

// ---- argon2id hash ----

#[test]
fn hash_round_trip_correct_password() {
    let h = PasswordHash::create("123456", fast()).unwrap();
    assert!(h.verify("123456").unwrap());
}

#[test]
fn hash_rejects_wrong_password() {
    let h = PasswordHash::create("123456", fast()).unwrap();
    assert!(!h.verify("123457").unwrap());
}

#[test]
fn hash_rejects_empty_against_real_password() {
    let h = PasswordHash::create("123456", fast()).unwrap();
    assert!(!h.verify("").unwrap());
}

#[test]
fn hash_uses_random_salt_per_create() {
    let a = PasswordHash::create("same-password", fast()).unwrap();
    let b = PasswordHash::create("same-password", fast()).unwrap();
    assert_ne!(a.salt, b.salt, "fresh salt per create");
    assert_ne!(a.hash, b.hash, "different salt → different hash");
    assert!(a.verify("same-password").unwrap());
    assert!(b.verify("same-password").unwrap());
}

#[test]
fn argon2_production_params_meet_design_floor() {
    let p = Argon2Params::production();
    assert!(p.m_cost >= 65_536, "memory floor 64 MiB per design doc");
    assert_eq!(p.p_cost, 1);
    assert_eq!(p.output_len, 32);
}

#[test]
fn argon2_test_params_distinguished_from_production() {
    let p = Argon2Params::production();
    let t = Argon2Params::fast_for_tests();
    assert_ne!(p, t, "test params must differ from production");
    assert!(t.m_cost < p.m_cost);
}

// ---- record + verify_against_record ----

#[test]
fn unlock_only_record_verifies_unlock() {
    let mut rec = PasswordRecord::new("111111", None, fast()).unwrap();
    let r = verify_against_record(&mut rec, "111111").unwrap();
    assert_eq!(r, VerifyOutcome::Unlock);
    assert_eq!(rec.failed_attempts, 0);
}

#[test]
fn unlock_with_duress_verifies_duress() {
    let mut rec = PasswordRecord::new("111111", Some("222222"), fast()).unwrap();
    let r = verify_against_record(&mut rec, "222222").unwrap();
    assert_eq!(r, VerifyOutcome::Duress);
}

#[test]
fn wrong_password_increments_attempts() {
    let mut rec = PasswordRecord::new("111111", None, fast()).unwrap();
    let r = verify_against_record(&mut rec, "999999").unwrap();
    assert_eq!(r, VerifyOutcome::Wrong { attempts: 1 });
    assert_eq!(rec.failed_attempts, 1);
    let r = verify_against_record(&mut rec, "888888").unwrap();
    assert_eq!(r, VerifyOutcome::Wrong { attempts: 2 });
}

#[test]
fn successful_unlock_resets_failed_attempts() {
    let mut rec = PasswordRecord::new("111111", None, fast()).unwrap();
    for _ in 0..3 {
        verify_against_record(&mut rec, "999999").unwrap();
    }
    assert_eq!(rec.failed_attempts, 3);
    let r = verify_against_record(&mut rec, "111111").unwrap();
    assert_eq!(r, VerifyOutcome::Unlock);
    assert_eq!(rec.failed_attempts, 0);
}

#[test]
fn threshold_exceeded_returns_duress_by_threshold() {
    let mut rec = PasswordRecord::new("111111", None, fast()).unwrap();
    rec.failed_attempt_threshold = 3;
    for i in 0..2 {
        let r = verify_against_record(&mut rec, "999999").unwrap();
        assert_eq!(r, VerifyOutcome::Wrong { attempts: i + 1 });
    }
    let r = verify_against_record(&mut rec, "999999").unwrap();
    assert_eq!(r, VerifyOutcome::DuressByThreshold);
    // The counter is at the threshold, so any subsequent wrong
    // attempt also returns DuressByThreshold.
    let r = verify_against_record(&mut rec, "999999").unwrap();
    assert_eq!(r, VerifyOutcome::DuressByThreshold);
}

#[test]
fn default_threshold_matches_design_doc() {
    let rec = PasswordRecord::new("111111", None, fast()).unwrap();
    assert_eq!(
        rec.failed_attempt_threshold,
        DEFAULT_FAILED_ATTEMPT_THRESHOLD
    );
    assert_eq!(rec.inactivity_seconds, DEFAULT_INACTIVITY_SECONDS);
    assert_eq!(rec.failed_attempts, 0);
}

#[test]
fn record_creation_rejects_identical_passwords() {
    let res = PasswordRecord::new("111111", Some("111111"), fast());
    assert!(matches!(res, Err(PasswordError::PasswordsIdentical)));
}

#[test]
fn record_creation_rejects_short_unlock() {
    let res = PasswordRecord::new("12345", None, fast());
    assert!(matches!(
        res,
        Err(PasswordError::TooShort { got: 5, min: 6 })
    ));
}

// ---- inactivity timer ----

#[test]
fn inactivity_timer_does_not_fire_within_window() {
    let now = Instant::now();
    let timer = InactivityTimer::with_last_activity(900, now);
    let later = now + Duration::from_secs(899);
    assert!(!timer.should_reprompt_at(later));
}

#[test]
fn inactivity_timer_fires_at_threshold() {
    let now = Instant::now();
    let timer = InactivityTimer::with_last_activity(900, now);
    let later = now + Duration::from_secs(900);
    assert!(timer.should_reprompt_at(later));
    let much_later = now + Duration::from_secs(3600);
    assert!(timer.should_reprompt_at(much_later));
}

#[test]
fn inactivity_timer_resets_on_activity() {
    let t0 = Instant::now();
    let mut timer = InactivityTimer::with_last_activity(60, t0);
    let t1 = t0 + Duration::from_secs(45);
    timer.mark_activity_at(t1);
    let check = t1 + Duration::from_secs(59);
    assert!(!timer.should_reprompt_at(check));
    let check = t1 + Duration::from_secs(60);
    assert!(timer.should_reprompt_at(check));
}

// ---- persistence ----

#[test]
fn save_load_round_trip_with_memory_sealer() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("password.json");
    let sealer = MemorySealer::new();

    let original = PasswordRecord::new("111111", Some("222222"), fast()).unwrap();
    save_password_record(&path, &original, &sealer).unwrap();

    let loaded = load_password_record(&path, &sealer).unwrap();
    // Salts and hashes must match exactly; verifying the right
    // plaintext must still succeed.
    assert_eq!(loaded.unlock_hash.salt, original.unlock_hash.salt);
    assert_eq!(loaded.unlock_hash.hash, original.unlock_hash.hash);
    assert!(loaded.unlock_hash.verify("111111").unwrap());
    assert!(loaded
        .duress_hash
        .as_ref()
        .unwrap()
        .verify("222222")
        .unwrap());
    assert_eq!(loaded.failed_attempts, original.failed_attempts);
    assert_eq!(
        loaded.failed_attempt_threshold,
        original.failed_attempt_threshold
    );
    assert_eq!(loaded.inactivity_seconds, original.inactivity_seconds);
}

#[test]
fn save_load_round_trip_with_noop_sealer_writes_banner() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("password.json");
    let sealer = NoOpSealer::new();
    let rec = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&path, &rec, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("INSECURE prototype storage"));
    let loaded = load_password_record(&path, &sealer).unwrap();
    assert!(loaded.unlock_hash.verify("111111").unwrap());
}

#[test]
fn load_rejects_method_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("password.json");
    let writer = NoOpSealer::new();
    let reader = MemorySealer::new();
    let rec = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&path, &rec, &writer).unwrap();
    let res = load_password_record(&path, &reader);
    assert!(matches!(res, Err(Error::BlobMethodMismatch { .. })));
}

#[test]
fn load_rejects_version_mismatch() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("password.json");
    let sealer = NoOpSealer::new();
    let rec = PasswordRecord::new("111111", None, fast()).unwrap();
    save_password_record(&path, &rec, &sealer).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    let bumped = raw.replace("\"version\": 1", "\"version\": 999");
    std::fs::write(&path, bumped).unwrap();
    assert!(matches!(
        load_password_record(&path, &sealer),
        Err(Error::BlobVersionMismatch {
            got: 999,
            expected: 1
        })
    ));
}

// ---- failed-attempt + persistence integration ----

#[test]
fn failed_attempts_persist_across_save_load() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("password.json");
    let sealer = MemorySealer::new();
    let mut rec = PasswordRecord::new("111111", None, fast()).unwrap();
    for _ in 0..3 {
        verify_against_record(&mut rec, "wrong-pw").unwrap();
    }
    save_password_record(&path, &rec, &sealer).unwrap();
    let loaded = load_password_record(&path, &sealer).unwrap();
    assert_eq!(loaded.failed_attempts, 3);
}
