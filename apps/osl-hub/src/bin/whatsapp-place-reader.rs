use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use osl_privacy_hub::services::save_messaging_risk_agreement;
use osl_privacy_hub::whatsapp_place_reader::{
    read_whatsapp_conversation_places, seeded_whatsapp_conversation_places,
};

const OWNER: &str = "task-3024-owner";
const ACCOUNT: &str = "whatsapp-scrub";
const PASSWORD: &str = "task-3024-whatsapp-place-reader";

fn main() -> ExitCode {
    let mode = std::env::args().nth(1).unwrap_or_default();
    let ticked = match mode.as_str() {
        "whatsapp-ticked" => true,
        "whatsapp-unticked" => false,
        _ => {
            eprintln!("usage: whatsapp-place-reader <whatsapp-ticked|whatsapp-unticked>");
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
            save_messaging_risk_agreement(OWNER, "whatsapp", ACCOUNT)?;
        }
        let places = read_whatsapp_conversation_places(
            OWNER,
            ACCOUNT,
            &seeded_whatsapp_conversation_places(),
        )?;
        let mut output = format!(
            "TASK3024_DIRECT_READER=read_whatsapp_conversation_places\nTASK3024_ACCOUNT_TICKED={ticked}\nTASK3024_PLACE_COUNT={}\n",
            places.len()
        );
        for place in places {
            output.push_str(&format!(
                "TASK3024_PLACE name={} kind={} id={}\n",
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
    std::env::temp_dir().join(format!("osl-task-3024-{}-{nanos}", std::process::id()))
}
