use std::fs;
use std::path::{Path, PathBuf};

const KNOWN_ORPHANS: &[&str] = &[
    "cloud_autoscrub_authority",
    "cloud_autoscrub_consent",
    "cloud_autoscrub_envelope",
    "cloud_autoscrub_execution",
    "cloud_autoscrub_run",
    "scrub_evidence_manifest",
    "scrub_receipt",
];

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn scrub_modules(source_root: &Path) -> Vec<String> {
    let mut modules = fs::read_dir(source_root)
        .expect("read hub source directory")
        .map(|entry| entry.expect("read hub source entry").path())
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?;
            (path.extension().and_then(|extension| extension.to_str()) == Some("rs")
                && (stem.starts_with("scrub_") || stem.starts_with("cloud_autoscrub_")))
            .then(|| stem.to_owned())
        })
        .collect::<Vec<_>>();
    modules.sort();
    modules
}

fn is_reachable_from_main(source_root: &Path, module: &str) -> bool {
    let main = fs::read_to_string(source_root.join("main.rs")).expect("read hub main entrypoint");
    main.contains(module)
}

#[test]
fn records_every_scrub_module_without_a_non_test_caller() {
    let source_root = source_root();
    let orphaned = scrub_modules(&source_root)
        .into_iter()
        .filter(|module| !is_reachable_from_main(&source_root, module))
        .collect::<Vec<_>>();

    assert_eq!(orphaned, KNOWN_ORPHANS);
}
