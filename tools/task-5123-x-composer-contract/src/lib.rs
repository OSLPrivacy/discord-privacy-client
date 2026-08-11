//! TASK 5123 contract-only X protected-composer gate.
//!
//! This crate is deliberately outside every production workspace. It owns the
//! non-shipping visual contract and audits production and install inputs without
//! making the contract importable by an OSL runtime crate.

use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const CONTRACT_RELATIVE_PATH: &str =
    "tools/task-5123-x-composer-contract/fixtures/x-protected-composer-contract.json";

pub const GEOMETRY_KEYS: [&str; 11] = [
    "composerMarginPx",
    "composerBorderWidthPx",
    "composerCornerRadiusPx",
    "headerMinHeightPx",
    "headerPaddingPx",
    "inputMinHeightPx",
    "inputPaddingPx",
    "footerMinHeightPx",
    "footerPaddingPx",
    "footerGapPx",
    "controlMinHeightPx",
];

pub const TYPE_KEYS: [&str; 10] = [
    "privateInputFamily",
    "privateInputSizePx",
    "statusFamily",
    "statusSizePx",
    "actionFamily",
    "actionSizePx",
    "statusLetterSpacingEm",
    "actionLetterSpacingEm",
    "statusTextTransform",
    "actionTextTransform",
];

pub const STYLE_KEYS: [&str; 14] = [
    "groundRgb",
    "lineRgb",
    "inkRgb",
    "mutedRgb",
    "accentRgb",
    "safeRgb",
    "timerRgb",
    "composerBackground",
    "controlBackground",
    "backgroundImage",
    "composerBorderStyle",
    "controlBorderStyle",
    "controlFillRule",
    "maxControlCornerRadiusPx",
];

pub const CONTROLS: [&str; 7] = [
    "lock",
    "private-box",
    "count",
    "eye-toggle",
    "tray",
    "tick",
    "send-trigger",
];

const PRODUCTION_ANCHORS: [(&str, &str); 4] = [
    ("apps/osl-hub/src/lib.rs", "pub mod adapters"),
    ("apps/osl-hub-ui/src/main.ts", "native_discord_overlay"),
    ("src-tauri/src/main.rs", "fn main"),
    ("crates/ipc/src/lib.rs", "pub mod"),
];

const INSTALLED_ANCHORS: [(&str, &str); 5] = [
    (
        "scripts/build-release-installer.mjs",
        "buildReleaseInstaller",
    ),
    ("apps/osl-hub/tauri.conf.json", "\"bundle\""),
    (
        "apps/osl-hub/capabilities/hub.json",
        "allow-set-native-discord-protected-overlay-open",
    ),
    ("data/pricing.json", "capability_registry"),
    ("data/public-surface-manifest.json", "schema_version"),
];

const IMPORT_NEEDLES: [&str; 3] = [
    "x_protected_composer_contract",
    "task_5123_x_composer_contract",
    "osl-x-protected-composer-contract-v1",
];
const PAINTER_NEEDLES: [&str; 3] = [
    "x_protected_composer_painter",
    "xprotectedcomposerpainter",
    "x-protected-composer-painter",
];
const ACTION_NEEDLES: [&str; 3] = [
    "open_x_protected_composer",
    "open-x-protected-composer",
    "allow-open-x-protected-composer",
];
const RELEASE_ROW_NEEDLES: [&str; 2] = ["\"x-protected-composer\"", "\"x_protected_composer\""];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScopeCounts {
    pub imports: usize,
    pub painters: usize,
    pub actions: usize,
    pub release_capability_rows: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryReport {
    pub scope: &'static str,
    pub scanned_files: usize,
    pub installer_files: usize,
    pub action_files: usize,
    pub release_manifest_files: usize,
    pub counts: ScopeCounts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditReport {
    pub geometry_keys: usize,
    pub type_keys: usize,
    pub style_keys: usize,
    pub controls: usize,
    pub production: InventoryReport,
    pub installed: InventoryReport,
}

pub fn default_repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("task 5123 tool must remain below the repository tools directory")
}

pub fn expected_contract() -> Value {
    json!({
        "schema": "osl-x-protected-composer-contract-v1",
        "carrier": "x",
        "releaseState": "contract-only-not-shippable",
        "controls": CONTROLS,
        "geometry": {
            "composerMarginPx": [0, 28, 28, 28],
            "composerBorderWidthPx": 1,
            "composerCornerRadiusPx": 2,
            "headerMinHeightPx": 36,
            "headerPaddingPx": [0, 13],
            "inputMinHeightPx": 76,
            "inputPaddingPx": [15, 13],
            "footerMinHeightPx": 46,
            "footerPaddingPx": [0, 10, 0, 13],
            "footerGapPx": 12,
            "controlMinHeightPx": 28
        },
        "type": {
            "privateInputFamily": "Arial, sans-serif",
            "privateInputSizePx": 15,
            "statusFamily": "Consolas, \"Liberation Mono\", monospace",
            "statusSizePx": 12,
            "actionFamily": "Consolas, \"Liberation Mono\", monospace",
            "actionSizePx": 11,
            "statusLetterSpacingEm": 0.06,
            "actionLetterSpacingEm": 0.04,
            "statusTextTransform": "uppercase",
            "actionTextTransform": "uppercase"
        },
        "style": {
            "groundRgb": [8, 12, 13],
            "lineRgb": [56, 80, 84],
            "inkRgb": [237, 246, 247],
            "mutedRgb": [145, 168, 172],
            "accentRgb": [42, 192, 240],
            "safeRgb": [61, 214, 140],
            "timerRgb": [240, 180, 41],
            "composerBackground": "ground",
            "controlBackground": "transparent",
            "backgroundImage": "none",
            "composerBorderStyle": "solid",
            "controlBorderStyle": "solid",
            "controlFillRule": "transparent-only",
            "maxControlCornerRadiusPx": 3
        }
    })
}

fn object<'a>(value: &'a Value, name: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("X composer contract is missing non-empty {name} inventory"))
}

fn validate_group(
    actual: &Map<String, Value>,
    expected: &Map<String, Value>,
    group: &str,
    keys: &[&str],
) -> Result<(), String> {
    if actual.is_empty() {
        return Err(format!("X composer contract {group} inventory is empty"));
    }
    for key in keys {
        let actual_value = actual
            .get(*key)
            .ok_or_else(|| format!("X composer contract missing {group}.{key}"))?;
        let expected_value = expected.get(*key).expect("expected contract key");
        if actual_value != expected_value {
            return Err(format!(
                "X composer contract live value changed for {group}.{key}"
            ));
        }
    }
    if actual.len() != keys.len() {
        let unexpected = actual
            .keys()
            .find(|key| !keys.contains(&key.as_str()))
            .map(String::as_str)
            .unwrap_or("duplicate-or-unknown");
        return Err(format!(
            "X composer contract has unexpected {group}.{unexpected}"
        ));
    }
    Ok(())
}

pub fn validate_contract_bytes(bytes: &[u8]) -> Result<(usize, usize, usize, usize), String> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err("X composer contract is empty".into());
    }
    let actual: Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("X composer contract is malformed: {error}"))?;
    let expected = expected_contract();
    let actual_root = actual
        .as_object()
        .ok_or_else(|| "X composer contract root must be an object".to_owned())?;
    if actual_root.is_empty() {
        return Err("X composer contract is empty".into());
    }
    for key in ["schema", "carrier", "releaseState"] {
        if actual.get(key) != expected.get(key) {
            return Err(format!("X composer contract missing or changed {key}"));
        }
    }
    let actual_controls = actual
        .get("controls")
        .and_then(Value::as_array)
        .ok_or_else(|| "X composer contract controls inventory is missing".to_owned())?;
    if actual_controls.is_empty() {
        return Err("X composer contract controls inventory is empty".into());
    }
    if actual_controls != expected.get("controls").and_then(Value::as_array).unwrap() {
        return Err("X composer contract required control inventory changed".into());
    }
    let expected_root = expected.as_object().unwrap();
    let geometry = object(&actual, "geometry")?;
    let type_group = object(&actual, "type")?;
    let style = object(&actual, "style")?;
    validate_group(
        geometry,
        expected_root.get("geometry").unwrap().as_object().unwrap(),
        "geometry",
        &GEOMETRY_KEYS,
    )?;
    validate_group(
        type_group,
        expected_root.get("type").unwrap().as_object().unwrap(),
        "type",
        &TYPE_KEYS,
    )?;
    validate_group(
        style,
        expected_root.get("style").unwrap().as_object().unwrap(),
        "style",
        &STYLE_KEYS,
    )?;
    Ok((
        geometry.len(),
        type_group.len(),
        style.len(),
        actual_controls.len(),
    ))
}

fn require_anchors(root: &Path, scope: &str, anchors: &[(&str, &str)]) -> Result<(), String> {
    for (relative, marker) in anchors {
        let path = root.join(relative);
        let bytes = fs::read(&path).map_err(|_| {
            format!("{scope} inventory starved: missing required source {relative}")
        })?;
        let text = String::from_utf8_lossy(&bytes);
        if !text.contains(marker) {
            return Err(format!(
                "{scope} inventory starved: positive control absent from {relative}"
            ));
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("rs" | "ts" | "tsx" | "js" | "mjs" | "json" | "toml")
    )
}

fn collect_recursive(root: &Path, relative: &str, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let directory = root.join(relative);
    if !directory.is_dir() {
        return Err(format!(
            "production inventory starved: missing source root {relative}"
        ));
    }
    let mut entries: Vec<_> = fs::read_dir(&directory)
        .map_err(|error| format!("production inventory cannot read {relative}: {error}"))?
        .collect::<Result<_, _>>()
        .map_err(|error| format!("production inventory cannot enumerate {relative}: {error}"))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if matches!(
                name,
                "tests" | "test" | "fixtures" | "screenshots" | "examples" | "benches"
            ) {
                continue;
            }
            let nested = path.strip_prefix(root).unwrap().to_string_lossy();
            collect_recursive(root, &nested, files)?;
        } else if is_source_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn count_needles(text: &str, needles: &[&str]) -> usize {
    let needles: Vec<_> = needles
        .iter()
        .map(|needle| needle.to_ascii_lowercase())
        .collect();
    text.lines()
        .map(str::to_ascii_lowercase)
        .filter(|line| needles.iter().any(|needle| line.contains(needle)))
        .count()
}

fn counts_for(files: &[PathBuf]) -> Result<ScopeCounts, String> {
    let mut counts = ScopeCounts::default();
    for path in files {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("inventory cannot read {}: {error}", path.display()))?;
        counts.imports += count_needles(&text, &IMPORT_NEEDLES);
        counts.painters += count_needles(&text, &PAINTER_NEEDLES);
        counts.actions += count_needles(&text, &ACTION_NEEDLES);
        counts.release_capability_rows += count_needles(&text, &RELEASE_ROW_NEEDLES);
    }
    Ok(counts)
}

fn reject_scope_breach(scope: &str, counts: &ScopeCounts) -> Result<(), String> {
    for (label, count) in [
        ("X imports", counts.imports),
        ("X runtime painters", counts.painters),
        ("X installed actions", counts.actions),
        ("X release-capability rows", counts.release_capability_rows),
    ] {
        if count != 0 {
            return Err(format!("{scope} scope breach: {label}={count}, required 0"));
        }
    }
    Ok(())
}

pub fn inventory_production(root: &Path) -> Result<InventoryReport, String> {
    require_anchors(root, "production", &PRODUCTION_ANCHORS)?;
    let mut files = Vec::new();
    for relative in [
        "apps/osl-hub/src",
        "apps/osl-hub-ui/src",
        "src-tauri/src",
        "crates",
    ] {
        collect_recursive(root, relative, &mut files)?;
    }
    files.sort();
    files.dedup();
    if files.len() < PRODUCTION_ANCHORS.len() {
        return Err("production inventory starved: source file census is empty".into());
    }
    let counts = counts_for(&files)?;
    reject_scope_breach("production", &counts)?;
    Ok(InventoryReport {
        scope: "production",
        scanned_files: files.len(),
        installer_files: 0,
        action_files: 0,
        release_manifest_files: 0,
        counts,
    })
}

fn installed_files(root: &Path) -> Result<(Vec<PathBuf>, usize, usize, usize), String> {
    require_anchors(root, "installed", &INSTALLED_ANCHORS)?;
    let installer_relatives = [
        "scripts/build-release-installer.mjs",
        "apps/osl-hub/Cargo.toml",
        "apps/osl-hub/tauri.conf.json",
        ".github/workflows/osl-hub-release.yml",
    ];
    let release_relatives = ["data/pricing.json", "data/public-surface-manifest.json"];
    let mut files: Vec<PathBuf> = installer_relatives
        .iter()
        .chain(release_relatives.iter())
        .map(|relative| root.join(relative))
        .collect();
    let mut action_files = Vec::new();
    for relative in [
        "apps/osl-hub/capabilities",
        "apps/osl-hub/permissions",
        "src-tauri/capabilities",
        "src-tauri/permissions",
    ] {
        let directory = root.join(relative);
        if !directory.is_dir() {
            return Err(format!(
                "installed inventory starved: missing action root {relative}"
            ));
        }
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("installed inventory cannot read {relative}: {error}"))?
        {
            let path = entry
                .map_err(|error| format!("installed inventory cannot enumerate: {error}"))?
                .path();
            if path.is_file() && is_source_file(&path) {
                action_files.push(path);
            }
        }
    }
    if action_files.is_empty() {
        return Err("installed inventory starved: installed action census is empty".into());
    }
    for path in &files {
        if !path.is_file() {
            return Err(format!(
                "installed inventory starved: missing installer/release source {}",
                path.strip_prefix(root).unwrap_or(path).display()
            ));
        }
    }
    files.extend(action_files.iter().cloned());
    files.sort();
    files.dedup();
    Ok((
        files,
        installer_relatives.len(),
        action_files.len(),
        release_relatives.len(),
    ))
}

pub fn inventory_installed(root: &Path) -> Result<InventoryReport, String> {
    let (files, installer_files, action_files, release_manifest_files) = installed_files(root)?;
    let counts = counts_for(&files)?;
    reject_scope_breach("installed", &counts)?;
    Ok(InventoryReport {
        scope: "installed",
        scanned_files: files.len(),
        installer_files,
        action_files,
        release_manifest_files,
        counts,
    })
}

pub fn audit_repository(root: &Path, contract: &Path) -> Result<AuditReport, String> {
    let bytes = fs::read(contract)
        .map_err(|_| format!("X composer contract missing: {}", contract.display()))?;
    let (geometry_keys, type_keys, style_keys, controls) = validate_contract_bytes(&bytes)?;
    let production = inventory_production(root)?;
    let installed = inventory_installed(root)?;
    Ok(AuditReport {
        geometry_keys,
        type_keys,
        style_keys,
        controls,
        production,
        installed,
    })
}

pub fn finding_map(report: &AuditReport) -> BTreeMap<&'static str, usize> {
    BTreeMap::from([
        ("production_imports", report.production.counts.imports),
        ("production_painters", report.production.counts.painters),
        ("production_actions", report.production.counts.actions),
        (
            "production_release_capability_rows",
            report.production.counts.release_capability_rows,
        ),
        ("installed_imports", report.installed.counts.imports),
        ("installed_painters", report.installed.counts.painters),
        ("installed_actions", report.installed.counts.actions),
        (
            "installed_release_capability_rows",
            report.installed.counts.release_capability_rows,
        ),
    ])
}
