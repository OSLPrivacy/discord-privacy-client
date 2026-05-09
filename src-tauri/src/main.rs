// Tauri shell entry point.
//
// v1 alpha prototype: registers the IPC command surface (the pure
// functions in `ipc::commands`) as `#[tauri::command]` wrappers, then
// hands off to Tauri's default builder. The Tauri attribute glue lives
// here, not in the `ipc` crate, so `ipc` itself has no Tauri dep —
// keeping its tests portable across dev environments without GTK /
// WebKit system libs.
//
// Layer 9 onward replaces this main with the discord.com-loading
// shell + injection hooks.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ipc::commands::{
    cmd_aead_open, cmd_aead_seal, cmd_fetch_pubkeys, cmd_generate_identity,
    cmd_init_keyserver, cmd_load_identity, cmd_register, cmd_save_identity,
    cmd_status, cmd_stego_decode, cmd_stego_encode, cmd_x25519_diffie_hellman,
    AeadOpenRequest, AeadSealRequest, AeadSealResponse, FetchPubkeysResponse,
    GenerateIdentityResponse, RegisterResponse, StatusResponse, StegoDecodeResponse,
    StegoEncodeRequest, StegoEncodeResponse,
};
use ipc::{AppState, IpcError, IpcResult};
use tauri::{Manager, State};

#[tauri::command]
async fn generate_identity(
    state: State<'_, AppState>,
    user_id: String,
) -> IpcResult<GenerateIdentityResponse> {
    cmd_generate_identity(state.inner(), user_id)
}

#[tauri::command]
async fn load_identity(
    state: State<'_, AppState>,
    path: String,
) -> IpcResult<GenerateIdentityResponse> {
    cmd_load_identity(state.inner(), path)
}

#[tauri::command]
async fn save_identity(state: State<'_, AppState>, path: String) -> IpcResult<()> {
    cmd_save_identity(state.inner(), path)
}

#[tauri::command]
async fn init_keyserver(state: State<'_, AppState>, base_url: String) -> IpcResult<()> {
    cmd_init_keyserver(state.inner(), base_url)
}

#[tauri::command]
async fn register(app: tauri::AppHandle) -> IpcResult<RegisterResponse> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_register(state.inner())
    })
    .await
    .map_err(|e| IpcError::Crypto(format!("join error: {e}")))?
}

#[tauri::command]
async fn fetch_pubkeys(
    app: tauri::AppHandle,
    user_id: String,
) -> IpcResult<FetchPubkeysResponse> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_fetch_pubkeys(state.inner(), user_id)
    })
    .await
    .map_err(|e| IpcError::Crypto(format!("join error: {e}")))?
}

#[tauri::command]
async fn aead_seal(req: AeadSealRequest) -> IpcResult<AeadSealResponse> {
    cmd_aead_seal(req)
}

#[tauri::command]
async fn aead_open(req: AeadOpenRequest) -> IpcResult<AeadSealResponse> {
    cmd_aead_open(req)
}

#[tauri::command]
async fn stego_encode(req: StegoEncodeRequest) -> IpcResult<StegoEncodeResponse> {
    cmd_stego_encode(req)
}

#[tauri::command]
async fn stego_decode(stego_message: String) -> IpcResult<StegoDecodeResponse> {
    cmd_stego_decode(stego_message)
}

#[tauri::command]
async fn status(state: State<'_, AppState>) -> IpcResult<StatusResponse> {
    Ok(cmd_status(state.inner()))
}

#[tauri::command]
async fn x25519_diffie_hellman(
    secret_b64: String,
    peer_public_b64: String,
) -> IpcResult<String> {
    cmd_x25519_diffie_hellman(secret_b64, peer_public_b64)
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            generate_identity,
            load_identity,
            save_identity,
            init_keyserver,
            register,
            fetch_pubkeys,
            aead_seal,
            aead_open,
            stego_encode,
            stego_decode,
            status,
            x25519_diffie_hellman,
        ])
        .run(tauri::generate_context!())
        .expect("error while running discord-privacy-client tauri app");
}
