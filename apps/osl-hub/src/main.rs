#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use osl_privacy_hub::broker::{
    self, DecryptedLocalProtectedMessage, HubBrokerState, OpenedHubAttachment,
    OpenedPeerProseMessage, PreparedCoreMessage, PreparedHubAttachment,
    PreparedLocalProtectedMessage, PreparedPeerProseMessage,
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
use osl_privacy_hub::native_apps::{
    self, BrowserAccountImportResult, BrowserImportId, BrowserImportResult, BrowserImportStatus,
    FirefoxInstallResult, FirefoxLaunchResult, FirefoxServiceId, FirefoxStatus,
    MullvadActionResult, MullvadStatus, NativeAppId, NativeAppStatus, NativeInstallResult,
};
use osl_privacy_hub::native_window_host::{NativeWindowHostResult, NativeWindowHostState};
use osl_privacy_hub::osl_chat;
use osl_privacy_hub::osl_profile::{HubProfileDto, HubProfileInput};
use osl_privacy_hub::password_lifecycle::{
    self, HubIdentitySetupResult, HubMainPasswordSetupResult,
};
use osl_privacy_hub::preferences::PreviewState;
use osl_privacy_hub::privacy_scan::{self, LocalMessageCandidate, LocalPrivacyScanResult};
use osl_privacy_hub::scrub_index::{
    ScrubIndexChunkRequest, ScrubIndexInitializeRequest, ScrubIndexState, ScrubIndexStatus,
};
use osl_privacy_hub::security::{
    self, AddFriendResult, FriendCodeExport, HubScopeBurnResult, HubSecurityState, PersonDto,
    ScopeSecurityDto,
};
use osl_privacy_hub::security_credentials::{self, HubPasswordRoleStatus};
use osl_privacy_hub::service_host::{self, ServiceHostState};
use osl_privacy_hub::service_scope_index::{ImmutableServiceBurnManifest, ServiceScopeIndexState};
use osl_privacy_hub::services::ServiceRegistryState;
use osl_privacy_hub::startup_gate::{self, HubGateUnlockResult, VerifiedGateRole};
use osl_privacy_hub::updates::{bounded_plain_notes, bounded_version, RELEASES_URL};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{Manager, State};
use tauri_plugin_updater::UpdaterExt;

#[cfg(windows)]
mod window_border;

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
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    profile: HubProfileInput,
) -> Result<HubProfileDto, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    tokio::task::spawn_blocking(move || {
        osl_privacy_hub::osl_profile::save_active_profile(&owner, profile)
    })
    .await
    .map_err(|_| "OSL profile save was interrupted".to_owned())?
}

fn active_unlocked_osl_user_id(core: &HubCoreState) -> Result<String, String> {
    core_bridge::readiness(core)
        .active_osl_user_id
        .ok_or_else(|| "Unlock an OSL identity before accessing service profiles".to_owned())
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
            Ok(HubGateUnlockResult::unlocked(verification, readiness))
        }
        VerifiedGateRole::Stealth => {
            service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
            let _ = app.state::<NativeWindowHostState>().detach();
            app.state::<HubBrokerState>().clear()?;
            startup_gate::enter_stealth_landing(&app.state::<HubCoreState>());
            Ok(HubGateUnlockResult::decoy(verification))
        }
        VerifiedGateRole::Burn => {
            service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
            let _ = app.state::<NativeWindowHostState>().detach();
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
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HubCoreState>();
        password_lifecycle::setup_main_password(&state, password)
    })
    .await
    .map_err(|_| "OSL password setup worker failed".to_string())?
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
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", RELEASES_URL]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(RELEASES_URL);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(RELEASES_URL);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "The fixed OSL releases page could not be opened".to_owned())
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
    native_apps::begin_browser_account_import(&app_local_data_dir, &owner)
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
) -> Result<NativeWindowHostResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let parent = main_window_hwnd(&app)?;
    let profile_root = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL-owned native profile directory is unavailable".to_owned())?;
    let operation_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        operation_app
            .state::<NativeWindowHostState>()
            .host(app_id, &profile_root, &owner, parent)
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
    app.state::<NativeWindowHostState>().detach()
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

#[tauri::command]
async fn prepare_osl_chat_text(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    plaintext: String,
    view_once: bool,
) -> Result<osl_chat::PreparedText, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may send OSL Chats".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        osl_chat::prepare_text(
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
) -> Result<osl_chat::OpenedBatch, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may receive OSL Chats".to_owned());
    }
    screenshot::apply_to_window(&caller, runtime::ScreenshotProtection::On)
        .map_err(|_| "Windows capture resistance is required to receive OSL Chats".to_owned())?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        osl_chat::open_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
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
) -> Result<Vec<osl_chat::HistoryRow>, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL window may read OSL Chat history".to_owned());
    }
    screenshot::apply_to_window(&caller, runtime::ScreenshotProtection::On).map_err(|_| {
        "Windows capture resistance is required to read OSL Chat history".to_owned()
    })?;
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        osl_chat::history(&app.state::<HubCoreState>(), &app.state::<HubBrokerState>())
    })
    .await
    .map_err(|error| format!("OSL Chat history worker failed: {error}"))?
}

/// Produce marker-free encrypted copy text for manual user placement. Nothing
/// is placed into or sent through the hosted service by this command.
#[tauri::command]
async fn prepare_peer_prose_text(
    app: tauri::AppHandle,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    plaintext: String,
) -> Result<PreparedPeerProseMessage, String> {
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let security_state = app.state::<HubSecurityState>();
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let prepared = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::prepare_peer_prose_text(
                &core,
                &security_state,
                &broker_state,
                &context_token,
                plaintext,
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
        let broker_state = app.state::<HubBrokerState>();
        let host_state = app.state::<ServiceHostState>();
        let active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let opened = with_indexed_context_write(&app, &broker_state, &context_token, || {
            broker::open_peer_prose_text(
                &core,
                &broker_state,
                &context_token,
                sender_person_id,
                cover_text,
            )
        })?;
        let still_active = host_state
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL broker service host closed during decryption".to_owned())?;
        broker_state.validate_active_host(&context_token, &still_active)?;
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
    host_state: State<'_, ServiceHostState>,
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
    let active = host_state
        .current()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "OSL People requires an active service host".to_owned())?;
    broker_state.validate_active_host(&context_token, &active)?;
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
    })
}

#[tauri::command]
async fn get_active_hub_context_security(
    host_state: State<'_, ServiceHostState>,
    broker_state: State<'_, HubBrokerState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
) -> Result<ScopeSecurityDto, String> {
    let _session = session.transition.lock().await;
    let active = host_state
        .current()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "OSL security settings require an active service host".to_owned())?;
    broker_state.validate_active_host(&context_token, &active)?;
    security::scope_security(broker_state.scope_for_context(&context_token)?)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn set_active_hub_context_security(
    app: tauri::AppHandle,
    host_state: State<'_, ServiceHostState>,
    broker_state: State<'_, HubBrokerState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    ttl_seconds: u32,
    decrypt_display_enabled: bool,
) -> Result<ScopeSecurityDto, String> {
    let _session = session.transition.lock().await;
    let active = host_state
        .current()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "OSL security settings require an active service host".to_owned())?;
    broker_state.validate_active_host(&context_token, &active)?;
    let scope = broker_state.scope_for_context(&context_token)?;
    with_indexed_context_write(&app, &broker_state, &context_token, || {
        security::set_scope_security(&security_state, scope, ttl_seconds, decrypt_display_enabled)
    })
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
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
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
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
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
    let host = app.state::<ServiceHostState>();
    service_host::desktop::shutdown(&app, &host).await?;
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
    tauri::async_runtime::spawn_blocking(move || {
        let broker_state = app.state::<HubBrokerState>();
        let host = app.state::<ServiceHostState>();
        let active = host
            .current()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OSL burn requires an active service host".to_owned())?;
        broker_state.validate_active_host(&context_token, &active)?;
        let result = if let Some(manual) = broker_state.manual_burn_target(&context_token)? {
            security::burn_manual_peer_scope(
                &app.state::<HubCoreState>(),
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
                &app.state::<HubCoreState>(),
                &app.state::<HubSecurityState>(),
                scope_input,
                known_channel_ids,
                true,
                Vec::new(),
            )?
        };
        broker::burn_local_protected_context(
            &app.state::<HubCoreState>(),
            &broker_state,
            &context_token,
        )?;
        Ok(result)
    })
    .await
    .map_err(|_| "OSL active-context burn worker failed".to_owned())?
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_page_load(|webview, _| {
            #[cfg(windows)]
            let _ = window_border::suppress_accent_border(webview);
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
            keystore::set_base_dir_override(Some(config_dir.join("osl-core")));
            let local_data_dir = app
                .path()
                .app_local_data_dir()
                .map_err(|error| format!("could not resolve app local-data directory: {error}"))?;
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
            app.manage(HubCoreState::bootstrap_from_disk());
            app.manage(HubBrokerState::default());
            app.manage(HubSecurityState::default());
            app.manage(HubIdentityRegistryState::default());
            app.manage(ServiceHostState::default());
            app.manage(NativeWindowHostState::default());
            app.manage(HubAccountSessionState::default());
            app.manage(HubUpdaterState::default());
            app.manage(HubNotificationState::default());
            app.manage(ScrubIndexState::default());
            let registration_app = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                registration_app
                    .state::<HubCoreState>()
                    .register_after_local_bootstrap();
            });
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
            list_hub_app_notifications,
            set_hub_notifications_enabled,
            set_hub_screenshot_protection,
            save_onboarding_preferences,
            scan_local_privacy,
            get_osl_profile,
            save_osl_profile,
            initialize_scrub_index,
            append_scrub_index_chunk,
            get_scrub_index_status,
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
            launch_firefox_service,
            host_native_app_window,
            resize_native_app_window,
            focus_native_app_window,
            detach_native_app_window,
            create_service_account,
            open_service_host,
            close_service_host,
            set_local_protected_sheet_open,
            remove_service_account,
            activate_local_loopback_context,
            activate_manual_peer_context,
            activate_osl_chat_context,
            close_osl_chat_context,
            prepare_osl_chat_text,
            open_osl_chat_text,
            list_osl_chat_history,
            prepare_encrypted_text,
            decrypt_hub_capsule,
            prepare_peer_prose_text,
            open_peer_prose_text,
            prepare_local_protected_text_with_policy,
            decrypt_local_protected_capsule,
            prepare_hub_attachment,
            open_hub_attachment,
            export_hub_friend_code,
            add_hub_friend,
            claim_hub_username,
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
