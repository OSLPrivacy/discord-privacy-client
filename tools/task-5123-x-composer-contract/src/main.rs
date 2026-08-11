use std::path::PathBuf;
use std::process::ExitCode;
use task_5123_x_composer_contract::{audit_repository, default_repo_root, CONTRACT_RELATIVE_PATH};

fn main() -> ExitCode {
    let mut root = default_repo_root();
    let mut contract: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--root" => match args.next() {
                Some(value) => root = PathBuf::from(value),
                None => {
                    eprintln!("TASK5123_FAIL --root requires a path");
                    return ExitCode::FAILURE;
                }
            },
            "--contract" => match args.next() {
                Some(value) => contract = Some(PathBuf::from(value)),
                None => {
                    eprintln!("TASK5123_FAIL --contract requires a path");
                    return ExitCode::FAILURE;
                }
            },
            other => {
                eprintln!("TASK5123_FAIL unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let contract = contract.unwrap_or_else(|| root.join(CONTRACT_RELATIVE_PATH));
    match audit_repository(&root, &contract) {
        Ok(report) => {
            println!(
                "TASK5123_CONTRACT geometry_keys={} type_keys={} style_keys={} controls={}",
                report.geometry_keys, report.type_keys, report.style_keys, report.controls
            );
            for inventory in [&report.production, &report.installed] {
                println!(
                    "TASK5123_{}_INVENTORY scanned_files={} installer_files={} action_files={} release_manifest_files={} imports={} painters={} actions={} release_capability_rows={}",
                    inventory.scope.to_ascii_uppercase(),
                    inventory.scanned_files,
                    inventory.installer_files,
                    inventory.action_files,
                    inventory.release_manifest_files,
                    inventory.counts.imports,
                    inventory.counts.painters,
                    inventory.counts.actions,
                    inventory.counts.release_capability_rows
                );
            }
            println!("TASK5123_FINISH contract_only=true x_shippable=false");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("TASK5123_FAIL {error}");
            ExitCode::FAILURE
        }
    }
}
