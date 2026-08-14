//! TASK 5131 development-only Messenger composer contract and shipping oracle.
//!
//! This module belongs to the isolated carrier-reference tool workspace.  No
//! OSL product crate imports it, and the contract itself lives under `tests/`.

use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const CONTRACT_JSON: &str =
    include_str!("../tests/fixtures/messenger-protected-composer-contract-v1.json");

pub const GEOMETRY_KEYS: [&str; 12] = [
    "composer_min_height_px",
    "composer_padding_block_px",
    "composer_padding_inline_px",
    "leading_control_size_px",
    "control_gap_px",
    "input_min_height_px",
    "input_max_height_px",
    "input_padding_block_px",
    "input_padding_inline_start_px",
    "input_padding_inline_end_px",
    "input_radius_px",
    "send_control_size_px",
];
pub const TYPE_KEYS: [&str; 6] = [
    "font_family",
    "font_size",
    "line_height",
    "font_weight",
    "letter_spacing",
    "placeholder_weight",
];
pub const STYLE_KEYS: [&str; 10] = [
    "composer_background",
    "input_background",
    "input_foreground",
    "placeholder_foreground",
    "control_foreground",
    "active_control_foreground",
    "input_border",
    "input_focus_outline",
    "input_shadow",
    "disabled_opacity",
];
pub const COMPOSER_KEYS: [&str; 3] = [
    "direct-message/ordinary-unprotected",
    "group/ordinary-unprotected",
    "community/ordinary-unprotected",
];
pub const INVENTORY_IDS: [&str; 6] = [
    "production_imports",
    "installer_files",
    "release_manifest_rows",
    "live_providers",
    "painters",
    "installed_actions",
];

#[derive(Clone, Debug, Default)]
pub struct Attacks {
    pub empty_contract: bool,
    pub starve_contract: Option<String>,
    pub starve_inventory: Option<String>,
    pub promote: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Contract {
    schema: String,
    surface: String,
    origin: String,
    component: String,
    status: String,
    shipping_eligible: bool,
    composer_keys: Vec<String>,
    required: Required,
    geometry: BTreeMap<String, Value>,
    #[serde(rename = "type")]
    type_values: BTreeMap<String, Value>,
    style: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Required {
    geometry: Vec<String>,
    #[serde(rename = "type")]
    type_keys: Vec<String>,
    style: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShippingHit {
    pub inventory: &'static str,
    pub path: PathBuf,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckSummary {
    pub composer_keys: usize,
    pub geometry_keys: usize,
    pub type_keys: usize,
    pub style_keys: usize,
    pub inventory_counts: BTreeMap<&'static str, usize>,
}

fn named_error(scope: &str, detail: impl AsRef<str>) -> String {
    format!("TASK5131_SCOPE_BREACH scope={scope} {}", detail.as_ref())
}

fn exact_required<'a>(values: &'a [&'a str]) -> BTreeSet<&'a str> {
    values.iter().copied().collect()
}

fn validate_key_set(
    category: &str,
    declared: &[String],
    values: &BTreeMap<String, Value>,
    required: &[&str],
    numeric: bool,
) -> Result<(), String> {
    let expected = exact_required(required);
    let declared_set = declared.iter().map(String::as_str).collect::<BTreeSet<_>>();
    let actual = values.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if declared.len() != declared_set.len() {
        return Err(named_error(category, "duplicate required key"));
    }
    for key in &expected {
        let full = format!("{category}.{key}");
        if !declared_set.contains(key) {
            return Err(named_error(&full, "required-key inventory is starved"));
        }
        let value = values
            .get(*key)
            .ok_or_else(|| named_error(&full, "contract value is starved"))?;
        let valid = if numeric {
            value
                .as_f64()
                .is_some_and(|number| number.is_finite() && number > 0.0)
        } else {
            value.as_str().is_some_and(|text| !text.trim().is_empty())
        };
        if !valid {
            return Err(named_error(&full, "value has the wrong type or is empty"));
        }
    }
    if let Some(extra) = declared_set.difference(&expected).next() {
        return Err(named_error(
            &format!("{category}.{extra}"),
            "undeclared contract extension",
        ));
    }
    if let Some(extra) = actual.difference(&expected).next() {
        return Err(named_error(
            &format!("{category}.{extra}"),
            "value is absent from required-key inventory",
        ));
    }
    Ok(())
}

fn apply_contract_attack(contract: &mut Contract, attacks: &Attacks) {
    if attacks.empty_contract {
        contract.composer_keys.clear();
        contract.required.geometry.clear();
        contract.required.type_keys.clear();
        contract.required.style.clear();
        contract.geometry.clear();
        contract.type_values.clear();
        contract.style.clear();
    }
    let Some(target) = attacks.starve_contract.as_deref() else {
        return;
    };
    if target == "composer_keys" {
        contract.composer_keys.clear();
        return;
    }
    let Some((category, key)) = target.split_once('.') else {
        return;
    };
    match category {
        "geometry" => {
            contract.geometry.remove(key);
        }
        "type" => {
            contract.type_values.remove(key);
        }
        "style" => {
            contract.style.remove(key);
        }
        _ => {}
    }
}

fn validate_contract(
    contract_json: &str,
    attacks: &Attacks,
) -> Result<(usize, usize, usize, usize), String> {
    let mut contract: Contract = serde_json::from_str(contract_json)
        .map_err(|error| named_error("contract", format!("malformed JSON: {error}")))?;
    apply_contract_attack(&mut contract, attacks);
    if contract.schema != "osl-messenger-protected-composer-contract-v1"
        || contract.surface != "Messenger"
        || contract.origin != "https://www.messenger.com"
        || contract.component != "protected-composer"
        || contract.status != "test-development-contract-only"
        || contract.shipping_eligible
    {
        return Err(named_error(
            "contract_identity",
            "Messenger composer was promoted outside test/development contract-only status",
        ));
    }
    if contract.composer_keys.is_empty() {
        return Err(named_error("composer_keys", "contract is empty"));
    }
    let keys = contract
        .composer_keys
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for key in COMPOSER_KEYS {
        if !keys.contains(key) {
            return Err(named_error(key, "composer contract key is starved"));
        }
    }
    if keys != exact_required(&COMPOSER_KEYS) || keys.len() != contract.composer_keys.len() {
        return Err(named_error(
            "composer_keys",
            "duplicate or uncontracted composer key",
        ));
    }
    validate_key_set(
        "geometry",
        &contract.required.geometry,
        &contract.geometry,
        &GEOMETRY_KEYS,
        true,
    )?;
    validate_key_set(
        "type",
        &contract.required.type_keys,
        &contract.type_values,
        &TYPE_KEYS,
        false,
    )?;
    validate_key_set(
        "style",
        &contract.required.style,
        &contract.style,
        &STYLE_KEYS,
        false,
    )?;
    Ok((
        keys.len(),
        contract.geometry.len(),
        contract.type_values.len(),
        contract.style.len(),
    ))
}

#[derive(Clone)]
struct Inventory {
    id: &'static str,
    roots: Vec<&'static str>,
}

fn inventories() -> Vec<Inventory> {
    vec![
        Inventory {
            id: "production_imports",
            roots: vec![
                "apps/osl-hub/src",
                "apps/osl-hub-ui/src",
                "crates/adapter-profile/src/lib.rs",
            ],
        },
        Inventory {
            id: "installer_files",
            roots: vec![
                "apps/osl-hub/build.rs",
                "apps/osl-hub/tauri.conf.json",
                "apps/osl-hub/nsis",
                "apps/osl-hub/windows",
                "scripts/build-release-installer.mjs",
                "scripts/installer_recipe.py",
            ],
        },
        Inventory {
            id: "release_manifest_rows",
            roots: vec![
                "data/public-surface-manifest.json",
                "apps/osl-hub/src/service_host.rs",
                "apps/osl-hub/src/services.rs",
            ],
        },
        Inventory {
            id: "live_providers",
            roots: vec![
                "apps/osl-hub/src",
                "crates/adapter-profile/src/defaults_web.rs",
                "crates/adapter-profile/src/lib.rs",
            ],
        },
        Inventory {
            id: "painters",
            roots: vec!["apps/osl-hub-ui/src", "apps/osl-hub/src"],
        },
        Inventory {
            id: "installed_actions",
            roots: vec!["apps/osl-hub-ui/src", "apps/osl-hub/src", "crates/ipc/src"],
        },
    ]
}

fn is_development_path(path: &Path) -> bool {
    let text = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    text.contains("/tests/")
        || text.contains("/fixtures/")
        || text.contains("/examples/")
        || text.contains("/qa/")
        || text.ends_with(".test.ts")
        || text.ends_with(".test.mjs")
        || text.ends_with("_test.rs")
}

fn collect_files(path: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        output.push(path.to_path_buf());
        return Ok(());
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("cannot enumerate {}: {error}", path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("inventory entry: {error}"))?;
        let child = entry.path();
        if child.is_dir() {
            let name = child
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if !matches!(name, "node_modules" | "target" | ".git" | "dist") {
                collect_files(&child, output)?;
            }
        } else {
            output.push(child);
        }
    }
    Ok(())
}

fn has_component(text: &str, components: &[&str]) -> bool {
    text.contains("messenger") && components.iter().any(|component| text.contains(component))
}

fn line_is_hit(inventory: &str, path: &Path, line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let file = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    let haystack = format!("{file} {lower}");
    match inventory {
        "production_imports" => {
            let trimmed = lower.trim_start();
            let import = [
                "use ", "pub use ", "mod ", "pub mod ", "import ", "export ", "require(",
            ]
            .iter()
            .any(|prefix| trimmed.starts_with(prefix));
            (import
                && has_component(
                    &haystack,
                    &["composer", "provider", "adapter", "painter", "action"],
                ))
                || haystack.contains("messenger_web_default_profile")
        }
        "installer_files" => {
            has_component(
                &haystack,
                &["composer", "provider", "adapter", "painter", "action"],
            ) || lower.contains("messenger.com")
        }
        "release_manifest_rows" => has_component(
            &lower,
            &[
                "protected composer",
                "live provider",
                "shipping",
                "release claim",
                "supported",
            ],
        ),
        "live_providers" => {
            has_component(
                &haystack,
                &["composer", "provider", "adapter", "compositor"],
            ) && [
                "fn ", "struct ", "enum ", "const ", "static ", "mod ", "use ", "pub ",
            ]
            .iter()
            .any(|term| lower.contains(term))
                || lower.contains("messenger_web_default_profile")
        }
        "painters" => {
            has_component(&haystack, &["composer"])
                && [
                    "paint",
                    "render",
                    "style",
                    "class",
                    "css",
                    "background",
                    "border",
                ]
                .iter()
                .any(|term| lower.contains(term))
        }
        "installed_actions" => {
            has_component(&haystack, &["composer", "provider"])
                && ["action", "send", "dispatch", "command", "install", "invoke"]
                    .iter()
                    .any(|term| lower.contains(term))
        }
        _ => false,
    }
}

fn scan_inventory(repo_root: &Path, inventory: &Inventory) -> Result<Vec<ShippingHit>, String> {
    if inventory.roots.is_empty() {
        return Err(named_error(
            inventory.id,
            "shipping inventory is starved: no roots",
        ));
    }
    let mut files = Vec::new();
    for relative in &inventory.roots {
        let path = repo_root.join(relative);
        if !path.exists() {
            return Err(named_error(
                inventory.id,
                format!("shipping inventory root is starved: {relative}"),
            ));
        }
        collect_files(&path, &mut files).map_err(|error| named_error(inventory.id, error))?;
    }
    files.sort();
    files.dedup();
    let mut hits = Vec::new();
    for path in files {
        if is_development_path(&path) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if line_is_hit(inventory.id, &path, line) {
                hits.push(ShippingHit {
                    inventory: inventory.id,
                    path: path.strip_prefix(repo_root).unwrap_or(&path).to_path_buf(),
                    line: index + 1,
                });
            }
        }
    }
    Ok(hits)
}

pub fn check(
    repo_root: &Path,
    contract_json: &str,
    attacks: &Attacks,
) -> Result<CheckSummary, String> {
    let (composer_keys, geometry_keys, type_keys, style_keys) =
        validate_contract(contract_json, attacks)?;
    let mut specs = inventories();
    let actual_ids = specs.iter().map(|item| item.id).collect::<BTreeSet<_>>();
    if actual_ids != exact_required(&INVENTORY_IDS) || actual_ids.len() != specs.len() {
        return Err(named_error(
            "shipping_inventories",
            "required independent inventory is missing or duplicated",
        ));
    }
    if let Some(starved) = attacks.starve_inventory.as_deref() {
        let Some(spec) = specs.iter_mut().find(|spec| spec.id == starved) else {
            return Err(named_error(starved, "unknown inventory starvation attack"));
        };
        spec.roots.clear();
    }
    let mut counts = BTreeMap::new();
    for spec in &specs {
        let mut hits = scan_inventory(repo_root, spec)?;
        if attacks.promote.as_deref() == Some(spec.id) {
            hits.push(ShippingHit {
                inventory: spec.id,
                path: PathBuf::from(format!("attack://promoted-{}", spec.id)),
                line: 1,
            });
        }
        counts.insert(spec.id, hits.len());
        if let Some(hit) = hits.first() {
            return Err(named_error(
                spec.id,
                format!(
                    "Messenger shipping component count={} first={}:{}",
                    hits.len(),
                    hit.path.display(),
                    hit.line
                ),
            ));
        }
    }
    Ok(CheckSummary {
        composer_keys,
        geometry_keys,
        type_keys,
        style_keys,
        inventory_counts: counts,
    })
}
