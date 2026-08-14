use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn joined(left: &[u8], right: &[u8]) -> Vec<u8> {
    [left, right].concat()
}

fn private_markers() -> Vec<Vec<u8>> {
    [
        b"PRIVATE KEY-----".as_slice(),
        b"ENCRYPTED PRIVATE KEY-----",
        b"RSA PRIVATE KEY-----",
        b"EC PRIVATE KEY-----",
        b"DSA PRIVATE KEY-----",
        b"OPENSSH PRIVATE KEY-----",
    ]
    .iter()
    .map(|suffix| joined(b"-----BEGIN ", suffix))
    .chain(std::iter::once(joined(b"AGE-SECRET", b"-KEY-")))
    .collect()
}

fn role_secret_names() -> Vec<Vec<u8>> {
    [
        "ROOT",
        "ROOT_1",
        "ROOT_2",
        "ROOT_3",
        "TARGETS",
        "SNAPSHOT",
        "TIMESTAMP",
        "BUILD_PROOF",
        "CARRIER_TABLE",
    ]
    .iter()
    .map(|role| format!("OSL_TUF_{role}_PRIVATE_{}", "KEY").into_bytes())
    .collect()
}

fn tracked_and_untracked(root: &Path) -> Result<Vec<PathBuf>, String> {
    let visible = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not enumerate package inputs: {error}"))?;
    if !visible.status.success() {
        return Err("git ls-files failed".to_owned());
    }
    // Private-key extensions are commonly gitignored. Enumerate those ignored
    // paths explicitly; otherwise the scanner would skip the most likely leak.
    let ignored = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--",
            "*.key",
            "*.pem",
            "*.p8",
            "*.p12",
            "*.pfx",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not enumerate ignored key paths: {error}"))?;
    if !ignored.status.success() {
        return Err("git ignored-key enumeration failed".to_owned());
    }
    let mut paths = BTreeSet::new();
    for output in [&visible.stdout, &ignored.stdout] {
        paths.extend(
            output
                .split(|byte| *byte == 0)
                .filter(|entry| !entry.is_empty())
                .filter_map(|entry| std::str::from_utf8(entry).ok())
                .map(PathBuf::from),
        );
    }
    Ok(paths.into_iter().collect())
}

fn category(path: &Path) -> Option<&'static str> {
    let text = path.to_string_lossy();
    if text.starts_with(".github/") {
        Some("ci")
    } else if text.starts_with("scripts/") || text.starts_with("infra/") {
        Some("builder")
    } else if text.starts_with("docs/") || text == "payment-help.html" {
        Some("website")
    } else if ["apps/", "crates/", "data/", "release-trust/", "src-tauri/"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
    {
        Some("package")
    } else {
        None
    }
}

fn suspicious_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "key" | "p8" | "p12" | "pfx"
            )
        })
}

fn run() -> Result<(), String> {
    let root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir().map_err(|error| error.to_string())?);
    let mut bytes_by_category = BTreeMap::from([
        ("builder", 0_u64),
        ("ci", 0_u64),
        ("package", 0_u64),
        ("website", 0_u64),
    ]);
    let mut files_scanned = 0_u64;
    let mut violations = Vec::new();
    for relative in tracked_and_untracked(&root)? {
        let Some(category) = category(&relative) else {
            continue;
        };
        let path = root.join(&relative);
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("could not scan {}: {error}", relative.display()))?;
        files_scanned += 1;
        let marker = private_markers()
            .iter()
            .any(|marker| contains(&bytes, marker));
        let role_secret = role_secret_names()
            .iter()
            .any(|name| contains(&bytes, name));
        let private_json = relative.starts_with("release-trust")
            && (contains(&bytes, b"\"private\"") || contains(&bytes, b"\"secret\""));
        if suspicious_extension(&relative) || marker || role_secret || private_json {
            *bytes_by_category.get_mut(category).unwrap() += bytes.len() as u64;
            violations.push(relative);
        }
    }
    let total = bytes_by_category.values().sum::<u64>();
    println!(
        "TASK5169_PRIVATE_SCAN files={} ci_private_key_bytes={} builder_private_key_bytes={} website_private_key_bytes={} package_private_key_bytes={} total_private_key_bytes={}",
        files_scanned,
        bytes_by_category["ci"],
        bytes_by_category["builder"],
        bytes_by_category["website"],
        bytes_by_category["package"],
        total,
    );
    if total != 0 {
        return Err(format!("private role key material found in {violations:?}"));
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("TASK5169_PRIVATE_SCAN REFUSED: {error}");
        std::process::exit(1);
    }
}
