use std::fs;

use osl_privacy_hub::ai_carrier::{ai_carrier_status_for, AiCarrierState};

#[test]
fn task_3794_readiness_follows_pack_presence_without_reinstalling_on_refresh() {
    let install = tempfile::tempdir().expect("temporary model-pack root");
    let state = AiCarrierState::default();
    let installed = state
        .ensure_bundled_local_model(install.path())
        .expect("bundled pack installs");
    state
        .refresh_bundled_local_model(install.path())
        .expect("installed pack refreshes ready");
    assert!(ai_carrier_status_for(&state).local_model_ready);

    fs::remove_file(installed.artifact_path).expect("remove the installed pack");
    assert!(state.refresh_bundled_local_model(install.path()).is_err());
    assert!(!ai_carrier_status_for(&state).local_model_ready);
    assert!(fs::read_dir(install.path())
        .expect("read model-pack root")
        .next()
        .is_none());
}
