use keystore::{
    verify_lost_device_recovery_declaration, DevicePrivateKeys, LostDeviceRecoveryDeclaration,
    LostDeviceRecoveryService, PackagedReplacementProfile, PreparedLostDeviceRecovery,
    ProductionRecoveryClient, RecoveryAuthorization,
};
use std::{
    env,
    sync::{Arc, Barrier},
    thread,
};

const MUTATION_ENV: &str = "OSL_TASK5189B_MUTATION";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    AcceptBoth,
    RetainReplay,
    OmitRotation,
    DropMessage,
    SkipSignature,
}

impl Mutation {
    fn from_environment() -> Result<Option<Self>, String> {
        let Some(value) = env::var_os(MUTATION_ENV) else {
            return Ok(None);
        };
        let value = value.to_string_lossy();
        let mutation = match value.as_ref() {
            "accept-both" => Self::AcceptBoth,
            "retain-replay" => Self::RetainReplay,
            "omit-rotation" => Self::OmitRotation,
            "drop-message" => Self::DropMessage,
            "skip-signature" => Self::SkipSignature,
            _ => {
                return Err(format!(
                    "invalid signature: fail closed for unrecognized {MUTATION_ENV}={value:?}"
                ));
            }
        };
        Ok(Some(mutation))
    }
}

struct RaceOutcome {
    profile: PackagedReplacementProfile,
    declaration: LostDeviceRecoveryDeclaration,
    result: Result<RecoveryAuthorization, String>,
}

fn main() {
    match run() {
        Ok(summary) => println!("TASK5189b recovery_check=green {summary}"),
        Err(error) => {
            eprintln!("TASK5189b recovery_check=red {error}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<String, String> {
    let mutation = Mutation::from_environment()?;
    let temp = tempfile::tempdir().map_err(|error| format!("setup: {error}"))?;

    let old_devices = (1..=3)
        .map(|index| {
            let keys = DevicePrivateKeys::generate_on_device(format!("old-device-{index}"));
            keystore::replacement_key(keys.public_keys())
        })
        .collect::<Vec<_>>();
    let issued_kit_path = temp.path().join("issued-recovery-kit.osl");
    let (service, current_kit) =
        LostDeviceRecoveryService::bootstrap(old_devices, &issued_kit_path)
            .map_err(|error| format!("setup: {error}"))?;
    let service = Arc::new(service);
    let consumed_authority = current_kit.public_authority();
    let consumed_epoch = current_kit.recovery_epoch();

    let profile_a =
        PackagedReplacementProfile::new_clean(temp.path().join("replacement-a"), "replacement-a")
            .map_err(|error| format!("setup: {error}"))?;
    let profile_b =
        PackagedReplacementProfile::new_clean(temp.path().join("replacement-b"), "replacement-b")
            .map_err(|error| format!("setup: {error}"))?;
    let prepared_a = ProductionRecoveryClient::construct_declaration(&current_kit, &profile_a)
        .map_err(|error| format!("setup: {error}"))?;
    let prepared_b = ProductionRecoveryClient::construct_declaration(&current_kit, &profile_b)
        .map_err(|error| format!("setup: {error}"))?;

    // This explicit check is the break-test's independent signature tripwire.
    // It runs before either request reaches the stateful service.
    verify_lost_device_recovery_declaration(consumed_authority, &prepared_a.declaration)
        .map_err(|error| format!("invalid signature: {error}"))?;
    verify_lost_device_recovery_declaration(consumed_authority, &prepared_b.declaration)
        .map_err(|error| format!("invalid signature: {error}"))?;

    let gate = Arc::new(Barrier::new(3));
    let claim_a = spawn_claim(
        Arc::clone(&service),
        profile_a,
        prepared_a,
        Arc::clone(&gate),
    );
    let claim_b = spawn_claim(
        Arc::clone(&service),
        profile_b,
        prepared_b,
        Arc::clone(&gate),
    );
    gate.wait();
    let outcomes = vec![
        claim_a
            .join()
            .map_err(|_| "race: first recovery worker panicked".to_owned())?,
        claim_b
            .join()
            .map_err(|_| "race: second recovery worker panicked".to_owned())?,
    ];

    let real_accepts = outcomes
        .iter()
        .filter(|outcome| outcome.result.is_ok())
        .count();
    // The mutation models the forbidden non-atomic compare/consume transition.
    // The checker must still turn red even though production remains safe.
    let observed_accepts = if mutation == Some(Mutation::AcceptBoth) {
        2
    } else {
        real_accepts
    };
    if observed_accepts != 1 {
        return Err(format!(
            "race: simultaneous first-use claims authorized {observed_accepts} keys (expected exactly 1)"
        ));
    }

    let mut winner = None;
    let mut loser = None;
    for outcome in outcomes {
        if outcome.result.is_ok() {
            winner = Some(outcome);
        } else {
            loser = Some(outcome);
        }
    }
    let winner = winner.ok_or_else(|| "race: no replacement key was authorized".to_owned())?;
    let loser = loser.ok_or_else(|| "race: no competing recovery was refused".to_owned())?;
    let authorization = winner
        .result
        .as_ref()
        .map_err(|error| format!("race: winning recovery failed: {error}"))?;
    if service.authorized_device_keys() != vec![authorization.replacement_device_key]
        || service
            .authorized_device_keys()
            .contains(&loser.profile.replacement_device_key())
    {
        return Err("unauthorized key: losing race key entered the roster".to_owned());
    }

    let roster_after_win = service.roster_bytes();
    let replay_result = service.authorize_replacement(winner.declaration.clone());
    let replay_was_retained = mutation == Some(Mutation::RetainReplay) || replay_result.is_ok();
    if replay_was_retained {
        return Err(
            "unauthorized key: retained/replayed consumed declaration authorized a key".to_owned(),
        );
    }
    if service.roster_bytes() != roster_after_win {
        return Err("unauthorized key: refused replay changed roster bytes".to_owned());
    }

    let successor = winner
        .profile
        .saved_successor_kit()
        .map_err(|error| format!("dead-end grant: successor kit was not saved: {error}"))?;
    let actually_rotated = successor.public_authority() != consumed_authority
        && service.current_authority() == successor.public_authority()
        && successor.recovery_epoch() == consumed_epoch + 1
        && service.recovery_epoch() == consumed_epoch + 1;
    let observed_rotated = mutation != Some(Mutation::OmitRotation) && actually_rotated;
    if !observed_rotated {
        return Err(
            "exhausted recovery: successful grant omitted authority/epoch rotation".to_owned(),
        );
    }

    if mutation != Some(Mutation::DropMessage) {
        service
            .deliver_new_message("independent-account", "post-recovery-message")
            .map_err(|error| format!("dead-end grant: message delivery failed: {error}"))?;
    }
    let received = winner
        .profile
        .receive_messages(&service)
        .map_err(|error| format!("dead-end grant: winner could not read messages: {error}"))?;
    if received.len() != 1
        || received[0].sender_account != "independent-account"
        || received[0].body != "post-recovery-message"
    {
        return Err(format!(
            "dead-end grant: recovered key received {} valid messages (expected exactly 1)",
            received.len()
        ));
    }

    let mut changed_signed_field = winner.declaration.clone();
    changed_signed_field.replacement_device_key = loser.profile.replacement_device_key();
    let real_signature_rejection =
        verify_lost_device_recovery_declaration(consumed_authority, &changed_signed_field).is_err();
    let observed_signature_rejection =
        mutation != Some(Mutation::SkipSignature) && real_signature_rejection;
    if !observed_signature_rejection {
        return Err(
            "invalid signature: changed signed replacement-key field was accepted".to_owned(),
        );
    }

    Ok(format!(
        "race_winners=1 race_losers=1 replay_authorizations=0 successor_epoch={} delivered_messages=1 invalid_signatures_accepted=0",
        successor.recovery_epoch()
    ))
}

fn spawn_claim(
    service: Arc<LostDeviceRecoveryService>,
    profile: PackagedReplacementProfile,
    prepared: PreparedLostDeviceRecovery,
    gate: Arc<Barrier>,
) -> thread::JoinHandle<RaceOutcome> {
    thread::spawn(move || {
        let declaration = prepared.declaration.clone();
        gate.wait();
        let result = ProductionRecoveryClient::submit_prepared(&service, &profile, prepared)
            .map_err(|error| error.to_string());
        RaceOutcome {
            profile,
            declaration,
            result,
        }
    })
}
