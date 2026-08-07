use std::path::PathBuf;

use osl_privacy_hub::run_choices::{build_osl_run_plan, save_osl_run_choices, OslRunChoices};
use osl_privacy_hub::scrub_index::ScrubAccountSelection;

fn usage() -> ! {
    eprintln!("usage: task_1417_optional_file_picker plan [store-dir] [selected-file ...]");
    std::process::exit(2);
}

fn temp_store_root() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "osl-task-1417-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock unavailable".to_owned())?
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).map_err(|error| format!("create fixture dir: {error}"))?;
    Ok(root)
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("plan") = args.first().map(String::as_str) else {
        usage();
    };

    let cleanup_root;
    let store_root = if let Some(path) = args.get(1) {
        cleanup_root = None;
        PathBuf::from(path)
    } else {
        let root = temp_store_root().unwrap_or_else(|error| {
            eprintln!("TASK1417_ERROR={error}");
            std::process::exit(1);
        });
        cleanup_root = Some(root.clone());
        root
    };
    let selected_files = args
        .iter()
        .skip(if args.len() > 1 { 2 } else { 1 })
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let expected_selected_file_count = selected_files.len();

    ipc::main_password::set_file_storage_key(Some([0x17; 32]));

    let selected_real_account_scan = ScrubAccountSelection {
        service_id: "discord".to_owned(),
        account_id: "account-1417".to_owned(),
    };
    let choices = OslRunChoices::watch_live("task-1417-run")
        .and_then(|choices| choices.with_selected_account_scans([selected_real_account_scan]))
        .and_then(|choices| choices.with_selected_file_paths(selected_files))
        .unwrap_or_else(|error| {
            eprintln!("TASK1417_ERROR={error}");
            std::process::exit(1);
        });

    save_osl_run_choices(&store_root, &choices).unwrap_or_else(|error| {
        eprintln!("TASK1417_ERROR={error}");
        std::process::exit(1);
    });
    let plan = build_osl_run_plan(&store_root, "task-1417-run").unwrap_or_else(|error| {
        eprintln!("TASK1417_ERROR={error}");
        std::process::exit(1);
    });
    let real_account_scan = plan
        .selected_account_scans
        .first()
        .map(|scan| format!("{}/{}", scan.service_id, scan.account_id))
        .unwrap_or_else(|| "none".to_owned());

    println!("TASK1417_DIRECT_COMMAND=plan");
    println!(
        "TASK1417_SELECTED_FILE_COUNT={}",
        plan.selected_file_count()
    );
    println!(
        "TASK1417_SELECTED_ACCOUNT_SCAN_COUNT={}",
        plan.selected_account_scan_count()
    );
    println!("TASK1417_SELECTED_REAL_ACCOUNT_SCAN={real_account_scan}");

    if plan.selected_file_count() != expected_selected_file_count
        || plan.selected_account_scan_count() != 1
        || real_account_scan != "discord/account-1417"
    {
        eprintln!("TASK1417_ERROR=finish line mismatch");
        std::process::exit(1);
    }

    if let Some(root) = cleanup_root {
        let _ = std::fs::remove_dir_all(root);
    }
}
