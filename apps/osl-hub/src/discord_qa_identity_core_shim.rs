pub fn require_installed_device_bound_storage_key() -> Result<[u8; 32], String> {
    ipc::main_password::get_file_storage_key()
        .ok_or_else(|| "OSL QA identity storage key is unavailable".to_owned())
}
