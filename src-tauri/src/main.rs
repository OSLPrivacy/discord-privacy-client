// Tauri shell entry point.
//
// Layer 9: the main window loads `https://discord.com/app` directly.
// WebView2's default user-data folder under
// `%LOCALAPPDATA%\<bundle-id>\EBWebView` persists cookies across
// restarts so Discord login survives without extra config. Discord
// serves its own CSP via response headers; Tauri's `app.security.csp`
// is `null` so no local CSP gets injected to clash with it.
//
// Layer 10 / Phase 3: the main window is now built programmatically
// (rather than declared in tauri.conf.json) so we can attach an
// `initialization_script` that runs before Discord's bundle loads.
// The script (`injection::BOOT_SCRIPT`) hooks
// `webpackChunkdiscord_app` and source-rewrites the chat-input
// module's sendMessage call site to route outbound content through
// the `osl_encrypt_message` IPC command. Phase 3 of that command is a
// stub returning `"[OSL-STUB] " + plaintext`; Phase 4 wires the real
// crypto crate path.
//
// IPC from the discord.com origin is gated by
// `capabilities/main.json` (Tauri 2 blocks IPC from remote URLs by
// default) — only the `allow-osl-encrypt-message` permission is
// granted, so the existing keystore / crypto / stego commands
// declared below remain non-callable from Discord.
//
// The Tauri attribute glue lives here, not in the `ipc` crate, so
// `ipc` itself has no Tauri dep — keeping its tests portable across
// dev environments without GTK / WebKit system libs.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bootstrap;
mod injection;
mod screenshot;

use ipc::commands::{
    cmd_aead_open, cmd_aead_seal, cmd_fetch_pubkeys, cmd_generate_identity,
    cmd_init_keyserver, cmd_load_identity, cmd_osl_decrypt_message,
    cmd_osl_encrypt_message, cmd_register, cmd_save_identity, cmd_status,
    cmd_stego_decode, cmd_stego_encode, cmd_x25519_diffie_hellman, AeadOpenRequest,
    AeadSealRequest, AeadSealResponse, FetchPubkeysResponse, GenerateIdentityResponse,
    RegisterResponse, StatusResponse, StegoDecodeResponse, StegoEncodeRequest,
    StegoEncodeResponse,
};
use ipc::{AppState, IpcError, IpcResult};
use runtime::ScreenshotProtection;
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

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

/// Layer 10 / Phase 4 entry point. The injected boot script (see
/// `injection::BOOT_SCRIPT`) intercepts outbound `/messages` /
/// `/messages/edit` requests and routes the chat-input plaintext
/// through this IPC command; the returned cover string is then
/// substituted as the request's `content` body.
///
/// Phase 4 wires the real pipeline: recipient resolution
/// (`keystore::recipients`) → per-recipient X25519 ECDH + HKDF →
/// session-key wrap (XChaCha20-Poly1305) → bulk message AEAD →
/// Mode 0 stego encode. The wire-format details and AEAD
/// associated-data strings live alongside the implementation in
/// `ipc::commands::cmd_osl_encrypt_message`.
///
/// Phase 5+ swaps the X25519 ECDH leg for the full PQXDH handshake
/// + Double Ratchet header keys behind the same wire-shape
/// contract: `(channel_id, plaintext, options) -> Result<String,
/// String>` — boot.js requires no further edits.
///
/// Returns `Result<String, String>` (not `IpcResult`) deliberately:
/// the JS bootloader's reject path simulates a network failure
/// (Phase 4 boot.js update), so a flat string error message is the
/// most predictable shape across that boundary. The keystore /
/// crypto commands above use `IpcResult` because they're consumed
/// by typed Rust callers (Layer 11 overlay UI) that benefit from
/// structured error variants. Different audiences, different shapes.
///
/// The body runs in a `spawn_blocking` task because the underlying
/// `KeyServerClient::fetch_pubkeys` call is synchronous (hand-rolled
/// HTTP/1.1 over `std::net::TcpStream`) and will iterate
/// once-per-recipient over network IO; we don't want to block the
/// async runtime.
#[tauri::command]
async fn osl_encrypt_message(
    app: tauri::AppHandle,
    channel_id: String,
    plaintext: String,
    options: serde_json::Value,
) -> Result<String, String> {
    let plaintext_len = plaintext.len();
    tracing::debug!(
        channel_id = %channel_id,
        plaintext_len,
        "osl_encrypt_message Phase 4 invoked"
    );
    let app_handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_encrypt_message(state.inner(), channel_id, plaintext, options)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?;
    if let Err(ref err) = result {
        // Logged at warn so it's visible in `--release` runs without
        // requiring debug filters; downstream callers will already
        // see this surface as a Discord "Failed to send" toast.
        tracing::warn!(
            error = %err,
            "osl_encrypt_message returning error (fail-closed)"
        );
    } else {
        tracing::debug!(
            plaintext_len,
            "osl_encrypt_message succeeded"
        );
    }
    result
}

/// Layer 10 / Phase 5 entry point. The injected boot script
/// (`injection::BOOT_SCRIPT`) subscribes to Discord's FluxDispatcher
/// for `MESSAGE_CREATE` / `MESSAGE_UPDATE` events; when a message's
/// `content` carries the `DPC0::` prefix, the JS hook routes it
/// through this IPC command. On success the JS hook re-dispatches
/// a synthetic `MESSAGE_UPDATE` with the decrypted content;
/// `Err(_)` returns leave the cover string visible (the recipient
/// is not us, the wire is corrupt, or the key is stale).
///
/// `sender_user_id` is the Discord message author's `user.id` —
/// passed through verbatim. v1 closed beta: OSL identities use
/// Discord IDs, so this is also the keyserver lookup key. v2
/// would map Discord ID → OSL UUID locally per peer; see
/// `docs/design/key-server-api.md`.
///
/// Body runs in `spawn_blocking` because the keyserver lookup
/// (cache miss path) is sync HTTP. The
/// `crate::state::SenderPubkeyCache` (5-min TTL) absorbs repeat
/// lookups — first message from a sender pays a roundtrip, the
/// next ~5 minutes' worth are local.
#[tauri::command]
async fn osl_decrypt_message(
    app: tauri::AppHandle,
    channel_id: String,
    sender_user_id: String,
    content: String,
) -> Result<String, String> {
    let content_len = content.len();
    let sender_dbg = sender_user_id.clone();
    tracing::debug!(
        channel_id = %channel_id,
        sender_user_id = %sender_user_id,
        content_len,
        "osl_decrypt_message Phase 5 invoked"
    );
    let app_handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_decrypt_message(state.inner(), channel_id, sender_user_id, content)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?;
    match &result {
        Ok(plaintext) => tracing::debug!(
            sender_user_id = %sender_dbg,
            plaintext_len = plaintext.len(),
            "osl_decrypt_message succeeded"
        ),
        Err(err) => {
            // The common case here is `NoMatchingSlot` (we're not a
            // recipient of a multi-party DM message, or it's a
            // non-OSL message we tried to decrypt anyway).
            // Logged at debug, NOT warn — we expect frequent
            // not-our-message rejections in normal operation.
            tracing::debug!(
                sender_user_id = %sender_dbg,
                error = %err,
                "osl_decrypt_message returning error (cover left in place)"
            );
        }
    }
    result
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
            // Build the main window programmatically so we can attach
            // the Layer 10 injection script via `initialization_script`
            // — Tauri 2 only exposes that on `WebviewWindowBuilder`,
            // not on config-built windows. URL / UA / dimensions
            // moved here from `tauri.conf.json` (which now has
            // `windows: []`).
            //
            // `WebviewUrl::External` parses the discord.com URL and
            // marks the window as remote-content; the IPC bridge for
            // it is gated by `capabilities/main.json` rather than
            // automatically open as it would be for local content.
            let main_url: tauri::Url = "https://discord.com/app"
                .parse()
                .expect("hardcoded discord.com URL parses");
            let window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(main_url),
            )
            .title("Discord Privacy Client")
            .inner_size(1280.0, 800.0)
            .min_inner_size(940.0, 600.0)
            .resizable(true)
            .decorations(true)
            // Pinned UA — see the Layer 9 verification doc. Bump
            // when Discord starts complaining about Chrome 130.
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0",
            )
            // Layer 10 injection. Runs before discord.com's bundle.
            .initialization_script(injection::BOOT_SCRIPT)
            .build()?;

            // Apply screenshot resistance immediately. This runs
            // once at startup; `set_screenshot_protection` lets a
            // future overlay UI toggle it later. The `on_page_load`
            // hook above also re-applies on every subsequent
            // navigation event so newly-spawned WebView2 child HWNDs
            // get the affinity flag too.
            if let Err(e) =
                screenshot::apply_to_window(&window, ScreenshotProtection::On)
            {
                tracing::warn!(
                    ?e,
                    "screenshot protection unavailable at startup; \
                     continuing without it (Windows-only feature)"
                );
            }

            // Layer 10 / Phase 4 autostart: load identity +
            // init_keyserver + register from on-disk config so the
            // first `osl_encrypt_message` call from the Discord
            // webview hits a fully initialised pipeline. Each step
            // is fail-loud (warn-and-continue); see
            // `bootstrap::run_autostart` docs.
            let app_state = app.state::<AppState>();
            bootstrap::run_autostart(app_state.inner());

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
            osl_encrypt_message,
            osl_decrypt_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running discord-privacy-client tauri app");
}
