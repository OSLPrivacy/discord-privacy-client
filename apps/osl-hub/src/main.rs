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
#[cfg(feature = "discord-qa-shell")]
use osl_privacy_hub::native_discord_adapter::{
    compatibility_delay_ms, DiscordProtectedSendOutcome, VerifiedSentCarrierRow,
};
use osl_privacy_hub::native_discord_adapter::{
    deidentify_prepared_visual_structure, AccessibilityBounds, DiscordCarrierLayout,
    DiscordCarrierMode, DiscordCarrierReceipt, DiscordCarrierStatus, NativeDiscordComposerState,
    MAX_VISIBLE_CARRIER_ROWS,
};
use osl_privacy_hub::native_window_host::{
    DiscordSessionMode, DiscordTakeover, NativeWindowHostReason, NativeWindowHostResult,
    NativeWindowHostState,
};
use osl_privacy_hub::password_lifecycle::{
    self, HubIdentitySetupResult, HubMainPasswordSetupResult,
};
use osl_privacy_hub::peer_attachment_io;
use osl_privacy_hub::preferences::PreviewState;
use osl_privacy_hub::privacy_scan::{self, LocalMessageCandidate, LocalPrivacyScanResult};
use osl_privacy_hub::pro_context_cover::LocalCoverState;
use osl_privacy_hub::scrub_index::{
    ScrubIndexChunkRequest, ScrubIndexInitializeRequest, ScrubIndexState, ScrubIndexStatus,
};
use osl_privacy_hub::security::{
    self, AddFriendResult, FriendCodeExport, HubScopeBurnResult, HubSecurityState, PersonDto,
    RemoveFriendResult, ScopeSecurityDto,
};
use osl_privacy_hub::security_credentials::{self, HubPasswordRoleStatus};
use osl_privacy_hub::service_host::{self, ActiveServiceHost, ServiceHostState};
use osl_privacy_hub::service_scope_index::{ImmutableServiceBurnManifest, ServiceScopeIndexState};
use osl_privacy_hub::services::ServiceRegistryState;
use osl_privacy_hub::startup_gate::{self, HubGateUnlockResult, VerifiedGateRole};
use osl_privacy_hub::updates::{bounded_plain_notes, bounded_version, RELEASES_URL};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;

/// Diagnostics-only startup breadcrumb trace. TEMPORARY: added to bracket the
/// exact point where a freshly built binary hangs during launch before any
/// window is created. Every call site is marked `// STARTUP-TRACE` so the
/// whole trace can be grepped out later.
///
/// Appends one `"<elapsed_ms> <label>"` line to
/// `std::env::temp_dir()/osl-startup-trace.txt`, opening, writing, flushing
/// and dropping the file handle on every call so a line is durable on disk
/// even if the process wedges immediately afterward. Never panics and never
/// blocks: every fallible step swallows its error with `let _ = ...`.
fn startup_breadcrumb(label: &str) {
    // STARTUP-TRACE
    use std::io::Write as _;
    static PROCESS_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = *PROCESS_START.get_or_init(std::time::Instant::now);
    let elapsed_ms = start.elapsed().as_millis();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("osl-startup-trace.txt"))
    {
        let _ = writeln!(file, "{elapsed_ms} {label}");
        let _ = file.flush();
    }
}

#[cfg(feature = "discord-qa-shell")]
const B6_PREFLIGHT_ONLY_ARG: &str = "--b6-preflight-only";
#[cfg(feature = "discord-qa-shell")]
const B6_PREFLIGHT_RECEIPT_FILE: &str = "osl-discord-qa-b6-preflight.v2.json";

#[cfg(feature = "discord-qa-shell")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordQaB6StartupReceipt<'a> {
    schema_version: u8,
    b6_preflight: &'a broker::DiscordQaB6Preflight,
    b6_proof_receipt: broker::B6ProofReceipt,
}

/// The first QA-shell action. Apart from retaining this public preflight
/// receipt, it performs no write, loads no identity and contacts no server.
#[cfg(feature = "discord-qa-shell")]
fn discord_qa_b6_startup_gate() -> Result<bool, String> {
    let preflight = broker::discord_qa_b6_startup_preflight();
    let startup_allowed = preflight.startup_allowed;
    let receipt = DiscordQaB6StartupReceipt {
        schema_version: 2,
        b6_preflight: &preflight,
        b6_proof_receipt: preflight.proof_receipt(),
    };
    let encoded = serde_json::to_vec_pretty(&receipt)
        .map_err(|_| "B6 startup preflight receipt could not be encoded".to_owned())?;
    std::fs::write(
        std::env::temp_dir().join(B6_PREFLIGHT_RECEIPT_FILE),
        encoded,
    )
    .map_err(|_| "B6 startup preflight receipt could not be retained".to_owned())?;
    Ok(startup_allowed)
}

#[cfg(windows)]
mod window_border;

mod native_attachment_transport;
mod native_discord_overlay;
mod native_image_viewer;
mod native_surface_capture;

use native_discord_overlay::OverlaySessionState;

#[allow(dead_code)]
#[path = "../../../src-tauri/src/screenshot.rs"]
mod screenshot;

#[cfg(feature = "discord-qa-shell")]
fn active_osl_capture_protection() -> runtime::ScreenshotProtection {
    // The disposable Discord QA shell contains a fixed synthetic probe and a
    // device-sealed throwaway identity. Let the VM harness capture OSL-owned
    // pixels so visual regressions can be reviewed without foregrounding RDP.
    // Ordinary builds retain capture exclusion below.
    runtime::ScreenshotProtection::Off
}

#[cfg(not(feature = "discord-qa-shell"))]
fn active_osl_capture_protection() -> runtime::ScreenshotProtection {
    runtime::ScreenshotProtection::On
}

const MAIN_WINDOW_CAPTURE_REFUSED_EVENT: &str = "hub-main-capture-protection-refused";

fn protect_main_window_or_hide(window: &tauri::WebviewWindow) -> bool {
    if screenshot::apply_to_window(window, active_osl_capture_protection()).is_ok() {
        return true;
    }
    let _ = window.hide();
    let _ = window.emit(MAIN_WINDOW_CAPTURE_REFUSED_EVENT, ());
    false
}

fn protect_main_webview_or_hide(webview: &tauri::Webview) -> bool {
    if screenshot::apply_to_webview(webview, active_osl_capture_protection()).is_ok() {
        return true;
    }
    let window = webview.window();
    let _ = window.hide();
    let _ = window.emit(MAIN_WINDOW_CAPTURE_REFUSED_EVENT, ());
    false
}

#[cfg(feature = "discord-qa-shell")]
fn qa_discord_overlay_stage(stage: &'static str) {
    let _ = std::fs::write(
        std::env::temp_dir().join("osl-discord-qa-overlay-stage.txt"),
        stage,
    );
}

#[cfg(feature = "discord-qa-shell")]
fn qa_discord_overlay_error(error: &str) {
    let _ = std::fs::write(
        std::env::temp_dir().join("osl-discord-qa-overlay-error.txt"),
        error,
    );
}

/// Where one lock-engage actually spent its wall clock.
///
/// The engage path is four unrelated costs in a row -- proving the host, driving
/// Discord's accessibility tree to calibrate the composer, capturing the verified
/// surface, and revealing it -- and they used to arrive as a single number, which
/// is how "the lock is slow" turned into guesswork about which of them to attack.
/// Each phase is named here so a run attributes the cost instead of implying it.
///
/// Nothing but fixed `&'static str` phase labels and whole elapsed milliseconds
/// can reach this, so no draft, conversation, geometry, rectangle or identifier
/// is recordable through it by construction. QA-shell only: production builds
/// compile every field and every call below away to nothing.
#[derive(Default)]
struct OverlayOpenTiming {
    #[cfg(feature = "discord-qa-shell")]
    started: Option<std::time::Instant>,
    #[cfg(feature = "discord-qa-shell")]
    phase_started: std::cell::Cell<Option<std::time::Instant>>,
    #[cfg(feature = "discord-qa-shell")]
    phases: std::cell::RefCell<Vec<(&'static str, u128)>>,
}

impl OverlayOpenTiming {
    #[cfg(feature = "discord-qa-shell")]
    fn started() -> Self {
        let now = std::time::Instant::now();
        Self {
            started: Some(now),
            phase_started: std::cell::Cell::new(Some(now)),
            phases: std::cell::RefCell::new(Vec::new()),
        }
    }

    #[cfg(not(feature = "discord-qa-shell"))]
    fn started() -> Self {
        Self::default()
    }

    /// Close the phase that has been running since the last mark.
    ///
    /// Takes `&self` deliberately. The open path threads one recorder through a
    /// blocking worker and an inner closure, and a `&mut self` mark forced a
    /// `mut` binding that production -- where every one of these compiles away --
    /// then warned about.
    #[cfg(feature = "discord-qa-shell")]
    fn mark(&self, phase: &'static str) {
        let now = std::time::Instant::now();
        if let Some(phase_started) = self.phase_started.replace(Some(now)) {
            if let Ok(mut phases) = self.phases.try_borrow_mut() {
                phases.push((phase, now.duration_since(phase_started).as_millis()));
            }
        }
    }

    #[cfg(not(feature = "discord-qa-shell"))]
    fn mark(&self, _phase: &'static str) {}

    /// The exact bytes `record` appends.
    ///
    /// Split out so the "labels and integers only" property is machine-checked
    /// rather than asserted in a comment: a phase label is a `&'static str` from
    /// this file and a cost is a `u128`, so there is no expression here that
    /// could carry a draft, a conversation or a rectangle into the trail.
    #[cfg(feature = "discord-qa-shell")]
    fn line(&self, elapsed_ms: u128, outcome: &'static str) -> String {
        use std::fmt::Write as _;

        let mut line = format!("total={elapsed_ms}ms");
        if let Ok(phases) = self.phases.try_borrow() {
            for (phase, millis) in phases.iter() {
                let _ = write!(line, " {phase}={millis}ms");
            }
        }
        let _ = writeln!(line, " outcome={outcome}");
        line
    }

    #[cfg(feature = "discord-qa-shell")]
    fn record(&self, outcome: &'static str) {
        let Some(started) = self.started else {
            return;
        };
        let line = self.line(started.elapsed().as_millis(), outcome);
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(std::env::temp_dir().join("osl-discord-qa-overlay-open-cost.txt"))
        {
            use std::io::Write as _;
            let _ = file.write_all(line.as_bytes());
        }
    }

    #[cfg(not(feature = "discord-qa-shell"))]
    fn record(&self, _outcome: &'static str) {}
}

#[cfg(all(test, feature = "discord-qa-shell"))]
mod overlay_open_timing_tests {
    use super::OverlayOpenTiming;

    /// Every phase the engage path can name, in the order the path marks them.
    /// A cost that lands in no phase is a cost nobody attributes, which is the
    /// state this recorder exists to end.
    #[test]
    fn every_engage_phase_is_named_once_in_path_order() {
        let timing = OverlayOpenTiming::started();
        for phase in [
            "registration",
            "host_target",
            "session_activated",
            "scope_bound",
            "calibrate",
            "surface_capture",
            "surface_published",
            "reveal",
            "renderer_ready",
        ] {
            timing.mark(phase);
        }
        let line = timing.line(1_460, "ok");
        assert!(line.starts_with("total=1460ms "), "{line}");
        assert!(line.ends_with(" outcome=ok\n"), "{line}");
        let phases = line
            .trim_end()
            .split(' ')
            .skip(1)
            .filter(|field| !field.starts_with("outcome="))
            .map(|field| field.split_once('=').expect("phase=cost").0)
            .collect::<Vec<_>>();
        assert_eq!(
            phases,
            [
                "registration",
                "host_target",
                "session_activated",
                "scope_bound",
                "calibrate",
                "surface_capture",
                "surface_published",
                "reveal",
                "renderer_ready",
            ]
        );
    }

    /// The trail may carry nothing but fixed labels and whole milliseconds. A
    /// draft, a conversation name, a rectangle or a hash reaching this file would
    /// be a disclosure, so the shape of every field is asserted rather than
    /// trusted.
    #[test]
    fn recorded_fields_are_only_labels_and_whole_milliseconds() {
        let timing = OverlayOpenTiming::started();
        timing.mark("calibrate");
        timing.mark("reveal");
        let line = timing.line(0, "error");
        for field in line.trim_end().split(' ') {
            let (label, value) = field.split_once('=').expect("label=value");
            assert!(
                label.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{label}"
            );
            if label == "outcome" {
                assert!(matches!(value, "ok" | "error"), "{value}");
                continue;
            }
            let millis = value.strip_suffix("ms").expect("ms suffix");
            assert!(
                !millis.is_empty() && millis.chars().all(|c| c.is_ascii_digit()),
                "{value}"
            );
        }
    }
}

/// Append-only QA breadcrumb for the protected-composer send path.
///
/// Only fixed `&'static str` labels are accepted, so no draft text, carrier
/// text, hash, measurement, identifier or error detail can ever reach this
/// file. It exists purely so one disposable QA run can name the exact hop that
/// stopped a send instead of leaving a silent no-op.
#[cfg(feature = "discord-qa-shell")]
fn qa_discord_send_stage(stage: &'static str) {
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("osl-discord-qa-send-stage.txt"))
    {
        let _ = file.write_all(stage.as_bytes());
        let _ = file.write_all(b"\n");
    }
}

/// Every label the disposable QA renderer may ask the native side to append.
/// The renderer's request is matched against this table and the matched
/// `&'static str` is what gets written, so renderer-supplied text is never
/// persisted even if the QA WebView is compromised.
#[cfg(feature = "discord-qa-shell")]
const QA_RENDERER_SEND_STAGES: [&str; 19] = [
    "renderer_keydown_observed",
    "renderer_enter_recognised",
    "renderer_enter_refocused_draft",
    "renderer_double_enter_handoff_keydown",
    "renderer_double_enter_handoff_keyup",
    "renderer_send_refused_not_ready",
    "renderer_send_refused_busy",
    "renderer_send_refused_empty_draft",
    "renderer_send_refused_too_large",
    "renderer_send_started",
    "renderer_send_refused_state_unavailable",
    "renderer_send_refused_marker_unavailable",
    "renderer_send_refused_covertext_off",
    "renderer_send_command_invoked",
    "renderer_send_command_rejected",
    "renderer_send_command_accepted",
    "renderer_send_refused_invalid_response",
    "renderer_send_failed",
    "renderer_send_complete",
];

/// Fixed receipt subject for the atomic QA send path. The user's draft is never
/// hashed onto disk here: a refusal or a completion is proven by the hop, not
/// by the message.
#[cfg(feature = "discord-qa-shell")]
const QA_ATOMIC_SEND_MARKER: &str = "OSL Discord QA atomic send";

/// Terminal, durable evidence for a send that stopped, so a refusal is never a
/// silent no-op. `phase` and `outcome` are fixed strings validated again by the
/// receipt writer. `error_detail` is an additional fixed, site-specific label
/// (e.g. `carrier_not_confirmed`) so the several distinct "post" phase
/// refusals below no longer collapse to one indistinguishable `error_class`.
/// `carrier_diagnostics` carries only structural booleans/labels about the
/// carrier placement, never carrier or draft text.
#[cfg(feature = "discord-qa-shell")]
fn qa_atomic_send_receipt(
    registration: &osl_privacy_hub::discord_qa_identity::RegistrationBarrierOutcome,
    phase: &'static str,
    outcome: &'static str,
    error: Option<&str>,
    error_detail: Option<&'static str>,
    carrier_diagnostics: Option<
        osl_privacy_hub::discord_qa_inbound_receipt::PostCarrierDiagnostics,
    >,
) {
    let _ = osl_privacy_hub::discord_qa_inbound_receipt::record_headless_send_phase_detailed(
        QA_ATOMIC_SEND_MARKER,
        registration.terminal_state,
        registration.keyserver_available,
        registration.identity_unchanged,
        phase,
        outcome,
        error,
        error_detail,
        carrier_diagnostics,
    );
}

/// Fixed label for a `DiscordCarrierStatus` variant, for receipts only. This
/// is the variant name itself (a structural fact), never carrier or message
/// text.
#[cfg(feature = "discord-qa-shell")]
fn discord_carrier_status_label(status: DiscordCarrierStatus) -> &'static str {
    match status {
        DiscordCarrierStatus::Sent => "Sent",
        DiscordCarrierStatus::CalibrationRequired => "CalibrationRequired",
        DiscordCarrierStatus::ContextChanged => "ContextChanged",
        DiscordCarrierStatus::ComposerUnavailable => "ComposerUnavailable",
        DiscordCarrierStatus::ComposerNotEmpty => "ComposerNotEmpty",
        DiscordCarrierStatus::PlacementRejected => "PlacementRejected",
        DiscordCarrierStatus::EnterRejected => "EnterRejected",
        DiscordCarrierStatus::CarrierUnconfirmed => "CarrierUnconfirmed",
        DiscordCarrierStatus::PlatformUnsupported => "PlatformUnsupported",
    }
}

/// Record one fixed send-path breadcrumb on behalf of the disposable QA
/// overlay renderer.
///
/// The renderer cannot reach the filesystem, so the keyboard and gesture hops
/// that live in the WebView would otherwise be invisible. This command accepts
/// no free text: an unknown label is refused outright.
#[cfg(feature = "discord-qa-shell")]
#[tauri::command]
fn record_native_discord_qa_send_stage(
    caller: tauri::WebviewWindow,
    stage: String,
) -> Result<(), String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err(
            "Only the trusted native Discord overlay may record its QA send stage".to_owned(),
        );
    }
    let allowed = QA_RENDERER_SEND_STAGES
        .iter()
        .copied()
        .find(|candidate| *candidate == stage.as_str())
        .ok_or_else(|| "That Discord QA send stage is not allowed".to_owned())?;
    qa_discord_send_stage(allowed);
    Ok(())
}

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
        active_osl_capture_protection()
    } else {
        runtime::ScreenshotProtection::Off
    };
    if screenshot::apply_to_window(&window, protection).is_ok() {
        return Ok(());
    }
    if enabled {
        let _ = window.hide();
        let _ = window.emit(MAIN_WINDOW_CAPTURE_REFUSED_EVENT, ());
    }
    Err("Windows capture resistance could not be changed".to_owned())
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
    if request.source == osl_privacy_hub::scrub_index::ScrubIndexSource::OslVisibleData {
        for selection in &request.selections {
            let service = osl_privacy_hub::services::service_kind_from_id(&selection.service_id)
                .ok_or_else(|| "Scrub account selection is invalid".to_owned())?;
            registry.require_owned(&owner, service, &selection.account_id)?;
        }
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

#[derive(Default)]
struct MainWindowLifecycleState {
    close_started: AtomicBool,
    harness_released: AtomicBool,
}

impl MainWindowLifecycleState {
    fn begin_close(&self) -> bool {
        !self.close_started.swap(true, Ordering::AcqRel)
    }

    /// Claims the one-and-only release of the harnessed native window.
    ///
    /// Several exit routes converge on that release (the main window's
    /// `CloseRequested` handler and the runtime's `ExitRequested` event, which
    /// the former itself triggers via `AppHandle::exit`). Exactly one of them
    /// performs it; the rest return immediately rather than paying the bounded
    /// host-slot wait again.
    fn begin_harness_release(&self) -> bool {
        !self.harness_released.swap(true, Ordering::AcqRel)
    }
}

/// Longest the event loop will be held while the harnessed native window is
/// released. Comfortably above `native_window_host`'s own worst case so the
/// normal path completes, and far below the 15s shutdown watchdog.
const HARNESSED_RELEASE_BUDGET: std::time::Duration = std::time::Duration::from_secs(8);

/// Detach OSL from the harnessed native window on the way out.
///
/// OSL adopts the operator's client window by owner-linking it and taking it
/// off the taskbar, so exiting without undoing that would strand a window with
/// no taskbar button and a dead owner. `shutdown_with_app` restores it first
/// and then asks it to close; `terminate` only restores.
///
/// A restart is not an exit as far as that window is concerned: the next OSL
/// process re-adopts the very same window, so a restart restores it and never
/// asks it to close.
fn release_harnessed_windows_for_exit(app: &tauri::AppHandle, restarting: bool) {
    // `try_state` rather than `state`: this also runs from the runtime's
    // `ExitRequested` event, which can in principle be delivered for an exit
    // that beats `setup` to managing these. A panic on the exit path would be
    // strictly worse than doing nothing, and doing nothing here is still safe
    // because no window can have been adopted yet either.
    let Some(lifecycle) = app.try_state::<MainWindowLifecycleState>() else {
        return;
    };
    if !lifecycle.begin_harness_release() {
        return;
    }
    let Some(host) = app.try_state::<NativeWindowHostState>() else {
        return;
    };
    let _ = if restarting {
        host.terminate()
    } else {
        host.shutdown_with_app()
    };
}

/// The same release, bounded, for callers that run on the event-loop thread.
///
/// Losing the race is safe rather than merely tolerable: the native host's
/// recovery guardian subprocess is still armed and restores the borrowed
/// window when this process dies, including when it dies by crashing.
fn release_harnessed_windows_bounded(app: &tauri::AppHandle, restarting: bool) {
    let (finished_send, finished_receive) = std::sync::mpsc::sync_channel::<()>(1);
    let worker = app.clone();
    std::thread::spawn(move || {
        release_harnessed_windows_for_exit(&worker, restarting);
        let _ = finished_send.send(());
    });
    let _ = finished_receive.recv_timeout(HARNESSED_RELEASE_BUDGET);
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
            // First moment the deletion outbox can be read at all: it is sealed
            // with the file storage key, which is why `scavenge_staging_on_startup`
            // is the wrong place for this. Detached on purpose — a deletion OSL
            // could not retry must never become a refusal to unlock, and a pass
            // can spend several network round trips against a dead store. See
            // `native_attachment_transport::drain_pending_deletions_detached`.
            native_attachment_transport::drain_pending_deletions_detached(&app);
            // Same reason, for the same reason it cannot block: the expiry and
            // receipt ledgers are sealed with the file storage key too, so this
            // is the first moment a sweep can do anything. `nudge` is a lock, an
            // increment and a notify — no I/O, no ledger lock, cannot fail — so
            // an unreadable ledger can never become a refusal to unlock. The
            // sweep itself happens on the tick thread.
            app.state::<LifecycleTickState>().nudge();
            let readiness = startup_gate::readiness_after_main(&app.state::<HubCoreState>());
            Ok(HubGateUnlockResult::unlocked(verification, readiness))
        }
        VerifiedGateRole::Stealth => {
            service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
            native_discord_overlay::clear_and_hide(&app);
            let _ = app.state::<NativeWindowHostState>().terminate();
            let _ = app.state::<MullvadWindowHostState>().restore();
            let _ = app.state::<BrowserCompanionState>().terminate();
            app.state::<HubBrokerState>().clear()?;
            startup_gate::enter_stealth_landing(&app.state::<HubCoreState>());
            Ok(HubGateUnlockResult::decoy(verification))
        }
        VerifiedGateRole::Duress => {
            service_host::desktop::shutdown(&app, &app.state::<ServiceHostState>()).await?;
            native_discord_overlay::clear_and_hide(&app);
            let _ = app.state::<NativeWindowHostState>().terminate();
            let _ = app.state::<MullvadWindowHostState>().restore();
            let _ = app.state::<BrowserCompanionState>().terminate();
            app.state::<HubBrokerState>().clear()?;
            Ok(HubGateUnlockResult::duress(verification))
        }
        VerifiedGateRole::Burn => {
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
async fn begin_protected_browser_import(
    app: tauri::AppHandle,
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
    browser_ids: Vec<BrowserImportId>,
) -> Result<ProtectedBrowserImportResult, String> {
    let _session = session.transition.lock().await;
    let owner = active_unlocked_osl_user_id(&core)?;
    let app_local_data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|_| "The OSL Firefox profile directory is unavailable".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        native_apps::begin_protected_browser_import(&app_local_data_dir, &owner, browser_ids)
    })
    .await
    .map_err(|_| "The protected browser import worker stopped unexpectedly".to_owned())?
}

#[tauri::command]
async fn finish_protected_browser_import(
    core: State<'_, HubCoreState>,
    session: State<'_, HubAccountSessionState>,
) -> Result<(), String> {
    let _session = session.transition.lock().await;
    let _owner = active_unlocked_osl_user_id(&core)?;
    native_apps::finish_protected_browser_import()
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
    // A consent receipt, never a request for consent: the caller only names the
    // destructive value after an explicit affirmative click. Tauri deserializes a
    // missing key to `None`, so a renderer that omits the field keeps exactly
    // today's borrow behaviour and can never quit anything.
    discord_takeover: Option<DiscordTakeover>,
) -> Result<NativeWindowHostResult, String> {
    let discord_takeover = discord_takeover.unwrap_or_default();
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
        let mut result = state.host_mode_with_takeover(
            app_id,
            &profile_root,
            &owner,
            parent,
            discord_session_mode,
            discord_takeover,
        );
        // Dedicated-only, so a refused takeover -- which can only happen in
        // `ExistingSession` -- is never retried here; it is surfaced to the
        // caller, which offers a plain borrow instead.
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
                result = state.host_mode_with_takeover(
                    app_id,
                    &profile_root,
                    &owner,
                    parent,
                    discord_session_mode,
                    discord_takeover,
                );
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

/// Read-only presence probe: would a takeover have to quit something the
/// operator is using? The UI calls this *before* it offers a takeover, and only
/// asks for consent when the answer is true.
#[tauri::command]
fn native_app_takeover_requires_consent(app: tauri::AppHandle, app_id: NativeAppId) -> bool {
    app.state::<NativeWindowHostState>()
        .takeover_requires_consent(app_id)
}

/// Read-only composer-marker probe used by the trusted header. Returning the
/// native state directly makes an unavailable marker close the QA send gate;
/// the renderer's fail-safe display default applies only until this call settles.
#[tauri::command]
fn discord_marker_available(app: tauri::AppHandle) -> bool {
    app.state::<NativeDiscordComposerState>()
        .marker_available()
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
    // Started before the first hop, so the trail measures the operator's whole
    // gesture rather than only the part that runs on the blocking worker.
    let timing = OverlayOpenTiming::started();
    #[cfg(feature = "discord-qa-shell")]
    if open {
        let registration_app = app.clone();
        let registration = tauri::async_runtime::spawn_blocking(move || {
            osl_privacy_hub::discord_qa_identity::wait_for_registered_transport(
                &registration_app.state::<HubCoreState>(),
            )
        })
        .await
        .map_err(|_| "OSL native Discord registration worker was interrupted".to_owned())?;
        if !registration.ready {
            return Err("The protected Discord identity is not registered yet".to_owned());
        }
        timing.mark("registration");
    }
    if !open {
        return tauri::async_runtime::spawn_blocking(move || {
            let composer = app.state::<NativeDiscordComposerState>();
            if composer.has_suspended_native_draft() {
                let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
                let scope_binding = native_discord_scope_binding(&app)?;
                composer.restore_suspended_native_draft(
                    &app.state::<NativeWindowHostState>(),
                    &owner,
                    &scope_binding,
                )?;
            }
            // The lock is ENCRYPTION ONLY. Lowering it must stop OSL owning
            // Discord's message box and nothing else -- it may not take the
            // operator's decrypted display away, because display is the eye's
            // business. The session, its guard and its retained surface
            // therefore stay alive while the eye is on, and the guard's next
            // pass simply stops including the composer in what OSL covers.
            //
            // Only a lock-off with nothing being displayed ends the session,
            // because then there is genuinely no protected pixel left to
            // guard. That is also the only path that may announce the surface
            // is gone. This must hold in EVERY build: the QA shell used to
            // hide the overlay and its shield unconditionally here, so the
            // eye went dark whenever the lock came down.
            //
            // `disengage_lock` lowers the lock and answers whether any protected
            // pixel is still on screen. It used to be read as an `if` whose body
            // was empty apart from a comment claiming the draft restore above had
            // already dealt with the surviving case -- which was never what kept
            // the composer covered. The guard computes presence from the lock, so
            // a surviving display needs nothing from this path at all; only the
            // empty case is this path's business.
            if !app.state::<OverlaySessionState>().disengage_lock() {
                #[cfg(feature = "discord-qa-shell")]
                native_discord_overlay::suspend_and_hide_for_qa_toggle(&app)?;
                #[cfg(not(feature = "discord-qa-shell"))]
                {
                    native_discord_overlay::clear_and_hide(&app);
                    app.state::<NativeDiscordComposerState>().clear();
                }
            }
            Ok(true)
        })
        .await
        .map_err(|_| "The native Discord overlay operation was interrupted".to_owned())?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let opened: Result<bool, String> = (|| {
            #[cfg(feature = "discord-qa-shell")]
            osl_privacy_hub::discord_qa_inbound_receipt::record_overlay_open_stage(
                "guarding", None,
            )?;
            let core = app.state::<HubCoreState>();
            let broker_state = app.state::<HubBrokerState>();
            let owner = active_unlocked_osl_user_id(&core)?;
            let current = require_current_context_host(&app, &core, &broker_state, &context_token)?;
            let native = app
                .state::<NativeWindowHostState>()
                .current_discord_service_host(&owner)?;
            if current != native || current.service_id != "discord" {
                return Err(
                    "The protected context is not the current native Discord host".to_owned(),
                );
            }
            let target = app
                .state::<NativeWindowHostState>()
                .discord_overlay_target(&owner)?;
            if target.generation != current.generation {
                return Err("The native Discord window changed before protection opened".to_owned());
            }
            timing.mark("host_target");
            let overlay_state = app.state::<OverlaySessionState>();
            #[cfg(feature = "discord-qa-shell")]
            let expected_overlay_host = current.clone();
            #[cfg(feature = "discord-qa-shell")]
            if overlay_state.qa_dormant_reuse(&context_token, &current)?
                == native_discord_overlay::QaDormantReuse::Changed
            {
                native_discord_overlay::discard_changed_qa_toggle(&app);
            }
            let epoch = overlay_state.activate(context_token, current)?;
            #[cfg(feature = "discord-qa-shell")]
            qa_discord_overlay_stage("session_activated");
            timing.mark("session_activated");
            let scope_binding = native_discord_scope_binding(&app)?;
            #[cfg(feature = "discord-qa-shell")]
            qa_discord_overlay_stage("scope_bound");
            timing.mark("scope_bound");
            let composer_calibration = app.state::<NativeDiscordComposerState>().calibrate(
                &app.state::<NativeWindowHostState>(),
                &owner,
                &scope_binding,
            );
            timing.mark("calibrate");
            if let Err(error) = composer_calibration {
                // Discord exposed no bounded, trustworthy accessibility composer.
                // Everything below this line needs one: the very next statement
                // asks for `verified_surface_bounds()`, which `clear()` has just
                // emptied, so this open was already going to fail.
                //
                // It used to fail with "The verified native composer surface is
                // unavailable" in production while the QA shell alone got the real
                // reason, because the `return Err(error)` sat inside the
                // `discord-qa-shell` block. Same failure, two different stories,
                // and the one the operator saw named a symptom two statements
                // downstream instead of the cause. The calibration error is the
                // honest answer in every build, so it is returned in every build.
                app.state::<NativeDiscordComposerState>().clear();
                #[cfg(feature = "discord-qa-shell")]
                {
                    qa_discord_overlay_stage("composer_calibration_failed");
                    qa_discord_overlay_error(&error);
                }
                return Err(error);
            }
            #[cfg(feature = "discord-qa-shell")]
            qa_discord_overlay_stage("composer_calibration_ready");
            let (native_surface_bounds, native_input_bounds) = app
                .state::<NativeDiscordComposerState>()
                .verified_surface_bounds()
                .ok_or_else(|| "The verified native composer surface is unavailable".to_owned())?;
            let expected_target = target;
            let native_surface = native_surface_capture::capture_verified_surface_guarded(
                [
                    native_surface_bounds.left,
                    native_surface_bounds.top,
                    native_surface_bounds.right,
                    native_surface_bounds.bottom,
                ],
                [
                    native_input_bounds.left,
                    native_input_bounds.top,
                    native_input_bounds.right,
                    native_input_bounds.bottom,
                ],
                app.state::<NativeDiscordComposerState>()
                    .verified_text_presentation(),
                || {
                    app.state::<NativeWindowHostState>()
                        .discord_overlay_target(&owner)
                        .is_ok_and(|current_target| {
                            native_discord_overlay::same_native_surface_target(
                                current_target,
                                expected_target,
                            )
                        })
                        && app
                            .state::<NativeDiscordComposerState>()
                            .verified_surface_bounds()
                            == Some((native_surface_bounds, native_input_bounds))
                },
            )?;
            timing.mark("surface_capture");
            let presentation = native_surface
                .presentation_bounds([
                    native_surface_bounds.left,
                    native_surface_bounds.top,
                    native_surface_bounds.right,
                    native_surface_bounds.bottom,
                ])
                .ok_or_else(|| "The adaptive native composer geometry is invalid".to_owned())?;
            app.state::<NativeDiscordComposerState>()
                .apply_adaptive_presentation_bounds(
                    native_surface_bounds,
                    AccessibilityBounds {
                        left: presentation[0],
                        top: presentation[1],
                        right: presentation[2],
                        bottom: presentation[3],
                    },
                )?;
            app.state::<native_surface_capture::NativeSurfaceCaptureState>()
                .replace(
                    native_surface_capture::NativeSurfaceKey {
                        session_epoch: epoch,
                        host_generation: target.generation,
                    },
                    native_surface,
                )?;
            #[cfg(feature = "discord-qa-shell")]
            qa_discord_overlay_stage("native_surface_captured");
            timing.mark("surface_published");
            native_discord_overlay::show(
                &app,
                target.rect,
                target.window,
                target.trusted_parent,
                epoch,
            )?;
            #[cfg(feature = "discord-qa-shell")]
            qa_discord_overlay_stage("overlay_shown");
            // The reveal itself, which is also where the open path pays for
            // acquiring keyboard focus. Attributed on its own so that cost is
            // never counted against the accessibility calibration above.
            timing.mark("reveal");
            #[cfg(feature = "discord-qa-shell")]
            {
                overlay_state.wait_until_ready(
                    epoch,
                    &expected_overlay_host,
                    std::time::Duration::from_secs(3),
                )?;
                osl_privacy_hub::discord_qa_inbound_receipt::record_overlay_open_stage(
                    "ready", None,
                )?;
                timing.mark("renderer_ready");
            }
            Ok(true)
        })();
        timing.record(if opened.is_ok() { "ok" } else { "error" });
        opened.map_err(|error| {
            let composer = app.state::<NativeDiscordComposerState>();
            let mut terminal_error = error;
            if composer.has_suspended_native_draft() {
                let restored =
                    active_unlocked_osl_user_id(&app.state::<HubCoreState>()).and_then(|owner| {
                        native_discord_scope_binding(&app).and_then(|scope_binding| {
                            composer
                                .restore_suspended_native_draft(
                                    &app.state::<NativeWindowHostState>(),
                                    &owner,
                                    &scope_binding,
                                )
                                .map(|_| ())
                        })
                    });
                if restored.is_err() {
                    terminal_error =
                        "OSL could not open protection or restore the saved Discord draft"
                            .to_owned();
                }
            }
            #[cfg(feature = "discord-qa-shell")]
            let _ = osl_privacy_hub::discord_qa_inbound_receipt::record_overlay_open_stage(
                "error",
                Some(&terminal_error),
            );
            native_discord_overlay::clear_and_hide(&app);
            terminal_error
        })
    })
    .await
    .map_err(|_| "The native Discord overlay operation was interrupted".to_owned())?
}

/// Encryption is the lock's whole job, so nothing may be encrypted or placed
/// while the lock is down. A session outlives a lock-off so the eye can keep
/// displaying, which makes this the gate that keeps the surviving session from
/// also being a surviving send capability.
fn require_engaged_lock(app: &tauri::AppHandle) -> Result<(), String> {
    if app.state::<OverlaySessionState>().lock_engaged() {
        return Ok(());
    }
    Err("Protected Discord encryption is switched off".to_owned())
}

#[cfg(any(test, feature = "discord-qa-shell"))]
fn canonical_native_visible_row_qa_build_hash(value: Option<&str>) -> Result<String, String> {
    let value = value.ok_or_else(|| "The QA build hash is unavailable".to_owned())?;
    if !matches!(value.len(), 40 | 64) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("The QA build hash is unavailable".to_owned());
    }
    Ok(value.to_ascii_lowercase())
}

#[cfg(all(feature = "discord-qa-shell", target_os = "windows"))]
fn trusted_native_visible_row_qa_caller_identity(
    caller: &tauri::WebviewWindow,
) -> Result<String, String> {
    let window = caller
        .hwnd()
        .map_err(|_| "The trusted OSL QA window is unavailable".to_owned())?
        .0 as isize;
    osl_privacy_hub::native_discord_adapter::native_visible_row_qa_osl_target_sha256(
        window,
        std::process::id(),
    )
    .ok_or_else(|| "The trusted OSL QA window identity is unavailable".to_owned())
}

#[cfg(all(feature = "discord-qa-shell", not(target_os = "windows")))]
fn trusted_native_visible_row_qa_caller_identity(
    _caller: &tauri::WebviewWindow,
) -> Result<String, String> {
    Err("Native visible-row runtime evidence requires Windows".to_owned())
}

/// Take one non-mutating, bounded runtime census from the real Windows native
/// visible-row producer.
///
/// No renderer value selects a window, scope, generation, identity or row. The
/// trusted main window can only ask; native state supplies every authority.
/// A zero-row or any-proof-missing result remains a refused receipt, never a
/// positive. Nothing here changes focus or sends input. After the real broker
/// authentication/orientation path returns, the command re-proves its lock and
/// context and atomically persists only the bounded nonsecret receipt.
#[cfg(feature = "discord-qa-shell")]
#[tauri::command]
async fn request_native_discord_visible_row_qa_receipt(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
) -> Result<broker::NativeVisibleRowRuntimeReceipt, String> {
    if caller.label() != "main" {
        return Err("Only the trusted OSL Privacy window may request native QA evidence".to_owned());
    }
    require_engaged_lock(&app)?;
    let owner = active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
    let (epoch, context_host) = require_overlay_context_snapshot(&app)?;
    let scope_binding = native_discord_scope_binding(&app)?;
    require_same_overlay_context(&app, epoch, &context_host)?;
    let build_hash =
        canonical_native_visible_row_qa_build_hash(option_env!("OSL_SOURCE_COMMIT"))?;
    let osl_target_identity_sha256 =
        trusted_native_visible_row_qa_caller_identity(&caller)?;

    let read_app = app.clone();
    let receipt = tauri::async_runtime::spawn_blocking(move || {
        broker::request_native_visible_row_runtime_receipt(
            &read_app.state::<NativeWindowHostState>(),
            &read_app.state::<HubCoreState>(),
            &read_app.state::<HubBrokerState>(),
            &owner,
            &scope_binding,
            &build_hash,
            &osl_target_identity_sha256,
            MAX_VISIBLE_CARRIER_ROWS,
        )
    })
    .await
    .map_err(|_| "The native visible-row QA request was interrupted".to_owned())??;

    // The native host callback re-proves its HWND/process/generation after the
    // producer returns. Re-prove the broker context and lock as well, so a
    // receipt from a superseded session never leaves this command.
    require_same_overlay_context(&app, epoch, &context_host)?;
    require_engaged_lock(&app)?;
    broker::persist_native_visible_row_runtime_receipt(&receipt)?;
    Ok(receipt)
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
    require_engaged_lock(&app)?;
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
    let carrier = plan.cover_text().ok_or_else(|| {
        "Discord carrier geometry could not be proven; nothing was placed".to_owned()
    })?;
    let overlay_state = app.state::<OverlaySessionState>();
    let carrier_placement = overlay_state.begin_carrier_placement()?;
    let receipt = composer.place_carrier(
        &app.state::<NativeWindowHostState>(),
        &owner,
        &scope_binding,
        mode,
        chars_per_second,
        &carrier,
    );
    drop(carrier_placement);
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
    /// Whether what the operator types is being encrypted, and therefore whether
    /// OSL owns a composer over Discord's real message box. Never a display
    /// question: the eye (`decrypt_display_enabled`) is the only one of those.
    lock_engaged: bool,
    native_surface: Option<native_surface_capture::NativeSurfaceCapture>,
    #[cfg(feature = "discord-qa-shell")]
    visible_carrier_rows: Vec<NativeDiscordCarrierRowDto>,
}

/// Where the protected overlay window is, how big it is, and the scale between
/// the physical screen pixels every accessibility read is measured in and the
/// CSS pixels the protected renderer lays out in.
///
/// Production, deliberately: this is the one conversion that turns "OSL knows
/// where a Discord row is" into "OSL can paint over it", and it used to exist
/// only inside a `discord-qa-shell` function. A shipping build could therefore
/// measure rows perfectly and had no way to express where any of them were.
struct ProtectedOverlayFrame {
    origin_x: i32,
    origin_y: i32,
    width: i32,
    height: i32,
    scale: f64,
}

fn protected_overlay_frame(app: &tauri::AppHandle) -> Option<ProtectedOverlayFrame> {
    let window = app.get_webview_window(native_discord_overlay::OVERLAY_LABEL)?;
    let origin = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let scale = window.scale_factor().ok()?;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some(ProtectedOverlayFrame {
        origin_x: origin.x,
        origin_y: origin.y,
        width: i32::try_from(size.width).ok()?,
        height: i32::try_from(size.height).ok()?,
        scale,
    })
}

/// Re-express one screen rectangle in the protected overlay window's own CSS
/// pixel space, or refuse.
///
/// Refusing is the honest answer and the common one: the overlay window is sized
/// to Discord's composer until the guard loop has grown it over the rows OSL is
/// painting, so the first read after a scope change legitimately answers `None`
/// for every row. The renderer must paint nothing rather than paint somewhere
/// wrong, and the window growing is itself a `resize` edge that asks again.
///
/// Screen coordinates never leave this function -- only the offset from the
/// overlay window's own origin does, which is exactly what the renderer can use
/// and reveals nothing about where anything is on the operator's desktop.
fn overlay_relative_row_rect(frame: &ProtectedOverlayFrame, bounds: [i32; 4]) -> Option<[f64; 4]> {
    let [screen_left, screen_top, screen_right, screen_bottom] = bounds;
    let left = screen_left.checked_sub(frame.origin_x)?;
    let top = screen_top.checked_sub(frame.origin_y)?;
    let width = screen_right.checked_sub(screen_left)?;
    let height = screen_bottom.checked_sub(screen_top)?;
    let right = left.checked_add(width)?;
    let bottom = top.checked_add(height)?;
    // Same bounds contract the QA carrier-row path already used, so a row that
    // is off the overlay window is refused rather than clamped: a clamped
    // rectangle is decrypted text over the wrong Discord row.
    if left < 0
        || top < 0
        || width <= 0
        || height < 12
        || right > frame.width
        || bottom > frame.height
    {
        return None;
    }
    Some([
        f64::from(left) / frame.scale,
        f64::from(top) / frame.scale,
        f64::from(width) / frame.scale,
        f64::from(height) / frame.scale,
    ])
}

#[cfg(feature = "discord-qa-shell")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeDiscordCarrierRowDto {
    message_id: String,
    native_locator_sha256: String,
    carrier_sha256: String,
    left_px: f64,
    top_px: f64,
    width_px: f64,
    height_px: f64,
    background_color: String,
    foreground_color: String,
    font_family: String,
    font_size_px: f64,
    font_weight: u16,
    line_height_px: f64,
    letter_spacing_px: f64,
    zoom: f64,
    density: f64,
}

#[cfg(feature = "discord-qa-shell")]
fn native_discord_visible_carrier_rows(
    app: &tauri::AppHandle,
    owner_osl_user_id: &str,
    scope_binding: &str,
    generation: u64,
) -> Vec<NativeDiscordCarrierRowDto> {
    let rows = app
        .state::<NativeDiscordComposerState>()
        .refresh_verified_sent_carriers(
            &app.state::<NativeWindowHostState>(),
            owner_osl_user_id,
            scope_binding,
            generation,
        );
    native_discord_carrier_row_dtos(app, rows)
}

#[cfg(feature = "discord-qa-shell")]
fn native_discord_carrier_row_dtos(
    app: &tauri::AppHandle,
    rows: Vec<VerifiedSentCarrierRow>,
) -> Vec<NativeDiscordCarrierRowDto> {
    let Some(frame) = protected_overlay_frame(app) else {
        return Vec::new();
    };
    let scale = frame.scale;
    rows.into_iter()
        .filter_map(|row| {
            let [left_px, top_px, width_px, height_px] = overlay_relative_row_rect(
                &frame,
                [
                    row.bounds.left,
                    row.bounds.top,
                    row.bounds.right,
                    row.bounds.bottom,
                ],
            )?;
            let font_size_px =
                f64::from(row.presentation.font_size_milli_points) / 1_000.0 * (96.0 / 72.0);
            let line_height_px = f64::from(row.presentation.line_height_milli_px) / 1_000.0 / scale;
            let zoom = font_size_px / 16.0;
            let density = line_height_px / font_size_px;
            if !font_size_px.is_finite()
                || !line_height_px.is_finite()
                || !zoom.is_finite()
                || !density.is_finite()
                || !(8.0..=128.0).contains(&font_size_px)
                || !(10.0..=128.0).contains(&line_height_px)
                || !(0.5..=4.0).contains(&zoom)
                || !(0.7..=3.0).contains(&density)
            {
                return None;
            }
            let [background_red, background_green, background_blue] =
                row.presentation.background_rgb;
            let [foreground_red, foreground_green, foreground_blue] =
                row.presentation.foreground_rgb;
            Some(NativeDiscordCarrierRowDto {
                message_id: row.message_id,
                native_locator_sha256: row.native_locator_sha256,
                carrier_sha256: row.carrier_sha256,
                left_px,
                top_px,
                width_px,
                height_px,
                background_color: format!(
                    "rgb({background_red} {background_green} {background_blue})"
                ),
                foreground_color: format!(
                    "rgb({foreground_red} {foreground_green} {foreground_blue})"
                ),
                font_family: row.presentation.font_family,
                font_size_px,
                font_weight: row.presentation.font_weight,
                line_height_px,
                letter_spacing_px: 0.0,
                zoom,
                density,
            })
        })
        .collect()
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
    #[cfg(feature = "discord-qa-shell")]
    let visible_carrier_rows = native_discord_visible_carrier_rows(
        app,
        &owner,
        &native_discord_scope_binding(app)?,
        stored_host.generation,
    );
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
        lock_engaged: state.lock_engaged(),
        native_surface: app
            .state::<native_surface_capture::NativeSurfaceCaptureState>()
            .current(native_surface_capture::NativeSurfaceKey {
                session_epoch: context_epoch,
                host_generation: stored_host.generation,
            }),
        #[cfg(feature = "discord-qa-shell")]
        visible_carrier_rows,
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
) -> Result<broker::PreparedNativeDiscordOverlayText, String> {
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        return Err("Only the trusted native Discord overlay may protect text".to_owned());
    }
    require_engaged_lock(&app)?;
    #[cfg(feature = "discord-qa-shell")]
    {
        let registration_app = app.clone();
        let registration = tauri::async_runtime::spawn_blocking(move || {
            let core = registration_app.state::<HubCoreState>();
            osl_privacy_hub::discord_qa_identity::wait_for_registered_transport(&core)
        })
        .await
        .map_err(|_| "OSL native Discord registration worker was interrupted".to_owned())?;
        if !registration.ready {
            return Err("The protected Discord identity is not registered yet".to_owned());
        }
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = require_overlay_context_snapshot(&app)?;
        let scope_binding = native_discord_scope_binding(&app)?;
        let visual = deidentify_prepared_visual_structure(&plaintext);
        let carrier = broker::prepare_native_discord_overlay_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            plaintext,
            view_once,
        )?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        // Exactly one String: the composer remembers the cover it will type, and
        // the receipt echoes that same cover so the renderer can label the Discord
        // row this message is about to become. The plaintext is already gone.
        app.state::<NativeDiscordComposerState>()
            .remember_prepared_visual_structure(
                &scope_binding,
                visual,
                carrier.flagtext.clone().unwrap_or_default(),
            );
        Ok(broker::PreparedNativeDiscordOverlayText {
            prepared: carrier.prepared,
            flagtext: carrier.flagtext,
        })
    })
    .await
    .map_err(|error| format!("OSL native overlay worker failed: {error}"))?
}

#[cfg(feature = "discord-qa-shell")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeDiscordQaAtomicText {
    prepared: broker::PreparedNativeDiscordOverlayText,
    carrier: DiscordCarrierReceipt,
    visible_carrier_row: Option<NativeDiscordCarrierRowDto>,
}

/// Disposable-QA transaction ordering for a manually typed protected message.
///
/// The encrypted OSL inbox commit happens before Discord is touched. This
/// prevents an OSL-generated flag from ever pointing at a missing protected
/// message. A later Discord refusal is returned as an honest OSL-only send so
/// the renderer clears the already-delivered draft instead of duplicating it.
#[cfg(feature = "discord-qa-shell")]
#[tauri::command]
async fn send_native_discord_qa_atomic_text(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    plaintext: String,
    view_once: bool,
    mode: DiscordCarrierMode,
    chars_per_second: u16,
    layout: Option<DiscordCarrierLayout>,
) -> Result<NativeDiscordQaAtomicText, String> {
    qa_discord_send_stage("send_command_entered");
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        qa_discord_send_stage("send_refused_caller_untrusted");
        return Err("Only the trusted native Discord overlay may protect text".to_owned());
    }
    if mode == DiscordCarrierMode::Compatibility
        && !ipc::tier_gate::is_paid_equivalent(&app.state::<HubCoreState>().osl)
    {
        qa_discord_send_stage("send_refused_tier");
        return Err("Compatibility typing requires OSL Pro".to_owned());
    }
    let registration = osl_privacy_hub::discord_qa_identity::registered_transport_now(
        &app.state::<HubCoreState>(),
    );
    if !registration.ready {
        qa_discord_send_stage("send_refused_registration");
        qa_atomic_send_receipt(
            &registration,
            "recipient_registration",
            "error",
            Some("The protected Discord identity is not registered yet"),
            None,
            None,
        );
        return Err("The protected Discord identity is not registered yet".to_owned());
    }
    qa_discord_send_stage("send_registration_ready");
    let _session = session.transition.lock().await;
    qa_discord_send_stage("send_session_locked");
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = match require_overlay_context_snapshot(&app) {
            Ok(context) => context,
            Err(error) => {
                qa_discord_send_stage("send_refused_context");
                qa_atomic_send_receipt(&registration, "context", "error", Some(&error), None, None);
                return Err(error);
            }
        };
        let owner = match active_unlocked_osl_user_id(&app.state::<HubCoreState>()) {
            Ok(owner) => owner,
            Err(error) => {
                qa_discord_send_stage("send_refused_owner");
                qa_atomic_send_receipt(
                    &registration,
                    "permission",
                    "error",
                    Some(&error),
                    None,
                    None,
                );
                return Err(error);
            }
        };
        let scope_binding = match native_discord_scope_binding(&app) {
            Ok(scope_binding) => scope_binding,
            Err(error) => {
                qa_discord_send_stage("send_refused_scope");
                qa_atomic_send_receipt(&registration, "scope", "error", Some(&error), None, None);
                return Err(error);
            }
        };
        qa_discord_send_stage("send_plaintext_accepted");
        let visual = deidentify_prepared_visual_structure(&plaintext);
        let composer = app.state::<NativeDiscordComposerState>();
        // Encryption now comes first unconditionally: the carrier row *is* this
        // message's wordbank flagtext, so it does not exist until the encrypted
        // copy has been committed. That matches the documented ordering — the
        // OSL inbox commit precedes any Discord contact — and a later carrier
        // refusal is reported as an honest OSL-only send below.
        let carrier_prepared = match broker::prepare_native_discord_overlay_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            plaintext,
            view_once,
        ) {
            Ok(prepared) => prepared,
            Err(error) => {
                qa_discord_send_stage("send_refused_encrypt");
                // QA-only: record *which* refusal this was. `errorDetail` on the
                // receipt is a fixed `&'static str`, so it cannot carry this
                // message, and without it "encrypt_rejected" names a whole family
                // of unrelated causes. These are OSL's own refusal strings --
                // never draft, plaintext, or conversation content.
                #[cfg(feature = "discord-qa-shell")]
                {
                    let _ = std::fs::write(
                        std::env::temp_dir().join("osl-discord-qa-encrypt-refusal.txt"),
                        error.as_bytes(),
                    );
                }
                qa_atomic_send_receipt(&registration, "encrypt", "error", Some(&error), None, None);
                return Err(error);
            }
        };
        qa_discord_send_stage("send_encryption_done");
        let prepared = carrier_prepared.prepared;
        // Public wire content, carried alongside the receipt and never alongside
        // the draft. It is the same String the composer is handed below.
        let flagtext = carrier_prepared.flagtext;
        if !prepared.person_to_person_e2ee || !prepared.delivered_to_osl_inbox {
            qa_discord_send_stage("send_refused_commit");
            let error = "The protected Discord message was not committed".to_owned();
            qa_atomic_send_receipt(&registration, "record", "error", Some(&error), None, None);
            return Err(error);
        }
        let failed_carrier = || DiscordCarrierReceipt {
            placed: false,
            enter_sent: false,
            status: DiscordCarrierStatus::ContextChanged,
            mode,
            compatibility_delay_ms: compatibility_delay_ms(chars_per_second),
        };
        composer.remember_prepared_visual_structure(
            &scope_binding,
            visual,
            flagtext.clone().unwrap_or_default(),
        );
        let plan = composer.take_prepared_carrier_plan(&scope_binding, layout);
        let Some(carrier_text) = plan.cover_text() else {
            qa_discord_send_stage("send_refused_carrier_geometry");
            qa_atomic_send_receipt(
                &registration,
                "post",
                "error",
                Some("Discord carrier geometry could not be proven"),
                Some("carrier_geometry_unproven"),
                None,
            );
            qa_discord_send_stage("send_receipt_written");
            return Ok(NativeDiscordQaAtomicText {
                prepared: broker::PreparedNativeDiscordOverlayText { prepared, flagtext },
                carrier: failed_carrier(),
                visible_carrier_row: None,
            });
        };
        qa_discord_send_stage("send_carrier_text_ready");
        if require_same_overlay_context(&app, context_epoch, &host).is_err() {
            qa_discord_send_stage("send_pre_placement_context_changed");
            qa_atomic_send_receipt(
                &registration,
                "post",
                "error",
                Some("The native Discord overlay context changed"),
                Some("overlay_context_changed"),
                None,
            );
            qa_discord_send_stage("send_receipt_written");
            return Ok(NativeDiscordQaAtomicText {
                prepared: broker::PreparedNativeDiscordOverlayText { prepared, flagtext },
                carrier: failed_carrier(),
                visible_carrier_row: None,
            });
        }
        let overlay_state = app.state::<OverlaySessionState>();
        let Ok(carrier_placement) = overlay_state.begin_carrier_placement() else {
            qa_discord_send_stage("send_refused_placement_lock");
            qa_atomic_send_receipt(
                &registration,
                "post",
                "error",
                Some("A native Discord carrier placement is already in flight"),
                Some("placement_already_in_flight"),
                None,
            );
            qa_discord_send_stage("send_receipt_written");
            return Ok(NativeDiscordQaAtomicText {
                prepared: broker::PreparedNativeDiscordOverlayText { prepared, flagtext },
                carrier: failed_carrier(),
                visible_carrier_row: None,
            });
        };
        qa_discord_send_stage("send_place_carrier_called");
        let mut carrier = composer.place_carrier(
            &app.state::<NativeWindowHostState>(),
            &owner,
            &scope_binding,
            mode,
            chars_per_second,
            &carrier_text,
        );
        drop(carrier_placement);
        qa_discord_send_stage(if carrier.placed {
            "send_carrier_typed"
        } else {
            "send_carrier_not_typed"
        });
        qa_discord_send_stage(if carrier.enter_sent {
            "send_enter_injected"
        } else {
            "send_enter_not_injected"
        });
        let context_unchanged = require_same_overlay_context(&app, context_epoch, &host).is_ok();
        let carrier_outcome = carrier
            .status
            .protected_send_outcome(carrier.placed, carrier.enter_sent);
        let carrier_sent =
            context_unchanged && carrier_outcome == DiscordProtectedSendOutcome::Sent;
        qa_discord_send_stage(if carrier_sent {
            "send_proof_confirmed"
        } else {
            "send_proof_failed"
        });
        let visible_carrier_row = if carrier_sent
            && composer.commit_pending_sent_carrier(&scope_binding, &prepared.message_id)
        {
            native_discord_carrier_row_dtos(
                &app,
                composer.verified_sent_carriers(&scope_binding, host.generation),
            )
            .into_iter()
            .find(|row| row.message_id == prepared.message_id)
        } else {
            None
        };
        if !context_unchanged || (carrier_sent && visible_carrier_row.is_none()) {
            carrier.status = DiscordCarrierStatus::ContextChanged;
        }
        if carrier.status == DiscordCarrierStatus::Sent {
            qa_atomic_send_receipt(&registration, "post", "ready", None, None, None);
        } else {
            // Structural facts (never carrier/message text) explaining which
            // conjunct of the post-send confirmation check failed:
            // `context_unchanged && carrier.status == Sent && carrier.placed
            // && carrier.enter_sent`.
            let carrier_diagnostics =
                osl_privacy_hub::discord_qa_inbound_receipt::PostCarrierDiagnostics {
                    carrier_status: discord_carrier_status_label(carrier.status),
                    carrier_placed: carrier.placed,
                    carrier_enter_sent: carrier.enter_sent,
                    overlay_context_unchanged: context_unchanged,
                };
            qa_atomic_send_receipt(
                &registration,
                "post",
                "error",
                Some("The native Discord carrier was not confirmed as sent"),
                Some("carrier_not_confirmed"),
                Some(carrier_diagnostics),
            );
        }
        qa_discord_send_stage("send_receipt_written");
        Ok(NativeDiscordQaAtomicText {
            prepared: broker::PreparedNativeDiscordOverlayText { prepared, flagtext },
            carrier,
            visible_carrier_row,
        })
    })
    .await
    .map_err(|error| format!("OSL native Discord atomic worker failed: {error}"))?
}

/// Send one fixed synthetic message through the real peer encryption and inbox
/// transport without depending on WebView2 descendant accessibility.
///
/// This command does not exist in production binaries. It accepts no text,
/// peer, path, network endpoint, or transport option from the renderer, and is
/// callable only by the already-authenticated native Discord overlay.
#[cfg(feature = "discord-qa-shell")]
#[tauri::command]
async fn send_native_discord_qa_probe(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<PreparedNativeOverlayText, String> {
    const QA_PROBE_PLAINTEXT: &str = "OSL Discord QA probe";
    if caller.label() != native_discord_overlay::OVERLAY_LABEL && caller.label() != "main" {
        return Err("Only a trusted Discord QA window may send its probe".to_owned());
    }
    let registration_app = app.clone();
    let registration = tauri::async_runtime::spawn_blocking(move || {
        let core = registration_app.state::<HubCoreState>();
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            "pending",
            core.osl.has_keyserver(),
            core.osl.has_identity(),
            false,
            false,
        )?;
        let outcome = osl_privacy_hub::discord_qa_identity::wait_for_registered_transport(&core);
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            outcome.terminal_state,
            outcome.keyserver_available,
            outcome.identity_unchanged,
            false,
            false,
        )?;
        Ok::<_, String>(outcome)
    })
    .await
    .map_err(|_| "OSL native Discord QA registration worker was interrupted".to_owned())??;
    if !registration.ready {
        return Err("Discord QA registration was not ready".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = match require_overlay_context_snapshot(&app) {
            Ok(context) => context,
            Err(error) => {
                osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
                    QA_PROBE_PLAINTEXT,
                    registration.terminal_state,
                    registration.keyserver_available,
                    registration.identity_unchanged,
                    false,
                    false,
                )?;
                return Err(error);
            }
        };
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            registration.terminal_state,
            registration.keyserver_available,
            registration.identity_unchanged,
            true,
            false,
        )?;
        let prepared = match broker::prepare_native_discord_overlay_text(
            &app.state::<HubCoreState>(),
            &app.state::<HubSecurityState>(),
            &app.state::<HubBrokerState>(),
            QA_PROBE_PLAINTEXT.to_owned(),
            false,
        ) {
            // The headless probe never touches Discord, so the carrier
            // flagtext this send produced is deliberately dropped here.
            Ok(carrier) => carrier.prepared,
            Err(error) => {
                osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
                    QA_PROBE_PLAINTEXT,
                    registration.terminal_state,
                    registration.keyserver_available,
                    registration.identity_unchanged,
                    true,
                    false,
                )?;
                return Err(error);
            }
        };
        if let Err(error) = require_same_overlay_context(&app, context_epoch, &host) {
            osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
                QA_PROBE_PLAINTEXT,
                registration.terminal_state,
                registration.keyserver_available,
                registration.identity_unchanged,
                false,
                true,
            )?;
            return Err(error);
        }
        if !prepared.person_to_person_e2ee || prepared.view_once || !prepared.delivered_to_osl_inbox
        {
            osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
                QA_PROBE_PLAINTEXT,
                registration.terminal_state,
                registration.keyserver_available,
                registration.identity_unchanged,
                true,
                true,
            )?;
            return Err("Discord QA probe did not produce an authenticated P2P receipt".to_owned());
        }
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            registration.terminal_state,
            registration.keyserver_available,
            registration.identity_unchanged,
            true,
            true,
        )?;
        osl_privacy_hub::discord_qa_inbound_receipt::record_outbound(
            QA_PROBE_PLAINTEXT,
            &prepared,
        )?;
        Ok(prepared)
    })
    .await
    .map_err(|_| "OSL native Discord QA probe worker was interrupted".to_owned())?
}

/// Compile-gated, renderer-input-free P2P QA lane. It derives the exact
/// current signed native Discord host and hash-pinned controller pairing inside
/// Rust, approves only that derived disposable scope, then sends one fixed
/// probe through the production peer encryption and authenticated inbox.
#[cfg(feature = "discord-qa-shell")]
fn run_headless_discord_qa_phase<T>(
    registration: &osl_privacy_hub::discord_qa_identity::RegistrationBarrierOutcome,
    phase: &'static str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    const QA_PROBE_PLAINTEXT: &str = "OSL Discord QA probe";
    osl_privacy_hub::discord_qa_inbound_receipt::record_headless_send_phase(
        QA_PROBE_PLAINTEXT,
        registration.terminal_state,
        registration.keyserver_available,
        registration.identity_unchanged,
        phase,
        "entered",
        None,
    )?;
    match operation() {
        Ok(value) => {
            osl_privacy_hub::discord_qa_inbound_receipt::record_headless_send_phase(
                QA_PROBE_PLAINTEXT,
                registration.terminal_state,
                registration.keyserver_available,
                registration.identity_unchanged,
                phase,
                "ready",
                None,
            )?;
            Ok(value)
        }
        Err(error) => {
            // The broker records exact fixed substages for the post operation,
            // including the typed keyserver status. Do not erase that receipt
            // with this intentionally coarser outer phase.
            if phase != "post" {
                osl_privacy_hub::discord_qa_inbound_receipt::record_headless_send_phase(
                    QA_PROBE_PLAINTEXT,
                    registration.terminal_state,
                    registration.keyserver_available,
                    registration.identity_unchanged,
                    phase,
                    "error",
                    Some(&error),
                )?;
            }
            Err(error)
        }
    }
}

#[cfg(feature = "discord-qa-shell")]
#[tauri::command]
async fn run_native_discord_headless_qa(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<PreparedNativeOverlayText, String> {
    const QA_PROBE_PLAINTEXT: &str = "OSL Discord QA probe";
    if caller.label() != "main" {
        return Err("Only the trusted Discord QA shell may run headless QA".to_owned());
    }
    let registration_app = app.clone();
    let registration = tauri::async_runtime::spawn_blocking(move || {
        let core = registration_app.state::<HubCoreState>();
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            "pending",
            core.osl.has_keyserver(),
            core.osl.has_identity(),
            false,
            false,
        )?;
        let outcome = osl_privacy_hub::discord_qa_identity::wait_for_registered_transport(&core);
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            outcome.terminal_state,
            outcome.keyserver_available,
            outcome.identity_unchanged,
            false,
            false,
        )?;
        Ok::<_, String>(outcome)
    })
    .await
    .map_err(|_| "OSL headless Discord QA registration worker was interrupted".to_owned())??;
    if !registration.ready {
        return Err("Discord QA registration was not ready".to_owned());
    }

    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let security_state = app.state::<HubSecurityState>();
        let broker_state = app.state::<HubBrokerState>();
        let owner = run_headless_discord_qa_phase(&registration, "host", || {
            active_unlocked_osl_user_id(&core)
        })?;
        let host = run_headless_discord_qa_phase(&registration, "host", || {
            let host = app
                .state::<NativeWindowHostState>()
                .current_discord_service_host(&owner)?;
            if host.service_id != "discord" {
                return Err("The exact native Discord QA host is unavailable".to_owned());
            }
            Ok(host)
        })?;
        let person_id = run_headless_discord_qa_phase(&registration, "pairing", || {
            let account_dir = osl_privacy_hub::discord_qa_identity::pairing_root_dir()?;
            osl_privacy_hub::discord_qa_identity::verified_pairing_person_id(&account_dir, &core)
        })?;
        let binding = run_headless_discord_qa_phase(&registration, "binding", || {
            security::manual_peer_binding(&core, person_id.clone())
        })?;
        let activated = run_headless_discord_qa_phase(&registration, "activation", || {
            broker::activate_owned_native_manual_peer_context(&broker_state, &owner, &host, binding)
        })?;
        run_headless_discord_qa_phase(&registration, "permission", || {
            security::set_manual_peer_scope_permission(
                &core,
                &security_state,
                &host.service_id,
                &host.account_id,
                person_id,
                activated.scope.clone(),
                true,
            )
        })?;
        run_headless_discord_qa_phase(&registration, "context", || {
            broker_state.validate_active_host(&activated.lease.context_token, &host)
        })?;
        run_headless_discord_qa_phase(&registration, "pre_send_drain", || {
            let opened =
                broker::drain_native_discord_overlay_text(&core, &security_state, &broker_state);
            osl_privacy_hub::discord_qa_inbound_receipt::record_poll(
                opened.as_ref().map_err(String::as_str),
            )?;
            let opened = opened?;
            osl_privacy_hub::discord_qa_inbound_receipt::record(&opened)?;
            Ok(())
        })?;
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            registration.terminal_state,
            registration.keyserver_available,
            registration.identity_unchanged,
            true,
            false,
        )?;
        let prepared = run_headless_discord_qa_phase(&registration, "post", || {
            // Headless: no Discord row, so the carrier flagtext is dropped.
            let prepared = broker::prepare_native_discord_overlay_text(
                &core,
                &security_state,
                &broker_state,
                QA_PROBE_PLAINTEXT.to_owned(),
                false,
            )?
            .prepared;
            let current = app
                .state::<NativeWindowHostState>()
                .current_discord_service_host(&owner)?;
            if current != host {
                return Err("The native Discord QA host changed during send".to_owned());
            }
            broker_state.validate_active_host(&activated.lease.context_token, &current)?;
            Ok(prepared)
        })?;
        if !prepared.person_to_person_e2ee || prepared.view_once || !prepared.delivered_to_osl_inbox
        {
            return Err("Discord QA probe did not produce an authenticated P2P receipt".to_owned());
        }
        osl_privacy_hub::discord_qa_inbound_receipt::record_send_stage(
            QA_PROBE_PLAINTEXT,
            registration.terminal_state,
            registration.keyserver_available,
            registration.identity_unchanged,
            true,
            true,
        )?;
        osl_privacy_hub::discord_qa_inbound_receipt::record_outbound(
            QA_PROBE_PLAINTEXT,
            &prepared,
        )?;
        Ok(prepared)
    })
    .await
    .map_err(|_| "OSL headless Discord QA send worker was interrupted".to_owned())?
}

#[cfg(feature = "discord-qa-shell")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscordHeadlessQaPoll {
    opened_count: usize,
    pending_view_once_count: usize,
    acknowledgment_count: usize,
    fetched: u32,
}

/// Drain the exact active native Discord friend context through the real
/// authenticated inbox without creating or showing a WebView.
#[cfg(feature = "discord-qa-shell")]
#[tauri::command]
async fn poll_native_discord_headless_qa(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
) -> Result<DiscordHeadlessQaPoll, String> {
    if caller.label() != "main" {
        return Err("Only the trusted Discord QA shell may poll headless QA".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let core = app.state::<HubCoreState>();
        let broker_state = app.state::<HubBrokerState>();
        let owner = active_unlocked_osl_user_id(&core)?;
        let context_token = broker_state.active_native_manual_context_token()?;
        let host = app
            .state::<NativeWindowHostState>()
            .current_discord_service_host(&owner)?;
        broker_state.validate_active_host(&context_token, &host)?;
        let opened = broker::drain_native_discord_overlay_text(
            &core,
            &app.state::<HubSecurityState>(),
            &broker_state,
        );
        osl_privacy_hub::discord_qa_inbound_receipt::record_poll(
            opened.as_ref().map_err(String::as_str),
        )?;
        let opened = opened?;
        let current = app
            .state::<NativeWindowHostState>()
            .current_discord_service_host(&owner)?;
        if current != host {
            return Err("The native Discord QA host changed during receive".to_owned());
        }
        broker_state.validate_active_host(&context_token, &current)?;
        osl_privacy_hub::discord_qa_inbound_receipt::record(&opened)?;
        Ok(DiscordHeadlessQaPoll {
            opened_count: opened.messages.len(),
            pending_view_once_count: opened.pending_view_once.len(),
            acknowledgment_count: opened.acknowledgments.len(),
            fetched: opened.fetched,
        })
    })
    .await
    .map_err(|_| "OSL headless Discord QA poll worker was interrupted".to_owned())?
}

/// Longest conversation identifier the renderer may name when it asks for a
/// rehydration. It is only ever compared and hashed, never parsed or trusted.
const MAX_REHYDRATE_SCOPE_BYTES: usize = 256;

/// Append-only QA breadcrumb for one transcript rehydration.
///
/// Fixed `&'static str` labels plus one count. No row text, cover, decrypted
/// message, identifier or error detail can reach this file, because none of them
/// is in scope at a call site.
#[cfg(feature = "discord-qa-shell")]
fn qa_discord_rehydrate_stage(stage: &'static str, count: Option<usize>) {
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("osl-discord-qa-rehydrate.txt"))
    {
        let _ = match count {
            Some(count) => writeln!(file, "{stage} count={count}"),
            None => writeln!(file, "{stage}"),
        };
    }
}

/// The two rectangles `overlay_relative_row_rect` compares, and nothing else.
///
/// Placement refusal is a pure geometry question -- is this row inside OSL's
/// window -- but the answer was a bare `None`, so "the frame is too small" and
/// "the row is somewhere else entirely" were the same observation. Geometry
/// only: no row text, no candidate, no locator ever reaches this file.
#[cfg(feature = "discord-qa-shell")]
fn qa_discord_rehydrate_geometry(frame: Option<&ProtectedOverlayFrame>, row: Option<[i32; 4]>) {
    use std::io::Write as _;
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("osl-discord-qa-rehydrate.txt"))
    else {
        return;
    };
    match (frame, row) {
        (Some(frame), Some([left, top, right, bottom])) => {
            let _ = writeln!(
                file,
                "rehydrate_place_geometry frame_x={} frame_y={} frame_w={} frame_h={} \
                 row_l={} row_t={} row_r={} row_b={} rel_l={} rel_t={} rel_r={} rel_b={}",
                frame.origin_x,
                frame.origin_y,
                frame.width,
                frame.height,
                left,
                top,
                right,
                bottom,
                left - frame.origin_x,
                top - frame.origin_y,
                right - frame.origin_x,
                bottom - frame.origin_y,
            );
        }
        _ => {
            let _ = writeln!(
                file,
                "rehydrate_place_geometry frame={} row={}",
                frame.is_some(),
                row.is_some()
            );
        }
    }
}

#[cfg(not(feature = "discord-qa-shell"))]
fn qa_discord_rehydrate_geometry(_frame: Option<&ProtectedOverlayFrame>, _row: Option<[i32; 4]>) {}

#[cfg(not(feature = "discord-qa-shell"))]
fn qa_discord_rehydrate_stage(stage: &'static str, count: Option<usize>) {
    let _ = (stage, count);
}

/// Name one refusal on the way out, and refuse exactly as before.
///
/// Every pre-read leg of `rehydrate_native_discord_overlay_history` used to
/// return its `Err` straight to the renderer, which journals it in memory only.
/// Six preconditions could therefore stop the eye with no trace on disk at all,
/// which is why the very first live investigation of this feature had no artifact
/// to read. This changes no verdict and no message -- it only makes the verdict
/// observable.
///
/// PRIVACY: `stage` is a fixed label chosen at the call site. The error VALUE is
/// passed through untouched and is never written, so no backend sentence, scope
/// name or identifier can reach the trail through it.
fn qa_named_rehydrate_refusal<T>(
    stage: &'static str,
    result: Result<T, String>,
) -> Result<T, String> {
    if result.is_err() {
        qa_discord_rehydrate_stage(stage, None);
    }
    result
}

/// What one claimed transcript read answers.
///
/// `read` is false exactly when the floor refused this claim, and then
/// `retry_after_ms` says how much of the floor is left. Those two exist so a
/// refusal is distinguishable from "this conversation genuinely has no rows":
/// they were the same empty vector before, which meant a refused scroll edge
/// silently left decrypted text sitting over the wrong Discord rows until the
/// operator happened to cause another edge.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RehydratedNativeDiscordTranscriptDto {
    read: bool,
    retry_after_ms: u64,
    rows: Vec<broker::RehydratedNativeDiscordRowDto>,
}

/// Put the conversation back on screen, and keep OSL's decrypted text on the
/// rows it belongs to.
///
/// The protected overlay's capture shield is opaque across Discord's whole
/// message-row band while OSL's own transcript is populated by live append only,
/// so on a fresh session -- or on any ordinary, never-protected Discord message
/// -- the operator is shown black where their history should be. Every protected
/// Discord row is self-describing (the row text *is* the flag and the pointer)
/// and ordinary rows are simply themselves, so one bounded read of the rows
/// Discord is already displaying restores both -- and now returns where each of
/// them is, so the eye can paint over any row OSL can decrypt rather than only
/// over the ones this client happened to send.
///
/// TRIGGERS: renderer edges only -- a scope change, a scroll, a new message, a
/// re-measure. NEVER a timer, and this command has no way to become one:
/// `begin_rehydrate` refuses a same-scope repeat inside
/// `REHYDRATE_MIN_INTERVAL_MS`, and it only ever refuses -- nothing in this
/// process asks for a read when that interval elapses. Per-poll row reading is
/// what froze this machine for 19,207 ms.
///
/// LOCKS: the whole read runs in `spawn_blocking`, and the accessibility work
/// inside it runs on a further detached thread with its own budget. No OSL lock
/// is held across a cross-process call, and OSL's UI thread is never the one
/// waiting on Discord's.
#[tauri::command]
async fn rehydrate_native_discord_overlay_history(
    app: tauri::AppHandle,
    caller: tauri::WebviewWindow,
    session: State<'_, HubAccountSessionState>,
    scope: String,
) -> Result<RehydratedNativeDiscordTranscriptDto, String> {
    // First, unconditionally, before any refusal can happen: the renderer asked.
    // Nothing else in this process can say that, and without it every observation
    // below is ambiguous between "the backend refused" and "nobody called".
    qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_ENTERED, None);
    if caller.label() != native_discord_overlay::OVERLAY_LABEL {
        qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_REFUSED_CALLER, None);
        return Err(
            "Only the trusted native Discord overlay may rehydrate its transcript".to_owned(),
        );
    }
    if scope.is_empty() || scope.len() > MAX_REHYDRATE_SCOPE_BYTES {
        qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_REFUSED_SCOPE, None);
        return Err("The protected conversation could not be identified".to_owned());
    }
    let _session = session.transition.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let (context_epoch, host) = qa_named_rehydrate_refusal(
            native_discord_overlay::REHYDRATE_CONTEXT_UNAVAILABLE,
            require_overlay_context_snapshot(&app),
        )?;
        let core = app.state::<HubCoreState>();
        let owner = qa_named_rehydrate_refusal(
            native_discord_overlay::REHYDRATE_OWNER_UNAVAILABLE,
            active_unlocked_osl_user_id(&core),
        )?;
        // The authority is always OSL's own binding. The renderer's `scope` only
        // widens the edge, so switching the surface the overlay is showing counts
        // as a change even when the native binding has not moved.
        let scope_binding = qa_named_rehydrate_refusal(
            native_discord_overlay::REHYDRATE_SCOPE_BINDING_UNAVAILABLE,
            native_discord_scope_binding(&app),
        )?;
        let edge = format!("{scope_binding}\u{1f}{scope}");
        // The gate takes, compares and releases its own lock entirely inside this
        // call. Nothing is held across the accessibility read below.
        let admission = app
            .state::<NativeDiscordComposerState>()
            .begin_rehydrate(&edge);
        if !admission.admitted {
            qa_discord_rehydrate_stage(
                osl_privacy_hub::native_discord_adapter::REHYDRATE_SKIPPED_NO_SCOPE_CHANGE,
                None,
            );
            return Ok(RehydratedNativeDiscordTranscriptDto {
                read: false,
                retry_after_ms: admission.retry_after_ms,
                rows: Vec::new(),
            });
        }
        // The one leg that reaches Discord. It is refused outright whenever any
        // other accessibility operation holds the single non-blocking gate, and
        // that refusal is a bare `Err` with no label of its own -- so it needs one
        // here or the whole read disappears without trace.
        let rows = qa_named_rehydrate_refusal(
            native_discord_overlay::REHYDRATE_READ_UNAVAILABLE,
            osl_privacy_hub::native_discord_adapter::read_visible_message_rows(
                &app.state::<NativeWindowHostState>(),
                &owner,
                &scope_binding,
                MAX_VISIBLE_CARRIER_ROWS,
            ),
        )?;
        qa_named_rehydrate_refusal(
            native_discord_overlay::REHYDRATE_CONTEXT_CHANGED,
            require_same_overlay_context(&app, context_epoch, &host),
        )?;
        let rehydrated = broker::rehydrate_native_discord_overlay_history(
            &core,
            &app.state::<HubBrokerState>(),
            &scope_binding,
            host.generation,
            rows,
        )?;
        qa_named_rehydrate_refusal(
            native_discord_overlay::REHYDRATE_CONTEXT_CHANGED,
            require_same_overlay_context(&app, context_epoch, &host),
        )?;
        let counts = rehydrated.counts;
        let rehydrated = rehydrated.rows;
        let decoded = rehydrated
            .iter()
            .filter(|row| row.plaintext.is_some())
            .count();
        qa_discord_rehydrate_stage(
            osl_privacy_hub::native_discord_adapter::REHYDRATE_ROWS_DECODED,
            Some(decoded),
        );
        qa_discord_rehydrate_stage(
            osl_privacy_hub::native_discord_adapter::REHYDRATE_ROWS_UNDECODABLE,
            Some(rehydrated.len().saturating_sub(decoded)),
        );
        // Why each undecodable row was undecodable. Counts only, in a fixed
        // order, so a screenful that produced no plaintext says which of "no
        // protected rows here", "the cipher store is unreachable" and "a proof
        // refused it" actually happened.
        for (stage, count) in [
            (broker::REHYDRATE_DECODE_ROWS, counts.rows),
            // Immediately after the row count and before every verdict, because
            // it is what makes the verdicts readable: `pointer_absent` with a
            // healthy candidate count means "no OSL pointer in these rows", and
            // `pointer_absent` with zero candidates means the decoder was never
            // shown the body at all. Those were one indistinguishable number on
            // the 2026-07-26 live run.
            (broker::REHYDRATE_DECODE_CANDIDATES, counts.candidates),
            (broker::REHYDRATE_DECODE_DISPLAY_OFF, counts.display_off),
            (
                broker::REHYDRATE_DECODE_BUDGET_EXHAUSTED,
                counts.budget_exhausted,
            ),
            (
                broker::REHYDRATE_DECODE_POINTER_ABSENT,
                counts.pointer_absent,
            ),
            (
                broker::REHYDRATE_DECODE_POINTER_BLOB_GONE,
                counts.pointer_blob_gone,
            ),
            (
                broker::REHYDRATE_DECODE_STORE_UNREACHABLE,
                counts.store_unreachable,
            ),
            (broker::REHYDRATE_DECODE_REFUSED, counts.refused),
            (
                broker::REHYDRATE_DECODE_VIEW_ONCE_SKIPPED,
                counts.view_once_skipped,
            ),
            (broker::REHYDRATE_DECODE_PLAINTEXT, counts.plaintext),
        ] {
            qa_discord_rehydrate_stage(stage, Some(count));
        }
        // The last step of the placement path, and the one that used to be
        // missing entirely: turn each row's screen rectangle into a rectangle
        // inside OSL's own protected window, so the renderer can put this row's
        // decrypted text exactly over this row. A row the window does not
        // currently contain answers `None` and is simply not painted.
        let frame = protected_overlay_frame(&app);
        if frame.is_none() {
            // Not an error and not a guess: with no frame every row answers
            // `None` and the eye paints nothing. Worth naming, because the
            // symptom is identical to "nothing decoded".
            qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_FRAME_ABSENT, None);
        }
        // One decoded row is enough to answer the placement question: every row
        // is measured against the same frame, so a frame that refuses one
        // refuses all of them for the same reason.
        qa_discord_rehydrate_geometry(
            frame.as_ref(),
            rehydrated
                .iter()
                .find(|row| row.plaintext.is_some())
                .and_then(|row| row.bounds),
        );
        let placed = rehydrated
            .iter()
            .filter(|row| {
                row.plaintext.is_some()
                    && frame
                        .as_ref()
                        .zip(row.bounds)
                        .and_then(|(frame, bounds)| overlay_relative_row_rect(frame, bounds))
                        .is_some()
            })
            .count();
        let decoded = rehydrated
            .iter()
            .filter(|row| row.plaintext.is_some())
            .count();
        let unplaceable = decoded.saturating_sub(placed);
        let rows: Vec<broker::RehydratedNativeDiscordRowDto> = rehydrated
            .into_iter()
            .map(|row| {
                let relative_rect = frame
                    .as_ref()
                    .zip(row.bounds)
                    .and_then(|(frame, bounds)| overlay_relative_row_rect(frame, bounds));
                broker::rehydrated_native_discord_row_dto(row, relative_rect)
            })
            .collect();
        // The last two counts before the wire, and the pair that separates
        // "OSL opened nothing" from "OSL opened rows it is not over yet". Only a
        // row with BOTH is ever painted; see `RehydratedNativeDiscordRowDto`.
        qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_ROWS_PLACED, Some(placed));
        qa_discord_rehydrate_stage(
            native_discord_overlay::REHYDRATE_ROWS_UNPLACEABLE,
            Some(unplaceable),
        );
        qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_SHIPPED, Some(rows.len()));
        Ok(RehydratedNativeDiscordTranscriptDto {
            read: true,
            retry_after_ms: 0,
            rows,
        })
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
        );
        #[cfg(feature = "discord-qa-shell")]
        osl_privacy_hub::discord_qa_inbound_receipt::record_poll(
            opened.as_ref().map_err(String::as_str),
        )?;
        let opened = opened?;
        require_same_overlay_context(&app, context_epoch, &host)?;
        #[cfg(feature = "discord-qa-shell")]
        osl_privacy_hub::discord_qa_inbound_receipt::record(&opened)?;
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
    screenshot::apply_to_window(&caller, active_osl_capture_protection())
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
    screenshot::apply_to_window(&caller, active_osl_capture_protection()).map_err(|_| {
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
    screenshot::apply_to_window(&caller, active_osl_capture_protection()).map_err(|_| {
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
    let activated: Result<ManualPeerContextLease, String> = (|| {
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
    })();
    #[cfg(feature = "discord-qa-shell")]
    if let Err(error) = &activated {
        let _ = osl_privacy_hub::discord_qa_inbound_receipt::record_overlay_open_stage(
            "error",
            Some(error),
        );
    }
    activated
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
async fn remove_hub_friend(
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    person_id: String,
) -> Result<RemoveFriendResult, String> {
    let _session = session.transition.lock().await;
    security::remove_friend(&core, &security_state, person_id)
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

/// Deliberately widen or withdraw one verified friend's person-level reach.
///
/// Separate from `set_active_hub_friend_permission` so that widening trust can
/// never be a side effect of approving a chat. It re-runs the same checks as an
/// approval: the live signed host generation, the context capability, and the
/// exact verified friend bound to that context.
#[tauri::command]
async fn set_active_hub_friend_reach(
    app: tauri::AppHandle,
    broker_state: State<'_, HubBrokerState>,
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    person_id: String,
    broadened: bool,
) -> Result<PersonDto, String> {
    let _session = session.transition.lock().await;
    let active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    let person_id = broker_state.manual_reach_target(&context_token, &person_id)?;
    let person = security::set_friend_scope_reach(
        &core,
        &security_state,
        &active.service_id,
        &active.account_id,
        person_id,
        broadened,
    )?;
    let _still_active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    Ok(person)
}

/// Revoke exactly one approval OSL already recorded for the active verified
/// friend. The scope key is matched against that friend's own recorded entries,
/// so the renderer cannot name a conversation OSL never approved, and the
/// revocation applies immediately even while their reach is broadened.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
async fn revoke_active_hub_friend_scope(
    app: tauri::AppHandle,
    broker_state: State<'_, HubBrokerState>,
    core: State<'_, HubCoreState>,
    security_state: State<'_, HubSecurityState>,
    session: State<'_, HubAccountSessionState>,
    context_token: String,
    person_id: String,
    storage_key: String,
) -> Result<PersonDto, String> {
    let _session = session.transition.lock().await;
    let _active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    let person_id = broker_state.manual_reach_target(&context_token, &person_id)?;
    let person =
        security::revoke_friend_scope_entry(&core, &security_state, person_id, storage_key)?;
    let _still_active = require_current_context_host(&app, &core, &broker_state, &context_token)?;
    Ok(person)
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

/// Headless self-test driver for the protected Discord send path.
///
/// WHY THIS EXISTS: every existing way to prove the send path works needs a
/// human (or synthetic input) to click the composer and press Enter. Synthetic
/// input against WebView2 is unreliable and has already put plaintext into a
/// real conversation. This module drives the exact same native command the
/// protected renderer drives -- `send_native_discord_qa_atomic_text` -- with no
/// mouse, no keyboard, and no WebView interaction at all, and writes one
/// machine-diffable verdict.
///
/// TRIGGER: a watched file, `%TEMP%\osl-qa-selftest.request`, polled every
/// [`POLL_INTERVAL`]. A start-up CLI flag was rejected: the scenario needs an
/// adopted Discord window and an engaged lock, both of which the operator
/// establishes *after* launch, so a start-up flag would either fire into an
/// unready app or wait unbounded at start-up; and it could not be re-run
/// without relaunching the process, which breaks repeatability. The trigger
/// file also composes with `tauri-plugin-single-instance`, which would swallow
/// a second process's argv.
///
/// SCOPE: this module is entirely inside `#[cfg(feature = "discord-qa-shell")]`
/// and therefore cannot exist in a production binary. It is deliberately not a
/// Tauri command: no renderer, page or IPC caller can reach it.
///
/// SAFETY OF THE PROBE: exactly one fixed plaintext ([`PROBE_PLAINTEXT`]) is
/// sent per invocation, into whichever conversation is already open. Nothing
/// here selects a conversation, opens a context, engages the lock, or types
/// into anything OSL does not already own. Every wait is bounded, and every
/// missing precondition is a written verdict rather than a hang.
///
/// VERBS: the rendezvous used to drive exactly one thing -- the send -- which
/// meant a two-identity rig could *observe* the receiving instance but never
/// *drive* it, so drain, reveal, rehydrate and the eye were only ever provable
/// if a human happened to click at the right moment. It now drives six verbs,
/// named in the trigger file body:
///
/// | verb | drives | mutates |
/// |---|---|---|
/// | `status` | nothing | no |
/// | `host` | `host_native_app_window` | yes |
/// | `send` | `send_native_discord_qa_atomic_text` | yes |
/// | `drain` | `open_native_discord_overlay_text` | yes |
/// | `rehydrate` | `rehydrate_native_discord_overlay_history` | yes |
/// | `reveal-view-once` | a drain, then `reveal_native_discord_overlay_view_once` | yes |
///
/// Every one of those is the *exact* entry point the protected renderer already
/// calls, reached the same way [`drive_probe_send`] reaches the send command.
/// None of them is a QA-only path into the broker: a parallel implementation is
/// how this surface produced QA/production divergences before.
///
/// ADDRESSING: see [`spawn_trigger_watcher`]. The unqualified trigger is a
/// global rendezvous whose first consumer wins it, which two instances race
/// for, so each instance additionally polls a trigger addressed to its own
/// bundle identifier and answers it into an equally addressed verdict.
#[cfg(feature = "discord-qa-shell")]
mod qa_selftest {
    use super::*;
    use osl_privacy_hub::qa_selftest_request::{
        instance_file_token, not_ready_refusal, parse_request, readiness_criterion_is_graded,
        DrainReport, HostAction, HostReport, ParsedRequest, RehydrateReport, RevealReport,
        RevealTarget, SelftestRequest, Verb,
    };
    use osl_privacy_hub::native_window_host::NativeWindowHostStatus;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    /// The one and only string this driver will ever send.
    const PROBE_PLAINTEXT: &str = "OSL Discord QA probe";
    const TRIGGER_FILE: &str = "osl-qa-selftest.request";
    const VERDICT_FILE: &str = "osl-qa-selftest.json";
    /// Written first, then renamed onto the verdict path, so a harness never
    /// reads a half-written verdict.
    const VERDICT_PARTIAL_SUFFIX: &str = ".partial";
    /// The `{token}` in each of these is [`instance_file_token`] of this
    /// process's Tauri bundle identifier -- the same identifier the two-identity
    /// harness already reads off the single-instance marker window class, and
    /// the only thing that tells two OSL builds apart.
    const ADDRESSED_TRIGGER_FORMAT: &str = "osl-qa-selftest.{token}.request";
    const ADDRESSED_VERDICT_FORMAT: &str = "osl-qa-selftest.{token}.json";
    const ADDRESSED_DECLINE_FORMAT: &str = "osl-qa-selftest.{token}.declined.json";
    /// The append-only breadcrumb trail `qa_discord_send_stage` writes.
    const SEND_STAGE_TRAIL_FILE: &str = "osl-discord-qa-send-stage.txt";
    /// Mirrors `osl_privacy_hub::discord_qa_inbound_receipt::SEND_STAGE_RECEIPT_FILE`
    /// and its three siblings, which are `pub(crate)` to the library and so
    /// cannot be named from this binary crate. If those constants ever change,
    /// change these with them.
    const SEND_STAGE_RECEIPT_FILE: &str = "discord-qa-send-stage-receipt.json";
    const INBOUND_RECEIPT_FILE: &str = "discord-qa-inbound-receipt.json";
    const INBOUND_POLL_RECEIPT_FILE: &str = "discord-qa-inbound-poll-receipt.json";
    const OUTBOUND_RECEIPT_FILE: &str = "discord-qa-outbound-receipt.json";

    /// The scope this driver names when it asks for a rehydration.
    ///
    /// The authority for *what* is read is always OSL's own native binding; the
    /// renderer's `scope` only widens the edge the same-scope floor is keyed on.
    /// A fixed private value therefore reads the real conversation while sharing
    /// no floor with the renderer's own edges, so driving this verb can neither
    /// be starved by the renderer nor starve it. Two of these back to back
    /// inside `REHYDRATE_MIN_INTERVAL_MS` are refused by the floor, which is
    /// reported as `read: false` with the remaining `retryAfterMs` -- a rate
    /// limit, never an empty conversation.
    const REHYDRATE_SCOPE: &str = "osl-qa-selftest";

    /// The largest trigger body this driver will read. A request is a handful of
    /// fixed labels; anything larger is not one, and reading it unbounded would
    /// hand an arbitrary file the ability to stall the watcher.
    const MAX_REQUEST_BYTES: u64 = 8 * 1024;

    const POLL_INTERVAL: Duration = Duration::from_millis(500);
    /// How long one triggered run will wait for an adopted Discord window, an
    /// unlocked identity, an engaged lock and a live composer before giving up
    /// and reporting `not-ready`. Never unbounded.
    const READINESS_TIMEOUT: Duration = Duration::from_secs(60);
    /// How long one triggered run will wait for the send command to return
    /// before reporting `send-timeout`. The command keeps running; the
    /// in-flight latch below stops a second probe from overlapping it.
    const SEND_TIMEOUT: Duration = Duration::from_secs(45);
    /// Hosting enumerates native windows and runs its host operation on a
    /// blocking worker. Dedicated Discord may additionally use the production
    /// command's bounded install wait, so this has its own honest ceiling.
    const HOST_TIMEOUT: Duration = Duration::from_secs(240);
    /// The outer bound on one whole invocation, enforced by the watcher thread
    /// against the run thread.
    ///
    /// Reading the composer's visibility or handle is a round trip to Tauri's
    /// event loop, and reading the adopted Discord window touches the same
    /// native host the UI thread does. Neither has a timeout of its own, so a
    /// wedged UI thread could otherwise stall a run forever and the harness
    /// would see no verdict at all -- which is the one outcome this driver
    /// exists to make impossible. Comfortably larger than the two bounded
    /// waits it contains, so it only ever fires on a genuine stall.
    const RUN_TIMEOUT: Duration = Duration::from_secs(150);
    /// How long one drive of a receive-side verb will wait before reporting
    /// `verb-timeout`. A drain that has to resolve cover pointers pays a
    /// cipher-store fetch per row and the store allows each one 15 seconds, so
    /// this is deliberately larger than [`SEND_TIMEOUT`].
    const VERB_TIMEOUT: Duration = Duration::from_secs(60);
    /// Head-room between the bounded waits one verb contains and the outer
    /// bound the watcher enforces against the run thread, so the outer bound
    /// only ever fires on a genuine stall rather than on a verb that used its
    /// whole budget legitimately.
    const RUN_TIMEOUT_SLACK: Duration = Duration::from_secs(45);
    const ZORDER_WALK_LIMIT: usize = 128;
    const LIST_BROWSER_PROFILES_UNAVAILABLE: &str = "list-browser-profiles-unavailable";
    const GRANT_BROWSER_PROFILE_UNAVAILABLE: &str = "grant-browser-profile-unavailable";
    const REVOKE_BROWSER_PROFILE_UNAVAILABLE: &str = "revoke-browser-profile-unavailable";
    const RUN_BROWSER_IMPORT_UNAVAILABLE: &str = "run-browser-import-unavailable";

    /// The outer bound the watcher enforces on one whole invocation, per verb.
    ///
    /// One number cannot cover all six: `reveal-view-once` drives *two* bounded
    /// legs after the readiness wait, and `status` drives none at all. A single
    /// 150-second bound would report a legitimately slow reveal as `stalled`,
    /// which is the one verdict that must only ever mean "OSL wedged".
    fn run_timeout(verb: Verb) -> Duration {
        match verb {
            Verb::Status => Duration::from_secs(30),
            // These verbs are accepted only so this build can refuse them by
            // name. They do not drive state or wait for readiness.
            Verb::ListBrowserProfiles
            | Verb::GrantBrowserProfile
            | Verb::RevokeBrowserProfile
            | Verb::RunBrowserImport => Duration::from_secs(30),
            Verb::Host => READINESS_TIMEOUT + HOST_TIMEOUT + RUN_TIMEOUT_SLACK,
            Verb::Send => RUN_TIMEOUT,
            Verb::Drain | Verb::Rehydrate => READINESS_TIMEOUT + VERB_TIMEOUT + RUN_TIMEOUT_SLACK,
            // A drain to list, then a reveal to open.
            Verb::RevealViewOnce => {
                READINESS_TIMEOUT + VERB_TIMEOUT + VERB_TIMEOUT + RUN_TIMEOUT_SLACK
            }
        }
    }

    /// Every breadcrumb a healthy atomic send appends, in the order
    /// `send_native_discord_qa_atomic_text` appends them. Each failure site in
    /// that command writes a *different* label, so a missing entry here names
    /// the exact hop that stopped the send.
    const EXPECTED_STAGES: [&str; 11] = [
        "send_command_entered",
        "send_registration_ready",
        "send_session_locked",
        "send_plaintext_accepted",
        "send_encryption_done",
        "send_carrier_text_ready",
        "send_place_carrier_called",
        "send_carrier_typed",
        "send_enter_injected",
        "send_proof_confirmed",
        "send_receipt_written",
    ];

    /// Set for the whole lifetime of one spawned send, cleared by that send
    /// itself. A run that times out leaves this latched, so the next trigger is
    /// answered `busy` instead of overlapping a second probe onto the first.
    static SEND_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
    /// The same latch for the receive-side verbs. A drain, a rehydration or a
    /// reveal that outlives its run must not be overlapped by a second one: two
    /// concurrent drains of one inbox would each see half a batch, and two
    /// concurrent reveals of one message would make "it opened exactly once"
    /// unprovable by construction.
    static DRIVE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
    /// Set for the whole lifetime of one run thread, cleared by that thread. A
    /// stalled run leaves it latched, which is deliberate: the watcher stays
    /// responsive and answers every later trigger `busy` instead of stacking a
    /// second run on top of a wedged one.
    static RUN_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

    /// The id of the message this process last opened through
    /// `reveal-view-once`, held in memory for the lifetime of the process and
    /// **never written anywhere**.
    ///
    /// It exists for one claim: a view-once message opens exactly once. Proving
    /// the second attempt is refused needs the *same* id twice, and after the
    /// first reveal that id is no longer in any pending list the harness could
    /// read. Handing the id out in the verdict would put a routing handle for a
    /// live conversation on disk, so it stays here and the harness names it as
    /// `"target": "last"` instead.
    static LAST_REVEALED_MESSAGE_ID: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

    /// The body of the last legacy trigger this instance declined because it
    /// was addressed to a different one, hashed. Only a change writes a new
    /// decline record, so declining does not turn into two file writes a second
    /// for as long as the other instance takes to collect its trigger.
    static DECLINED_REQUEST_FINGERPRINT: std::sync::atomic::AtomicU64 =
        std::sync::atomic::AtomicU64::new(0);

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Criterion {
        pass: bool,
        /// Whether `pass` above counts toward the verdict's own `pass`.
        ///
        /// Every criterion is *reported* for every verb, because an observation
        /// is worth having either way; but only some of them are a claim the
        /// verb makes. A drain says nothing about whether the composer is
        /// stacked above Discord -- that decides where the operator's
        /// keystrokes go, and a drain sends no keystrokes -- so grading it
        /// would make a healthy drain unable to pass. Ungraded criteria were
        /// the alternative to dropping the field, and dropping it would have
        /// made the verdict quieter rather than more honest.
        graded: bool,
        detail: Option<String>,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct StageCriterion {
        label: &'static str,
        pass: bool,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Verdict {
        schema_version: u8,
        observed_at_unix_ms: u128,
        /// The exact trigger file this verdict answers -- the unqualified
        /// rendezvous or this instance's addressed one. Never inferred.
        trigger: String,
        /// This process's Tauri bundle identifier. Every verdict says who wrote
        /// it, so a verdict read out of a shared `%TEMP%` is never anonymous.
        instance: String,
        /// Which verb was driven: `status`, `host`, `send`, `drain`,
        /// `rehydrate` or `reveal-view-once`. `none` when the request was
        /// refused before a verb could be chosen.
        verb: &'static str,
        /// `legacy` -- an empty or free-text trigger body, meaning `send`.
        /// `json` -- a request object. `none` -- the body was never read.
        request_format: &'static str,
        /// `accepted`, or the fixed label of the refusal. Never free text and
        /// never a fragment of the request, so a malformed request cannot write
        /// its own bytes into this file.
        request_status: &'static str,
        /// The one fixed plaintext the `send` verb transmits, and `null` for
        /// every other verb -- none of which sends anything at all.
        probe: Option<&'static str>,
        /// `completed` -- the verb was driven and returned; verdict is about it.
        /// `not-ready` -- a precondition never arrived inside its bound.
        /// `busy` -- a previous invocation had not finished.
        /// `refused` -- the request or the verb's own precondition was refused
        ///   by name; see `requestStatus` and `refusal`.
        /// `send-timeout` -- the send command did not return inside its bound.
        /// `verb-timeout` -- a receive-side verb did not return inside its bound.
        /// `stalled` -- the run itself did not finish inside its bound.
        /// Only `completed` can ever carry `pass: true`.
        outcome: &'static str,
        /// Whether every precondition **this verb requires** was satisfied
        /// before it was driven. Not every verb requires the same set.
        ready: bool,
        /// True only when `outcome == "completed"` and every *graded* criterion
        /// -- and, for `send`, every stage -- passed.
        pass: bool,
        /// The fixed label naming why this invocation refused, when it did.
        /// A refusal is always named here; a silent no-op is never an answer.
        refusal: Option<&'static str>,
        criteria: BTreeMap<&'static str, Criterion>,
        /// Only the `send` verb makes a claim about send stages. Every other
        /// verb reports an empty list rather than eleven false ones, because
        /// eleven false stages read as eleven failures.
        send_stages: Vec<StageCriterion>,
        /// What one drive of the inbound drain observed. Counts only.
        drain: Option<DrainReport>,
        /// What one native-window adoption command and its after-sweep observed.
        host: Option<HostReport>,
        /// What one drive of the transcript read/decode pass observed.
        rehydrate: Option<RehydrateReport>,
        /// What one drive of the two-phase view-once reveal observed.
        reveal: Option<RevealReport>,
        /// Observable state that no verb had to change to establish. Present
        /// for the `status` verb only.
        status: Option<StatusReport>,
        /// OSL's own refusal string when a receive-side verb returned `Err`. It
        /// is an OSL diagnostic in the same sense `sendError` is: a fixed
        /// backend sentence, never draft, carrier or conversation text.
        verb_error: Option<String>,
        carrier_status: Option<String>,
        error_class: Option<String>,
        error_detail: Option<String>,
        receipt_phase: Option<String>,
        receipt_phase_outcome: Option<String>,
        /// `measured` when the carrier layout came from the calibrated Discord
        /// composer, `fallback` when it came from this module's constants.
        carrier_layout_source: &'static str,
        /// OSL's own refusal string when the send command returned `Err`. It is
        /// an OSL diagnostic, never draft, carrier or conversation text.
        send_error: Option<String>,
        /// Where the composer sat once the send returned. Reported but NOT
        /// graded: a placement hands focus to Discord, so the composer being
        /// behind it afterwards is not by itself a defect. The graded criterion
        /// is the state immediately *before* the send, which is what decides
        /// whether the operator's keystrokes reach OSL or Discord.
        composer_zorder_after_send: &'static str,
        composer_visible_after_send: Option<bool>,
    }

    /// What the `status` verb answers: the observable surface of this instance,
    /// established without driving anything.
    ///
    /// Everything here is a boolean, a byte count or a path this module itself
    /// chose. It reads no receipt *content* -- only whether each file is
    /// there -- so nothing a receipt contains can reach this file through it.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct StatusReport {
        send_in_flight: bool,
        drive_in_flight: bool,
        run_in_flight: bool,
        /// Read-only, fail-closed qualification for B6. This never claims the
        /// runtime ran; it names why a controller must stop before creating a
        /// disposable identity or contacting a server.
        b6_preflight: broker::DiscordQaB6Preflight,
        /// The current length of the append-only send-stage trail. A harness
        /// baselines this and grades only what was appended after, which is how
        /// it tells "this instance sent nothing" from "this instance sent
        /// something before the run started" -- the structural half of **P6**.
        send_stage_trail_bytes: u64,
        send_stage_receipt_present: bool,
        inbound_receipt_present: bool,
        inbound_poll_receipt_present: bool,
        outbound_receipt_present: bool,
        /// Whether the carrier layout a `send` would use is the calibrated one.
        /// Reading it costs nothing and changes nothing.
        carrier_layout_source: &'static str,
        /// Where to address a request to *this* instance, and where its answer
        /// will appear. Emitted so a harness never has to reconstruct the
        /// naming rule from the identifier itself.
        addressed_trigger_file: String,
        addressed_verdict_file: String,
        legacy_trigger_file: &'static str,
        /// The verb vocabulary this build understands, so a harness can refuse
        /// to grade a build that predates the verb it means to drive instead of
        /// silently getting a legacy send.
        verbs: [&'static str; 6],
    }

    /// Everything about one invocation that is not the original send path.
    ///
    /// Collected into one value rather than added as eight more parameters to
    /// [`build_verdict`], which already takes eight.
    struct VerbOutcome {
        verb: &'static str,
        instance: String,
        trigger_file: String,
        request_format: &'static str,
        request_status: &'static str,
        refusal: Option<&'static str>,
        verb_error: Option<String>,
        drain: Option<DrainReport>,
        host: Option<HostReport>,
        rehydrate: Option<RehydrateReport>,
        reveal: Option<RevealReport>,
        status: Option<StatusReport>,
    }

    impl VerbOutcome {
        fn new(verb: &'static str, instance: &str, trigger_file: &str) -> Self {
            Self {
                verb,
                instance: instance.to_owned(),
                trigger_file: trigger_file.to_owned(),
                request_format: "none",
                request_status: "accepted",
                refusal: None,
                verb_error: None,
                drain: None,
                host: None,
                rehydrate: None,
                reveal: None,
                status: None,
            }
        }

        fn refused(mut self, refusal: &'static str) -> Self {
            self.request_status = refusal;
            self.refusal = Some(refusal);
            self
        }
    }

    /// One read-only sweep of everything the verdict has to state about the
    /// world. Nothing here mutates any OSL state.
    struct Observation {
        owner: Option<String>,
        identity_detail: Option<String>,
        discord_adopted: bool,
        discord_detail: Option<String>,
        discord_window: Option<isize>,
        protection_engaged: bool,
        composer_exists: bool,
        composer_visible: bool,
        /// Three-valued and never optimistic: `None` is "could not be decided"
        /// and must never be read as "above".
        composer_above_discord: Option<bool>,
        overlay_context_ok: bool,
        overlay_context_detail: Option<String>,
    }

    impl Observation {
        /// What the driver knows when it could not look: nothing. Every
        /// criterion built from this fails with the same honest reason, which
        /// is what a stalled run has actually proved.
        fn unknown(reason: &'static str) -> Self {
            Self {
                owner: None,
                identity_detail: Some(reason.to_owned()),
                discord_adopted: false,
                discord_detail: Some(reason.to_owned()),
                discord_window: None,
                protection_engaged: false,
                composer_exists: false,
                composer_visible: false,
                composer_above_discord: None,
                overlay_context_ok: false,
                overlay_context_detail: Some(reason.to_owned()),
            }
        }

        /// Composer z-order is deliberately NOT a readiness gate: an
        /// undecidable stack walk would otherwise hang the run out to its
        /// timeout, and the send path re-stacks the composer itself. It is
        /// still reported as a pass/fail criterion.
        fn ready(&self) -> bool {
            self.owner.is_some()
                && self.discord_adopted
                && self.protection_engaged
                && self.composer_exists
                && self.composer_visible
                && self.overlay_context_ok
        }

        /// Readiness is per-verb, and deliberately not one set.
        ///
        /// `send` needs everything [`Self::ready`] needs, because it hands the
        /// operator's own composer to Discord and a keystroke that lands in the
        /// wrong window is plaintext in a real conversation.
        ///
        /// The receive-side verbs need an unlocked identity, an adopted Discord
        /// window, the overlay window to exist -- it is the caller identity all
        /// three commands check -- and a valid overlay context, which is what
        /// binds the drain to one conversation. They do **not** need the lock
        /// engaged or the composer on screen: `lockEngaged` is whether what the
        /// operator types is being encrypted, and none of these verbs types
        /// anything. Requiring it would have made a receiving instance
        /// undrainable for the entirely correct reason that nobody was writing
        /// on it.
        ///
        /// `status` requires nothing. Refusing to report the world because the
        /// world is not ready is the one thing a status verb must never do.
        fn ready_for(&self, verb: Verb) -> bool {
            match verb {
                Verb::Status => true,
                // Readiness cannot make an unavailable browser-profile driver
                // available. Proceed directly to its named refusal instead of
                // waiting and misreporting the request as `not-ready`.
                Verb::ListBrowserProfiles
                | Verb::GrantBrowserProfile
                | Verb::RevokeBrowserProfile
                | Verb::RunBrowserImport => true,
                // Hosting exists to establish adoption and overlay context, so
                // requiring either before the command would make it inert.
                Verb::Host => self.owner.is_some(),
                Verb::Send => self.ready(),
                Verb::Drain | Verb::Rehydrate | Verb::RevealViewOnce => {
                    self.owner.is_some()
                        && self.discord_adopted
                        && self.composer_exists
                        && self.overlay_context_ok
                }
            }
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn now_unix_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    #[cfg(windows)]
    fn composer_is_above_discord(app: &tauri::AppHandle, discord_window: isize) -> Option<bool> {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetAncestor, GetWindow, GA_ROOT, GW_HWNDPREV,
        };
        let overlay = app
            .get_webview_window(native_discord_overlay::OVERLAY_LABEL)?
            .hwnd()
            .ok()?
            .0 as isize;
        let discord_root = unsafe { GetAncestor(discord_window as HWND, GA_ROOT) } as isize;
        if overlay == 0 || discord_root == 0 || overlay == discord_root {
            return None;
        }
        // Walk upward from Discord. Reaching the composer proves it is above;
        // reaching the top of the stack proves it is not. An exhausted walk is
        // undecided, and undecided is never "above".
        let mut cursor = discord_root;
        for _ in 0..ZORDER_WALK_LIMIT {
            cursor = unsafe { GetWindow(cursor as HWND, GW_HWNDPREV) } as isize;
            if cursor == 0 {
                return Some(false);
            }
            if cursor == overlay {
                return Some(true);
            }
        }
        None
    }

    #[cfg(not(windows))]
    fn composer_is_above_discord(_app: &tauri::AppHandle, _discord_window: isize) -> Option<bool> {
        None
    }

    fn observe(app: &tauri::AppHandle) -> Observation {
        let (owner, identity_detail) =
            match active_unlocked_osl_user_id(&app.state::<HubCoreState>()) {
                Ok(owner) => (Some(owner), None),
                Err(error) => (None, Some(error)),
            };
        let mut discord_adopted = false;
        let mut discord_detail = None;
        let mut discord_window = None;
        if let Some(owner) = owner.as_deref() {
            let host_state = app.state::<NativeWindowHostState>();
            match host_state.current_discord_service_host(owner) {
                Ok(host) if host.service_id == "discord" => discord_adopted = true,
                Ok(_) => {
                    discord_detail = Some("The adopted native host is not Discord".to_owned());
                }
                Err(error) => discord_detail = Some(error),
            }
            if discord_adopted {
                match host_state.discord_overlay_target(owner) {
                    Ok(target) => discord_window = Some(target.window),
                    Err(error) => discord_detail = Some(error),
                }
            }
        }
        let composer_window = app.get_webview_window(native_discord_overlay::OVERLAY_LABEL);
        let composer_exists = composer_window.is_some();
        let composer_visible = composer_window
            .as_ref()
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false);
        let composer_above_discord =
            discord_window.and_then(|window| composer_is_above_discord(app, window));
        let (overlay_context_ok, overlay_context_detail) =
            match require_overlay_context_snapshot(app) {
                Ok(_) => (true, None),
                Err(error) => (false, Some(error)),
            };
        Observation {
            owner,
            identity_detail,
            discord_adopted,
            discord_detail,
            discord_window,
            protection_engaged: app.state::<OverlaySessionState>().lock_engaged(),
            composer_exists,
            composer_visible,
            composer_above_discord,
            overlay_context_ok,
            overlay_context_detail,
        }
    }

    /// The metrics the protected renderer normally measures from its own DOM.
    /// A headless driver has no DOM, so they are derived from the calibrated
    /// Discord composer where OSL has already measured it, and only fall back
    /// to constants when it has not. The verdict records which happened, so a
    /// fallback run can never be mistaken for a measured one.
    ///
    /// These are metrics only: `DiscordCarrierLayout` cannot carry draft,
    /// carrier, ciphertext, account or conversation text.
    fn carrier_layout(app: &tauri::AppHandle) -> (DiscordCarrierLayout, &'static str) {
        use osl_privacy_hub::native_discord_adapter::{
            DiscordCarrierPadding, DiscordCarrierRowKind,
        };
        const FALLBACK_CONTENT_WIDTH_PX: f64 = 560.0;
        const FALLBACK_GRAPHEME_WIDTH_PX: f64 = 7.5;
        const FALLBACK_LINE_HEIGHT_PX: f64 = 22.0;
        /// Points to CSS pixels at the 96 DPI baseline every Discord metric
        /// above is already expressed in.
        const POINTS_TO_PIXELS: f64 = 96.0 / 72.0;
        /// A proportional UI face averages roughly half its em across mixed
        /// prose. Only used to size the wrap column, never a payload byte.
        const AVERAGE_GRAPHEME_EM_RATIO: f64 = 0.5;

        let composer = app.state::<NativeDiscordComposerState>();
        let presentation = composer.verified_text_presentation();
        let bounds = presentation
            .as_ref()
            .map(|presentation| presentation.bounds)
            .or_else(|| composer.verified_composer_bounds());
        let content_width_px = bounds
            .map(|bounds| f64::from(bounds.right.saturating_sub(bounds.left)))
            .filter(|width| *width > 1.0);
        let line_height_px = presentation
            .as_ref()
            .and_then(|presentation| presentation.line_height_milli_px)
            .map(|milli| f64::from(milli) / 1000.0)
            .filter(|height| *height > 1.0);
        let average_grapheme_width_px = presentation
            .as_ref()
            .and_then(|presentation| presentation.font_size_milli_points)
            .map(|milli| (f64::from(milli) / 1000.0) * POINTS_TO_PIXELS * AVERAGE_GRAPHEME_EM_RATIO)
            .filter(|width| *width > 0.5);
        let source = if content_width_px.is_some()
            && line_height_px.is_some()
            && average_grapheme_width_px.is_some()
        {
            "measured"
        } else {
            "fallback"
        };
        (
            DiscordCarrierLayout {
                content_width_px: content_width_px.unwrap_or(FALLBACK_CONTENT_WIDTH_PX),
                average_grapheme_width_px: average_grapheme_width_px
                    .unwrap_or(FALLBACK_GRAPHEME_WIDTH_PX),
                line_height_px: line_height_px.unwrap_or(FALLBACK_LINE_HEIGHT_PX),
                // Every measurement above is already in screen pixels, so
                // neither scale factor may be applied a second time.
                zoom: 1.0,
                density: 1.0,
                padding: DiscordCarrierPadding::ShapeMatched,
                row_kind: DiscordCarrierRowKind::PlainText,
            },
            source,
        )
    }

    fn stage_trail_len() -> u64 {
        std::fs::metadata(temp_path(SEND_STAGE_TRAIL_FILE))
            .map(|metadata| metadata.len())
            .unwrap_or(0)
    }

    /// Only the breadcrumbs this run appended. The trail is append-only and
    /// shared with the manual QA path, so it is never truncated here.
    fn stage_trail_since(offset: u64) -> Vec<String> {
        let Ok(bytes) = std::fs::read(temp_path(SEND_STAGE_TRAIL_FILE)) else {
            return Vec::new();
        };
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        String::from_utf8_lossy(&bytes[start..])
            .lines()
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect()
    }

    /// Match [`EXPECTED_STAGES`] as an ordered subsequence of what was
    /// observed. The second return value is false only when a label was
    /// present but out of order, which is a different defect from a label that
    /// never happened at all.
    fn stage_criteria(observed: &[String]) -> (Vec<StageCriterion>, bool) {
        let mut cursor = 0usize;
        let mut in_order = true;
        let mut criteria = Vec::with_capacity(EXPECTED_STAGES.len());
        for label in EXPECTED_STAGES {
            match observed[cursor..]
                .iter()
                .position(|candidate| candidate.as_str() == label)
            {
                Some(offset) => {
                    cursor += offset + 1;
                    criteria.push(StageCriterion { label, pass: true });
                }
                None => {
                    if observed.iter().any(|candidate| candidate.as_str() == label) {
                        in_order = false;
                    }
                    criteria.push(StageCriterion { label, pass: false });
                }
            }
        }
        (criteria, in_order)
    }

    fn send_stage_receipt() -> Option<serde_json::Value> {
        let path = keystore::osl_base_dir().ok()?.join(SEND_STAGE_RECEIPT_FILE);
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    fn receipt_string(receipt: Option<&serde_json::Value>, key: &str) -> Option<String> {
        receipt?.get(key)?.as_str().map(str::to_owned)
    }

    /// Insert a criterion that counts toward the verdict's `pass`.
    fn insert(
        criteria: &mut BTreeMap<&'static str, Criterion>,
        id: &'static str,
        pass: bool,
        detail: Option<String>,
    ) {
        criteria.insert(
            id,
            Criterion {
                pass,
                graded: true,
                detail,
            },
        );
    }

    /// Insert a criterion that is reported but makes no claim for this verb.
    /// See [`Criterion::graded`].
    fn insert_graded(
        criteria: &mut BTreeMap<&'static str, Criterion>,
        id: &'static str,
        graded: bool,
        pass: bool,
        detail: Option<String>,
    ) {
        criteria.insert(
            id,
            Criterion {
                pass,
                graded,
                detail,
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn build_verdict(
        verb: Verb,
        outcome: &'static str,
        observation: &Observation,
        carrier_layout_source: &'static str,
        send: Option<&Result<NativeDiscordQaAtomicText, String>>,
        send_stages: Vec<StageCriterion>,
        stages_in_order: bool,
        busy_detail: Option<&'static str>,
        after_send: Option<&Observation>,
        outcome_detail: VerbOutcome,
    ) -> Verdict {
        // Only this run's receipt may be quoted. The send-stage receipt file is
        // shared with the manual QA paths and survives restarts, so reading it
        // when no send was driven would attribute somebody else's `errorClass`
        // to this invocation.
        let receipt = send.is_some().then(send_stage_receipt).flatten();
        let receipt = receipt.as_ref();
        let mut criteria = BTreeMap::new();
        // Every readiness observation is reported for every verb; which of them
        // this verb is actually claiming is `readiness_criterion_is_graded`.
        let mut readiness = |criteria: &mut BTreeMap<&'static str, Criterion>,
                             id: &'static str,
                             pass: bool,
                             detail: Option<String>| {
            insert_graded(
                criteria,
                id,
                readiness_criterion_is_graded(verb, id),
                pass,
                detail,
            );
        };
        readiness(
            &mut criteria,
            "identity_unlocked",
            observation.owner.is_some(),
            observation.identity_detail.clone(),
        );
        readiness(
            &mut criteria,
            "discord_window_adopted",
            observation.discord_adopted && observation.discord_window.is_some(),
            observation.discord_detail.clone(),
        );
        readiness(
            &mut criteria,
            "protection_engaged",
            observation.protection_engaged,
            (!observation.protection_engaged)
                .then(|| "Protected Discord encryption is switched off".to_owned()),
        );
        readiness(
            &mut criteria,
            "composer_window_exists",
            observation.composer_exists,
            (!observation.composer_exists)
                .then(|| "The protected composer window does not exist".to_owned()),
        );
        readiness(
            &mut criteria,
            "composer_window_visible",
            observation.composer_visible,
            (!observation.composer_visible)
                .then(|| "The protected composer window is not on screen".to_owned()),
        );
        readiness(
            &mut criteria,
            "composer_above_discord",
            observation.composer_above_discord == Some(true),
            match observation.composer_above_discord {
                Some(true) => None,
                Some(false) => Some("below-discord".to_owned()),
                None => Some("unknown".to_owned()),
            },
        );
        readiness(
            &mut criteria,
            "overlay_context_valid",
            observation.overlay_context_ok,
            observation.overlay_context_detail.clone(),
        );

        let carrier = match send {
            Some(Ok(result)) => Some(&result.carrier),
            _ => None,
        };
        let prepared = match send {
            Some(Ok(result)) => Some(&result.prepared.prepared),
            _ => None,
        };
        let send_error = match send {
            Some(Err(error)) => Some(error.clone()),
            _ => busy_detail.map(str::to_owned),
        };
        let enter_injected = carrier.is_some_and(|carrier| carrier.enter_sent);
        let carrier_placed = carrier.is_some_and(|carrier| carrier.placed);
        let carrier_status_sent =
            carrier.is_some_and(|carrier| carrier.status == DiscordCarrierStatus::Sent);
        let inbox_committed = prepared.is_some_and(|prepared| {
            prepared.person_to_person_e2ee && prepared.delivered_to_osl_inbox
        });
        let send_completed = carrier_status_sent && carrier_placed && enter_injected;
        let stages_reached = send_stages.iter().all(|stage| stage.pass);
        let is_send = verb == Verb::Send;

        // Send criteria belong to the send verb alone. Reporting them for a
        // drain would report six failures that nothing failed at.
        if is_send {
            insert(
                &mut criteria,
                "send_stages_reached_in_order",
                stages_reached && stages_in_order,
                (!stages_reached || !stages_in_order).then(|| {
                    if stages_reached {
                        "Every stage was reached but not in order".to_owned()
                    } else {
                        "At least one send stage was never reached".to_owned()
                    }
                }),
            );
            insert(&mut criteria, "carrier_placed", carrier_placed, None);
            insert(&mut criteria, "enter_injected", enter_injected, None);
            insert(&mut criteria, "send_completed", send_completed, None);
            insert(&mut criteria, "osl_inbox_committed", inbox_committed, None);
            insert(
                &mut criteria,
                "carrier_status_sent",
                carrier_status_sent,
                None,
            );
        }

        let carrier_status = carrier
            .map(|carrier| discord_carrier_status_label(carrier.status).to_owned())
            .or_else(|| receipt_string(receipt, "carrierStatus"));
        let error_class = receipt_string(receipt, "errorClass");
        let error_detail = receipt_string(receipt, "errorDetail");
        if is_send {
            insert(
                &mut criteria,
                "no_receipt_error_class",
                send.is_some() && error_class.is_none(),
                error_class
                    .clone()
                    .or_else(|| send.is_none().then(|| "No send was driven".to_owned())),
            );
        }

        insert_verb_criteria(&mut criteria, verb, &outcome_detail);

        let pass = outcome == "completed"
            && (!is_send || (stages_reached && stages_in_order))
            && criteria
                .values()
                .all(|criterion| !criterion.graded || criterion.pass);
        Verdict {
            // Bumped from 1: `trigger` is now the file that was actually
            // consumed rather than a fixed name, `probe` is nullable, every
            // criterion carries `graded`, and the verb reports are new.
            schema_version: 2,
            observed_at_unix_ms: now_unix_ms(),
            trigger: outcome_detail.trigger_file,
            instance: outcome_detail.instance,
            verb: outcome_detail.verb,
            request_format: outcome_detail.request_format,
            request_status: outcome_detail.request_status,
            probe: is_send.then_some(PROBE_PLAINTEXT),
            outcome,
            ready: observation.ready_for(verb),
            pass,
            refusal: outcome_detail.refusal,
            criteria,
            send_stages: if is_send { send_stages } else { Vec::new() },
            drain: outcome_detail.drain,
            host: outcome_detail.host,
            rehydrate: outcome_detail.rehydrate,
            reveal: outcome_detail.reveal,
            status: outcome_detail.status,
            verb_error: outcome_detail.verb_error,
            carrier_status,
            error_class,
            error_detail,
            receipt_phase: receipt_string(receipt, "phase"),
            receipt_phase_outcome: receipt_string(receipt, "phaseOutcome"),
            carrier_layout_source,
            send_error,
            composer_zorder_after_send: match after_send.map(|after| after.composer_above_discord) {
                Some(Some(true)) => "above-discord",
                Some(Some(false)) => "below-discord",
                Some(None) => "unknown",
                None => "not-observed",
            },
            composer_visible_after_send: after_send.map(|after| after.composer_visible),
        }
    }

    /// The criteria one receive-side verb makes a claim about.
    ///
    /// Each is a claim the verb is *for*, so a verb that was never driven -- a
    /// refusal, a `not-ready`, a timeout -- reports all of its own criteria
    /// false and cannot pass. That is the fail-closed half: an invocation that
    /// did nothing must never read as an invocation that succeeded.
    fn insert_verb_criteria(
        criteria: &mut BTreeMap<&'static str, Criterion>,
        verb: Verb,
        outcome: &VerbOutcome,
    ) {
        match verb {
            Verb::Send => {}
            Verb::Status => {
                insert(
                    criteria,
                    "status_observed",
                    outcome.status.is_some(),
                    outcome
                        .status
                        .is_none()
                        .then(|| "The read-only sweep did not complete".to_owned()),
                );
            }
            Verb::ListBrowserProfiles => {
                insert(
                    criteria,
                    "browser_profile_list_driven",
                    false,
                    Some(LIST_BROWSER_PROFILES_UNAVAILABLE.to_owned()),
                );
            }
            Verb::GrantBrowserProfile => {
                insert(
                    criteria,
                    "browser_profile_grant_driven",
                    false,
                    Some(GRANT_BROWSER_PROFILE_UNAVAILABLE.to_owned()),
                );
            }
            Verb::RevokeBrowserProfile => {
                insert(
                    criteria,
                    "browser_profile_revoke_driven",
                    false,
                    Some(REVOKE_BROWSER_PROFILE_UNAVAILABLE.to_owned()),
                );
            }
            Verb::RunBrowserImport => {
                insert(
                    criteria,
                    "browser_profile_import_driven",
                    false,
                    Some(RUN_BROWSER_IMPORT_UNAVAILABLE.to_owned()),
                );
            }
            Verb::Host => {
                let host = outcome.host.as_ref();
                insert(
                    criteria,
                    "host_adopted_the_window",
                    host.is_some_and(|host| host.adopted),
                    host.filter(|host| !host.adopted)
                        .map(|_| "The production host command did not adopt a window".to_owned())
                        .or_else(|| outcome.verb_error.clone()),
                );
            }
            Verb::Drain => {
                let drain = outcome.drain.as_ref();
                insert(
                    criteria,
                    "drain_returned",
                    drain.is_some(),
                    outcome.verb_error.clone(),
                );
                // A drain that opened nothing is a perfectly healthy drain, so
                // `openedCount` is reported and not graded. What IS graded is
                // that nothing was left behind for lack of a working store: a
                // non-zero `deferredRows` is the batch saying "incomplete".
                insert(
                    criteria,
                    "drain_left_no_deferred_rows",
                    drain.is_some_and(|drain| drain.deferred_rows == 0),
                    drain
                        .filter(|drain| drain.deferred_rows > 0)
                        .map(|drain| format!("{} row(s) deferred", drain.deferred_rows)),
                );
                // Every opened message must have authenticated against the exact
                // conversation OSL is bound to. One that did not is the defect
                // this whole surface exists to catch.
                insert(
                    criteria,
                    "drain_every_message_context_verified",
                    drain.is_some_and(|drain| drain.context_verified_count == drain.opened_count),
                    drain
                        .filter(|drain| drain.context_verified_count != drain.opened_count)
                        .map(|drain| {
                            format!(
                                "{} of {} opened message(s) verified their context",
                                drain.context_verified_count, drain.opened_count
                            )
                        }),
                );
            }
            Verb::Rehydrate => {
                let rehydrate = outcome.rehydrate.as_ref();
                insert(
                    criteria,
                    "rehydrate_returned",
                    rehydrate.is_some(),
                    outcome.verb_error.clone(),
                );
                // `read: false` is the same-scope floor refusing, which is a
                // rate limit and a rerun -- never a claim about the transcript.
                insert(
                    criteria,
                    "rehydrate_read_the_transcript",
                    rehydrate.is_some_and(|rehydrate| rehydrate.read),
                    rehydrate
                        .filter(|rehydrate| !rehydrate.read)
                        .map(|rehydrate| {
                            format!(
                                "The same-scope floor refused this read; {}ms remain",
                                rehydrate.retry_after_ms
                            )
                        }),
                );
            }
            Verb::RevealViewOnce => {
                let reveal = outcome.reveal.as_ref();
                insert(
                    criteria,
                    "reveal_phase_one_listed",
                    reveal.is_some_and(|reveal| reveal.phase_one_driven),
                    outcome.verb_error.clone(),
                );
                insert(
                    criteria,
                    "reveal_target_selected",
                    reveal.is_some_and(|reveal| reveal.selected),
                    reveal.filter(|reveal| !reveal.selected).map(|reveal| {
                        format!(
                            "No view-once message answered target \"{}\"; {} were pending",
                            reveal.target, reveal.phase_one_pending_count
                        )
                    }),
                );
                insert(
                    criteria,
                    "reveal_phase_two_driven",
                    reveal.is_some_and(|reveal| reveal.phase_two_driven),
                    None,
                );
                // Deliberately NOT graded. Both answers are correct depending
                // on which attempt this is: the first reveal must open, the
                // second must refuse. Grading either way would make one of the
                // two claims this verb exists to prove impossible to state.
                insert_graded(
                    criteria,
                    "reveal_opened_the_message",
                    false,
                    reveal.is_some_and(|reveal| reveal.revealed),
                    reveal.map(|reveal| {
                        if reveal.revealed {
                            "opened".to_owned()
                        } else if reveal.refused {
                            "refused".to_owned()
                        } else {
                            "not-attempted".to_owned()
                        }
                    }),
                );
                insert_graded(
                    criteria,
                    "reveal_consumed_the_view_once",
                    false,
                    reveal.is_some_and(|reveal| reveal.view_once_consumed),
                    None,
                );
            }
        }
    }

    fn write_verdict(verdict: &Verdict, path: &Path) {
        let Ok(encoded) = serde_json::to_vec_pretty(verdict) else {
            return;
        };
        let mut partial = path.as_os_str().to_owned();
        partial.push(VERDICT_PARTIAL_SUFFIX);
        let partial = PathBuf::from(partial);
        if std::fs::write(&partial, &encoded).is_ok() {
            let _ = std::fs::rename(&partial, path);
        }
    }

    /// Drive the exact command the protected renderer drives, with the exact
    /// window the renderer would be. No synthetic input is generated anywhere
    /// on this path.
    async fn drive_probe_send(
        app: tauri::AppHandle,
        layout: DiscordCarrierLayout,
    ) -> Result<NativeDiscordQaAtomicText, String> {
        let composer_window = app
            .get_webview_window(native_discord_overlay::OVERLAY_LABEL)
            .ok_or_else(|| "The protected composer window is unavailable".to_owned())?;
        let session = app.state::<HubAccountSessionState>();
        send_native_discord_qa_atomic_text(
            app.clone(),
            composer_window,
            session,
            PROBE_PLAINTEXT.to_owned(),
            false,
            // Atomic placement: `Compatibility` is Pro-gated and would refuse
            // on a disposable QA identity before the send path was reached.
            DiscordCarrierMode::Atomic,
            0,
            Some(layout),
        )
        .await
    }

    /// Drive native-window adoption through the exact command the protected
    /// renderer calls. The pure request decision has only a borrow-only action,
    /// and the final argument is deliberately the literal `None` that can never
    /// authorize quitting the operator's app.
    async fn drive_host(
        app: tauri::AppHandle,
        action: HostAction,
    ) -> Result<NativeWindowHostResult, String> {
        let HostAction::BorrowOnly {
            app_id,
            session_mode,
        } = action;
        let core = app.state::<HubCoreState>();
        let session = app.state::<HubAccountSessionState>();
        host_native_app_window(app.clone(), core, session, app_id, session_mode, None).await
    }

    /// Drive the inbound drain through the exact command the protected
    /// renderer calls, as the exact window the renderer would be. This is not a
    /// QA-only route into the broker: the caller check, the session-transition
    /// lock, the overlay-context check on both sides of the drain and the two
    /// receipts are all the production ones, because they are the same code.
    async fn drive_drain(app: tauri::AppHandle) -> Result<OpenedNativeOverlayTextBatch, String> {
        let composer_window = app
            .get_webview_window(native_discord_overlay::OVERLAY_LABEL)
            .ok_or_else(|| "The protected composer window is unavailable".to_owned())?;
        let session = app.state::<HubAccountSessionState>();
        open_native_discord_overlay_text(app.clone(), composer_window, session).await
    }

    /// Drive one transcript read/decode pass, likewise through the renderer's
    /// own command. See [`REHYDRATE_SCOPE`] for why the scope is a fixed
    /// private value rather than OSL's native binding.
    async fn drive_rehydrate(
        app: tauri::AppHandle,
    ) -> Result<RehydratedNativeDiscordTranscriptDto, String> {
        let composer_window = app
            .get_webview_window(native_discord_overlay::OVERLAY_LABEL)
            .ok_or_else(|| "The protected composer window is unavailable".to_owned())?;
        let session = app.state::<HubAccountSessionState>();
        rehydrate_native_discord_overlay_history(
            app.clone(),
            composer_window,
            session,
            REHYDRATE_SCOPE.to_owned(),
        )
        .await
    }

    /// Drive phase two of the view-once reveal, again through the renderer's
    /// own command.
    ///
    /// `message_id` came either from phase one's pending list or from the
    /// request, and it goes straight to the broker, which applies the real
    /// predicate. It is never written to any file.
    async fn drive_reveal(
        app: tauri::AppHandle,
        message_id: String,
    ) -> Result<broker::OpenedNativeOverlayText, String> {
        let composer_window = app
            .get_webview_window(native_discord_overlay::OVERLAY_LABEL)
            .ok_or_else(|| "The protected composer window is unavailable".to_owned())?;
        let session = app.state::<HubAccountSessionState>();
        reveal_native_discord_overlay_view_once(app.clone(), composer_window, session, message_id)
            .await
    }

    /// Run one bounded drive on the async runtime and answer within `timeout`,
    /// never longer.
    ///
    /// [`DRIVE_IN_FLIGHT`] is latched for the whole lifetime of the spawned
    /// future and cleared by the future itself, so a drive that outlives its
    /// bound leaves the latch set and the next trigger is answered `busy`
    /// rather than overlapping a second drain onto the first.
    fn drive_bounded_for<T, F>(future: F, timeout: Duration) -> Option<Result<T, String>>
    where
        T: Send + 'static,
        F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    {
        DRIVE_IN_FLIGHT.store(true, Ordering::SeqCst);
        let (sender, receiver) = std::sync::mpsc::channel();
        tauri::async_runtime::spawn(async move {
            let result = future.await;
            DRIVE_IN_FLIGHT.store(false, Ordering::SeqCst);
            let _ = sender.send(result);
        });
        receiver.recv_timeout(timeout).ok()
    }

    fn drive_bounded<T, F>(future: F) -> Option<Result<T, String>>
    where
        T: Send + 'static,
        F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    {
        drive_bounded_for(future, VERB_TIMEOUT)
    }

    fn receipt_exists(name: &str) -> bool {
        keystore::osl_base_dir()
            .map(|base| base.join(name).is_file())
            .unwrap_or(false)
    }

    /// The read-only sweep. Every field is read; none is established.
    fn status_report(app: &tauri::AppHandle, instance: &str) -> StatusReport {
        let (_, carrier_layout_source) = carrier_layout(app);
        StatusReport {
            send_in_flight: SEND_IN_FLIGHT.load(Ordering::SeqCst),
            drive_in_flight: DRIVE_IN_FLIGHT.load(Ordering::SeqCst),
            run_in_flight: RUN_IN_FLIGHT.load(Ordering::SeqCst),
            b6_preflight: broker::discord_qa_b6_preflight(&app.state::<HubCoreState>()),
            send_stage_trail_bytes: stage_trail_len(),
            send_stage_receipt_present: receipt_exists(SEND_STAGE_RECEIPT_FILE),
            inbound_receipt_present: receipt_exists(INBOUND_RECEIPT_FILE),
            inbound_poll_receipt_present: receipt_exists(INBOUND_POLL_RECEIPT_FILE),
            outbound_receipt_present: receipt_exists(OUTBOUND_RECEIPT_FILE),
            carrier_layout_source,
            addressed_trigger_file: addressed_name(ADDRESSED_TRIGGER_FORMAT, instance),
            addressed_verdict_file: addressed_name(ADDRESSED_VERDICT_FORMAT, instance),
            legacy_trigger_file: TRIGGER_FILE,
            verbs: [
                Verb::Status.label(),
                Verb::Host.label(),
                Verb::Send.label(),
                Verb::Drain.label(),
                Verb::Rehydrate.label(),
                Verb::RevealViewOnce.label(),
            ],
        }
    }

    /// Which message `reveal-view-once` will attempt, or `None` if this
    /// instance cannot name one. `None` is a refusal, never a no-op.
    fn reveal_target_message_id(
        request: &SelftestRequest,
        batch: Option<&OpenedNativeOverlayTextBatch>,
    ) -> Option<String> {
        match request.reveal_target {
            RevealTarget::PendingIndex => batch?
                .pending_view_once
                .get(request.pending_index)
                .map(|pending| pending.message_id.clone()),
            RevealTarget::MessageId => request.message_id.clone(),
            RevealTarget::Last => LAST_REVEALED_MESSAGE_ID
                .lock()
                .ok()
                .and_then(|last| last.clone()),
        }
    }

    fn run_once(
        app: &tauri::AppHandle,
        request: &SelftestRequest,
        instance: &str,
        trigger_file: &str,
    ) -> Verdict {
        let verb = request.verb;
        let mut outcome_detail = VerbOutcome::new(verb.label(), instance, trigger_file);
        outcome_detail.request_format = request.format;

        // Bounded readiness wait. The operator establishes the adopted Discord
        // window and the engaged lock by hand; this waits for them and then
        // reports, rather than firing into an unready app or blocking forever.
        // `status` waits for nothing: refusing to describe an unready app is the
        // one thing a status verb must never do.
        let deadline = Instant::now() + READINESS_TIMEOUT;
        let mut observation = observe(app);
        while !observation.ready_for(verb) && !verb.is_read_only() && Instant::now() < deadline {
            std::thread::sleep(POLL_INTERVAL);
            observation = observe(app);
        }
        if !observation.ready_for(verb) {
            outcome_detail.refusal = Some(not_ready_refusal(verb));
            if let Some(host) = request.host {
                outcome_detail.host = Some(HostReport::after(
                    host.action(),
                    false,
                    observation.discord_adopted && observation.discord_window.is_some(),
                    observation.overlay_context_ok,
                    observation.protection_engaged,
                ));
            }
            let (stages, in_order) = stage_criteria(&[]);
            return build_verdict(
                verb,
                "not-ready",
                &observation,
                "none",
                None,
                stages,
                in_order,
                None,
                None,
                outcome_detail,
            );
        }

        if verb != Verb::Send {
            return run_receive_side_verb(app, request, observation, outcome_detail);
        }

        let (layout, layout_source) = carrier_layout(app);
        let trail_offset = stage_trail_len();
        SEND_IN_FLIGHT.store(true, Ordering::SeqCst);
        let (sender, receiver) = std::sync::mpsc::channel();
        let send_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = drive_probe_send(send_app, layout).await;
            SEND_IN_FLIGHT.store(false, Ordering::SeqCst);
            let _ = sender.send(result);
        });
        // Bounded: a wedged send is reported, never waited on forever. The
        // in-flight latch above is what keeps a timed-out send from being
        // joined by a second probe.
        let send = receiver.recv_timeout(SEND_TIMEOUT).ok();
        let outcome = if send.is_some() {
            "completed"
        } else {
            "send-timeout"
        };
        // `observation` above is the graded one: it is the state the send was
        // actually driven against. This second sweep is reported alongside it
        // so a composer that vanished or sank during the send is still visible
        // in the verdict without failing an otherwise healthy run.
        let after_send = observe(app);
        let (stages, in_order) = stage_criteria(&stage_trail_since(trail_offset));
        build_verdict(
            Verb::Send,
            outcome,
            &observation,
            layout_source,
            send.as_ref(),
            stages,
            in_order,
            None,
            Some(&after_send),
            outcome_detail,
        )
    }

    /// The host and receive-side verbs, plus the read-only sweep.
    ///
    /// Every one of them either produces a report or names a refusal. There is
    /// no path through this function that returns a verdict claiming a verb was
    /// driven when it was not.
    fn run_receive_side_verb(
        app: &tauri::AppHandle,
        request: &SelftestRequest,
        observation: Observation,
        mut outcome_detail: VerbOutcome,
    ) -> Verdict {
        let verb = request.verb;
        let mut outcome: &'static str = "completed";
        let host_action = request.host.map(|host| host.action());
        let mut host_adopted = false;

        match verb {
            Verb::Send => unreachable!("the send verb is driven by run_once"),
            Verb::Status => {
                outcome_detail.status = Some(status_report(app, &outcome_detail.instance));
            }
            Verb::ListBrowserProfiles => {
                outcome = "refused";
                outcome_detail.refusal = Some(LIST_BROWSER_PROFILES_UNAVAILABLE);
            }
            Verb::GrantBrowserProfile => {
                outcome = "refused";
                outcome_detail.refusal = Some(GRANT_BROWSER_PROFILE_UNAVAILABLE);
            }
            Verb::RevokeBrowserProfile => {
                outcome = "refused";
                outcome_detail.refusal = Some(REVOKE_BROWSER_PROFILE_UNAVAILABLE);
            }
            Verb::RunBrowserImport => {
                outcome = "refused";
                outcome_detail.refusal = Some(RUN_BROWSER_IMPORT_UNAVAILABLE);
            }
            Verb::Host => {
                let action = host_action.expect("accepted host requests resolve host inputs");
                match drive_bounded_for(drive_host(app.clone(), action), HOST_TIMEOUT) {
                    Some(Ok(result)) => {
                        host_adopted = matches!(
                            result.status,
                            NativeWindowHostStatus::Hosted
                                | NativeWindowHostStatus::Resized
                                | NativeWindowHostStatus::Focused
                        );
                    }
                    Some(Err(error)) => {
                        outcome = "refused";
                        outcome_detail.verb_error = Some(error);
                        outcome_detail.refusal = Some("host-refused");
                    }
                    None => {
                        outcome = "verb-timeout";
                        outcome_detail.refusal = Some("host-timeout");
                    }
                }
            }
            Verb::Drain => match drive_bounded(drive_drain(app.clone())) {
                Some(Ok(batch)) => outcome_detail.drain = Some(DrainReport::from_batch(&batch)),
                Some(Err(error)) => {
                    outcome = "refused";
                    outcome_detail.verb_error = Some(error);
                    outcome_detail.refusal = Some("drain-refused");
                }
                None => {
                    outcome = "verb-timeout";
                    outcome_detail.refusal = Some("drain-timeout");
                }
            },
            Verb::Rehydrate => match drive_bounded(drive_rehydrate(app.clone())) {
                Some(Ok(transcript)) => {
                    // The rows are projected to two booleans each *here*, so no
                    // decrypted row text is ever in scope where a report is
                    // built. `RehydrateReport::tally` cannot be handed a row.
                    let rows: Vec<(bool, bool)> = transcript
                        .rows
                        .iter()
                        .map(|row| (row.plaintext.is_some(), row.row.is_some()))
                        .collect();
                    outcome_detail.rehydrate = Some(RehydrateReport::tally(
                        transcript.read,
                        transcript.retry_after_ms,
                        &rows,
                    ));
                }
                Some(Err(error)) => {
                    outcome = "refused";
                    outcome_detail.verb_error = Some(error);
                    outcome_detail.refusal = Some("rehydrate-refused");
                    outcome_detail.rehydrate = Some(RehydrateReport::refused());
                }
                None => {
                    outcome = "verb-timeout";
                    outcome_detail.refusal = Some("rehydrate-timeout");
                }
            },
            Verb::RevealViewOnce => {
                let mut report =
                    RevealReport::not_driven(request.reveal_target, request.pending_index);
                // PHASE ONE. The drain that *lists* a view-once message without
                // opening it. This is the same command the renderer runs, and
                // its `pendingViewOnce` entries are exactly what the operator
                // sees as an unopened view-once row.
                let batch = match drive_bounded(drive_drain(app.clone())) {
                    Some(Ok(batch)) => {
                        report.phase_one_driven = true;
                        report.phase_one_pending_count = batch.pending_view_once.len();
                        report.phase_one_opened_count = batch.messages.len();
                        outcome_detail.drain = Some(DrainReport::from_batch(&batch));
                        Some(batch)
                    }
                    Some(Err(error)) => {
                        outcome = "refused";
                        outcome_detail.verb_error = Some(error);
                        outcome_detail.refusal = Some("reveal-phase-one-refused");
                        None
                    }
                    None => {
                        outcome = "verb-timeout";
                        outcome_detail.refusal = Some("reveal-phase-one-timeout");
                        None
                    }
                };

                if outcome == "completed" {
                    match reveal_target_message_id(request, batch.as_ref()) {
                        Some(message_id) => {
                            report.selected = true;
                            // PHASE TWO. Opening it, exactly once.
                            match drive_bounded(drive_reveal(app.clone(), message_id.clone())) {
                                Some(Ok(message)) => {
                                    report.record_phase_two(Ok(&message));
                                    if let Ok(mut last) = LAST_REVEALED_MESSAGE_ID.lock() {
                                        *last = Some(message_id);
                                    }
                                }
                                Some(Err(error)) => {
                                    // A refusal here is a first-class answer,
                                    // not a failure of the run: on a second
                                    // attempt against an already-consumed
                                    // message it is the *expected* one. The
                                    // verb completed either way; what happened
                                    // is in `reveal.refused`.
                                    report.record_phase_two(Err(()));
                                    outcome_detail.verb_error = Some(error);
                                }
                                None => {
                                    outcome = "verb-timeout";
                                    outcome_detail.refusal = Some("reveal-phase-two-timeout");
                                }
                            }
                        }
                        None => {
                            // Nothing matched the target. Named, never silent.
                            outcome = "refused";
                            outcome_detail.refusal = Some(match request.reveal_target {
                                RevealTarget::PendingIndex => "reveal-pending-index-out-of-range",
                                RevealTarget::MessageId => "reveal-message-id-missing",
                                RevealTarget::Last => "reveal-no-previous-message",
                            });
                        }
                    }
                }
                outcome_detail.reveal = Some(report);
            }
        }

        let (stages, in_order) = stage_criteria(&[]);
        // The second sweep is reported for the same reason it is on the send
        // path: state that changed while the verb ran is visible without
        // failing a healthy run.
        let after = observe(app);
        if let Some(action) = host_action {
            outcome_detail.host = Some(HostReport::after(
                action,
                host_adopted,
                after.discord_adopted && after.discord_window.is_some(),
                after.overlay_context_ok,
                after.protection_engaged,
            ));
        }
        build_verdict(
            verb,
            outcome,
            &observation,
            "none",
            None,
            stages,
            in_order,
            None,
            Some(&after),
            outcome_detail,
        )
    }

    /// Report a verdict for a trigger that could not be run at all, without
    /// touching any OSL state. Used when a previous run or drive is still in
    /// flight, when a request is refused before a verb could be chosen, and
    /// when a run stalls past its bound.
    fn refused_verdict(
        outcome: &'static str,
        reason: &'static str,
        outcome_detail: VerbOutcome,
    ) -> Verdict {
        let (stages, in_order) = stage_criteria(&[]);
        build_verdict(
            // A refusal makes no verb's claims. `Send` is the strictest set and
            // every one of its criteria is false here, so the verdict cannot
            // pass -- which is the whole point of failing closed.
            Verb::Send,
            outcome,
            &Observation::unknown(reason),
            "none",
            None,
            stages,
            in_order,
            Some(reason),
            None,
            outcome_detail,
        )
    }

    /// `osl-qa-selftest.{token}.…` for this instance's bundle identifier.
    fn addressed_name(format: &str, instance: &str) -> String {
        format.replace("{token}", &instance_file_token(instance))
    }

    /// Read one trigger body, bounded. A trigger that cannot be read at all is
    /// `None`, which the caller treats as "not there this tick" -- never as an
    /// empty body, because an empty body means *send*.
    fn read_trigger(path: &Path) -> Option<String> {
        let metadata = std::fs::metadata(path).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_REQUEST_BYTES {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn fingerprint(body: &str) -> u64 {
        use std::hash::{Hash as _, Hasher as _};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        body.hash(&mut hasher);
        // Zero is "nothing declined yet", so never let a real body claim it.
        hasher.finish() | 1
    }

    /// Say, in writing, that this instance saw a trigger meant for another one
    /// and deliberately left it alone.
    ///
    /// Deleting it would be the racy behaviour this addressing exists to
    /// remove, and saying nothing would be a silent no-op. The record goes to
    /// this instance's own decline path so it cannot clobber the verdict the
    /// addressed instance is about to write, and it is written once per
    /// distinct body rather than twice a second for as long as the other
    /// instance takes to collect its trigger.
    fn record_decline(instance: &str, declared: &str, body: &str) {
        let previous = fingerprint(body);
        if DECLINED_REQUEST_FINGERPRINT.swap(previous, Ordering::SeqCst) == previous {
            return;
        }
        let record = serde_json::json!({
            "schemaVersion": 1,
            "observedAtUnixMs": now_unix_ms(),
            "instance": instance,
            "declaredInstance": declared,
            "trigger": TRIGGER_FILE,
            "requestStatus": "declined-wrong-instance",
            "action": "left-for-its-owner",
        });
        let Ok(encoded) = serde_json::to_vec_pretty(&record) else {
            return;
        };
        let path = temp_path(&addressed_name(ADDRESSED_DECLINE_FORMAT, instance));
        let mut partial = path.as_os_str().to_owned();
        partial.push(VERDICT_PARTIAL_SUFFIX);
        let partial = PathBuf::from(partial);
        if std::fs::write(&partial, &encoded).is_ok() {
            let _ = std::fs::rename(&partial, &path);
        }
    }

    /// Poll for a trigger and run one scenario per file.
    ///
    /// TWO TRIGGER PATHS, AND WHY.
    ///
    /// `%TEMP%\osl-qa-selftest.request` is a **global rendezvous whose first
    /// consumer wins it**. With one instance that is fine and it is what the
    /// existing harness writes. With two it is a race: a trigger meant for the
    /// receiving instance can be eaten by the sending one, and nothing in the
    /// resulting verdict says which instance answered.
    ///
    /// So this watcher also polls
    /// `%TEMP%\osl-qa-selftest.{identifier}.request` and answers it into
    /// `%TEMP%\osl-qa-selftest.{identifier}.json`. **Scoping the path is the
    /// primary fix** rather than declaring the instance in the body, for one
    /// reason: two instances then never poll the same path, so the race is gone
    /// *by construction* instead of being resolved after the fact. A
    /// body-declared instance cannot achieve that on its own -- whichever
    /// instance reads the shared file first has to decide, and if it deletes a
    /// trigger addressed elsewhere the intended instance never sees it at all.
    ///
    /// The declaration is still honoured, as the **second** layer: any request
    /// body may carry `"instance"`, and a body whose declaration does not match
    /// this process refuses. On the addressed path that is a written verdict --
    /// the file named this instance, so answering it is this instance's job and
    /// a body that contradicts the file name is a harness bug worth naming. On
    /// the shared path the trigger is **left where it is** for its owner and
    /// the decline is recorded separately, because consuming it is the exact
    /// failure being prevented. Together: the path makes the race impossible
    /// and the declaration proves it, so a rig that forgot to give the second
    /// instance its own `%TEMP%` is caught rather than silently mismeasured.
    ///
    /// The trigger is consumed (deleted) before the run starts, so a crash
    /// mid-run cannot re-fire it and a harness can tell "picked up" from "not
    /// picked up". The verdict for that same path is removed at the same
    /// moment, so a harness waiting for it to reappear can never read a stale
    /// one.
    ///
    /// The scenario runs on its own thread and this one supervises it, so a
    /// run that wedges inside a UI-thread round trip still produces a verdict
    /// and still leaves the watcher able to answer the next trigger.
    pub(crate) fn spawn_trigger_watcher(app: tauri::AppHandle) {
        // Read once, on the watcher thread, from the same config the
        // single-instance marker window class is built from -- the only thing
        // that tells two OSL builds apart.
        let instance = app.config().identifier.clone();
        let addressed_trigger = temp_path(&addressed_name(ADDRESSED_TRIGGER_FORMAT, &instance));
        let addressed_verdict = temp_path(&addressed_name(ADDRESSED_VERDICT_FORMAT, &instance));
        let legacy_trigger = temp_path(TRIGGER_FILE);
        let legacy_verdict = temp_path(VERDICT_FILE);
        let _ = std::thread::Builder::new()
            .name("osl-qa-selftest".to_owned())
            .spawn(move || loop {
                std::thread::sleep(POLL_INTERVAL);

                // The addressed path first: it is unambiguous, so a harness
                // that uses it is never made to wait behind the shared one.
                let picked = if let Some(body) = read_trigger(&addressed_trigger) {
                    if std::fs::remove_file(&addressed_trigger).is_err() {
                        continue;
                    }
                    Some((body, addressed_trigger.clone(), addressed_verdict.clone()))
                } else if let Some(body) = read_trigger(&legacy_trigger) {
                    // Decide BEFORE consuming. A trigger declared for another
                    // instance must survive this tick.
                    let declared = match parse_request(&body) {
                        ParsedRequest::Accepted(request) => request.instance,
                        // An unparseable body declares nothing, so it is this
                        // instance's to consume and to refuse by name below.
                        ParsedRequest::Refused(_) => None,
                    };
                    match declared {
                        Some(declared) if declared != instance => {
                            record_decline(&instance, &declared, &body);
                            continue;
                        }
                        _ => {
                            if std::fs::remove_file(&legacy_trigger).is_err() {
                                continue;
                            }
                            Some((body, legacy_trigger.clone(), legacy_verdict.clone()))
                        }
                    }
                } else {
                    None
                };
                let Some((body, trigger_path, verdict_path)) = picked else {
                    continue;
                };
                let trigger_name = trigger_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| TRIGGER_FILE.to_owned());
                let _ = std::fs::remove_file(&verdict_path);

                let outcome_detail = || VerbOutcome::new("none", &instance, &trigger_name);
                let request = match parse_request(&body) {
                    ParsedRequest::Accepted(request) => request,
                    ParsedRequest::Refused(refusal) => {
                        // Fail closed and say so. An unreadable request is
                        // never quietly downgraded to the one verb with an
                        // irreversible side effect.
                        write_verdict(
                            &refused_verdict(
                                "refused",
                                "The self-test request could not be understood",
                                outcome_detail().refused(refusal),
                            ),
                            &verdict_path,
                        );
                        continue;
                    }
                };
                // The addressed path already named this instance; a body that
                // contradicts it is a harness bug, and answering it anyway
                // would attribute one instance's measurement to the other.
                if !osl_privacy_hub::qa_selftest_request::request_is_for_me(
                    request.instance.as_deref(),
                    &instance,
                ) {
                    let mut detail =
                        VerbOutcome::new(request.verb.label(), &instance, &trigger_name)
                            .refused("declined-wrong-instance");
                    detail.request_format = request.format;
                    write_verdict(
                        &refused_verdict(
                            "refused",
                            "This self-test request was addressed to another instance",
                            detail,
                        ),
                        &verdict_path,
                    );
                    continue;
                }

                let mut busy_detail =
                    VerbOutcome::new(request.verb.label(), &instance, &trigger_name);
                busy_detail.request_format = request.format;
                if RUN_IN_FLIGHT.load(Ordering::SeqCst)
                    || SEND_IN_FLIGHT.load(Ordering::SeqCst)
                    || DRIVE_IN_FLIGHT.load(Ordering::SeqCst)
                {
                    write_verdict(
                        &refused_verdict(
                            "busy",
                            "A previous self-test invocation has not finished",
                            busy_detail,
                        ),
                        &verdict_path,
                    );
                    continue;
                }
                RUN_IN_FLIGHT.store(true, Ordering::SeqCst);
                let (sender, receiver) = std::sync::mpsc::channel();
                let run_app = app.clone();
                let run_request = request.clone();
                let run_instance = instance.clone();
                let run_trigger = trigger_name.clone();
                if std::thread::Builder::new()
                    .name("osl-qa-selftest-run".to_owned())
                    .spawn(move || {
                        let verdict = run_once(&run_app, &run_request, &run_instance, &run_trigger);
                        RUN_IN_FLIGHT.store(false, Ordering::SeqCst);
                        // Dropped on the floor when this run already stalled
                        // past its budget: the stalled verdict is the record.
                        let _ = sender.send(verdict);
                    })
                    .is_err()
                {
                    RUN_IN_FLIGHT.store(false, Ordering::SeqCst);
                    write_verdict(
                        &refused_verdict(
                            "busy",
                            "The self-test run thread could not be started",
                            busy_detail,
                        ),
                        &verdict_path,
                    );
                    continue;
                }
                match receiver.recv_timeout(run_timeout(request.verb)) {
                    Ok(verdict) => write_verdict(&verdict, &verdict_path),
                    Err(_) => write_verdict(
                        &refused_verdict(
                            "stalled",
                            "The self-test run did not finish inside its bound",
                            busy_detail,
                        ),
                        &verdict_path,
                    ),
                }
            });
    }
}

/// Wake-up channel for the one lifecycle tick.
///
/// The tick sleeps on the condvar rather than a bare `thread::sleep` so the
/// password gate can ask for a pass *immediately* on unlock instead of leaving
/// expired content sitting for up to one interval. `nudge` is a lock, an
/// increment and a notify: it does no I/O, takes no ledger lock, and cannot
/// fail, so it can never turn unlocking OSL into a wait on a sweep — let alone
/// into a refusal to let the operator into their own app.
#[derive(Default)]
struct LifecycleTickState {
    generation: std::sync::Mutex<u64>,
    wake: std::sync::Condvar,
}

impl LifecycleTickState {
    fn nudge(&self) {
        if let Ok(mut generation) = self.generation.lock() {
            *generation = generation.wrapping_add(1);
        }
        self.wake.notify_all();
    }

    /// Sleep until the next pass is due, or until someone nudges.
    fn wait_for_next_pass(&self, interval: std::time::Duration) {
        let Ok(generation) = self.generation.lock() else {
            // A poisoned mutex must not stop the sweep. Fall back to a plain
            // sleep so expiry keeps happening on the slower schedule.
            std::thread::sleep(interval);
            return;
        };
        let _ = self.wake.wait_timeout(generation, interval);
    }
}

/// How many lifecycle ticks pass between attachment-deletion-outbox retries.
///
/// The tick absorbs what used to be a second, 15-minute thread. The ledger and
/// staging legs want to run often; the outbox leg wants to stay slow, because a
/// pass with work spends real network round trips and its author picked that
/// interval deliberately. Deriving the ratio from both constants means the two
/// cannot drift apart, and OSL does not become twenty times chattier merely
/// because the schedulers were merged.
const DELETION_DRAIN_EVERY_N_PASSES: u64 = {
    let drain = native_attachment_transport::DELETION_DRAIN_INTERVAL.as_secs();
    let tick = osl_privacy_hub::message_expiry::LIFECYCLE_TICK_INTERVAL.as_secs();
    if tick == 0 || drain <= tick {
        1
    } else {
        drain / tick
    }
};

/// The one scheduler in this app.
///
/// Runs a bounded expiry sweep every
/// [`message_expiry::LIFECYCLE_TICK_INTERVAL`](osl_privacy_hub::message_expiry::LIFECYCLE_TICK_INTERVAL),
/// starting with one pass immediately, and retries the attachment deletion
/// outbox every [`DELETION_DRAIN_EVERY_N_PASSES`] passes.
///
/// Three properties this deliberately has:
///
/// * **It runs regardless of eye state.** Nothing here consults the overlay, the
///   focused window, or whether Discord is visible. Content that has died must
///   die whether or not anyone is looking at it.
/// * **It becomes effective when the gate opens, without being coupled to it.**
///   Both ledgers and the outbox are sealed with the file storage key, so a pass
///   taken while OSL is locked reads nothing and reports nothing. It does not
///   have to be told about the unlock to start working — but the unlock nudges it
///   anyway so the first post-unlock purge is prompt rather than up to an
///   interval late. This is why `scavenge_staging_on_startup` was the wrong home
///   for any of it: that call happens before a key exists.
/// * **A failed pass is not fatal.** [`message_expiry::run_pass`] never returns
///   an error, and the outbox leg is detached, so a dead cipher store or an
///   unwritable ledger costs one degraded pass and is retried next tick.
fn spawn_lifecycle_tick(app: tauri::AppHandle, local_data_dir: std::path::PathBuf) {
    let _ = std::thread::Builder::new()
        .name("osl-lifecycle-tick".to_owned())
        .spawn(move || {
            let mut passes = 0u64;
            loop {
                // A store handle is only borrowed opportunistically: the decrypt
                // path holds this mutex while it works, and a sweep must never
                // queue behind it. A skipped shred is retried next tick.
                let core = app.state::<HubCoreState>();
                let store_guard = core.osl.message_store.try_lock().ok();
                let store = store_guard.as_ref().and_then(|slot| slot.as_ref());
                let _report = osl_privacy_hub::message_expiry::run_pass(
                    &local_data_dir,
                    store,
                    ipc::main_password::now_unix_secs_pub(),
                );
                drop(store_guard);

                if passes % DELETION_DRAIN_EVERY_N_PASSES == 0 {
                    // Detached for the same reason the unlock path detaches it:
                    // a pass can spend several network round trips, and the tick
                    // must not stall the ledger legs behind a dead store.
                    native_attachment_transport::drain_pending_deletions_detached(&app);
                }
                passes = passes.wrapping_add(1);

                app.state::<LifecycleTickState>()
                    .wait_for_next_pass(osl_privacy_hub::message_expiry::LIFECYCLE_TICK_INTERVAL);
            }
        });
}

fn main() {
    #[cfg(feature = "discord-qa-shell")]
    {
        // This is deliberately before the guardian, breadcrumbs, plugins,
        // profile resolution and setup. A blocked build therefore cannot
        // create an identity, register, touch a ledger, or contact a server.
        let preflight_only =
            std::env::args_os().any(|arg| arg == std::ffi::OsStr::new(B6_PREFLIGHT_ONLY_ARG));
        match discord_qa_b6_startup_gate() {
            Ok(true) if preflight_only => return,
            Ok(true) => {}
            Ok(false) => {
                eprintln!("B6 startup refused: retained preflight is not startup-allowed");
                std::process::exit(78);
            }
            Err(error) => {
                eprintln!("B6 startup refused: {error}");
                std::process::exit(78);
            }
        }
    }

    startup_breadcrumb("main_enter"); // STARTUP-TRACE
    startup_breadcrumb("guardian_check_before"); // STARTUP-TRACE
    if osl_privacy_hub::native_window_host::run_borrowed_window_guardian_if_requested() {
        startup_breadcrumb("guardian_check_after_early_return"); // STARTUP-TRACE
        return;
    }
    startup_breadcrumb("guardian_check_after"); // STARTUP-TRACE

    let builder = tauri::Builder::default();
    startup_breadcrumb("plugin_dialog_before"); // STARTUP-TRACE
    let builder = builder.plugin(tauri_plugin_dialog::init());
    startup_breadcrumb("plugin_dialog_after"); // STARTUP-TRACE
    startup_breadcrumb("plugin_single_instance_before"); // STARTUP-TRACE
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        startup_breadcrumb("single_instance_callback_fired"); // STARTUP-TRACE
        let restored = app.get_webview_window("main").is_some_and(|window| {
            main_window_is_live(&window)
                && protect_main_window_or_hide(&window)
                && window.unminimize().is_ok()
                && window.show().is_ok()
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
    }));
    startup_breadcrumb("plugin_single_instance_after"); // STARTUP-TRACE
    startup_breadcrumb("plugin_updater_before"); // STARTUP-TRACE
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());
    startup_breadcrumb("plugin_updater_after"); // STARTUP-TRACE
    let builder = builder.on_page_load(|webview, payload| {
        startup_breadcrumb("page_load_fired"); // STARTUP-TRACE
        #[cfg(windows)]
        {
            let _ = window_border::suppress_accent_border(webview);
            // The main window stays hidden from setup through page load. It is
            // revealed only after exact capture-affinity readback succeeds on
            // this HWND. Foreign native app windows remain outside this
            // boundary and are never claimed as protected.
            if webview.label() == "main" {
                if matches!(payload.event(), tauri::webview::PageLoadEvent::Started) {
                    let _ = webview.window().hide();
                }
                let protected = protect_main_webview_or_hide(webview);
                if protected && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                    let _ = webview.window().show();
                }
            } else {
                let _ = screenshot::apply_to_webview(webview, active_osl_capture_protection());
            }
        }
        // This hook fires twice for every load, once on PageLoadEvent::Started
        // and once on PageLoadEvent::Finished. Pre-warming on both spawned two
        // concurrent builders for the same window labels, and Tauri only
        // registers a label after the native window exists, so both passed
        // their "does it exist yet" check and each created a real HWND titled
        // "OSL private composer". Pre-warm once, after the load completes.
        if webview.label() == "main"
            && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
        {
            // Build the protected composer pair once, hidden, off the first
            // paint. Creating a WebView is what made the lock toggle slow;
            // with the pair retained the toggle is only a show()/hide().
            // It is idempotent and never activates a protection session.
            let prewarm_app = webview.window().app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = native_discord_overlay::prewarm(&prewarm_app);
            });
        }
    });
    let builder = builder.on_window_event(|window, event| {
        if window.label() != "main" {
            return;
        }
        #[cfg(windows)]
        if matches!(event, tauri::WindowEvent::Focused(true)) {
            if let Some(webview) = window.app_handle().get_webview_window("main") {
                protect_main_window_or_hide(&webview);
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
                    // Covers the window close button, the taskbar
                    // context-menu Close and Alt+F4 -- Windows turns all
                    // three into the same `WM_CLOSE`, which Tauri surfaces
                    // here. Programmatic exits are caught by the
                    // `ExitRequested` handler instead. Already off the
                    // event-loop thread, so this runs unbounded-by-caller
                    // and relies on its own internal budgets.
                    release_harnessed_windows_for_exit(&native_cleanup_app, false);
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
    });
    startup_breadcrumb("setup_before"); // STARTUP-TRACE
    let builder = builder.setup(|app| {
        startup_breadcrumb("setup_enter"); // STARTUP-TRACE
        let main_window = app
            .get_webview_window("main")
            .ok_or_else(|| "OSL main window is unavailable".to_owned())?;
        main_window
            .hide()
            .map_err(|_| "OSL main window could not start hidden".to_owned())?;
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|error| format!("could not resolve app config directory: {error}"))?;
        startup_breadcrumb("setup_step_01_config_dir_resolved"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        let config_dir = if config_dir
            .join("osl-core")
            .join("password_marker.json")
            .is_file()
        {
            // Never open or weaken a consumer password-protected profile.
            // Existing disposable VM QA profiles have no password marker,
            // so they retain their paired identity and receipts.
            config_dir.join("discord-qa-shell-v1")
        } else {
            config_dir
        };
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_02_qa_config_dir_remapped"); // STARTUP-TRACE
                                                                    // The app owns a separate OSL identity namespace. Never inherit
                                                                    // the original Discord client's `%APPDATA%/osl` login merely
                                                                    // because both applications run on the same Windows account.
        keystore::set_active_account_dir(None);
        startup_breadcrumb("setup_step_03_keystore_active_account_dir_cleared"); // STARTUP-TRACE
        let osl_core_dir = config_dir.join("osl-core");
        keystore::set_base_dir_override(Some(osl_core_dir.clone()));
        startup_breadcrumb("setup_step_04_keystore_base_dir_overridden"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        osl_privacy_hub::discord_qa_identity::install_device_bound_storage_key(&osl_core_dir)?;
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_05_qa_device_bound_storage_key_installed"); // STARTUP-TRACE
        let local_data_dir = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("could not resolve app local-data directory: {error}"))?;
        startup_breadcrumb("setup_step_06_local_data_dir_resolved"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        let local_data_dir = local_data_dir.join("discord-qa-shell-v1");
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_07_qa_local_data_dir_remapped"); // STARTUP-TRACE
        startup_breadcrumb("setup_step_08_scavenge_staging_before"); // STARTUP-TRACE
        peer_attachment_io::scavenge_staging_on_startup(&local_data_dir)?;
        startup_breadcrumb("setup_step_09_scavenge_staging_after"); // STARTUP-TRACE
                                                                    // Resume an already-committed gate burn before any identity can be
                                                                    // selected or decrypted. The recovery record contains no paths or
                                                                    // secrets; it only authorizes the same fixed-root idempotent purge.
        startup_breadcrumb("setup_step_10_resume_gate_burn_before"); // STARTUP-TRACE
        cleanup::resume_interrupted_gate_burn(&config_dir, &local_data_dir)?;
        startup_breadcrumb("setup_step_11_resume_gate_burn_after"); // STARTUP-TRACE
        startup_breadcrumb("setup_step_12_select_active_identity_before"); // STARTUP-TRACE
        identity_registry::select_active_identity_before_bootstrap()?;
        startup_breadcrumb("setup_step_13_select_active_identity_after"); // STARTUP-TRACE
        app.manage(PreviewState::load(
            config_dir.join("preview-preferences.json"),
        ));
        startup_breadcrumb("setup_step_14_preview_state_managed"); // STARTUP-TRACE
        app.manage(ServiceRegistryState::load(
            config_dir.join("service-registry.json"),
        ));
        startup_breadcrumb("setup_step_15_service_registry_state_managed"); // STARTUP-TRACE
        app.manage(ServiceScopeIndexState::load(
            config_dir.join("service-scope-index.json"),
        ));
        startup_breadcrumb("setup_step_16_service_scope_index_state_managed"); // STARTUP-TRACE
        startup_breadcrumb("setup_step_17_hub_core_bootstrap_before"); // STARTUP-TRACE
        let core = HubCoreState::bootstrap_from_disk();
        startup_breadcrumb("setup_step_18_hub_core_bootstrap_after"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_19_qa_disposable_identity_before"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        osl_privacy_hub::discord_qa_identity::ensure_disposable_identity(&core)?;
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_20_qa_disposable_identity_after"); // STARTUP-TRACE
        let security_state = HubSecurityState::default();
        startup_breadcrumb("setup_step_21_security_state_created"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_22_qa_pairing_before"); // STARTUP-TRACE
        #[cfg(feature = "discord-qa-shell")]
        osl_privacy_hub::discord_qa_identity::publish_and_consume_pairing(
            &osl_core_dir,
            &core,
            &security_state,
        )?;
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_23_qa_pairing_after"); // STARTUP-TRACE
        app.manage(core);
        startup_breadcrumb("setup_step_24_core_state_managed"); // STARTUP-TRACE
        app.manage(HubBrokerState::default());
        startup_breadcrumb("setup_step_25_broker_state_managed"); // STARTUP-TRACE
        app.manage(security_state);
        startup_breadcrumb("setup_step_26_security_state_managed"); // STARTUP-TRACE
        app.manage(HubIdentityRegistryState::default());
        startup_breadcrumb("setup_step_27_identity_registry_state_managed"); // STARTUP-TRACE
        app.manage(ServiceHostState::default());
        startup_breadcrumb("setup_step_28_service_host_state_managed"); // STARTUP-TRACE
        app.manage(NativeWindowHostState::default());
        startup_breadcrumb("setup_step_29_native_window_host_state_managed"); // STARTUP-TRACE
        app.manage(NativeDiscordComposerState::default());
        startup_breadcrumb("setup_step_30_native_discord_composer_state_managed"); // STARTUP-TRACE
        app.manage(LocalCoverState::default());
        startup_breadcrumb("setup_step_31_local_cover_state_managed"); // STARTUP-TRACE
        app.manage(OverlaySessionState::default());
        startup_breadcrumb("setup_step_32_overlay_session_state_managed"); // STARTUP-TRACE
        app.manage(native_surface_capture::NativeSurfaceCaptureState::default());
        startup_breadcrumb("setup_step_33_native_surface_capture_state_managed"); // STARTUP-TRACE
        app.manage(MullvadWindowHostState::default());
        startup_breadcrumb("setup_step_34_mullvad_window_host_state_managed"); // STARTUP-TRACE
        app.manage(BrowserCompanionState::default());
        startup_breadcrumb("setup_step_35_browser_companion_state_managed"); // STARTUP-TRACE
        app.manage(HubAccountSessionState::default());
        startup_breadcrumb("setup_step_36_hub_account_session_state_managed"); // STARTUP-TRACE
        app.manage(MainWindowLifecycleState::default());
        startup_breadcrumb("setup_step_37_main_window_lifecycle_state_managed"); // STARTUP-TRACE
        app.manage(HubUpdaterState::default());
        startup_breadcrumb("setup_step_38_hub_updater_state_managed"); // STARTUP-TRACE
        app.manage(HubNotificationState::default());
        startup_breadcrumb("setup_step_39_hub_notification_state_managed"); // STARTUP-TRACE
        app.manage(ScrubIndexState::default());
        startup_breadcrumb("setup_step_40_scrub_index_state_managed"); // STARTUP-TRACE
        let registration_app = app.handle().clone();
        startup_breadcrumb("setup_step_41_register_after_bootstrap_spawn_before"); // STARTUP-TRACE
        tauri::async_runtime::spawn_blocking(move || {
            registration_app
                .state::<HubCoreState>()
                .register_after_local_bootstrap();
        });
        startup_breadcrumb("setup_step_42_register_after_bootstrap_spawn_after"); // STARTUP-TRACE
                                                                                  // A failed browser-profile deletion can leave a large tombstone.
                                                                                  // Retrying it synchronously here would hold the first paint behind
                                                                                  // an unbounded recursive filesystem walk. Tombstones are already
                                                                                  // detached from every live account name, so retry them off the UI
                                                                                  // startup path and keep failures pending for the next launch.
        let cleanup_app = app.handle().clone();
        startup_breadcrumb("setup_step_43_scavenge_tombstones_spawn_before"); // STARTUP-TRACE
        tauri::async_runtime::spawn_blocking(move || {
            let _ = service_host::desktop::scavenge_profile_tombstones_on_startup(&cleanup_app);
        });
        startup_breadcrumb("setup_step_44_scavenge_tombstones_spawn_after"); // STARTUP-TRACE
                                                                             // The one scheduler in this app. It prunes the expiry and receipt
                                                                             // ledgers, clears abandoned decrypted staging files, shreds the local
                                                                             // plaintext cache of whatever expired, and — every
                                                                             // `DELETION_DRAIN_EVERY_N_PASSES` passes — retries the remote
                                                                             // ciphertext OSL promised to delete but could not.
                                                                             //
                                                                             // Everything it touches except the staging directory is sealed with
                                                                             // the file storage key, so its passes are no-ops until the password
                                                                             // gate opens; the unlock nudges it so the first real pass is prompt.
                                                                             // This is also why `scavenge_staging_on_startup` above cannot be the
                                                                             // home for any of it: no key exists yet at that point.
        app.manage(LifecycleTickState::default());
        spawn_lifecycle_tick(app.handle().clone(), local_data_dir.clone());
        startup_breadcrumb("setup_step_45_lifecycle_tick_spawned"); // STARTUP-TRACE
                                                                    // Last, so every state the driver reads is already managed. It only
                                                                    // ever polls a trigger file; it starts no scenario on its own.
        #[cfg(feature = "discord-qa-shell")]
        qa_selftest::spawn_trigger_watcher(app.handle().clone());
        #[cfg(feature = "discord-qa-shell")]
        startup_breadcrumb("setup_step_46_qa_selftest_watcher_spawned"); // STARTUP-TRACE
        startup_breadcrumb("setup_done"); // STARTUP-TRACE
        Ok(())
    });
    let builder = builder.invoke_handler(tauri::generate_handler![
        get_onboarding_preferences,
        list_hub_app_notifications,
        set_hub_notifications_enabled,
        set_hub_screenshot_protection,
        save_onboarding_preferences,
        scan_local_privacy,
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
        begin_protected_browser_import,
        finish_protected_browser_import,
        launch_firefox_service,
        get_default_browser_companion_status,
        host_default_browser_companion,
        resize_default_browser_companion,
        focus_default_browser_companion,
        detach_default_browser_companion,
        host_native_app_window,
        native_app_takeover_requires_consent,
        discord_marker_available,
        resize_native_app_window,
        focus_native_app_window,
        detach_native_app_window,
        set_native_discord_protected_overlay_open,
        get_native_discord_overlay_state,
        prepare_native_discord_overlay_text,
        #[cfg(feature = "discord-qa-shell")]
            send_native_discord_qa_atomic_text,
        #[cfg(feature = "discord-qa-shell")]
        record_native_discord_qa_send_stage,
        #[cfg(feature = "discord-qa-shell")]
            send_native_discord_qa_probe,
        #[cfg(feature = "discord-qa-shell")]
            request_native_discord_visible_row_qa_receipt,
        #[cfg(feature = "discord-qa-shell")]
        run_native_discord_headless_qa,
        #[cfg(feature = "discord-qa-shell")]
        poll_native_discord_headless_qa,
        prepare_osl_chat_text,
        send_native_discord_overlay_carrier,
        open_native_discord_overlay_text,
        rehydrate_native_discord_overlay_history,
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
        verify_hub_friend_safety_number,
        remove_hub_friend,
        list_hub_people,
        set_hub_friend_nickname,
        set_active_hub_friend_permission,
            set_active_hub_friend_reach,
            revoke_active_hub_friend_scope,
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
    ]);
    startup_breadcrumb("run_before"); // STARTUP-TRACE
    let app = builder
        .build(tauri::generate_context!())
        .expect("error while running OSL Privacy");
    app.run(|app_handle, event| {
        // The backstop for every exit route that does not pass through the
        // main window's `CloseRequested` handler: `AppHandle::exit` from
        // anywhere (including the shutdown watchdog and the service-host
        // shutdown task), `request_restart`, and the runtime's own
        // last-window-closed exit. `begin_harness_release` makes it a no-op
        // once the close handler has already released the window, so the
        // ordinary path pays nothing here.
        if let tauri::RunEvent::ExitRequested { code, .. } = event {
            release_harnessed_windows_bounded(app_handle, code == Some(tauri::RESTART_EXIT_CODE));
        }
    });
}

#[cfg(all(test, feature = "discord-qa-shell"))]
mod b6_startup_gate_tests {
    #[test]
    fn b6_gate_is_textually_before_every_qa_startup_side_effect() {
        let source = include_str!("main.rs");
        let main_start = source.find("fn main()").expect("main must exist");
        let source = &source[main_start..];
        let gate = source
            .find("match discord_qa_b6_startup_gate()")
            .expect("QA startup must enforce the B6 preflight");
        let first_breadcrumb = source
            .find("startup_breadcrumb(\"main_enter\")")
            .expect("startup breadcrumb must remain");
        let identity = source
            .find("ensure_disposable_identity(&core)")
            .expect("QA identity bootstrap must remain");
        let registration = source
            .find("publish_and_consume_pairing(")
            .expect("QA registration/pairing must remain");
        assert!(gate < first_breadcrumb);
        assert!(gate < identity);
        assert!(gate < registration);
        assert!(
            source[..first_breadcrumb].contains("std::process::exit(78)"),
            "a rejected or unwritable preflight must terminate before startup"
        );
    }

    #[test]
    fn b6_controllers_read_the_retained_preflight_before_consent_or_drive() {
        let launcher = include_str!("../../../scripts/qa/osl-launch-instance-b.ps1");
        let launcher_read = launcher
            .find("$b6 = $b6Receipt.b6Preflight")
            .expect("instance launcher must read b6Preflight");
        let launcher_consent = launcher
            .find("if (-not $ConfirmCreatesIdentity)")
            .expect("identity consent gate must remain");
        assert!(launcher_read < launcher_consent);

        let controller = include_str!("../../../scripts/qa/osl-p2p-loop.ps1");
        let controller_read = controller
            .find("$b6A = Read-B6StartupReceipt")
            .expect("two-identity controller must read both B6 receipts");
        let controller_consent = controller
            .find("# G0 consent")
            .expect("live-drive consent gate must remain");
        assert!(controller_read < controller_consent);
        assert!(
            controller.contains("'negativeCrossPeerIsolation'"),
            "the retained receipt gate must bind the eighth starvation fact"
        );
    }
}

#[cfg(all(test, feature = "discord-qa-shell"))]
mod native_visible_row_qa_command_tests {
    use super::canonical_native_visible_row_qa_build_hash;

    fn command_is_registered(source: &str) -> bool {
        let Some(handler_start) = source.find("tauri::generate_handler![") else {
            return false;
        };
        let Some(handler_end) = source[handler_start..].find("]);") else {
            return false;
        };
        source[handler_start..handler_start + handler_end]
            .contains("request_native_discord_visible_row_qa_receipt,")
    }

    #[test]
    fn native_visible_row_qa_command_is_reachable_only_through_trusted_state() {
        let source = include_str!("main.rs");
        assert!(command_is_registered(source));
        let start = source
            .find(
                "#[cfg(feature = \"discord-qa-shell\")]\n#[tauri::command]\nasync fn request_native_discord_visible_row_qa_receipt(",
            )
            .expect("QA receipt command must exist");
        let end = source[start..]
            .find("#[tauri::command]\nfn send_native_discord_overlay_carrier(")
            .map(|offset| start + offset)
            .expect("QA receipt command must remain bounded");
        let command = &source[start..end];
        for required in [
            "#[tauri::command]",
            "caller.label() != \"main\"",
            "require_engaged_lock(&app)?",
            "active_unlocked_osl_user_id(",
            "require_overlay_context_snapshot(&app)?",
            "native_discord_scope_binding(&app)?",
            "require_same_overlay_context(&app, epoch, &context_host)?",
            "trusted_native_visible_row_qa_caller_identity(&caller)?",
            "spawn_blocking(move ||",
            "broker::request_native_visible_row_runtime_receipt(",
            "broker::persist_native_visible_row_runtime_receipt(&receipt)?;",
            "MAX_VISIBLE_CARRIER_ROWS",
        ] {
            assert!(command.contains(required), "missing command gate: {required}");
        }
        assert_eq!(
            command.matches("require_engaged_lock(&app)?").count(),
            2,
            "the lock must be checked before and after the native read"
        );
        assert_eq!(
            command
                .matches("require_same_overlay_context(&app, epoch, &context_host)?")
                .count(),
            2,
            "the broker context must be checked before and after the native read"
        );

        let signature_end = command.find(") -> Result<").expect("command signature");
        let signature = &command[..signature_end];
        assert!(!signature.contains("String"));
        assert!(!signature.contains("u64"));
        assert!(!signature.contains("usize"));

        let registration_removed = source.replacen(
            "            request_native_discord_visible_row_qa_receipt,",
            "",
            1,
        );
        assert!(
            !command_is_registered(&registration_removed),
            "removing the real handler registration must fail reachability"
        );
    }

    #[test]
    fn native_visible_row_qa_build_hash_is_present_canonical_and_bounded() {
        assert_eq!(
            canonical_native_visible_row_qa_build_hash(Some(
                "ABCDEF0123456789ABCDEF0123456789ABCDEF01"
            ))
            .unwrap(),
            "abcdef0123456789abcdef0123456789abcdef01"
        );
        assert!(canonical_native_visible_row_qa_build_hash(None).is_err());
        assert!(canonical_native_visible_row_qa_build_hash(Some("abc")).is_err());
        assert!(canonical_native_visible_row_qa_build_hash(Some(
            "gggggggggggggggggggggggggggggggggggggggg"
        ))
        .is_err());
    }
}
