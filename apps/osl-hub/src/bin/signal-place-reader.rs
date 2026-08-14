use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::signal_place_reader::{
    read_signal_conversation_places, seeded_signal_conversation_places,
};

const OWNER: &str = "task-3020-owner";
const ACCOUNT: &str = "signal-scrub";
const PASSWORD: &str = "task-3020-signal-place-reader";

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let ticked = match mode.as_str() {
        "signal-ticked" => true,
        "signal-unticked" => false,
        _ => {
            eprintln!("usage: signal-place-reader <signal-ticked|signal-unticked>");
            return ExitCode::from(2);
        }
    };

    match render(ticked) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn render(ticked: bool) -> Result<String, String> {
    let root = temporary_root();
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    keystore::set_base_dir_override(Some(root.clone()));
    keystore::set_active_account_dir(None);
    ipc::main_password::set_file_storage_key(None);

    let result = (|| {
        ipc::main_password::set_main_password(&root, PASSWORD)?;
        let account_dir = root.join("account");
        std::fs::create_dir(&account_dir).map_err(|error| error.to_string())?;
        keystore::set_active_account_dir(Some(account_dir));
        if ticked {
            save_messaging_risk_agreement(OWNER, "signal", ACCOUNT)?;
        }
        let places =
            read_signal_conversation_places(OWNER, ACCOUNT, &seeded_signal_conversation_places())?;
        let mut output = format!(
            "TASK3020_DIRECT_READER=read_signal_conversation_places\nTASK3020_ACCOUNT_TICKED={ticked}\nTASK3020_PLACE_COUNT={}\n",
            places.len()
        );
        for place in places {
            output.push_str(&format!(
                "TASK3020_PLACE name={} kind={} id={}\n",
                place.label,
                place.place_kind.as_str(),
                place.place_id,
            ));
        }
        Ok(output)
    })();

    keystore::set_active_account_dir(None);
    keystore::set_base_dir_override(None);
    ipc::main_password::set_file_storage_key(None);
    let _ = std::fs::remove_dir_all(&root);
    result
}

fn temporary_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("osl-task-3020-{}-{nanos}", std::process::id()))
}
