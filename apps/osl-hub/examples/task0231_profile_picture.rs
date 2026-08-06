#![cfg(feature = "core")]

use osl_privacy_hub::osl_profile;

const OWNER: &str = "task0231-owner";
const IMAGE: &str = "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";

struct Guard {
    root: std::path::PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn install_disposable_unlocked_account() -> Result<Guard, String> {
    let root = std::env::temp_dir().join(format!(
        "osl-task0231-profile-picture-{}",
        std::process::id()
    ));
    let account = root.join("account");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&account)
        .map_err(|error| format!("create disposable account: {error}"))?;
    keystore::set_base_dir_override(Some(root.clone()));
    keystore::set_active_account_dir(Some(account));
    ipc::main_password::set_file_storage_key(Some([0x23; 32]));
    Ok(Guard { root })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("TASK0231 error={error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let _guard = install_disposable_unlocked_account()?;

    osl_profile::set_active_profile_picture(OWNER, IMAGE.to_owned())?;
    let after_set = osl_profile::read_active_profile_picture(OWNER)?;
    println!("TASK0231 set_then_read={}", after_set.status());

    osl_profile::clear_active_profile_picture(OWNER)?;
    let after_clear = osl_profile::read_active_profile_picture(OWNER)?;
    println!("TASK0231 clear_then_read={}", after_clear.status());

    Ok(())
}
