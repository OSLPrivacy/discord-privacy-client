use osl_privacy_hub::build_integrity::{
    verify_peer_build_integrity, PeerBuildState, PublishedHash,
};

#[test]
fn direct_peer_build_check_returns_normal_and_changed_warning_with_same_send_permission() {
    let normal = verify_peer_build_integrity(PublishedHash::Published);
    let changed = verify_peer_build_integrity(PublishedHash::Unpublished);

    assert_eq!(normal.state, PeerBuildState::Normal);
    assert_eq!(normal.warning, None);
    assert_eq!(changed.state, PeerBuildState::ChangedBuildWarning);
    assert_eq!(changed.warning.as_deref(), Some("changed-build-warning"));
    assert_eq!(normal.send_permission, changed.send_permission);
    assert!(normal.send_permission);

    println!(
        "TASK1357 direct_command=verify_peer_build_integrity normal_state={} changed_state={} changed_warning={} normal_send_permission={} changed_send_permission={} identical_send_permission={}",
        normal.state.as_str(),
        changed.state.as_str(),
        changed.warning.as_deref().unwrap_or("none"),
        normal.send_permission,
        changed.send_permission,
        normal.send_permission == changed.send_permission
    );
}
