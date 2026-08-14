use mail_prefilter_boundary::{audit_folder_evidence, FolderAuditEvidence, FOLDER_ACCESS_BOUNDARY};
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!(
            "provider_id=inventory boundary={} failure=usage: audit-4351 <live-evidence.json>",
            FOLDER_ACCESS_BOUNDARY
        );
        return ExitCode::FAILURE;
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "provider_id=inventory boundary={} failure=cannot read evidence: {}",
                FOLDER_ACCESS_BOUNDARY, error
            );
            return ExitCode::FAILURE;
        }
    };
    let evidence: FolderAuditEvidence = match serde_json::from_slice(&bytes) {
        Ok(evidence) => evidence,
        Err(error) => {
            eprintln!(
                "provider_id=inventory boundary={} failure=cannot parse evidence: {}",
                FOLDER_ACCESS_BOUNDARY, error
            );
            return ExitCode::FAILURE;
        }
    };
    match audit_folder_evidence(&evidence) {
        Ok(summary) => {
            println!(
                "TASK4351_PASS provider={} allowlist_count={} catalog_folders={} opened_allowed={} other_folders_opened={} forbidden_refusals={}",
                summary.provider_id,
                summary.allowlist_count,
                summary.catalog_folders,
                summary.opened_allowed_requests,
                summary.other_folders_opened,
                summary.forbidden_refusals
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
