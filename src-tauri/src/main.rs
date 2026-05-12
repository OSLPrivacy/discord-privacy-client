// Tauri shell entry point.
//
// Layer 9: the main window loads `https://discord.com/app` directly.
// WebView2's default user-data folder under
// `%LOCALAPPDATA%\<bundle-id>\EBWebView` persists cookies across
// restarts so Discord login survives without extra config. Discord
// serves its own CSP via response headers; Tauri's `app.security.csp`
// is `null` so no local CSP gets injected to clash with it.
//
// Layer 10: the main window is built programmatically
// (rather than declared in tauri.conf.json) so we can attach an
// `initialization_script` that runs before Discord's bundle loads.
// The script (`injection::BOOT_SCRIPT`) hooks
// `webpackChunkdiscord_app` and source-rewrites the chat-input
// module's sendMessage call site to route outbound content through
// the OSL IPC encryption commands.
//
// IPC from the discord.com origin is gated by
// `capabilities/main.json` (Tauri 2 blocks IPC from remote URLs by
// default). `capabilities/main.json` grants the Discord origin only
// the message and whitelist operations needed by the injected
// workflow; raw crypto, identity-generation, filesystem-backed, and
// password/recovery commands remain off the remote origin.
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
    cmd_load_identity, cmd_osl_accept_invitation, cmd_osl_apply_burn, cmd_osl_burn_engage,
    cmd_osl_burn_message, cmd_osl_burn_password_status, cmd_osl_burn_scope_data,
    cmd_osl_change_main_password, cmd_osl_decline_invitation, cmd_osl_decrypt_message_v2,
    cmd_osl_encrypt_message, cmd_osl_encrypt_message_v2, cmd_osl_get_identity_info,
    cmd_osl_get_scope_encryption_state, cmd_osl_get_self_user_id, cmd_osl_list_all_whitelists,
    cmd_osl_list_burned_scopes, cmd_osl_list_pending_invitations, cmd_osl_load_channel_history,
    cmd_osl_lockout_status, cmd_osl_mark_scope_burned, cmd_osl_password_status,
    cmd_osl_persist_edit, cmd_osl_register_self_snowflake, cmd_osl_remove_burn_password,
    cmd_osl_remove_main_password, cmd_osl_remove_stealth_password, cmd_osl_send_burn_marker,
    cmd_osl_send_whitelist_invitation, cmd_osl_send_whitelist_response, cmd_osl_set_burn_password,
    cmd_osl_set_main_password, cmd_osl_set_main_password_after_recovery,
    cmd_osl_set_stealth_password, cmd_osl_set_whitelist, cmd_osl_stealth_mode_engage,
    cmd_osl_stealth_password_status, cmd_osl_toggle_scope_encryption, cmd_osl_unburn_scope,
    cmd_osl_unwhitelist_scope, cmd_osl_verify_gate_password, cmd_osl_verify_main_password,
    cmd_osl_verify_recovery_phrase, cmd_osl_view_recovery_phrase, cmd_register, cmd_save_identity,
    cmd_status, cmd_stego_decode, cmd_stego_encode, cmd_x25519_diffie_hellman, AeadOpenRequest,
    AeadSealRequest, AeadSealResponse, BurnScopeDataDto, BurnedScopeDto, FetchPubkeysResponse,
    GateVerifyDto, GenerateIdentityResponse, IdentityInfoDto, LockoutStatusDto, PasswordStatusDto,
    PendingInvitationDto, RegisterResponse, ScopeEncryptionState, StatusResponse,
    StegoDecodeResponse, StegoEncodeRequest, StegoEncodeResponse, StoredMessageDto,
    WhitelistRowDto,
};
use ipc::scope::ScopeInput;
use ipc::{AppState, IpcError, IpcResult};
use runtime::ScreenshotProtection;
use tauri::{Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

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
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_persist_edit(state.inner(), discord_message_id, new_plaintext)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

// ---- Phase 7b: wire v=2 + control message Tauri wrappers ----

/// Layer 10 / Phase 7b: encrypt a v=2 content message under a
/// whitelist-resolved recipient list. See
/// [`ipc::commands::cmd_osl_encrypt_message_v2`].
#[tauri::command]
async fn osl_encrypt_message_v2(
    app: tauri::AppHandle,
    plaintext: String,
    scope_input: ScopeInput,
    channel_members: Vec<String>,
    self_discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    let scope_for_unburn = scope_input.clone();
    let scope_for_event = scope_input.clone();
    let result: Result<String, String> = tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let wire = cmd_osl_encrypt_message_v2(
            state.inner(),
            plaintext,
            scope_input,
            channel_members,
            self_discord_id,
        )?;
        // 7d-PIVOT-FIX2 Bug F: re-engaging a burned scope via a
        // fresh encrypted send un-burns it. Returns true iff the
        // scope was actually in the burned ledger and got removed.
        let unburned =
            ipc::commands::cmd_osl_unburn_scope_after_encrypt(state.inner(), scope_for_unburn);
        Ok((wire, unburned))
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
    .map(|(wire, unburned)| {
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
        wire
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

/// Layer 10 / Phase 7b: build the wire-format whitelist
/// invitation for a peer + scope. See §7.1.
#[tauri::command]
async fn osl_send_whitelist_invitation(
    app: tauri::AppHandle,
    to_discord_id: String,
    scope_input: ScopeInput,
    from_discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_send_whitelist_invitation(
            state.inner(),
            to_discord_id,
            scope_input,
            from_discord_id,
        )
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Layer 10 / Phase 7b: build the wire-format whitelist response
/// (accept / decline). See §7.3 / §7.4.
#[tauri::command]
async fn osl_send_whitelist_response(
    app: tauri::AppHandle,
    to_discord_id: String,
    scope_input: ScopeInput,
    accepted: bool,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_send_whitelist_response(state.inner(), to_discord_id, scope_input, accepted)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

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

/// Layer 10 / Phase 7b: accept a pending whitelist invitation
/// and grant future decryption of `(from, scope)` messages.
#[tauri::command]
async fn osl_accept_invitation(app: tauri::AppHandle, invitation_id: String) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_accept_invitation(state.inner(), invitation_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

/// Layer 10 / Phase 7b: decline a pending whitelist invitation.
#[tauri::command]
async fn osl_decline_invitation(
    app: tauri::AppHandle,
    invitation_id: String,
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_decline_invitation(state.inner(), invitation_id)
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

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
/// Returns the wire-format invitation to ship through Discord's
/// API so the peer can accept/decline.
#[tauri::command]
async fn osl_set_whitelist(
    app: tauri::AppHandle,
    peer_discord_id: String,
    scope_input: ScopeInput,
    broadened: bool,
    from_discord_id: String,
) -> Result<String, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_set_whitelist(
            state.inner(),
            peer_discord_id,
            scope_input,
            broadened,
            from_discord_id,
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

/// Phase 7c: list `pending_invitations` for the banner system.
/// Returns one DTO per pending entry, oldest first.
#[tauri::command]
async fn osl_list_pending_invitations(
    app: tauri::AppHandle,
) -> Result<Vec<PendingInvitationDto>, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_list_pending_invitations(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
}

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
        cmd_osl_burn_engage(state.inner())
    })
    .await
    .map_err(|e| format!("OSL: join error: {e}"))?
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
) -> Result<(), String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        cmd_osl_mark_scope_burned(state.inner(), scope_kind, scope_id, server_id, channel_id)
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

fn main() {
    tauri::Builder::default()
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
            osl_encrypt_message_v2,
            osl_seal_attachment,
            osl_open_attachment,
            osl_encrypt_attachment_envelope,
            osl_send_burn_marker,
            osl_send_whitelist_invitation,
            osl_send_whitelist_response,
            osl_apply_burn,
            osl_accept_invitation,
            osl_decline_invitation,
            osl_unwhitelist_scope,
            osl_set_whitelist,
            osl_get_scope_encryption_state,
            osl_toggle_scope_encryption,
            osl_set_scope_encrypt,
            osl_list_pending_invitations,
            osl_get_self_user_id,
            osl_register_self_snowflake,
            osl_get_identity_info,
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
            osl_mark_scope_burned,
            osl_unburn_scope,
            osl_list_burned_scopes,
            osl_open_settings_window,
            osl_close_settings_window_if_open,
        ])
        .run(tauri::generate_context!())
        .expect("error while running discord-privacy-client tauri app");
}
