// Tauri shell entry point.
//
// Layer 9: the main window loads `https://discord.com/app` directly
// (configured via `windows[0].url` in `tauri.conf.json`). WebView2's
// default user-data folder under `%LOCALAPPDATA%\<bundle-id>\EBWebView`
// persists cookies across restarts, so Discord login survives without
// extra config. Discord serves its own CSP via response headers;
// Tauri's `app.security.csp` is `null` so no local CSP gets injected
// to clash with it (Tauri only injects into local content anyway, but
// keeping it null documents the layer-9 intent).
//
// Layer 10 will add Discord injection hooks; Layer 11 the encryption
// UI overlays. This file currently exposes:
//   - the IPC command surface (the pure functions in `ipc::commands`)
//     wired as `#[tauri::command]` wrappers, and
//   - the screenshot-resistance wiring (parent + WebView2 descendants
//     via `runtime::apply_to_hwnd_and_children`), re-applied on every
//     page load so newly-created WebView2 child HWNDs are covered.
//
// The Tauri attribute glue lives here, not in the `ipc` crate, so
// `ipc` itself has no Tauri dep — keeping its tests portable across
// dev environments without GTK / WebKit system libs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod screenshot;

use ipc::commands::{
    cmd_aead_open, cmd_aead_seal, cmd_fetch_pubkeys, cmd_generate_identity,
    cmd_init_keyserver, cmd_load_identity, cmd_register, cmd_save_identity,
    cmd_status, cmd_stego_decode, cmd_stego_encode, cmd_x25519_diffie_hellman,
    AeadOpenRequest, AeadSealRequest, AeadSealResponse, FetchPubkeysResponse,
    GenerateIdentityResponse, RegisterResponse, StatusResponse, StegoDecodeResponse,
    StegoEncodeRequest, StegoEncodeResponse,
};
use ipc::{AppState, IpcError, IpcResult};
use runtime::ScreenshotProtection;
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

/// Tauri command: turn screenshot capture protection on or off for
/// the main webview window. Wraps `SetWindowDisplayAffinity` on
/// Windows; no-op on non-Windows targets.
#[tauri::command]
async fn set_screenshot_protection(
    app: tauri::AppHandle,
    enabled: bool,
) -> IpcResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| IpcError::Crypto("main window not present".into()))?;
    let protection = if enabled {
        ScreenshotProtection::On
    } else {
        ScreenshotProtection::Off
    };
    screenshot::apply_to_window(&window, protection)
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        // Tauri 2: app-level page-load hook fires for every page-load
        // event on every webview window. We use it to re-apply
        // screenshot protection on every navigation: WebView2's child
        // HWND tree (Chrome_WidgetWin_* host + render surface) may not
        // be fully constructed at `app.setup` time when navigation to
        // `discord.com/app` is still in flight, and any new child
        // HWNDs Discord's SPA spawns later (embeds, modals, popouts)
        // need the affinity flag applied as well.
        //
        // `apply_to_hwnd_and_children` is idempotent on the parent
        // and best-effort on descendants, so re-applying is safe and
        // the `PageLoadPayload::Started` vs `Finished` distinction
        // doesn't matter — we want the flag set on whatever children
        // exist at each event.
        //
        // Lives on the Builder rather than the WebviewWindow because
        // Tauri 2 only exposes `on_page_load` on the builder side
        // (`WebviewBuilder::on_page_load` during construction or
        // `Builder::on_page_load` for app-wide). `WebviewWindow`
        // post-creation doesn't have a page-load listener API.
        .on_page_load(|webview, _payload| {
            // Tauri 2's `Builder::on_page_load` callback delivers
            // `&tauri::Webview` (not `&WebviewWindow`) because page
            // loads are webview-scoped — Tauri 2 supports multiple
            // webviews per window. `apply_to_webview` extracts the
            // WebView2 surface HWND and walks descendants from there.
            if let Err(e) =
                screenshot::apply_to_webview(webview, ScreenshotProtection::On)
            {
                tracing::debug!(
                    ?e,
                    "screenshot protection re-apply on page load failed",
                );
            }
        })
        .setup(|app| {
            // Apply screenshot resistance to the main webview window
            // as soon as it exists. This runs once at startup; the
            // `set_screenshot_protection` command lets JS toggle it
            // later (e.g. when entering an unencrypted DM, the user
            // may want to relax protection). The `on_page_load` hook
            // above re-applies on every subsequent navigation.
            if let Some(window) = app.get_webview_window("main") {
                if let Err(e) =
                    screenshot::apply_to_window(&window, ScreenshotProtection::On)
                {
                    tracing::warn!(
                        ?e,
                        "screenshot protection unavailable at startup; \
                         continuing without it (Windows-only feature)"
                    );
                }
            }
            Ok(())
        })
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
            set_screenshot_protection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running discord-privacy-client tauri app");
}
