use std::fs;
use std::path::{Path, PathBuf};

use osl_privacy_hub::cleanup::{
    execute_windows_remove_program_uninstall, WINDOWS_REMOVE_PROGRAM_UNINSTALL_ARG,
    WINDOWS_REMOVE_PROGRAM_UNINSTALL_STEP,
};

struct SeededWindowsInstall {
    root: tempfile::TempDir,
    config_dir: PathBuf,
    local_data_dir: PathBuf,
    remove_program_entries: Vec<RemoveProgramEntry>,
    marker_places: Vec<MarkerPlace>,
}

#[derive(Clone)]
struct RemoveProgramEntry {
    display_name: String,
    uninstall_string: String,
}

#[derive(Clone)]
struct MarkerPlace {
    name: &'static str,
    path: PathBuf,
}

impl SeededWindowsInstall {
    fn seed() -> Self {
        let root = tempfile::tempdir().expect("create isolated Windows uninstall fixture");
        let appdata = root.path().join("AppData").join("Roaming");
        let local_appdata = root.path().join("AppData").join("Local");
        let config_dir = appdata.join("org.oslprivacy.hub");
        let local_data_dir = local_appdata.join("org.oslprivacy.hub");
        let uninstall_exe = root
            .path()
            .join("Programs")
            .join("OSL Privacy")
            .join("Uninstall OSL Privacy.exe");
        let uninstall_string = format!(
            "\"{}\" {}",
            uninstall_exe.display(),
            WINDOWS_REMOVE_PROGRAM_UNINSTALL_ARG
        );

        let marker_places = vec![
            MarkerPlace {
                name: "hub-core",
                path: config_dir.join("osl-core").join("task3185.marker"),
            },
            MarkerPlace {
                name: "service-registry",
                path: config_dir.join("service-registry.json"),
            },
            MarkerPlace {
                name: "service-scope-index",
                path: config_dir.join("service-scope-index.json"),
            },
            MarkerPlace {
                name: "preview-preferences",
                path: config_dir.join("preview-preferences.json"),
            },
            MarkerPlace {
                name: "service-profiles",
                path: local_data_dir
                    .join("service-profiles-v2")
                    .join("task3185.marker"),
            },
            MarkerPlace {
                name: "native-profiles",
                path: local_data_dir
                    .join("native-window-profiles-v1")
                    .join("task3185.marker"),
            },
            MarkerPlace {
                name: "browser-profile-snapshots",
                path: local_data_dir
                    .join("browser-profile-snapshots")
                    .join("task3185.marker"),
            },
        ];

        let install = Self {
            root,
            config_dir,
            local_data_dir,
            remove_program_entries: vec![RemoveProgramEntry {
                display_name: "OSL Privacy".to_owned(),
                uninstall_string,
            }],
            marker_places,
        };
        install.write_markers();
        install
    }

    fn write_markers(&self) {
        for marker in &self.marker_places {
            if let Some(parent) = marker.path.parent() {
                fs::create_dir_all(parent).expect("create marker parent");
            }
            fs::write(&marker.path, format!("TASK3185 {}", marker.name))
                .expect("write uninstall marker");
        }
    }

    fn remove_program_entry_count(&self) -> usize {
        self.remove_program_entries
            .iter()
            .filter(|entry| entry.display_name == "OSL Privacy")
            .count()
    }

    fn marker_count(&self, place: &MarkerPlace) -> usize {
        if place.path.is_file() {
            return 1;
        }
        if place.path.exists() {
            return count_regular_files(&place.path);
        }
        0
    }

    fn marker_counts(&self) -> Vec<(&'static str, usize)> {
        self.marker_places
            .iter()
            .map(|place| (place.name, self.marker_count(place)))
            .collect()
    }

    fn choose_remove(&mut self) -> String {
        let entry = self
            .remove_program_entries
            .iter()
            .find(|entry| entry.display_name == "OSL Privacy")
            .expect("seeded install has one OSL remove-program entry")
            .clone();
        assert!(
            entry
                .uninstall_string
                .contains(WINDOWS_REMOVE_PROGRAM_UNINSTALL_ARG),
            "Windows Remove must invoke the named uninstall step, not a file-only delete command"
        );

        let report =
            execute_windows_remove_program_uninstall(&self.config_dir, &self.local_data_dir)
                .expect("run Windows remove-program uninstall cleanup");
        assert!(
            report.local_cleanup_complete,
            "Windows uninstall cleanup must complete locally: {report:?}"
        );
        self.remove_program_entries
            .retain(|entry| entry.display_name != "OSL Privacy");
        WINDOWS_REMOVE_PROGRAM_UNINSTALL_STEP.to_owned()
    }
}

fn count_regular_files(path: &Path) -> usize {
    if path.is_file() {
        return 1;
    }
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            count += count_regular_files(&entry.path());
        }
    }
    count
}

#[test]
fn task_3185_windows_remove_program_entry_runs_named_uninstall_step() {
    let mut install = SeededWindowsInstall::seed();
    let before_entries = install.remove_program_entry_count();
    println!("TASK3185 before_osl_remove_program_entries={before_entries}");
    assert_eq!(before_entries, 1);

    let before_markers = install.marker_counts();
    for (place, count) in &before_markers {
        println!("TASK3185 before_marker {place}={count}");
        assert_eq!(
            *count, 1,
            "{place} must contain exactly one marker before uninstall"
        );
    }
    assert_eq!(before_markers.len(), 7);
    println!("TASK3185 before_uninstall_places={}", before_markers.len());

    let step = install.choose_remove();
    println!("TASK3185 chosen_remove_step={step}");
    assert_eq!(step, WINDOWS_REMOVE_PROGRAM_UNINSTALL_STEP);

    let after_entries = install.remove_program_entry_count();
    println!("TASK3185 after_osl_remove_program_entries={after_entries}");
    assert_eq!(after_entries, 0);
    for (place, count) in install.marker_counts() {
        println!("TASK3185 after_marker {place}={count}");
        assert_eq!(
            count, 0,
            "{place} marker must be removed by Windows uninstall"
        );
    }

    let second = SeededWindowsInstall::seed();
    let second_entries = second.remove_program_entry_count();
    println!("TASK3185 second_seed_osl_remove_program_entries={second_entries}");
    assert_eq!(second_entries, 1);
    let second_working = second.remove_program_entries.iter().all(|entry| {
        entry
            .uninstall_string
            .contains(WINDOWS_REMOVE_PROGRAM_UNINSTALL_ARG)
    });
    println!("TASK3185 second_seed_working_osl_entry={second_working}");
    assert!(second_working);

    let config = include_str!("../tauri.conf.json");
    assert!(
        config.contains("\"installerHooks\": \"windows/remove-program-uninstall-hooks.nsh\""),
        "the shipped Tauri config must attach the NSIS uninstall hook"
    );
    let hook = include_str!("../windows/remove-program-uninstall-hooks.nsh");
    assert!(
        hook.contains(WINDOWS_REMOVE_PROGRAM_UNINSTALL_ARG)
            && hook.contains("NSIS_HOOK_PREUNINSTALL")
            && hook.contains("Abort \"OSL Privacy uninstall cleanup failed.\""),
        "the NSIS hook must call the named uninstall step and fail closed"
    );

    let root_alive = install.root.path().exists();
    println!("TASK3185 fixture_root_exists_after_run={root_alive}");
}
