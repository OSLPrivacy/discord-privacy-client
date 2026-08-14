#[path = "../sync_policy.rs"]
mod sync_policy;

use sync_policy::{
    allowed_sync_kinds, check_sync_payload_before_send, refused_sync_kinds, SyncPayloadCheckError,
};

fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("list") => {
            for refused in refused_sync_kinds() {
                println!(
                    "TASK4811_REFUSED kind={} reason={}",
                    refused.kind, refused.reason
                );
            }
            for allowed in allowed_sync_kinds() {
                println!("TASK4811_ALLOWED kind={allowed}");
            }
        }
        Some("check") => {
            let kinds = args.collect::<Vec<_>>();
            if kinds.is_empty() {
                eprintln!("TASK4811_ERROR missing_kind");
                std::process::exit(1);
            }
            match check_sync_payload_before_send(kinds.iter().map(String::as_str)) {
                Ok(()) => println!("TASK4811_CHECK sent=1"),
                Err(SyncPayloadCheckError::Refused { kind, reason }) => {
                    println!("TASK4811_REFUSED_BEFORE_SEND kind={kind} sent=0 reason={reason}");
                    std::process::exit(2);
                }
                Err(SyncPayloadCheckError::UnknownKind { kind }) => {
                    eprintln!("TASK4811_UNKNOWN_KIND kind={kind}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("usage: task-4811-sync-policy-check <list|check KIND...>");
            std::process::exit(1);
        }
    }
}
