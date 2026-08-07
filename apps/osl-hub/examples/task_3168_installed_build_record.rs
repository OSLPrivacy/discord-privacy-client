use std::path::PathBuf;

use osl_privacy_hub::installed_build::{
    store_and_read_installed_build_record, INSTALLED_BUILD_RECORD_FILE,
};

fn usage() -> ! {
    eprintln!(
        "usage: task_3168_installed_build_record read --record-dir <dir> --built-file <path>"
    );
    std::process::exit(2);
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let Some("read") = args.first().map(String::as_str) else {
        usage();
    };
    if args.len() != 5 || args[1] != "--record-dir" || args[3] != "--built-file" {
        usage();
    }

    let record_dir = PathBuf::from(&args[2]);
    let built_file = PathBuf::from(&args[4]);
    let record_path = record_dir.join(INSTALLED_BUILD_RECORD_FILE);
    let record =
        store_and_read_installed_build_record(&record_path, &built_file).unwrap_or_else(|error| {
            eprintln!("TASK3168_ERROR={error}");
            std::process::exit(1);
        });

    println!("TASK3168_DIRECT_COMMAND=read");
    println!("TASK3168_INSTALLED_VERSION={}", record.version);
    println!("TASK3168_INSTALLED_FINGERPRINT={}", record.fingerprint);
    println!("TASK3168_INSTALLED_BUILT_FILE={}", record.built_file);
    println!("TASK3168_RECORD_PATH={}", record_path.display());
}
