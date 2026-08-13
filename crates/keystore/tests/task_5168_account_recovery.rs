use keystore::{
    identity::verify_signed_device_list, AccountDevice, AccountRootKey, DevicePrivateKeys,
    LocalOslInstance, RecoveryDeclaration, RecoveryKit, RecoveryServiceState, SignedRoster,
    RECOVERY_PRIVATE_KEY_BYTES,
};

#[test]
fn task_5168_recovers_compromised_account_root_at_epoch_four() {
    let temp = tempfile::tempdir().expect("temporary recovery-kit directory");
    let root_a = AccountRootKey::generate();
    let root_b = AccountRootKey::generate();
    let refused_root_c = AccountRootKey::generate();

    let recovery_kit = RecoveryKit::generate(root_a.public_key());
    assert_ne!(
        recovery_kit.public_authority(),
        *root_a.public_key().as_bytes(),
        "recovery authority must be generated independently of root A"
    );
    let kit_path = temp.path().join("synthetic-recovery-kit.osl");
    // TASK 5402: the kit artifact is authenticated ciphertext under an
    // Argon2id key derived from this passphrase, which the file never carries.
    const KIT_PASSPHRASE: &str = "task-5168-kit-passphrase";
    recovery_kit
        .save_protected(&kit_path, KIT_PASSPHRASE)
        .expect("save recovery kit");
    let kit_bytes = std::fs::read(&kit_path).expect("read saved recovery kit");
    let reopened = RecoveryKit::open_protected(&kit_path, KIT_PASSPHRASE)
        .expect("the kit passphrase reopens the saved kit");
    assert_eq!(reopened.public_authority(), recovery_kit.public_authority());
    assert!(
        RecoveryKit::open_protected(&kit_path, "task-5168-kit-passphrasf").is_err(),
        "a wrong kit passphrase must release nothing"
    );
    println!(
        "task_5168 kit_file_bytes={} kit_carries_recovery_authority_plaintext={}",
        kit_bytes.len(),
        kit_bytes
            .windows(32)
            .filter(|window| *window == recovery_kit.public_authority())
            .count()
    );

    let mut service = RecoveryServiceState::new(
        recovery_kit.public_authority(),
        recovery_kit.initial_signed_state(),
    )
    .expect("pin recovery public key and genesis state");

    let initial_devices: Vec<AccountDevice> = (1..=3)
        .map(|index| {
            let device = DevicePrivateKeys::generate_on_device(format!("local-{index}"));
            root_a.sign_device(device.public_keys())
        })
        .collect();
    let initial_list = root_a.sign_device_list(7, initial_devices.clone());
    let initial_roster = SignedRoster::sign(
        &root_a,
        7,
        vec!["alice".into(), "bob".into(), "carol".into()],
    );
    let mut instances: Vec<LocalOslInstance> = (1..=3)
        .map(|index| {
            LocalOslInstance::new(
                format!("local-{index}"),
                &service,
                initial_list.clone(),
                initial_roster.clone(),
            )
            .expect("initialize local OSL instance")
        })
        .collect();
    let old_safety_numbers: Vec<String> = instances
        .iter()
        .map(|instance| instance.safety_number().to_owned())
        .collect();

    let active_root_only =
        RecoveryDeclaration::signed_by_active_root_only(&root_a, 4, root_b.public_key());
    let active_root_only_error = service
        .accept_recovery(active_root_only)
        .expect_err("active root alone must not authorize recovery");
    assert_eq!(
        active_root_only_error.to_string(),
        "independent recovery signature invalid"
    );

    let epoch_four = recovery_kit.declare_recovery(root_a.public_key(), 4, root_b.public_key());
    service
        .accept_recovery(epoch_four.clone())
        .expect("independent recovery signature rotates root A to root B");
    assert_eq!(service.recovery_epoch(), 4);
    assert_eq!(service.active_root(), *root_b.public_key().as_bytes());

    let epoch_three =
        recovery_kit.declare_recovery(root_b.public_key(), 3, refused_root_c.public_key());
    let epoch_three_error = service
        .accept_recovery(epoch_three)
        .expect_err("epoch 3 must not roll epoch 4 backward");
    assert_eq!(
        epoch_three_error.to_string(),
        "recovery epoch must go up (current 4, proposed 3)"
    );

    for instance in &mut instances {
        instance
            .accept_recovery(&epoch_four)
            .expect("local instance accepts epoch 4");
        assert_eq!(instance.recovery_epoch(), 4);
        assert_eq!(instance.active_root(), *root_b.public_key().as_bytes());
        assert!(
            !instance.has_device_list(),
            "old device list must be revoked"
        );
        assert!(!instance.has_roster(), "old roster must be revoked");
    }

    let changed_safety_numbers = instances
        .iter()
        .zip(&old_safety_numbers)
        .filter(|(instance, old)| instance.safety_number() != old.as_str())
        .count();
    let unverified_safety_numbers = instances
        .iter()
        .filter(|instance| !instance.safety_number_verified())
        .count();
    assert_eq!(changed_safety_numbers, 3);
    assert_eq!(unverified_safety_numbers, 3);
    for instance in &mut instances {
        let mut wrong_number = instance.safety_number().to_owned();
        wrong_number.replace_range(
            ..1,
            if wrong_number.starts_with('9') {
                "0"
            } else {
                "9"
            },
        );
        assert_eq!(
            instance
                .reverify_safety_number(&wrong_number)
                .expect_err("wrong safety number must not re-verify")
                .to_string(),
            "safety number re-verification mismatch"
        );
        assert!(!instance.safety_number_verified());
    }

    let post_recovery_root_a_device_list = root_a.sign_device_list(99, initial_devices);
    verify_signed_device_list(&post_recovery_root_a_device_list)
        .expect("root A signature remains cryptographically valid before revocation policy");
    let post_recovery_root_a_roster =
        SignedRoster::sign(&root_a, 99, vec!["mallory-controlled-roster".into()]);
    let root_a_device_signature_rejections = instances
        .iter_mut()
        .map(|instance| {
            instance
                .accept_device_list(post_recovery_root_a_device_list.clone())
                .expect_err("compromised root A device list must be rejected")
                .to_string()
                == "device list signature from compromised root rejected"
        })
        .filter(|rejected| *rejected)
        .count();
    let root_a_roster_signature_rejections = instances
        .iter_mut()
        .map(|instance| {
            instance
                .accept_roster(post_recovery_root_a_roster.clone())
                .expect_err("compromised root A roster must be rejected")
                .to_string()
                == "roster signature from compromised root rejected"
        })
        .filter(|rejected| *rejected)
        .count();
    assert_eq!(root_a_device_signature_rejections, 3);
    assert_eq!(root_a_roster_signature_rejections, 3);

    let fresh_device = DevicePrivateKeys::generate_on_device("replacement-device");
    let published_root_b_lists =
        vec![root_b.sign_device_list(1, vec![root_b.sign_device(fresh_device.public_keys())])];
    let root_b_fresh_device_lists_published = published_root_b_lists.len();
    for instance in &mut instances {
        instance
            .accept_device_list(published_root_b_lists[0].clone())
            .expect("root B fresh device list accepted");
        assert_eq!(instance.device_count(), 1);
    }
    let root_b_fresh_list_acceptances = instances
        .iter()
        .filter(|instance| instance.device_count() == 1)
        .count();
    let safety_numbers_still_unverified = instances
        .iter()
        .filter(|instance| !instance.safety_number_verified())
        .count();
    assert_eq!(root_b_fresh_device_lists_published, 1);
    assert_eq!(root_b_fresh_list_acceptances, 3);
    assert_eq!(safety_numbers_still_unverified, 3);

    let kit_bytes = std::fs::read(&kit_path).expect("read synthetic recovery kit");
    let private_start = kit_bytes.len() - RECOVERY_PRIVATE_KEY_BYTES;
    let recovery_private = &kit_bytes[private_start..];
    let synthetic_recovery_kit_private_bytes = recovery_private.len();
    assert_eq!(synthetic_recovery_kit_private_bytes, 32);
    let recovery_kit_private_hits = kit_bytes
        .windows(recovery_private.len())
        .filter(|window| *window == recovery_private)
        .count();
    let service_recovery_private_bytes = service
        .public_state_bytes()
        .windows(recovery_private.len())
        .filter(|window| *window == recovery_private)
        .count()
        * recovery_private.len();
    assert_eq!(recovery_kit_private_hits, 1);
    assert_eq!(service_recovery_private_bytes, 0);

    println!(
        "TASK5168 synthetic_recovery_kit_private_bytes={synthetic_recovery_kit_private_bytes}"
    );
    println!("TASK5168 recovery_authority_distinct_from_root_a=true");
    println!("TASK5168 root_rotation=A->B");
    println!(
        "TASK5168 accepted_recovery_epoch={}",
        service.recovery_epoch()
    );
    println!("TASK5168 local_osl_instances={}", instances.len());
    println!("TASK5168 root_a_device_signature_rejections={root_a_device_signature_rejections}");
    println!("TASK5168 root_a_roster_signature_rejections={root_a_roster_signature_rejections}");
    println!("TASK5168 root_b_fresh_device_lists_published={root_b_fresh_device_lists_published}");
    println!("TASK5168 root_b_fresh_list_acceptances={root_b_fresh_list_acceptances}");
    println!("TASK5168 safety_numbers_changed={changed_safety_numbers}");
    println!("TASK5168 safety_numbers_unverified={safety_numbers_still_unverified}");
    println!("TASK5168 active_root_only_refusal={active_root_only_error}");
    println!("TASK5168 epoch_3_refusal={epoch_three_error}");
    println!("TASK5168 service_recovery_private_bytes={service_recovery_private_bytes}");
}
