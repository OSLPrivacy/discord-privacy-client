//! TASK 3234 / attack 50: interrupt every concrete Burn cleanup step.
//!
//! `TASK3234_FIXTURE` selects the exact artifact fixture. The default has
//! protected markers; the saved empty fixture is the anti-vacuity red control.

use osl_privacy_hub::cleanup::{
    list_hub_uninstall_footprint, task_3234_cleanup_step_count,
    task_3234_cut_power_after_cleanup_step, task_3234_restart_and_resume_gate_burn,
    HubUninstallRoots, Task3234PowerCutKeyStore,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::startup_gate::{verify_password_role, VerifiedGateRole};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

const DEFAULT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/task_3234/with_protected_items.json"
);
const SURFACES: [&str; 11] = [
    "program files",
    "settings",
    "keys",
    "stored messages",
    "downloaded files",
    "startup entry",
    "uninstall logs",
    "backups",
    "temporary files",
    "logs",
    "waiting jobs",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    protected_marker: String,
    windows_key_store_protected: bool,
    items: Vec<FixtureItem>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureItem {
    surface: String,
    root: String,
    path: String,
    protected: bool,
}

struct Materialized {
    _root: tempfile::TempDir,
    config: PathBuf,
    local: PathBuf,
    paths: Vec<(String, PathBuf)>,
    marker: Vec<u8>,
}

impl Materialized {
    fn new(fixture: &Fixture) -> Self {
        let root = tempfile::tempdir().expect("TASK3234 isolated fixture root");
        let config = root.path().join("config");
        let local = root.path().join("local");
        let program_files = root.path().join("program-files");
        let startup = root.path().join("startup");
        let uninstall_logs = root.path().join("uninstall-logs");
        let marker = fixture.protected_marker.as_bytes().to_vec();
        let mut paths = Vec::new();

        for item in &fixture.items {
            let base = match item.root.as_str() {
                "config" => &config,
                "local" => &local,
                "program_files" => &program_files,
                "startup" => &startup,
                "uninstall_logs" => &uninstall_logs,
                other => panic!("HAZEL-3234 fixture root is unknown: {other}"),
            };
            let path = base.join(safe_relative(&item.path));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("TASK3234 fixture parent");
            }
            fs::write(
                &path,
                if item.protected {
                    marker.as_slice()
                } else {
                    b"ordinary non-protected fixture data"
                },
            )
            .expect("TASK3234 fixture artifact");
            paths.push((item.surface.clone(), path));
        }

        Self {
            _root: root,
            config,
            local,
            paths,
            marker,
        }
    }

    fn core(&self) -> PathBuf {
        self.config.join("osl-core")
    }

    fn uninstall_roots(&self) -> HubUninstallRoots {
        HubUninstallRoots::new(
            self._root.path().join("program-files"),
            self.config.clone(),
            self.local.clone(),
            self._root.path().join("startup/OSL Privacy.lnk"),
            self._root.path().join("uninstall-logs"),
        )
    }

    fn readable_counts(
        &self,
        key_store: &Task3234PowerCutKeyStore,
    ) -> (usize, BTreeMap<String, usize>) {
        let mut by_surface = BTreeMap::new();
        for surface in SURFACES {
            by_surface.insert(surface.to_owned(), 0usize);
        }
        for (surface, path) in &self.paths {
            let readable = fs::read(path).is_ok_and(|bytes| {
                bytes
                    .windows(self.marker.len())
                    .any(|window| window == self.marker)
            });
            if readable {
                *by_surface.get_mut(surface).expect("known surface") += 1;
            }
        }
        by_surface.insert("Windows key store".to_owned(), key_store.usable_count());
        (by_surface.values().sum(), by_surface)
    }
}

fn safe_relative(raw: &str) -> PathBuf {
    let path = Path::new(raw);
    assert!(
        !raw.is_empty()
            && !path.is_absolute()
            && path
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "HAZEL-3234 fixture path is not a safe relative path: {raw:?}"
    );
    path.to_owned()
}

fn load_fixture(path: &Path) -> Fixture {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!("HAZEL-3234 fixture={} read failed: {error}", path.display())
    });
    let fixture: Fixture = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!(
            "HAZEL-3234 fixture={} parse failed: {error}",
            path.display()
        )
    });
    assert!(
        fixture.protected_marker.len() >= 24,
        "HAZEL-3234 fixture={} protected marker is too short",
        path.display()
    );
    for surface in SURFACES {
        assert!(
            fixture.items.iter().any(|item| item.surface == surface),
            "HAZEL-3234 fixture={} does not inspect required surface={surface}",
            path.display()
        );
    }
    fixture
}

fn usable_keys(key_store: &Task3234PowerCutKeyStore) -> usize {
    key_store.usable_count()
        + usize::from(ipc::main_password::get_file_storage_key().is_some())
        + usize::from(keystore::active_account_dir().is_some())
}

fn reset_process_globals() {
    ipc::main_password::set_file_storage_key(None);
    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
}

#[test]
fn task_3234_power_cut_at_each_burn_cleanup_step_recovers_without_readable_state() {
    reset_process_globals();
    let fixture_path = env::var_os("TASK3234_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIXTURE));
    let fixture = load_fixture(&fixture_path);

    // Positive control runs before any deletion. A fixture with no protected
    // items must fail here even though production Burn is correctly idempotent.
    let positive = Materialized::new(&fixture);
    let positive_keys = Task3234PowerCutKeyStore::seeded(fixture.windows_key_store_protected);
    let (positive_count, _) = positive.readable_counts(&positive_keys);
    assert!(
        positive_count > 0,
        "HAZEL-3234 positive-control fixture={} expected protected_items_before>0 actual=0",
        fixture_path.display()
    );
    let uninstall_places = list_hub_uninstall_footprint(&positive.uninstall_roots())
        .expect("TASK3234 read shipping uninstall footprint");
    assert_eq!(uninstall_places.len(), 7);
    for place in &uninstall_places {
        let fixture_surface = if place.name == "logs" {
            "uninstall logs"
        } else {
            place.name
        };
        assert!(
            fixture
                .items
                .iter()
                .any(|item| item.surface == fixture_surface),
            "HAZEL-3234 shipping uninstall place is not scanned: {}",
            place.name
        );
    }
    drop(positive);

    // Store the burn password once, then copy that saved marker into every
    // disposable run. Each run still performs the real Argon2 verification.
    let password_setup = Materialized::new(&fixture);
    keystore::set_base_dir_override(Some(password_setup.core()));
    ipc::main_password::set_main_password(&password_setup.core(), "main-pass-3234")
        .expect("TASK3234 save main password");
    ipc::main_password::set_burn_password(
        &password_setup.core(),
        "main-pass-3234",
        "burn-pass-3234",
    )
    .expect("TASK3234 save burn password");
    let saved_password_marker =
        fs::read(password_setup.core().join("password_marker.json")).expect("saved marker");
    reset_process_globals();
    drop(password_setup);

    let plan = Materialized::new(&fixture);
    keystore::set_base_dir_override(Some(plan.core()));
    let cut_runs = task_3234_cleanup_step_count(&plan.config, &plan.local)
        .expect("TASK3234 enumerate cleanup steps");
    reset_process_globals();
    drop(plan);

    let mut minimum_before = usize::MAX;
    let mut maximum_after_protected = 0usize;
    let mut maximum_after_usable_keys = 0usize;
    let mut dishonest_finished_claims = 0usize;
    let mut burn_password_runs = 0usize;
    let mut after_surface_counts = BTreeMap::new();

    for cut_after in 1..=cut_runs {
        let run = Materialized::new(&fixture);
        fs::create_dir_all(run.core()).expect("TASK3234 core");
        fs::write(
            run.core().join("password_marker.json"),
            &saved_password_marker,
        )
        .expect("TASK3234 install saved password marker");
        keystore::set_base_dir_override(Some(run.core()));
        let state = HubCoreState::default();
        let verification = verify_password_role(&state, "burn-pass-3234".to_owned())
            .expect("TASK3234 enter saved burn password");
        assert_eq!(verification.role, VerifiedGateRole::Burn);
        burn_password_runs += 1;

        let key_store = Task3234PowerCutKeyStore::seeded(fixture.windows_key_store_protected);
        ipc::main_password::set_file_storage_key(Some([0x34; 32]));
        keystore::set_active_account_dir(Some(run.core().join("accounts/task-3234")));
        let (before_protected, _) = run.readable_counts(&key_store);
        let before_usable_keys = usable_keys(&key_store);
        assert!(
            before_protected > 0,
            "HAZEL-3234 run={cut_after} protected_items_before must be above zero"
        );
        assert!(before_usable_keys > 0);
        minimum_before = minimum_before.min(before_protected);

        let cut_step =
            task_3234_cut_power_after_cleanup_step(&run.config, &run.local, &key_store, cut_after)
                .unwrap_or_else(|error| panic!("HAZEL-3234 run={cut_after} cut failed: {error}"));

        // A dead process has no completion UI and loses process-only keys.
        ipc::main_password::set_file_storage_key(None);
        keystore::set_active_account_dir(None);
        let burn_finished_before_restart = false;
        let resumed = task_3234_restart_and_resume_gate_burn(&run.config, &run.local, &key_store)
            .unwrap_or_else(|error| {
                panic!("HAZEL-3234 run={cut_after} step={cut_step} recovery failed: {error}")
            });
        assert!(resumed);

        let (after_protected, counts) = run.readable_counts(&key_store);
        let after_usable_keys = usable_keys(&key_store);
        let burn_finished_after_restart = resumed;
        if (burn_finished_before_restart || burn_finished_after_restart) && after_protected > 0 {
            dishonest_finished_claims += 1;
        }
        assert_eq!(after_protected, 0, "run={cut_after} step={cut_step}");
        assert_eq!(after_usable_keys, 0, "run={cut_after} step={cut_step}");
        maximum_after_protected = maximum_after_protected.max(after_protected);
        maximum_after_usable_keys = maximum_after_usable_keys.max(after_usable_keys);
        after_surface_counts = counts;
        println!(
            "TASK3234 run={cut_after}/{cut_runs} cut_after={cut_step} protected_items_before={before_protected} usable_keys_before={before_usable_keys} protected_items_after={after_protected} usable_keys_after={after_usable_keys} burn_finished_before_restart={burn_finished_before_restart} burn_finished_after_restart={burn_finished_after_restart}"
        );
        reset_process_globals();
    }

    assert_eq!(burn_password_runs, cut_runs);
    assert_eq!(maximum_after_protected, 0);
    assert_eq!(maximum_after_usable_keys, 0);
    assert_eq!(dishonest_finished_claims, 0);
    assert!(after_surface_counts.values().all(|count| *count == 0));
    println!(
        "TASK3234 SUMMARY fixture={} cut_runs={} burn_password_runs={} uninstall_places={} inspected_surfaces={} minimum_protected_items_before={} maximum_readable_protected_items_after={} maximum_usable_keys_after={} dishonest_finished_claims={}",
        fixture_path.display(),
        cut_runs,
        burn_password_runs,
        uninstall_places.len(),
        after_surface_counts.len(),
        minimum_before,
        maximum_after_protected,
        maximum_after_usable_keys,
        dishonest_finished_claims
    );
    for (surface, count) in after_surface_counts {
        println!("TASK3234 AFTER surface={surface} readable_protected_items={count}");
    }
    reset_process_globals();
}
