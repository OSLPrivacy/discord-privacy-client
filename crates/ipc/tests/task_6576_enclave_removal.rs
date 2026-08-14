use ipc::enclave_removal::{
    EnclaveEpochKey, EnclaveMemberAuthority, EnclaveRemovalError, EnclaveRemovalJob, RemovalStage,
    RestartBoundary, REMOVAL_DELAY_WARNING,
};
use std::path::Path;

const PREDECESSOR_EPOCH: u64 = 41;

struct FileKeyReset;

impl Drop for FileKeyReset {
    fn drop(&mut self) {
        ipc::main_password::set_file_storage_key(None);
    }
}

fn members(count: usize) -> Vec<EnclaveMemberAuthority> {
    (0..count)
        .map(|index| EnclaveMemberAuthority::generate(format!("member-{index:05}")).unwrap())
        .collect()
}

fn assert_discarded(path: &Path) {
    assert!(!path.exists(), "primary enclave job must be discarded");
    assert!(
        !path.with_extension("bak").exists(),
        "backup enclave job must be discarded"
    );
    assert!(
        !path.with_extension("tmp").exists(),
        "temporary enclave job must be discarded"
    );
}

fn exercise_size(count: usize, measured_n: usize) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(format!("removal-{count}.json"));
    let enclave_id = format!("enclave-{count}");
    let epoch_key = EnclaveEpochKey::generate().unwrap();
    let authorities = members(count);
    let removed_id = authorities.last().unwrap().member_id().to_owned();
    let mut clients: Vec<_> = authorities
        .iter()
        .map(|member| {
            member
                .package_client(&enclave_id, PREDECESSOR_EPOCH, &epoch_key)
                .unwrap()
        })
        .collect();

    let mut job = EnclaveRemovalJob::begin(
        &path,
        &enclave_id,
        PREDECESSOR_EPOCH,
        authorities,
        &removed_id,
        measured_n,
    )
    .unwrap();
    let confirmation = job.confirmation();
    assert_eq!(confirmation.member_count, count);
    assert_eq!(confirmation.measured_progress_n, measured_n);
    assert_eq!(
        confirmation.warning,
        (count >= measured_n).then_some(REMOVAL_DELAY_WARNING),
        "warning is a measured threshold, never an admission cap"
    );
    assert_eq!(job.confirm().unwrap().stage, RemovalStage::Rekeying);

    let first = job.step(2).unwrap();
    let mut rekey_batch_edges = vec![first.completed];
    assert_eq!(first.completed, 2.min(count - 1));
    assert_eq!(first.completed + first.remaining, count - 1);

    job = EnclaveRemovalJob::reopen(&path, RestartBoundary::Client).unwrap();
    let after_client = job.observed_progress().unwrap();
    assert_eq!(after_client.completed, first.completed);
    let second = job.step(3).unwrap();
    rekey_batch_edges.push(second.completed);
    assert!(second.completed >= after_client.completed);
    assert!(second.remaining <= after_client.remaining);

    job = EnclaveRemovalJob::reopen(&path, RestartBoundary::Worker).unwrap();
    let after_worker = job.observed_progress().unwrap();
    assert_eq!(after_worker.completed, second.completed);
    let third = job.step(1).unwrap();
    rekey_batch_edges.push(third.completed);
    assert!(third.completed >= after_worker.completed);

    job = EnclaveRemovalJob::reopen(&path, RestartBoundary::Machine).unwrap();
    assert_eq!(
        job.restart_history(),
        &[
            RestartBoundary::Client,
            RestartBoundary::Worker,
            RestartBoundary::Machine
        ]
    );
    let mut progress = job.observed_progress().unwrap();
    while progress.stage != RemovalStage::Succeeded {
        let before = progress.clone();
        progress = job.step(5).unwrap();
        rekey_batch_edges.push(progress.completed);
        assert!(progress.completed >= before.completed);
        assert!(progress.remaining <= before.remaining);
        assert_eq!(progress.completed + progress.remaining, progress.total);
    }
    assert_eq!(progress.completed, count - 1);
    assert_eq!(progress.remaining, 0);
    assert_eq!(job.successor_epoch(), Some(PREDECESSOR_EPOCH + 1));
    assert!(!job.active_member_ids().contains(&removed_id));
    assert_eq!(job.active_member_ids().len(), count - 1);

    let message = job.encrypt_new_message(b"post-removal canary").unwrap();
    let removed = clients.last_mut().unwrap();
    assert!(
        matches!(
            removed.read_cached(&message),
            Err(EnclaveRemovalError::ReadRefused)
        ),
        "removed packaged cache read 0"
    );
    assert!(
        matches!(
            job.direct_service_read(removed, &message),
            Err(EnclaveRemovalError::ReadRefused)
        ),
        "removed direct service read 0"
    );

    let mut remaining_reads = 0;
    for client in clients.iter_mut().take(count - 1) {
        let first_read = job.direct_service_read(client, &message).unwrap();
        assert_eq!(first_read.as_deref(), Some(&b"post-removal canary"[..]));
        remaining_reads += usize::from(first_read.is_some());
        assert_eq!(
            job.direct_service_read(client, &message).unwrap(),
            None,
            "each remaining member reads exactly once"
        );
    }
    assert_eq!(remaining_reads, count - 1);

    job.discard().unwrap();
    assert_discarded(&path);
    println!(
        "TASK6577_OBSERVATION_JSON={}",
        serde_json::json!({
            "schema": "osl.task6577.removal-size-observation.v1",
            "size": count,
            "n": measured_n,
            "admittedSize": count,
            "warning": confirmation.warning,
            "progress": {
                "completed": progress.completed,
                "remaining": progress.remaining,
                "total": progress.total,
                "rekeyCompleted": progress.completed,
                "source": "authenticated-successor-authority-wraps"
            },
            "rekeyWorkLimits": [2, 3, 1, 5],
            "rekeyBatchEdges": rekey_batch_edges,
            "status": "succeeded",
            "removedMemberId": removed_id,
            "removedPackagedReads": 0,
            "removedDirectReads": 0,
            "remainingExactReads": remaining_reads,
            "restarts": {
                "client": "resumed",
                "worker": "resumed",
                "machine": "resumed"
            },
            "epoch": PREDECESSOR_EPOCH + 1,
            "successAfterLastRekey": true,
            "freshMessage": "post-removal-canary",
            "discarded": true
        })
    );
}

#[test]
fn task_6576_below_at_and_far_above_n_have_no_member_cap_and_remove_safely() {
    let _reset = FileKeyReset;
    ipc::main_password::set_file_storage_key(Some([0x65; 32]));

    let measured_n: usize = std::env::var("TASK6576_MEASURED_N")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8);
    let far_above_n: usize = std::env::var("TASK6576_FAR_ABOVE_N")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| measured_n.saturating_mul(4).max(measured_n + 100));
    assert!(measured_n >= 2, "the campaign must measure a size below N");
    assert!(
        far_above_n >= measured_n.saturating_mul(4).max(measured_n + 100),
        "far-above control must satisfy the frozen campaign"
    );
    for count in [measured_n - 1, measured_n, far_above_n] {
        exercise_size(count, measured_n);
    }
}
