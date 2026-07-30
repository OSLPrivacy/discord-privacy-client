use ipc::commands::guard_backup_destination;
use ipc::AtRestBoundary;
use std::collections::HashSet;

#[test]
fn staging_backup_boundary_regression() {
    let all = AtRestBoundary::ALL;
    let unique = all
        .iter()
        .map(|boundary| boundary.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(unique.len(), all.len(), "boundary names must be unique");
    assert!(all.contains(&AtRestBoundary::AttachmentStaging));
    assert!(all.contains(&AtRestBoundary::BackupRollbackCopies));
    assert!(all.contains(&AtRestBoundary::MessageStore));
    assert_eq!(
        AtRestBoundary::AttachmentStaging.as_str(),
        "attachment_staging"
    );
    assert_eq!(
        AtRestBoundary::BackupRollbackCopies.as_str(),
        "backup_rollback_copies"
    );

    for rel in [
        "store/messages.sqlite",
        "store/messages.sqlite-wal",
        "store/messages.sqlite-shm",
    ] {
        let refusal = guard_backup_destination(rel, false).unwrap_err();
        assert!(
            refusal.contains(AtRestBoundary::MessageStore.as_str()),
            "{rel}: {refusal}"
        );
        assert!(
            refusal.contains(AtRestBoundary::BackupRollbackCopies.as_str()),
            "{rel}: {refusal}"
        );
        assert!(
            refusal.contains("an encrypted destination"),
            "{rel}: {refusal}"
        );
    }

    assert!(guard_backup_destination("store/messages.sqlite", true).is_ok());
    assert!(guard_backup_destination("store/messages.sqlite-wal", true).is_ok());
    assert!(guard_backup_destination("store/messages.sqlite-shm", true).is_ok());
    assert!(
        guard_backup_destination("attachment-staging/pending.bin", false).is_ok(),
        "attachment staging is a distinct boundary and must not be classified as a Store rollback backup"
    );
}
