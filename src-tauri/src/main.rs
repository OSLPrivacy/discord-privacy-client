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
    cmd_aead_open, cmd_aead_seal, cmd_fetch_pubkeys, cmd_generate_identity, cmd_init_keyserver,
    cmd_load_identity, cmd_osl_apply_burn, cmd_osl_bulk_set_whitelist,
    cmd_osl_bulk_unwhitelist_scope, cmd_osl_burn_engage, cmd_osl_burn_message,
    cmd_osl_burn_password_status, cmd_osl_burn_scope_data, cmd_osl_change_main_password,
    cmd_osl_clear_license, cmd_osl_decrypt_message_v2, cmd_osl_encrypt_message,
    cmd_osl_encrypt_message_v2, cmd_osl_get_identity_info, cmd_osl_get_license_state,
    cmd_osl_get_scope_encryption_state, cmd_osl_get_scope_whitelist_summary,
    cmd_osl_get_self_user_id, cmd_osl_get_tier_gate_status, cmd_osl_list_all_whitelists,
    cmd_osl_list_burned_scopes, cmd_osl_load_channel_history, cmd_osl_lockout_status,
    cmd_osl_mark_scope_burned, cmd_osl_password_status, cmd_osl_persist_edit,
    cmd_osl_register_self_snowflake, cmd_osl_remove_burn_password, cmd_osl_remove_main_password,
    cmd_osl_remove_stealth_password, cmd_osl_send_burn_marker, cmd_osl_set_burn_password,
    cmd_osl_set_main_password, cmd_osl_set_main_password_after_recovery,
    cmd_osl_set_stealth_password, cmd_osl_set_whitelist, cmd_osl_stealth_mode_engage,
    cmd_osl_stealth_password_status, cmd_osl_toggle_scope_encryption, cmd_osl_unburn_scope,
    cmd_osl_unwhitelist_scope, cmd_osl_validate_license, cmd_osl_verify_gate_password,
    cmd_osl_verify_main_password, cmd_osl_verify_recovery_phrase, cmd_osl_view_recovery_phrase,
    cmd_register, cmd_save_identity, cmd_status, cmd_stego_decode, cmd_stego_encode,
    cmd_x25519_diffie_hellman, AeadOpenRequest, AeadSealRequest, AeadSealResponse,
    BurnScopeDataDto, BurnedScopeDto, FetchPubkeysResponse, GateVerifyDto,
    GenerateIdentityResponse, IdentityInfoDto, LockoutStatusDto, PasswordStatusDto,
    RegisterResponse, ScopeEncryptionState, ScopeWhitelistSummary, StatusResponse,
    StegoDecodeResponse, StegoEncodeRequest, StegoEncodeResponse, StoredMessageDto,
    TierGateStatusDto, WhitelistRowDto,
};
use ipc::scope::ScopeInput;
use ipc::{AppState, IpcError, IpcResult};
use runtime::ScreenshotProtection;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
// Phase F0: deep-link plugin extension trait. Brings the
// `.deep_link()` method into scope on `&AppHandle`, returning the
// plugin's runtime handle from which we register the `on_open_url`
// callback in `setup`.
use tauri_plugin_deep_link::DeepLinkExt;

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
async fn fetch_pubkeys(app: tauri::AppHandle, user_id: String) -> IpcResult<FetchPubkeysResponse> {
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
async fn x25519_diffie_hellman(secret_b64: String, peer_public_b64: String) -> IpcResult<String> {
    cmd_x25519_diffie_hellman(secret_b64, peer_public_b64)
}

/// Tauri command: turn screenshot capture protection on or off for
/// the main webview window. Wraps `SetWindowDisplayAffinity` on
/// Windows; no-op on non-Windows targets.
#[tauri::command]
async fn set_screenshot_protection(app: tauri::AppHandle, enabled: bool) -> IpcResult<()> {
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
        tracing::debug!(plaintext_len, "osl_encrypt_message succeeded");
    }
    result
}

/// Layer 10 / Phase 5 entry point. The injected boot script
/// (`injection::BOOT_SCRIPT`) installs a DOM `MutationObserver`
/// on `document.body`; when a rendered message's first text
/// child carries the `DPC0::` prefix, the JS hook routes it
/// through this IPC command. On success the JS hook replaces
/// `textContent` in place with the decrypted plaintext;
/// `Err(_)` returns leave the cover string visible (the
/// recipient is not us, the wire is corrupt, the sender isn't
/// in `peer_map.json`, or the key is stale).
///
/// `sender_discord_id` is the raw Discord snowflake the boot.js
/// observer extracted from the message DOM (`data-author-id` or
/// avatar-URL fallback). The IPC layer translates it to an OSL
/// `user_id` via `AppState::peer_map` (loaded at bootstrap from
/// `<osl_config_dir>/peer_map.json`) before any keyserver call.
/// An unmapped id surfaces as
/// `OSL: no peer mapping for discord_id=…` and the JS hook
/// handles it silently with a one-time onboarding hint per
/// unique discord_id.
///
/// Body runs in `spawn_blocking` because the keyserver lookup
/// (cache miss path) is sync HTTP. The
/// `crate::state::SenderPubkeyCache` (30-min TTL, keyed by OSL
/// user_id post-resolution) absorbs repeat lookups — first
/// message from a peer pays a roundtrip, the next half-hour's
/// worth are local.
#[tauri::command]
async fn osl_decrypt_message(
    app: tauri::AppHandle,
    channel_id: String,
    sender_discord_id: String,
    content: String,
    discord_message_id: Option<String>,
    scope_input: Option<ScopeInput>,
) -> Result<String, String> {
    let content_len = content.len();
    let sender_dbg = sender_discord_id.clone();
    tracing::debug!(
        channel_id = %channel_id,
        sender_discord_id = %sender_discord_id,
        content_len,
        message_id_present = discord_message_id.is_some(),
        scope_present = scope_input.is_some(),
        "osl_decrypt_message Phase 5/7 invoked"
    );
    let app_handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        // Phase 7b: cmd_osl_decrypt_message_v2 peeks the wire
        // version and routes to the v=1 path or the v=2 path
        // (which dispatches on msg_type). The optional scope is
        // used by the v=2 should_decrypt_from gate.
        let config_dir = keystore::osl_config_dir().ok();
        cmd_osl_decrypt_message_v2(
            state.inner(),
            discord_message_id,
            channel_id,
            sender_discord_id,
            content,
            scope_input,
            config_dir,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?;
    match &result {
        Ok(plaintext) => tracing::debug!(
            sender_discord_id = %sender_dbg,
            plaintext_len = plaintext.len(),
            "osl_decrypt_message succeeded"
        ),
        Err(err) => {
            // The common cases here are `UnknownSender` (peer
            // not in peer_map.json) and `NoMatchingSlot` (we're
            // not a recipient of a multi-party DM message, or
            // it's a non-OSL message). Logged at debug, NOT
            // warn — we expect frequent not-our-message
            // rejections in normal operation.
            tracing::debug!(
                sender_discord_id = %sender_dbg,
                error = %err,
                "osl_decrypt_message returning error (cover left in place)"
            );
        }
    }
    result
}

/// Layer 10 / Phase 5b2 IPC entry point: list previously
/// decrypted messages for a channel from the at-rest-encrypted
/// store, newest-first. boot.js calls this on channel-switch /
/// fresh-load to rehydrate the scrollback so prior decryptions
/// survive Discord refresh + Tauri restart.
///
/// Returns an empty vector when the store is disabled (open
/// failed at bootstrap) so the JS hook can render that as
/// "nothing to rehydrate" without an error toast.
#[tauri::command]
async fn osl_load_channel_history(
    app: tauri::AppHandle,
    channel_id: String,
    limit: Option<u32>,
) -> Result<Vec<StoredMessageDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_load_channel_history(state.inner(), channel_id, limit)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Layer 10 / Phase 6a IPC entry point: re-persist a stored
/// message under a fresh plaintext after the user edited it
/// through Discord's edit flow. Boot.js calls this from the
/// PATCH-response load listener; the *original plaintext* the
/// user typed is the second arg (we don't need to round-trip
/// through decrypt — the typed bytes are authoritative).
#[tauri::command]
async fn osl_persist_edit(
    app: tauri::AppHandle,
    discord_message_id: String,
    new_plaintext: String,
    channel_id: Option<String>,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_persist_edit(state.inner(), discord_message_id, new_plaintext, channel_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Probe-2 fix: persist a freshly-sent outbound message so it survives
/// session restart. Boot.js calls this immediately after a successful
/// send, before any DOM swap, so the in-memory `selfSentPlaintext`
/// cache and the durable store both hold the same plaintext. See
/// [`ipc::commands::cmd_osl_persist_outbound`].
#[tauri::command]
async fn osl_persist_outbound(
    app: tauri::AppHandle,
    channel_id: String,
    discord_message_id: String,
    plaintext: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_persist_outbound(state.inner(), channel_id, discord_message_id, plaintext)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Beta 1.0: persist a decrypted attachment's bytes to the local
/// sealed store. boot.js calls this after a successful attachment
/// decrypt so re-entry / restart rehydrates the image without a CDN
/// re-fetch + re-decrypt.
#[tauri::command]
async fn osl_attachment_cache_put(
    app: tauri::AppHandle,
    discord_message_id: String,
    random_filename: String,
    mime: String,
    bytes_b64: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_attachment_cache_put(
            state.inner(),
            discord_message_id,
            random_filename,
            mime,
            bytes_b64,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Beta 1.0: fetch a previously-cached decrypted attachment. Returns
/// null when not cached (boot.js then fetches + decrypts from the CDN).
#[tauri::command]
async fn osl_attachment_cache_get(
    app: tauri::AppHandle,
    discord_message_id: String,
    random_filename: String,
) -> Result<Option<ipc::commands::AttachmentCacheDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_attachment_cache_get(
            state.inner(),
            discord_message_id,
            random_filename,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- Phase 7b: wire v=2 + control message Tauri wrappers ----

/// Layer 10 / Phase 7b: encrypt a v=2 content message under a
/// whitelist-resolved recipient list. See
/// [`ipc::commands::cmd_osl_encrypt_message_v2`].
///
/// 9-B1 shape change — returns [`ipc::commands::EncryptOutput`]
/// which carries one or more Mode 0/Mode 1 cover strings plus
/// chunking metadata.
#[tauri::command]
async fn osl_encrypt_message_v2(
    app: tauri::AppHandle,
    plaintext: String,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<ipc::commands::EncryptOutput, String> {
    let app_handle = app.clone();
    let scope_for_unburn = scope_input.clone();
    let scope_for_event = scope_input.clone();
    let result: Result<ipc::commands::EncryptOutput, String> =
        tauri::async_runtime::spawn_blocking(move || {
            let state = app_handle.state::<AppState>();
            let out = cmd_osl_encrypt_message_v2(
                state.inner(),
                plaintext,
                scope_input,
                channel_members,
                self_discord_id,
            )?;
            // 7d-PIVOT-FIX2 Bug F: re-engaging a burned scope via a
            // fresh encrypted send un-burns it.
            let unburned =
                ipc::commands::cmd_osl_unburn_scope_after_encrypt(state.inner(), scope_for_unburn);
            Ok((out, unburned))
        })
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
        .map(|(out, unburned)| {
            if unburned {
                let payload = serde_json::json!({
                    "scope_kind": scope_for_event.kind,
                    "scope_id": scope_for_event.id,
                    "server_id": scope_for_event.server_id,
                    "channel_id": scope_for_event.channel_id,
                });
                if let Err(e) = app.emit("osl:scope_unburned", payload) {
                    tracing::debug!(?e, "OSL: emit scope_unburned event failed");
                }
            }
            out
        });
    result
}

// ---- Phase 8: attachment encrypt + decrypt Tauri wrappers ----

/// Phase 8: seal a user-attached file. Generates a fresh AEAD key,
/// runs the streaming AEAD over `original_bytes`, prepends the
/// decoy PNG, returns the upload-ready bytes (base64) + the
/// random upload filename + the AEAD key (base64) so JS can
/// hand it to the recipients via [`osl_encrypt_attachment_envelope`].
#[tauri::command]
async fn osl_seal_attachment(
    app: tauri::AppHandle,
    original_bytes_b64: String,
    original_filename: String,
) -> Result<ipc::attachment_wire::SealedAttachment, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::attachment_wire::cmd_osl_seal_attachment_b64(
            state.inner(),
            &original_bytes_b64,
            original_filename,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 8: decrypt a CDN-served attachment. JS fetches the
/// Discord-hosted file, base64-encodes it, and calls this with the
/// AEAD key recovered from the message's attachment envelope.
#[tauri::command]
async fn osl_open_attachment(
    app: tauri::AppHandle,
    att_key_b64: String,
    file_bytes_b64: String,
) -> Result<ipc::attachment_wire::OpenedAttachment, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::attachment_wire::cmd_osl_open_attachment_b64(
            state.inner(),
            att_key_b64,
            &file_bytes_b64,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 8: build the v=2 cover string that carries an attachment
/// envelope (per-attachment AEAD key + filenames + MIME) to every
/// whitelisted recipient in the scope. Returned string is what
/// boot.js sets as the message-text body when an encrypted
/// attachment is being sent.
/// Phase 8d: one-shot seal that embeds the cover envelope inside
/// the file so the Discord message text can stay empty. Replaces the
/// 8c flow of (osl_seal_attachment + osl_encrypt_attachment_envelope
/// + osl_encrypt_message_v2) for the attachment send path.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn osl_seal_attachment_with_cover_v2(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    original_bytes_b64: String,
    original_filename: String,
    random_filename: String,
) -> Result<ipc::commands::SealedAttachmentV2, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_seal_attachment_with_cover_v2(
            state.inner(),
            scope_input,
            channel_members,
            self_discord_id,
            original_bytes_b64,
            original_filename,
            random_filename,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 8d: full open including embedded-cover decrypt. Backwards-
/// compatible with V1 files via the optional legacy_att_key_b64.
#[tauri::command]
async fn osl_open_attachment_v2(
    app: tauri::AppHandle,
    sender_discord_id: String,
    scope_input: Option<ScopeInput>,
    file_bytes_b64: String,
    legacy_att_key_b64: Option<String>,
    discord_message_id: Option<String>,
) -> Result<ipc::attachment_wire::OpenedAttachment, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_open_attachment_v2(
            state.inner(),
            sender_discord_id,
            scope_input,
            file_bytes_b64,
            legacy_att_key_b64,
            discord_message_id,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_encrypt_attachment_envelope(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    attachments: Vec<ipc::commands::AttachmentEnvelopeInput>,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_encrypt_attachment_envelope(
            state.inner(),
            scope_input,
            channel_members,
            self_discord_id,
            attachments,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 8e: V3 seal — MP4 wrapper around the same envelope V2 used.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn osl_seal_attachment_with_cover_v3(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    original_bytes_b64: String,
    original_filename: String,
    random_filename: String,
) -> Result<ipc::commands::SealedAttachmentV2, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_seal_attachment_with_cover_v3(
            state.inner(),
            scope_input,
            channel_members,
            self_discord_id,
            original_bytes_b64,
            original_filename,
            random_filename,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 8d-FIX5: fetch an attachment URL's bytes from Rust so the
/// browser's CSP (which blocks cross-subdomain fetch from discord.com
/// to cdn.discordapp.com) doesn't stop the encrypted-attachment
/// scanner. Returns standard base64 to match what the existing JS
/// path produced via `btoa(arrayBuffer)`. URL allowlist enforced
/// here so JS can't direct the Rust client at arbitrary hosts.
#[tauri::command]
async fn osl_fetch_attachment_bytes(url: String) -> Result<String, String> {
    const ALLOWED_PREFIXES: &[&str] = &[
        "https://cdn.discordapp.com/attachments/",
        "https://media.discordapp.net/attachments/",
        "https://discord-attachments-uploads-prd.storage.googleapis.com/",
    ];
    if !ALLOWED_PREFIXES.iter().any(|p| url.starts_with(p)) {
        return Err(format!("URL not in allowlist: {url}"));
    }
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("reqwest client build: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {status} for {url}"));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("body read failed: {e}"))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Layer 10 / Phase 7b: build the wire-format burn marker for a
/// scope. Caller (boot.js) ships the wire string through
/// Discord's API; the same scope's local state is then mutated
/// via `osl_apply_burn` to wipe wrapped_keys.
#[tauri::command]
async fn osl_send_burn_marker(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_send_burn_marker(state.inner(), scope_input, channel_members, self_discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// 9-C1: osl_send_whitelist_invitation / osl_send_whitelist_response
// removed alongside the invitation handshake.

/// Layer 10 / Phase 7b: apply a local burn for `scope`. Wipes
/// `wrapped_key` on matching rows in messages.sqlite.
#[tauri::command]
async fn osl_apply_burn(app: tauri::AppHandle, scope_input: ScopeInput) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_apply_burn(state.inner(), scope_input)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// 9-C1: osl_accept_invitation / osl_decline_invitation removed
// alongside the invitation handshake.

/// Layer 10 / Phase 7b: un-whitelist a peer in a scope. Returns
/// the wire-format burn marker to ship through Discord's API
/// so the peer wipes its decrypt capability.
#[tauri::command]
async fn osl_unwhitelist_scope(
    app: tauri::AppHandle,
    peer_discord_id: String,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
    revoke_broadened: bool,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_unwhitelist_scope(
            state.inner(),
            peer_discord_id,
            scope_input,
            channel_members,
            self_discord_id,
            revoke_broadened,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Layer 10 / Phase 7b: set a whitelist for a peer + scope.
/// 9-C1: per-peer whitelist set. Permissive decrypt means there's
/// no wire invitation to ship — the peer's recv path will simply
/// decrypt our messages once it has the keys. (Whitelist repair:
/// the dead `from_discord_id` 9-C1 handshake leftover was dropped
/// from the signature end-to-end.)
#[tauri::command]
async fn osl_set_whitelist(
    app: tauri::AppHandle,
    peer_discord_id: String,
    scope_input: ScopeInput,
    broadened: bool,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_set_whitelist(state.inner(), peer_discord_id, scope_input, broadened)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Whitelist repair (Bug C): settings-side LOCAL-ONLY unwhitelist.
/// Same local state mutation as `osl_unwhitelist_scope` (shared
/// `local_unwhitelist_apply` helper — no drift) but emits NO burn
/// wire. The Whitelist Manager has no channel-roster/self-id
/// context to address a burn marker to; operator-accepted
/// "removed locally" semantics (the removed peer can still decrypt
/// ciphertext sent before removal).
#[tauri::command]
async fn osl_local_unwhitelist_scope(
    app: tauri::AppHandle,
    peer_discord_id: String,
    scope_input: ScopeInput,
    revoke_broadened: bool,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_local_unwhitelist_scope(
            state.inner(),
            peer_discord_id,
            scope_input,
            revoke_broadened,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- Phase 7c: UI-supporting Tauri wrappers ----

/// Phase 7c: read `{ encrypt_toggle, has_whitelist }` for a
/// scope. Boot.js calls this on channel-switch to drive the
/// header lock icon's state.
#[tauri::command]
async fn osl_get_scope_encryption_state(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
) -> Result<ScopeEncryptionState, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_get_scope_encryption_state(state.inner(), scope_input)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C1: tri-state whitelist summary for the channel header icon.
/// boot.js feeds in the live channel roster (via the gateway tap
/// in Stage 3); we return whether all/some/none/unknown of the
/// non-self members are whitelisted.
#[tauri::command]
async fn osl_get_scope_whitelist_summary(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<ScopeWhitelistSummary, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_get_scope_whitelist_summary(
            state.inner(),
            scope_input,
            channel_members,
            self_discord_id,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C1 Stage 3: bulk-promote N channel members to whitelisted in
/// a single scope. Boot.js calls this from the tri-state header
/// icon's "encrypt with everyone" flow. Returns the count of peers
/// whose state was actually mutated.
#[tauri::command]
async fn osl_bulk_set_whitelist(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    member_dids: Vec<String>,
) -> Result<usize, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_bulk_set_whitelist(state.inner(), scope_input, member_dids)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C1 Stage 3: bulk-demote N channel members from a scope.
/// Used by the tri-state icon's "stop encrypting with everyone"
/// flow. Returns the count of peers actually mutated.
#[tauri::command]
async fn osl_bulk_unwhitelist_scope(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    member_dids: Vec<String>,
) -> Result<usize, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_bulk_unwhitelist_scope(state.inner(), scope_input, member_dids)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 7c: flip `encrypt_toggle` for a scope. Returns the new
/// value. Errors with `"encrypt_toggle_refused_no_whitelist"`
/// when the user tries to enable encryption in a scope with no
/// whitelisted recipients.
#[tauri::command]
async fn osl_toggle_scope_encryption(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
) -> Result<bool, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_toggle_scope_encryption(state.inner(), scope_input)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 7d-PIVOT: explicit set (not toggle) of a scope's
/// encrypt state. Composer-bar toggle UI calls this with the
/// desired state on click. Emits an `osl:scope_encrypt_changed`
/// event so the settings window's Whitelist Manager can re-render
/// in real time.
#[tauri::command]
async fn osl_set_scope_encrypt(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    enabled: bool,
) -> Result<bool, String> {
    let app_handle = app.clone();
    let scope_for_event = scope_input.clone();
    let result: Result<bool, String> = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_set_scope_encrypt(state.inner(), scope_input, enabled)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?;
    if let Ok(new_state) = result {
        let payload = serde_json::json!({
            "scope_kind": scope_for_event.kind,
            "scope_id": scope_for_event.id,
            "server_id": scope_for_event.server_id,
            "channel_id": scope_for_event.channel_id,
            "enabled": new_state,
        });
        if let Err(e) = app.emit("osl:scope_encrypt_changed", payload) {
            tracing::debug!(?e, "OSL: emit scope_encrypt_changed event failed");
        }
    }
    result
}

// 9-C1: osl_list_pending_invitations removed alongside the
// invitation handshake.

/// Phase 7c bug-fix #1: surface the loaded identity's `user_id`
/// (== local Discord ID) so boot.js can stamp it onto send/burn
/// invocations without trying to walk the React tree for it.
#[tauri::command]
async fn osl_get_self_user_id(app: tauri::AppHandle) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_get_self_user_id(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 7d-FIX3b: persist a Discord snowflake on the loaded
/// identity and repair the peer_map self-entry to match. Called
/// from boot.js after the React-fiber walk resolves the local
/// user's snowflake on first launch.
#[tauri::command]
async fn osl_register_self_snowflake(
    app: tauri::AppHandle,
    snowflake: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    let result: Result<(), String> = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_register_self_snowflake(state.inner(), snowflake)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?;
    if result.is_ok() {
        if let Err(e) = app.emit("osl:self_registered", ()) {
            tracing::debug!(?e, "OSL: emit self_registered event failed");
        }
    }
    result
}

/// Phase 7d-A: settings-menu Identity page payload.
#[tauri::command]
async fn osl_get_identity_info(app: tauri::AppHandle) -> Result<IdentityInfoDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_get_identity_info(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// F2.1: validate a user-entered license key against the keyserver.
/// F2.2 added the sealed cache write on a successful round-trip.
/// Invoked from the Settings Account page (F2.3); also from F2.4's
/// scheduled re-validate.
#[tauri::command]
async fn osl_validate_license(
    app: tauri::AppHandle,
    license_key: String,
) -> Result<keystore::LicenseValidateResponse, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_validate_license(state.inner(), license_key)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// F2.2: read the cached license classification. Exposed to BOTH
/// the settings window (Account page) and the main webview (F3
/// will read this from boot.js for ad gating).
#[tauri::command]
async fn osl_get_license_state(app: tauri::AppHandle) -> Result<keystore::LicenseStateDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_get_license_state(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// F2.2: idempotently delete the cached license. Settings-only.
#[tauri::command]
async fn osl_clear_license(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_clear_license(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// F3.1: read the in-memory tier-gate snapshot. Both windows
/// (main + settings) consume this — main for the encrypt gate +
/// nag toast; settings for the Account-page countdown next to
/// the license state.
#[tauri::command]
async fn osl_get_tier_gate_status(app: tauri::AppHandle) -> Result<TierGateStatusDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_get_tier_gate_status(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// F3.6 pivot: `osl_record_ad_unlock` (F3.1) is deleted alongside
// the ad-unlock + launch-window model. There's no deep-link
// redemption flow under the new "free text / paid attachments"
// tier model — paid users acquire entitlement through Stripe /
// crypto checkout, not through an in-app ad watch.

/// Phase 7d-A: settings-menu Whitelist Manager data source.
#[tauri::command]
async fn osl_list_all_whitelists(app: tauri::AppHandle) -> Result<Vec<WhitelistRowDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_list_all_whitelists(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ===== Phase 7d-B1: main-password gate commands. =====
// argon2id verification runs on a blocking thread; recovery-phrase
// generation pulls from the OS RNG. All eight wrappers follow the
// same spawn_blocking shape.

#[tauri::command]
async fn osl_password_status() -> Result<PasswordStatusDto, String> {
    tauri::async_runtime::spawn_blocking(cmd_osl_password_status)
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_set_main_password(password: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_set_main_password(password))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_change_main_password(current: String, new: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_change_main_password(current, new))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_remove_main_password(current: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_remove_main_password(current))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_view_recovery_phrase(current: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_view_recovery_phrase(current))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_verify_main_password(password: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_verify_main_password(password))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_verify_recovery_phrase(
    app: tauri::AppHandle,
    phrase: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_verify_recovery_phrase(state.inner(), phrase)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_set_main_password_after_recovery(
    app: tauri::AppHandle,
    new_password: String,
    token: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_set_main_password_after_recovery(state.inner(), new_password, token)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_lockout_status() -> Result<LockoutStatusDto, String> {
    tauri::async_runtime::spawn_blocking(cmd_osl_lockout_status)
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

// ===== Phase 7d-B2: stealth password commands. =====

#[tauri::command]
async fn osl_set_stealth_password(current_main: String, new_stealth: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        cmd_osl_set_stealth_password(current_main, new_stealth)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_remove_stealth_password(current_main: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_remove_stealth_password(current_main))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_stealth_password_status() -> Result<PasswordStatusDto, String> {
    tauri::async_runtime::spawn_blocking(cmd_osl_stealth_password_status)
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

// ===== Phase 7d-B3: burn password commands. =====

#[tauri::command]
async fn osl_set_burn_password(current_main: String, new_burn: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_set_burn_password(current_main, new_burn))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_remove_burn_password(current_main: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cmd_osl_remove_burn_password(current_main))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_burn_password_status() -> Result<PasswordStatusDto, String> {
    tauri::async_runtime::spawn_blocking(cmd_osl_burn_password_status)
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

// ===== Phase 7d-B2/B3: gate-side single verify + engage commands. =====

#[tauri::command]
async fn osl_verify_gate_password(
    app: tauri::AppHandle,
    password: String,
) -> Result<GateVerifyDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_verify_gate_password(state.inner(), password)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_stealth_mode_engage(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_stealth_mode_engage(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_burn_engage(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_burn_engage(state.inner())?;
        // Phase 4b: write the decommission sentinel AFTER the wipe so
        // it lands in the (possibly recreated) config dir and is the
        // last file standing. boot.js reads this via
        // `osl_is_decommissioned` and exits before any injection;
        // localStorage caches the result so subsequent boots short-
        // circuit synchronously. The only way to bring OSL back is to
        // manually delete this file (or reinstall).
        if let Err(e) = write_decommissioned_flag() {
            tracing::warn!(error = %e, "OSL: failed to write decommissioned.flag");
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ===== Phase 6.4: control-message inbox (OOB delivery) =====

/// Phase 6.4: hand-off a control wire (SKDM, burn marker,
/// SKDM_REQUEST, recovery SKDM) to the keyserver inbox for a single
/// recipient instead of posting it as a Discord-channel cover.
///
/// `wire_string` is the full `DPC0::<base64>` wire produced by the
/// v=3/v=4 send paths in Rust. boot.js receives that string from
/// the encrypt response and calls this command per recipient.
#[tauri::command]
async fn osl_control_inbox_post(
    app: tauri::AppHandle,
    recipient_id: String,
    scope_input: ScopeInput,
    wire_string: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_control_inbox_post(
            state.inner(),
            recipient_id,
            scope_input,
            wire_string,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 6.4: drain the local user's keyserver inbox.
///
/// Fetches all pending control items, dispatches each through the
/// v=2/v=3/v=4 decrypt pipeline (which applies SKDM/burn/etc), and
/// deletes successfully-applied rows. Returns the count of items
/// applied so boot.js can log throughput. boot.js calls this every
/// 10s while a Discord client window is alive.
#[tauri::command]
async fn osl_control_inbox_drain(app: tauri::AppHandle) -> Result<u32, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let config_dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_control_inbox_drain(state.inner(), config_dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 4b: persistent sentinel that disables every OSL UI/runtime
/// injection. Written by `osl_burn_engage` (account burn), read by
/// boot.js at IIFE-top. Living in the OSL config dir means a full
/// reinstall (which the user does to bring OSL back) removes it
/// alongside everything else.
fn decommissioned_flag_path() -> Result<std::path::PathBuf, String> {
    let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
    Ok(dir.join("decommissioned.flag"))
}

fn write_decommissioned_flag() -> Result<(), String> {
    let path = decommissioned_flag_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("OSL: mkdir: {e}"))?;
    }
    std::fs::write(&path, b"1").map_err(|e| format!("OSL: write {}: {e}", path.display()))
}

fn read_decommissioned_flag() -> bool {
    match decommissioned_flag_path() {
        Ok(path) => path.exists(),
        Err(_) => false,
    }
}

/// Phase 4b: boot.js's backup check. The synchronous localStorage
/// check at IIFE-top catches the common case (immediately after burn,
/// localStorage was set before nav). This IPC catches the edge case
/// where localStorage was cleared independently — boot.js re-sets
/// localStorage and reloads.
#[tauri::command]
async fn osl_is_decommissioned(app: tauri::AppHandle) -> Result<bool, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(read_decommissioned_flag)
        .await
        .map_err(|e| format!("OSL: join error: {e}"))
}

// ===== Phase 7d-FIX1: scope-burn data destruction + burned-scope ledger. =====

#[tauri::command]
async fn osl_burn_scope_data(
    app: tauri::AppHandle,
    scope_kind: String,
    scope_id: String,
    server_id: Option<String>,
) -> Result<BurnScopeDataDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_burn_scope_data(state.inner(), scope_kind, scope_id, server_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_mark_scope_burned(
    app: tauri::AppHandle,
    scope_kind: String,
    scope_id: String,
    server_id: Option<String>,
    channel_id: Option<String>,
    burned_message_ids: Option<Vec<String>>,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_mark_scope_burned(
            state.inner(),
            scope_kind,
            scope_id,
            server_id,
            channel_id,
            burned_message_ids.unwrap_or_default(),
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_unburn_scope(
    app: tauri::AppHandle,
    scope_kind: String,
    scope_id: String,
) -> Result<bool, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_unburn_scope(state.inner(), scope_kind, scope_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_list_burned_scopes(app: tauri::AppHandle) -> Result<Vec<BurnedScopeDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_list_burned_scopes(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 9-A3: boot.js pushes the gateway-derived membership of a
/// channel here. Updates the in-memory `channel_members` cache that
/// the v=5 sender-keys dispatcher consults to detect membership
/// changes and trigger rotation.
#[tauri::command]
async fn osl_membership_update(
    app: tauri::AppHandle,
    channel_id: String,
    member_ids: Vec<String>,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_membership_update(state.inner(), channel_id, member_ids)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_membership_get(
    app: tauri::AppHandle,
    channel_id: String,
) -> Result<Vec<String>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_membership_get(state.inner(), channel_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// W2: durable scope-membership accrual fed by the gateway taps.
#[tauri::command]
async fn osl_note_scope_membership(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    member_ids: Vec<String>,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_note_scope_membership(state.inner(), scope_input, member_ids)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// W2: server-header + per-channel whitelist/encrypt flags for UI.
#[tauri::command]
async fn osl_get_server_whitelist_state(
    app: tauri::AppHandle,
    server_id: String,
    channel_scope_input: Option<ScopeInput>,
) -> Result<ipc::commands::ServerWhitelistStateDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_get_server_whitelist_state(
            state.inner(),
            server_id,
            channel_scope_input,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// W2: the server-header whitelist button (whole-server, OSL members).
#[tauri::command]
async fn osl_set_server_header_whitelist(
    app: tauri::AppHandle,
    server_id: String,
    on: bool,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_set_server_header_whitelist(state.inner(), server_id, on)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// W2: the per-channel sidebar whitelist button.
#[tauri::command]
async fn osl_set_channel_whitelist(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    on: bool,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_set_channel_whitelist(state.inner(), scope_input, on)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- Phase 9-C2: friend list + guild list + bulk-DM whitelist ----

/// 9-C2: boot.js gateway tap pushes the user's friend-id list here
/// on every Discord READY. Consumed by the settings-window Bulk
/// Whitelist modal's "Friend list" action.
#[tauri::command]
async fn osl_set_friend_ids(app: tauri::AppHandle, ids: Vec<String>) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_set_friend_ids(state.inner(), ids)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C2: settings-window read of the friend-id snapshot. Empty
/// vec if the user hasn't connected to Discord yet (gateway READY
/// hasn't fired).
#[tauri::command]
async fn osl_get_friend_ids(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_get_friend_ids(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C2: boot.js gateway tap pushes the user's guild list here on
/// each GUILD_CREATE. The member_ids list may be partial on large
/// guilds (Discord only ships the ~100 online members at that
/// time).
#[tauri::command]
async fn osl_set_guild_list(
    app: tauri::AppHandle,
    guilds: Vec<ipc::commands::GuildDto>,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_set_guild_list(state.inner(), guilds)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C2: settings-window read of the guild-list snapshot. Empty
/// vec if no GUILD_CREATE has fired yet.
#[tauri::command]
async fn osl_get_guild_list(app: tauri::AppHandle) -> Result<Vec<ipc::commands::GuildDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_get_guild_list(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C2: bulk-whitelist N peers under DM scope (one DM scope per
/// peer). Used by the Bulk Whitelist modal's "Friend list" and
/// "Paste IDs" actions. Returns the count actually mutated.
#[tauri::command]
async fn osl_bulk_set_dm_whitelist(
    app: tauri::AppHandle,
    member_dids: Vec<String>,
) -> Result<usize, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_bulk_set_dm_whitelist(state.inner(), member_dids)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- Phase 9-C3: per-server "encrypt new channels by default" ----

/// 9-C3: set or clear the per-server "encrypt new channels by default"
/// flag. Persists to disk via the whitelist_state.json envelope.
#[tauri::command]
async fn osl_set_server_default(
    app: tauri::AppHandle,
    server_id: String,
    encrypt_by_default: bool,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_set_server_default(state.inner(), server_id, encrypt_by_default)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C3: read all server-default entries (sorted by server_id).
/// Used by the settings modal + the sidebar overlay's paint pass.
#[tauri::command]
async fn osl_get_server_defaults(
    app: tauri::AppHandle,
) -> Result<Vec<ipc::commands::ServerDefaultDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_get_server_defaults(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// 9-C3: retroactively flip `encrypt_toggle = true` on every
/// already-known channel in `server_id`. Returns the count mutated.
/// Settings-window only (the sidebar overlay's primary action is
/// just to flip the default; retro-apply is opt-in).
#[tauri::command]
async fn osl_apply_server_default_to_existing_channels(
    app: tauri::AppHandle,
    server_id: String,
) -> Result<usize, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_apply_server_default_to_existing_channels(state.inner(), server_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 9-B1: read user-tunable preferences (stego mode, Mode 1
/// preview flags). Returns the in-memory snapshot loaded at boot.
#[tauri::command]
async fn osl_get_app_preferences(
    app: tauri::AppHandle,
) -> Result<ipc::commands::AppPreferencesDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_get_app_preferences(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_set_app_preferences(
    app: tauri::AppHandle,
    prefs: ipc::commands::AppPreferencesDto,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_set_app_preferences(state.inner(), prefs, dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- Phase 9-D: onboarding tour + VPN warning ----

/// 9-TD1.4: read + clear the most recent disk-persist failure
/// message. Boot.js calls this after mutation invokes to surface
/// "couldn't save change to disk" as a toast.
#[tauri::command]
async fn osl_take_last_persist_error(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        Ok::<_, String>(ipc::commands::cmd_osl_take_last_persist_error(
            state.inner(),
        ))
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- REGISTER-FIX: TOFU + registration-conflict surface ----

/// Read + clear the one-shot registration-conflict alert (set when
/// open `/v1/register` returned 403 — our user_id is held by a
/// different key). Boot.js polls this and shows a BLOCKING warning;
/// it is deliberately NOT warn-swallowed.
#[tauri::command]
async fn osl_take_registration_alert(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_take_registration_alert(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// List pending peer key-change (TOFU) alerts. Boot.js polls to show
/// the blocking "peer's security key changed" banner; the settings
/// window lists them for accept/decline.
#[tauri::command]
async fn osl_list_key_change_alerts(
    app: tauri::AppHandle,
) -> Result<Vec<ipc::state::KeyChangeAlert>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_list_key_change_alerts(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// User accepted a peer's new identity key → adopt as new baseline.
#[tauri::command]
async fn osl_accept_key_change(
    app: tauri::AppHandle,
    discord_id: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_accept_key_change(state.inner(), discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// User declined a peer's new identity key → keep old baseline.
#[tauri::command]
async fn osl_decline_key_change(
    app: tauri::AppHandle,
    discord_id: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_decline_key_change(state.inner(), discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// A(a): operator-driven single-peer v=4 session reset. Drops the
/// peer's `ratchet_state` so the next v=4 message re-handshakes.
/// Run on BOTH ends to recover a desynced ratchet.
#[tauri::command]
async fn osl_reset_v4_session(
    app: tauri::AppHandle,
    discord_id: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_reset_v4_session(state.inner(), discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// SKDM-fix (3/3): operator-driven v=5 sender-key reset for a scope.
/// Drops the scope's sender-key chain AND the paired v=4 ratchet for
/// its non-self peers so the next v=5 send re-emits a fresh SKDM.
/// Remedy for a scope poisoned by the pre-fix discarded-SKDM bug.
#[tauri::command]
async fn osl_reset_v5_sender_key(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
) -> Result<Vec<ipc::commands::SessionResetNotice>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_reset_v5_sender_key(state.inner(), scope_input)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Auto-recovery: build a v=2-wrapped SKDM_REQUEST for `peer_discord_id`
/// in `scope_input`. Called by boot.js when a v=5 message stays
/// "awaiting SKDM" past its retry budget. Returns the DPC0:: wire for
/// boot.js to POST; a stable Err on outbound throttle (boot.js skips).
#[tauri::command]
async fn osl_build_skdm_request(
    app: tauri::AppHandle,
    scope_input: ScopeInput,
    peer_discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_build_skdm_request(state.inner(), scope_input, peer_discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Auto-recovery: drop our v=4 ratchet for `peer_discord_id` and build
/// a v=2-wrapped SESSION_RESET telling that peer to do the same.
/// Called by boot.js when a v=4 message from the peer keeps failing
/// (ratchet desync). Returns the DPC0:: wire to POST; Err on throttle.
#[tauri::command]
async fn osl_build_session_reset(
    app: tauri::AppHandle,
    peer_discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_build_session_reset(state.inner(), peer_discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Auto-recovery (stale-identity): re-fetch a peer's keyserver bundle
/// so a changed identity surfaces the existing loud TOFU alert instead
/// of stranding a passive receiver on "not a recipient". Never
/// auto-accepts a changed key.
#[tauri::command]
async fn osl_recover_peer_identity(
    app: tauri::AppHandle,
    discord_id: String,
) -> Result<bool, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_recover_peer_identity(state.inner(), discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Safety number for a peer's current trusted Ed25519 baseline.
#[tauri::command]
async fn osl_peer_safety_number(
    app: tauri::AppHandle,
    discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_peer_safety_number(state.inner(), discord_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Safety number for our own Ed25519 identity pub (read out OOB).
#[tauri::command]
async fn osl_self_safety_number(app: tauri::AppHandle) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_self_safety_number(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_tour_get_state(app: tauri::AppHandle) -> Result<ipc::commands::TourStateDto, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_tour_get_state(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_tour_advance(app: tauri::AppHandle, slide: u8) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_tour_advance(state.inner(), slide, dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_tour_complete(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_tour_complete(state.inner(), dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_tour_skip(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_tour_skip(state.inner(), dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

#[tauri::command]
async fn osl_tour_reset(app: tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_tour_reset(state.inner(), dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// W4: osl_vpn_warning_dismiss_forever / osl_vpn_warning_reset /
// osl_check_vpn and the extract_country_from_locale helper were
// removed with the VPN feature. The check leaked the user IP to
// ipapi.co every launch and the locale-vs-geo heuristic never
// warned correctly; see project memory for the decision.

/// Phase 7d-C: open the trusted local settings window.
///
/// Idempotent — if a window labelled `settings` already exists,
/// brings it to front (un-minimizes if needed, sets focus, raises
/// always-on-top) and returns Ok. Otherwise, constructs a fresh
/// `WebviewWindow` at `osl-gate://localhost/settings`. The
/// `osl-gate` URI scheme handler (above) inspects the URL path
/// and serves `settings_window.html` for `/settings`.
///
/// Modal-style behavior (Task 8d): Tauri 2 supports parent/owner
/// windows on Windows via `WebviewWindowBuilder::parent`, but
/// `is_modal` is not exposed in the stable API. We approximate
/// modality by setting `always_on_top(true)` on the settings
/// window so Discord can't visually cover it, and emit a
/// `settings_window_closed` event on close so the Discord-origin
/// boot.js can re-sync state.
#[tauri::command]
async fn osl_open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    // If the window already exists, focus + raise + bail.
    if let Some(existing) = app.get_webview_window("settings") {
        if let Err(e) = existing.unminimize() {
            tracing::debug!(?e, "OSL settings: unminimize on existing window");
        }
        if let Err(e) = existing.show() {
            tracing::debug!(?e, "OSL settings: show on existing window");
        }
        if let Err(e) = existing.set_focus() {
            tracing::debug!(?e, "OSL settings: set_focus on existing window");
        }
        return Ok(());
    }
    let url: tauri::Url = "osl-gate://localhost/settings"
        .parse()
        .map_err(|e| format!("OSL settings: URL parse: {e}"))?;
    let mut builder = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::External(url))
        .title("OSL Settings")
        .inner_size(900.0, 650.0)
        .min_inner_size(700.0, 500.0)
        .resizable(true)
        .decorations(true)
        .center()
        .always_on_top(true);
    // Best-effort: declare the Discord window as the parent so
    // Windows treats settings as an owned-secondary. If lookup
    // fails (transient race during app startup), proceed without
    // — the always_on_top fallback still gives modal-ish UX.
    if let Some(main) = app.get_webview_window("main") {
        builder = builder.parent(&main).map_err(|e| {
            tracing::debug!(?e, "OSL settings: parent(main) failed; continuing without");
            format!("OSL settings: parent: {e}")
        })?;
    }
    let window = builder
        .build()
        .map_err(|e| format!("OSL settings: build: {e}"))?;
    // Apply the same screenshot protection as the main window so
    // recovery phrases / passwords aren't capturable.
    if let Err(e) = screenshot::apply_to_window(&window, ScreenshotProtection::On) {
        tracing::debug!(
            ?e,
            "OSL settings: screenshot protection unavailable on settings window"
        );
    }
    // 7d-D Task 1: disable the Discord main window while settings
    // is open so clicks/keys don't reach Discord behind the
    // settings overlay. Backed by Win32 `EnableWindow(hwnd, FALSE)`
    // on Windows via tauri-runtime-wry. Pair with the Destroyed
    // handler below which re-enables the window.
    if let Some(main) = app.get_webview_window("main") {
        if let Err(e) = main.set_enabled(false) {
            tracing::warn!(?e, "OSL settings: set_enabled(false) on main window failed");
        }
    }
    // Emit settings_window_closed when the window is destroyed,
    // and re-enable the Discord main window.
    let app_for_close = app.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Destroyed = event {
            if let Some(main) = app_for_close.get_webview_window("main") {
                if let Err(e) = main.set_enabled(true) {
                    tracing::warn!(
                        ?e,
                        "OSL settings: set_enabled(true) on main window failed; \
                         Discord may remain non-interactive — relaunch the app"
                    );
                }
                // W6: closing an owned + always-on-top child while the
                // owner was EnableWindow(FALSE)'d makes Windows drop
                // the owner (it minimizes / falls behind) unless focus
                // is explicitly restored. Re-raise the Discord window
                // so closing settings just closes settings.
                if let Err(e) = main.unminimize() {
                    tracing::debug!(?e, "OSL settings: unminimize(main) on close");
                }
                if let Err(e) = main.set_focus() {
                    tracing::debug!(?e, "OSL settings: set_focus(main) on close");
                }
            }
            if let Err(e) = app_for_close.emit("osl:settings_window_closed", ()) {
                tracing::debug!(?e, "OSL settings: emit close event failed");
            }
        }
    });
    Ok(())
}

/// Phase 7d-D: close the settings window if it's open. Called
/// from the Discord-origin account-burn flow so the settings
/// window doesn't end up with stale state pointing at files
/// that `osl_burn_engage` is about to wipe. Idempotent — Ok if
/// the settings window doesn't exist. Note that closing the
/// window triggers `WindowEvent::Destroyed` which re-enables
/// the Discord main window via the handler above, so the burn
/// flow doesn't need to do that step itself.
#[tauri::command]
async fn osl_close_settings_window_if_open(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(settings) = app.get_webview_window("settings") {
        if let Err(e) = settings.close() {
            tracing::warn!(?e, "OSL settings: close request failed");
            return Err(format!("OSL settings: close: {e}"));
        }
    }
    Ok(())
}

/// Layer 10 / Phase 5b2 IPC entry point: mark a message burned
/// in the at-rest store. Subsequent
/// `osl_load_channel_history` calls will not return it. Burns
/// against unknown ids return `Ok(())` (idempotent).
#[tauri::command]
async fn osl_burn_message(app: tauri::AppHandle, discord_message_id: String) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_burn_message(state.inner(), discord_message_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 7d-B1: bundled boot-gate HTML. Served via the
/// `osl-gate://` custom URI scheme when `password_marker.json`
/// exists in the OSL config dir. The page handles password +
/// recovery-phrase entry, then `window.location.href`-navigates
/// to discord.com when the user authenticates successfully.
const PASSWORD_GATE_HTML: &str = include_str!("../assets/password_gate.html");

/// Phase 7d-C: bundled trusted-local Settings page. Served at
/// `osl-gate://localhost/settings`. Hosts Identity / Whitelist
/// Manager / Passwords / About in a separate Tauri window so
/// sensitive operations (password mgmt, recovery phrase view,
/// identity display) do not run on the Discord remote origin.
const SETTINGS_WINDOW_HTML: &str = include_str!("../assets/settings_window.html");

/// Phase F0: deep-link smoke-test wrapper. Round-trips an `osl://...`
/// URL string through the pure parser in `ipc::commands` and
/// returns the structured response to JS.
///
/// The actual deep-link arrival is handled by the `on_open_url`
/// callback registered in the Builder's `setup` block — that's
/// what fires when Windows activates `osl://...`. This command is
/// the JS → Rust direction: boot.js receives the
/// `osl:deep-link-received` event and invokes us to do a Rust-side
/// parse + log + return-to-JS, which proves the full
/// Rust → JS → Rust round-trip path.
///
/// Replaced in F2 by `cmd_osl_redeem_unlock`, which validates the
/// token against the keyserver and resets the foreground-time
/// ad timer.
#[tauri::command]
async fn osl_test_deep_link(url: String) -> Result<ipc::commands::OslTestDeepLinkResponse, String> {
    tauri::async_runtime::spawn_blocking(move || ipc::commands::cmd_osl_test_deep_link(url))
        .await
        .map_err(|e| format!("OSL: join error: {e}"))?
}

/// G3.3: build a `tauri-plugin-updater` `Updater` whose manifest
/// endpoint carries the user's selected channel as `?channel=`.
///
/// v2.9.0 has no built-in "channel" concept; the clean supported
/// mechanism is a runtime endpoint override via
/// `updater_builder().endpoints(..)`. The `{{target}}/{{arch}}/
/// {{current_version}}` placeholders are still interpolated by the
/// plugin at fetch time (it `.replace()`s them on whatever endpoints
/// are set), and the query param survives that substitution.
///
/// The channel is read from the SAME persisted app_preferences as
/// every other client setting (no new persistence layer).
fn osl_build_channel_updater(
    app: &tauri::AppHandle,
) -> Result<tauri_plugin_updater::Updater, String> {
    use tauri_plugin_updater::UpdaterExt;

    let channel = {
        let state = app.state::<AppState>();
        ipc::commands::cmd_osl_get_update_channel(state.inner())?
    };

    // Doubled braces escape the format! placeholders so the literal
    // `{{target}}` etc. reach the plugin for its own interpolation.
    let endpoint = format!(
        "https://keyserver.oslprivacy.com/v1/update-manifest/{{{{target}}}}/{{{{arch}}}}/{{{{current_version}}}}?channel={}",
        channel.as_query_value()
    );
    let url = tauri::Url::parse(&endpoint)
        .map_err(|e| format!("OSL: bad updater endpoint: {e}"))?;

    app.updater_builder()
        .endpoints(vec![url])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())
}

/// G3.1/G3.3: ask the keyserver manifest whether a newer build
/// exists on the user's channel. Check-only — no download/install.
/// The Tauri-coupled bits stay here; the pure result classification
/// lives in `ipc::commands::cmd_osl_check_for_updates` so it
/// unit-tests without a webview runtime.
#[tauri::command]
async fn osl_check_for_updates(
    app: tauri::AppHandle,
) -> Result<ipc::commands::UpdateCheckResult, String> {
    let current = app.package_info().version.to_string();

    let outcome: Result<Option<ipc::commands::UpdateInfo>, String> =
        match osl_build_channel_updater(&app) {
            Ok(updater) => match updater.check().await {
                Ok(Some(update)) => Ok(Some(ipc::commands::UpdateInfo {
                    version: update.version.clone(),
                    notes: update.body.clone(),
                    url: update.download_url.to_string(),
                })),
                Ok(None) => Ok(None),
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e),
        };

    Ok(ipc::commands::cmd_osl_check_for_updates(current, outcome))
}

/// G3.3: download + signature-verify + install the available update,
/// then relaunch. The plugin verifies the minisign signature against
/// the configured pubkey inside `download_and_install`; a bad/missing
/// signature returns `Err` and NOTHING is installed (security-
/// critical — we surface an explicit error, never a silent skip).
///
/// User consent for the restart is obtained in the UI *before* this
/// command is invoked (a confirm dialog). v2.9.0's
/// `download_and_install` is atomic (download+verify+install in one
/// call) — there is no clean "verify, pause, then apply" seam — so
/// consent-before-call is the supported shape; we never auto-apply.
///
/// Progress is streamed via the `osl:update-progress` event
/// (`{ downloaded, total }`); `osl:update-finished` fires when the
/// byte stream completes (just before install + relaunch).
#[tauri::command]
async fn osl_install_update(
    app: tauri::AppHandle,
) -> Result<ipc::commands::UpdateInstallResult, String> {
    let updater = match osl_build_channel_updater(&app) {
        Ok(u) => u,
        Err(e) => {
            return Ok(ipc::commands::UpdateInstallResult::Error { message: e })
        }
    };

    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => return Ok(ipc::commands::UpdateInstallResult::NoUpdate),
        Err(e) => {
            return Ok(ipc::commands::UpdateInstallResult::Error {
                message: format!("Couldn't check for the update: {e}"),
            })
        }
    };

    let app_for_progress = app.clone();
    let mut downloaded: u64 = 0;
    let on_chunk = move |chunk_len: usize, content_length: Option<u64>| {
        downloaded += chunk_len as u64;
        let _ = app_for_progress.emit(
            "osl:update-progress",
            serde_json::json!({ "downloaded": downloaded, "total": content_length }),
        );
    };
    let app_for_finish = app.clone();
    let on_finish = move || {
        let _ = app_for_finish.emit("osl:update-finished", ());
    };

    match update.download_and_install(on_chunk, on_finish).await {
        Ok(()) => {
            // Verified + installed. Relaunch into the new version.
            // `restart()` diverges, so JS never gets a return value
            // on success (handled UI-side: the confirm dialog already
            // told the user OSL would restart).
            tracing::info!(
                target: "osl::updater",
                "[OSL updater] update installed; relaunching"
            );
            app.restart();
        }
        Err(e) => {
            // Includes signature-verification failure. Nothing was
            // installed. Surface an explicit, unambiguous error.
            let msg = e.to_string();
            tracing::error!(
                target: "osl::updater",
                error = %msg,
                "[OSL updater] download/verify/install FAILED — not installed"
            );
            Ok(ipc::commands::UpdateInstallResult::Error {
                message: format!(
                    "Update could not be verified and was NOT installed: {msg}"
                ),
            })
        }
    }
}

/// G3.3: read the persisted update channel (stable/beta).
#[tauri::command]
async fn osl_get_update_channel(
    app: tauri::AppHandle,
) -> Result<ipc::app_preferences::UpdateChannel, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        ipc::commands::cmd_osl_get_update_channel(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// G3.3: persist the update channel. Channel eligibility is a UX
/// affordance, not a security boundary (see `UpdateChannel` docs) —
/// this command does not and must not verify the caller is paid.
#[tauri::command]
async fn osl_set_update_channel(
    app: tauri::AppHandle,
    channel: ipc::app_preferences::UpdateChannel,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let dir = keystore::osl_config_dir().ok();
        ipc::commands::cmd_osl_set_update_channel(state.inner(), channel, dir)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 2 prose-token send. Takes a `DPC0::<base64>` wire string
/// produced by the existing encrypt pipeline, uploads the underlying
/// cipher bytes to the cipher-store with the chosen TTL, and encodes
/// the returned blob ID as natural-English prose. The returned
/// `cover_text` is what the client posts to Discord — no DPC0::
/// marker, no high-entropy base64 blob visible on the wire.
#[derive(serde::Serialize)]
struct ProseTokenSendDto {
    cover_text: String,
    blob_id: String,
    expires_at: i64,
}

#[tauri::command]
async fn osl_prose_token_send(
    app: tauri::AppHandle,
    scope_input: ipc::scope::ScopeInput,
    dpc0_wire: String,
    ttl_seconds: u32,
) -> Result<ProseTokenSendDto, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
        let r = ipc::prose_token::prose_token_send(&dir, &scope_input, &dpc0_wire, ttl_seconds)
            .map_err(|e| e.to_string())?;
        // Phase 4: track every blob_id under its scope so scope-burn
        // can DELETE them server-side and instantly revert covers to
        // un-decryptable junk. Best-effort: a persist failure here
        // does NOT fail the send (worst case the blob lingers until
        // its TTL expires naturally).
        if let Ok(scope) = TryInto::<ipc::scope::Scope>::try_into(scope_input) {
            let blobs_path = dir.join("scope_blobs.json");
            let mut blobs = ipc::scope_blobs_file::load(&blobs_path);
            ipc::scope_blobs_file::record_blob(
                &mut blobs,
                scope.storage_key(),
                r.blob_id.clone(),
            );
            if let Err(e) = ipc::scope_blobs_file::write(&blobs_path, &blobs) {
                tracing::warn!(error = %e, "OSL: scope_blobs persist failed (send still succeeded)");
            }
        }
        Ok(ProseTokenSendDto {
            cover_text: r.cover_text,
            blob_id: r.blob_id,
            expires_at: r.expires_at,
        })
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 4: scope-burn cipher delete. Walks every blob_id this
/// client recorded under the scope in `scope_blobs.json` and DELETEs
/// each from the cipher-store, clearing the local list afterwards.
///
/// Boot.js invokes this between the burn-marker send and the local
/// `osl_apply_burn` so an apply-failure still leaves the blobs gone
/// (the worst case there is "user thinks burn succeeded; in fact
/// only the server-side covers were burned, not the local ratchet
/// state" — apply_burn re-runs cleanly because the IPC is
/// idempotent). Best-effort per-blob: a single DELETE failure
/// (already 404, network blip) is logged and the iteration
/// continues; the local list is cleared regardless because
/// retrying on next launch isn't the right UX.
#[derive(serde::Serialize)]
struct ScopeBurnBlobsDto {
    deleted: u32,
    failed: u32,
}

#[tauri::command]
async fn osl_scope_burn_blobs(
    app: tauri::AppHandle,
    scope_input: ipc::scope::ScopeInput,
) -> Result<ScopeBurnBlobsDto, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let scope_for_token = scope_input.clone();
        let scope: ipc::scope::Scope = scope_input
            .try_into()
            .map_err(|e: ipc::scope::ScopeError| format!("OSL: {e}"))?;
        let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
        let path = dir.join("scope_blobs.json");
        let mut file = ipc::scope_blobs_file::load(&path);
        let blob_ids = ipc::scope_blobs_file::take_blobs(&mut file, &scope.storage_key());
        let mut deleted = 0u32;
        let mut failed = 0u32;
        for id in &blob_ids {
            match ipc::prose_token::prose_token_burn_id(&dir, &scope_for_token, id) {
                Ok(()) => deleted += 1,
                Err(e) => {
                    tracing::warn!(blob_id = %id, error = %e, "OSL: scope_burn_blobs delete failed");
                    failed += 1;
                }
            }
        }
        if let Err(e) = ipc::scope_blobs_file::write(&path, &file) {
            tracing::warn!(error = %e, "OSL: scope_blobs persist (post-burn clear) failed");
        }
        tracing::info!(
            scope = %scope.storage_key(),
            deleted,
            failed,
            "OSL: scope_burn_blobs"
        );
        Ok(ScopeBurnBlobsDto { deleted, failed })
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 2 prose-token receive. Attempts to decode a Discord message
/// as an OSL prose-token. Returns `None` if the HMAC doesn't validate
/// (normal chat — caller leaves the message alone). Returns
/// `Some({ wire, blob_id })` on a successful decode + cipher fetch;
/// the caller feeds `wire` (a `DPC0::<base64>` string) into the
/// existing decrypt path to recover plaintext.
#[derive(serde::Serialize)]
struct ProseTokenRecvDto {
    wire: String,
    blob_id: String,
}

#[tauri::command]
async fn osl_prose_token_recv(
    app: tauri::AppHandle,
    scope_input: ipc::scope::ScopeInput,
    msg: String,
) -> Result<Option<ProseTokenRecvDto>, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
        let r = ipc::prose_token::prose_token_recv(&dir, &scope_input, &msg)
            .map_err(|e| e.to_string())?;
        Ok(r.map(|x| ProseTokenRecvDto {
            wire: x.wire,
            blob_id: x.blob_id,
        }))
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 2 prose-token burn (Phase 6 now scope-gated). Deletes a
/// single blob from the cipher-store. Idempotent on the server side.
/// Requires `scope_input` so the client can derive the same
/// capability token the original uploader used — without it the
/// worker 401s the DELETE (defends against blob_id-leak DoS).
#[tauri::command]
async fn osl_prose_token_burn(
    app: tauri::AppHandle,
    scope_input: ipc::scope::ScopeInput,
    blob_id: String,
) -> Result<(), String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
        ipc::prose_token::prose_token_burn_id(&dir, &scope_input, &blob_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 3: read the per-scope cipher-store TTL (seconds). Returns
/// `DEFAULT_TTL_SECONDS` (72h) when no entry exists for the scope,
/// clamped to `[MIN_TTL_SECONDS, MAX_TTL_SECONDS]` (1h..=7d). boot.js
/// calls this immediately before each `osl_prose_token_send` and uses
/// the result as the `ttl_seconds` arg.
#[tauri::command]
async fn osl_get_scope_ttl(
    app: tauri::AppHandle,
    scope_input: ipc::scope::ScopeInput,
) -> Result<u32, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let scope: ipc::scope::Scope = scope_input
            .try_into()
            .map_err(|e: ipc::scope::ScopeError| format!("OSL: {e}"))?;
        let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
        let path = dir.join("scope_ttl.json");
        let file = ipc::scope_ttl_file::load_scope_ttls(&path);
        Ok(ipc::scope_ttl_file::get_scope_ttl(
            &file,
            &scope.storage_key(),
        ))
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Phase 3: set the per-scope cipher-store TTL (seconds). The stored
/// value is clamped to `[MIN_TTL_SECONDS, MAX_TTL_SECONDS]`; the
/// effective (post-clamp) value is returned so the UI can echo it
/// back to the slider without a follow-up read.
#[tauri::command]
async fn osl_set_scope_ttl(
    app: tauri::AppHandle,
    scope_input: ipc::scope::ScopeInput,
    ttl_seconds: u32,
) -> Result<u32, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let scope: ipc::scope::Scope = scope_input
            .try_into()
            .map_err(|e: ipc::scope::ScopeError| format!("OSL: {e}"))?;
        let dir = keystore::osl_config_dir().map_err(|e| format!("OSL: config_dir: {e}"))?;
        let path = dir.join("scope_ttl.json");
        let mut file = ipc::scope_ttl_file::load_scope_ttls(&path);
        let stored =
            ipc::scope_ttl_file::set_scope_ttl(&mut file, scope.storage_key(), ttl_seconds);
        ipc::scope_ttl_file::write_scope_ttls(&path, &file)?;
        Ok(stored)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

fn main() {
    // Without a subscriber, every `tracing::info!/warn!/error!`
    // across the workspace is silently discarded — three diagnosis
    // reports stalled because the actual send-vs-receive failure was
    // invisible. Install one before anything else runs. Verbosity is
    // RUST_LOG-controlled; with RUST_LOG unset EnvFilter defaults to
    // ERROR only, so the v=4 send/recv triage lines REQUIRE
    // `RUST_LOG=osl::v4=info` (or broader, e.g. `RUST_LOG=info`).
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        // Phase F0: single-instance plugin MUST be initialized before
        // tauri-plugin-deep-link. Windows spawns a new process when
        // `osl://...` fires while OSL is already running; without
        // single-instance we'd end up with two OSL windows fighting
        // over the same Discord login. The `deep-link` feature on the
        // single-instance crate forwards the second process's CLI
        // args through the deep-link plugin's event channel, so the
        // existing instance's `on_open_url` callback fires (rather
        // than the spawned duplicate having to handle it).
        //
        // The closure body runs in the *first* instance when the OS
        // hands off a second-instance launch. We just bring the main
        // window to the front; the URL itself flows through the
        // deep-link feature's automatic forwarding.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tracing::info!(
                target: "osl::deep_link",
                ?argv,
                "[OSL deep-link] second instance launched; focusing main window"
            );
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        // G3.1/G3.2: auto-updater. `dialog:false` suppresses Tauri's
        // built-in update dialog (custom UI lands in G3.3 alongside
        // license-state channel selection). No auto download/install
        // is configured, so the plugin only enables
        // `osl_check_for_updates` to query the keyserver manifest.
        //
        // G3.2 — update signing:
        //   * Public key (in `tauri.conf.json` -> plugins.updater.pubkey,
        //     shipped to every client in the manifest):
        //     minisign identifier 44AD89E36BC119F8.
        //   * Private key: NEVER in the repo. The build picks it up
        //     from the operator-only OS env var
        //     TAURI_SIGNING_PRIVATE_KEY_PATH (+ ..._PASSWORD); Tauri
        //     auto-signs the .msi at `cargo tauri build` time when
        //     those are set. No code/config references the path.
        //   * Key rotation: ship an update *signed with the OLD key*
        //     whose `tauri.conf.json` flips `pubkey` to the NEW key.
        //     Clients only trust a new key once they've installed an
        //     update verified by the old one. Do NOT rotate without a
        //     written rollover plan — a botched rotation bricks the
        //     update channel for every existing install.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new())
        // Phase 7d-B1 / 7d-C: serve bundled local HTML pages on the
        // `osl-gate://` custom URI scheme. WebView2 routes requests
        // here synchronously; we inspect the path and return either
        // the boot-gate page or the trusted-local Settings page.
        // IPC works from this origin because Tauri 2 injects
        // __TAURI_INTERNALS__ on every webview load regardless of
        // URL scheme. Capabilities are window-scoped, so the gate
        // window ("main", before navigation) uses
        // `password-gate-capability` while the settings window
        // ("settings", spawned by `osl_open_settings_window`) uses
        // `settings-window-capability`.
        //
        // Path routing:
        //   /settings, /settings/ → settings_window.html
        //   anything else         → password_gate.html
        .register_uri_scheme_protocol("osl-gate", |_app, request| {
            let path = request.uri().path();
            let body: &[u8] = if path == "/settings" || path == "/settings/" {
                SETTINGS_WINDOW_HTML.as_bytes()
            } else {
                PASSWORD_GATE_HTML.as_bytes()
            };
            tauri::http::Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(body.to_vec())
                .unwrap_or_else(|_| {
                    tauri::http::Response::new(b"<h1>OSL gate render failed</h1>".to_vec())
                })
        })
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
            if let Err(e) = screenshot::apply_to_webview(webview, ScreenshotProtection::On) {
                tracing::debug!(?e, "screenshot protection re-apply on page load failed",);
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
            // Phase 7d-B1: route the initial URL based on
            // password-marker presence. Marker exists → load the
            // local boot-gate page first; absent → load
            // discord.com directly (preserves pre-7d-B1 behavior
            // for users who haven't set a password).
            let gate_required = match keystore::osl_config_dir() {
                Ok(dir) => ipc::main_password::marker_exists(&dir),
                Err(e) => {
                    tracing::warn!(
                        ?e,
                        "OSL boot: cannot resolve config dir — \
                         skipping password gate, loading discord.com directly"
                    );
                    false
                }
            };
            let initial_url: tauri::Url = if gate_required {
                tracing::info!("OSL boot: password_marker.json present, loading boot gate");
                "osl-gate://localhost".parse().expect("osl-gate URL parses")
            } else {
                tracing::info!("OSL boot: no password marker, loading discord.com directly");
                "https://discord.com/app"
                    .parse()
                    .expect("hardcoded discord.com URL parses")
            };
            // 7d-FIX3b: optional CSP strip for Discord origins.
            // Tauri 2.11's `on_web_resource_request` closure type:
            //   F: Fn(http::Request<Vec<u8>>,
            //         &mut http::Response<Cow<'static, [u8]>>)
            //      + Send + Sync + 'static
            // We don't touch the body — just remove the CSP header
            // for Discord-origin responses so the WebView treats the
            // page as unrestricted. This is what unblocks
            // `window.__TAURI__.event.listen` (and its underlying
            // fetch to ipc.localhost) for cross-window events.
            //
            // FIX3a's rewrite path mutated the CSP string in place;
            // some Discord responses ended up with a malformed CSP
            // that WebView2 then rejected, producing a white screen.
            // Removing the header entirely sidesteps that whole
            // class of bug — there's no string parse / serialize
            // round-trip to get wrong.
            //
            // Threat-model note: Discord's CSP exists to protect
            // against XSS from third-party CDN content and click-
            // jacking on the web app. Neither applies inside a
            // Tauri WebView where we control all JS injection via
            // `initialization_script` and the only trusted UI lives
            // on the separate `osl-gate://` origin under its own
            // capability. Tauri's capability layer remains the
            // actual security boundary.
            //
            // Env var `OSL_DISABLE_CSP_STRIP=1` (or `=true`) skips
            // the strip entirely — emergency escape hatch if the
            // strip is implicated in a future regression.
            let csp_strip_disabled = std::env::var("OSL_DISABLE_CSP_STRIP")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if csp_strip_disabled {
                tracing::warn!(
                    "[OSL][csp] stripping disabled via OSL_DISABLE_CSP_STRIP; \
                     cross-window events may fail"
                );
            }
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(initial_url))
                .title("OSL Privacy")
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
                // 7d-FIX3b: strip CSP from discord-origin responses
                // so the WebView lets Tauri's IPC fetch endpoint
                // through. Body untouched; only the CSP header is
                // removed. Non-Discord responses pass through
                // unchanged.
                .on_web_resource_request(move |request, response| {
                    if csp_strip_disabled {
                        return;
                    }
                    let host = request.uri().host().unwrap_or("");
                    let is_discord_origin = host == "discord.com"
                        || host == "www.discord.com"
                        || host == "ptb.discord.com"
                        || host == "canary.discord.com"
                        || host == "cdn.discordapp.com"
                        || host.ends_with(".discord.com")
                        || host.ends_with(".discordapp.com");
                    if !is_discord_origin {
                        return;
                    }
                    let headers = response.headers_mut();
                    let removed = headers.remove("content-security-policy").is_some();
                    let removed_ro = headers
                        .remove("content-security-policy-report-only")
                        .is_some();
                    if removed || removed_ro {
                        tracing::debug!(
                            host = %host,
                            path = %request.uri().path(),
                            "[OSL][csp] stripped CSP from discord response"
                        );
                    }
                })
                .build()?;

            // Apply screenshot resistance immediately. This runs
            // once at startup; `set_screenshot_protection` lets a
            // future overlay UI toggle it later. The `on_page_load`
            // hook above also re-applies on every subsequent
            // navigation event so newly-spawned WebView2 child HWNDs
            // get the affinity flag too.
            if let Err(e) = screenshot::apply_to_window(&window, ScreenshotProtection::On) {
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

            // F2.4: license-refresh task. `run_autostart` did the
            // synchronous cache-only classify (so the first webview
            // render reads a real value, not Free-by-default). This
            // task then:
            //   1. Fires one immediate `refresh_license_state` —
            //      the keyserver round-trip happens in the
            //      background; AppState gets the fresh classification
            //      typically within a second or two of launch.
            //   2. Re-fires every 6 hours for the lifetime of the
            //      process. The interval is from-launch, not
            //      wallclock — fine per F2 discovery D5.
            //
            // Sync `refresh_license_state` uses `reqwest::blocking`
            // (which carries its own tokio runtime); we wrap it in
            // `spawn_blocking` so it doesn't stall tauri's async
            // runtime. AppState's Mutex serialises against the
            // hot-path `osl_validate_license` write — they can't
            // race destructively.
            let refresh_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use std::time::Duration;
                // Immediate fire.
                let h = refresh_handle.clone();
                let _ = tauri::async_runtime::spawn_blocking(move || {
                    let s = h.state::<AppState>();
                    if let Ok(dir) = keystore::osl_config_dir() {
                        let _ = ipc::license_lifecycle::refresh_license_state(s.inner(), &dir);
                    }
                })
                .await;
                // Then every 6 hours. `tokio::time::interval`'s
                // first tick fires immediately; we discard it
                // because we just ran the refresh above.
                let mut iv = tokio::time::interval(Duration::from_secs(6 * 60 * 60));
                iv.tick().await;
                loop {
                    iv.tick().await;
                    let h = refresh_handle.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let s = h.state::<AppState>();
                        if let Ok(dir) = keystore::osl_config_dir() {
                            let _ = ipc::license_lifecycle::refresh_license_state(s.inner(), &dir);
                        }
                    })
                    .await;
                }
            });

            // Phase F0: register the deep-link on_open_url handler.
            // tauri-plugin-deep-link delivers `osl://...` URLs here
            // whenever Windows activates our protocol. We log to the
            // Rust console (proves Rust-side reception independently
            // of any webview event channel) and emit a Tauri event
            // to the webview so boot.js's listener can show a toast
            // and round-trip the URL through `osl_test_deep_link`.
            //
            // `event.urls()` returns a Vec because the OS can hand
            // off multiple URLs in a single activation (rare but
            // possible). F0 logs and emits each one independently —
            // F2 will replace this whole callback with the real
            // unlock-token handler.
            //
            // Best-effort emit: if no webview is up yet (race during
            // cold launch — scenario a in the F0 verification
            // matrix), the emit drops silently. The Rust log still
            // proves the URL arrived. F2 adds a buffered-URL replay
            // pattern keyed on a "boot.js ready" signal.
            let app_handle_for_dl = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    let url_str = url.to_string();
                    tracing::info!(
                        target: "osl::deep_link",
                        url = %url_str,
                        "[OSL deep-link] arrived from on_open_url"
                    );
                    if let Err(e) = app_handle_for_dl.emit("osl:deep-link-received", &url_str) {
                        tracing::warn!(
                            target: "osl::deep_link",
                            error = ?e,
                            "[OSL deep-link] emit to webview failed"
                        );
                    }
                }
            });

            // G3.3 T-G3.3.5: one silent background update check a few
            // seconds after launch. Non-blocking, non-intrusive: it
            // never opens a modal — on a hit it just emits
            // `osl:update-available` so the UI can show a passive
            // badge on the settings icon. No polling; the only other
            // automatic trigger is the manual "Check for updates"
            // button. Failures are swallowed (a missed silent check
            // is not worth bothering the user about).
            {
                let app_for_check = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                    match osl_build_channel_updater(&app_for_check) {
                        Ok(updater) => match updater.check().await {
                            Ok(Some(update)) => {
                                tracing::info!(
                                    target: "osl::updater",
                                    next = %update.version,
                                    "[OSL updater] startup check: update available"
                                );
                                let _ = app_for_check.emit(
                                    "osl:update-available",
                                    serde_json::json!({ "next": update.version }),
                                );
                            }
                            Ok(None) => {
                                tracing::info!(
                                    target: "osl::updater",
                                    "[OSL updater] startup check: up to date"
                                );
                            }
                            Err(e) => {
                                tracing::warn!(
                                    target: "osl::updater",
                                    error = %e,
                                    "[OSL updater] startup check failed (ignored)"
                                );
                            }
                        },
                        Err(e) => {
                            tracing::warn!(
                                target: "osl::updater",
                                error = %e,
                                "[OSL updater] startup check: updater build failed (ignored)"
                            );
                        }
                    }
                });
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
            osl_encrypt_message,
            osl_decrypt_message,
            osl_load_channel_history,
            osl_burn_message,
            osl_persist_edit,
            osl_persist_outbound,
            osl_encrypt_message_v2,
            osl_seal_attachment,
            osl_open_attachment,
            osl_seal_attachment_with_cover_v2,
            osl_open_attachment_v2,
            osl_encrypt_attachment_envelope,
            osl_seal_attachment_with_cover_v3,
            osl_fetch_attachment_bytes,
            osl_send_burn_marker,
            // 9-C1: invitation/response/accept/decline/list_pending all retired.
            osl_apply_burn,
            osl_unwhitelist_scope,
            osl_local_unwhitelist_scope,
            osl_set_whitelist,
            osl_get_scope_encryption_state,
            osl_get_scope_whitelist_summary,
            osl_bulk_set_whitelist,
            osl_bulk_unwhitelist_scope,
            osl_toggle_scope_encryption,
            osl_set_scope_encrypt,
            osl_get_self_user_id,
            osl_register_self_snowflake,
            osl_get_identity_info,
            osl_validate_license,
            osl_get_license_state,
            osl_clear_license,
            osl_get_tier_gate_status,
            osl_list_all_whitelists,
            osl_password_status,
            osl_set_main_password,
            osl_change_main_password,
            osl_remove_main_password,
            osl_view_recovery_phrase,
            osl_verify_main_password,
            osl_verify_recovery_phrase,
            osl_set_main_password_after_recovery,
            osl_lockout_status,
            osl_set_stealth_password,
            osl_remove_stealth_password,
            osl_stealth_password_status,
            osl_set_burn_password,
            osl_remove_burn_password,
            osl_burn_password_status,
            osl_verify_gate_password,
            osl_stealth_mode_engage,
            osl_burn_engage,
            osl_burn_scope_data,
            osl_control_inbox_post,
            osl_control_inbox_drain,
            osl_attachment_cache_put,
            osl_attachment_cache_get,
            osl_mark_scope_burned,
            osl_unburn_scope,
            osl_list_burned_scopes,
            osl_membership_update,
            osl_membership_get,
            osl_note_scope_membership,
            osl_get_server_whitelist_state,
            osl_set_server_header_whitelist,
            osl_set_channel_whitelist,
            osl_set_friend_ids,
            osl_get_friend_ids,
            osl_set_guild_list,
            osl_get_guild_list,
            osl_bulk_set_dm_whitelist,
            osl_set_server_default,
            osl_get_server_defaults,
            osl_apply_server_default_to_existing_channels,
            osl_get_app_preferences,
            osl_set_app_preferences,
            osl_tour_get_state,
            osl_tour_advance,
            osl_tour_complete,
            osl_tour_skip,
            osl_tour_reset,
            osl_take_last_persist_error,
            // REGISTER-FIX: TOFU + registration-conflict surface.
            osl_take_registration_alert,
            osl_list_key_change_alerts,
            osl_accept_key_change,
            osl_decline_key_change,
            osl_reset_v4_session,
            osl_reset_v5_sender_key,
            osl_build_skdm_request,
            osl_build_session_reset,
            osl_recover_peer_identity,
            osl_peer_safety_number,
            osl_self_safety_number,
            osl_open_settings_window,
            osl_close_settings_window_if_open,
            // Phase F0: deep-link smoke-test parser. Removed in F2.
            osl_test_deep_link,
            // G3.1/G3.3: updater check + channel-aware install.
            osl_check_for_updates,
            osl_install_update,
            osl_get_update_channel,
            osl_set_update_channel,
            osl_prose_token_send,
            osl_prose_token_recv,
            osl_prose_token_burn,
            osl_get_scope_ttl,
            osl_set_scope_ttl,
            osl_scope_burn_blobs,
            osl_is_decommissioned,
        ])
        .run(tauri::generate_context!())
        .expect("error while running discord-privacy-client tauri app");
}
