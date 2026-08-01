//! Dependency-boundary test for the local sensitive-content classifier.
//!
//! This test examines Cargo's resolved lockfile rather than this crate's
//! source.  The classifier is intentionally dependency-free: adding even a
//! transitive package requires an explicit review because it could introduce
//! network, filesystem, or logging capability around scanned plaintext.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Package {
    name: String,
    dependencies: Vec<String>,
}

fn workspace_lockfile() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("sensitive-classify must be nested under the workspace root")
        .join("Cargo.lock")
}

fn package_stanzas(lockfile: &str) -> Vec<Package> {
    lockfile
        .split("[[package]]")
        .skip(1)
        .filter_map(|stanza| {
            let name = stanza
                .lines()
                .find_map(|line| line.strip_prefix("name = \"")?.strip_suffix('\"'))?
                .to_owned();
            let dependencies = stanza
                .split_once("dependencies = [")
                .map(|(_, rest)| {
                    rest.split(']')
                        .next()
                        .expect("a dependencies array must be closed")
                        .lines()
                        .filter_map(|line| {
                            line.trim()
                                .strip_prefix('\"')?
                                .strip_suffix("\",")
                                .map(|dependency| {
                                    dependency.split_whitespace().next().unwrap().to_owned()
                                })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(Package { name, dependencies })
        })
        .collect()
}

fn manifest_dependency_declarations(manifest: &str) -> Vec<String> {
    let mut declarations = Vec::new();
    let mut in_dependency_section = false;

    for line in manifest.lines() {
        let trimmed = line.trim();
        if let Some(section) = trimmed
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            in_dependency_section = section == "dependencies"
                || section == "build-dependencies"
                || section.ends_with(".dependencies")
                || section.ends_with(".build-dependencies");
            continue;
        }
        if in_dependency_section && !trimmed.is_empty() && !trimmed.starts_with('#') {
            declarations.push(trimmed.to_owned());
        }
    }

    declarations
}

#[test]
fn classifier_resolved_dependency_graph_is_empty() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read sensitive-classify Cargo.toml");
    let declarations = manifest_dependency_declarations(&manifest);
    assert!(
        declarations.is_empty(),
        "sensitive-classify must not declare dependencies; found {declarations:?}"
    );

    let lockfile = fs::read_to_string(workspace_lockfile()).expect("read workspace Cargo.lock");
    let classifier = package_stanzas(&lockfile)
        .into_iter()
        .find(|package| package.name == "sensitive-classify");

    if let Some(classifier) = classifier {
        assert!(
            classifier.dependencies.is_empty(),
            "sensitive-classify must remain dependency-free; resolved dependencies were {:?}",
            classifier.dependencies
        );
    }
}
