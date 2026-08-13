//! TASK 5181b - prove the Windows zone handoff cannot be starved silently.
//!
//! Drives the VERBATIM production zone handoff
//! (`apps/osl-hub/src/download_zone_handoff.rs` and its embedded
//! `download_zone_handoff.ps1`, copied in beside this file) against the REAL
//! Windows Attachment Services COM server, with a real clean executable saved
//! to a real NTFS destination. The harness script runs it three times:
//!
//!   real          - the untouched handoff. Must exit 0 with ZoneId=3 from
//!                   exactly one IAttachmentExecute::Save call.
//!   starved-save  - the embedded helper sabotaged so IAttachmentExecute::Save
//!                   is never called while the helper still reports a save.
//!                   Must exit 1 naming the missing zone handoff.
//!   starved-rust  - the module sabotaged so Attachment Services is never
//!                   invoked at all and a mark is fabricated in memory. Must
//!                   exit 1 naming the missing zone handoff.
//!
//! The mark is read back off the destination by a SEPARATE `powershell.exe`
//! process this harness runs itself, so a handoff that never happened cannot
//! talk its way to a pass.

// The copied production module carries the whole boundary API; this harness
// only drives part of it.
#[allow(dead_code)]
mod download_zone_handoff;

use std::path::{Path, PathBuf};
use std::process::Command;

use download_zone_handoff::{
    windows_path_for, AttachmentServicesSaver, WindowsAttachmentServicesSaver, ZoneHandoffOutcome,
    ZoneHandoffRequest, INTERNET_ZONE_ID, REQUIRED_SAVE_CALLS,
};

const SOURCE_URL: &str = "https://downloads.oslprivacy.example/attachments/osl-5181b-report.exe";
const REFERRER_URL: &str = "https://downloads.oslprivacy.example/attachments/";
const CLEAN_EXECUTABLE_SOURCE: &str = "/mnt/c/Windows/System32/hostname.exe";

fn powershell() -> PathBuf {
    PathBuf::from("/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
}

/// A second, independent observation of the mark: a separate `powershell.exe`
/// process, not the copied helper, reading the stream off the destination.
fn independent_zone_identifier(path: &Path) -> String {
    let windows_path = match windows_path_for(path) {
        Ok(path) => path,
        Err(detail) => return format!("UNTRANSLATABLE({detail})"),
    };
    let command = format!(
        "$ErrorActionPreference='Stop'; try {{ (Get-Content -LiteralPath '{windows_path}' \
         -Stream 'Zone.Identifier' -Raw) -replace \"`r|`n\", '|' }} catch {{ 'ABSENT' }}"
    );
    match Command::new(powershell())
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(command)
        .output()
    {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if text.is_empty() {
                "EMPTY".to_owned()
            } else {
                text
            }
        }
        Err(error) => format!("UNREADABLE({error})"),
    }
}

/// Discard the throwaway destination and leave with a verdict.
fn cleanup(root: &Path, code: i32) -> ! {
    let _ = std::fs::remove_dir_all(root);
    std::process::exit(code)
}

fn fail(reason: &str) -> ! {
    println!("TASK5181B_FAIL the Windows zone handoff is missing: {reason}");
    std::process::exit(1)
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "real".to_owned());
    println!("TASK5181B_MODE={mode}");

    let bytes = match std::fs::read(CLEAN_EXECUTABLE_SOURCE) {
        Ok(bytes) => bytes,
        Err(error) => fail(&format!(
            "the clean executable fixture {CLEAN_EXECUTABLE_SOURCE} is unreadable: {error}"
        )),
    };
    // A real NTFS Windows volume, so a missing mark can only mean a missing
    // handoff and never a filesystem that could not hold one.
    let destination_root = PathBuf::from("/mnt/d").join(format!(
        "osl-task-5181b-{mode}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&destination_root);
    if let Err(error) = std::fs::create_dir_all(&destination_root) {
        fail(&format!("the destination could not be created: {error}"));
    }
    let chosen_path = destination_root.join("osl-5181b-report.exe");
    if let Err(error) = std::fs::write(&chosen_path, &bytes) {
        fail(&format!("the download could not be placed: {error}"));
    }
    println!("TASK5181B_DESTINATION={}", chosen_path.display());
    println!("TASK5181B_DESTINATION_BYTES={}", bytes.len());

    let request = ZoneHandoffRequest {
        local_path: chosen_path.clone(),
        file_name: "osl-5181b-report.exe".to_owned(),
        source_url: SOURCE_URL.to_owned(),
        referrer_url: REFERRER_URL.to_owned(),
    };
    let saver = WindowsAttachmentServicesSaver::new();
    let outcome = saver.save(&request);


    let mark = match outcome {
        Ok(ZoneHandoffOutcome::Marked(mark)) => mark,
        Ok(ZoneHandoffOutcome::PlatformLimited(limited)) => {
            println!("TASK5181B_ZONE_MARKED=false");
            println!("TASK5181B_REASON={}", limited.reason.name());
            println!("TASK5181B_FILESYSTEM={}", limited.filesystem);
            println!(
                "TASK5181B_INDEPENDENT_ZONE_IDENTIFIER={}",
                independent_zone_identifier(&chosen_path)
            );
            println!(
                "TASK5181B_FAIL the Windows zone handoff is missing: {} ({})",
                limited.reason.name(),
                limited.message
            );
            cleanup(&destination_root, 1)
        }
        Err(failure) => {
            println!("TASK5181B_ZONE_MARKED=false");
            println!("TASK5181B_REASON={}", failure.name());
            println!(
                "TASK5181B_INDEPENDENT_ZONE_IDENTIFIER={}",
                independent_zone_identifier(&chosen_path)
            );
            println!(
                "TASK5181B_FAIL the Windows zone handoff is missing: {} ({})",
                failure.name(),
                failure.message()
            );
            cleanup(&destination_root, 1)
        }
    };

    println!("TASK5181B_ZONE_MARKED=true");
    println!("TASK5181B_FILESYSTEM={}", mark.filesystem);
    println!("TASK5181B_WINDOWS_PATH={}", mark.windows_path);
    println!("TASK5181B_ZONE_ID={}", mark.zone_id);
    println!("TASK5181B_SAVE_CALLS={}", mark.save_calls);
    println!("TASK5181B_HR_SAVE={}", mark.save_hresult);
    println!(
        "TASK5181B_ZONE_IDENTIFIER={}",
        mark.zone_identifier_text.replace(['\r', '\n'], "|")
    );

    // The disk, not the struct. A fabricated mark dies here.
    let independent = independent_zone_identifier(&chosen_path);
    println!("TASK5181B_INDEPENDENT_ZONE_IDENTIFIER={independent}");

    if mark.zone_id != INTERNET_ZONE_ID {
        println!(
            "TASK5181B_FAIL the Windows zone handoff is missing: the mark says ZoneId={} and not \
             the Internet zone {INTERNET_ZONE_ID}",
            mark.zone_id
        );
        cleanup(&destination_root, 1)
    }
    if mark.save_calls != REQUIRED_SAVE_CALLS {
        println!(
            "TASK5181B_FAIL the Windows zone handoff is missing: the mark rests on {} \
             IAttachmentExecute::Save calls, not {REQUIRED_SAVE_CALLS}",
            mark.save_calls
        );
        cleanup(&destination_root, 1)
    }
    if !independent.contains(&format!("ZoneId={INTERNET_ZONE_ID}")) {
        println!(
            "TASK5181B_FAIL the Windows zone handoff is missing: an independent reader found no \
             ZoneId={INTERNET_ZONE_ID} on the saved file ({independent})"
        );
        cleanup(&destination_root, 1)
    }
    if !independent.contains(SOURCE_URL) {
        println!(
            "TASK5181B_FAIL the Windows zone handoff is missing: the mark on disk does not name \
             the download source ({independent})"
        );
        cleanup(&destination_root, 1)
    }

    println!("TASK5181B_PASS the saved download carries the Internet-zone mark");
    cleanup(&destination_root, 0)
}
