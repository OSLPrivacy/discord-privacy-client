use std::fs;
use std::path::{Path, PathBuf};

/// Scrub modules that compile and pass their own tests but have no non-test
/// caller, so they do not ship. This registry exists so that fact stays VISIBLE
/// — adding a name here is an admission, not a fix. Every entry must say why it
/// is unreachable, and entries are meant to be replaced by a reachable module
/// rather than accumulate.
///
/// All of these are the erasure/deletion lane. v1 ships Scrub as DISCOVERY
/// (scan, consent, dry-run, index, grouped review), so live provider deletion
/// being unreachable is the intended v1 scope, not an accident:
///   - `cloud_autoscrub_*`  — cloud-executed AutoScrub: authority, consent,
///     envelope, execution and run. Cloud execution is deferred.
///   - `scrub_erasure_contacts` / `_queue` / `_tracker` — offline erasure-request
///     queueing and follow-up (added by t12-g4), never wired to a caller.
///   - `scrub_evidence_manifest`, `scrub_receipt` — proof-of-deletion artifacts,
///     which cannot be honest until deletion itself ships.
///   - `scrub_hosted_port` — hosted-session deletion port.
///
/// If deletion is ever scoped into a release, these are the modules to wire,
/// and this list is the checklist.
const KNOWN_ORPHANS: &[&str] = &[
    "cloud_autoscrub_authority",
    "cloud_autoscrub_consent",
    "cloud_autoscrub_envelope",
    "cloud_autoscrub_execution",
    "cloud_autoscrub_run",
    "scrub_erasure_contacts",
    "scrub_erasure_queue",
    "scrub_erasure_tracker",
    "scrub_evidence_manifest",
    "scrub_hosted_port",
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
