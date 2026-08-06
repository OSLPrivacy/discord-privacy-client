use std::path::PathBuf;

use osl_privacy_hub::run_choices::{
    read_osl_run_file_scan_receipt, save_osl_run_choices, scan_selected_files_for_approved_account,
    OslRunChoices,
};
use osl_privacy_hub::scrub_index::ScrubAccountSelection;

fn usage() -> ! {
    eprintln!("usage: task_1419_break_file_only_start prove [store-dir]");
    std::process::exit(2);
}

fn temp_store_root() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "osl-task-1419-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock unavailable".to_owned())?
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).map_err(|error| format!("create fixture dir: {error}"))?;
    Ok(root)
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("prove") = args.first().map(String::as_str) else {
        usage();
    };
    if args.len() > 2 {
        usage();
    }

    let cleanup_root;
    let store_root = if let Some(path) = args.get(1) {
        cleanup_root = None;
        PathBuf::from(path)
    } else {
        let root = temp_store_root()?;
        cleanup_root = Some(root.clone());
        root
    };

    ipc::main_password::set_file_storage_key(Some([0x19; 32]));

    let maple = store_root.join("maple.txt");
    std::fs::write(&maple, b"fingerprint=MAPLE-4172\n")
        .map_err(|error| format!("write maple fixture: {error}"))?;

    let before_count = read_osl_run_file_scan_receipt(&store_root, "task-1419-run")?
        .map(|receipt| receipt.scanned_file_count)
        .unwrap_or(0);

    let approved_account = ScrubAccountSelection {
        service_id: "discord".to_owned(),
        account_id: "discord-maple".to_owned(),
    };
    let approved_choices = OslRunChoices::watch_live("task-1419-run")?
        .with_selected_account_scans([approved_account])?
        .with_selected_file_paths([maple.clone()])?;
    save_osl_run_choices(&store_root, &approved_choices)?;
    let after = scan_selected_files_for_approved_account(&store_root, "task-1419-run")?;

    let no_account_choices =
        OslRunChoices::watch_live("task-1419-run")?.with_selected_file_paths([maple])?;
    save_osl_run_choices(&store_root, &no_account_choices)?;
    let refused = scan_selected_files_for_approved_account(&store_root, "task-1419-run")
        .expect_err("account none must refuse");
    let final_receipt = read_osl_run_file_scan_receipt(&store_root, "task-1419-run")?
        .ok_or_else(|| "final receipt missing".to_owned())?;
    let first = final_receipt
        .files
        .first()
        .ok_or_else(|| "final first result missing".to_owned())?;

    println!("TASK1419_DIRECT_COMMAND=prove");
    println!("TASK1419_BEFORE_SCANNED_FILE_COUNT={before_count}");
    println!(
        "TASK1419_AFTER_SCANNED_FILE_COUNT={}",
        after.scanned_file_count
    );
    println!("TASK1419_AFTER_FILE={}", after.files[0].file_name);
    println!("TASK1419_AFTER_FINGERPRINT={}", after.files[0].fingerprint);
    println!("TASK1419_AFTER_ACCOUNT={}", after.files[0].account_id);
    println!("TASK1419_REFUSED={refused}");
    println!("TASK1419_FINAL_FIRST_FINGERPRINT={}", first.fingerprint);
    println!(
        "TASK1419_FINAL_SCANNED_FILE_COUNT={}",
        final_receipt.scanned_file_count
    );

    if before_count != 0
        || after.scanned_file_count != 1
        || after.files[0].file_name != "maple.txt"
        || after.files[0].fingerprint != "MAPLE-4172"
        || after.files[0].account_id != "discord-maple"
        || refused != "approved account required"
        || first.fingerprint != "MAPLE-4172"
        || final_receipt.scanned_file_count != 1
    {
        return Err("finish line mismatch".to_owned());
    }

    if let Some(root) = cleanup_root {
        let _ = std::fs::remove_dir_all(root);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("TASK1419_ERROR={error}");
        std::process::exit(1);
    }
}
