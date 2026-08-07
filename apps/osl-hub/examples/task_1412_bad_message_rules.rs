use std::path::PathBuf;

use osl_privacy_hub::bad_message_rules::{
    read_bad_message_run_selection, save_bad_message_run_selection, BadMessageRunSelection,
};

fn usage() -> ! {
    eprintln!("usage: task_1412_bad_message_rules save-selected [store-dir]");
    std::process::exit(2);
}

fn temp_store_root() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!(
        "osl-task-1412-{}-{}",
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
    let Some("save-selected") = args.first().map(String::as_str) else {
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
            eprintln!("TASK1412_ERROR={error}");
            std::process::exit(1);
        });
        cleanup_root = Some(root.clone());
        root
    };

    let selection = BadMessageRunSelection::new(
        "task-1412-run",
        vec!["harassment".to_owned(), "credential_leak".to_owned()],
        vec!["project-bluebird".to_owned(), "launch-code-17".to_owned()],
    )
    .unwrap_or_else(|error| {
        eprintln!("TASK1412_ERROR={error}");
        std::process::exit(1);
    });

    ipc::main_password::set_file_storage_key(Some([0x14; 32]));
    let receipt = save_bad_message_run_selection(&store_root, &selection).unwrap_or_else(|error| {
        eprintln!("TASK1412_ERROR={error}");
        std::process::exit(1);
    });
    let read_back = read_bad_message_run_selection(&store_root, "task-1412-run")
        .unwrap_or_else(|error| {
            eprintln!("TASK1412_ERROR={error}");
            std::process::exit(1);
        })
        .unwrap_or_else(|| {
            eprintln!("TASK1412_ERROR=selection missing after save");
            std::process::exit(1);
        });

    println!("TASK1412_DIRECT_COMMAND=save-selected");
    println!("TASK1412_RUN_ID={}", read_back.run_id);
    println!("TASK1412_SAVED_RULE_COUNT={}", receipt.saved_rule_count);
    println!(
        "TASK1412_SAVED_RULES={}",
        selection.selected_rules.join(",")
    );
    println!(
        "TASK1412_SAVED_PRIVATE_WORD_COUNT={}",
        receipt.saved_private_word_count
    );
    println!(
        "TASK1412_SAVED_PRIVATE_WORDS={}",
        selection.private_words.join(",")
    );
    println!("TASK1412_READ_RULE_COUNT={}", receipt.read_rule_count);
    println!("TASK1412_READ_RULES={}", read_back.selected_rules.join(","));
    println!(
        "TASK1412_READ_PRIVATE_WORD_COUNT={}",
        receipt.read_private_word_count
    );
    println!(
        "TASK1412_READ_PRIVATE_WORDS={}",
        read_back.private_words.join(",")
    );
    println!(
        "TASK1412_MATCH_TREATMENT={}",
        read_back.match_treatment.as_str()
    );

    if receipt.saved_rule_count != 2
        || receipt.saved_private_word_count != 2
        || receipt.read_rule_count != 2
        || receipt.read_private_word_count != 2
        || read_back != selection
    {
        eprintln!("TASK1412_ERROR=finish line mismatch");
        std::process::exit(1);
    }

    if let Some(root) = cleanup_root {
        let _ = std::fs::remove_dir_all(root);
    }
}
