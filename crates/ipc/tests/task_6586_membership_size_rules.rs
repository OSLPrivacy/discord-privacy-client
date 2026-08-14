use crypto::ed25519;
use ipc::commands::{cmd_osl_create_group_conversation, cmd_osl_membership_update};
use ipc::enclave_removal::{
    EnclaveEpochKey, EnclaveMemberAuthority, EnclaveRemovalError, EnclaveRemovalJob, RemovalStage,
    REMOVAL_DELAY_WARNING,
};
use ipc::membership_service::{JoinRequest, MembershipError, MembershipService, PlaceKind};
use ipc::membership_size_rules::{
    cmd_osl_membership_size_rules, enclave_admission_rule, enclave_size_consumer_inventory,
    group_chat_admission_rule, group_chat_size_consumer_inventory, MembershipProduct,
    ENCLAVE_HELP_COPY, ENCLAVE_SETTINGS_COPY, GROUP_CHAT_FULL_ERROR, GROUP_CHAT_HELP_COPY,
    GROUP_CHAT_MAX_PEOPLE, GROUP_CHAT_SETTINGS_COPY, MEMBERSHIP_SIZE_RULES_SCHEMA,
};

const MEASURED_N: usize = 3;
const PREDECESSOR_EPOCH: u64 = 41;

struct FileKeyReset;

impl Drop for FileKeyReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn join(service: &MembershipService, place: &str, person: usize) {
    let (secret, _) = ed25519::generate_keypair();
    let receipt = service
        .join(&JoinRequest::signed(
            place,
            format!("person-{person:03}"),
            false,
            format!("join-{person:03}"),
            &secret,
        ))
        .unwrap_or_else(|error| panic!("{place} person {person} admission failed: {error}"));
    assert_eq!(receipt.member_count, person, "{place} person {person}");
    assert_eq!(receipt.allowance_reservations, person as u64);
    assert_eq!(receipt.delivery_state_version, receipt.roster_version);
}

#[test]
fn task_6586_independent_schema_command_settings_help_and_runtime_inventories() {
    let command = cmd_osl_membership_size_rules(MEASURED_N).unwrap();
    let json = serde_json::to_value(&command).unwrap();

    assert_eq!(command.schema, MEMBERSHIP_SIZE_RULES_SCHEMA);
    assert_eq!(
        command.group_chat.maximum_people_including_creator,
        GROUP_CHAT_MAX_PEOPLE
    );
    assert_eq!(GROUP_CHAT_MAX_PEOPLE, 20);
    assert_eq!(command.group_chat.settings_copy, GROUP_CHAT_SETTINGS_COPY);
    assert_eq!(command.group_chat.help_copy, GROUP_CHAT_HELP_COPY);
    assert_eq!(command.enclave.maximum_people, None);
    assert_eq!(command.enclave.measured_removal_threshold, MEASURED_N);
    assert_eq!(command.enclave.settings_copy, ENCLAVE_SETTINGS_COPY);
    assert_eq!(command.enclave.help_copy, ENCLAVE_HELP_COPY);
    assert_eq!(json["groupChat"]["maximumPeopleIncludingCreator"], 20);
    assert!(json["enclave"]["maximumPeople"].is_null());
    assert_eq!(json["enclave"]["measuredRemovalThreshold"], MEASURED_N);
    assert_eq!(
        group_chat_admission_rule().maximum_people_including_creator,
        20
    );
    assert_eq!(enclave_admission_rule().maximum_people, None);

    let group_inventory = group_chat_size_consumer_inventory();
    let enclave_inventory = enclave_size_consumer_inventory();
    assert_eq!(group_inventory.len(), 10, "group chat consumer starvation");
    assert_eq!(enclave_inventory.len(), 8, "enclave consumer starvation");
    assert!(group_inventory.iter().all(|binding| {
        binding.product == MembershipProduct::GroupChat && binding.maximum_people == Some(20)
    }));
    assert!(enclave_inventory.iter().all(|binding| {
        binding.product == MembershipProduct::Enclave && binding.maximum_people.is_none()
    }));
    for required in [
        "schema.group_chat",
        "command.membership_size_rules.group_chat",
        "settings.group_chat",
        "help.group_chat",
        "membership.scope.admit",
        "membership.scope.replace",
        "membership_service.join",
        "membership_service.reopen",
        "command.create_group_conversation",
        "command.membership_update_and_send_seed",
    ] {
        assert!(
            group_inventory
                .iter()
                .any(|entry| entry.consumer == required),
            "group chat consumer starvation: {required}"
        );
    }
    for required in [
        "schema.enclave",
        "command.membership_size_rules.enclave",
        "settings.enclave",
        "help.enclave",
        "membership_service.join",
        "hub.named_enclave_registry",
        "hub.enclave_conversation_context",
        "enclave_removal.begin",
    ] {
        assert!(
            enclave_inventory
                .iter()
                .any(|entry| entry.consumer == required),
            "enclave consumer starvation: {required}"
        );
    }

    println!(
        "TASK6586_RULES schema={} command=cmd_osl_membership_size_rules group_chat_max={} enclave_max=null measured_N={} group_chat_consumers={} enclave_consumers={} group_settings={:?} group_help={:?} enclave_settings={:?} enclave_help={:?}",
        command.schema,
        command.group_chat.maximum_people_including_creator,
        command.enclave.measured_removal_threshold,
        group_inventory.len(),
        enclave_inventory.len(),
        command.group_chat.settings_copy,
        command.group_chat.help_copy,
        command.enclave.settings_copy,
        command.enclave.help_copy,
    );
}

#[test]
fn task_6586_real_group_admits_1_through_20_and_refuses_21_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("group-chat.json");
    let (signer_secret, _) = ed25519::generate_keypair();
    let (_creator_secret, creator_public) = ed25519::generate_keypair();
    let service = MembershipService::create(
        &path,
        "task-6586-group",
        PlaceKind::GroupChat,
        "person-001",
        creator_public,
        signer_secret.clone(),
    )
    .unwrap();
    let creator = service.snapshot();
    assert_eq!(creator.members.len(), 1);
    assert_eq!(creator.channel_epoch, 1);
    assert_eq!(creator.delivery_state_version, 1);
    assert_eq!(creator.allowance_reservations, 1);

    for person in 2..=20 {
        join(&service, "task-6586-group", person);
    }
    let before = service.snapshot();
    let before_bytes = service.durable_bytes().unwrap();
    assert_eq!(before.members.len(), 20);
    assert_eq!(before.roster_version, 20);
    assert_eq!(before.channel_epoch, 20);
    assert_eq!(before.delivery_state_version, 20);
    assert_eq!(before.allowance_reservations, 20);

    let (person_21_secret, _) = ed25519::generate_keypair();
    let person_21 = JoinRequest::signed(
        "task-6586-group",
        "person-021",
        false,
        "join-021",
        &person_21_secret,
    );
    assert_eq!(
        service.join(&person_21),
        Err(MembershipError::GroupChatFull)
    );
    assert_eq!(
        service.join(&person_21),
        Err(MembershipError::GroupChatFull)
    );
    assert_eq!(
        MembershipError::GroupChatFull.to_string(),
        GROUP_CHAT_FULL_ERROR
    );
    let after = service.snapshot();
    assert_eq!(
        after.members, before.members,
        "group chat membership changed"
    );
    assert_eq!(
        after.channel_epoch, before.channel_epoch,
        "group chat key epoch changed"
    );
    assert_eq!(
        after.channel_key_hash, before.channel_key_hash,
        "group chat key changed"
    );
    assert_eq!(
        after.delivery_state_version, before.delivery_state_version,
        "group chat delivery state changed"
    );
    assert_eq!(
        after.allowance_reservations, before.allowance_reservations,
        "group chat allowance state changed"
    );
    assert_eq!(after.pending_operations, before.pending_operations);
    assert_eq!(service.durable_bytes().unwrap(), before_bytes);
    drop(service);
    let reopened = MembershipService::reopen(&path, signer_secret).unwrap();
    assert_eq!(reopened.snapshot(), before);

    // Execute the command-facing roster transaction as well: its recipient
    // and sender-key inputs remain untouched by a 21-person replacement.
    let state = ipc::state::AppState::new();
    let twenty: Vec<_> = (1..=20)
        .map(|person| format!("person-{person:03}"))
        .collect();
    let created =
        cmd_osl_create_group_conversation(&state, "Task 6586 group".to_owned(), twenty.clone())
            .unwrap();
    let command_membership_before = state.scope_membership.lock().unwrap().clone();
    let delivery_before = state.channel_members.lock().unwrap().clone();
    let keys_before = state.sender_key_state.lock().unwrap().len();
    let mut twenty_one = twenty;
    twenty_one.push("person-021".to_owned());
    assert_eq!(
        cmd_osl_membership_update(&state, created.group_id.clone(), twenty_one),
        Err(GROUP_CHAT_FULL_ERROR.to_owned())
    );
    assert_eq!(
        *state.scope_membership.lock().unwrap(),
        command_membership_before
    );
    assert_eq!(*state.channel_members.lock().unwrap(), delivery_before);
    assert_eq!(state.sender_key_state.lock().unwrap().len(), keys_before);

    println!(
        "TASK6586_GROUP_CHAT admitted=1-20 refused=21 prior_members=20 final_members={} roster_version={} key_epoch={} key_hash_unchanged=true delivery_state={} allowance_state={} durable_bytes_unchanged=true restart_members={} command_membership_unchanged=true command_key_unchanged=true command_delivery_unchanged=true error={:?}",
        after.members.len(),
        after.roster_version,
        after.channel_epoch,
        after.delivery_state_version,
        after.allowance_reservations,
        reopened.snapshot().members.len(),
        GROUP_CHAT_FULL_ERROR,
    );
}

#[test]
fn task_6586_real_enclave_admits_20_21_n_and_generated_larger_then_rekeys() {
    let _reset = FileKeyReset;
    ipc::main_password::set_file_storage_key(Some([0x86; 32]));
    let generated_larger = MEASURED_N.saturating_mul(4).max(MEASURED_N + 100);
    assert_eq!(generated_larger, 103);

    let temp = tempfile::tempdir().unwrap();
    let roster_path = temp.path().join("enclave-roster.json");
    let (signer_secret, _) = ed25519::generate_keypair();
    let (_owner_secret, owner_public) = ed25519::generate_keypair();
    let enclave = MembershipService::create(
        &roster_path,
        "task-6586-enclave",
        PlaceKind::Enclave,
        "person-001",
        owner_public,
        signer_secret.clone(),
    )
    .unwrap();
    for person in 2..=generated_larger {
        join(&enclave, "task-6586-enclave", person);
        if [MEASURED_N, 20, 21, generated_larger].contains(&person) {
            assert_eq!(enclave.snapshot().members.len(), person);
        }
    }
    let admitted = enclave.snapshot();
    assert_eq!(admitted.members.len(), generated_larger);
    drop(enclave);
    let reopened = MembershipService::reopen(&roster_path, signer_secret).unwrap();
    assert_eq!(reopened.snapshot(), admitted);

    let removal_path = temp.path().join("enclave-removal.json");
    let epoch_key = EnclaveEpochKey::generate().unwrap();
    let authorities: Vec<_> = (1..=generated_larger)
        .map(|person| EnclaveMemberAuthority::generate(format!("person-{person:03}")).unwrap())
        .collect();
    let removed_id = authorities.last().unwrap().member_id().to_owned();
    let mut remaining_client = authorities[0]
        .package_client("task-6586-enclave", PREDECESSOR_EPOCH, &epoch_key)
        .unwrap();
    let mut removed_client = authorities
        .last()
        .unwrap()
        .package_client("task-6586-enclave", PREDECESSOR_EPOCH, &epoch_key)
        .unwrap();
    let mut job = EnclaveRemovalJob::begin(
        &removal_path,
        "task-6586-enclave",
        PREDECESSOR_EPOCH,
        authorities,
        &removed_id,
        MEASURED_N,
    )
    .unwrap();
    let confirmation = job.confirmation();
    assert_eq!(confirmation.member_count, generated_larger);
    assert_eq!(confirmation.measured_progress_n, MEASURED_N);
    assert_eq!(confirmation.warning, Some(REMOVAL_DELAY_WARNING));

    let initial = job.confirm().unwrap();
    assert_eq!(initial.stage, RemovalStage::Rekeying);
    assert_eq!(initial.completed, 0);
    let mut progress_samples = vec![(initial.completed, initial.remaining)];
    let mut progress = initial;
    while progress.stage != RemovalStage::Succeeded {
        let previous = progress.clone();
        progress = job.step(11).unwrap();
        assert!(progress.completed >= previous.completed);
        assert!(progress.remaining <= previous.remaining);
        assert_eq!(progress.completed + progress.remaining, progress.total);
        progress_samples.push((progress.completed, progress.remaining));
    }
    assert!(progress_samples.len() > 2, "enclave progress starvation");
    assert_eq!(progress.completed, generated_larger - 1);
    assert_eq!(progress.remaining, 0);
    assert_eq!(job.successor_epoch(), Some(PREDECESSOR_EPOCH + 1));
    assert!(!job.active_member_ids().contains(&removed_id));

    let message = job.encrypt_new_message(b"task-6586-fresh-epoch").unwrap();
    assert!(matches!(
        removed_client.read_cached(&message),
        Err(EnclaveRemovalError::ReadRefused)
    ));
    assert!(matches!(
        job.direct_service_read(&mut removed_client, &message),
        Err(EnclaveRemovalError::ReadRefused)
    ));
    assert_eq!(
        job.direct_service_read(&mut remaining_client, &message)
            .unwrap()
            .as_deref(),
        Some(&b"task-6586-fresh-epoch"[..])
    );
    job.discard().unwrap();
    assert!(!removal_path.exists());

    println!(
        "TASK6586_ENCLAVE admitted_sizes={},20,21,{} maximum=null N={} generated_larger={} removal_size={} warning={:?} progress_samples={} progress_completed={} progress_remaining={} fresh_epoch={} removed_packaged_reads=0 removed_direct_reads=0 remaining_fresh_reads=1 restart_members={}",
        MEASURED_N,
        generated_larger,
        MEASURED_N,
        generated_larger,
        generated_larger,
        confirmation.warning.unwrap(),
        progress_samples.len(),
        progress.completed,
        progress.remaining,
        PREDECESSOR_EPOCH + 1,
        reopened.snapshot().members.len(),
    );
}
