use release_trust::verify_repository;
use std::path::PathBuf;

const DEFAULT_NOW: u64 = 1_786_435_200; // 2026-08-11T00:00:00Z

fn main() {
    let trust_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("release-trust"));
    match verify_repository(&trust_dir, DEFAULT_NOW) {
        Ok(report) => {
            println!(
                "TASK5169 root_keys={} root_threshold={} roles={} artifacts={}",
                report.root_keys,
                report.root_threshold,
                report.verified_roles.join(","),
                report
                    .artifacts
                    .iter()
                    .map(|artifact| format!("{}:{}", artifact.role, artifact.path))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
        Err(error) => {
            eprintln!("TASK5169 REFUSED: {error}");
            std::process::exit(1);
        }
    }
}
