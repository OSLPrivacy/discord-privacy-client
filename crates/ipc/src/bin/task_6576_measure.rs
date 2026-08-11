use ipc::enclave_removal::{
    EnclaveEpochKey, EnclaveMemberAuthority, EnclaveRemovalError, EnclaveRemovalJob, RemovalStage,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Instant;

struct Arguments {
    members: usize,
    artifact_dir: PathBuf,
    run_id: String,
}

fn arguments() -> Result<Arguments, String> {
    let mut members = None;
    let mut artifact_dir = None;
    let mut run_id = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--members" => {
                members = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "--members must be a positive integer".to_owned())?,
                )
            }
            "--artifact-dir" => artifact_dir = Some(PathBuf::from(value)),
            "--run-id" => run_id = Some(value),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    let members = members.ok_or_else(|| "missing --members".to_owned())?;
    if members < 2 {
        return Err("--members must be at least 2".to_owned());
    }
    let artifact_dir = artifact_dir.ok_or_else(|| "missing --artifact-dir".to_owned())?;
    let run_id = run_id.ok_or_else(|| "missing --run-id".to_owned())?;
    if run_id.is_empty()
        || !run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("--run-id must contain only ASCII letters, numbers, '-' or '_'".to_owned());
    }
    Ok(Arguments {
        members,
        artifact_dir,
        run_id,
    })
}

fn run(arguments: Arguments) -> Result<serde_json::Value, String> {
    std::fs::create_dir_all(&arguments.artifact_dir)
        .map_err(|error| format!("create artifact directory: {error}"))?;
    let job_path = arguments
        .artifact_dir
        .join(format!("enclave-removal-{}.json", arguments.run_id));
    let enclave_id = format!("measured-enclave-{}", arguments.run_id);
    let predecessor_epoch = 1_u64;
    let successor_epoch = predecessor_epoch + 1;
    let predecessor_key = EnclaveEpochKey::generate().map_err(|error| error.to_string())?;
    let authorities = (0..arguments.members)
        .map(|index| EnclaveMemberAuthority::generate(format!("member-{index:010}")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let removed_id = authorities
        .last()
        .ok_or_else(|| "generated enclave has no removed member".to_owned())?
        .member_id()
        .to_owned();
    let mut clients = authorities
        .iter()
        .map(|member| member.package_client(&enclave_id, predecessor_epoch, &predecessor_key))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    // This executable is an isolated measurement producer. The production app
    // supplies its unlocked account key; the producer supplies a fresh run key
    // and discards every encrypted artifact before exit.
    let run_storage_key: [u8; 32] = crypto::random::random_bytes(32)
        .try_into()
        .map_err(|_| "generate run storage key".to_owned())?;
    ipc::main_password::set_file_storage_key(Some(run_storage_key));

    let mut job = EnclaveRemovalJob::begin(
        &job_path,
        &enclave_id,
        predecessor_epoch,
        authorities,
        &removed_id,
        // N is supplied by the measurement consumer after repeated trials.
        // `usize::MAX` suppresses a warning during the measurement itself and
        // cannot cap admission or fan-out.
        usize::MAX,
    )
    .map_err(|error| error.to_string())?;

    let started = Instant::now();
    let mut progress = job.confirm().map_err(|error| error.to_string())?;
    let mut progress_samples = vec![json!({
        "epoch": successor_epoch,
        "completed": progress.completed,
        "remaining": progress.remaining,
    })];
    while progress.stage != RemovalStage::Succeeded {
        // Exactly one real wrap and one atomic fsync-backed state commit per
        // step. This is intentionally not a fabricated CPU loop.
        progress = job.step(1).map_err(|error| error.to_string())?;
        progress_samples.push(json!({
            "epoch": successor_epoch,
            "completed": progress.completed,
            "remaining": progress.remaining,
        }));
    }
    let message = job
        .encrypt_new_message(b"task-6576-measurement-canary")
        .map_err(|error| error.to_string())?;
    let removed = clients
        .last_mut()
        .ok_or_else(|| "removed packaged client is absent".to_owned())?;
    if !matches!(
        removed.read_cached(&message),
        Err(EnclaveRemovalError::ReadRefused)
    ) {
        return Err("removed packaged client was not explicitly refused".to_owned());
    }
    if !matches!(
        job.direct_service_read(removed, &message),
        Err(EnclaveRemovalError::ReadRefused)
    ) {
        return Err("removed direct service attempt was not explicitly refused".to_owned());
    }
    let removed_new_message_reads = 0_usize;
    let mut remaining_member_reads = 0_usize;
    for client in clients.iter_mut().take(arguments.members - 1) {
        if job
            .direct_service_read(client, &message)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            remaining_member_reads += 1;
        }
        if job
            .direct_service_read(client, &message)
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(format!(
                "member {} read the canary more than once",
                client.member_id()
            ));
        }
    }
    // The product is not prompt until both the successor fan-out and every
    // post-removal read-path refusal/remaining-member read have completed.
    let elapsed = started.elapsed();
    let duration_ns = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
    let duration_ms_u128 = elapsed.as_millis();
    let duration_ms = u64::try_from(duration_ms_u128).unwrap_or(u64::MAX);
    let removal_complete = progress.stage == RemovalStage::Succeeded
        && progress.remaining == 0
        && !job.active_member_ids().contains(&removed_id);
    job.discard().map_err(|error| error.to_string())?;
    let disposed = !job_path.exists()
        && !job_path.with_extension("bak").exists()
        && !job_path.with_extension("tmp").exists();
    ipc::main_password::set_file_storage_key(None);

    let executable = std::env::current_exe()
        .map_err(|error| format!("resolve measurement executable: {error}"))?;
    let executable_bytes = std::fs::read(&executable)
        .map_err(|error| format!("read measurement executable: {error}"))?;
    let build_hash = Sha256::digest(executable_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    Ok(json!({
        "schema": "osl.task6576.removal-benchmark.v1",
        "build_hash": build_hash,
        "members": arguments.members,
        "admitted_members": arguments.members,
        "epoch": successor_epoch,
        "removal_complete": removal_complete,
        "success_after_last_rekey": removal_complete && progress.completed == progress.total,
        "duration_ms": duration_ms,
        "duration_ns": duration_ns,
        "completed": progress.completed,
        "remaining": progress.remaining,
        "progress_samples": progress_samples,
        "removed_new_message_reads": removed_new_message_reads,
        "remaining_member_reads": remaining_member_reads,
        "disposed": disposed,
        "enclave_discarded": disposed,
    }))
}

fn main() {
    let result = arguments().and_then(run);
    match result {
        Ok(record) => println!("{record}"),
        Err(error) => {
            eprintln!("task_6576_measure: {error}");
            std::process::exit(1);
        }
    }
}
