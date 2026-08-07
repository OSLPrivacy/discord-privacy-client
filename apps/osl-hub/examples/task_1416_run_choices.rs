use std::path::PathBuf;

use osl_privacy_hub::run_choices::{build_osl_run_plan, save_osl_run_choices, OslRunChoices};

fn usage() -> ! {
    eprintln!("usage: task_1416_run_choices plan [store-dir]");
    std::process::exit(2);
}

fn temp_store_root() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "osl-task-1416-{}-{}",
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
    if args.len() > 2 {
        usage();
    }

    let cleanup_root;
    let store_root = if let Some(path) = args.get(1) {
        cleanup_root = None;
        PathBuf::from(path)
    } else {
        let root = temp_store_root().unwrap_or_else(|error| {
            eprintln!("TASK1416_ERROR={error}");
            std::process::exit(1);
        });
        cleanup_root = Some(root.clone());
        root
    };

    ipc::main_password::set_file_storage_key(Some([0x16; 32]));

    let watch_live = OslRunChoices::watch_live("task-1416-watch-live").unwrap_or_else(|error| {
        eprintln!("TASK1416_ERROR={error}");
        std::process::exit(1);
    });
    save_osl_run_choices(&store_root, &watch_live).unwrap_or_else(|error| {
        eprintln!("TASK1416_ERROR={error}");
        std::process::exit(1);
    });
    let watch_plan =
        build_osl_run_plan(&store_root, "task-1416-watch-live").unwrap_or_else(|error| {
            eprintln!("TASK1416_ERROR={error}");
            std::process::exit(1);
        });

    let downloaded = store_root.join("task-1416-downloaded-file.txt");
    std::fs::write(&downloaded, b"downloaded").unwrap_or_else(|error| {
        eprintln!("TASK1416_ERROR=write downloaded-file fixture: {error}");
        std::process::exit(1);
    });
    let background =
        OslRunChoices::run_in_background("task-1416-background").unwrap_or_else(|error| {
            eprintln!("TASK1416_ERROR={error}");
            std::process::exit(1);
        });
    save_osl_run_choices(&store_root, &background.with_downloaded_file(&downloaded))
        .unwrap_or_else(|error| {
            eprintln!("TASK1416_ERROR={error}");
            std::process::exit(1);
        });
    let background_plan =
        build_osl_run_plan(&store_root, "task-1416-background").unwrap_or_else(|error| {
            eprintln!("TASK1416_ERROR={error}");
            std::process::exit(1);
        });

    println!("TASK1416_DIRECT_COMMAND=plan");
    println!(
        "TASK1416_WATCH_LIVE_CHOSEN_VIEW={}",
        watch_plan.chosen_view.as_str()
    );
    println!(
        "TASK1416_WATCH_LIVE_FILE_PATH={}",
        watch_plan
            .downloaded_file_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_owned())
    );
    println!(
        "TASK1416_RUN_IN_BACKGROUND_CHOSEN_VIEW={}",
        background_plan.chosen_view.as_str()
    );
    println!(
        "TASK1416_RUN_IN_BACKGROUND_FILE_PATH={}",
        background_plan
            .downloaded_file_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_owned())
    );

    if watch_plan.chosen_view.as_str() != "watch-live"
        || watch_plan.downloaded_file_path.is_some()
        || background_plan.chosen_view.as_str() != "run-in-background"
        || background_plan.downloaded_file_path.is_none()
    {
        eprintln!("TASK1416_ERROR=finish line mismatch");
        std::process::exit(1);
    }

    if let Some(root) = cleanup_root {
        let _ = std::fs::remove_dir_all(root);
    }
}
