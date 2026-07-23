//! Native Discord overlay attachment orchestration.
//!
//! The renderer can request selection/open by opaque attachment ID only. File
//! paths, capabilities, keys, ciphertext, and plaintext bytes stay in Rust.

use osl_privacy_hub::broker::{
    self, HubBrokerState, PendingNativeOverlayAttachment, PreparedNativeOverlayAttachment,
};
use osl_privacy_hub::core_bridge::HubCoreState;
use osl_privacy_hub::peer_attachment_io;
use osl_privacy_hub::security::HubSecurityState;
use osl_privacy_hub::service_host::ActiveServiceHost;
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::Path;
#[cfg(windows)]
use std::process::Command;
#[cfg(windows)]
use std::time::Duration;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroize;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenedNativeOverlayAttachment {
    attachment_id: String,
    original_filename: String,
    mime_type: String,
    plaintext_size: u64,
    view_once_consumed: bool,
    opened_in_native_viewer: bool,
}

fn require_active_pro(core: &HubCoreState) -> Result<(), String> {
    if ipc::tier_gate::is_paid_equivalent(&core.osl) {
        Ok(())
    } else {
        Err("Encrypted attachments require OSL Pro".to_owned())
    }
}

pub(crate) fn select_encrypt_upload_deliver(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    context_epoch: u64,
    expected_host: &ActiveServiceHost,
    view_once: bool,
) -> Result<Option<PreparedNativeOverlayAttachment>, String> {
    select_encrypt_upload_deliver_inner(
        app,
        core,
        security,
        broker_state,
        Some((context_epoch, expected_host)),
        view_once,
    )
}

pub(crate) fn select_osl_chat_attachment(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    view_once: bool,
) -> Result<Option<PreparedNativeOverlayAttachment>, String> {
    select_encrypt_upload_deliver_inner(app, core, security, broker_state, None, view_once)
}

fn validate_surface(
    app: &tauri::AppHandle,
    broker: &HubBrokerState,
    overlay: Option<(u64, &ActiveServiceHost)>,
) -> Result<(), String> {
    if let Some((epoch, host)) = overlay {
        super::require_same_overlay_context(app, epoch, host)
    } else {
        broker.active_osl_chat_context_token().map(|_| ())
    }
}

fn select_encrypt_upload_deliver_inner(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    overlay_context: Option<(u64, &ActiveServiceHost)>,
    view_once: bool,
) -> Result<Option<PreparedNativeOverlayAttachment>, String> {
    require_active_pro(core)?;
    let parent_label = if overlay_context.is_some() {
        super::native_discord_overlay::OVERLAY_LABEL
    } else {
        "main"
    };
    let parent = app
        .get_webview_window(parent_label)
        .ok_or_else(|| "The trusted attachment picker is unavailable".to_owned())?;
    let selected = app
        .dialog()
        .file()
        .set_parent(&parent)
        .set_title("Choose a private OSL attachment")
        .add_filter(
            "Supported private files",
            &[
                "jpg", "jpeg", "png", "gif", "webp", "mp4", "webm", "mov", "mp3", "m4a", "wav",
                "flac", "pdf", "txt", "md", "csv", "json", "docx", "xlsx", "pptx", "odt", "ods",
                "odp", "zip", "7z", "rar", "tar", "gz",
            ],
        )
        .blocking_pick_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    require_active_pro(core)?;
    validate_surface(app, broker_state, overlay_context)?;
    let selected_path = selected
        .into_path()
        .map_err(|_| "The selected attachment path is unavailable".to_owned())?;
    let filename = selected_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "The selected attachment filename is invalid".to_owned())?
        .to_owned();
    let mut source = File::open(&selected_path)
        .map_err(|_| "The selected attachment could not be opened".to_owned())?;
    let metadata = source
        .metadata()
        .map_err(|_| "The selected attachment could not be checked".to_owned())?;
    if !metadata.is_file() {
        return Err("The selected attachment is not a regular file".to_owned());
    }
    let plan = if overlay_context.is_some() {
        broker::begin_native_overlay_attachment(
            core,
            broker_state,
            filename,
            metadata.len(),
            view_once,
        )?
    } else {
        broker::begin_osl_chat_attachment(core, broker_state, filename, metadata.len(), view_once)?
    };
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "OSL attachment storage is unavailable".to_owned())?;
    let staged = peer_attachment_io::encrypt_file(
        &local_root,
        &mut source,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
        plan.content_id.to_vec(),
        0,
    )?;
    let (digest, sealed_size) = match peer_attachment_io::sha256_file(staged.path()) {
        Ok(value) => value,
        Err(error) => {
            let _ = peer_attachment_io::remove_staged_file(staged);
            return Err(error);
        }
    };
    let mut fetch_token = [0u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    let token_len = fetch_token.len();
    fetch_token.copy_from_slice(&crypto::random::random_bytes(token_len));
    let ttl = u32::try_from(plan.expires_at.saturating_sub(plan.created_at))
        .map_err(|_| "OSL attachment expiry is invalid".to_owned())?;
    let config_root = app
        .path()
        .app_config_dir()
        .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    let client = ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(&config_root),
    )
    .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    let sealed_file = File::open(staged.path())
        .map_err(|_| "OSL sealed attachment could not be reopened".to_owned())?;
    let upload = match client.upload_attachment_file(sealed_file, ttl, &fetch_token) {
        Ok(upload) => upload,
        Err(_) => {
            fetch_token.zeroize();
            let _ = peer_attachment_io::remove_staged_file(staged);
            return Err("The private attachment could not be uploaded".to_owned());
        }
    };
    if peer_attachment_io::remove_staged_file(staged).is_err() {
        let _ = client.delete_attachment(&upload.id_hex, &fetch_token);
        fetch_token.zeroize();
        return Err("OSL could not clear its sealed attachment staging".to_owned());
    }
    if let Err(error) = validate_surface(app, broker_state, overlay_context) {
        let _ = client.delete_attachment(&upload.id_hex, &fetch_token);
        fetch_token.zeroize();
        return Err(error);
    }
    if let Err(error) = require_active_pro(core) {
        let _ = client.delete_attachment(&upload.id_hex, &fetch_token);
        fetch_token.zeroize();
        return Err(error);
    }
    let burn_scope = plan.burn_scope.clone();
    let mut fetch_token_hex = lower_hex(&fetch_token);
    if osl_privacy_hub::security::record_peer_attachment_burn_capability(
        security,
        burn_scope.clone(),
        upload.id_hex.clone(),
        fetch_token_hex.clone(),
        upload.expires_at,
    )
    .is_err()
    {
        let _ = client.delete_attachment(&upload.id_hex, &fetch_token);
        fetch_token_hex.zeroize();
        fetch_token.zeroize();
        return Err("OSL could not retain the attachment burn capability".to_owned());
    }
    let digest_hex = lower_hex(&digest);
    let delivered = if overlay_context.is_some() {
        broker::deliver_native_overlay_attachment(
            core,
            broker_state,
            plan,
            sealed_size,
            digest_hex,
            upload.id_hex.clone(),
            fetch_token_hex,
        )
    } else {
        broker::deliver_osl_chat_attachment(
            core,
            broker_state,
            plan,
            sealed_size,
            digest_hex,
            upload.id_hex.clone(),
            fetch_token_hex,
        )
    };
    match delivered {
        Ok(prepared) => {
            fetch_token.zeroize();
            Ok(Some(prepared))
        }
        Err(error) => {
            if client
                .delete_attachment(&upload.id_hex, &fetch_token)
                .is_ok()
            {
                let _ = osl_privacy_hub::security::remove_peer_attachment_burn_capability(
                    security,
                    burn_scope,
                    &upload.id_hex,
                );
            }
            fetch_token.zeroize();
            let _ = error;
            Err("The private attachment could not be delivered".to_owned())
        }
    }
}

pub(crate) fn list_pending(
    core: &HubCoreState,
    security: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    require_active_pro(core)?;
    broker::list_native_overlay_attachments(core, security, broker)
}

pub(crate) fn list_osl_chat_pending(
    core: &HubCoreState,
    security: &HubSecurityState,
    broker: &HubBrokerState,
) -> Result<Vec<PendingNativeOverlayAttachment>, String> {
    require_active_pro(core)?;
    broker::list_osl_chat_attachments(core, security, broker)
}

pub(crate) fn open_pending(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    context_epoch: u64,
    expected_host: &ActiveServiceHost,
    attachment_id: &str,
) -> Result<OpenedNativeOverlayAttachment, String> {
    open_pending_inner(
        app,
        core,
        security,
        broker_state,
        Some((context_epoch, expected_host)),
        attachment_id,
    )
}

pub(crate) fn open_osl_chat_pending(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    attachment_id: &str,
) -> Result<OpenedNativeOverlayAttachment, String> {
    open_pending_inner(app, core, security, broker_state, None, attachment_id)
}

fn open_pending_inner(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    security: &HubSecurityState,
    broker_state: &HubBrokerState,
    overlay_context: Option<(u64, &ActiveServiceHost)>,
    attachment_id: &str,
) -> Result<OpenedNativeOverlayAttachment, String> {
    require_active_pro(core)?;
    let plan = if overlay_context.is_some() {
        broker::take_native_overlay_attachment(core, security, broker_state, attachment_id)?
    } else {
        broker::take_osl_chat_attachment(core, security, broker_state, attachment_id)?
    };
    let is_image = plan.mime_type.starts_with("image/");
    if is_image && !peer_attachment_io::supported_protected_image_mime(&plan.mime_type) {
        return Err(
            "This protected image format is not supported by the in-memory viewer".to_owned(),
        );
    }
    let token = parse_token(&plan.fetch_token)?;
    let local_root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "OSL attachment storage is unavailable".to_owned())?;
    let config_root = app
        .path()
        .app_config_dir()
        .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    let client = ipc::cipher_store_client::CipherStoreClient::new(
        ipc::cipher_store_client::resolve_cipher_store_base_url(&config_root),
    )
    .map_err(|_| "OSL attachment transport is unavailable".to_owned())?;
    let (download_path, mut download) = peer_attachment_io::create_download_file(&local_root)?;
    let cleanup_download = |path: &Path| {
        let _ = peer_attachment_io::remove_staging_path(path);
    };
    let fetched = match client.fetch_attachment_to_writer(&plan.object_id, &token, &mut download) {
        Ok(size) => size,
        Err(_) => {
            drop(download);
            cleanup_download(&download_path);
            return Err("This private attachment is unavailable or expired".to_owned());
        }
    };
    if fetched != plan.sealed_size || download.sync_all().is_err() {
        drop(download);
        cleanup_download(&download_path);
        return Err("This private attachment has an invalid size".to_owned());
    }
    drop(download);
    let (digest, size) = peer_attachment_io::sha256_file(&download_path)?;
    if size != plan.sealed_size || lower_hex(&digest) != plan.ciphertext_sha256 {
        cleanup_download(&download_path);
        return Err("This private attachment failed authentication".to_owned());
    }
    let mut sealed = File::open(&download_path)
        .map_err(|_| "This private attachment could not be opened".to_owned())?;
    sealed
        .seek(SeekFrom::Start(0))
        .map_err(|_| "This private attachment could not be opened".to_owned())?;
    if is_image {
        let opened = match peer_attachment_io::decrypt_file_to_memory(
            &mut sealed,
            &plan.original_filename,
            &plan.mime_type,
            crypto::aead::Key::from_bytes(plan.attachment_key),
        ) {
            Ok(opened) => opened,
            Err(error) => {
                cleanup_download(&download_path);
                return Err(error);
            }
        };
        cleanup_download(&download_path);
        if opened.len() as u64 != plan.plaintext_size {
            return Err("This private attachment has an invalid plaintext size".to_owned());
        }
        if let Err(error) = validate_surface(app, broker_state, overlay_context) {
            return Err(error);
        }
        require_active_pro(core)?;
        // Decode and create the window while it is hidden. `prepare` applies
        // and reads back capture exclusion before returning; Drop closes the
        // hidden window and zeroizes its pixels on every later failure.
        let viewer = super::native_image_viewer::prepare(app, opened)?;
        validate_surface(app, broker_state, overlay_context)?;
        require_active_pro(core)?;
        if overlay_context.is_some() {
            broker::commit_native_overlay_attachment_open(core, security, broker_state, &plan)?;
        } else {
            broker::commit_osl_chat_attachment_open(core, security, broker_state, &plan)?;
        }
        if plan.view_once {
            let _ = client.delete_attachment(&plan.object_id, &token);
        }
        viewer.show()?;
        return Ok(OpenedNativeOverlayAttachment {
            attachment_id: plan.attachment_id.clone(),
            original_filename: plan.original_filename.clone(),
            mime_type: plan.mime_type.clone(),
            plaintext_size: plan.plaintext_size,
            view_once_consumed: plan.view_once,
            opened_in_native_viewer: true,
        });
    }
    let opened = match peer_attachment_io::decrypt_file(
        &local_root,
        &mut sealed,
        &plan.original_filename,
        &plan.mime_type,
        crypto::aead::Key::from_bytes(plan.attachment_key),
    ) {
        Ok(opened) => opened,
        Err(error) => {
            cleanup_download(&download_path);
            return Err(error);
        }
    };
    cleanup_download(&download_path);
    if opened.plaintext_len() != plan.plaintext_size {
        let _ = peer_attachment_io::remove_staged_file(opened);
        return Err("This private attachment has an invalid plaintext size".to_owned());
    }
    if let Err(error) = validate_surface(app, broker_state, overlay_context) {
        let _ = peer_attachment_io::remove_staged_file(opened);
        return Err(error);
    }
    if let Err(error) = require_active_pro(core) {
        let _ = peer_attachment_io::remove_staged_file(opened);
        return Err(error);
    }
    let committed = if overlay_context.is_some() {
        broker::commit_native_overlay_attachment_open(core, security, broker_state, &plan)
    } else {
        broker::commit_osl_chat_attachment_open(core, security, broker_state, &plan)
    };
    if let Err(error) = committed {
        let _ = peer_attachment_io::remove_staged_file(opened);
        return Err(error);
    }
    if plan.view_once {
        // Replay is committed and the encrypted inbox capability is deleted
        // before best-effort R2 deletion. A backend failure cannot reopen it.
        let _ = client.delete_attachment(&plan.object_id, &token);
    }
    let response = OpenedNativeOverlayAttachment {
        attachment_id: plan.attachment_id.clone(),
        original_filename: plan.original_filename.clone(),
        mime_type: plan.mime_type.clone(),
        plaintext_size: plan.plaintext_size,
        view_once_consumed: plan.view_once,
        opened_in_native_viewer: true,
    };
    launch_and_scavenge(opened)?;
    Ok(response)
}

fn lower_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn parse_token(value: &str) -> Result<[u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES], String> {
    if value.len() != ipc::cipher_store_client::FETCH_TOKEN_BYTES * 2 {
        return Err("This private attachment capability is invalid".to_owned());
    }
    let mut output = [0u8; ipc::cipher_store_client::FETCH_TOKEN_BYTES];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| "This private attachment capability is invalid".to_owned())?;
        output[index] = u8::from_str_radix(text, 16)
            .map_err(|_| "This private attachment capability is invalid".to_owned())?;
    }
    Ok(output)
}

fn launch_and_scavenge(staged: peer_attachment_io::StagedAttachment) -> Result<(), String> {
    if ipc::attachment_wire::is_blocked_automatic_open_filename(staged.original_filename()) {
        let _ = peer_attachment_io::remove_staged_file(staged);
        return Err("This attachment type cannot be opened automatically".to_owned());
    }
    #[cfg(windows)]
    {
        let path = staged.path().to_owned();
        let child = Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(&path)
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(_) => {
                let _ = peer_attachment_io::remove_staged_file(staged);
                return Err("The native attachment viewer could not be opened".to_owned());
            }
        };
        std::thread::spawn(move || {
            let _ = child.wait();
            // Some Windows shell handlers hand off to an existing process and
            // exit immediately. Give that viewer a bounded read window, then
            // remove OSL's only plaintext staging file.
            std::thread::sleep(Duration::from_secs(10));
            let _ = peer_attachment_io::remove_staged_file(staged);
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = peer_attachment_io::remove_staged_file(staged);
        Err("Native attachment viewing is available only on Windows".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_parser_is_exact_and_lowercase_agnostic_only_at_decode() {
        assert_eq!(
            parse_token("00112233445566778899aabbccddeeff").unwrap()[0],
            0
        );
        assert!(parse_token("0011").is_err());
        assert!(parse_token("00112233445566778899aabbccddeefg").is_err());
    }
}
