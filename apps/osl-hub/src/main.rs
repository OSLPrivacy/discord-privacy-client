#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use osl_privacy_hub::broker::{
    self, DecryptedLocalProtectedMessage, HubBrokerState, OpenedHubAttachment,
    OpenedNativeOverlayTextBatch, OpenedPeerProseMessage, PreparedCoreMessage,
    PreparedHubAttachment, PreparedLocalProtectedMessage, PreparedNativeOverlayText,
    PreparedPeerProseMessage,
};
use osl_privacy_hub::browser_companion::{
    BrowserAccountMode, BrowserCompanionAction, BrowserCompanionState, BrowserCompanionStatus,
};
use osl_privacy_hub::cleanup::{self, HubFullCleanupResult};
use osl_privacy_hub::core_bridge::{
    self, CoreFeature, CoreReadiness, HubCoreState, HubLicenseState,
};
use osl_privacy_hub::identity_registry::{
    self, HubIdentityBurnResult, HubIdentityRegistryState, HubIdentitySlotCreation,
    HubIdentitySlotDto, HubIdentitySwitchResult,
};
use osl_privacy_hub::mass_cleanup::{
    self, MassCleanupCapabilityManifest, MassCleanupDiscoveryRequest, MassCleanupExecutionRequest,
};
use osl_privacy_hub::models::{
    EmailProvider, LinkedAccountDemo, LinkedServiceDemo, OnboardingPreferences, ServiceKind,
};
use osl_privacy_hub::mullvad_window_host::{MullvadWindowHostResult, MullvadWindowHostState};
use osl_privacy_hub::native_apps::{
    self, BrowserAccountImportResult, BrowserImportId, BrowserImportResult, BrowserImportStatus,
    FirefoxInstallResult, FirefoxLaunchResult, FirefoxServiceId, FirefoxStatus,
    MullvadActionResult, MullvadStatus, NativeAppId, NativeAppStatus, NativeInstallResult,
    ProtectedBrowserImportResult,
};
use osl_privacy_hub::native_discord_adapter::{
    deidentify_prepared_visual_structure, DiscordCarrierLayout, DiscordCarrierMode,
    DiscordCarrierReceipt, NativeDiscordComposerState,
};
use osl_privacy_hub::native_window_host::{
    DiscordSessionMode, NativeWindowHostReason, NativeWindowHostResult, NativeWindowHostState,
};
use osl_privacy_hub::osl_profile::{HubProfileDto, HubProfileInput};
use osl_privacy_hub::password_lifecycle::{
    self, HubIdentitySetupResult, HubMainPasswordSetupResult,
};
use osl_privacy_hub::peer_attachment_io;
use osl_privacy_hub::preferences::PreviewState;
use osl_privacy_hub::privacy_scan::{self, LocalMessageCandidate, LocalPrivacyScanResult};
use osl_privacy_hub::pro_context_cover::LocalCoverState;
use osl_privacy_hub::scrub_imap::{
    self, ConfigureImapRequest, ImapAccountRequest, ImapCapability, ImapDeleteResult,
    ImapEnumeration, ImapInspection, ImapItemRequest, ImapVerification, ScrubImapState,
};
use osl_privacy_hub::scrub_index::{
    selection_requires_registry_ownership, ScrubIndexChunkRequest, ScrubIndexInitializeRequest,
    ScrubIndexState, ScrubIndexStatus,
};
use osl_privacy_hub::security::{
    self, AddFriendResult, FriendCodeExport, HubScopeBurnResult, HubSecurityState, PersonDto,
    ScopeSecurityDto,
};
use osl_privacy_hub::security_credentials::{self, HubPasswordRoleStatus};
use osl_privacy_hub::service_host::{self, ActiveServiceHost, ServiceHostState};
use osl_privacy_hub::service_scope_index::{ImmutableServiceBurnManifest, ServiceScopeIndexState};
use osl_privacy_hub::services::ServiceRegistryState;
use osl_privacy_hub::startup_gate::{self, HubGateUnlockResult, VerifiedGateRole};
use osl_privacy_hub::updates::{
    bounded_plain_notes, bounded_version, RELEASES_URL, SOURCE_REPOSITORY_URL,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{Manager, State};
use tauri_plugin_updater::UpdaterExt;

#[cfg(windows)]
mod window_border;

mod native_attachment_transport;
mod native_discord_overlay;
mod native_image_viewer;

use native_discord_overlay::OverlaySessionState;

#[allow(dead_code)]
#[path = "../../../src-tauri/src/screenshot.rs"]
mod screenshot;

#[tauri::command]
fn get_onboarding_preferences(
    state: State<'_, PreviewState>,
) -> Result<OnboardingPreferences, String> {
    state.get()
}

#[tauri::command]
fn set_hub_screenshot_protection(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    if !enabled {
        native_discord_overlay::clear_and_hide(&app);
    }
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "OSL Privacy is unavailable".to_owned())?;
    let protection = if enabled {
        runtime::ScreenshotProtection::On
    } else {
        runtime::ScreenshotProtection::Off
    };
    screenshot::apply_to_window(&window, protection)
        .map_err(|_| "Windows capture resistance could not be changed".to_owned())
}

#[tauri::command]
async fn configure_scrub_imap_account(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    request: ConfigureImapRequest,
) -> Result<ImapCapability, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        scrub_imap::configure(&app.state::<ScrubImapState>(), &owner, request)
    })
    .await
    .map_err(|_| "IMAP configuration worker was unavailable".to_owned())?
}

#[tauri::command]
async fn get_scrub_imap_capability(
    core: State<'_, HubCoreState>,
    state: State<'_, ScrubImapState>,
    session: State<'_, HubAccountSessionState>,
    request: ImapAccountRequest,
) -> Result<ImapCapability, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    Ok(scrub_imap::capability(&state, &owner, &request.account_id))
}

#[tauri::command]
async fn reauth_scrub_imap_account(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    request: ImapAccountRequest,
) -> Result<ImapCapability, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        scrub_imap::reauthenticate(&app.state::<ScrubImapState>(), &owner, &request.account_id)
    })
    .await
    .map_err(|_| "IMAP re-authentication worker was unavailable".to_owned())?
}

#[tauri::command]
async fn scrub_imap_enumerate(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    request: ImapItemRequest,
) -> Result<ImapEnumeration, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        scrub_imap::enumerate(&app.state::<ScrubImapState>(), &owner, &request)
    })
    .await
    .map_err(|_| "IMAP enumeration worker was unavailable".to_owned())?
}

#[tauri::command]
async fn scrub_imap_inspect(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    request: ImapItemRequest,
) -> Result<ImapInspection, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        scrub_imap::inspect(&app.state::<ScrubImapState>(), &owner, &request)
    })
    .await
    .map_err(|_| "IMAP inspection worker was unavailable".to_owned())?
}

#[tauri::command]
async fn scrub_imap_delete(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    request: ImapItemRequest,
) -> Result<ImapDeleteResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        scrub_imap::delete(&app.state::<ScrubImapState>(), &owner, &request)
    })
    .await
    .map_err(|_| "IMAP deletion worker was unavailable".to_owned())?
}

#[tauri::command]
async fn scrub_imap_verify(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    request: ImapItemRequest,
) -> Result<ImapVerification, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    tauri::async_runtime::spawn_blocking(move || {
        scrub_imap::verify(&app.state::<ScrubImapState>(), &owner, &request)
    })
    .await
    .map_err(|_| "IMAP verification worker was unavailable".to_owned())
}

#[tauri::command]
fn save_onboarding_preferences(
    state: State<'_, PreviewState>,
    preferences: OnboardingPreferences,
) -> Result<OnboardingPreferences, String> {
    state.save(preferences)
}

#[tauri::command]
async fn scan_local_privacy(
    messages: Vec<LocalMessageCandidate>,
) -> Result<LocalPrivacyScanResult, String> {
    privacy_scan::validate_attachment_input_batch(&messages)?;
    tokio::task::spawn_blocking(move || privacy_scan::scan_local_messages(messages))
        .await
        .map_err(|_| "The local privacy scan was interrupted".to_owned())
}

#[tauri::command]
async fn get_osl_profile(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Option<HubProfileDto>, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    tokio::task::spawn_blocking(move || osl_privacy_hub::osl_profile::get_active_profile(&owner))
        .await
        .map_err(|_| "OSL profile read was interrupted".to_owned())?
}

#[tauri::command]
async fn save_osl_profile(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    profile: HubProfileInput,
) -> Result<HubProfileDto, String> {
    let _session = session.transition.lock().await;
    tokio::task::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let owner = active_unlocked_osl_user_id(&core)?;
        let previous = osl_privacy_hub::osl_profile::get_active_profile(&owner)?;
        let exported = security::export_friend_code(&core)?;
        let identity = core
            .osl
            .identity
            .lock()
            .map_err(|_| "OSL identity state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
        let keyserver = core
            .osl
            .keyserver
            .lock()
            .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL keyserver is not initialized".to_owned())?;
        // Local encrypted persistence happens before the public claim, but
        // only after every local dependency is available. A local write
        // failure can therefore never mutate or strand a public reservation.
        let saved = osl_privacy_hub::osl_profile::save_active_profile(&owner, profile)?;
        match keyserver.claim_username(&identity, &saved.username_candidate, &exported.friend_code)
        {
            Ok(claimed)
                if claimed.username == saved.username_candidate && claimed.user_id == owner =>
            {
                Ok(saved)
            }
            Ok(_) => {
                osl_privacy_hub::osl_profile::restore_active_profile(&owner, previous)?;
                Err("OSL username claim returned the wrong identity".to_owned())
            }
            Err(claim_error) => match keyserver.lookup_username(&saved.username_candidate) {
                // A response can be lost after the server commits. Read back
                // exact signed ownership before deciding whether to roll back.
                Ok(found) if found.friend_code == exported.friend_code => Ok(saved),
                Ok(_) => {
                    osl_privacy_hub::osl_profile::restore_active_profile(&owner, previous)?;
                    Err(format!("OSL username could not be claimed: {claim_error}"))
                }
                // Offline is indeterminate. Keep the locally requested value
                // for later read-only reconciliation; never guess or mutate a
                // second username as compensation.
                Err(_) => Ok(saved),
            },
        }
    })
    .await
    .map_err(|_| "OSL profile save was interrupted".to_owned())?
}

#[tauri::command]
async fn list_osl_notes(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<osl_privacy_hub::osl_notes::OslNote>, String> {
    let _session = session.transition.lock().await;
    let _owner = active_unlocked_osl_user_id(&core)?;
    tokio::task::spawn_blocking(osl_privacy_hub::osl_notes::list)
        .await
        .map_err(|_| "OSL Notes read was interrupted".to_owned())?
}

#[tauri::command]
async fn save_osl_note(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    input: osl_privacy_hub::osl_notes::OslNoteInput,
) -> Result<osl_privacy_hub::osl_notes::OslNote, String> {
    let _session = session.transition.lock().await;
    let _owner = active_unlocked_osl_user_id(&core)?;
    tokio::task::spawn_blocking(move || osl_privacy_hub::osl_notes::upsert(input))
        .await
        .map_err(|_| "OSL Notes save was interrupted".to_owned())?
}

#[tauri::command]
async fn delete_osl_note(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    note_id: String,
) -> Result<bool, String> {
    let _session = session.transition.lock().await;
    let _owner = active_unlocked_osl_user_id(&core)?;
    tokio::task::spawn_blocking(move || osl_privacy_hub::osl_notes::delete(&note_id))
        .await
        .map_err(|_| "OSL Notes deletion was interrupted".to_owned())?
}

fn active_unlocked_osl_user_id(core: &HubCoreState) -> Result<String, String> {
    core_bridge::readiness(core)
        .active_osl_user_id
        .ok_or_else(|| "Unlock an OSL identity before accessing service profiles".to_owned())
}

fn require_current_context_host(
    app: &tauri::AppHandle,
    core: &HubCoreState,
    broker: &HubBrokerState,
    context_token: &str,
) -> Result<ActiveServiceHost, String> {
    let owner = active_unlocked_osl_user_id(core)?;
    if let Ok(native) = app
        .state::<NativeWindowHostState>()
        .current_discord_service_host(&owner)
    {
        if broker.validate_active_host(context_token, &native).is_ok() {
            return Ok(native);
        }
    }
    let active = app
        .state::<ServiceHostState>()
        .current()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "OSL broker requires an active trusted host".to_owned())?;
    broker.validate_active_host(context_token, &active)?;
    Ok(active)
}

#[tauri::command]
async fn initialize_scrub_index(
    state: State<'_, ScrubIndexState>,
    registry: State<'_, ServiceRegistryState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    request: ScrubIndexInitializeRequest,
) -> Result<ScrubIndexStatus, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    for selection in &request.selections {
        if !selection_requires_registry_ownership(request.source, selection)? {
            continue;
        }
        let service = osl_privacy_hub::services::service_kind_from_id(&selection.service_id)
            .ok_or_else(|| "Scrub account selection is invalid".to_owned())?;
        registry.require_owned(&owner, service, &selection.account_id)?;
    }
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.initialize(&owner, request))
        .await
        .map_err(|_| "Scrub initialization was interrupted".to_owned())?
}

#[tauri::command]
async fn get_scrub_index_scan(
    state: State<'_, ScrubIndexState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    import_id: String,
) -> Result<LocalPrivacyScanResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.read_scan(&owner, &import_id))
        .await
        .map_err(|_| "Scrub review loading was interrupted".to_owned())?
}

#[tauri::command]
async fn append_scrub_index_chunk(
    state: State<'_, ScrubIndexState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    request: ScrubIndexChunkRequest,
) -> Result<ScrubIndexStatus, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.append_chunk(&owner, request))
        .await
        .map_err(|_| "Scrub indexing was interrupted".to_owned())?
}

#[tauri::command]
async fn get_scrub_index_status(
    state: State<'_, ScrubIndexState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Option<ScrubIndexStatus>, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.status(&owner))
        .await
        .map_err(|_| "Scrub status check was interrupted".to_owned())?
}

#[tauri::command]
async fn pause_scrub_index(
    state: State<'_, ScrubIndexState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    import_id: String,
) -> Result<ScrubIndexStatus, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.pause(&owner, &import_id))
        .await
        .map_err(|_| "Scrub pause was interrupted".to_owned())?
}

#[tauri::command]
async fn resume_scrub_index(
    state: State<'_, ScrubIndexState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    import_id: String,
) -> Result<ScrubIndexStatus, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.resume(&owner, &import_id))
        .await
        .map_err(|_| "Scrub resume was interrupted".to_owned())?
}

#[tauri::command]
async fn cancel_scrub_index(
    state: State<'_, ScrubIndexState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    import_id: String,
) -> Result<(), String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.cancel(&owner, &import_id))
        .await
        .map_err(|_| "Scrub cancellation was interrupted".to_owned())?
}

#[derive(Default)]
struct HubAccountSessionState {
    transition: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct MainWindowLifecycleState {
    close_started: AtomicBool,
}

impl MainWindowLifecycleState {
    fn begin_close(&self) -> bool {
        !self.close_started.swap(true, Ordering::AcqRel)
    }
}

#[cfg(target_os = "windows")]
fn main_window_is_live(window: &tauri::WebviewWindow) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

    let Ok(handle) = window.hwnd() else {
        return false;
    };
    let hwnd = handle.0 as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() || unsafe { IsWindow(hwnd) } == 0 {
        return false;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &mut process_id) };
    process_id == std::process::id()
}

#[cfg(not(target_os = "windows"))]
fn main_window_is_live(_window: &tauri::WebviewWindow) -> bool {
    true
}

#[tauri::command]
async fn list_linked_services(
    state: State<'_, ServiceRegistryState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<LinkedServiceDemo>, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    state.list_for_owner(&owner)
}

#[tauri::command]
fn get_core_readiness(state: State<'_, HubCoreState>) -> CoreReadiness {
    core_bridge::readiness(&state)
}

#[tauri::command]
fn list_core_features() -> Vec<CoreFeature> {
    core_bridge::feature_manifest()
}

#[tauri::command]
fn get_hub_license_state(state: State<'_, HubCoreState>) -> Result<HubLicenseState, String> {
    core_bridge::license_state(&state)
}

fn require_active_pro_entitlement(core: &HubCoreState) -> Result<(), String> {
    if ipc::tier_gate::is_paid_equivalent(&core.osl) {
        Ok(())
    } else {
        Err("Encrypted attachments require OSL Pro".to_owned())
    }
}

#[tauri::command]
fn get_mass_cleanup_capabilities(
    state: State<'_, HubCoreState>,
) -> Result<MassCleanupCapabilityManifest, String> {
    mass_cleanup::capability_manifest(&state.osl)
}

#[tauri::command]
fn discover_mass_cleanup_targets(
    state: State<'_, HubCoreState>,
    request: MassCleanupDiscoveryRequest,
) -> Result<(), String> {
    mass_cleanup::discover_targets(&state.osl, request)
}

#[tauri::command]
fn execute_mass_cleanup_batch(
    state: State<'_, HubCoreState>,
    request: MassCleanupExecutionRequest,
) -> Result<(), String> {
    mass_cleanup::execute_batch(&state.osl, request)
}

#[tauri::command]
async fn validate_hub_activation_code(
    app: tauri::AppHandle,
    activation_code: String,
) -> Result<HubLicenseState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        core_bridge::validate_activation_code(&app.state::<HubCoreState>(), activation_code)
    })
    .await
    .map_err(|_| "OSL activation worker failed".to_owned())?
}

#[tauri::command]
async fn clear_hub_activation_code(app: tauri::AppHandle) -> Result<HubLicenseState, String> {
    tauri::async_runtime::spawn_blocking(move || {
        core_bridge::clear_activation_code(&app.state::<HubCoreState>())
    })
    .await
    .map_err(|_| "OSL activation worker failed".to_owned())?
}

#[tauri::command]
async fn unlock_hub_password_gate(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    password: String,
) -> Result<HubGateUnlockResult, String> {
    let _session = session.transition.lock().await;
    let verify_app = app.clone();
    let verification = tauri::async_runtime::spawn_blocking(move || {
        startup_gate::verify_password_role(&verify_app.state::<HubCoreState>(), password)
    })
    .await
    .map_err(|_| "OSL password-gate worker failed".to_owned())??;

    match verification.role {
        VerifiedGateRole::Wrong => Ok(HubGateUnlockResult::wrong(verification)),
        VerifiedGateRole::Main => {
            let readiness = startup_gate::readiness_after_main(&app.state::<HubCoreState>());
            schedule_deferred_registration(&app);
            Ok(HubGateUnlockResult::unlocked(verification, readiness))
        }
        VerifiedGateRole::Stealth => {
            app.state::<ScrubImapState>().revoke_all();
            service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
            native_discord_overlay::clear_and_hide(&app);
            let _ = app.state::<NativeWindowHostState>().terminate();
            let _ = app.state::<MullvadWindowHostState>().restore();
            let _ = app.state::<BrowserCompanionState>().terminate();
            app.state::<HubBrokerState>().clear()?;
            startup_gate::enter_stealth_landing(&app.state::<HubCoreState>());
            Ok(HubGateUnlockResult::decoy(verification))
        }
        VerifiedGateRole::Burn => {
            app.state::<ScrubImapState>().revoke_all();
            service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
            native_discord_overlay::clear_and_hide(&app);
            let _ = app.state::<NativeWindowHostState>().terminate();
            let _ = app.state::<MullvadWindowHostState>().restore();
            let _ = app.state::<BrowserCompanionState>().terminate();
            app.state::<HubBrokerState>().clear()?;
            let config_dir = app
                .path()
                .app_config_dir()
                .map_err(|_| "OSL Privacy configuration storage is unavailable".to_owned())?;
            let local_data_dir = app
                .path()
                .app_local_data_dir()
                .map_err(|_| "OSL Privacy local storage is unavailable".to_owned())?;
            let burn_app = app.clone();
            let burn = tauri::async_runtime::spawn_blocking(move || {
                cleanup::execute_verified_gate_burn(
                    &burn_app.state::<HubCoreState>(),
                    &config_dir,
                    &local_data_dir,
                    true,
                )
            })
            .await
            .map_err(|_| "OSL burn worker failed".to_owned())??;
            Ok(HubGateUnlockResult::burned(verification, burn))
        }
    }
}

#[tauri::command]
async fn create_hub_osl_identity(app: tauri::AppHandle) -> Result<HubIdentitySetupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubCoreState>();
        password_lifecycle::create_native_identity(&state)
    })
    .await
    .map_err(|_| "OSL identity setup worker failed".to_string())?
}

#[tauri::command]
async fn import_hub_osl_identity_phrase(
    app: tauri::AppHandle,
    recovery_phrase: String,
) -> Result<HubIdentitySetupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubCoreState>();
        password_lifecycle::import_native_identity_phrase(&state, recovery_phrase)
    })
    .await
    .map_err(|_| "OSL identity import worker failed".to_string())?
}

#[tauri::command]
async fn setup_hub_main_password(
    app: tauri::AppHandle,
    password: String,
) -> Result<HubMainPasswordSetupResult, String> {
    let setup_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = setup_app.state::<HubCoreState>();
        password_lifecycle::setup_main_password(&state, password)
    })
    .await
    .map_err(|_| "OSL password setup worker failed".to_string())?;
    if result.is_ok() {
        schedule_deferred_registration(&app);
    }
    result
}

fn schedule_deferred_registration(app: &tauri::AppHandle) {
    if !app.state::<HubCoreState>().begin_deferred_registration() {
        return;
    }
    let registration_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let worker_app = registration_app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            worker_app
                .state::<HubCoreState>()
                .register_after_local_bootstrap();
        })
        .await;
        registration_app
            .state::<HubCoreState>()
            .settle_deferred_registration();
    });
}

#[tauri::command]
async fn get_hub_password_role_status(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<HubPasswordRoleStatus, String> {
    let _session = session.transition.lock().await;
    security_credentials::password_role_status(&core)
}

#[tauri::command]
async fn set_hub_stealth_password(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    current_main: String,
    new_stealth: String,
) -> Result<HubPasswordRoleStatus, String> {
    let _session = session.transition.lock().await;
    security_credentials::set_stealth_password(&core, current_main, new_stealth)
}

#[tauri::command]
async fn remove_hub_stealth_password(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    current_main: String,
) -> Result<HubPasswordRoleStatus, String> {
    let _session = session.transition.lock().await;
    security_credentials::remove_stealth_password(&core, current_main)
}

#[tauri::command]
async fn set_hub_burn_password(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    current_main: String,
    new_burn: String,
) -> Result<HubPasswordRoleStatus, String> {
    let _session = session.transition.lock().await;
    security_credentials::set_burn_password(&core, current_main, new_burn)
}

#[tauri::command]
async fn remove_hub_burn_password(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    current_main: String,
) -> Result<HubPasswordRoleStatus, String> {
    let _session = session.transition.lock().await;
    security_credentials::remove_burn_password(&core, current_main)
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum HubUpdateCheck {
    UpToDate {
        current: String,
    },
    UpdateAvailable {
        current: String,
        next: String,
        notes: String,
    },
    Error,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum HubUpdateInstall {
    NoUpdate,
}

#[derive(Default)]
struct HubUpdaterState {
    transition: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct HubNotificationState {
    enabled: Mutex<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HubAppNotification {
    id: String,
    title: String,
    detail: String,
    created_at: String,
}

#[tauri::command]
fn set_hub_notifications_enabled(
    state: State<'_, HubNotificationState>,
    enabled: bool,
) -> Result<(), String> {
    *state
        .enabled
        .lock()
        .map_err(|_| "OSL notification state is unavailable".to_owned())? = enabled;
    Ok(())
}

#[tauri::command]
async fn list_hub_app_notifications(
    core: State<'_, HubCoreState>,
    state: State<'_, HubNotificationState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<HubAppNotification>, String> {
    let _session = session.transition.lock().await;
    if !*state
        .enabled
        .lock()
        .map_err(|_| "OSL notification state is unavailable".to_owned())?
    {
        return Err("OSL notifications require explicit local opt-in".to_owned());
    }
    let people = security::list_people(&core)?;
    Ok(people
        .into_iter()
        .filter(|person| person.pending_key_change)
        .take(20)
        .map(|person| HubAppNotification {
            id: format!("key-change-{}", person.person_id),
            title: "Friend encryption key changed".to_owned(),
            detail:
                "Verify the new safety number outside this chat before allowing encrypted messages."
                    .to_owned(),
            created_at: "Pending verification".to_owned(),
        })
        .collect())
}

#[tauri::command]
async fn check_hub_for_updates(
    app: tauri::AppHandle,
    state: State<'_, HubUpdaterState>,
) -> Result<HubUpdateCheck, String> {
    let _transition = state.transition.lock().await;
    let current = app.package_info().version.to_string();
    let Ok(updater) = app.updater() else {
        return Ok(HubUpdateCheck::Error);
    };
    match updater.check().await {
        Ok(Some(update)) => {
            let Some(next) = bounded_version(&update.version) else {
                return Ok(HubUpdateCheck::Error);
            };
            Ok(HubUpdateCheck::UpdateAvailable {
                current,
                next,
                notes: bounded_plain_notes(update.body.as_deref()),
            })
        }
        Ok(None) => Ok(HubUpdateCheck::UpToDate { current }),
        Err(_) => Ok(HubUpdateCheck::Error),
    }
}

#[tauri::command]
async fn install_hub_update(
    app: tauri::AppHandle,
    state: State<'_, HubUpdaterState>,
    expected_version: String,
) -> Result<HubUpdateInstall, String> {
    let expected_version = bounded_version(&expected_version)
        .ok_or_else(|| "The expected update version is invalid".to_owned())?;
    let _transition = state.transition.lock().await;
    let updater = app
        .updater()
        .map_err(|_| "The signed OSL updater is unavailable".to_owned())?;
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Ok(HubUpdateInstall::NoUpdate),
        Err(_) => {
            return Err("The signed OSL update check failed; nothing was installed".to_owned())
        }
    };
    if update.version != expected_version {
        return Err("The available update changed; check again before installing".to_owned());
    }
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|_| "The update could not be verified and was not installed".to_owned())?;
    app.restart();
}

#[tauri::command]
fn open_hub_releases_page() -> Result<(), String> {
    open_fixed_system_url(
        RELEASES_URL,
        "The fixed OSL releases page could not be opened",
    )
}

#[tauri::command]
fn open_hub_source_repository() -> Result<(), String> {
    open_fixed_system_url(
        SOURCE_REPOSITORY_URL,
        "The fixed OSL source repository could not be opened",
    )
}

fn open_fixed_system_url(url: &'static str, failure: &'static str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", url]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(url);
        command
    };
    command.spawn().map(|_| ()).map_err(|_| failure.to_owned())
}

#[tauri::command]
fn open_external_link_in_default_browser(url: String) -> Result<(), String> {
    if url.len() > 2_048 {
        return Err("The external link is too long".to_owned());
    }
    let parsed = url::Url::parse(&url).map_err(|_| "The external link is invalid".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("Only credential-free HTTP links can open outside OSL".to_owned());
    }

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", parsed.as_str()]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(parsed.as_str());
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(parsed.as_str());
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "The operating-system browser could not open the link".to_owned())
}

#[tauri::command]
async fn lock_hub_session(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
) -> Result<(), String> {
    let _session = session.transition.lock().await;
    app.state::<ScrubImapState>().revoke_all();
    service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
    native_discord_overlay::clear_and_hide(&app);
    let _ = app.state::<NativeWindowHostState>().terminate();
    let _ = app.state::<MullvadWindowHostState>().restore();
    let _ = app.state::<BrowserCompanionState>().terminate();
    app.state::<HubBrokerState>().clear()?;
    startup_gate::lock_session(&app.state::<HubCoreState>());
    Ok(())
}

#[tauri::command]
fn schedule_protected_clipboard_clear(timeout_seconds: u64) -> Result<(), String> {
    if !(5..=300).contains(&timeout_seconds) {
        return Err("The clipboard clear timeout is invalid".to_owned());
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;
        let expected_sequence = unsafe { GetClipboardSequenceNumber() };
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(timeout_seconds));
            // Never erase content copied after OSL's protected message.
            if unsafe { GetClipboardSequenceNumber() } != expected_sequence {
                return;
            }
            use std::ptr;
            use windows_sys::Win32::System::DataExchange::{
                CloseClipboard, EmptyClipboard, OpenClipboard,
            };
            if unsafe { OpenClipboard(ptr::null_mut()) } == 0 {
                return;
            }
            unsafe {
                EmptyClipboard();
                CloseClipboard();
            }
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = timeout_seconds;
        Err("Timed clipboard clearing is available in the Windows app".to_owned())
    }
}

#[tauri::command]
fn list_native_apps() -> Vec<NativeAppStatus> {
    native_apps::list_native_apps()
}

#[tauri::command]
fn install_native_app(app_id: NativeAppId) -> Result<NativeInstallResult, String> {
    native_apps::install_native_app(app_id)
}

#[tauri::command]
fn get_mullvad_status() -> MullvadStatus {
    native_apps::get_mullvad_status()
}

#[tauri::command]
fn install_mullvad() -> Result<MullvadActionResult, String> {
    native_apps::install_mullvad()
}

#[tauri::command]
fn open_mullvad() -> Result<MullvadActionResult, String> {
    native_apps::open_mullvad()
}

#[tauri::command]
fn list_browser_imports() -> Vec<BrowserImportStatus> {
    native_apps::list_browser_imports()
}

#[tauri::command]
fn open_browser_import(browser_id: BrowserImportId) -> Result<BrowserImportResult, String> {
    native_apps::open_browser_import(browser_id)
}

#[tauri::command]
fn get_firefox_status() -> FirefoxStatus {
    native_apps::get_firefox_status()
}

#[tauri::command]
fn install_firefox() -> Result<FirefoxInstallResult, String> {
    native_apps::install_firefox()
}

#[tauri::command]
async fn begin_browser_account_import(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<BrowserAccountImportResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let app_local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL Firefox profile directory is unavailable".to_owned())?;
    let operation_id = crypto::random::random_bytes(16)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    native_apps::begin_browser_account_import(&app_local_data_dir, &owner, &operation_id)
}

#[tauri::command]
async fn begin_protected_browser_import(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    browser_ids: Vec<BrowserImportId>,
    operation_id: String,
) -> Result<ProtectedBrowserImportResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let owner_window = main_window_hwnd(&app)?;
    let app_local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL Firefox profile directory is unavailable".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        native_apps::begin_protected_browser_import(
            &app_local_data_dir,
            &owner,
            &operation_id,
            browser_ids,
            owner_window,
        )
    })
    .await
    .map_err(|_| "The protected browser import worker stopped unexpectedly".to_owned())?
}

#[tauri::command]
async fn finish_protected_browser_import(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    operation_id: String,
) -> Result<(), String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    native_apps::finish_protected_browser_import(&owner, &operation_id)
}

#[tauri::command]
async fn cancel_protected_browser_import(
    core: State<'_, HubCoreState>,
    operation_id: String,
) -> Result<(), String> {
    // Cancellation deliberately does not wait for the account-transition
    // mutex: begin holds it for the bounded worker lifetime, and the exact
    // owner + operation capability is what authorizes an in-flight cancel.
    let owner = active_unlocked_osl_user_id(&core)?;
    native_apps::cancel_protected_browser_import(&owner, &operation_id)
}

#[tauri::command]
async fn launch_firefox_service(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: FirefoxServiceId,
) -> Result<FirefoxLaunchResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let app_local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL Firefox profile directory is unavailable".to_owned())?;
    native_apps::launch_firefox_service(&app_local_data_dir, &owner, service_id)
}

#[tauri::command]
async fn get_default_browser_companion_status(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    companion: State<'_, BrowserCompanionState>,
) -> Result<BrowserCompanionStatus, String> {
    let _session = session.transition.lock().await;
    let _owner = active_unlocked_osl_user_id(&core)?;
    Ok(companion.status())
}

#[tauri::command]
async fn host_default_browser_companion(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: FirefoxServiceId,
    browser_id: Option<BrowserImportId>,
    account_mode: BrowserAccountMode,
) -> Result<BrowserCompanionAction, String> {
    let owner = {
        let _session = session.transition.lock().await;
        active_unlocked_osl_user_id(&core)?
    };
    let app_local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL browser profile directory is unavailable".to_owned())?;
    let parent = main_window_hwnd(&app)?;
    let operation_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        operation_app.state::<BrowserCompanionState>().host(
            service_id,
            browser_id,
            account_mode,
            &app_local_data_dir,
            &owner,
            parent,
        )
    })
    .await
    .map_err(|_| "The default-browser companion operation was interrupted".to_owned())
}

#[tauri::command]
fn resize_default_browser_companion(
    app: tauri::AppHandle,
) -> Result<BrowserCompanionAction, String> {
    let parent = main_window_hwnd(&app)?;
    Ok(app.state::<BrowserCompanionState>().resize(parent))
}

#[tauri::command]
fn focus_default_browser_companion(app: tauri::AppHandle) -> BrowserCompanionAction {
    app.state::<BrowserCompanionState>().focus()
}

#[tauri::command]
fn detach_default_browser_companion(app: tauri::AppHandle) -> BrowserCompanionAction {
    app.state::<BrowserCompanionState>().detach()
}

#[tauri::command]
async fn launch_system_browser_service(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: FirefoxServiceId,
) -> Result<FirefoxLaunchResult, String> {
    let _session = session.transition.lock().await;
    let _owner = active_unlocked_osl_user_id(&core)?;
    native_apps::launch_system_browser_service(service_id)
}

#[cfg(target_os = "windows")]
fn main_window_hwnd(app: &tauri::AppHandle) -> Result<isize, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "The trusted OSL Privacy window is unavailable".to_owned())?
        .hwnd()
        .map(|handle| handle.0 as isize)
        .map_err(|_| "The trusted OSL Privacy window handle is unavailable".to_owned())
}

#[cfg(not(target_os = "windows"))]
fn main_window_hwnd(_app: &tauri::AppHandle) -> Result<isize, String> {
    Ok(0)
}

#[tauri::command]
async fn host_native_app_window(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    app_id: NativeAppId,
    discord_session_mode: DiscordSessionMode,
) -> Result<NativeWindowHostResult, String> {
    native_discord_overlay::clear_and_hide(&app);
    if discord_session_mode == DiscordSessionMode::ExistingSession
        && !matches!(
            app_id,
            NativeAppId::Discord
                | NativeAppId::Telegram
                | NativeAppId::Signal
                | NativeAppId::Whatsapp
                | NativeAppId::Outlook
        )
    {
        return Err("An existing native session is not supported for this app".to_owned());
    }
    let owner = {
        let _session = session.transition.lock().await;
        active_unlocked_osl_user_id(&core)?
    };
    let parent = main_window_hwnd(&app)?;
    let profile_root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL-owned native profile directory is unavailable".to_owned())?;
    let operation_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = operation_app.state::<NativeWindowHostState>();
        let mut result =
            state.host_mode(app_id, &profile_root, &owner, parent, discord_session_mode);
        if app_id == NativeAppId::Discord
            && discord_session_mode == DiscordSessionMode::Dedicated
            && matches!(
                result.reason,
                NativeWindowHostReason::ChannelNotOwned
                    | NativeWindowHostReason::NoChannelAvailable
                    | NativeWindowHostReason::AppNotInstalled
            )
            && native_apps::install_discord_dedicated_channel().is_ok()
        {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
            while std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_secs(1));
                result =
                    state.host_mode(app_id, &profile_root, &owner, parent, discord_session_mode);
                if !matches!(
                    result.reason,
                    NativeWindowHostReason::ChannelNotOwned
                        | NativeWindowHostReason::NoChannelAvailable
                        | NativeWindowHostReason::AppNotInstalled
                        | NativeWindowHostReason::WindowNotFound
                        | NativeWindowHostReason::ProfileInitializationFailed
                ) {
                    break;
                }
            }
        }
        result
    })
    .await
    .map_err(|_| "The experimental native host operation was interrupted".to_owned())
}

#[tauri::command]
fn resize_native_app_window(app: tauri::AppHandle) -> Result<NativeWindowHostResult, String> {
    let parent = main_window_hwnd(&app)?;
    Ok(app.state::<NativeWindowHostState>().resize(parent))
}

#[tauri::command]
fn focus_native_app_window(app: tauri::AppHandle) -> NativeWindowHostResult {
    app.state::<NativeWindowHostState>().focus()
}

#[tauri::command]
fn detach_native_app_window(app: tauri::AppHandle) -> NativeWindowHostResult {
    native_discord_overlay::clear_and_hide(&app);
    app.state::<NativeDiscordComposerState>().clear();
    app.state::<NativeWindowHostState>().detach()
}

fn native_discord_scope_binding(app: &tauri::AppHandle) -> Result<String, String> {
    let broker = app.state::<HubBrokerState>();
    app.state::<OverlaySessionState>()
        .with_bootstrap_context(|context_token, host| {
            broker.validate_active_host(context_token, host)?;
            let target = broker
                .manual_burn_target(context_token)?
                .ok_or_else(|| "The native Discord friend context is unavailable".to_owned())?;
            serde_json::to_string(&(
                target.service_id,
                target.account_id,
                target.person_id,
                target.scope,
            ))
            .map_err(|_| "The native Discord friend context is unavailable".to_owned())
        })
}

#[tauri::command]
async fn set_native_discord_protected_overlay_open(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    context_token: String,
    open: bool,
) -> Result<bool, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL Privacy window may control the overlay".to_owned());
    }
    if !open {
        native_discord_overlay::clear_and_hide(&app);
        app.state::<NativeDiscordComposerState>().clear();
        return Ok(true);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let broker_state = app.state::<HubBrokerState>();
        let owner = active_unlocked_osl_user_id(&core)?;
        let current = require_current_context_host(&app, &core, &broker_state, &context_token)?;
        let native = app
            .state::<NativeWindowHostState>()
            .current_discord_service_host(&owner)?;
        if current != native || current.service_id != "discord" {
            return Err("The protected context is not the current native Discord host".to_owned());
        }
        let target = app
            .state::<NativeWindowHostState>()
            .discord_overlay_target(&owner)?;
        if target.generation != current.generation {
            return Err("The native Discord window changed before protection opened".to_owned());
        }
        let overlay_state = app.state::<OverlaySessionState>();
        let epoch = overlay_state.activate(context_token, current)?;
        let scope_binding = native_discord_scope_binding(&app)?;
        if app
            .state::<NativeDiscordComposerState>()
            .calibrate(
                &app.state::<NativeWindowHostState>(),
                &owner,
                &scope_binding,
            )
            .is_err()
        {
            // Discord may expose no bounded, trustworthy accessibility composer.
            // Keep the verified OSL friend relay usable, but make marker placement
            // unavailable. The carrier command remains independently fail-closed.
            app.state::<NativeDiscordComposerState>().clear();
        }
        native_discord_overlay::show(&app, target.rect, epoch)
            .map(|()| true)
            .map_err(|error| {
                native_discord_overlay::clear_and_hide(&app);
                error
            })
    })
    .await
    .map_err(|_| "The native Discord overlay operation was interrupted".to_owned())?
}

#[tauri::command]
fn send_native_discord_overlay_carrier(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    mode: DiscordCarrierMode,
    chars_per_second: u16,
    layout: Option<DiscordCarrierLayout>,
) -> Result<DiscordCarrierReceipt, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may send its carrier".to_owned());
    }
    let paid = ipc::tier_gate::is_paid_equivalent(&app.state::<HubCoreState>().osl);
    if mode == DiscordCarrierMode::Compatibility && !paid {
        return Err("Compatibility typing requires OSL Pro".to_owned());
    }
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    let (epoch, host) = require_overlay_context_snapshot(&app)?;
    let scope_binding = native_discord_scope_binding(&app)?;
    require_same_overlay_context(&app, epoch, &host)?;
    let composer = app.state::<NativeDiscordComposerState>();
    let plan = composer.take_prepared_carrier_plan(&scope_binding, layout);
    let carrier = plan
        .cover_text()
        .unwrap_or_else(|| LocalCoverState::free_cover().to_owned());
    let receipt = composer.place_carrier(
        &app.state::<NativeWindowHostState>(),
        &owner,
        &scope_binding,
        mode,
        chars_per_second,
        &carrier,
    );
    require_same_overlay_context(&app, epoch, &host)?;
    if let Some(window) = app.get_webview_window(native_discord_overlay::OVERLAY_LABEL) {
        let _ = window.set_focus();
    }
    Ok(receipt)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeDiscordOverlayStateDto {
    active: bool,
    friend_label: String,
    scope_approved: bool,
    ttl_seconds: u32,
    decrypt_display_enabled: bool,
    view_once_enabled: bool,
    attachments_enabled: bool,
    discord_marker_available: bool,
    covertext_enabled: bool,
}

#[tauri::command]
fn set_native_discord_covertext_enabled(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    enabled: bool,
) -> Result<bool, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL header may change Covertext".to_owned());
    }
    let state = app.state::<OverlaySessionState>();
    state.set_covertext_enabled(enabled);
    Ok(state.covertext_enabled())
}

#[tauri::command]
fn get_native_discord_overlay_state(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
) -> Result<NativeDiscordOverlayStateDto, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may read this state".to_owned());
    }
    native_discord_overlay_state(&app)
}

fn native_discord_overlay_state(
    app: &tauri::AppHandle,
) -> Result<NativeDiscordOverlayStateDto, String> {
    let core = app.state::<HubCoreState>();
    let broker = app.state::<HubBrokerState>();
    let state = app.state::<OverlaySessionState>();
    let owner = active_unlocked_osl_user_id(&core)?;
    let (context_epoch, stored_host) = state
        .validated_marker(|context_token, host| broker.validate_active_host(context_token, host))?;
    let current_host = app
        .state::<NativeWindowHostState>()
        .current_discord_service_host(&owner)?;
    if current_host != stored_host {
        return Err("The native Discord overlay context changed".to_owned());
    }
    let target = state.with_context(|context_token, host| {
        broker.validate_active_host(context_token, host)?;
        broker
            .manual_burn_target(context_token)?
            .ok_or_else(|| "The native Discord friend context is unavailable".to_owned())
    })?;
    state.validate_marker(context_epoch, &stored_host, |context_token, host| {
        broker.validate_active_host(context_token, host)
    })?;
    let scope = security::scope_security(target.scope.clone())?;
    let scope_approved = security::manual_peer_scope_approved(
        &core,
        &target.service_id,
        &target.account_id,
        target.person_id.clone(),
        target.scope,
    )?;
    let friend_label = security::list_people(&core)?
        .into_iter()
        .find(|person| person.person_id == target.person_id)
        .and_then(|person| person.alias)
        .unwrap_or_else(|| "Friend".to_owned());
    require_same_overlay_context(app, context_epoch, &stored_host)?;
    Ok(NativeDiscordOverlayStateDto {
        active: true,
        friend_label,
        scope_approved,
        ttl_seconds: scope.ttl_seconds,
        decrypt_display_enabled: scope.decrypt_display_enabled,
        view_once_enabled: true,
        attachments_enabled: ipc::tier_gate::is_paid_equivalent(&core.osl),
        discord_marker_available: app.state::<NativeDiscordComposerState>().marker_available(),
        covertext_enabled: state.covertext_enabled(),
    })
}

fn require_overlay_context_snapshot(
    app: &tauri::AppHandle,
) -> Result<(u64, ActiveServiceHost), String> {
    let broker = app.state::<HubBrokerState>();
    let marker = app
        .state::<OverlaySessionState>()
        .validated_marker(|context_token, host| broker.validate_active_host(context_token, host))?;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    let current = app
        .state::<NativeWindowHostState>()
        .current_discord_service_host(&owner)?;
    if current != marker.1 {
        return Err("The native Discord overlay context changed".to_owned());
    }
    Ok(marker)
}

fn require_same_overlay_context(
    app: &tauri::AppHandle,
    expected_epoch: u64,
    expected_host: &ActiveServiceHost,
) -> Result<(), String> {
    app.state::<OverlaySessionState>().validate_marker(
        expected_epoch,
        expected_host,
        |context_token, host| {
            app.state::<HubBrokerState>()
                .validate_active_host(context_token, host)
        },
    )?;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    let current = app
        .state::<NativeWindowHostState>()
        .current_discord_service_host(&owner)?;
    if &current != expected_host {
        return Err("The native Discord overlay context changed".to_owned());
    }
    Ok(())
}

#[tauri::command]
async fn set_native_discord_overlay_security(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    ttl_seconds: u32,
    decrypt_display_enabled: bool,
) -> Result<NativeDiscordOverlayStateDto, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may change protection".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let broker = app.state::<HubBrokerState>();
        app.state::<OverlaySessionState>()
            .with_context(|context_token, stored_host| {
                if stored_host != &host {
                    return Err("The native Discord overlay context changed".to_owned());
                }
                broker.validate_active_host(context_token, stored_host)?;
                let scope = broker.scope_for_context(context_token)?;
                with_indexed_context_write(&app, &broker, context_token, || {
                    security::set_scope_security(
                        &app.state::<HubSecurityState>(),
                        scope,
                        ttl_seconds,
                        decrypt_display_enabled,
                    )
                })?;
                Ok(())
            })?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        native_discord_overlay_state(&app)
    })
    .await
    .map_err(|error| format!("OSL native overlay worker failed: {error}"))?
}

#[tauri::command]
async fn prepare_native_discord_overlay_text(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedNativeOverlayText, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may protect text".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let scope_binding = native_discord_scope_binding(&app)?;
        let visual = deidentify_prepared_visual_structure(&plaintext);
        let prepared = broker::prepare_native_discord_overlay_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            plaintext,
            view_once,
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        app.state::<NativeDiscordComposerState>()
            .remember_prepared_visual_structure(&scope_binding, visual);
        Ok(prepared)
    })
    .await
    .map_err(|error| format!("OSL native overlay worker failed: {error}"))?
}

#[tauri::command]
async fn open_native_discord_overlay_text(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<OpenedNativeOverlayTextBatch, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may receive text".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let opened = broker::drain_native_discord_overlay_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        Ok(opened)
    })
    .await
    .map_err(|error| format!("OSL native overlay worker failed: {error}"))?
}

#[tauri::command]
async fn reveal_native_discord_overlay_view_once(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    message_id: String,
) -> Result<broker::OpenedNativeOverlayText, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may reveal view-once text".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let opened = broker::reveal_native_discord_overlay_view_once(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            &message_id,
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        Ok(opened)
    })
    .await
    .map_err(|error| format!("OSL native overlay worker failed: {error}"))?
}

#[tauri::command]
async fn prepare_osl_chat_text(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedNativeOverlayText, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may send OSL Chats".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        broker::prepare_osl_chat_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            plaintext,
            view_once,
        )
    })
    .await
    .map_err(|error| format!("OSL Chat worker failed: {error}"))?
}

#[tauri::command]
async fn open_osl_chat_text(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<OpenedNativeOverlayTextBatch, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may receive OSL Chats".to_owned());
    }
    screenshot::apply_to_window(&caller, runtime::ScreenshotProtection::On)
        .map_err(|_| "Windows capture resistance is required to receive OSL Chats".to_owned())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        broker::drain_osl_chat_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            true,
        )
    })
    .await
    .map_err(|error| format!("OSL Chat worker failed: {error}"))?
}

#[tauri::command]
async fn list_osl_chat_history(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<ipc::commands::StoredMessageDto>, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may read OSL Chat history".to_owned());
    }
    screenshot::apply_to_window(&caller, runtime::ScreenshotProtection::On).map_err(|_| {
        "Windows capture resistance is required to read OSL Chat history".to_owned()
    })?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        broker::load_osl_chat_history(&app.state::<HubCoreState>(), &app.state::<HubBrokerState>())
    })
    .await
    .map_err(|error| format!("OSL Chat history worker failed: {error}"))?
}

#[tauri::command]
async fn select_osl_chat_attachment(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    view_once: bool,
) -> Result<Option<broker::PreparedNativeOverlayAttachment>, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may choose OSL Chat attachments".to_owned());
    }
    require_active_pro_entitlement(&app.state::<HubCoreState>())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        native_attachment_transport::select_osl_chat_attachment(
            &app,
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            view_once,
        )
    })
    .await
    .map_err(|error| format!("OSL Chat attachment worker failed: {error}"))?
}

#[tauri::command]
async fn list_osl_chat_attachments(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<broker::PendingNativeOverlayAttachment>, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may list OSL Chat attachments".to_owned());
    }
    require_active_pro_entitlement(&app.state::<HubCoreState>())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        native_attachment_transport::list_osl_chat_pending(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
        )
    })
    .await
    .map_err(|error| format!("OSL Chat attachment worker failed: {error}"))?
}

#[tauri::command]
async fn open_osl_chat_attachment(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    attachment_id: String,
) -> Result<native_attachment_transport::OpenedNativeOverlayAttachment, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may open OSL Chat attachments".to_owned());
    }
    screenshot::apply_to_window(&caller, runtime::ScreenshotProtection::On).map_err(|_| {
        "Windows capture resistance is required to open OSL Chat attachments".to_owned()
    })?;
    require_active_pro_entitlement(&app.state::<HubCoreState>())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        native_attachment_transport::open_osl_chat_pending(
            &app,
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            &attachment_id,
        )
    })
    .await
    .map_err(|error| format!("OSL Chat attachment worker failed: {error}"))?
}

#[tauri::command]
async fn select_native_discord_overlay_attachment(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    view_once: bool,
) -> Result<Option<broker::PreparedNativeOverlayAttachment>, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may choose attachments".to_owned());
    }
    require_active_pro_entitlement(&app.state::<HubCoreState>())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let prepared = native_attachment_transport::select_encrypt_upload_deliver(
            &app,
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            context_epoch,
            &host,
            view_once,
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        Ok(prepared)
    })
    .await
    .map_err(|error| format!("OSL native attachment worker failed: {error}"))?
}

#[tauri::command]
async fn list_native_discord_overlay_attachments(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<broker::PendingNativeOverlayAttachment>, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may list attachments".to_owned());
    }
    require_active_pro_entitlement(&app.state::<HubCoreState>())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let pending = native_attachment_transport::list_pending(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        Ok(pending)
    })
    .await
    .map_err(|error| format!("OSL native attachment worker failed: {error}"))?
}

#[tauri::command]
async fn open_native_discord_overlay_attachment(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    attachment_id: String,
) -> Result<native_attachment_transport::OpenedNativeOverlayAttachment, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may open attachments".to_owned());
    }
    require_active_pro_entitlement(&app.state::<HubCoreState>())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let opened = native_attachment_transport::open_pending(
            &app,
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            context_epoch,
            &host,
            &attachment_id,
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        Ok(opened)
    })
    .await
    .map_err(|error| format!("OSL native attachment worker failed: {error}"))?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeDiscordOverlayBurnResult {
    rows_destroyed: usize,
    channels_destroyed: usize,
    whitelist_entries_removed: usize,
    local_protected_rows_destroyed: usize,
    remote_blobs_deleted: usize,
    remote_blob_deletions_failed: usize,
    local_cleanup_complete: bool,
    remote_cleanup_complete: bool,
    discord_history_deleted: bool,
    recipient_copies_deleted: bool,
}

#[tauri::command]
async fn burn_native_discord_overlay_chat(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<NativeDiscordOverlayBurnResult, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may burn this OSL chat".to_owned());
    }
    let _session = session.transition.lock().await;
    let cover_scope = native_discord_scope_binding(&app)?;
    let burn_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&burn_app)?;
        let broker_state = burn_app.state::<HubBrokerState>();
        let core = burn_app.state::<HubCoreState>();
        let result = burn_app.state::<OverlaySessionState>().with_context(
            |context_token, stored_host| {
                if stored_host != &host {
                    return Err("The native Discord overlay context changed".to_owned());
                }
                broker_state.validate_active_host(context_token, stored_host)?;
                let manual = broker_state
                    .manual_burn_target(context_token)?
                    .ok_or_else(|| "The native Discord friend context is unavailable".to_owned())?;
                let scope_result = security::burn_manual_peer_scope(
                    &core,
                    &burn_app.state::<HubSecurityState>(),
                    &manual.service_id,
                    &manual.account_id,
                    &manual.person_id,
                    manual.scope,
                )?;
                let (local_protected_rows_destroyed, local_ledger_complete) =
                    match broker::burn_local_protected_context(&core, &broker_state, context_token)
                    {
                        Ok(rows) => (rows, true),
                        Err(_) => (0, false),
                    };
                Ok(NativeDiscordOverlayBurnResult {
                    rows_destroyed: scope_result.rows_destroyed,
                    channels_destroyed: scope_result.channels_destroyed,
                    whitelist_entries_removed: scope_result.whitelist_entries_removed,
                    local_protected_rows_destroyed,
                    remote_blobs_deleted: scope_result.remote_blobs_deleted,
                    remote_blob_deletions_failed: scope_result.remote_blob_deletions_failed,
                    local_cleanup_complete: scope_result.local_cleanup_complete
                        && local_ledger_complete,
                    remote_cleanup_complete: scope_result.remote_cleanup_complete,
                    // OSL burn never touches the native Discord profile/history
                    // and cannot revoke copies already received by another user.
                    discord_history_deleted: false,
                    recipient_copies_deleted: false,
                })
            },
        )?;
        // Recheck the Rust-held epoch/host after every destructive operation,
        // then revoke the broker lease regardless. The exact old scope has
        // already been burned, so a concurrent host change must not suppress
        // its truthful counts or leave the overlay capable of retrying it.
        let _ = require_same_overlay_context(&burn_app, context_epoch, &host);
        let _ = broker_state.clear();
        Ok::<NativeDiscordOverlayBurnResult, String>(result)
    })
    .await
    .map_err(|error| format!("OSL native overlay burn worker failed: {error}"))??;

    app.state::<LocalCoverState>().burn_scope(&cover_scope);
    app.state::<NativeDiscordComposerState>().clear();
    let close_app = app.clone();
    std::thread::spawn(move || {
        // Leave enough time for Tauri to deliver the truthful result DTO to
        // the invoking overlay before that webview is closed. Its broker and
        // composer authority were already revoked synchronously above.
        std::thread::sleep(std::time::Duration::from_millis(250));
        native_discord_overlay::clear_and_hide(&close_app);
    });
    Ok(result)
}

/// With the user's explicit consent, visually borrow the one existing Mullvad
/// window from this Windows logon session. The native boundary accepts no PID,
/// HWND, executable path, account value, or launch argument from the renderer.
#[tauri::command]
async fn host_mullvad_window(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<MullvadWindowHostResult, String> {
    {
        let _session = session.transition.lock().await;
        let _ = active_unlocked_osl_user_id(&core)?;
    }
    let parent = main_window_hwnd(&app)?;
    let operation_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        operation_app.state::<MullvadWindowHostState>().host(parent)
    })
    .await
    .map_err(|_| "The Mullvad window operation was interrupted".to_owned())
}

#[tauri::command]
fn resize_mullvad_window(app: tauri::AppHandle) -> Result<MullvadWindowHostResult, String> {
    let parent = main_window_hwnd(&app)?;
    Ok(app.state::<MullvadWindowHostState>().resize(parent))
}

#[tauri::command]
fn focus_mullvad_window(app: tauri::AppHandle) -> MullvadWindowHostResult {
    app.state::<MullvadWindowHostState>().focus()
}

#[tauri::command]
fn restore_mullvad_window(app: tauri::AppHandle) -> MullvadWindowHostResult {
    app.state::<MullvadWindowHostState>().restore()
}

fn with_indexed_context_write<T>(
    app: &tauri::AppHandle,
    broker_state: &HubBrokerState,
    context_token: &str,
    write: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let registration = broker_state.service_scope_registration(context_token)?;
    app.state::<ServiceScopeIndexState>()
        .with_registered_write(registration, write)
}

#[tauri::command]
async fn create_service_account(
    core: State<'_, HubCoreState>,
    registry: State<'_, ServiceRegistryState>,
    index: State<'_, ServiceScopeIndexState>,
    session: State<'_, HubAccountSessionState>,
    service_id: ServiceKind,
    label: String,
    provider: Option<EmailProvider>,
) -> Result<LinkedAccountDemo, String> {
    let _session = session.transition.lock().await;
    if matches!(
        service_id,
        ServiceKind::Discord | ServiceKind::Telegram | ServiceKind::Signal | ServiceKind::WhatsApp
    ) {
        return Err("This service requires its dedicated native app".to_owned());
    }
    let owner = active_unlocked_osl_user_id(&core)?;
    let account = registry.create_with_provider_for_owner(&owner, service_id, label, provider)?;
    let service = service_kind_id(service_id);
    if let Err(error) = index.initialize_clean_account(&owner, service, &account.id) {
        let _ = registry.remove_for_owner(&owner, service_id, &account.id);
        return Err(error);
    }
    Ok(account)
}

#[tauri::command]
async fn open_service_host(
    app: tauri::AppHandle,
    host: State<'_, ServiceHostState>,
    registry: State<'_, ServiceRegistryState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
) -> Result<service_host::ActiveServiceHost, String> {
    let _session = session.transition.lock().await;
    if matches!(
        service_id.as_str(),
        "discord" | "telegram" | "signal" | "whatsapp"
    ) {
        return Err("This service requires its dedicated native app".to_owned());
    }
    let owner = active_unlocked_osl_user_id(&core)?;
    service_host::desktop::open(app, host, registry, owner, service_id, account_id).await
}

#[tauri::command]
async fn close_service_host(
    app: tauri::AppHandle,
    host: State<'_, ServiceHostState>,
    broker: State<'_, HubBrokerState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<(), String> {
    let _session = session.transition.lock().await;
    native_discord_overlay::clear_and_hide(&app);
    broker.clear()?;
    service_host::desktop::close(app, host).await
}

#[tauri::command]
async fn set_local_protected_sheet_open(
    app: tauri::AppHandle,
    host: State<'_, ServiceHostState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    open: bool,
) -> Result<bool, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let current = host
        .current()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The service view is not active".to_owned())?;
    let expected = host
        .require_current_owned(&owner, &current.service_id, &current.account_id)
        .map_err(|error| error.to_string())?;
    service_host::desktop::set_local_protected_sheet_open(app, host, expected, open).await
}

#[allow(clippy::too_many_arguments)]
async fn mutate_service_account(
    app: tauri::AppHandle,
    host: State<'_, ServiceHostState>,
    registry: State<'_, ServiceRegistryState>,
    broker: State<'_, HubBrokerState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
    remove_registry: bool,
) -> Result<service_host::ServiceAccountMutation, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    // Any active context may be bound to the profile being removed. Clearing
    // every lease is cheap and prevents stale composer authority surviving a
    // profile reset or removal.
    native_discord_overlay::clear_and_hide(&app);
    broker.clear()?;
    service_host::desktop::mutate_account_profile(
        app,
        host,
        registry,
        owner,
        service_id,
        account_id,
        remove_registry,
    )
    .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn remove_service_account(
    app: tauri::AppHandle,
    host: State<'_, ServiceHostState>,
    registry: State<'_, ServiceRegistryState>,
    broker: State<'_, HubBrokerState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
) -> Result<service_host::ServiceAccountMutation, String> {
    mutate_service_account(
        app, host, registry, broker, core, session, service_id, account_id, true,
    )
    .await
}

fn service_kind_id(kind: ServiceKind) -> &'static str {
    match kind {
        ServiceKind::Discord => "discord",
        ServiceKind::Telegram => "telegram",
        ServiceKind::WhatsApp => "whatsapp",
        ServiceKind::Instagram => "instagram",
        ServiceKind::Messenger => "messenger",
        ServiceKind::Snapchat => "snapchat",
        ServiceKind::X => "x",
        ServiceKind::Email => "email",
        ServiceKind::Signal => "signal",
        ServiceKind::Slack => "slack",
        ServiceKind::Linkedin => "linkedin",
        ServiceKind::Teams => "teams",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalLoopbackContextLease {
    context_token: String,
    service_id: String,
    account_id: String,
    conversation_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManualPeerContextLease {
    context_token: String,
    service_id: String,
    account_id: String,
    person_id: String,
    peer_osl_user_id: String,
    scope_approved: bool,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn activate_local_loopback_context(
    app: tauri::AppHandle,
    broker: State<'_, HubBrokerState>,
    host: State<'_, ServiceHostState>,
    registry: State<'_, ServiceRegistryState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
    conversation_id: String,
) -> Result<LocalLoopbackContextLease, String> {
    let _session = session.transition.lock().await;
    native_discord_overlay::clear_and_hide(&app);
    let owner = active_unlocked_osl_user_id(&core)?;
    let lease = broker::activate_owned_local_loopback_context(
        &broker,
        &registry,
        &host,
        &owner,
        &service_id,
        &account_id,
        conversation_id.clone(),
    )?;
    Ok(LocalLoopbackContextLease {
        context_token: lease.context_token,
        service_id: lease.service_id,
        account_id: lease.account_id,
        conversation_id,
    })
}

/// Activate only a renderer-selected existing friend. Recipient keys, the
/// participant set, and the symmetric manual-DM binding are derived locally;
/// this command does not inspect or claim proof of a service-page conversation.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn activate_manual_peer_context(
    app: tauri::AppHandle,
    broker: State<'_, HubBrokerState>,
    host: State<'_, ServiceHostState>,
    registry: State<'_, ServiceRegistryState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
    person_id: String,
) -> Result<ManualPeerContextLease, String> {
    let _session = session.transition.lock().await;
    native_discord_overlay::clear_and_hide(&app);
    let owner = active_unlocked_osl_user_id(&core)?;
    let binding = security::manual_peer_binding(&core, person_id)?;
    let activated = broker::activate_owned_manual_peer_context(
        &broker,
        &registry,
        &host,
        &owner,
        &service_id,
        &account_id,
        binding,
    )?;
    let scope_approved = security::manual_peer_scope_approved(
        &core,
        &activated.lease.service_id,
        &activated.lease.account_id,
        activated.person_id.clone(),
        activated.scope,
    )?;
    Ok(ManualPeerContextLease {
        context_token: activated.lease.context_token,
        service_id: activated.lease.service_id,
        account_id: activated.lease.account_id,
        person_id: activated.person_id,
        peer_osl_user_id: activated.peer_osl_user_id,
        scope_approved,
    })
}

/// Activate a synthetic OSL protection scope over the currently attached,
/// signed native Discord lifecycle. The renderer selects only an already-known
/// friend; service id, account id, owner, and generation are derived locally.
#[tauri::command]
async fn activate_native_manual_peer_context(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    person_id: String,
) -> Result<ManualPeerContextLease, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    native_discord_overlay::clear_and_hide(&app);
    let active = app
        .state::<NativeWindowHostState>()
        .current_discord_service_host(&owner)?;
    let binding = security::manual_peer_binding(&core, person_id)?;
    let activated = broker::activate_owned_native_manual_peer_context(
        &app.state::<HubBrokerState>(),
        &owner,
        &active,
        binding,
    )?;
    let scope_approved = security::manual_peer_scope_approved(
        &core,
        &activated.lease.service_id,
        &activated.lease.account_id,
        activated.person_id.clone(),
        activated.scope,
    )?;
    Ok(ManualPeerContextLease {
        context_token: activated.lease.context_token,
        service_id: activated.lease.service_id,
        account_id: activated.lease.account_id,
        person_id: activated.person_id,
        peer_osl_user_id: activated.peer_osl_user_id,
        scope_approved,
    })
}

#[tauri::command]
async fn activate_osl_chat_context(
    caller: tauri::WebviewWindow,
    broker: State<'_, HubBrokerState>,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    person_id: String,
) -> Result<ManualPeerContextLease, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may open OSL Chats".to_owned());
    }
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let binding = security::manual_peer_binding(&core, person_id)?;
    let activated = broker::activate_owned_osl_chat_context(&broker, &owner, binding)?;
    let scope_approved = security::manual_peer_scope_approved(
        &core,
        &activated.lease.service_id,
        &activated.lease.account_id,
        activated.person_id.clone(),
        activated.scope,
    )?;
    Ok(ManualPeerContextLease {
        context_token: activated.lease.context_token,
        service_id: activated.lease.service_id,
        account_id: activated.lease.account_id,
        person_id: activated.person_id,
        peer_osl_user_id: activated.peer_osl_user_id,
        scope_approved,
    })
}

#[tauri::command]
async fn close_osl_chat_context(
    caller: tauri::WebviewWindow,
    broker: State<'_, HubBrokerState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<(), String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may close OSL Chats".to_owned());
    }
    let _session = session.transition.lock().await;
    broker.clear_osl_chat_context()
}

/// Produce marker-free encrypted copy text for manual user placement. Nothing
/// is placed into or sent through the hosted service by this command.
#[tauri::command]
async fn prepare_peer_prose_text(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedPeerProseMessage, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let security_state = app.state::<HubSecurityState>();
        let broker_state = app.state::<HubBrokerState>();
        let require_capture_protection = app
            .state::<PreviewState>()
            .get()
            .map(|preferences| preferences.window_capture_enabled)
            .unwrap_or(true);
        let _active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
        let prepared = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::prepare_peer_prose_text_with_capture(
                &core,
                &security_state,
                &broker_state,
                &context_token,
                plaintext,
                view_once,
                require_capture_protection,
            )
        })?;
        let _still_active =
            require_current_context_host(&app, &core, &broker_state, &context_token)?;
        Ok(prepared)
    })
    .await
    .map_err(|error| format!("OSL broker worker failed: {error}"))?
}

/// Open manually pasted marker-free encrypted text in the trusted local UI.
#[tauri::command]
async fn open_peer_prose_text(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    sender_person_id: String,
    cover_text: String,
) -> Result<OpenedPeerProseMessage, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let security_state = app.state::<HubSecurityState>();
        let broker_state = app.state::<HubBrokerState>();
        let _active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
        let opened = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::open_peer_prose_text(
                &core,
                &security_state,
                &broker_state,
                &context_token,
                sender_person_id,
                cover_text,
            )
        })?;
        let _still_active =
            require_current_context_host(&app, &core, &broker_state, &context_token)?;
        Ok(opened)
    })
    .await
    .map_err(|error| format!("OSL broker worker failed: {error}"))?
}

#[tauri::command]
async fn prepare_encrypted_text(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    plaintext: String,
) -> Result<PreparedCoreMessage, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let prepared = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::prepare_encrypted_text(&core, &broker_state, &context_token, plaintext)
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker service host closed during preparation".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
        Ok(prepared)
    })
    .await
    .map_err(|error| format!("OSL broker worker failed: {error}"))?
}

#[tauri::command]
async fn decrypt_hub_capsule(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    sender_osl_id: String,
    service_message_id: Option<String>,
    capsule: String,
) -> Result<String, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let plaintext = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::decrypt_capsule(
                &core,
                &broker_state,
                &context_token,
                sender_osl_id,
                service_message_id,
                capsule,
            )
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker service host closed during decryption".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
        Ok(plaintext)
    })
    .await
    .map_err(|error| format!("OSL broker worker failed: {error}"))?
}

#[tauri::command]
async fn export_hub_friend_code(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<FriendCodeExport, String> {
    let _session = session.transition.lock().await;
    security::export_friend_code(&core)
}

/// Copy only the current identity's freshly signed friend invite. This command
/// accepts no text and exposes no clipboard-read or generic write surface.
#[tauri::command]
async fn copy_hub_friend_invite(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<(), String> {
    let friend_code = {
        let _session = session.transition.lock().await;
        security::export_friend_code(&core)?.friend_code
    };

    #[cfg(windows)]
    {
        tokio::task::spawn_blocking(move || write_windows_clipboard_text(&friend_code))
            .await
            .map_err(|_| "The Windows clipboard operation was interrupted".to_owned())?
    }
    #[cfg(not(windows))]
    {
        let _ = friend_code;
        Err("Copy invite is available in the Windows app".to_owned())
    }
}

#[cfg(windows)]
fn write_windows_clipboard_text(value: &str) -> Result<(), String> {
    use std::{ptr, thread, time::Duration};
    use windows_sys::Win32::{
        Foundation::GlobalFree,
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
            Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
            Ole::CF_UNICODETEXT,
        },
    };

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            // SAFETY: this guard exists only after this thread successfully
            // opened the clipboard and closes that exact open operation.
            unsafe { CloseClipboard() };
        }
    }

    let mut opened = false;
    for _ in 0..8 {
        // SAFETY: a null owner is explicitly supported by OpenClipboard. The
        // command never reads the clipboard and immediately closes it below.
        if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
            opened = true;
            break;
        }
        thread::sleep(Duration::from_millis(8));
    }
    if !opened {
        return Err("The Windows clipboard is busy".to_owned());
    }
    let _clipboard = ClipboardGuard;

    let utf16 = value
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let byte_len = utf16
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| "The friend invite is too large for the clipboard".to_owned())?;
    // SAFETY: byte_len is checked above and the returned movable allocation is
    // kept owned by this function until SetClipboardData accepts ownership.
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) };
    if memory.is_null() {
        return Err("The Windows clipboard could not allocate memory".to_owned());
    }
    // SAFETY: memory is a valid allocation from GlobalAlloc.
    let destination = unsafe { GlobalLock(memory) }.cast::<u16>();
    if destination.is_null() {
        // SAFETY: ownership has not been transferred to the clipboard.
        unsafe { GlobalFree(memory) };
        return Err("The Windows clipboard memory could not be locked".to_owned());
    }
    // SAFETY: destination is byte_len bytes and utf16 contains exactly the
    // same number of u16 values, including one trailing NUL.
    unsafe {
        ptr::copy_nonoverlapping(utf16.as_ptr(), destination, utf16.len());
        GlobalUnlock(memory);
    }
    // SAFETY: this thread owns the open clipboard and writes only Unicode text.
    if unsafe { EmptyClipboard() } == 0 {
        // SAFETY: ownership has not been transferred to the clipboard.
        unsafe { GlobalFree(memory) };
        return Err("The Windows clipboard could not be cleared".to_owned());
    }
    // SAFETY: after success Windows owns memory; after failure we free it.
    if unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null() {
        unsafe { GlobalFree(memory) };
        return Err("The friend invite could not be copied".to_owned());
    }
    Ok(())
}

#[tauri::command]
async fn add_hub_friend(
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    friend_code: String,
    alias: Option<String>,
) -> Result<AddFriendResult, String> {
    let _session = session.transition.lock().await;
    security::add_friend_code(&core, &security_state, friend_code, alias)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HubUsernameClaim {
    username: String,
    osl_user_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HubUsernameStatus {
    username: String,
    owned_by_active_identity: bool,
}

#[tauri::command]
async fn get_hub_username_status(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    username: String,
) -> Result<HubUsernameStatus, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let _owner = active_unlocked_osl_user_id(&core)?;
        let exported = security::export_friend_code(&core)?;
        let keyserver = core
            .osl
            .keyserver
            .lock()
            .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL keyserver is not initialized".to_owned())?;
        let found = keyserver
            .lookup_username(&username)
            .map_err(|error| format!("OSL username ownership is unavailable: {error}"))?;
        Ok(HubUsernameStatus {
            username: found.username,
            owned_by_active_identity: found.friend_code == exported.friend_code,
        })
    })
    .await
    .map_err(|error| format!("OSL username status worker failed: {error}"))?
}

#[tauri::command]
async fn claim_hub_username(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    username: String,
) -> Result<HubUsernameClaim, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let exported = security::export_friend_code(&core)?;
        let identity = core
            .osl
            .identity
            .lock()
            .map_err(|_| "OSL identity state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL identity is not loaded".to_owned())?;
        let keyserver = core
            .osl
            .keyserver
            .lock()
            .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL keyserver is not initialized".to_owned())?;
        let claimed = keyserver
            .claim_username(&identity, &username, &exported.friend_code)
            .map_err(|error| format!("OSL username could not be claimed: {error}"))?;
        Ok(HubUsernameClaim {
            username: claimed.username,
            osl_user_id: claimed.user_id,
        })
    })
    .await
    .map_err(|error| format!("OSL username worker failed: {error}"))?
}

#[tauri::command]
async fn add_hub_friend_by_username(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    username: String,
    alias: Option<String>,
) -> Result<AddFriendResult, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let security_state = app.state::<HubSecurityState>();
        let _owner = active_unlocked_osl_user_id(&core)?;
        let keyserver = core
            .osl
            .keyserver
            .lock()
            .map_err(|_| "OSL keyserver state is unavailable".to_owned())?
            .clone()
            .ok_or_else(|| "OSL keyserver is not initialized".to_owned())?;
        let resolved = keyserver
            .lookup_username(&username)
            .map_err(|error| format!("OSL username could not be resolved: {error}"))?;
        if resolved.username != username {
            return Err("OSL username lookup did not match the requested username".to_owned());
        }
        security::add_friend_code(&core, &security_state, resolved.friend_code, alias)
    })
    .await
    .map_err(|error| format!("OSL username worker failed: {error}"))?
}

#[tauri::command]
async fn verify_hub_friend_safety_number(
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    person_id: String,
    safety_number: String,
) -> Result<PersonDto, String> {
    let _session = session.transition.lock().await;
    security::verify_friend_safety_number(&core, &security_state, person_id, safety_number)
}

#[tauri::command]
async fn list_hub_people(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<PersonDto>, String> {
    let _session = session.transition.lock().await;
    security::list_people(&core)
}

#[tauri::command]
async fn set_hub_friend_nickname(
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    person_id: String,
    nickname: Option<String>,
) -> Result<PersonDto, String> {
    let _session = session.transition.lock().await;
    security::set_friend_alias(&core, &security_state, person_id, nickname)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn set_active_hub_friend_permission(
    app: tauri::AppHandle,
    broker_state: State<'_, HubBrokerState>,
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    person_id: String,
    enabled: bool,
    broadened: bool,
) -> Result<(), String> {
    let _session = session.transition.lock().await;
    let active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    let person_id = broker_state.manual_permission_target(&context_token, &person_id, broadened)?;
    let scope = broker_state.scope_for_context(&context_token)?;
    with_indexed_context_write(&app, &broker_state, &context_token, || {
        security::set_manual_peer_scope_permission(
            &core,
            &security_state,
            &active.service_id,
            &active.account_id,
            person_id,
            scope,
            enabled,
        )
    })?;
    let _still_active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    Ok(())
}

#[tauri::command]
async fn get_active_hub_context_security(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    broker_state: State<'_, HubBrokerState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
) -> Result<ScopeSecurityDto, String> {
    let _session = session.transition.lock().await;
    let _active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    security::scope_security(broker_state.scope_for_context(&context_token)?)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn set_active_hub_context_security(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    broker_state: State<'_, HubBrokerState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    ttl_seconds: u32,
    decrypt_display_enabled: bool,
) -> Result<ScopeSecurityDto, String> {
    let _session = session.transition.lock().await;
    let _active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    let scope = broker_state.scope_for_context(&context_token)?;
    let saved = with_indexed_context_write(&app, &broker_state, &context_token, || {
        security::set_scope_security(&security_state, scope, ttl_seconds, decrypt_display_enabled)
    })?;
    let _still_active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    Ok(saved)
}

#[tauri::command]
async fn prepare_local_protected_text_with_policy(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    plaintext: String,
    view_once: bool,
) -> Result<PreparedLocalProtectedMessage, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let prepared = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::prepare_local_protected_text_with_policy(
                &core,
                &broker_state,
                &context_token,
                plaintext,
                view_once,
            )
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker service host closed during preparation".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
        Ok(prepared)
    })
    .await
    .map_err(|error| format!("OSL protected worker failed: {error}"))?
}

#[tauri::command]
async fn prepare_hub_attachment(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    original_bytes_b64: String,
    original_filename: String,
) -> Result<PreparedHubAttachment, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        require_active_pro_entitlement(&core)?;
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL attachment preparation requires an active service".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let prepared = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::prepare_encrypted_attachment(
                &core,
                &broker_state,
                &context_token,
                original_bytes_b64,
                original_filename,
            )
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL service closed during attachment preparation".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
        require_active_pro_entitlement(&core)?;
        Ok(prepared)
    })
    .await
    .map_err(|error| format!("OSL attachment worker failed: {error}"))?
}

#[tauri::command]
async fn open_hub_attachment(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    sender_osl_id: String,
    service_message_id: Option<String>,
    sealed_b64: String,
) -> Result<OpenedHubAttachment, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        require_active_pro_entitlement(&core)?;
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL attachment opening requires an active service".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let opened = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::open_encrypted_attachment(
                &core,
                &broker_state,
                &context_token,
                sender_osl_id,
                service_message_id,
                sealed_b64,
            )
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL service closed during attachment opening".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
        require_active_pro_entitlement(&core)?;
        Ok(opened)
    })
    .await
    .map_err(|error| format!("OSL attachment worker failed: {error}"))?
}

#[tauri::command]
async fn decrypt_local_protected_capsule(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    capsule: String,
) -> Result<DecryptedLocalProtectedMessage, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let decrypted = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::decrypt_local_protected_capsule(&core, &broker_state, &context_token, capsule)
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker service host closed during decryption".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
        Ok(decrypted)
    })
    .await
    .map_err(|error| format!("OSL protected worker failed: {error}"))?
}

#[tauri::command]
async fn list_hub_identities(
    core: State<'_, HubCoreState>,
    identities: State<'_, HubIdentityRegistryState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<Vec<HubIdentitySlotDto>, String> {
    let _session = session.transition.lock().await;
    identity_registry::list_identity_slots(&core, &identities)
}

#[tauri::command]
async fn create_hub_identity_slot(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    label: String,
) -> Result<HubIdentitySlotCreation, String> {
    let _session = session.transition.lock().await;
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
    native_discord_overlay::clear_and_hide(&app);
    let _ = app.state::<NativeWindowHostState>().terminate();
    let _ = app.state::<MullvadWindowHostState>().restore();
    let _ = app.state::<BrowserCompanionState>().terminate();
    app.state::<HubBrokerState>().clear()?;
    tauri::async_runtime::spawn_blocking(move || {
        identity_registry::create_identity_slot(
            &app.state::<HubCoreState>(),
            &app.state::<HubIdentityRegistryState>(),
            label,
        )
    })
    .await
    .map_err(|_| "OSL identity creation worker failed".to_owned())?
}

#[tauri::command]
async fn recover_hub_identity_slot(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    label: String,
    identity_recovery_phrase: String,
) -> Result<HubIdentitySlotCreation, String> {
    let _session = session.transition.lock().await;
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
    native_discord_overlay::clear_and_hide(&app);
    let _ = app.state::<NativeWindowHostState>().terminate();
    let _ = app.state::<MullvadWindowHostState>().restore();
    let _ = app.state::<BrowserCompanionState>().terminate();
    app.state::<HubBrokerState>().clear()?;
    tauri::async_runtime::spawn_blocking(move || {
        identity_registry::recover_identity_slot(
            &app.state::<HubCoreState>(),
            &app.state::<HubIdentityRegistryState>(),
            label,
            identity_recovery_phrase,
        )
    })
    .await
    .map_err(|_| "OSL identity recovery worker failed".to_owned())?
}

#[tauri::command]
async fn switch_hub_identity(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    slot_id: String,
) -> Result<HubIdentitySwitchResult, String> {
    let _session = session.transition.lock().await;
    app.state::<ScrubImapState>().revoke_all();
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
    native_discord_overlay::clear_and_hide(&app);
    let _ = app.state::<NativeWindowHostState>().terminate();
    let _ = app.state::<MullvadWindowHostState>().restore();
    let _ = app.state::<BrowserCompanionState>().terminate();
    app.state::<HubBrokerState>().clear()?;
    tauri::async_runtime::spawn_blocking(move || {
        identity_registry::switch_identity_slot(
            &app.state::<HubCoreState>(),
            &app.state::<HubIdentityRegistryState>(),
            slot_id,
        )
    })
    .await
    .map_err(|_| "OSL identity switch worker failed".to_owned())?
}

#[tauri::command]
async fn burn_active_hub_identity(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
) -> Result<HubIdentityBurnResult, String> {
    let _session = session.transition.lock().await;
    app.state::<ScrubImapState>().revoke_all();
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
    native_discord_overlay::clear_and_hide(&app);
    let _ = app.state::<NativeWindowHostState>().terminate();
    let _ = app.state::<MullvadWindowHostState>().restore();
    let _ = app.state::<BrowserCompanionState>().terminate();
    app.state::<HubBrokerState>().clear()?;
    tauri::async_runtime::spawn_blocking(move || {
        let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
        app.state::<ServiceScopeIndexState>()
            .remove_identity(&owner)?;
        identity_registry::burn_active_identity(
            &app.state::<HubCoreState>(),
            &app.state::<HubIdentityRegistryState>(),
        )
    })
    .await
    .map_err(|_| "OSL identity burn worker failed".to_owned())?
}

#[tauri::command]
async fn execute_hub_full_cleanup(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
) -> Result<HubFullCleanupResult, String> {
    let _session = session.transition.lock().await;
    app.state::<ScrubImapState>().revoke_all();
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
    native_discord_overlay::clear_and_hide(&app);
    let _ = app.state::<NativeWindowHostState>().terminate();
    let _ = app.state::<MullvadWindowHostState>().restore();
    let _ = app.state::<BrowserCompanionState>().terminate();
    app.state::<HubBrokerState>().clear()?;
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "OSL Privacy configuration storage is unavailable".to_owned())?;
    let local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "OSL Privacy local storage is unavailable".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        cleanup::execute_full_hub_cleanup(
            &app.state::<HubCoreState>(),
            &config_dir,
            &local_data_dir,
            true,
        )
    })
    .await
    .map_err(|_| "OSL full cleanup worker failed".to_owned())?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HubServiceBurnReadiness {
    burn_id: String,
    manifest_digest: String,
    indexed_scopes: usize,
    coverage_complete: bool,
    login_profile_untouched: bool,
    native_history_untouched: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HubServiceBurnResult {
    burn_id: String,
    scopes_burned: usize,
    rows_destroyed: usize,
    whitelist_entries_removed: usize,
    remote_blobs_deleted: usize,
    remote_blob_deletions_failed: usize,
    local_cleanup_complete: bool,
    remote_cleanup_complete: bool,
    login_profile_untouched: bool,
    native_history_untouched: bool,
}

fn require_owned_service_account(
    core: &HubCoreState,
    registry: &ServiceRegistryState,
    service_id: &str,
    account_id: &str,
) -> Result<String, String> {
    let owner = active_unlocked_osl_user_id(core)?;
    let kind = osl_privacy_hub::services::service_kind_from_id(service_id)
        .ok_or_else(|| "unknown service".to_owned())?;
    registry.require_owned(&owner, kind, account_id)?;
    Ok(owner)
}

#[tauri::command]
async fn get_hub_service_burn_readiness(
    core: State<'_, HubCoreState>,
    registry: State<'_, ServiceRegistryState>,
    index: State<'_, ServiceScopeIndexState>,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
) -> Result<HubServiceBurnReadiness, String> {
    let _session = session.transition.lock().await;
    let owner = require_owned_service_account(&core, &registry, &service_id, &account_id)?;
    let manifest = index.preview_complete_manifest(&owner, &service_id, &account_id)?;
    Ok(HubServiceBurnReadiness {
        burn_id: bytes_hex(&manifest.burn_id),
        manifest_digest: bytes_hex(&manifest.manifest_digest),
        indexed_scopes: manifest.scopes.len(),
        coverage_complete: true,
        login_profile_untouched: true,
        native_history_untouched: true,
    })
}

#[tauri::command]
async fn burn_hub_service_account(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    service_id: String,
    account_id: String,
    confirmed_burn_id: String,
) -> Result<HubServiceBurnResult, String> {
    let _session = session.transition.lock().await;
    app.state::<ScrubImapState>().revoke_all();
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let registry = app.state::<ServiceRegistryState>();
        let index = app.state::<ServiceScopeIndexState>();
        let owner = require_owned_service_account(&core, &registry, &service_id, &account_id)?;
        let preview = index.preview_complete_manifest(&owner, &service_id, &account_id)?;
        if confirmed_burn_id != bytes_hex(&preview.burn_id) {
            return Err(
                "The service burn scope changed; review and confirm the new manifest".to_owned(),
            );
        }
        let manifest = index.freeze_complete_manifest(&owner, &service_id, &account_id)?;
        if manifest.burn_id != preview.burn_id {
            return Err("The service burn scope changed before it could be frozen".to_owned());
        }
        burn_indexed_service_manifest(&app, &index, &manifest)
    })
    .await
    .map_err(|_| "OSL service burn worker failed".to_owned())?
}

fn burn_indexed_service_manifest(
    app: &tauri::AppHandle,
    index: &ServiceScopeIndexState,
    manifest: &ImmutableServiceBurnManifest,
) -> Result<HubServiceBurnResult, String> {
    let mut scopes_burned = 0usize;
    let mut rows_destroyed = 0usize;
    let mut whitelist_entries_removed = 0usize;
    let mut remote_blobs_deleted = 0usize;
    let mut remote_blob_deletions_failed = 0usize;
    for indexed in index.pending_scopes(manifest)? {
        let result = if let Some(person_id) = indexed.manual_peer_person_id.as_deref() {
            security::burn_manual_peer_scope(
                &app.state::<HubCoreState>(),
                &app.state::<HubSecurityState>(),
                &manifest.service_id,
                &manifest.account_id,
                person_id,
                indexed.scope.clone(),
            )?
        } else {
            security::burn_scope(
                &app.state::<HubCoreState>(),
                &app.state::<HubSecurityState>(),
                indexed.scope.clone(),
                indexed.canonical_channel_ids.clone(),
                true,
                Vec::new(),
            )?
        };
        broker::burn_indexed_local_protected_binding(
            &app.state::<HubCoreState>(),
            &indexed.local_context_binding_sha256,
        )?;
        index.mark_scope_burned(manifest, &indexed.storage_key)?;
        scopes_burned = scopes_burned.saturating_add(1);
        rows_destroyed = rows_destroyed.saturating_add(result.rows_destroyed);
        whitelist_entries_removed =
            whitelist_entries_removed.saturating_add(result.whitelist_entries_removed);
        remote_blobs_deleted = remote_blobs_deleted.saturating_add(result.remote_blobs_deleted);
        remote_blob_deletions_failed =
            remote_blob_deletions_failed.saturating_add(result.remote_blob_deletions_failed);
    }
    index.finish_burn(manifest)?;
    native_discord_overlay::clear_and_hide(&app);
    app.state::<HubBrokerState>().clear()?;
    Ok(HubServiceBurnResult {
        burn_id: bytes_hex(&manifest.burn_id),
        scopes_burned,
        rows_destroyed,
        whitelist_entries_removed,
        remote_blobs_deleted,
        remote_blob_deletions_failed,
        local_cleanup_complete: true,
        remote_cleanup_complete: remote_blob_deletions_failed == 0,
        login_profile_untouched: true,
        native_history_untouched: true,
    })
}

fn bytes_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[tauri::command]
async fn burn_active_hub_context(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
) -> Result<HubScopeBurnResult, String> {
    let _session = session.transition.lock().await;
    app.state::<ScrubImapState>().revoke_all();
    tauri::async_runtime::spawn_blocking(move || {
        let broker_state = app.state::<HubBrokerState>();
        let core = app.state::<HubCoreState>();
        let _active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
        let result = if let Some(manual) = broker_state.manual_burn_target(&context_token)? {
            security::burn_manual_peer_scope(
                &core,
                &app.state::<HubSecurityState>(),
                &manual.service_id,
                &manual.account_id,
                &manual.person_id,
                manual.scope,
            )?
        } else {
            let scope_input = broker_state.scope_for_context(&context_token)?;
            let known_channel_ids = scope_input.channel_id.clone().into_iter().collect();
            security::burn_scope(
                &core,
                &app.state::<HubSecurityState>(),
                scope_input,
                known_channel_ids,
                true,
                Vec::new(),
            )?
        };
        let _still_active =
            require_current_context_host(&app, &core, &broker_state, &context_token)?;
        broker::burn_local_protected_context(&core, &broker_state, &context_token)?;
        Ok(result)
    })
    .await
    .map_err(|_| "OSL active-context burn worker failed".to_owned())?
}

fn main() {
    if osl_privacy_hub::native_window_host::run_borrowed_window_guardian_if_requested() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let restored = app.get_webview_window("main").is_some_and(|window| {
                main_window_is_live(&window) && window.unminimize().is_ok() && window.show().is_ok()
            });
            if restored {
                if let Some(window) = app.get_webview_window("main") {
                    // Focus can legitimately fail in a disconnected or
                    // minimized RDP session. A visible window is still healthy.
                    let _ = window.set_focus();
                }
            } else {
                // A retained service webview can otherwise leave the
                // single-instance process alive without a recoverable main UI.
                app.request_restart();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_page_load(|webview, _| {
            #[cfg(windows)]
            {
                let _ = window_border::suppress_accent_border(webview);
                // Protect OSL-owned pixels before the renderer can paint any
                // account or recovery UI. Foreign native app windows remain
                // outside this boundary and are never claimed as protected.
                let _ = screenshot::apply_to_webview(webview, runtime::ScreenshotProtection::On);
            }
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            #[cfg(windows)]
            if matches!(event, tauri::WindowEvent::Focused(true)) {
                if let Some(webview) = window.app_handle().get_webview_window("main") {
                    let _ =
                        screenshot::apply_to_window(&webview, runtime::ScreenshotProtection::On);
                }
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                if !app.state::<MainWindowLifecycleState>().begin_close() {
                    return;
                }
                let _ = window.hide();
                // Arm the bounded exit before waiting on any native-host lock.
                // A concurrent launch may legitimately hold that lock while it
                // discovers and adopts its exact window. Close must never block
                // Tauri's window-event thread behind that operation.
                let watchdog_app = app.clone();
                std::thread::spawn(move || {
                    // Native discovery/adoption is bounded to eleven seconds.
                    // Leave enough time for its exact window restoration while
                    // retaining an unconditional upper bound on shutdown.
                    std::thread::sleep(std::time::Duration::from_secs(15));
                    watchdog_app.exit(0);
                });
                native_discord_overlay::clear_and_hide(&app);
                tauri::async_runtime::spawn(async move {
                    let native_cleanup_app = app.clone();
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let _ = native_cleanup_app
                            .state::<NativeWindowHostState>()
                            .terminate();
                        let _ = native_cleanup_app
                            .state::<MullvadWindowHostState>()
                            .restore();
                        let _ = native_cleanup_app
                            .state::<BrowserCompanionState>()
                            .terminate();
                    })
                    .await;
                    let host = app.state::<ServiceHostState>();
                    let _ = service_host::desktop::shutdown(&app, &host).await;
                    app.exit(0);
                });
            }
        })
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .map_err(|error| format!("could not resolve app config directory: {error}"))?;
            // The app owns a separate OSL identity namespace. Never inherit
            // the original Discord client's `%APPDATA%/osl` login merely
            // because both applications run on the same Windows account.
            keystore::set_active_account_dir(None);
            let osl_core_dir = config_dir.join("osl-core");
            keystore::set_base_dir_override(Some(osl_core_dir.clone()));
            #[cfg(feature = "discord-qa-shell")]
            osl_privacy_hub::discord_qa_identity::install_device_bound_storage_key(&osl_core_dir)?;
            let local_data_dir = app
                .path()
                .app_local_data_dir()
                .map_err(|error| format!("could not resolve app local-data directory: {error}"))?;
            peer_attachment_io::scavenge_staging_on_startup(&local_data_dir)?;
            // Resume an already-committed gate burn before any identity can be
            // selected or decrypted. The recovery record contains no paths or
            // secrets; it only authorizes the same fixed-root idempotent purge.
            cleanup::resume_interrupted_gate_burn(&config_dir, &local_data_dir)?;
            identity_registry::select_active_identity_before_bootstrap()?;
            app.manage(PreviewState::load(
                config_dir.join("preview-preferences.json"),
            ));
            app.manage(ServiceRegistryState::load(
                config_dir.join("service-registry.json"),
            ));
            app.manage(ServiceScopeIndexState::load(
                config_dir.join("service-scope-index.json"),
            ));
            let core = HubCoreState::bootstrap_from_disk();
            #[cfg(feature = "discord-qa-shell")]
            osl_privacy_hub::discord_qa_identity::ensure_disposable_identity(&core)?;
            let security_state = HubSecurityState::default();
            #[cfg(feature = "discord-qa-shell")]
            osl_privacy_hub::discord_qa_identity::publish_and_consume_pairing(
                &osl_core_dir,
                &core,
                &security_state,
            )?;
            app.manage(core);
            app.manage(HubBrokerState::default());
            app.manage(security_state);
            app.manage(HubIdentityRegistryState::default());
            app.manage(ServiceHostState::default());
            app.manage(NativeWindowHostState::default());
            app.manage(NativeDiscordComposerState::default());
            app.manage(LocalCoverState::default());
            app.manage(OverlaySessionState::default());
            app.manage(MullvadWindowHostState::default());
            app.manage(BrowserCompanionState::default());
            app.manage(HubAccountSessionState::default());
            app.manage(MainWindowLifecycleState::default());
            app.manage(HubUpdaterState::default());
            app.manage(HubNotificationState::default());
            app.manage(ScrubIndexState::default());
            app.manage(ScrubImapState::default());
            // Snapshot identity availability before scheduling. On first run
            // this remains unscheduled until password setup finishes its
            // verified local bootstrap; an identity created moments later can
            // therefore never race this launch worker into a duplicate call.
            schedule_deferred_registration(app.handle());
            // A failed browser-profile deletion can leave a large tombstone.
            // Retrying it synchronously here would hold the first paint behind
            // an unbounded recursive filesystem walk. Tombstones are already
            // detached from every live account name, so retry them off the UI
            // startup path and keep failures pending for the next launch.
            let cleanup_app = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = service_host::desktop::scavenge_profile_tombstones_on_startup(&cleanup_app);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_onboarding_preferences,
            configure_scrub_imap_account,
            get_scrub_imap_capability,
            reauth_scrub_imap_account,
            scrub_imap_enumerate,
            scrub_imap_inspect,
            scrub_imap_delete,
            scrub_imap_verify,
            list_hub_app_notifications,
            set_hub_notifications_enabled,
            set_hub_screenshot_protection,
            open_external_link_in_default_browser,
            lock_hub_session,
            schedule_protected_clipboard_clear,
            save_onboarding_preferences,
            scan_local_privacy,
            get_osl_profile,
            save_osl_profile,
            list_osl_notes,
            save_osl_note,
            delete_osl_note,
            initialize_scrub_index,
            append_scrub_index_chunk,
            get_scrub_index_status,
            get_scrub_index_scan,
            pause_scrub_index,
            resume_scrub_index,
            cancel_scrub_index,
            list_linked_services,
            get_core_readiness,
            list_core_features,
            get_hub_license_state,
            get_mass_cleanup_capabilities,
            discover_mass_cleanup_targets,
            execute_mass_cleanup_batch,
            validate_hub_activation_code,
            clear_hub_activation_code,
            unlock_hub_password_gate,
            create_hub_osl_identity,
            import_hub_osl_identity_phrase,
            setup_hub_main_password,
            get_hub_password_role_status,
            set_hub_stealth_password,
            remove_hub_stealth_password,
            set_hub_burn_password,
            remove_hub_burn_password,
            check_hub_for_updates,
            install_hub_update,
            open_hub_releases_page,
            open_hub_source_repository,
            list_native_apps,
            install_native_app,
            get_mullvad_status,
            install_mullvad,
            open_mullvad,
            list_browser_imports,
            open_browser_import,
            get_firefox_status,
            install_firefox,
            begin_browser_account_import,
            begin_protected_browser_import,
            finish_protected_browser_import,
            cancel_protected_browser_import,
            launch_firefox_service,
            launch_system_browser_service,
            get_default_browser_companion_status,
            host_default_browser_companion,
            resize_default_browser_companion,
            focus_default_browser_companion,
            detach_default_browser_companion,
            host_native_app_window,
            resize_native_app_window,
            focus_native_app_window,
            detach_native_app_window,
            set_native_discord_protected_overlay_open,
            get_native_discord_overlay_state,
            prepare_native_discord_overlay_text,
            prepare_osl_chat_text,
            send_native_discord_overlay_carrier,
            open_native_discord_overlay_text,
            reveal_native_discord_overlay_view_once,
            open_osl_chat_text,
            list_osl_chat_history,
            select_osl_chat_attachment,
            list_osl_chat_attachments,
            open_osl_chat_attachment,
            select_native_discord_overlay_attachment,
            list_native_discord_overlay_attachments,
            open_native_discord_overlay_attachment,
            burn_native_discord_overlay_chat,
            set_native_discord_overlay_security,
            set_native_discord_covertext_enabled,
            host_mullvad_window,
            resize_mullvad_window,
            focus_mullvad_window,
            restore_mullvad_window,
            create_service_account,
            open_service_host,
            close_service_host,
            set_local_protected_sheet_open,
            remove_service_account,
            activate_local_loopback_context,
            activate_manual_peer_context,
            activate_native_manual_peer_context,
            activate_osl_chat_context,
            close_osl_chat_context,
            prepare_encrypted_text,
            decrypt_hub_capsule,
            prepare_peer_prose_text,
            open_peer_prose_text,
            prepare_local_protected_text_with_policy,
            decrypt_local_protected_capsule,
            prepare_hub_attachment,
            open_hub_attachment,
            export_hub_friend_code,
            copy_hub_friend_invite,
            add_hub_friend,
            claim_hub_username,
            get_hub_username_status,
            add_hub_friend_by_username,
            verify_hub_friend_safety_number,
            list_hub_people,
            set_hub_friend_nickname,
            set_active_hub_friend_permission,
            get_active_hub_context_security,
            set_active_hub_context_security,
            list_hub_identities,
            create_hub_identity_slot,
            recover_hub_identity_slot,
            switch_hub_identity,
            burn_active_hub_identity,
            execute_hub_full_cleanup,
            get_hub_service_burn_readiness,
            burn_hub_service_account,
            burn_active_hub_context
        ])
        .run(tauri::generate_context!())
        .expect("error while running OSL Privacy");
}
