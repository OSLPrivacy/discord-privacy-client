use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use ipc::commands::{
    cmd_osl_accept_friend_request, cmd_osl_get_friend_ids, cmd_osl_send_friend_request,
};
use ipc::peer_map::PeerEntry;
use ipc::scope::{Scope, ScopeInput};
use ipc::state::AppState;
use ipc::tofu::KeyBundle;
use keystore::Identity;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static CONFIG_DIR_LOCK: Mutex<()> = Mutex::new(());

const COPY_A_NAME: &str = "OSL Copy A";
const COPY_B_NAME: &str = "OSL Copy B";
const COPY_A_ID: &str = "900000000000022501";
const COPY_B_ID: &str = "900000000000022502";

struct GlobalStateGuard;

impl Drop for GlobalStateGuard {
    fn drop(&mut self) {
        keystore::set_active_account_dir(None);
        keystore::set_base_dir_override(None);
        ipc::main_password::set_file_storage_key(None);
    }
}

struct LocalCopy {
    name: &'static str,
    discord_id: &'static str,
    dir: PathBuf,
    state: AppState,
}

impl LocalCopy {
    fn activate(&self) {
        keystore::set_active_account_dir(Some(self.dir.clone()));
    }

    fn all(&self) -> Vec<String> {
        let mut all = cmd_osl_get_friend_ids(&self.state).expect("All query succeeds");
        all.sort();
        all
    }
}

fn identity_for(label: &str, discord_id: &str) -> Identity {
    let mut identity = keystore::generate_identity(label.to_owned());
    identity.discord_snowflake = Some(discord_id.to_owned());
    identity
}

fn bundle(identity: &Identity) -> KeyBundle {
    KeyBundle {
        ed25519_pub: STANDARD.encode(identity.ed25519_public.as_bytes()),
        x25519_pub: STANDARD.encode(identity.x25519_public.as_bytes()),
        mlkem768_pub: STANDARD.encode(identity.mlkem_public_bytes),
        ratchet_initial_pub: identity
            .ratchet_initial_pub
            .as_ref()
            .map(|public| STANDARD.encode(public.as_bytes())),
    }
}

fn local_copy(
    root: &Path,
    name: &'static str,
    discord_id: &'static str,
    identity: Identity,
    peer_id: &'static str,
    peer_identity: &Identity,
) -> LocalCopy {
    let dir = root.join(name.replace(' ', "-").to_ascii_lowercase());
    std::fs::create_dir_all(&dir).expect("create local copy dir");
    let state = AppState::new();
    state.install_identity(identity);
    state.peer_map.lock().unwrap().insert(
        peer_id.to_owned(),
        PeerEntry {
            discord_id: Some(peer_id.to_owned()),
            tofu_key_bundle: Some(bundle(peer_identity)),
            ..PeerEntry::default()
        },
    );
    LocalCopy {
        name,
        discord_id,
        dir,
        state,
    }
}

#[test]
fn task_0225_request_acceptance_reaches_both_named_copy_all_lists() {
    let _lock = CONFIG_DIR_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _guard = GlobalStateGuard;
    ipc::main_password::set_file_storage_key(Some([0x25; 32]));
    let root = tempfile::tempdir().expect("two local OSL copy root");
    keystore::set_base_dir_override(Some(root.path().to_path_buf()));

    let identity_a = identity_for("task-0225-copy-a", COPY_A_ID);
    let identity_b = identity_for("task-0225-copy-b", COPY_B_ID);
    let copy_a = local_copy(
        root.path(),
        COPY_A_NAME,
        COPY_A_ID,
        identity_a.clone(),
        COPY_B_ID,
        &identity_b,
    );
    let copy_b = local_copy(
        root.path(),
        COPY_B_NAME,
        COPY_B_ID,
        identity_b,
        COPY_A_ID,
        &identity_a,
    );

    copy_a.activate();
    let scope = Scope::dm(COPY_B_ID);
    let sent = cmd_osl_send_friend_request(
        &copy_a.state,
        COPY_B_ID.to_owned(),
        ScopeInput::from(&scope),
    )
    .expect("copy A creates one typed friend request for copy B");

    copy_b.activate();
    cmd_osl_accept_friend_request(&copy_b.state, COPY_A_ID.to_owned(), sent.request.clone())
        .expect("copy B accepts the request by direct command");

    copy_a.activate();
    cmd_osl_accept_friend_request(&copy_a.state, COPY_B_ID.to_owned(), sent.request)
        .expect("copy A records the same accepted request by direct command");

    let copy_a_all = copy_a.all();
    let copy_b_all = copy_b.all();
    let copy_a_contains_b = copy_a_all.iter().any(|id| id == COPY_B_ID);
    let copy_b_contains_a = copy_b_all.iter().any(|id| id == COPY_A_ID);

    println!(
        "TASK0225 request.created_by={} request.peer={} request.scope={}",
        copy_a.name,
        copy_b.discord_id,
        scope.storage_key()
    );
    println!(
        "TASK0225 all_query copy={} exact_other={} contains_other={} all={}",
        copy_a.name,
        COPY_B_ID,
        copy_a_contains_b,
        copy_a_all.join(",")
    );
    println!(
        "TASK0225 all_query copy={} exact_other={} contains_other={} all={}",
        copy_b.name,
        COPY_A_ID,
        copy_b_contains_a,
        copy_b_all.join(",")
    );

    assert!(
        copy_a_contains_b,
        "{} All must include {}",
        copy_a.name, COPY_B_ID
    );
    assert!(
        copy_b_contains_a,
        "{} All must include {}",
        copy_b.name, COPY_A_ID
    );
}
