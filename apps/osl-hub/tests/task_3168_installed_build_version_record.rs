use osl_privacy_hub::installed_build_version::{
    cmd_read_installed_build_version_record, write_installed_build_version_record_at_start,
};

#[test]
fn direct_command_returns_installed_version_and_fingerprint_that_changes_with_one_byte() {
    let dir = tempfile::TempDir::new().unwrap();
    let built_file = dir.path().join("installed-build.bin");
    let record_path = dir.path().join("installed-build-version.json");

    std::fs::write(&built_file, b"installed-build-A").unwrap();
    let first_written =
        write_installed_build_version_record_at_start(&record_path, &built_file).unwrap();
    let first_read = cmd_read_installed_build_version_record(&record_path).unwrap();
    assert_eq!(first_read, first_written);

    std::fs::write(&built_file, b"installed-build-B").unwrap();
    let second_written =
        write_installed_build_version_record_at_start(&record_path, &built_file).unwrap();
    let second_read = cmd_read_installed_build_version_record(&record_path).unwrap();
    assert_eq!(second_read, second_written);

    assert_eq!(first_read.installed_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(second_read.installed_version, env!("CARGO_PKG_VERSION"));
    assert_ne!(
        first_read.built_file_fingerprint,
        second_read.built_file_fingerprint
    );

    println!(
        "TASK3168 direct_command=cmd_read_installed_build_version_record installed_version={} first_fingerprint={} second_fingerprint={} one_byte_change_fingerprint_changed={}",
        second_read.installed_version,
        first_read.built_file_fingerprint,
        second_read.built_file_fingerprint,
        first_read.built_file_fingerprint != second_read.built_file_fingerprint
    );
}
