//! TASK 3233 / attack 49: hunt for real-workspace data from the decoy.
//!
//! The test materializes an exported workspace snapshot, then runs one scanner
//! over the same six surfaces in both the real and decoy workspaces.  The real
//! side is the positive control: every marker must be found there before a zero
//! on the decoy side is accepted.  `TASK3233_FIXTURE` can point the exact check
//! at a different fixture; the saved empty-real fixture is the red control.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

const DEFAULT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/task_3233/workspaces.json"
);

const SURFACES: [&str; 6] = [
    "files",
    "logs",
    "deep_links",
    "notices",
    "recent_files",
    "windows_search",
];

const CATEGORIES: [&str; 4] = ["conversation_names", "friend_names", "keys", "files"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HuntFixture {
    markers: Markers,
    real: WorkspaceFixture,
    decoy: WorkspaceFixture,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Markers {
    conversation_names: String,
    friend_names: String,
    keys: String,
    files: String,
}

impl Markers {
    fn entries(&self) -> [(&'static str, &str); 4] {
        [
            ("conversation_names", &self.conversation_names),
            ("friend_names", &self.friend_names),
            ("keys", &self.keys),
            ("files", &self.files),
        ]
    }

    fn validate(&self) -> Result<(), String> {
        let mut unique = BTreeSet::new();
        for (category, marker) in self.entries() {
            if marker.len() < 16 || marker.trim() != marker {
                return Err(format!(
                    "HAZEL-3233 fixture marker category={category} must be trimmed and at least 16 bytes"
                ));
            }
            if !unique.insert(marker) {
                return Err(format!(
                    "HAZEL-3233 fixture marker category={category} is not unique"
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceFixture {
    files: Vec<Artifact>,
    logs: Vec<Artifact>,
    deep_links: Vec<Artifact>,
    notices: Vec<Artifact>,
    recent_files: Vec<Artifact>,
    windows_search: Vec<Artifact>,
}

impl WorkspaceFixture {
    fn surface(&self, name: &str) -> &[Artifact] {
        match name {
            "files" => &self.files,
            "logs" => &self.logs,
            "deep_links" => &self.deep_links,
            "notices" => &self.notices,
            "recent_files" => &self.recent_files,
            "windows_search" => &self.windows_search,
            _ => unreachable!("SURFACES contains only known names"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: String,
    contents: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    conversation_names: usize,
    friend_names: usize,
    keys: usize,
    files: usize,
}

impl Counts {
    fn get(self, category: &str) -> usize {
        match category {
            "conversation_names" => self.conversation_names,
            "friend_names" => self.friend_names,
            "keys" => self.keys,
            "files" => self.files,
            _ => unreachable!("CATEGORIES contains only known names"),
        }
    }

    fn total(self) -> usize {
        self.conversation_names + self.friend_names + self.keys + self.files
    }
}

#[derive(Debug)]
struct HuntReport {
    real: BTreeMap<&'static str, Counts>,
    decoy: BTreeMap<&'static str, Counts>,
}

#[test]
fn task_3233_real_data_is_absent_from_all_six_decoy_search_surfaces() {
    let fixture_path = env::var_os("TASK3233_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIXTURE));

    let report = run_hunt(&fixture_path).unwrap_or_else(|error| panic!("{error}"));
    print_report(&fixture_path, &report);
}

fn run_hunt(fixture_path: &Path) -> Result<HuntReport, String> {
    let bytes = fs::read(fixture_path).map_err(|error| {
        format!(
            "HAZEL-3233 could not read fixture {}: {error}",
            fixture_path.display()
        )
    })?;
    let fixture: HuntFixture = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "HAZEL-3233 could not parse fixture {}: {error}",
            fixture_path.display()
        )
    })?;
    fixture.markers.validate()?;

    let workspace = tempfile::tempdir()
        .map_err(|error| format!("HAZEL-3233 could not create workspace: {error}"))?;
    materialize_workspace(workspace.path(), "real", &fixture.real)?;
    materialize_workspace(workspace.path(), "decoy", &fixture.decoy)?;

    let real = scan_workspace(workspace.path().join("real"), &fixture.markers)?;
    let decoy = scan_workspace(workspace.path().join("decoy"), &fixture.markers)?;

    // The positive control is checked first.  A broken search or an empty-real
    // fixture must turn the check red before any decoy zero can be accepted.
    for surface in SURFACES {
        let counts = real[surface];
        for category in CATEGORIES {
            let actual = counts.get(category);
            if actual != 1 {
                return Err(format!(
                    "HAZEL-3233 positive-control fixture={} workspace=real surface={surface} category={category} expected=1 actual={actual}",
                    fixture_path.display()
                ));
            }
        }
    }

    for surface in SURFACES {
        let counts = decoy[surface];
        for (category, marker) in fixture.markers.entries() {
            let actual = counts.get(category);
            if actual != 0 {
                return Err(format!(
                    "HAZEL-3233 decoy-leak fixture={} workspace=decoy surface={surface} category={category} result={marker} is the result that should have been refused expected=0 actual={actual}",
                    fixture_path.display()
                ));
            }
        }
    }

    Ok(HuntReport { real, decoy })
}

fn materialize_workspace(
    root: &Path,
    workspace_name: &str,
    fixture: &WorkspaceFixture,
) -> Result<(), String> {
    let workspace_root = root.join(workspace_name);
    for surface in SURFACES {
        let surface_root = workspace_root.join(surface);
        fs::create_dir_all(&surface_root).map_err(|error| {
            format!(
                "HAZEL-3233 could not create {}: {error}",
                surface_root.display()
            )
        })?;
        for artifact in fixture.surface(surface) {
            let relative = safe_relative_path(&artifact.path)?;
            let path = surface_root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("HAZEL-3233 could not create {}: {error}", parent.display())
                })?;
            }
            fs::write(&path, artifact.contents.as_bytes()).map_err(|error| {
                format!("HAZEL-3233 could not write {}: {error}", path.display())
            })?;
        }
    }
    Ok(())
}

fn safe_relative_path(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if raw.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "HAZEL-3233 fixture artifact path is not a safe relative path: {raw:?}"
        ));
    }
    Ok(path.to_owned())
}

fn scan_workspace(
    root: PathBuf,
    markers: &Markers,
) -> Result<BTreeMap<&'static str, Counts>, String> {
    let mut report = BTreeMap::new();
    for surface in SURFACES {
        report.insert(surface, scan_surface(&root.join(surface), markers)?);
    }
    Ok(report)
}

fn scan_surface(root: &Path, markers: &Markers) -> Result<Counts, String> {
    let mut paths = Vec::new();
    collect_regular_files(root, &mut paths)?;
    paths.sort();

    let mut found = BTreeSet::new();
    for path in paths {
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("HAZEL-3233 could not stat {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "HAZEL-3233 refuses symlink artifact {}",
                path.display()
            ));
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("HAZEL-3233 could not read {}: {error}", path.display()))?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| format!("HAZEL-3233 path escaped surface root: {}", path.display()))?
            .to_string_lossy();

        for (category, marker) in markers.entries() {
            if relative.contains(marker)
                || contains_bytes(&bytes, marker.as_bytes())
                || contains_bytes(&bytes, &utf16le(marker))
            {
                found.insert(category);
            }
        }
    }

    Ok(Counts {
        conversation_names: usize::from(found.contains("conversation_names")),
        friend_names: usize::from(found.contains("friend_names")),
        keys: usize::from(found.contains("keys")),
        files: usize::from(found.contains("files")),
    })
}

fn collect_regular_files(root: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(root)
        .map_err(|error| format!("HAZEL-3233 could not list {}: {error}", root.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("HAZEL-3233 could not read {}: {error}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("HAZEL-3233 could not inspect {}: {error}", path.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "HAZEL-3233 refuses symlink artifact {}",
                path.display()
            ));
        }
        if file_type.is_dir() {
            collect_regular_files(&path, paths)?;
        } else if file_type.is_file() {
            paths.push(path);
        }
    }
    Ok(())
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn utf16le(value: &str) -> Vec<u8> {
    value.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn print_report(fixture_path: &Path, report: &HuntReport) {
    println!(
        "TASK3233 fixture={} scanner=path+utf8+utf16le surfaces=6",
        fixture_path.display()
    );
    for surface in SURFACES {
        let real = report.real[surface];
        let decoy = report.decoy[surface];
        println!(
            "TASK3233 surface={surface} decoy_conversation_names={} decoy_friend_names={} decoy_keys={} decoy_files={} decoy_total={} real_conversation_names={} real_friend_names={} real_keys={} real_files={} real_total={}",
            decoy.conversation_names,
            decoy.friend_names,
            decoy.keys,
            decoy.files,
            decoy.total(),
            real.conversation_names,
            real.friend_names,
            real.keys,
            real.files,
            real.total(),
        );
    }

    let decoy_total: usize = report.decoy.values().copied().map(Counts::total).sum();
    let real_total: usize = report.real.values().copied().map(Counts::total).sum();
    println!("TASK3233 finish=PASS decoy_total={decoy_total} real_total={real_total}");
}
