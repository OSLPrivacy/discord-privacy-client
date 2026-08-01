//! T16-E1: freeze the native Pro-gate inventory.
//!
//! This is deliberately a source inventory, not a behavioral proof.  The
//! behavioral tests for each feature own that proof; this alarm makes a new,
//! removed, or relocated `is_paid_equivalent` gate require an explicit audit.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const FROZEN_GATE_CALLS: &[(&str, usize)] = &[
    ("autoscrub_run.rs", 1),
    ("main.rs", 4),
    ("mass_cleanup.rs", 1),
    ("native_attachment_transport.rs", 1),
];

fn source_root() -> PathBuf {
    if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
        return Path::new(manifest_dir).join("src");
    }

    // Keeps this std-only inventory executable without Cargo, which is useful
    // for its required sabotage check in author lanes.
    std::env::current_dir()
        .expect("read current directory")
        .join("apps/osl-hub/src")
}

fn rust_sources(root: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(root).expect("read hub source directory") {
        let path = entry.expect("read hub source entry").path();
        if path.is_dir() {
            sources.extend(rust_sources(&path));
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            sources.push(path);
        }
    }
    sources
}

fn call_count(source: &str) -> usize {
    let needle = "is_paid_equivalent";
    let mut remainder = source;
    let mut count = 0;

    while let Some(index) = remainder.find(needle) {
        let after_name = &remainder[index + needle.len()..];
        let after_whitespace = after_name.trim_start_matches(char::is_whitespace);
        if after_whitespace.starts_with('(') {
            count += 1;
        }
        remainder = after_name;
    }

    count
}

fn production_gate_calls(root: &Path) -> BTreeMap<String, usize> {
    let mut calls = BTreeMap::new();
    for path in rust_sources(root) {
        let source = fs::read_to_string(&path).expect("read hub Rust source");
        let count = call_count(&source);
        if count > 0 {
            let relative = path
                .strip_prefix(root)
                .expect("source is under the hub source root")
                .to_string_lossy()
                .replace('\\', "/");
            calls.insert(relative, count);
        }
    }
    calls
}

#[test]
fn native_pro_gate_inventory_is_closed() {
    let actual = production_gate_calls(&source_root());
    let expected = FROZEN_GATE_CALLS
        .iter()
        .map(|(path, count)| ((*path).to_owned(), *count))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        actual, expected,
        "Pro gate inventory changed. Audit the gate, its Free-tier alternative, and this frozen list. \
         This source scan is an inventory alarm, not proof of gate behavior."
    );
}
