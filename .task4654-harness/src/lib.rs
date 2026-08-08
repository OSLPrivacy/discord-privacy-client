extern crate self as ipc;
extern crate self as keystore;

#[path = "../../apps/osl-hub/src/osl_profile.rs"]
pub mod osl_profile;

pub mod atomic_file {
    use std::path::Path;

    pub fn read_recoverable_bounded(
        path: &Path,
        max_bytes: u64,
        label: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() as u64 <= max_bytes => Ok(Some(bytes)),
            Ok(_) => Err(format!("{label} exceeds its storage limit")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("{label} could not be read: {error}")),
        }
    }

    pub fn write_recoverable(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{label} directory could not be created: {error}"))?;
        }
        std::fs::write(path, bytes).map_err(|error| format!("{label} could not be written: {error}"))
    }
}

pub mod main_password {
    const MAGIC: &[u8] = b"task4654-enc:";

    pub fn get_file_storage_key() -> Option<[u8; 32]> {
        Some([0x51; 32])
    }

    pub fn has_enc_magic(bytes: &[u8]) -> bool {
        bytes.starts_with(MAGIC)
    }

    pub fn encrypt_at_rest(plaintext: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
        let mut sealed = MAGIC.to_vec();
        sealed.extend(
            plaintext
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ key[index % key.len()]),
        );
        Ok(sealed)
    }

    pub fn decrypt_at_rest(sealed: &[u8], key: &[u8; 32]) -> Result<Vec<u8>, String> {
        let body = sealed
            .strip_prefix(MAGIC)
            .ok_or_else(|| "missing magic".to_owned())?;
        Ok(body
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ key[index % key.len()])
            .collect())
    }
}

pub fn active_account_dir() -> Option<std::path::PathBuf> {
    Some(std::env::temp_dir().join("task4654-profile-harness"))
}
