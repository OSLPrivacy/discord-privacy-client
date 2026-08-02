use ipc::wire_rn::{RnError, RnUserVisibleState};

#[test]
fn every_rn_error_has_a_named_nonempty_user_visible_failure_state() {
    let cases = [
        (RnError::WireInDisabled, RnUserVisibleState::WireInDisabled),
        (
            RnError::PinnedToRn,
            RnUserVisibleState::LegacyDowngradeRefused,
        ),
        (
            RnError::RecoveryRequiresRnPin,
            RnUserVisibleState::RecoveryRequiresPinnedSession,
        ),
        (
            RnError::RnRequiredButUnsupported,
            RnUserVisibleState::RequiredButUnsupported,
        ),
        (RnError::WriterBusy, RnUserVisibleState::SessionBusy),
        (
            RnError::RolledBackSession {
                blob_counter: 2,
                high_water: 3,
            },
            RnUserVisibleState::SessionRollbackRefused,
        ),
        (
            RnError::RolledBackSessionGeneration,
            RnUserVisibleState::SessionRollbackRefused,
        ),
        (
            RnError::PlaintextSealerRefused,
            RnUserVisibleState::LocalSecureStorageRequired,
        ),
        (RnError::BadPeerKemKey, RnUserVisibleState::PeerKeyInvalid),
        (
            RnError::PrekeyAdapter("missing signed prekey"),
            RnUserVisibleState::PrekeySetupFailed,
        ),
        (
            RnError::Protocol("invalid header".to_owned()),
            RnUserVisibleState::ProtocolMessageRefused,
        ),
        (
            RnError::StateTooLarge { got: 17, max: 16 },
            RnUserVisibleState::SessionStateTooLarge,
        ),
        (
            RnError::StoreFull {
                held: 512,
                max: 512,
            },
            RnUserVisibleState::SessionStoreFull,
        ),
        (
            RnError::SkippedCacheTooLarge {
                got: 4097,
                max: 4096,
            },
            RnUserVisibleState::SkippedKeyLimitExceeded,
        ),
        (
            RnError::Storage("disk unavailable".to_owned()),
            RnUserVisibleState::SessionStorageFailed,
        ),
    ];

    for (error, expected_state) in cases {
        let actual_state = error.user_visible_state();
        assert_eq!(
            actual_state, expected_state,
            "error {error:?} mapped incorrectly"
        );
        assert!(
            !actual_state.message().trim().is_empty(),
            "error {error:?} mapped to an empty user-visible state"
        );
    }
}
