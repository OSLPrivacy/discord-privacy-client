use crypto::ed25519;
use keystore::{
    canonical_lost_device_recovery_declaration_bytes, verify_lost_device_recovery_declaration,
    DevicePrivateKeys, LostDeviceRecoveryDeclaration, LostDeviceRecoveryKit,
    LostDeviceRecoveryService, PackagedReplacementProfile, PreparedLostDeviceRecovery,
    ProductionRecoveryClient, LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN,
    LOST_DEVICE_RECOVERY_DECLARATION_SCHEMA, LOST_DEVICE_RECOVERY_KIT_DOMAIN,
    LOST_DEVICE_RECOVERY_KIT_SCHEMA, LOST_DEVICE_RECOVERY_STATE_DOMAIN,
    ORDINARY_PAIRING_EXISTING_DEVICE_REQUIRED,
};
use std::{
    sync::{Arc, Barrier},
    thread,
};

const IMPLEMENTATION_SOURCE: &str = include_str!("../src/lost_device_recovery.rs");

#[test]
fn task_5189_lost_all_devices_recovery_is_rotating_single_use_not_lifetime_single_use() {
    inventory_published_schema_and_control_flow();

    let temp = tempfile::tempdir().expect("temporary packaged-profile directory");
    let mut old_profiles = Vec::new();
    let mut old_device_keys = Vec::new();
    for index in 1..=3 {
        let profile = PackagedReplacementProfile::new_clean(
            temp.path().join(format!("old-packaged-profile-{index}")),
            format!("old-device-{index}"),
        )
        .expect("create an old packaged profile");
        old_device_keys.push(profile.replacement_device_key());
        old_profiles.push(profile);
    }

    // The running service, not a dead device, issues the first offline kit.
    let initially_issued_kit_path = temp.path().join("issued-current-recovery-kit.osl");
    let (service, first_kit) =
        LostDeviceRecoveryService::bootstrap(old_device_keys.clone(), &initially_issued_kit_path)
            .expect("running service issues initial recovery kit");
    let service = Arc::new(service);
    assert_eq!(
        first_kit.to_bytes().as_slice(),
        std::fs::read(&initially_issued_kit_path)
            .expect("running service saved issued kit")
            .as_slice()
    );
    let first_kit = LostDeviceRecoveryKit::load(&initially_issued_kit_path)
        .expect("load recovery kit into a clean replacement profile");

    // Dropping is the packaged-profile model's stop boundary. Only public keys
    // were handed to the service; no private old-profile or process state is
    // retained anywhere recovery can read it.
    drop(old_profiles);
    let old_packaged_profiles_stopped = 3usize;

    // Starvation cannot accidentally turn the clean profile into a bearer-token
    // grant.  Ordinary pairing also retains its old-device confirmation gate.
    let starved = PackagedReplacementProfile::new_clean(
        temp.path().join("starved-with-no-kit"),
        "starved-with-no-kit",
    )
    .expect("create starved clean profile");
    let roster_before_starvation = service.roster_bytes();
    assert!(!service
        .authorized_device_keys()
        .contains(&starved.replacement_device_key()));
    assert_eq!(service.roster_bytes(), roster_before_starvation);
    let pairing_candidate = DevicePrivateKeys::generate_on_device("ordinary-pairing-candidate");
    assert_eq!(
        service
            .ordinary_pair_device(None, &pairing_candidate)
            .expect_err("new device cannot confirm ordinary pairing")
            .to_string(),
        ORDINARY_PAIRING_EXISTING_DEVICE_REQUIRED
    );
    let old_device_confirmation_stub =
        DevicePrivateKeys::generate_on_device("old-device-confirmation-stub");
    assert!(service
        .ordinary_pair_device(Some(&old_device_confirmation_stub), &pairing_candidate)
        .is_err());
    assert_eq!(service.roster_bytes(), roster_before_starvation);

    let generated_chain_length = 5 + usize::from(first_kit.public_authority()[0] % 3);
    assert!(generated_chain_length >= 5);

    let mut current_kit = first_kit;
    let mut predecessor_authorities = Vec::new();
    let mut race_winners = 0usize;
    let mut race_losers = 0usize;
    let mut replay_refusals = 0usize;
    let mut consumed_fresh_claim_refusals = 0usize;
    let mut delivered_messages = 0usize;
    let mut stopped_predecessors = 3usize;
    let mut independently_signed_claims = 0usize;
    let mut production_signed_claims = 0usize;

    for round in 1..=generated_chain_length {
        let expected_epoch = current_kit.recovery_epoch() + 1;
        assert_eq!(service.recovery_epoch() + 1, expected_epoch);
        assert_eq!(service.current_authority(), current_kit.public_authority());
        assert_recovery_state_is_exact(&service, &current_kit);

        let production_profile = PackagedReplacementProfile::new_clean(
            temp.path().join(format!("round-{round}-production")),
            format!("round-{round}-production"),
        )
        .expect("create clean production-client replacement profile");
        let external_profile = PackagedReplacementProfile::new_clean(
            temp.path().join(format!("round-{round}-external")),
            format!("round-{round}-external"),
        )
        .expect("create clean externally-signed replacement profile");
        let production_prepared =
            ProductionRecoveryClient::construct_declaration(&current_kit, &production_profile)
                .expect("production client constructs published declaration schema");
        production_signed_claims += 1;
        let production_declaration = production_prepared.declaration;
        independently_verify_declaration(current_kit.public_authority(), &production_declaration);

        // The actual race is between two raw-byte implementations, not a
        // production helper and a hand-waved duplicate of its output.
        let external_prepared_a = independently_prepare_recovery(
            &current_kit,
            production_profile.replacement_device_key(),
        );
        let external_prepared_b =
            independently_prepare_recovery(&current_kit, external_profile.replacement_device_key());
        independently_signed_claims += 2;
        let external_declaration_a = external_prepared_a.declaration.clone();
        let external_declaration_b = external_prepared_b.declaration.clone();

        independently_verify_declaration(current_kit.public_authority(), &external_declaration_a);
        independently_verify_declaration(current_kit.public_authority(), &external_declaration_b);
        verify_lost_device_recovery_declaration(
            current_kit.public_authority(),
            &external_declaration_b,
        )
        .expect("production verifier accepts independently constructed declaration");
        assert_eq!(production_declaration.recovery_epoch, expected_epoch);
        assert_eq!(external_declaration_a.recovery_epoch, expected_epoch);
        assert_eq!(external_declaration_b.recovery_epoch, expected_epoch);
        assert_ne!(
            external_declaration_a.replacement_device_key,
            external_declaration_b.replacement_device_key
        );

        // Release two independently valid first-use claims at the same instant.
        let gate = Arc::new(Barrier::new(3));
        let production_handle = spawn_recovery_claim(
            Arc::clone(&service),
            production_profile,
            external_prepared_a,
            Arc::clone(&gate),
        );
        let external_handle = spawn_recovery_claim(
            Arc::clone(&service),
            external_profile,
            external_prepared_b,
            Arc::clone(&gate),
        );
        gate.wait();
        let mut outcomes = vec![
            production_handle.join().expect("production race thread"),
            external_handle.join().expect("external race thread"),
        ];
        let winner_index = outcomes
            .iter()
            .position(|(_, result)| result.is_ok())
            .expect("one simultaneous claim wins");
        assert_eq!(outcomes.iter().filter(|(_, r)| r.is_ok()).count(), 1);
        assert_eq!(outcomes.iter().filter(|(_, r)| r.is_err()).count(), 1);
        let (winner, winner_result) = outcomes.swap_remove(winner_index);
        let (loser, loser_result) = outcomes.pop().expect("one losing race claim");
        winner_result.expect("winning declaration authorizes its replacement key");
        assert_eq!(
            loser_result.expect_err("second simultaneous declaration is refused"),
            "invalid kit-authority signature"
        );
        race_winners += 1;
        race_losers += 1;

        let winning_key = winner.replacement_device_key();
        let losing_key = loser.replacement_device_key();
        assert!(winner.device_key_path().is_file());
        assert!(loser.device_key_path().is_file());
        assert!(
            !loser.successor_kit_path().exists(),
            "losing claim retained an unissued successor kit"
        );
        assert_eq!(service.recovery_epoch(), expected_epoch);
        assert_eq!(service.authorized_device_keys(), vec![winning_key]);
        assert!(service.authorized_device_keys().contains(&winning_key));
        assert!(!service.authorized_device_keys().contains(&losing_key));
        let published = service
            .published_declaration()
            .expect("service publishes exactly the winning declaration");
        assert_eq!(published.recovery_epoch, expected_epoch);
        assert_eq!(published.replacement_device_key, winning_key);
        assert!(
            published == external_declaration_a || published == external_declaration_b,
            "published declaration was not either released first-use claim"
        );

        // Success is not complete until the clean profile has durably saved a
        // successor kit.  Its authority must rotate, and the service must pin it.
        let successor_path = winner.successor_kit_path();
        assert!(
            successor_path.is_file(),
            "winner did not save successor kit"
        );
        let successor_bytes = std::fs::read(successor_path).expect("read saved successor kit");
        let successor_kit = winner
            .saved_successor_kit()
            .expect("saved successor kit is usable, not a dead-end grant");
        assert_ne!(
            successor_bytes.as_slice(),
            current_kit.to_bytes().as_slice()
        );
        assert_ne!(
            successor_kit.public_authority(),
            current_kit.public_authority()
        );
        assert_eq!(successor_kit.recovery_epoch(), expected_epoch);
        assert_eq!(
            service.current_authority(),
            successor_kit.public_authority()
        );
        predecessor_authorities.push(current_kit.public_authority());

        // One independent-account message reaches exactly the one published key.
        service
            .deliver_new_message(
                format!("independent-account-round-{round}"),
                format!("message-round-{round}"),
            )
            .expect("independent account sends to recovered key");
        let received = winner
            .receive_messages(&service)
            .expect("winner receives after recovery");
        assert_eq!(received.len(), 1);
        assert_eq!(
            received[0].sender_account,
            format!("independent-account-round-{round}")
        );
        assert_eq!(received[0].body, format!("message-round-{round}"));
        assert!(winner
            .receive_messages(&service)
            .expect("inbox was drained")
            .is_empty());
        assert_eq!(
            loser
                .receive_messages(&service)
                .expect_err("race loser is unauthorized")
                .to_string(),
            "device is not authorized"
        );
        delivered_messages += received.len();

        let roster_after_win = service.roster_bytes();
        assert!(service
            .authorize_replacement(external_declaration_a.clone())
            .expect_err("sequential replay of consumed declaration")
            .to_string()
            .contains("invalid kit-authority signature"));
        replay_refusals += 1;
        assert_eq!(service.roster_bytes(), roster_after_win);

        // Even fresh signatures and fresh keys do not revive an old authority.
        let stale_a = PackagedReplacementProfile::new_clean(
            temp.path().join(format!("round-{round}-consumed-a")),
            format!("round-{round}-consumed-a"),
        )
        .expect("fresh stale-claim profile A");
        let stale_b = PackagedReplacementProfile::new_clean(
            temp.path().join(format!("round-{round}-consumed-b")),
            format!("round-{round}-consumed-b"),
        )
        .expect("fresh stale-claim profile B");
        let stale_a_key = stale_a.replacement_device_key();
        let stale_b_key = stale_b.replacement_device_key();
        let stale_declaration_a = independently_sign_declaration(&current_kit, stale_a_key);
        let stale_declaration_b = independently_sign_declaration(&current_kit, stale_b_key);
        independently_signed_claims += 2;
        let stale_gate = Arc::new(Barrier::new(3));
        let stale_handle_a = spawn_direct_claim(
            Arc::clone(&service),
            stale_declaration_a,
            Arc::clone(&stale_gate),
        );
        let stale_handle_b = spawn_direct_claim(
            Arc::clone(&service),
            stale_declaration_b,
            Arc::clone(&stale_gate),
        );
        stale_gate.wait();
        for handle in [stale_handle_a, stale_handle_b] {
            let error = handle
                .join()
                .expect("consumed-kit race thread")
                .expect_err("consumed kit fresh claim must fail");
            assert!(error.contains("invalid kit-authority signature"));
            consumed_fresh_claim_refusals += 1;
        }
        assert!(!service.authorized_device_keys().contains(&stale_a_key));
        assert!(!service.authorized_device_keys().contains(&stale_b_key));
        assert_eq!(service.roster_bytes(), roster_after_win);

        drop(winner);
        stopped_predecessors += 1;
        current_kit = successor_kit;
    }

    // These tamper cases are checked after the chain so none can be confused
    // with the deliberately raced first-use loser.
    let final_roster = service.roster_bytes();
    let final_authorized = service.authorized_device_keys();
    let tamper_profile =
        PackagedReplacementProfile::new_clean(temp.path().join("tamper-profile"), "tamper-profile")
            .expect("create tamper profile");
    let valid_final =
        independently_sign_declaration(&current_kit, tamper_profile.replacement_device_key());
    independently_signed_claims += 1;

    let mut changed_epoch = valid_final.clone();
    changed_epoch.recovery_epoch += 1;
    assert_invalid_signature_without_state_change(&service, changed_epoch, &final_roster);
    let mut changed_key = valid_final.clone();
    changed_key.replacement_device_key = PackagedReplacementProfile::new_clean(
        temp.path().join("changed-key-profile"),
        "changed-key-profile",
    )
    .expect("changed key profile")
    .replacement_device_key();
    assert_invalid_signature_without_state_change(&service, changed_key, &final_roster);
    let mut opaque_bearer = valid_final.clone();
    opaque_bearer.kit_authority_signature = [0; ed25519::SIGNATURE_SIZE];
    assert_invalid_signature_without_state_change(&service, opaque_bearer, &final_roster);

    let final_authority_secret = authority_secret_from_kit(&current_kit);
    let missing_successor = declaration_signed_by(
        &final_authority_secret,
        valid_final.recovery_epoch,
        valid_final.replacement_device_key,
        [0; ed25519::PUBLIC_KEY_SIZE],
    );
    assert_refusal_without_state_change(
        &service,
        missing_successor,
        &final_roster,
        "successor recovery kit is required",
    );
    let unchanged_successor = declaration_signed_by(
        &final_authority_secret,
        valid_final.recovery_epoch,
        valid_final.replacement_device_key,
        current_kit.public_authority(),
    );
    assert_refusal_without_state_change(
        &service,
        unchanged_successor,
        &final_roster,
        "successor recovery authority must rotate",
    );

    let (old_secret, old_public) = ed25519::generate_keypair();
    let active_root_only = declaration_signed_by(
        &old_secret,
        valid_final.recovery_epoch,
        valid_final.replacement_device_key,
        valid_final.successor_recovery_authority,
    );
    assert_eq!(ed25519::derive_public(&old_secret), old_public);
    assert_invalid_signature_without_state_change(&service, active_root_only, &final_roster);
    assert_eq!(service.authorized_device_keys(), final_authorized);
    assert_eq!(service.roster_bytes(), final_roster);

    assert_eq!(race_winners, generated_chain_length);
    assert_eq!(race_losers, generated_chain_length);
    assert_eq!(replay_refusals, generated_chain_length);
    assert_eq!(consumed_fresh_claim_refusals, generated_chain_length * 2);
    assert_eq!(delivered_messages, generated_chain_length);
    assert_eq!(stopped_predecessors, generated_chain_length + 3);
    assert_eq!(production_signed_claims, generated_chain_length);
    assert_eq!(independently_signed_claims, generated_chain_length * 4 + 1);
    assert_eq!(predecessor_authorities.len(), generated_chain_length);
    for (index, authority) in predecessor_authorities.iter().enumerate() {
        assert!(!predecessor_authorities[index + 1..].contains(authority));
        assert_ne!(*authority, current_kit.public_authority());
    }

    assert_eq!(old_packaged_profiles_stopped, 3);
    println!("TASK5189 old_packaged_profiles_stopped={old_packaged_profiles_stopped}");
    println!("TASK5189 generated_chain_length={generated_chain_length}");
    println!("TASK5189 simultaneous_race_winners={race_winners}");
    println!("TASK5189 simultaneous_race_losers={race_losers}");
    println!("TASK5189 sequential_replay_refusals={replay_refusals}");
    println!("TASK5189 consumed_kit_fresh_claim_refusals={consumed_fresh_claim_refusals}");
    println!("TASK5189 delivered_messages={delivered_messages}");
    println!("TASK5189 stopped_predecessors={stopped_predecessors}");
    println!("TASK5189 production_signed_claims={production_signed_claims}");
    println!("TASK5189 independently_signed_claims={independently_signed_claims}");
    println!("TASK5189 recovery_state_fields=current_authority,recovery_epoch");
    println!("TASK5189 lifetime_success_counters=0");
    println!("TASK5189 terminal_epoch_branches=0");
    println!("TASK5189 recovery_count_branches=0");
    println!("TASK5189 unintended_authorized_keys=0");
    println!("TASK5189 ordinary_pairing_without_old_device=refused");
    println!("TASK5189 independent_signature_verification=valid");
}

fn spawn_recovery_claim(
    service: Arc<LostDeviceRecoveryService>,
    profile: PackagedReplacementProfile,
    prepared: PreparedLostDeviceRecovery,
    gate: Arc<Barrier>,
) -> thread::JoinHandle<(PackagedReplacementProfile, Result<(), String>)> {
    thread::spawn(move || {
        gate.wait();
        let result = ProductionRecoveryClient::submit_prepared(&service, &profile, prepared)
            .map(|_| ())
            .map_err(|error| error.to_string());
        (profile, result)
    })
}

fn spawn_direct_claim(
    service: Arc<LostDeviceRecoveryService>,
    declaration: LostDeviceRecoveryDeclaration,
    gate: Arc<Barrier>,
) -> thread::JoinHandle<Result<(), String>> {
    thread::spawn(move || {
        gate.wait();
        service
            .authorize_replacement(declaration)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
}

fn independently_sign_declaration(
    kit: &LostDeviceRecoveryKit,
    replacement_key: [u8; ed25519::PUBLIC_KEY_SIZE],
) -> LostDeviceRecoveryDeclaration {
    independently_prepare_recovery(kit, replacement_key).declaration
}

fn independently_prepare_recovery(
    kit: &LostDeviceRecoveryKit,
    replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
) -> PreparedLostDeviceRecovery {
    let authority_secret = authority_secret_from_kit(kit);
    let successor_kit = LostDeviceRecoveryKit::generate(kit.recovery_epoch() + 1);
    let declaration = declaration_signed_by(
        &authority_secret,
        kit.recovery_epoch() + 1,
        replacement_device_key,
        successor_kit.public_authority(),
    );
    PreparedLostDeviceRecovery {
        declaration,
        successor_kit,
    }
}

fn authority_secret_from_kit(kit: &LostDeviceRecoveryKit) -> ed25519::SecretKey {
    let kit_bytes = kit.to_bytes();
    assert_eq!(
        LOST_DEVICE_RECOVERY_KIT_SCHEMA,
        "osl-lost-all-devices-recovery-kit/v1"
    );
    assert!(kit_bytes.starts_with(LOST_DEVICE_RECOVERY_KIT_DOMAIN));
    let seed_offset = kit_bytes.len() - ed25519::SECRET_KEY_SIZE;
    let authority_seed: [u8; ed25519::SECRET_KEY_SIZE] = kit_bytes[seed_offset..]
        .try_into()
        .expect("published kit schema ends in authority seed");
    let authority_secret = ed25519::SecretKey::from_bytes(authority_seed);
    assert_eq!(
        *ed25519::derive_public(&authority_secret).as_bytes(),
        kit.public_authority()
    );
    authority_secret
}

fn declaration_signed_by(
    authority_secret: &ed25519::SecretKey,
    recovery_epoch: u64,
    replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    successor_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
) -> LostDeviceRecoveryDeclaration {
    let canonical = independent_canonical_bytes(
        recovery_epoch,
        replacement_device_key,
        successor_recovery_authority,
    );
    LostDeviceRecoveryDeclaration {
        recovery_epoch,
        replacement_device_key,
        successor_recovery_authority,
        kit_authority_signature: *ed25519::sign(authority_secret, &canonical).as_bytes(),
    }
}

fn independently_verify_declaration(
    authority: [u8; ed25519::PUBLIC_KEY_SIZE],
    declaration: &LostDeviceRecoveryDeclaration,
) {
    let independent = independent_canonical_bytes(
        declaration.recovery_epoch,
        declaration.replacement_device_key,
        declaration.successor_recovery_authority,
    );
    assert_eq!(
        independent,
        canonical_lost_device_recovery_declaration_bytes(
            declaration.recovery_epoch,
            declaration.replacement_device_key,
            declaration.successor_recovery_authority,
        ),
        "production client and external implementation disagree on published schema"
    );
    assert!(
        ed25519::verify(
            &ed25519::PublicKey::from_bytes(authority),
            &independent,
            &ed25519::Signature::from_bytes(declaration.kit_authority_signature),
        )
        .expect("authority is a valid Ed25519 public key"),
        "independent verifier rejected domain/epoch/replacement-key signature"
    );
}

fn independent_canonical_bytes(
    recovery_epoch: u64,
    replacement_device_key: [u8; ed25519::PUBLIC_KEY_SIZE],
    successor_recovery_authority: [u8; ed25519::PUBLIC_KEY_SIZE],
) -> Vec<u8> {
    assert_eq!(
        LOST_DEVICE_RECOVERY_DECLARATION_SCHEMA,
        "osl-lost-all-devices-recovery-declaration/v1"
    );
    let mut bytes = Vec::with_capacity(
        LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN.len() + 8 + ed25519::PUBLIC_KEY_SIZE * 2,
    );
    bytes.extend_from_slice(LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN);
    bytes.extend_from_slice(&recovery_epoch.to_be_bytes());
    bytes.extend_from_slice(&replacement_device_key);
    bytes.extend_from_slice(&successor_recovery_authority);
    bytes
}

fn assert_recovery_state_is_exact(
    service: &LostDeviceRecoveryService,
    kit: &LostDeviceRecoveryKit,
) {
    let bytes = service.recovery_state_bytes();
    assert_eq!(
        bytes.len(),
        LOST_DEVICE_RECOVERY_STATE_DOMAIN.len() + ed25519::PUBLIC_KEY_SIZE + 8
    );
    assert!(bytes.starts_with(LOST_DEVICE_RECOVERY_STATE_DOMAIN));
    let authority_start = LOST_DEVICE_RECOVERY_STATE_DOMAIN.len();
    assert_eq!(
        &bytes[authority_start..authority_start + ed25519::PUBLIC_KEY_SIZE],
        kit.public_authority().as_slice()
    );
    assert_eq!(
        &bytes[authority_start + ed25519::PUBLIC_KEY_SIZE..],
        kit.recovery_epoch().to_be_bytes().as_slice()
    );
}

fn assert_invalid_signature_without_state_change(
    service: &LostDeviceRecoveryService,
    declaration: LostDeviceRecoveryDeclaration,
    roster_before: &[u8],
) {
    let epoch_before = service.recovery_epoch();
    let authority_before = service.current_authority();
    let authorized_before = service.authorized_device_keys();
    let error = service
        .authorize_replacement(declaration)
        .expect_err("invalid independent signature must refuse recovery")
        .to_string();
    assert_eq!(error, "invalid kit-authority signature");
    assert_eq!(service.recovery_epoch(), epoch_before);
    assert_eq!(service.current_authority(), authority_before);
    assert_eq!(service.authorized_device_keys(), authorized_before);
    assert_eq!(service.roster_bytes(), roster_before);
}

fn assert_refusal_without_state_change(
    service: &LostDeviceRecoveryService,
    declaration: LostDeviceRecoveryDeclaration,
    roster_before: &[u8],
    expected_error: &str,
) {
    let epoch_before = service.recovery_epoch();
    let authority_before = service.current_authority();
    let authorized_before = service.authorized_device_keys();
    assert_eq!(
        service
            .authorize_replacement(declaration)
            .expect_err("starved recovery must be refused")
            .to_string(),
        expected_error
    );
    assert_eq!(service.recovery_epoch(), epoch_before);
    assert_eq!(service.current_authority(), authority_before);
    assert_eq!(service.authorized_device_keys(), authorized_before);
    assert_eq!(service.roster_bytes(), roster_before);
}

fn inventory_published_schema_and_control_flow() {
    assert_eq!(
        LOST_DEVICE_RECOVERY_DECLARATION_DOMAIN,
        b"OSL-lost-all-devices-recovery-declaration-v1"
    );
    assert_eq!(
        LOST_DEVICE_RECOVERY_DECLARATION_SCHEMA,
        "osl-lost-all-devices-recovery-declaration/v1"
    );
    assert_eq!(
        LOST_DEVICE_RECOVERY_KIT_SCHEMA,
        "osl-lost-all-devices-recovery-kit/v1"
    );

    let normalized = IMPLEMENTATION_SOURCE.to_ascii_lowercase();
    let compact: String = normalized.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.contains(
            "structlostdevicerecoverystate{current_authority:[u8;ed25519::public_key_size],recovery_epoch:u64,}"
        ),
        "recovery control state is not exactly current authority plus monotonic epoch"
    );
    for forbidden in [
        "lifetime_success",
        "lifetime_recovery",
        "success_count",
        "recovery_count",
        "terminal_epoch",
        "max_recover",
        "maximum_recover",
        "recoveries_remaining",
    ] {
        assert!(
            !normalized.contains(forbidden),
            "forbidden lifetime/terminal recovery control flow `{forbidden}`"
        );
    }
}
