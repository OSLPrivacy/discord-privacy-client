use std::path::PathBuf;

use osl_privacy_hub::osl_chat_drag_drop::OslChatAttachmentTray;

fn usage() -> ! {
    eprintln!("usage: task_1335_drag_drop_intake direct-drop [path ...]");
    std::process::exit(2);
}

fn fixture_paths() -> Result<(PathBuf, Vec<PathBuf>), String> {
    let root = std::env::temp_dir().join(format!(
        "osl-task-1335-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "clock unavailable".to_owned())?
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).map_err(|error| format!("create fixture dir: {error}"))?;
    let first = root.join("task-1335-alpha.txt");
    let second = root.join("task-1335-beta.png");
    std::fs::write(&first, b"alpha").map_err(|error| format!("write first fixture: {error}"))?;
    std::fs::write(&second, b"beta").map_err(|error| format!("write second fixture: {error}"))?;
    Ok((root, vec![first, second]))
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("direct-drop") = args.first().map(String::as_str) else {
        usage();
    };

    let (cleanup_root, paths) = if args.len() == 1 {
        fixture_paths().unwrap_or_else(|error| {
            eprintln!("TASK1335_ERROR={error}");
            std::process::exit(1);
        })
    } else {
        (
            PathBuf::new(),
            args.iter().skip(1).map(PathBuf::from).collect(),
        )
    };

    let mut tray = OslChatAttachmentTray::default();
    let receipt = tray.accept_dropped_files(&paths).unwrap_or_else(|error| {
        eprintln!("TASK1335_ERROR={error}");
        std::process::exit(1);
    });

    println!("TASK1335_DIRECT_DROP_COMMAND=direct-drop");
    println!(
        "TASK1335_DROPPED_FILE_COUNT={}",
        receipt.accepted_file_count
    );
    println!(
        "TASK1335_ATTACHMENT_TRAY_FILE_COUNT={}",
        receipt.tray_file_count
    );
    println!(
        "TASK1335_CREATED_MESSAGE_COUNT={}",
        receipt.messages_created
    );
    println!(
        "TASK1335_ACCEPTED_FILENAMES={}",
        receipt.accepted_filenames.join(",")
    );

    if receipt.accepted_file_count != 2
        || receipt.tray_file_count != 2
        || receipt.messages_created != 0
    {
        eprintln!("TASK1335_ERROR=finish line mismatch");
        std::process::exit(1);
    }

    if !cleanup_root.as_os_str().is_empty() {
        let _ = std::fs::remove_dir_all(cleanup_root);
    }
}
