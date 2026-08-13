use ipc::commands::{cmd_osl_copy_my_history_here, cmd_osl_export_history_for_copy};
use ipc::state::AppState;
use keystore::identity_from_entropy;
use std::path::Path;
use std::process::Command;
use store::{MessageStore, StoredMessage};
use tempfile::TempDir;

const ACCOUNT: &str = "task4805b-account";
const CHANNEL: &str = "task4805b-channel";

fn state(dir: &Path, secret: &[u8; 32], entropy: [u8; 16]) -> AppState {
    let state = AppState::new();
    let mut identity = identity_from_entropy(entropy, ACCOUNT.to_string());
    identity.user_id = ACCOUNT.to_string();
    identity.discord_snowflake = Some(ACCOUNT.to_string());
    state.install_identity(identity);
    *state.message_store.lock().unwrap() =
        Some(MessageStore::open(dir, secret).expect("open store"));
    state
}

fn phrase() -> String {
    bip39::Mnemonic::from_entropy_in(bip39::Language::English, &[0x48; 16])
        .unwrap()
        .to_string()
}

#[test]
fn task_4805b_empty_new_device_and_retry_gate_can_go_red() {
    let source_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let source = state(source_dir.path(), &[0x48; 32], [0x48; 16]);
    let dest = state(dest_dir.path(), &[0x05; 32], [0x05; 16]);
    source
        .message_store
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .put(&StoredMessage {
            discord_message_id: "task4805b-source".to_string(),
            channel_id: CHANNEL.to_string(),
            sender_discord_id: "sender".to_string(),
            sender_osl_user_id: "sender".to_string(),
            plaintext: "TASK4805B nonempty old history".to_string(),
            decrypted_at: 4_805,
            reply_parent_id: None,
            edit_revision: 1,
            burned: false,
        })
        .unwrap();
    assert!(dest
        .message_store
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .list_by_channel(CHANNEL, 10)
        .unwrap()
        .is_empty());
    let package = cmd_osl_export_history_for_copy(
        &source,
        CHANNEL.to_string(),
        vec!["task4805b-source".to_string()],
    )
    .unwrap();
    let first = cmd_osl_copy_my_history_here(&dest, package.clone(), phrase()).unwrap();
    let retry = cmd_osl_copy_my_history_here(&dest, package, phrase()).unwrap();
    assert_eq!(retry, first);
    assert_eq!(
        dest.message_store
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .list_by_channel(CHANNEL, 10)
            .unwrap()
            .len(),
        1
    );

    if std::env::var_os("OSL_TASK4823_NEUTRAL_RETRY_WRITER").is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "task_4805b_neutral_retry_writer_child",
                "--nocapture",
            ])
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(1), "{text}");
        assert!(text.contains("neutral_cache_checkpoint"), "{text}");
        println!("TASK4805B empty_before_copy=0 complete_after_copy=1");
        println!("TASK4805B neutral_retry_writer=neutral_cache_checkpoint exit=1");
    }
}

#[test]
#[ignore = "throwaway child normalizes the deliberately red gate to exit 1"]
fn task_4805b_neutral_retry_writer_child() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "task_4805b_empty_new_device_and_retry_gate_can_go_red",
            "--nocapture",
        ])
        .env("OSL_TASK4823_NEUTRAL_RETRY_WRITER", "1")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(text.contains("neutral_cache_checkpoint"), "{text}");
    eprintln!(
        "TASK4805B unclassified writer=neutral_cache_checkpoint effect=retry_conditional_write"
    );
    std::process::exit(1);
}
