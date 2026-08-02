//! Browser-account import footprint.
//!
//! This module records only the minimal binding needed to prove that a native
//! browser import was explicitly consented for one owner, browser, profile,
//! account and run. It never stores credentials, paths, cookies, handles or
//! profile labels.

use crate::native_apps::BrowserImportId;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeBrowserImportBinding {
    pub owner_osl_user_id: String,
    pub browser_id: BrowserImportId,
    pub browser_profile_account: String,
    pub browser_profile_id: String,
    pub import_run_id: String,
    pub explicit_consent: bool,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintObservation {
    pub owner_osl_user_id: String,
    pub browser_id: BrowserImportId,
    pub browser_profile_account: String,
    pub browser_profile_id: String,
    pub import_run_id: String,
    pub observed_at_unix_ms: u64,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserFootprintState {
    pub observations: Vec<FootprintObservation>,
    pub bindings: Vec<NativeBrowserImportBinding>,
}

pub struct BrowserFootprintStore {
    path: PathBuf,
}

#[derive(Clone, Eq, PartialEq)]
pub enum BrowserFootprintCommit {
    Hydrated(Vec<FootprintObservation>),
    Refused(BrowserFootprintRefusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserFootprintRefusal {
    Malformed,
    MissingExplicitConsent,
}

impl BrowserFootprintStore {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<BrowserFootprintState, BrowserFootprintRefusal> {
        load_state(&self.path)
    }

    pub fn grant(
        &self,
        binding: NativeBrowserImportBinding,
    ) -> Result<(), BrowserFootprintRefusal> {
        let mut state = self.load()?;
        state
            .bindings
            .retain(|existing| !same_scope(existing, &binding));
        state.bindings.push(binding);
        store_state(&self.path, &state)
    }

    pub fn commit(
        &self,
        observation: FootprintObservation,
    ) -> Result<BrowserFootprintCommit, BrowserFootprintRefusal> {
        let mut state = self.load()?;
        state.observations.push(observation.clone());
        store_state(&self.path, &state)?;
        Ok(
            match hydrate_consented_for_owner(
                &state,
                &observation.owner_osl_user_id,
                observation.browser_id,
                &observation.browser_profile_account,
                &observation.browser_profile_id,
                &observation.import_run_id,
            ) {
                Some(observations) => BrowserFootprintCommit::Hydrated(observations),
                None => {
                    BrowserFootprintCommit::Refused(BrowserFootprintRefusal::MissingExplicitConsent)
                }
            },
        )
    }
}

impl BrowserFootprintState {
    pub fn empty() -> Self {
        Self {
            observations: Vec::new(),
            bindings: Vec::new(),
        }
    }
}

pub fn hydrate_consented_for_owner(
    state: &BrowserFootprintState,
    owner_osl_user_id: &str,
    browser_id: BrowserImportId,
    browser_profile_account: &str,
    browser_profile_id: &str,
    import_run_id: &str,
) -> Option<Vec<FootprintObservation>> {
    let consented = state.bindings.iter().any(|binding| {
        binding.explicit_consent
            && binding.owner_osl_user_id == owner_osl_user_id
            && binding.browser_id == browser_id
            && binding.browser_profile_account == browser_profile_account
            && binding.browser_profile_id == browser_profile_id
            && binding.import_run_id == import_run_id
    });
    if !consented {
        return None;
    }
    let hydrated = state
        .observations
        .iter()
        .filter(|observation| {
            observation.owner_osl_user_id == owner_osl_user_id
                && observation.browser_id == browser_id
                && observation.browser_profile_account == browser_profile_account
                && observation.browser_profile_id == browser_profile_id
                && observation.import_run_id == import_run_id
        })
        .cloned()
        .collect::<Vec<_>>();
    (!hydrated.is_empty()).then_some(hydrated)
}

fn same_scope(a: &NativeBrowserImportBinding, b: &NativeBrowserImportBinding) -> bool {
    a.owner_osl_user_id == b.owner_osl_user_id
        && a.browser_id == b.browser_id
        && a.browser_profile_account == b.browser_profile_account
        && a.browser_profile_id == b.browser_profile_id
        && a.import_run_id == b.import_run_id
}

fn load_state(path: &Path) -> Result<BrowserFootprintState, BrowserFootprintRefusal> {
    match fs::read(path) {
        Ok(sealed) => {
            if !ipc::main_password::has_enc_magic(&sealed) {
                return Err(BrowserFootprintRefusal::Malformed);
            }
            let key = ipc::main_password::get_file_storage_key()
                .ok_or(BrowserFootprintRefusal::Malformed)?;
            let plaintext = ipc::main_password::decrypt_at_rest(&sealed, &key)
                .map_err(|_| BrowserFootprintRefusal::Malformed)?;
            serde_json::from_slice::<BrowserFootprintState>(&plaintext)
                .map_err(|_| BrowserFootprintRefusal::Malformed)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(BrowserFootprintState::empty())
        }
        Err(_) => Err(BrowserFootprintRefusal::Malformed),
    }
}

fn store_state(path: &Path, state: &BrowserFootprintState) -> Result<(), BrowserFootprintRefusal> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| BrowserFootprintRefusal::Malformed)?;
    }
    let plaintext = serde_json::to_vec(state).map_err(|_| BrowserFootprintRefusal::Malformed)?;
    let key =
        ipc::main_password::get_file_storage_key().ok_or(BrowserFootprintRefusal::Malformed)?;
    let sealed = ipc::main_password::encrypt_at_rest(&plaintext, &key)
        .map_err(|_| BrowserFootprintRefusal::Malformed)?;
    fs::write(path, sealed).map_err(|_| BrowserFootprintRefusal::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_FILE_KEY: [u8; 32] = [0x7c; 32];

    struct FileStorageKeyHarness {
        previous_key: Option<[u8; 32]>,
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    impl FileStorageKeyHarness {
        fn new() -> Self {
            let serial = crate::global_keystore_test_lock();
            let previous_key = ipc::main_password::get_file_storage_key();
            ipc::main_password::set_file_storage_key(Some(TEST_FILE_KEY));
            Self {
                previous_key,
                _serial: serial,
            }
        }
    }

    impl Drop for FileStorageKeyHarness {
        fn drop(&mut self) {
            ipc::main_password::set_file_storage_key(self.previous_key);
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "osl-browser-footprint-{name}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn binding() -> NativeBrowserImportBinding {
        NativeBrowserImportBinding {
            owner_osl_user_id: "owner-a".to_owned(),
            browser_id: BrowserImportId::Firefox,
            browser_profile_account: "account-a".to_owned(),
            browser_profile_id: "profile-a".to_owned(),
            import_run_id: "run-a".to_owned(),
            explicit_consent: true,
        }
    }

    fn observation() -> FootprintObservation {
        FootprintObservation {
            owner_osl_user_id: "owner-a".to_owned(),
            browser_id: BrowserImportId::Firefox,
            browser_profile_account: "account-a".to_owned(),
            browser_profile_id: "profile-a".to_owned(),
            import_run_id: "run-a".to_owned(),
            observed_at_unix_ms: 1_770_000_000,
        }
    }

    #[test]
    fn browser_footprint_types_define_observations_bindings_and_state() {
        let mut state = BrowserFootprintState::empty();
        state.bindings.push(binding());
        state.observations.push(observation());

        let hydrated = hydrate_consented_for_owner(
            &state,
            "owner-a",
            BrowserImportId::Firefox,
            "account-a",
            "profile-a",
            "run-a",
        )
        .expect("explicitly consented scope hydrates");

        assert_eq!(hydrated.len(), 1);
        assert_eq!(hydrated[0].observed_at_unix_ms, 1_770_000_000);
        assert!(hydrate_consented_for_owner(
            &BrowserFootprintState {
                bindings: vec![NativeBrowserImportBinding {
                    explicit_consent: false,
                    ..binding()
                }],
                observations: vec![observation()],
            },
            "owner-a",
            BrowserImportId::Firefox,
            "account-a",
            "profile-a",
            "run-a",
        )
        .is_none());
    }

    #[test]
    fn browser_footprint_commit_rereads_and_hydrates_only_explicit_consent() {
        let _key = FileStorageKeyHarness::new();
        let path = temp_path("reread");
        let stale_view = BrowserFootprintStore::at(path.clone());
        let writer = BrowserFootprintStore::at(path.clone());
        writer.grant(binding()).unwrap();

        let committed = stale_view.commit(observation()).unwrap();
        assert!(matches!(&committed, BrowserFootprintCommit::Hydrated(rows) if rows.len() == 1));

        let no_consent_path = temp_path("no-consent");
        let no_consent = BrowserFootprintStore::at(no_consent_path.clone());
        no_consent
            .grant(NativeBrowserImportBinding {
                explicit_consent: false,
                ..binding()
            })
            .unwrap();
        assert!(matches!(
            no_consent.commit(observation()).unwrap(),
            BrowserFootprintCommit::Refused(BrowserFootprintRefusal::MissingExplicitConsent)
        ));
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(no_consent_path);
    }

    #[test]
    fn browser_footprint_store_seals_observations_on_disk_and_refuses_plaintext() {
        let _key = FileStorageKeyHarness::new();
        let path = temp_path("sealed");
        let store = BrowserFootprintStore::at(path.clone());

        store.grant(binding()).expect("grant is sealed locally");
        assert!(matches!(
            store.commit(observation()),
            Ok(BrowserFootprintCommit::Hydrated(rows)) if rows == vec![observation()]
        ));
        let sealed = fs::read(&path).expect("sealed footprint exists on disk");
        assert!(ipc::main_password::has_enc_magic(&sealed));
        assert!(
            !sealed
                .windows(b"owner-a".len())
                .any(|window| window == b"owner-a"),
            "the owner identifier must not be present in the on-disk bytes"
        );

        assert_eq!(
            store.load().expect("sealed footprint reloads").bindings,
            vec![binding()]
        );

        fs::write(
            &path,
            serde_json::to_vec(&BrowserFootprintState::empty()).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            store.load(),
            Err(BrowserFootprintRefusal::Malformed)
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn hydrate_consented_for_owner_requires_exact_browser_profile_account_and_run() {
        let state = BrowserFootprintState {
            bindings: vec![binding()],
            observations: vec![
                observation(),
                FootprintObservation {
                    owner_osl_user_id: "owner-b".to_owned(),
                    ..observation()
                },
                FootprintObservation {
                    browser_profile_account: "account-b".to_owned(),
                    ..observation()
                },
                FootprintObservation {
                    browser_profile_id: "profile-b".to_owned(),
                    ..observation()
                },
                FootprintObservation {
                    import_run_id: "run-b".to_owned(),
                    ..observation()
                },
            ],
        };

        let hydrated = hydrate_consented_for_owner(
            &state,
            "owner-a",
            BrowserImportId::Firefox,
            "account-a",
            "profile-a",
            "run-a",
        )
        .expect("the exact scope hydrates");
        assert_eq!(hydrated.len(), 1);
        assert_eq!(
            hydrated[0].observed_at_unix_ms,
            observation().observed_at_unix_ms
        );

        assert!(hydrate_consented_for_owner(
            &state,
            "owner-b",
            BrowserImportId::Firefox,
            "account-a",
            "profile-a",
            "run-a",
        )
        .is_none());
        assert!(hydrate_consented_for_owner(
            &state,
            "owner-a",
            BrowserImportId::Firefox,
            "account-b",
            "profile-a",
            "run-a",
        )
        .is_none());
        assert!(hydrate_consented_for_owner(
            &state,
            "owner-a",
            BrowserImportId::Firefox,
            "account-a",
            "profile-b",
            "run-a",
        )
        .is_none());
        assert!(hydrate_consented_for_owner(
            &state,
            "owner-a",
            BrowserImportId::Firefox,
            "account-a",
            "profile-a",
            "run-b",
        )
        .is_none());
    }
}
