use mail_prefilter_boundary::task_5902_audit::{audit_task_5902, Task5902Evidence};
use mail_prefilter_boundary::PROCESS_ENTRY_BOUNDARY;
use std::env;
use std::fs;
use std::process::ExitCode;

fn reject(reason: impl std::fmt::Display) -> ExitCode {
    eprintln!(
        "provider_id=gmail hostile_id=inventory first_crossed_byte=none boundary={} failure={reason}",
        PROCESS_ENTRY_BOUNDARY
    );
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        return reject("usage: audit-5902 <real-provider-evidence.json>");
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return reject(format!("cannot read evidence: {error}")),
    };
    let evidence: Task5902Evidence = match serde_json::from_slice(&bytes) {
        Ok(evidence) => evidence,
        Err(error) => return reject(format!("cannot parse evidence: {error}")),
    };
    match audit_task_5902(&evidence) {
        Ok(summary) => {
            println!(
                "TASK5902_PASS providers={} candidate_ready={} genuine_bodies={} hostile_attacks={} hostile_body_fetch_bytes={} hostile_process_entry_body_bytes={} refusals={} proof_bindings=signature+digest+conversation+provider_message_id+recipient",
                summary.providers,
                summary.candidate_ready,
                summary.genuine_bodies,
                summary.hostile_attacks,
                summary.hostile_body_fetch_bytes,
                summary.hostile_process_entry_body_bytes,
                summary.refusals,
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
