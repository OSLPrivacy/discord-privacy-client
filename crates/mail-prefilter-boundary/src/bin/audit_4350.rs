use mail_prefilter_boundary::{audit_live_evidence, AuditEvidence, PROCESS_ENTRY_BOUNDARY};
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!(
            "provider_id=inventory boundary={} failure=usage: audit-4350 <live-evidence.json>",
            PROCESS_ENTRY_BOUNDARY
        );
        return ExitCode::FAILURE;
    };
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "provider_id=inventory boundary={} failure=cannot read evidence: {}",
                PROCESS_ENTRY_BOUNDARY, error
            );
            return ExitCode::FAILURE;
        }
    };
    let evidence: AuditEvidence = match serde_json::from_slice(&bytes) {
        Ok(evidence) => evidence,
        Err(error) => {
            eprintln!(
                "provider_id=inventory boundary={} failure=cannot parse evidence: {}",
                PROCESS_ENTRY_BOUNDARY, error
            );
            return ExitCode::FAILURE;
        }
    };
    match audit_live_evidence(&evidence) {
        Ok(summary) => {
            println!(
                "TASK4350_PASS providers={} ready={} allowed={} disallowed={} body_crossings={} surfaces={} disallowed_post_boundary_hits={}",
                summary.providers,
                summary.ready_providers,
                summary.allowed_messages_per_ready,
                summary.disallowed_messages_per_ready,
                summary.body_crossings_per_ready,
                summary.surfaces_per_provider,
                summary.disallowed_post_boundary_hits
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
