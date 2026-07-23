//! Experimental Windows native-window hosting boundary.
//!
//! The public API accepts only [`NativeAppId`]. It never accepts an executable,
//! profile path, process id, window handle, URL, or command-line argument from
//! IPC. A native client is eligible only after its current first-party binary
//! has a verified secondary-instance switch that isolates all writable state in
//! an OSL-owned profile. Unsupported clients fail closed; callers must not fall
//! back to a web surface or the user's ordinary desktop-client session.
//!
//! Windows 10 1703 and later may reset a cross-process child's DPI awareness
//! during `SetParent`. The reviewed Windows path accepts that compatibility
//! tradeoff so the verified client becomes a real child of OSL's protected
//! top-level window. Capture protection is claimed only while the child
//! relationship, identity, visibility, and exact content bounds all reverify.

use crate::native_apps::NativeAppId;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[cfg(any(target_os = "windows", test))]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(target_os = "windows")]
use std::sync::{Arc, Mutex};

#[cfg(target_os = "windows")]
const TRUSTED_VERTICAL_RESERVE: i32 = 98;
#[cfg(any(target_os = "windows", test))]
const PROFILE_NAMESPACE: &str = "native-window-profiles-v1";
#[cfg(any(target_os = "windows", test))]
const BORROWED_CONTROL_SHIELD_WIDTH: i32 = 168;
#[cfg(any(target_os = "windows", test))]
const BORROWED_CONTROL_SHIELD_HEIGHT: i32 = 44;
#[cfg(any(target_os = "windows", test))]
const NATIVE_DISCORD_ACCOUNT_PREFIX: &str = "native-discord-";
const DISCORD_ACCESSIBILITY_ARGUMENT: &str = "--force-renderer-accessibility=complete";
#[cfg(any(target_os = "windows", test))]
const SIGNAL_PRIMARY_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";
#[cfg(any(target_os = "windows", test))]
const SIGNAL_PRIMARY_WINDOW_TITLE: &str = "Signal";
#[cfg(any(target_os = "windows", test))]
const WHATSAPP_PRIMARY_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
#[cfg(any(target_os = "windows", test))]
const WHATSAPP_PRIMARY_WINDOW_TITLE: &str = "WhatsApp";
#[cfg(any(target_os = "windows", test))]
const OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS: &str = "rctrl_renwnd32";
#[cfg(any(target_os = "windows", test))]
const OUTLOOK_NEW_PRIMARY_WINDOW_CLASS: &str = "WinUIDesktopWin32WindowClass";
// The containment gates below are necessary but not sufficient evidence that
// each Electron/client build preserves interaction and compositing semantics.
// Flip only after exact Windows builds pass the dedicated compatibility suite.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum WarmHostAction {
    Reuse,
    Replace,
}

#[cfg(any(target_os = "windows", test))]
fn warm_host_action(
    current_id: NativeAppId,
    current_mode: DiscordSessionMode,
    current_owner_namespace: &str,
    current_valid: bool,
    requested_id: NativeAppId,
    requested_mode: DiscordSessionMode,
    requested_owner_namespace: &str,
) -> WarmHostAction {
    if current_valid
        && current_id == requested_id
        && current_mode == requested_mode
        && current_owner_namespace == requested_owner_namespace
    {
        WarmHostAction::Reuse
    } else {
        WarmHostAction::Replace
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum ColdHostAction {
    LaunchDedicated,
    ClaimExisting,
}

#[cfg(any(target_os = "windows", test))]
fn cold_host_action(mode: DiscordSessionMode) -> ColdHostAction {
    match mode {
        DiscordSessionMode::Dedicated => ColdHostAction::LaunchDedicated,
        DiscordSessionMode::ExistingSession => ColdHostAction::ClaimExisting,
    }
}

#[cfg(any(target_os = "windows", test))]
fn should_relaunch_existing_session(id: NativeAppId, reason: NativeWindowHostReason) -> bool {
    matches!(
        id,
        NativeAppId::Discord
            | NativeAppId::Telegram
            | NativeAppId::Signal
            | NativeAppId::Whatsapp
            | NativeAppId::Outlook
    ) && reason == NativeWindowHostReason::ExistingSessionUnavailable
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_identity_fields_match(
    stored_pid: u32,
    current_pid: u32,
    stored_creation_time: u64,
    current_creation_time: u64,
    expected_session: u32,
    current_session: u32,
    expected_path: &Path,
    current_path: &Path,
) -> bool {
    stored_pid == current_pid
        && stored_creation_time == current_creation_time
        && expected_session == current_session
        && expected_path == current_path
}

#[cfg(test)]
fn existing_candidate_count(count: usize) -> Result<(), NativeWindowHostReason> {
    match count {
        1 => Ok(()),
        0 => Err(NativeWindowHostReason::ExistingSessionUnavailable),
        _ => Err(NativeWindowHostReason::ExistingSessionAmbiguous),
    }
}

#[cfg(any(target_os = "windows", test))]
fn existing_primary_candidate_index<F>(
    id: NativeAppId,
    count: usize,
    mut is_decoration: F,
) -> Result<usize, NativeWindowHostReason>
where
    F: FnMut(usize, usize) -> bool,
{
    match count {
        0 => Err(NativeWindowHostReason::ExistingSessionUnavailable),
        1 => Ok(0),
        _ if id == NativeAppId::Telegram => {
            let primary = (0..count)
                .filter(|target| {
                    (0..count)
                        .all(|candidate| candidate == *target || is_decoration(*target, candidate))
                })
                .collect::<Vec<_>>();
            if primary.len() == 1 {
                Ok(primary[0])
            } else {
                Err(NativeWindowHostReason::ExistingSessionAmbiguous)
            }
        }
        _ => Err(NativeWindowHostReason::ExistingSessionAmbiguous),
    }
}

#[cfg(any(target_os = "windows", test))]
fn existing_session_supported(id: NativeAppId) -> bool {
    matches!(
        id,
        NativeAppId::Discord
            | NativeAppId::Telegram
            | NativeAppId::Signal
            | NativeAppId::Whatsapp
            | NativeAppId::Outlook
    )
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_snapshot_pid_matches(stored_pid: u32, node_pid: u32) -> bool {
    stored_pid == node_pid
}

#[cfg(any(target_os = "windows", test))]
fn native_discord_account_id(owner_namespace: &str) -> Option<String> {
    let digest = owner_namespace.strip_prefix("owner-")?;
    if digest.len() != 48 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{NATIVE_DISCORD_ACCOUNT_PREFIX}{digest}"))
}

#[cfg(any(target_os = "windows", test))]
fn native_context_matches(
    attached: bool,
    id: NativeAppId,
    stored_owner_namespace: &str,
    requested_owner_namespace: &str,
) -> bool {
    attached && id == NativeAppId::Discord && stored_owner_namespace == requested_owner_namespace
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_presentation_matches(
    visible: bool,
    iconic: bool,
    expected_rect: [i32; 4],
    actual_rect: [i32; 4],
) -> bool {
    visible && !iconic && expected_rect == actual_rect
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_style_is_preserved(
    original_style: isize,
    original_ex_style: isize,
    current_style: isize,
    current_ex_style: isize,
) -> bool {
    original_style == current_style && original_ex_style == current_ex_style
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_task_ex_style(original: isize, app_window: isize, tool_window: isize) -> isize {
    (original & !app_window) | tool_window
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_owner_contract_unchanged(original_owner: isize, current_owner: isize) -> bool {
    original_owner == current_owner
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_owner_is_restorable(
    original_owner: isize,
    attached_owner: isize,
    current_owner: isize,
) -> bool {
    current_owner == original_owner || current_owner == attached_owner
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_restore_contract_matches(
    original_owner: isize,
    current_owner: isize,
    original_style: isize,
    current_style: isize,
    original_ex_style: isize,
    current_ex_style: isize,
    placement_matches: bool,
) -> bool {
    borrowed_owner_contract_unchanged(original_owner, current_owner)
        && original_style == current_style
        && original_ex_style == current_ex_style
        && placement_matches
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedMutationStage {
    Captured,
    GuardianArmed,
    OwnerLinked,
    TaskStyleApplied,
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_mutation_transition(
    current: BorrowedMutationStage,
    next: BorrowedMutationStage,
) -> bool {
    matches!(
        (current, next),
        (
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed
        ) | (
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::OwnerLinked
        ) | (
            BorrowedMutationStage::OwnerLinked,
            BorrowedMutationStage::TaskStyleApplied
        )
    )
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_guardian_identity_matches(
    stored_pid: u32,
    current_pid: u32,
    stored_creation_time: u64,
    current_creation_time: u64,
    stored_session: u32,
    current_session: u32,
    stored_path: &Path,
    current_path: &Path,
) -> bool {
    borrowed_identity_fields_match(
        stored_pid,
        current_pid,
        stored_creation_time,
        current_creation_time,
        stored_session,
        current_session,
        stored_path,
        current_path,
    )
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_focus_state_valid(visible: bool, iconic: bool) -> bool {
    visible && !iconic
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_control_shield_rect(
    window_origin: [i32; 2],
    window_size: [i32; 2],
) -> Option<[i32; 4]> {
    let [width, height] = window_size;
    if width <= 0 || height <= 0 {
        return None;
    }
    let shield_width = width.min(BORROWED_CONTROL_SHIELD_WIDTH);
    let shield_height = height.min(BORROWED_CONTROL_SHIELD_HEIGHT);
    let right = window_origin[0].checked_add(width)?;
    Some([
        right.checked_sub(shield_width)?,
        window_origin[1],
        right,
        window_origin[1].checked_add(shield_height)?,
    ])
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_control_shield_color(samples: &[[u8; 3]]) -> [u8; 3] {
    const DARK_FALLBACK: [u8; 3] = [35, 36, 40];
    const LIGHT_FALLBACK: [u8; 3] = [242, 243, 245];
    if samples.len() < 4 {
        return DARK_FALLBACK;
    }
    let mut minimum = [u8::MAX; 3];
    let mut maximum = [u8::MIN; 3];
    let mut totals = [0u32; 3];
    for sample in samples {
        for channel in 0..3 {
            minimum[channel] = minimum[channel].min(sample[channel]);
            maximum[channel] = maximum[channel].max(sample[channel]);
            totals[channel] += u32::from(sample[channel]);
        }
    }
    let average = [
        (totals[0] / samples.len() as u32) as u8,
        (totals[1] / samples.len() as u32) as u8,
        (totals[2] / samples.len() as u32) as u8,
    ];
    let low_variance = (0..3).all(|channel| maximum[channel] - minimum[channel] <= 12);
    if low_variance && average.iter().copied().max().unwrap_or(255) <= 96 {
        return average.map(|channel| channel.clamp(20, 96));
    }
    if low_variance && average.iter().copied().min().unwrap_or(0) >= 160 {
        return average.map(|channel| channel.clamp(160, 248));
    }
    let luminance =
        (u32::from(average[0]) * 54 + u32::from(average[1]) * 183 + u32::from(average[2]) * 19)
            / 256;
    if luminance >= 128 {
        LIGHT_FALLBACK
    } else {
        DARK_FALLBACK
    }
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_rect_choice(
    actual: Option<[i32; 4]>,
    iconic: bool,
    normal: Option<[i32; 4]>,
) -> Option<[i32; 4]> {
    let valid = |rect: [i32; 4]| (rect[2] > rect[0] && rect[3] > rect[1]).then_some(rect);
    actual
        .and_then(valid)
        .or_else(|| iconic.then_some(()).and_then(|_| normal.and_then(valid)))
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_presentation_attempt_limit(id: NativeAppId) -> usize {
    // Borrowed clients can remain iconic or asynchronously apply placement
    // after SW_RESTORE. Bounded retries let only the same verified HWND finish
    // restoring; they never discover or adopt a new one.
    match id {
        NativeAppId::Signal | NativeAppId::Whatsapp | NativeAppId::Outlook => 7,
        NativeAppId::Discord | NativeAppId::Telegram => 3,
    }
}

#[cfg(any(target_os = "windows", test))]
fn telegram_frame_decoration_geometry_matches(
    target_rect: [i32; 4],
    candidate_rect: [i32; 4],
) -> bool {
    const MAX_FRAME_THICKNESS: i32 = 16;
    let [target_left, target_top, target_right, target_bottom] = target_rect;
    let [left, top, right, bottom] = candidate_rect;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width <= 0 || height <= 0 {
        return false;
    }
    let vertical = width <= MAX_FRAME_THICKNESS
        && (right == target_left || left == target_right)
        && top <= target_top
        && bottom >= target_bottom
        && target_top.saturating_sub(top) <= MAX_FRAME_THICKNESS
        && bottom.saturating_sub(target_bottom) <= MAX_FRAME_THICKNESS;
    let horizontal = height <= MAX_FRAME_THICKNESS
        && (bottom == target_top || top == target_bottom)
        && left <= target_left
        && right >= target_right
        && target_left.saturating_sub(left) <= MAX_FRAME_THICKNESS
        && right.saturating_sub(target_right) <= MAX_FRAME_THICKNESS;
    vertical ^ horizontal
}

#[cfg(any(target_os = "windows", test))]
fn telegram_frame_decoration_matches(
    same_process: bool,
    owned_by_target: bool,
    popup: bool,
    child: bool,
    caption: bool,
    interactive_chrome: bool,
    target_rect: [i32; 4],
    candidate_rect: [i32; 4],
) -> bool {
    same_process
        && owned_by_target
        && popup
        && !child
        && !caption
        && !interactive_chrome
        && telegram_frame_decoration_geometry_matches(target_rect, candidate_rect)
}

#[cfg(test)]
fn mode_owns_process(mode: DiscordSessionMode) -> bool {
    mode == DiscordSessionMode::Dedicated
}

#[cfg(any(target_os = "windows", test))]
fn protected_child_mode_allowed(mode: DiscordSessionMode) -> bool {
    // A borrowed child could be destroyed with the OSL parent after an
    // abnormal shutdown. Only OSL-spawned, kill-on-job-close guests may enter
    // the protected child lifecycle.
    mode == DiscordSessionMode::Dedicated
}

#[cfg(any(target_os = "windows", test))]
fn protected_child_capture_claim(mode: DiscordSessionMode, status: NativeWindowHostStatus) -> bool {
    protected_child_mode_allowed(mode)
        && matches!(
            status,
            NativeWindowHostStatus::Hosted
                | NativeWindowHostStatus::Resized
                | NativeWindowHostStatus::Focused
        )
}

#[cfg(any(target_os = "windows", test))]
fn dedicated_window_class_allowed(id: NativeAppId, class_name: &str) -> bool {
    id != NativeAppId::Signal || class_name == SIGNAL_PRIMARY_WINDOW_CLASS
}

#[cfg(any(target_os = "windows", test))]
fn existing_window_identity_allowed(
    id: NativeAppId,
    visible: bool,
    class_name: &str,
    title: &str,
) -> bool {
    match id {
        NativeAppId::Signal => {
            class_name == SIGNAL_PRIMARY_WINDOW_CLASS && title == SIGNAL_PRIMARY_WINDOW_TITLE
        }
        NativeAppId::Whatsapp => {
            class_name == WHATSAPP_PRIMARY_WINDOW_CLASS && title == WHATSAPP_PRIMARY_WINDOW_TITLE
        }
        NativeAppId::Outlook => {
            visible
                && matches!(
                    class_name,
                    OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS | OUTLOOK_NEW_PRIMARY_WINDOW_CLASS
                )
        }
        NativeAppId::Discord | NativeAppId::Telegram => visible,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeWindowHostStatus {
    Hosted,
    Resized,
    Focused,
    Detached,
    Unsupported,
    Failed,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscordSessionMode {
    Dedicated,
    ExistingSession,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NativeWindowHostReason {
    None,
    PlatformUnsupported,
    SecondaryInstanceUnverified,
    AppNotInstalled,
    ProfileUnavailable,
    ChannelNotOwned,
    NoChannelAvailable,
    ExistingSessionUnavailable,
    ExistingSessionAmbiguous,
    LaunchFailed,
    WindowNotFound,
    ProfileInitializationFailed,
    WindowIdentityChanged,
    OwnerWindowUnavailable,
    HostWindowUnavailable,
    ChildHierarchyRejected,
    ChildStyleRejected,
    ChildProcessRejected,
    ChildDpiRejected,
    ChildVisibilityRejected,
    ChildBoundsRejected,
    ChildSiblingRejected,
    BorrowedPlacementRejected,
    BorrowedStyleRejected,
    BorrowedVisibilityRejected,
    BorrowedBoundsRejected,
    WindowOperationRejected,
    NotHosted,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeWindowHostResult {
    pub id: NativeAppId,
    pub status: NativeWindowHostStatus,
    pub reason: NativeWindowHostReason,
    /// This is a fixed enum-like label, never a path, PID, HWND, title, or
    /// process error. It is safe to expose to the bundled UI.
    pub mode: &'static str,
    /// True only while the verified foreign window is a visible, non-iconic
    /// child of OSL's capture-excluded top-level window at the trusted bounds.
    pub capture_protected: bool,
}

impl NativeWindowHostResult {
    fn unsupported(id: NativeAppId, reason: NativeWindowHostReason) -> Self {
        Self {
            id,
            status: NativeWindowHostStatus::Unsupported,
            reason,
            mode: "none",
            capture_protected: false,
        }
    }

    #[cfg(target_os = "windows")]
    fn failed(id: NativeAppId, reason: NativeWindowHostReason) -> Self {
        Self {
            id,
            status: NativeWindowHostStatus::Failed,
            reason,
            mode: "none",
            capture_protected: false,
        }
    }

    #[cfg(any(target_os = "windows", test))]
    fn success(id: NativeAppId, status: NativeWindowHostStatus, mode: DiscordSessionMode) -> Self {
        Self::success_with_capture(id, status, mode, false)
    }

    #[cfg(any(target_os = "windows", test))]
    fn success_with_capture(
        id: NativeAppId,
        status: NativeWindowHostStatus,
        mode: DiscordSessionMode,
        capture_certified: bool,
    ) -> Self {
        Self {
            id,
            status,
            reason: NativeWindowHostReason::None,
            mode: match mode {
                DiscordSessionMode::Dedicated => "ownedBorderless",
                DiscordSessionMode::ExistingSession => "existingNativeCompanion",
            },
            capture_protected: capture_certified && protected_child_capture_claim(mode, status),
        }
    }
}

#[derive(Debug, Default)]
pub struct NativeWindowHostState {
    #[cfg(target_os = "windows")]
    inner: Mutex<Option<HostedWindow>>,
    #[cfg(target_os = "windows")]
    next_generation: AtomicU64,
}

#[cfg(target_os = "windows")]
impl Drop for NativeWindowHostState {
    fn drop(&mut self) {
        if let Ok(slot) = self.inner.get_mut() {
            if let Some(hosted) = slot.take() {
                unsafe { windows::shutdown_hosted(hosted) };
            }
        }
    }
}

// Handles are stored as integers so the state remains Send + Sync without
// claiming that foreign HWND pointer values may be dereferenced.
#[derive(Debug)]
#[cfg(target_os = "windows")]
struct HostedWindow {
    generation: u64,
    id: NativeAppId,
    mode: DiscordSessionMode,
    owner_namespace: String,
    window_process_id: u32,
    process: windows::HostedProcess,
    trusted_window_executable: windows::TrustedWindowExecutable,
    window: isize,
    trusted_parent: isize,
    previous_owner: isize,
    previous_style: isize,
    previous_ex_style: isize,
    previous_rect: [i32; 4],
    previous_iconic: bool,
    original_dpi_context: isize,
    capture_certified: bool,
    last_aligned_rect: Option<[i32; 4]>,
    borrowed_control_shield: Option<windows::BorrowedControlShield>,
    borrowed_tether: Option<windows::BorrowedWindowTether>,
    borrowed_recovery_guardian: Option<windows::BorrowedRecoveryGuardian>,
    attached: bool,
}

#[cfg(any(target_os = "windows", test))]
fn aligned_geometry_is_current(
    cached: Option<[i32; 4]>,
    expected: [i32; 4],
    actual: [i32; 4],
) -> bool {
    cached == Some(expected) && actual == expected
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherObservation {
    Aligned,
    ParentMinimized,
    TransientDesktopUnavailable,
    IdentityChanged,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherDecision {
    Continue,
    ContinueAfterTransient,
    FailClosed,
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_decision(
    observation: BorrowedTetherObservation,
    consecutive_transient_failures: usize,
    transient_failure_limit: usize,
) -> BorrowedTetherDecision {
    match observation {
        BorrowedTetherObservation::Aligned | BorrowedTetherObservation::ParentMinimized => {
            BorrowedTetherDecision::Continue
        }
        BorrowedTetherObservation::IdentityChanged => BorrowedTetherDecision::FailClosed,
        BorrowedTetherObservation::TransientDesktopUnavailable
            if consecutive_transient_failures < transient_failure_limit =>
        {
            BorrowedTetherDecision::ContinueAfterTransient
        }
        BorrowedTetherObservation::TransientDesktopUnavailable => {
            BorrowedTetherDecision::FailClosed
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_requires_repair(
    bounds_match: bool,
    visible: bool,
    composite_is_active: bool,
) -> bool {
    !bounds_match || !visible || composite_is_active
}

#[derive(Debug, Clone, Copy)]
#[cfg(target_os = "windows")]
pub(crate) struct NativeDiscordAccessibilityTarget {
    pub(crate) generation: u64,
    pub(crate) window: isize,
    pub(crate) process_id: u32,
}

/// Credential-free presentation facts for the exact signed Discord window
/// already claimed by `NativeWindowHostState`. This contains no title, account,
/// conversation, accessibility, or process data and grants no authority to
/// operate the foreign window.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct NativeDiscordOverlayTarget {
    pub generation: u64,
    pub rect: [i32; 4],
    pub foreground: bool,
}

impl NativeWindowHostState {
    /// Run one bounded Discord accessibility operation while the exact native
    /// host identity and generation remain locked. The callback receives no
    /// path, title, credential, session value, or arbitrary HWND from IPC.
    #[cfg(target_os = "windows")]
    pub(crate) fn with_current_discord_accessibility_target<T>(
        &self,
        owner_osl_user_id: &str,
        operation: impl FnOnce(
            NativeDiscordAccessibilityTarget,
            &dyn Fn(u32) -> bool,
        ) -> Result<T, String>,
    ) -> Result<T, String> {
        windows::with_current_discord_accessibility_target(self, owner_osl_user_id, operation)
    }

    /// Return a credential-free broker identity for the currently attached,
    /// signed native Discord host. The account id is derived only from the
    /// unlocked OSL owner namespace; no Discord profile or account data is read.
    pub fn current_discord_service_host(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<crate::service_host::ActiveServiceHost, String> {
        #[cfg(target_os = "windows")]
        {
            windows::current_discord_service_host(self, owner_osl_user_id)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = owner_osl_user_id;
            Err("The trusted native Discord host is unavailable".to_owned())
        }
    }

    pub fn discord_accessibility_snapshot(
        &self,
    ) -> crate::native_discord_adapter::NativeDiscordAccessibilitySnapshot {
        #[cfg(target_os = "windows")]
        {
            windows::discord_accessibility_snapshot(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            crate::native_discord_adapter::NativeDiscordAccessibilitySnapshot::unavailable(
                0,
                crate::native_discord_adapter::DiscordSnapshotReason::PlatformUnsupported,
            )
        }
    }

    pub fn discord_overlay_target(
        &self,
        owner_osl_user_id: &str,
    ) -> Result<NativeDiscordOverlayTarget, String> {
        #[cfg(target_os = "windows")]
        {
            windows::discord_overlay_target(self, owner_osl_user_id)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = owner_osl_user_id;
            Err("The trusted native Discord window is unavailable".to_owned())
        }
    }

    /// Launch a distinct, empty OSL-owned client profile and visually dock only
    /// the window created by that exact spawned process.
    pub fn host(
        &self,
        id: NativeAppId,
        osl_profile_root: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
    ) -> NativeWindowHostResult {
        self.host_mode(
            id,
            osl_profile_root,
            owner_osl_user_id,
            trusted_parent,
            DiscordSessionMode::Dedicated,
        )
    }

    pub fn host_mode(
        &self,
        id: NativeAppId,
        osl_profile_root: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
        mode: DiscordSessionMode,
    ) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::host(
                self,
                id,
                osl_profile_root,
                owner_osl_user_id,
                trusted_parent,
                mode,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (osl_profile_root, owner_osl_user_id, trusted_parent, mode);
            NativeWindowHostResult::unsupported(id, NativeWindowHostReason::PlatformUnsupported)
        }
    }

    pub fn resize(&self, trusted_parent: isize) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::resize(self, trusted_parent)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = trusted_parent;
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    pub fn focus(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::focus(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    /// Hide the exact OSL-spawned process window while retaining one warm
    /// contained native session. The user's ordinary app instance is never
    /// enumerated or changed by this operation.
    pub fn detach(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::detach(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }

    /// Restore the hosted window and terminate its complete contained process
    /// tree. Security transitions such as identity change, stealth, burn, and
    /// application shutdown must use this rather than warm detach.
    pub fn terminate(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::terminate(self)
        }
        #[cfg(not(target_os = "windows"))]
        {
            NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::PlatformUnsupported,
            )
        }
    }
}

/// Handles the internal crash-recovery subprocess before Tauri initializes.
/// Returns true only when this process was invoked in the fixed guardian mode
/// and must exit immediately afterward.
pub fn run_borrowed_window_guardian_if_requested() -> bool {
    #[cfg(target_os = "windows")]
    {
        windows::run_borrowed_guardian_if_requested(&std::env::args().collect::<Vec<_>>())
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[cfg(any(target_os = "windows", test))]
fn profile_component(id: NativeAppId) -> &'static str {
    match id {
        NativeAppId::Discord => "discord",
        NativeAppId::Telegram => "telegram",
        NativeAppId::Signal => "signal",
        NativeAppId::Whatsapp => "whatsapp",
        NativeAppId::Outlook => "outlook",
    }
}

#[cfg(any(target_os = "windows", test))]
fn profile_relative_components(
    owner_osl_user_id: &str,
    id: NativeAppId,
) -> Result<[String; 3], NativeWindowHostReason> {
    let owner_namespace = crate::service_host::owner_profile_namespace(owner_osl_user_id)
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    Ok([
        PROFILE_NAMESPACE.to_owned(),
        owner_namespace,
        profile_component(id).to_owned(),
    ])
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum FixedSecondaryLaunch {
    DiscordDedicatedChannel,
    TelegramManyWorkdir,
    SignalUserDataDir,
    Unsupported,
}

#[cfg(any(target_os = "windows", test))]
fn fixed_secondary_launch(id: NativeAppId) -> FixedSecondaryLaunch {
    match id {
        NativeAppId::Discord => FixedSecondaryLaunch::DiscordDedicatedChannel,
        NativeAppId::Telegram => FixedSecondaryLaunch::TelegramManyWorkdir,
        NativeAppId::Signal => FixedSecondaryLaunch::SignalUserDataDir,
        NativeAppId::Whatsapp => FixedSecondaryLaunch::Unsupported,
        NativeAppId::Outlook => FixedSecondaryLaunch::Unsupported,
    }
}

// Discord dedicated hosting is restricted to the separately installed PTB
// channel. Stable is never claimed by this path. The signed PTB process is
// launched into a kill-on-close job and its fixed roaming directory must be
// either empty or already claimed by the same OSL identity.
#[cfg(any(target_os = "windows", test))]
const ENABLE_DISCORD_DEDICATED_CHANNEL_HOST: bool = true;
#[cfg(any(target_os = "windows", test))]
const ENABLE_TELEGRAM_SECONDARY_HOST: bool = true;
#[cfg(any(target_os = "windows", test))]
const ENABLE_SIGNAL_SECONDARY_HOST: bool = false;

/// No entry is enabled merely because an app is Electron or happens to accept
/// a Chromium switch. Telegram is enabled only after a local probe proved a
/// second visible process and writes inside the supplied empty OSL profile
/// while the ordinary client remained live. Signal Stable currently creates a
/// native `#32770` secondary-instance dialog rather than an Electron app window
/// when its ordinary session is live, so its dedicated gate remains closed.
/// Discord's Chromium
/// `--user-data-dir` switch does not isolate the official client. Discord can
/// therefore use only the separately installed, persistently OSL-claimed official
/// PTB channel; Stable and Canary remain outside dedicated hosting. WhatsApp exposes no fixed
/// secondary-profile switch, so it also fails closed.
#[cfg(any(target_os = "windows", test))]
fn secondary_instance_verified(id: NativeAppId) -> bool {
    match fixed_secondary_launch(id) {
        FixedSecondaryLaunch::DiscordDedicatedChannel => ENABLE_DISCORD_DEDICATED_CHANNEL_HOST,
        FixedSecondaryLaunch::TelegramManyWorkdir => ENABLE_TELEGRAM_SECONDARY_HOST,
        FixedSecondaryLaunch::SignalUserDataDir => ENABLE_SIGNAL_SECONDARY_HOST,
        FixedSecondaryLaunch::Unsupported => false,
    }
}

#[cfg(any(target_os = "windows", test))]
fn dedicated_launch_attempt_limit(id: NativeAppId) -> usize {
    // Telegram can exit its first `-many -workdir` launcher after preparing a
    // new isolated profile. Retrying that same fixed OSL-owned profile once is
    // safe; it never targets or discovers the user's ordinary Telegram data.
    if id == NativeAppId::Telegram {
        2
    } else {
        1
    }
}

#[cfg(any(target_os = "windows", test))]
fn child_presentation_attempt_limit(id: NativeAppId) -> usize {
    // Telegram applies Qt frame metrics after SetParent, while Signal's
    // Electron surface can publish its child bounds a little later. Retry the
    // same verified HWND only; discovery and identity are never broadened.
    match id {
        NativeAppId::Telegram => 2,
        // Signal can publish a short-lived Electron helper top-level after its
        // main HWND has already accepted SetParent. Keep verifying the same
        // signed process and exact HWND for a bounded three-second settle; an
        // extra real window still fails closed at the final sample.
        NativeAppId::Signal => 7,
        _ => 1,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum DiscordChannel {
    Stable,
    Ptb,
    Canary,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
struct DiscordChannelManifest {
    channel: DiscordChannel,
    claim_name: &'static str,
    install_directory: &'static str,
    executable_name: &'static str,
    data_directory: &'static str,
    package_id: &'static str,
}

#[cfg(any(target_os = "windows", test))]
const DISCORD_CHANNELS: &[DiscordChannelManifest] = &[
    DiscordChannelManifest {
        channel: DiscordChannel::Stable,
        claim_name: "stable",
        install_directory: "Discord",
        executable_name: "Discord.exe",
        data_directory: "discord",
        package_id: "Discord.Discord",
    },
    DiscordChannelManifest {
        channel: DiscordChannel::Ptb,
        claim_name: "ptb",
        install_directory: "DiscordPTB",
        executable_name: "DiscordPTB.exe",
        data_directory: "discordptb",
        package_id: "Discord.Discord.PTB",
    },
    DiscordChannelManifest {
        channel: DiscordChannel::Canary,
        claim_name: "canary",
        install_directory: "DiscordCanary",
        executable_name: "DiscordCanary.exe",
        data_directory: "discordcanary",
        package_id: "Discord.Discord.Canary",
    },
];

#[cfg(any(target_os = "windows", test))]
fn dedicated_discord_channels() -> impl Iterator<Item = &'static DiscordChannelManifest> {
    DISCORD_CHANNELS
        .iter()
        .filter(|channel| channel.channel == DiscordChannel::Ptb)
}

#[cfg(any(target_os = "windows", test))]
fn existing_discord_channel_executables<F>(mut resolve: F) -> Vec<PathBuf>
where
    F: FnMut(&DiscordChannelManifest) -> Option<PathBuf>,
{
    DISCORD_CHANNELS.iter().filter_map(&mut resolve).collect()
}

#[cfg(any(target_os = "windows", test))]
fn preferred_existing_discord_channel_executable<F>(mut resolve: F) -> Option<PathBuf>
where
    F: FnMut(&DiscordChannelManifest) -> Option<PathBuf>,
{
    // `Current account` means the ordinary Discord channel. Stable is the
    // deterministic first choice, while PTB remains the dedicated OSL channel.
    // Fall back only when the higher-priority official channel is absent; the
    // selected executable still passes the full publisher/path/PID/session and
    // exact-window checks below.
    DISCORD_CHANNELS.iter().find_map(&mut resolve)
}

#[cfg(any(target_os = "windows", test))]
const DISCORD_CLAIM_NAMESPACE: &str = "native-discord-channel-claims-v1";
#[cfg(any(target_os = "windows", test))]
const DISCORD_CLAIM_FORMAT: &str = "osl-native-discord-channel-claim-v1";

#[cfg(any(target_os = "windows", test))]
fn discord_claim_relative_path(
    owner_osl_user_id: &str,
    channel: &DiscordChannelManifest,
) -> Result<PathBuf, NativeWindowHostReason> {
    // Validate the owner even though the single channel-wide claim filename is
    // fixed. A channel must have exactly one authoritative claim; separate
    // per-owner filenames would allow two identities to race and both win.
    let _owner_namespace = crate::service_host::owner_profile_namespace(owner_osl_user_id)
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    Ok(PathBuf::from(DISCORD_CLAIM_NAMESPACE).join(format!("{}.claim", channel.claim_name)))
}

#[cfg(any(target_os = "windows", test))]
fn expected_discord_claim(
    owner_osl_user_id: &str,
    channel: &DiscordChannelManifest,
) -> Result<String, NativeWindowHostReason> {
    let owner_namespace = crate::service_host::owner_profile_namespace(owner_osl_user_id)
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    Ok(format!(
        "{DISCORD_CLAIM_FORMAT}\n{owner_namespace}\n{}\n",
        channel.claim_name
    ))
}

/// Claim only a fresh fixed Discord channel. This examines directory metadata
/// and names only; it never opens Discord databases, cookies, tokens, or other
/// profile contents. Once created, the owner-namespaced claim permits that same
/// OSL identity to reopen the channel after Discord has populated it.
#[cfg(any(target_os = "windows", test))]
fn claim_discord_channel(
    osl_profile_root: &Path,
    roaming_app_data: &Path,
    owner_osl_user_id: &str,
    channel: &DiscordChannelManifest,
) -> Result<PathBuf, NativeWindowHostReason> {
    if !osl_profile_root.is_absolute() || !roaming_app_data.is_absolute() {
        return Err(NativeWindowHostReason::ProfileUnavailable);
    }
    let relative_claim = discord_claim_relative_path(owner_osl_user_id, channel)?;
    let expected = expected_discord_claim(owner_osl_user_id, channel)?;
    let claim = osl_profile_root.join(&relative_claim);
    let data_root = roaming_app_data.join(channel.data_directory);

    if claim.is_file() {
        if std::fs::read_to_string(&claim).ok().as_deref() != Some(expected.as_str())
            || !claimed_discord_data_root_is_plain(&data_root)
        {
            return Err(NativeWindowHostReason::ChannelNotOwned);
        }
        return Ok(data_root);
    }
    if claim.exists() {
        return Err(NativeWindowHostReason::ChannelNotOwned);
    }

    match std::fs::symlink_metadata(&data_root) {
        Ok(metadata) => {
            if !plain_directory_metadata(&metadata) {
                return Err(NativeWindowHostReason::ChannelNotOwned);
            }
            let populated = std::fs::read_dir(&data_root)
                .map_err(|_| NativeWindowHostReason::ChannelNotOwned)?
                .next()
                .is_some();
            if populated {
                return Err(NativeWindowHostReason::ChannelNotOwned);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(NativeWindowHostReason::ChannelNotOwned),
    }

    std::fs::create_dir_all(osl_profile_root)
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    let canonical_root = osl_profile_root
        .canonicalize()
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    let mut canonical_parent = canonical_root.clone();
    for component in relative_claim
        .parent()
        .ok_or(NativeWindowHostReason::ProfileUnavailable)?
        .components()
    {
        let std::path::Component::Normal(component) = component else {
            return Err(NativeWindowHostReason::ProfileUnavailable);
        };
        canonical_parent.push(component);
        ensure_plain_claim_directory(&canonical_parent)?;
    }
    let claim = canonical_parent.join(
        relative_claim
            .file_name()
            .ok_or(NativeWindowHostReason::ProfileUnavailable)?,
    );

    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&claim)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return (std::fs::read_to_string(&claim).ok().as_deref() == Some(expected.as_str())
                && claimed_discord_data_root_is_plain(&data_root))
            .then_some(data_root)
            .ok_or(NativeWindowHostReason::ChannelNotOwned)
        }
        Err(_) => return Err(NativeWindowHostReason::ProfileUnavailable),
    };
    file.write_all(expected.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
    Ok(data_root)
}

#[cfg(any(target_os = "windows", test))]
fn claimed_discord_data_root_is_plain(data_root: &Path) -> bool {
    match std::fs::symlink_metadata(data_root) {
        Ok(metadata) => plain_directory_metadata(&metadata),
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

#[cfg(any(target_os = "windows", test))]
fn plain_directory_metadata(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

#[cfg(any(target_os = "windows", test))]
fn ensure_plain_claim_directory(path: &Path) -> Result<(), NativeWindowHostReason> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if plain_directory_metadata(&metadata) => Ok(()),
        Ok(_) => Err(NativeWindowHostReason::ProfileUnavailable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(path).map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
            plain_directory_metadata(&metadata)
                .then_some(())
                .ok_or(NativeWindowHostReason::ProfileUnavailable)
        }
        Err(_) => Err(NativeWindowHostReason::ProfileUnavailable),
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use crate::windows_executable_trust::{
        verify_executable, ExecutablePublisher, TrustedExecutable,
    };
    use sha2::{Digest, Sha256};
    use std::ffi::{c_void, OsString};
    use std::fs;
    use std::io::{Read, Write};
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::fs::MetadataExt;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{
        GetLastError, SetLastError, BOOL, FILETIME, HWND, LPARAM, POINT, RECT,
    };
    use windows_sys::Win32::Graphics::Gdi::{
        ClientToScreen, CreateSolidBrush, DeleteObject, FillRect, GetDC, GetPixel, MapWindowPoints,
        ReleaseDC,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::HiDpi::GetWindowDpiAwarenessContext;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath, ShellExecuteW,
        KF_FLAG_DEFAULT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, CreateWindowExW, DestroyWindow, DispatchMessageW, EnumWindows,
        GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow, GetParent,
        GetWindowDisplayAffinity, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
        GetWindowTextW, GetWindowThreadProcessId, IsChild, IsIconic, IsWindowVisible, PeekMessageW,
        SetForegroundWindow, SetParent, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos,
        ShowWindow, TranslateMessage, GA_ROOT, GWLP_HWNDPARENT, GWL_EXSTYLE, GWL_STYLE, HWND_TOP,
        MSG, PM_REMOVE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
        SWP_SHOWWINDOW, SW_MINIMIZE, SW_RESTORE, WDA_EXCLUDEFROMCAPTURE, WINDOWPLACEMENT,
        WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX,
        WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
    };
    const WINDOW_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
    // Cold desktop clients can spend several seconds in updater/bootstrap
    // work before their first real top-level window exists. Keep this below
    // the renderer's 30-second host deadline, but long enough that one click
    // remains sufficient after a Windows or app update.
    const EXISTING_SESSION_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
    const STABLE_WINDOW_SAMPLES: usize = 3;
    const DISCORD_STABLE_WINDOW_SAMPLES: usize = 20;
    const TELEGRAM_PRESENTATION_SETTLE_DELAY: Duration = Duration::from_millis(250);
    const SIGNAL_RESTORE_SETTLE_DELAY: Duration = Duration::from_millis(500);
    const PARENT_RESTORE_SETTLE: Duration = Duration::from_secs(3);
    const DISCORD_POST_ADOPTION_SETTLE: Duration = Duration::from_secs(3);
    const ERROR_SUCCESS: u32 = 0;
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x0000_1000;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_OBJECT_0: u32 = 0;
    const INFINITE: u32 = 0xffff_ffff;
    const BORROWED_GUARDIAN_MARKER: &str = "--osl-borrowed-window-guardian-v1";
    const BORROWED_TETHER_ACTIVE_INTERVAL: Duration = Duration::from_millis(16);
    const BORROWED_TETHER_IDLE_INTERVAL: Duration = Duration::from_millis(80);
    const THREAD_SUSPEND_RESUME: u32 = 0x0000_0002;
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    const CERTIFIED_TELEGRAM_WINDOWS_BUILD: u32 = 19_045;
    const CERTIFIED_TELEGRAM_SHA256: [u8; 32] = [
        0xd4, 0x67, 0x8b, 0x4c, 0x81, 0x5e, 0x60, 0x7f, 0x27, 0x69, 0x0d, 0x08, 0xab, 0xa4, 0xb0,
        0xa1, 0x9e, 0x3f, 0x22, 0xbd, 0x4e, 0x76, 0x5a, 0x06, 0xa0, 0xcb, 0x36, 0x96, 0xb5, 0xc4,
        0x26, 0xab,
    ];

    type RawHandle = *mut c_void;

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub(super) struct BorrowedWindowPlacement {
        flags: u32,
        show_cmd: u32,
        min_position: [i32; 2],
        max_position: [i32; 2],
        normal_position: [i32; 4],
    }

    #[repr(C)]
    struct RtlOsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        service_pack: [u16; 128],
    }

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn RtlGetVersion(version: *mut RtlOsVersionInfo) -> i32;
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[repr(C)]
    struct ThreadEntry32 {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_process_id: u32,
        base_priority: i32,
        priority_delta: i32,
        flags: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> RawHandle;
        fn SetInformationJobObject(
            job: RawHandle,
            info_class: i32,
            info: *const c_void,
            info_len: u32,
        ) -> BOOL;
        fn AssignProcessToJobObject(job: RawHandle, process: RawHandle) -> BOOL;
        fn IsProcessInJob(process: RawHandle, job: RawHandle, result: *mut BOOL) -> BOOL;
        fn TerminateJobObject(job: RawHandle, exit_code: u32) -> BOOL;
        fn OpenProcess(access: u32, inherit_handle: BOOL, process_id: u32) -> RawHandle;
        fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> BOOL;
        fn GetProcessTimes(
            process: RawHandle,
            creation: *mut FILETIME,
            exit: *mut FILETIME,
            kernel: *mut FILETIME,
            user: *mut FILETIME,
        ) -> BOOL;
        fn OpenThread(access: u32, inherit_handle: BOOL, thread_id: u32) -> RawHandle;
        fn ResumeThread(thread: RawHandle) -> u32;
        fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> RawHandle;
        fn Thread32First(snapshot: RawHandle, entry: *mut ThreadEntry32) -> BOOL;
        fn Thread32Next(snapshot: RawHandle, entry: *mut ThreadEntry32) -> BOOL;
        fn QueryFullProcessImageNameW(
            process: RawHandle,
            flags: u32,
            path: *mut u16,
            path_len: *mut u32,
        ) -> BOOL;
        fn CloseHandle(handle: RawHandle) -> BOOL;
        fn WaitForSingleObject(handle: RawHandle, milliseconds: u32) -> u32;
    }

    #[derive(Debug)]
    pub(super) struct JobHandle(RawHandle);

    #[derive(Debug)]
    pub(super) enum HostedProcess {
        Dedicated {
            child: Child,
            job: JobHandle,
        },
        Borrowed {
            process_id: u32,
            creation_time: u64,
            process: ProcessHandle,
        },
    }

    #[derive(Debug)]
    pub(super) struct ProcessHandle(RawHandle);

    enum BorrowedWindowTetherCommand {
        Reconcile(mpsc::SyncSender<bool>),
        Stop,
    }

    #[derive(Debug)]
    pub(super) struct BorrowedWindowTether {
        commands: mpsc::Sender<BorrowedWindowTetherCommand>,
        worker: Option<JoinHandle<()>>,
        healthy: Arc<AtomicBool>,
        generation: Arc<AtomicU64>,
    }

    unsafe impl Send for BorrowedWindowTether {}

    impl BorrowedWindowTether {
        fn create(snapshot: BorrowedTetherSnapshot) -> Option<Self> {
            if !unsafe { borrowed_tether_identity_is_valid(&snapshot) } {
                return None;
            }
            let (commands, receiver) = mpsc::channel();
            let (ready_send, ready_receive) = mpsc::sync_channel(1);
            let healthy = Arc::new(AtomicBool::new(true));
            let worker_healthy = Arc::clone(&healthy);
            let generation = Arc::new(AtomicU64::new(snapshot.generation));
            let worker = thread::spawn(move || unsafe {
                borrowed_window_tether_worker(snapshot, receiver, ready_send, worker_healthy)
            });
            if ready_receive.recv_timeout(Duration::from_secs(1)).ok() != Some(true) {
                let _ = commands.send(BorrowedWindowTetherCommand::Stop);
                let _ = worker.join();
                return None;
            }
            Some(Self {
                commands,
                worker: Some(worker),
                healthy,
                generation,
            })
        }

        fn reconcile(&self, generation: u64) -> bool {
            if !self.is_healthy(generation) {
                return false;
            }
            let (send, receive) = mpsc::sync_channel(1);
            self.commands
                .send(BorrowedWindowTetherCommand::Reconcile(send))
                .is_ok()
                && receive
                    .recv_timeout(Duration::from_millis(500))
                    .unwrap_or(false)
        }

        fn is_healthy(&self, generation: u64) -> bool {
            self.healthy.load(Ordering::Acquire)
                && self.generation.load(Ordering::Acquire) == generation
        }

        fn rebind_generation(&self, generation: u64) {
            self.generation.store(generation, Ordering::Release);
        }

        fn stop(&mut self) {
            let _ = self.commands.send(BorrowedWindowTetherCommand::Stop);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    impl Drop for BorrowedWindowTether {
        fn drop(&mut self) {
            self.stop();
        }
    }

    #[derive(Debug, Clone)]
    struct BorrowedTetherSnapshot {
        generation: u64,
        window: isize,
        parent: isize,
        process_id: u32,
        creation_time: u64,
        session_id: u32,
        expected_path: PathBuf,
    }

    enum BorrowedControlShieldCommand {
        Position(mpsc::SyncSender<bool>),
        Stop,
    }

    #[derive(Debug)]
    pub(super) struct BorrowedControlShield {
        commands: mpsc::Sender<BorrowedControlShieldCommand>,
        worker: Option<JoinHandle<()>>,
    }

    unsafe impl Send for BorrowedControlShield {}

    impl BorrowedControlShield {
        fn create(target: HWND, expected_process_id: u32) -> Option<Self> {
            if target.is_null() || unsafe { window_process_id(target) } != Some(expected_process_id)
            {
                return None;
            }
            let (commands, receiver) = mpsc::channel();
            let (ready_send, ready_receive) = mpsc::sync_channel(1);
            let target_value = target as isize;
            let worker = thread::spawn(move || unsafe {
                borrowed_control_shield_worker(
                    target_value as HWND,
                    expected_process_id,
                    receiver,
                    ready_send,
                )
            });
            if ready_receive
                .recv_timeout(Duration::from_secs(1))
                .ok()
                .flatten()
                .is_none()
            {
                let _ = commands.send(BorrowedControlShieldCommand::Stop);
                let _ = worker.join();
                return None;
            }
            Some(Self {
                commands,
                worker: Some(worker),
            })
        }

        fn position(&self) -> bool {
            let (send, receive) = mpsc::sync_channel(1);
            self.commands
                .send(BorrowedControlShieldCommand::Position(send))
                .is_ok()
                && receive
                    .recv_timeout(Duration::from_millis(500))
                    .unwrap_or(false)
        }

        fn stop(&mut self) {
            let _ = self.commands.send(BorrowedControlShieldCommand::Stop);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    impl Drop for BorrowedControlShield {
        fn drop(&mut self) {
            self.stop();
        }
    }

    #[derive(Debug, Clone)]
    struct BorrowedRecoverySnapshot {
        id: NativeAppId,
        window: isize,
        process_id: u32,
        creation_time: u64,
        session_id: u32,
        expected_path: PathBuf,
        owner: isize,
        attached_owner: isize,
        style: isize,
        ex_style: isize,
        placement: BorrowedWindowPlacement,
    }

    #[derive(Debug)]
    pub(super) struct BorrowedRecoveryGuardian {
        child: Option<Child>,
        snapshot: BorrowedRecoverySnapshot,
    }

    impl BorrowedRecoveryGuardian {
        fn arm(snapshot: &BorrowedRecoverySnapshot) -> Option<Self> {
            let current = ProcessHandle::open(std::process::id())?;
            let (_, parent_creation_time, _) =
                process_identity_from_handle(&current, std::process::id())?;
            let executable = std::env::current_exe().ok()?.canonicalize().ok()?;
            let mut command = Command::new(executable);
            command
                .arg(BORROWED_GUARDIAN_MARKER)
                .arg(native_app_guardian_id(snapshot.id))
                .arg(snapshot.window.to_string())
                .arg(snapshot.process_id.to_string())
                .arg(snapshot.creation_time.to_string())
                .arg(snapshot.session_id.to_string())
                .arg(snapshot.owner.to_string())
                .arg(snapshot.attached_owner.to_string())
                .arg(snapshot.style.to_string())
                .arg(snapshot.ex_style.to_string())
                .arg(snapshot.placement.flags.to_string())
                .arg(snapshot.placement.show_cmd.to_string())
                .arg(snapshot.placement.min_position[0].to_string())
                .arg(snapshot.placement.min_position[1].to_string())
                .arg(snapshot.placement.max_position[0].to_string())
                .arg(snapshot.placement.max_position[1].to_string())
                .arg(snapshot.placement.normal_position[0].to_string())
                .arg(snapshot.placement.normal_position[1].to_string())
                .arg(snapshot.placement.normal_position[2].to_string())
                .arg(snapshot.placement.normal_position[3].to_string())
                .arg(encode_path_hex(&snapshot.expected_path))
                .arg(std::process::id().to_string())
                .arg(parent_creation_time.to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let mut child = command.spawn().ok()?;
            let stdout = child.stdout.take()?;
            let (ready_send, ready_receive) = mpsc::sync_channel(1);
            thread::spawn(move || {
                let mut stdout = stdout;
                let mut ready = [0u8; 6];
                let _ =
                    ready_send.send(stdout.read_exact(&mut ready).is_ok() && ready == *b"ready\n");
            });
            let read_ready = ready_receive
                .recv_timeout(Duration::from_secs(2))
                .unwrap_or(false);
            if !read_ready {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Some(Self {
                child: Some(child),
                snapshot: snapshot.clone(),
            })
        }

        unsafe fn restore_and_cancel(mut self) -> bool {
            if !restore_guardian_snapshot(&self.snapshot) {
                return false;
            }
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            true
        }

        fn cancel_after_verified_restore(mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn native_app_guardian_id(id: NativeAppId) -> &'static str {
        match id {
            NativeAppId::Discord => "discord",
            NativeAppId::Telegram => "telegram",
            NativeAppId::Signal => "signal",
            NativeAppId::Whatsapp => "whatsapp",
            NativeAppId::Outlook => "outlook",
        }
    }

    fn parse_native_app_guardian_id(value: &str) -> Option<NativeAppId> {
        match value {
            "discord" => Some(NativeAppId::Discord),
            "telegram" => Some(NativeAppId::Telegram),
            "signal" => Some(NativeAppId::Signal),
            "whatsapp" => Some(NativeAppId::Whatsapp),
            "outlook" => Some(NativeAppId::Outlook),
            _ => None,
        }
    }

    fn encode_path_hex(path: &Path) -> String {
        use std::os::windows::ffi::OsStrExt;
        let mut output = String::with_capacity(path.as_os_str().encode_wide().count() * 4);
        for unit in path.as_os_str().encode_wide() {
            use std::fmt::Write as _;
            let _ = write!(output, "{unit:04x}");
        }
        output
    }

    fn decode_path_hex(value: &str) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        if value.is_empty() || value.len() > 131_072 || !value.len().is_multiple_of(4) {
            return None;
        }
        let units = value
            .as_bytes()
            .chunks_exact(4)
            .map(|chunk| {
                std::str::from_utf8(chunk)
                    .ok()
                    .and_then(|hex| u16::from_str_radix(hex, 16).ok())
            })
            .collect::<Option<Vec<_>>>()?;
        (!units.contains(&0)).then(|| PathBuf::from(OsString::from_wide(&units)))
    }

    fn parse_guardian_snapshot(
        arguments: &[String],
    ) -> Option<(BorrowedRecoverySnapshot, u32, u64)> {
        if arguments.len() != 23 || arguments.first()?.as_str() != BORROWED_GUARDIAN_MARKER {
            return None;
        }
        let parse_isize = |index: usize| arguments.get(index)?.parse::<isize>().ok();
        let parse_u32 = |index: usize| arguments.get(index)?.parse::<u32>().ok();
        let parse_u64 = |index: usize| arguments.get(index)?.parse::<u64>().ok();
        let parse_i32 = |index: usize| arguments.get(index)?.parse::<i32>().ok();
        let snapshot = BorrowedRecoverySnapshot {
            id: parse_native_app_guardian_id(arguments.get(1)?)?,
            window: parse_isize(2)?,
            process_id: parse_u32(3)?,
            creation_time: parse_u64(4)?,
            session_id: parse_u32(5)?,
            owner: parse_isize(6)?,
            attached_owner: parse_isize(7)?,
            style: parse_isize(8)?,
            ex_style: parse_isize(9)?,
            placement: BorrowedWindowPlacement {
                flags: parse_u32(10)?,
                show_cmd: parse_u32(11)?,
                min_position: [parse_i32(12)?, parse_i32(13)?],
                max_position: [parse_i32(14)?, parse_i32(15)?],
                normal_position: [
                    parse_i32(16)?,
                    parse_i32(17)?,
                    parse_i32(18)?,
                    parse_i32(19)?,
                ],
            },
            expected_path: decode_path_hex(arguments.get(20)?)?.canonicalize().ok()?,
        };
        Some((snapshot, parse_u32(21)?, parse_u64(22)?))
    }

    fn guardian_target_process(snapshot: &BorrowedRecoverySnapshot) -> Option<ProcessHandle> {
        let window = snapshot.window as HWND;
        if window.is_null() || unsafe { window_process_id(window) } != Some(snapshot.process_id) {
            return None;
        }
        let process = ProcessHandle::open(snapshot.process_id)?;
        let valid = process_identity_from_handle(&process, snapshot.process_id).is_some_and(
            |(path, creation_time, session_id)| {
                borrowed_guardian_identity_matches(
                    snapshot.process_id,
                    snapshot.process_id,
                    snapshot.creation_time,
                    creation_time,
                    snapshot.session_id,
                    session_id,
                    &snapshot.expected_path,
                    &path,
                ) && trust_existing_executable(snapshot.id, &path).is_some()
            },
        );
        valid.then_some(process)
    }

    fn guardian_target_is_valid(snapshot: &BorrowedRecoverySnapshot) -> bool {
        guardian_target_process(snapshot).is_some()
    }

    unsafe fn restore_guardian_snapshot(snapshot: &BorrowedRecoverySnapshot) -> bool {
        let window = snapshot.window as HWND;
        // Retaining this exact process handle through restoration prevents its
        // PID from being recycled between identity verification and mutation.
        let Some(_process) = guardian_target_process(snapshot) else {
            return false;
        };
        if window_process_id(window) != Some(snapshot.process_id)
            || !borrowed_owner_is_restorable(
                snapshot.owner,
                snapshot.attached_owner,
                GetWindowLongPtrW(window, GWLP_HWNDPARENT),
            )
            || GetWindowLongPtrW(window, GWL_STYLE) != snapshot.style
        {
            return false;
        }
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, snapshot.owner);
        SetWindowLongPtrW(window, GWL_EXSTYLE, snapshot.ex_style);
        let _ = SetWindowPos(
            window,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
        restore_borrowed_window(window, snapshot.placement);
        window_process_id(window) == Some(snapshot.process_id)
            && borrowed_restore_contract_matches(
                snapshot.owner,
                GetWindowLongPtrW(window, GWLP_HWNDPARENT),
                snapshot.style,
                GetWindowLongPtrW(window, GWL_STYLE),
                snapshot.ex_style,
                GetWindowLongPtrW(window, GWL_EXSTYLE),
                capture_borrowed_placement(window) == Some(snapshot.placement),
            )
    }

    unsafe fn borrowed_tether_identity_is_valid(snapshot: &BorrowedTetherSnapshot) -> bool {
        let window = snapshot.window as HWND;
        let parent = snapshot.parent as HWND;
        if window.is_null()
            || parent.is_null()
            || window_process_id(window) != Some(snapshot.process_id)
            || window_process_id(parent) != Some(std::process::id())
            || GetAncestor(parent, GA_ROOT) != parent
            || GetWindowLongPtrW(window, GWLP_HWNDPARENT) != snapshot.parent
        {
            return false;
        }
        ProcessHandle::open(snapshot.process_id)
            .and_then(|process| process_identity_from_handle(&process, snapshot.process_id))
            .is_some_and(|(path, creation_time, session_id)| {
                borrowed_identity_fields_match(
                    snapshot.process_id,
                    snapshot.process_id,
                    snapshot.creation_time,
                    creation_time,
                    snapshot.session_id,
                    session_id,
                    &snapshot.expected_path,
                    &path,
                )
            })
    }

    unsafe fn reconcile_borrowed_tether(
        snapshot: &BorrowedTetherSnapshot,
    ) -> BorrowedTetherObservation {
        if !borrowed_tether_identity_is_valid(snapshot) {
            return BorrowedTetherObservation::IdentityChanged;
        }
        let window = snapshot.window as HWND;
        let parent = snapshot.parent as HWND;
        if IsIconic(parent) != 0 {
            if IsIconic(window) == 0 {
                ShowWindow(window, SW_MINIMIZE);
            }
            return BorrowedTetherObservation::ParentMinimized;
        }
        if IsWindowVisible(parent) == 0 {
            return BorrowedTetherObservation::TransientDesktopUnavailable;
        }
        if IsIconic(window) != 0 {
            ShowWindow(window, SW_RESTORE);
        }
        let Some(expected) = parent_target_rect(parent) else {
            return BorrowedTetherObservation::TransientDesktopUnavailable;
        };
        let expected = rect_array(expected);
        let mut actual: RECT = std::mem::zeroed();
        let actual_matches =
            GetWindowRect(window, &mut actual) != 0 && rect_array(actual) == expected;
        let foreground = GetForegroundWindow();
        let foreground_root = if foreground.is_null() {
            std::ptr::null_mut()
        } else {
            GetAncestor(foreground, GA_ROOT)
        };
        // When either half of the composite is active, keep the borrowed body
        // immediately visible above the OSL background. Its top edge begins
        // below the trusted controls, so this cannot cover the top shell.
        let composite_is_active = foreground_root == parent || foreground_root == window;
        if borrowed_tether_requires_repair(
            actual_matches,
            IsWindowVisible(window) != 0,
            composite_is_active,
        ) {
            if SetWindowPos(
                window,
                HWND_TOP,
                expected[0],
                expected[1],
                expected[2] - expected[0],
                expected[3] - expected[1],
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            ) == 0
            {
                return BorrowedTetherObservation::TransientDesktopUnavailable;
            }
        }
        let mut verified: RECT = std::mem::zeroed();
        if IsWindowVisible(window) == 0
            || IsIconic(window) != 0
            || GetWindowRect(window, &mut verified) == 0
            || rect_array(verified) != expected
        {
            BorrowedTetherObservation::TransientDesktopUnavailable
        } else {
            BorrowedTetherObservation::Aligned
        }
    }

    unsafe fn borrowed_window_tether_worker(
        snapshot: BorrowedTetherSnapshot,
        commands: mpsc::Receiver<BorrowedWindowTetherCommand>,
        ready: mpsc::SyncSender<bool>,
        healthy: Arc<AtomicBool>,
    ) {
        let initial = reconcile_borrowed_tether(&snapshot);
        let ready_value = !matches!(initial, BorrowedTetherObservation::IdentityChanged);
        let _ = ready.send(ready_value);
        if !ready_value {
            healthy.store(false, Ordering::Release);
            return;
        }
        let mut active_ticks = 8usize;
        loop {
            let interval = if active_ticks > 0 {
                BORROWED_TETHER_ACTIVE_INTERVAL
            } else {
                BORROWED_TETHER_IDLE_INTERVAL
            };
            let command = commands.recv_timeout(interval);
            match command {
                Ok(BorrowedWindowTetherCommand::Stop) => break,
                Ok(BorrowedWindowTetherCommand::Reconcile(result)) => {
                    let observation = reconcile_borrowed_tether(&snapshot);
                    let valid = !matches!(observation, BorrowedTetherObservation::IdentityChanged);
                    let _ = result.send(
                        valid
                            && observation
                                != BorrowedTetherObservation::TransientDesktopUnavailable,
                    );
                    if !valid {
                        healthy.store(false, Ordering::Release);
                        break;
                    }
                    active_ticks = 8;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let observation = reconcile_borrowed_tether(&snapshot);
                    if borrowed_tether_decision(observation, 0, usize::MAX)
                        == BorrowedTetherDecision::FailClosed
                    {
                        healthy.store(false, Ordering::Release);
                        break;
                    }
                    if observation == BorrowedTetherObservation::Aligned {
                        active_ticks = active_ticks.saturating_sub(1);
                    } else {
                        active_ticks = 8;
                    }
                }
            }
        }
    }

    pub(super) fn run_borrowed_guardian_if_requested(arguments: &[String]) -> bool {
        if arguments.get(1).map(String::as_str) != Some(BORROWED_GUARDIAN_MARKER) {
            return false;
        }
        let Some((snapshot, parent_pid, parent_creation_time)) =
            parse_guardian_snapshot(&arguments[1..])
        else {
            return true;
        };
        let Some(parent) = ProcessHandle::open_waitable(parent_pid) else {
            return true;
        };
        let parent_valid = process_identity_from_handle(&parent, parent_pid)
            .is_some_and(|(_, creation_time, _)| creation_time == parent_creation_time);
        if !parent_valid || !guardian_target_is_valid(&snapshot) {
            return true;
        }
        let window = snapshot.window as HWND;
        if unsafe {
            GetWindowLongPtrW(window, GWL_STYLE) != snapshot.style
                || GetWindowLongPtrW(window, GWL_EXSTYLE) != snapshot.ex_style
                || !borrowed_owner_contract_unchanged(
                    snapshot.owner,
                    GetWindowLongPtrW(window, GWLP_HWNDPARENT),
                )
        } {
            return true;
        }
        let _ = std::io::stdout().write_all(b"ready\n");
        let _ = std::io::stdout().flush();
        if unsafe { WaitForSingleObject(parent.0, INFINITE) } == WAIT_OBJECT_0 {
            let _ = unsafe { restore_guardian_snapshot(&snapshot) };
        }
        true
    }

    unsafe fn borrowed_control_shield_worker(
        target: HWND,
        expected_process_id: u32,
        commands: mpsc::Receiver<BorrowedControlShieldCommand>,
        ready: mpsc::SyncSender<Option<isize>>,
    ) {
        let static_class = [
            b'S' as u16,
            b'T' as u16,
            b'A' as u16,
            b'T' as u16,
            b'I' as u16,
            b'C' as u16,
            0,
        ];
        let shield = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            static_class.as_ptr(),
            std::ptr::null(),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            1,
            1,
            target,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if shield.is_null() {
            let _ = ready.send(None);
            return;
        }
        let _ = ready.send(Some(shield as isize));
        let mut running = true;
        while running {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            match commands.recv_timeout(Duration::from_millis(16)) {
                Ok(BorrowedControlShieldCommand::Position(result)) => {
                    let positioned = window_process_id(target) == Some(expected_process_id)
                        && GetWindowLongPtrW(shield, GWLP_HWNDPARENT) == target as isize
                        && position_borrowed_control_shield(target, shield);
                    let _ = result.send(positioned);
                }
                Ok(BorrowedControlShieldCommand::Stop) => running = false,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if window_process_id(target) == Some(expected_process_id)
                        && GetWindowLongPtrW(shield, GWLP_HWNDPARENT) == target as isize
                    {
                        let _ = position_borrowed_control_shield(target, shield);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => running = false,
            }
        }
        DestroyWindow(shield);
    }

    unsafe fn position_borrowed_control_shield(target: HWND, shield: HWND) -> bool {
        let mut window: RECT = std::mem::zeroed();
        if GetWindowRect(target, &mut window) == 0 {
            return false;
        }
        let Some([left, top, right, bottom]) = borrowed_control_shield_rect(
            [window.left, window.top],
            [window.right - window.left, window.bottom - window.top],
        ) else {
            return false;
        };
        SetWindowPos(
            shield,
            HWND_TOP,
            left,
            top,
            right - left,
            bottom - top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) != 0
            && paint_borrowed_control_shield(target, shield)
    }

    unsafe fn paint_borrowed_control_shield(target: HWND, shield: HWND) -> bool {
        let mut client: RECT = std::mem::zeroed();
        if GetClientRect(target, &mut client) == 0 {
            return false;
        }
        // Six fixed probes immediately left of Discord's top-right controls.
        // They remain inside the custom titlebar and never descend into the
        // channel list, message history, composer, or any other content area.
        let target_dc = GetDC(target);
        if target_dc.is_null() {
            return false;
        }
        let mut samples = Vec::with_capacity(6);
        for x_offset in [8, 20] {
            let x = client.right - BORROWED_CONTROL_SHIELD_WIDTH - x_offset;
            for y in [10, 22, 34] {
                if x >= client.left && y < client.bottom.min(BORROWED_CONTROL_SHIELD_HEIGHT) {
                    let color = GetPixel(target_dc, x, y);
                    if color != u32::MAX {
                        samples.push([
                            (color & 0xff) as u8,
                            ((color >> 8) & 0xff) as u8,
                            ((color >> 16) & 0xff) as u8,
                        ]);
                    }
                }
            }
        }
        ReleaseDC(target, target_dc);
        let [red, green, blue] = borrowed_control_shield_color(&samples);
        let brush =
            CreateSolidBrush(u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16));
        if brush.is_null() {
            return false;
        }
        let shield_dc = GetDC(shield);
        let mut shield_client: RECT = std::mem::zeroed();
        let painted = !shield_dc.is_null()
            && GetClientRect(shield, &mut shield_client) != 0
            && FillRect(shield_dc, &shield_client, brush) != 0;
        if !shield_dc.is_null() {
            ReleaseDC(shield, shield_dc);
        }
        DeleteObject(brush);
        painted
    }

    #[derive(Debug)]
    pub(super) enum TrustedWindowExecutable {
        Authenticode(TrustedExecutable),
        AppxPackage(PathBuf),
    }

    impl TrustedWindowExecutable {
        fn path(&self) -> &Path {
            match self {
                Self::Authenticode(executable) => executable.path(),
                Self::AppxPackage(path) => path,
            }
        }
    }

    unsafe impl Send for JobHandle {}
    unsafe impl Send for ProcessHandle {}

    impl ProcessHandle {
        fn open(process_id: u32) -> Option<Self> {
            let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
            (!raw.is_null()).then_some(Self(raw))
        }

        fn open_waitable(process_id: u32) -> Option<Self> {
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE,
                    0,
                    process_id,
                )
            };
            (!raw.is_null()).then_some(Self(raw))
        }
    }

    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let _ = unsafe { CloseHandle(self.0) };
                self.0 = std::ptr::null_mut();
            }
        }
    }

    impl JobHandle {
        fn new() -> std::io::Result<Self> {
            let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if raw.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let mut limits = JobObjectExtendedLimitInformation::default();
            limits.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    raw,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    (&limits as *const JobObjectExtendedLimitInformation).cast(),
                    std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
                )
            };
            if configured == 0 {
                unsafe { CloseHandle(raw) };
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self(raw))
        }

        fn assign_suspended(&self, child: &Child) -> std::io::Result<()> {
            let process = child.as_raw_handle().cast();
            if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            if !resume_suspended_primary_thread(child.id()) {
                let _ = unsafe { TerminateJobObject(self.0, 1) };
                return Err(std::io::Error::other(
                    "contained process primary thread could not be resumed",
                ));
            }
            Ok(())
        }

        fn terminate(&self) {
            let _ = unsafe { TerminateJobObject(self.0, 1) };
        }

        fn contains_process(&self, process: RawHandle) -> bool {
            let mut contained = 0;
            unsafe { IsProcessInJob(process, self.0, &mut contained) != 0 && contained != 0 }
        }
    }

    fn resume_suspended_primary_thread(process_id: u32) -> bool {
        let invalid_handle = -1isize as RawHandle;
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot.is_null() || snapshot == invalid_handle {
            return false;
        }
        let mut entry = ThreadEntry32 {
            size: std::mem::size_of::<ThreadEntry32>() as u32,
            usage: 0,
            thread_id: 0,
            owner_process_id: 0,
            base_priority: 0,
            priority_delta: 0,
            flags: 0,
        };
        let mut found = unsafe { Thread32First(snapshot, &mut entry) } != 0;
        let mut resumed = false;
        while found {
            if entry.owner_process_id == process_id {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.thread_id) };
                if !thread.is_null() {
                    resumed = unsafe { ResumeThread(thread) } != u32::MAX;
                    let _ = unsafe { CloseHandle(thread) };
                }
                break;
            }
            found = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
        }
        let _ = unsafe { CloseHandle(snapshot) };
        resumed
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                let _ = unsafe { CloseHandle(self.0) };
                self.0 = std::ptr::null_mut();
            }
        }
    }

    #[derive(Debug)]
    struct LaunchSpec {
        executable: TrustedExecutable,
        publisher: ExecutablePublisher,
        arguments: Vec<OsString>,
        profile: std::path::PathBuf,
    }

    struct WindowSearch {
        id: NativeAppId,
        job: *const JobHandle,
        publisher: ExecutablePublisher,
        best: HWND,
        best_area: i64,
        best_pid: u32,
        best_executable: Option<TrustedExecutable>,
    }

    struct ExistingWindowCandidate {
        window: HWND,
        area: i64,
        process_id: u32,
        creation_time: u64,
        process: ProcessHandle,
        executable: TrustedWindowExecutable,
    }

    struct ExistingWindowSearch {
        id: NativeAppId,
        expected_path: PathBuf,
        expected_session: u32,
        overflowed: bool,
        candidates: Vec<ExistingWindowCandidate>,
    }

    const MAX_EXISTING_WINDOW_CANDIDATES: usize = 32;
    const WHATSAPP_AUMID: &str = "shell:AppsFolder\\5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App";

    fn trust_existing_executable(id: NativeAppId, path: &Path) -> Option<TrustedWindowExecutable> {
        if id == NativeAppId::Whatsapp {
            let expected = crate::native_apps::whatsapp_store_executable_path()?
                .canonicalize()
                .ok()?;
            let actual = path.canonicalize().ok()?;
            return (actual == expected).then_some(TrustedWindowExecutable::AppxPackage(expected));
        }
        let publisher = existing_session_publisher(id)?;
        verify_executable(path, publisher)
            .ok()
            .map(TrustedWindowExecutable::Authenticode)
    }

    unsafe fn launch_whatsapp_aumid() -> Result<(), NativeWindowHostReason> {
        let verb = std::ffi::OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let target = std::ffi::OsStr::new(WHATSAPP_AUMID)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
        ((result as isize) > 32)
            .then_some(())
            .ok_or(NativeWindowHostReason::LaunchFailed)
    }

    unsafe fn launch_outlook_aumid() -> Result<(), NativeWindowHostReason> {
        let verb = std::ffi::OsStr::new("open")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let target = std::ffi::OsStr::new(crate::native_apps::OUTLOOK_PACKAGE_AUMID)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        );
        ((result as isize) > 32)
            .then_some(())
            .ok_or(NativeWindowHostReason::LaunchFailed)
    }

    fn next_host_generation(state: &NativeWindowHostState) -> Option<u64> {
        state
            .next_generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .ok()
            .and_then(|previous| previous.checked_add(1))
    }

    pub(super) fn host(
        state: &NativeWindowHostState,
        id: NativeAppId,
        root: &Path,
        owner_osl_user_id: &str,
        parent: isize,
        mode: DiscordSessionMode,
    ) -> NativeWindowHostResult {
        if parent == 0 {
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::OwnerWindowUnavailable,
            );
        }
        if mode == DiscordSessionMode::ExistingSession && !existing_session_supported(id) {
            return NativeWindowHostResult::unsupported(
                id,
                NativeWindowHostReason::SecondaryInstanceUnverified,
            );
        }
        if unsafe { !prepare_trusted_capture_parent(parent as HWND) } {
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::OwnerWindowUnavailable,
            );
        }
        if mode == DiscordSessionMode::Dedicated && !secondary_instance_verified(id) {
            return NativeWindowHostResult::unsupported(
                id,
                NativeWindowHostReason::SecondaryInstanceUnverified,
            );
        }

        let owner_namespace = match crate::service_host::owner_profile_namespace(owner_osl_user_id)
        {
            Ok(namespace) => namespace,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    id,
                    NativeWindowHostReason::ProfileUnavailable,
                )
            }
        };
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    id,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        if let Some(hosted) = guard.as_mut() {
            if warm_host_action(
                hosted.id,
                hosted.mode,
                &hosted.owner_namespace,
                hosted_window_is_valid(hosted),
                id,
                mode,
                &owner_namespace,
            ) == WarmHostAction::Reuse
            {
                let presented = unsafe {
                    if hosted.mode == DiscordSessionMode::Dedicated {
                        present_verified_child(hosted, parent as HWND)
                    } else {
                        realign_borrowed_window(hosted, parent as HWND)
                    }
                };
                if presented {
                    let Some(generation) = next_host_generation(state) else {
                        return NativeWindowHostResult::failed(
                            id,
                            NativeWindowHostReason::HostWindowUnavailable,
                        );
                    };
                    hosted.generation = generation;
                    if let Some(tether) = hosted.borrowed_tether.as_ref() {
                        tether.rebind_generation(generation);
                    }
                    hosted.attached = true;
                    return NativeWindowHostResult::success_with_capture(
                        id,
                        NativeWindowHostStatus::Hosted,
                        mode,
                        hosted.capture_certified,
                    );
                }
                // The process identity is still trusted, but its saved child
                // presentation is stale. Tear down only this owned/claimed
                // host and continue through the same bounded cold path.
            }
            if let Some(stale) = guard.take() {
                unsafe { shutdown_hosted(stale) };
            }
        }
        let Some(generation) = next_host_generation(state) else {
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::HostWindowUnavailable,
            );
        };
        let hosted = match cold_host_action(mode) {
            ColdHostAction::LaunchDedicated => unsafe {
                launch_dedicated_host(
                    generation,
                    id,
                    root,
                    owner_osl_user_id,
                    owner_namespace,
                    parent as HWND,
                )
            },
            ColdHostAction::ClaimExisting => unsafe {
                claim_or_relaunch_existing_host(generation, id, owner_namespace, parent as HWND)
            },
        };
        let hosted = match hosted {
            Ok(hosted) => hosted,
            Err(reason) => {
                return NativeWindowHostResult::failed(id, reason);
            }
        };
        let capture_certified = hosted.capture_certified;
        *guard = Some(hosted);
        NativeWindowHostResult::success_with_capture(
            id,
            NativeWindowHostStatus::Hosted,
            mode,
            capture_certified,
        )
    }

    unsafe fn launch_dedicated_host(
        generation: u64,
        id: NativeAppId,
        root: &Path,
        owner_osl_user_id: &str,
        owner_namespace: String,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        let spec = build_launch_spec(id, root, owner_osl_user_id)?;
        let attempt_limit = dedicated_launch_attempt_limit(id);
        let discovery_timeout = if id == NativeAppId::Telegram {
            EXISTING_SESSION_DISCOVERY_TIMEOUT
        } else {
            WINDOW_DISCOVERY_TIMEOUT
        };
        let mut attempt = 0usize;
        let (mut child, job, window, window_pid, trusted_window_executable) = loop {
            attempt += 1;
            let (mut child, job) = launch_isolated(&spec).map_err(|_| {
                if attempt > 1 {
                    NativeWindowHostReason::ProfileInitializationFailed
                } else {
                    NativeWindowHostReason::LaunchFailed
                }
            })?;
            if let Some((window, window_pid, executable)) =
                wait_for_process_window(id, &mut child, &job, spec.publisher, discovery_timeout)
            {
                break (child, job, window, window_pid, executable);
            }
            // This job contains only the process OSL spawned with the fixed
            // isolated profile and its descendants. End that attempt before
            // retrying; never enumerate, close, or adopt ordinary Telegram.
            job.terminate();
            let _ = child.wait();
            if attempt >= attempt_limit {
                return Err(if id == NativeAppId::Telegram {
                    NativeWindowHostReason::ProfileInitializationFailed
                } else {
                    NativeWindowHostReason::WindowNotFound
                });
            }
        };
        if window_process_id(window) != Some(window_pid)
            || !trusted_job_process_path(&job, window_pid)
                .is_some_and(|path| path == trusted_window_executable.path())
        {
            job.terminate();
            let _ = child.wait();
            return Err(NativeWindowHostReason::WindowIdentityChanged);
        }
        let hosted = adopt_borderless_owned_window(
            generation,
            id,
            DiscordSessionMode::Dedicated,
            owner_namespace,
            HostedProcess::Dedicated { child, job },
            window_pid,
            TrustedWindowExecutable::Authenticode(trusted_window_executable),
            window,
            parent,
        )?;
        if id == NativeAppId::Discord && !settle_discord_dedicated_host(&hosted, parent) {
            shutdown_hosted(hosted);
            return Err(NativeWindowHostReason::WindowIdentityChanged);
        }
        Ok(hosted)
    }

    unsafe fn settle_discord_dedicated_host(hosted: &HostedWindow, parent: HWND) -> bool {
        let deadline = Instant::now() + DISCORD_POST_ADOPTION_SETTLE;
        loop {
            if !hosted_window_is_valid(hosted)
                || !child_presentation_is_verified(
                    hosted.id,
                    hosted.window as HWND,
                    parent,
                    hosted.window_process_id,
                    hosted.original_dpi_context,
                )
            {
                return false;
            }
            if Instant::now() >= deadline {
                return true;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    unsafe fn claim_existing_host(
        generation: u64,
        id: NativeAppId,
        owner_namespace: String,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        let executable_paths =
            match id {
                NativeAppId::Discord => {
                    let local = known_folder(&FOLDERID_LocalAppData)
                        .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
                    let path = preferred_existing_discord_channel_executable(|channel| {
                        newest_discord_channel_executable(
                            &local.join(channel.install_directory),
                            channel.executable_name,
                        )
                    })
                    .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
                    vec![path]
                }
                NativeAppId::Telegram => vec![telegram_executable()
                    .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?],
                NativeAppId::Signal => vec![signal_executable()
                    .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?],
                NativeAppId::Whatsapp => vec![crate::native_apps::whatsapp_store_executable_path()
                    .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?],
                NativeAppId::Outlook => {
                    let paths = crate::native_apps::outlook_native_executable_paths();
                    if paths.is_empty() {
                        return Err(NativeWindowHostReason::ExistingSessionUnavailable);
                    }
                    paths
                }
            };
        let mut expected_session = 0u32;
        if ProcessIdToSessionId(std::process::id(), &mut expected_session) == 0 {
            return Err(NativeWindowHostReason::ExistingSessionUnavailable);
        }
        let mut candidates = Vec::with_capacity(8);
        for executable_path in executable_paths {
            let expected = trust_existing_executable(id, &executable_path)
                .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
            let mut search = ExistingWindowSearch {
                id,
                expected_path: expected.path().to_owned(),
                expected_session,
                overflowed: false,
                candidates: Vec::with_capacity(8),
            };
            EnumWindows(
                Some(enum_existing_window),
                (&mut search as *mut ExistingWindowSearch) as LPARAM,
            );
            drop(expected);
            if search.overflowed
                || candidates.len().saturating_add(search.candidates.len())
                    > MAX_EXISTING_WINDOW_CANDIDATES
            {
                return Err(NativeWindowHostReason::ExistingSessionAmbiguous);
            }
            candidates.extend(search.candidates);
        }
        let candidate = take_existing_candidate(id, candidates)?;
        if window_process_id(candidate.window) != Some(candidate.process_id)
            || !borrowed_process_is_valid(
                candidate.process_id,
                candidate.creation_time,
                &candidate.process,
                candidate.executable.path(),
                id,
            )
        {
            return Err(NativeWindowHostReason::WindowIdentityChanged);
        }
        adopt_existing_companion(
            generation,
            id,
            owner_namespace,
            candidate.process_id,
            candidate.creation_time,
            expected_session,
            candidate.process,
            candidate.executable,
            candidate.window,
            parent,
        )
    }

    unsafe fn claim_or_relaunch_existing_host(
        generation: u64,
        id: NativeAppId,
        owner_namespace: String,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        match claim_existing_host(generation, id, owner_namespace.clone(), parent) {
            Ok(hosted) => return Ok(hosted),
            Err(reason) if should_relaunch_existing_session(id, reason) => {}
            Err(reason) => return Err(reason),
        }

        let (executable_path, publisher) = match id {
            NativeAppId::Discord => {
                let local = known_folder(&FOLDERID_LocalAppData)
                    .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
                let path = preferred_existing_discord_channel_executable(|channel| {
                    newest_discord_channel_executable(
                        &local.join(channel.install_directory),
                        channel.executable_name,
                    )
                });
                (path, ExecutablePublisher::Discord)
            }
            NativeAppId::Telegram => (telegram_executable(), ExecutablePublisher::Telegram),
            NativeAppId::Signal => (signal_executable(), ExecutablePublisher::Signal),
            NativeAppId::Whatsapp => {
                launch_whatsapp_aumid()?;
                let deadline = Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT;
                loop {
                    thread::sleep(Duration::from_millis(100));
                    match claim_existing_host(generation, id, owner_namespace.clone(), parent) {
                        Ok(hosted) => return Ok(hosted),
                        Err(NativeWindowHostReason::ExistingSessionAmbiguous) => {
                            return Err(NativeWindowHostReason::ExistingSessionAmbiguous)
                        }
                        Err(reason) if Instant::now() >= deadline => return Err(reason),
                        Err(_) => {}
                    }
                }
            }
            NativeAppId::Outlook => {
                let paths = crate::native_apps::outlook_native_executable_paths();
                let Some(path) = paths.first() else {
                    return Err(NativeWindowHostReason::ExistingSessionUnavailable);
                };
                if path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("olk.exe"))
                {
                    launch_outlook_aumid()?;
                    let deadline = Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT;
                    loop {
                        thread::sleep(Duration::from_millis(100));
                        match claim_existing_host(generation, id, owner_namespace.clone(), parent) {
                            Ok(hosted) => return Ok(hosted),
                            Err(NativeWindowHostReason::ExistingSessionAmbiguous) => {
                                return Err(NativeWindowHostReason::ExistingSessionAmbiguous)
                            }
                            Err(reason) if Instant::now() >= deadline => return Err(reason),
                            Err(_) => {}
                        }
                    }
                }
                (Some(path.to_owned()), ExecutablePublisher::Microsoft)
            }
            _ => return Err(NativeWindowHostReason::ExistingSessionUnavailable),
        };
        let executable_path =
            executable_path.ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
        let executable = verify_executable(&executable_path, publisher)
            .map_err(|_| NativeWindowHostReason::ExistingSessionUnavailable)?;
        let mut child = Command::new(executable.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| NativeWindowHostReason::LaunchFailed)?;
        drop(executable);

        let deadline = Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT;
        loop {
            thread::sleep(Duration::from_millis(100));
            match claim_existing_host(generation, id, owner_namespace.clone(), parent) {
                Ok(hosted) => return Ok(hosted),
                Err(NativeWindowHostReason::ExistingSessionAmbiguous) => {
                    return Err(NativeWindowHostReason::ExistingSessionAmbiguous)
                }
                Err(reason) if Instant::now() >= deadline => {
                    let _ = child.try_wait();
                    return Err(reason);
                }
                Err(_) => {}
            }
        }
    }

    unsafe extern "system" fn enum_existing_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut ExistingWindowSearch);
        if search.candidates.len() >= MAX_EXISTING_WINDOW_CANDIDATES {
            search.overflowed = true;
            return 0;
        }
        let visible = IsWindowVisible(window) != 0;
        let mut class_name = [0u16; 64];
        let class_length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        let mut title = [0u16; 128];
        let title_length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
        if class_length <= 0
            || title_length < 0
            || !existing_window_identity_allowed(
                search.id,
                visible,
                &String::from_utf16_lossy(&class_name[..class_length as usize]),
                &String::from_utf16_lossy(&title[..title_length as usize]),
            )
        {
            return 1;
        }
        let Some(process_id) = window_process_id(window) else {
            return 1;
        };
        let Some((process, path, creation_time, session_id)) = process_identity(process_id) else {
            return 1;
        };
        if session_id != search.expected_session || path != search.expected_path {
            return 1;
        }
        let Some(executable) = trust_existing_executable(search.id, &path) else {
            return 1;
        };
        let Some(bounds) = borrowed_previous_rect(window, IsIconic(window) != 0) else {
            return 1;
        };
        search.candidates.push(ExistingWindowCandidate {
            window,
            area: i64::from(bounds.right - bounds.left) * i64::from(bounds.bottom - bounds.top),
            process_id,
            creation_time,
            process,
            executable,
        });
        1
    }

    unsafe fn take_existing_candidate(
        id: NativeAppId,
        mut candidates: Vec<ExistingWindowCandidate>,
    ) -> Result<ExistingWindowCandidate, NativeWindowHostReason> {
        let primary =
            existing_primary_candidate_index(id, candidates.len(), |target, candidate| {
                telegram_owned_frame_decoration(
                    candidates[target].window,
                    candidates[candidate].window,
                )
            })
            .or_else(|reason| {
                if !matches!(id, NativeAppId::Telegram | NativeAppId::Signal) {
                    return Err(reason);
                }
                let foreground = GetForegroundWindow();
                if let Some(index) = candidates
                    .iter()
                    .position(|candidate| candidate.window == foreground)
                {
                    return Ok(index);
                }
                let maximum = candidates
                    .iter()
                    .map(|candidate| candidate.area)
                    .max()
                    .ok_or(reason)?;
                let mut largest = candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| candidate.area == maximum);
                let first = largest.next().map(|(index, _)| index).ok_or(reason)?;
                largest.next().is_none().then_some(first).ok_or(reason)
            })?;
        Ok(candidates.swap_remove(primary))
    }

    fn process_identity(process_id: u32) -> Option<(ProcessHandle, PathBuf, u64, u32)> {
        if process_id == 0 {
            return None;
        }
        let process = ProcessHandle::open(process_id)?;
        let (path, creation_time, session_id) = process_identity_from_handle(&process, process_id)?;
        Some((process, path, creation_time, session_id))
    }

    fn process_identity_from_handle(
        process: &ProcessHandle,
        process_id: u32,
    ) -> Option<(PathBuf, u64, u32)> {
        (|| {
            let mut path = vec![0u16; 32_768];
            let mut path_len = path.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut path_len) }
                == 0
                || path_len == 0
            {
                return None;
            }
            path.truncate(path_len as usize);
            let path = PathBuf::from(OsString::from_wide(&path))
                .canonicalize()
                .ok()?;
            let mut creation: FILETIME = unsafe { std::mem::zeroed() };
            let mut exit: FILETIME = unsafe { std::mem::zeroed() };
            let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
            let mut user: FILETIME = unsafe { std::mem::zeroed() };
            if unsafe {
                GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user)
            } == 0
            {
                return None;
            }
            let creation_time =
                (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime);
            let mut session_id = 0u32;
            if unsafe { ProcessIdToSessionId(process_id, &mut session_id) } == 0 {
                return None;
            }
            Some((path, creation_time, session_id))
        })()
    }

    fn borrowed_process_is_valid(
        process_id: u32,
        creation_time: u64,
        process: &ProcessHandle,
        expected_path: &Path,
        id: NativeAppId,
    ) -> bool {
        process_identity_from_handle(process, process_id).is_some_and(
            |(path, current_creation, session)| {
                let mut osl_session = 0u32;
                let session_known =
                    unsafe { ProcessIdToSessionId(std::process::id(), &mut osl_session) != 0 };
                session_known
                    && borrowed_identity_fields_match(
                        process_id,
                        process_id,
                        creation_time,
                        current_creation,
                        osl_session,
                        session,
                        expected_path,
                        &path,
                    )
                    && trust_existing_executable(id, &path).is_some()
            },
        )
    }

    fn existing_session_publisher(id: NativeAppId) -> Option<ExecutablePublisher> {
        match id {
            NativeAppId::Discord => Some(ExecutablePublisher::Discord),
            NativeAppId::Telegram => Some(ExecutablePublisher::Telegram),
            NativeAppId::Signal => Some(ExecutablePublisher::Signal),
            NativeAppId::Outlook => Some(ExecutablePublisher::Microsoft),
            _ => None,
        }
    }

    pub(super) fn resize(state: &NativeWindowHostState, parent: isize) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_ref() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        if !hosted.attached || parent == 0 || !hosted_window_is_valid(hosted) {
            let id = hosted.id;
            if let Some(stale) = guard.take() {
                unsafe { shutdown_hosted(stale) };
            }
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        let hosted = guard.as_mut().expect("validated host remains present");
        let presented = unsafe {
            if hosted.mode == DiscordSessionMode::Dedicated {
                realign_verified_child(hosted, parent as HWND)
            } else {
                realign_borrowed_window(hosted, parent as HWND)
            }
        };
        if presented {
            NativeWindowHostResult::success_with_capture(
                hosted.id,
                NativeWindowHostStatus::Resized,
                hosted.mode,
                hosted.capture_certified,
            )
        } else {
            let id = hosted.id;
            let _ = hosted;
            if let Some(stale) = guard.take() {
                unsafe { shutdown_hosted(stale) };
            }
            NativeWindowHostResult::failed(id, NativeWindowHostReason::WindowOperationRejected)
        }
    }

    pub(super) fn focus(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_ref() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        if !hosted.attached || !hosted_window_is_valid(hosted) {
            let id = hosted.id;
            if let Some(stale) = guard.take() {
                unsafe { shutdown_hosted(stale) };
            }
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        let hosted = guard.as_ref().expect("validated host remains present");
        let id = hosted.id;
        let mode = hosted.mode;
        let capture_certified = hosted.capture_certified;
        let focused = unsafe {
            let window = hosted.window as HWND;
            if hosted.mode == DiscordSessionMode::ExistingSession {
                let mut tethered = hosted.borrowed_tether.as_ref().map_or_else(
                    || {
                        borrowed_focus_state_valid(
                            IsWindowVisible(window) != 0,
                            IsIconic(window) != 0,
                        )
                    },
                    |tether| tether.reconcile(hosted.generation),
                );
                if !tethered || hosted.borrowed_tether.is_none() {
                    // Foreground arbitration is only a recovery fallback. The
                    // continuous tether normally repairs visibility and order
                    // without stealing focus from the trusted top shell.
                    ShowWindow(window, SW_RESTORE);
                    let _ = BringWindowToTop(window);
                    let _ = SetForegroundWindow(window);
                    tethered = hosted.borrowed_tether.as_ref().map_or_else(
                        || {
                            borrowed_focus_state_valid(
                                IsWindowVisible(window) != 0,
                                IsIconic(window) != 0,
                            )
                        },
                        |tether| tether.reconcile(hosted.generation),
                    );
                }
                tethered
                    && borrowed_focus_state_valid(
                        IsWindowVisible(window) != 0,
                        IsIconic(window) != 0,
                    )
                    && hosted
                        .borrowed_control_shield
                        .as_ref()
                        .is_some_and(BorrowedControlShield::position)
            } else {
                ShowWindow(window, SW_RESTORE);
                let presented = present_verified_child(hosted, hosted.trusted_parent as HWND)
                    && BringWindowToTop(window) != 0;
                if presented {
                    let _ = SetForegroundWindow(hosted.trusted_parent as HWND);
                    let _ = SetFocus(window);
                }
                presented
            }
        };
        if !focused {
            if let Some(stale) = guard.take() {
                unsafe { shutdown_hosted(stale) };
            }
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::WindowOperationRejected,
            );
        }
        NativeWindowHostResult::success_with_capture(
            id,
            NativeWindowHostStatus::Focused,
            mode,
            capture_certified,
        )
    }

    pub(super) fn detach(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.as_mut() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        if !hosted_window_is_valid(hosted) {
            let id = hosted.id;
            if let Some(stale) = guard.take() {
                unsafe { shutdown_hosted(stale) };
            }
            return NativeWindowHostResult::failed(
                id,
                NativeWindowHostReason::WindowIdentityChanged,
            );
        }
        if hosted.mode == DiscordSessionMode::ExistingSession {
            let mut hosted = guard.take().expect("borrowed host is still present");
            let id = hosted.id;
            let mode = hosted.mode;
            hosted.borrowed_tether.take();
            hosted.borrowed_control_shield.take();
            if !unsafe { restore_window(&mut hosted) } {
                return NativeWindowHostResult::failed(
                    id,
                    NativeWindowHostReason::BorrowedStyleRejected,
                );
            }
            return NativeWindowHostResult::success(id, NativeWindowHostStatus::Detached, mode);
        }
        unsafe {
            ShowWindow(
                hosted.window as HWND,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
            )
        };
        hosted.attached = false;
        NativeWindowHostResult::success(hosted.id, NativeWindowHostStatus::Detached, hosted.mode)
    }

    pub(super) fn terminate(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let mut guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                )
            }
        };
        let Some(hosted) = guard.take() else {
            return NativeWindowHostResult::failed(
                NativeAppId::Discord,
                NativeWindowHostReason::NotHosted,
            );
        };
        let id = hosted.id;
        let mode = hosted.mode;
        unsafe { shutdown_hosted(hosted) };
        NativeWindowHostResult::success(id, NativeWindowHostStatus::Detached, mode)
    }

    fn build_launch_spec(
        id: NativeAppId,
        root: &Path,
        owner_osl_user_id: &str,
    ) -> Result<LaunchSpec, NativeWindowHostReason> {
        if fixed_secondary_launch(id) == FixedSecondaryLaunch::DiscordDedicatedChannel {
            return build_discord_launch_spec(root, owner_osl_user_id);
        }
        let profile = prepare_profile(root, owner_osl_user_id, id)?;
        let (executable_path, arguments) = match fixed_secondary_launch(id) {
            FixedSecondaryLaunch::DiscordDedicatedChannel => unreachable!(),
            FixedSecondaryLaunch::TelegramManyWorkdir => {
                let executable =
                    telegram_executable().ok_or(NativeWindowHostReason::AppNotInstalled)?;
                (
                    executable,
                    vec![
                        OsString::from("-many"),
                        OsString::from("-workdir"),
                        profile.as_os_str().to_owned(),
                    ],
                )
            }
            FixedSecondaryLaunch::SignalUserDataDir => {
                let executable =
                    signal_executable().ok_or(NativeWindowHostReason::AppNotInstalled)?;
                let mut profile_arg = OsString::from("--user-data-dir=");
                profile_arg.push(profile.as_os_str());
                (executable, vec![profile_arg])
            }
            FixedSecondaryLaunch::Unsupported => {
                return Err(NativeWindowHostReason::SecondaryInstanceUnverified)
            }
        };
        let publisher = crate::native_apps::native_app_publisher(id)
            .ok_or(NativeWindowHostReason::AppNotInstalled)?;
        let executable = verify_executable(&executable_path, publisher)
            .map_err(|_| NativeWindowHostReason::AppNotInstalled)?;
        Ok(LaunchSpec {
            executable,
            publisher,
            arguments,
            profile,
        })
    }

    fn build_discord_launch_spec(
        root: &Path,
        owner_osl_user_id: &str,
    ) -> Result<LaunchSpec, NativeWindowHostReason> {
        let local = known_folder(&FOLDERID_LocalAppData)
            .ok_or(NativeWindowHostReason::NoChannelAvailable)?;
        let roaming = known_folder(&FOLDERID_RoamingAppData)
            .ok_or(NativeWindowHostReason::NoChannelAvailable)?;
        let mut blocked_by_existing_profile = false;

        for channel in dedicated_discord_channels() {
            let install_root = local.join(channel.install_directory);
            let Some(executable_path) =
                newest_discord_channel_executable(&install_root, channel.executable_name)
            else {
                continue;
            };
            let Ok(executable) = verify_executable(
                &executable_path,
                crate::windows_executable_trust::ExecutablePublisher::Discord,
            ) else {
                continue;
            };
            let profile = match claim_discord_channel(root, &roaming, owner_osl_user_id, channel) {
                Ok(profile) => profile,
                Err(NativeWindowHostReason::ChannelNotOwned) => {
                    blocked_by_existing_profile = true;
                    continue;
                }
                Err(reason) => return Err(reason),
            };
            return Ok(LaunchSpec {
                executable,
                publisher: ExecutablePublisher::Discord,
                // Fixed, argument-free from the renderer's perspective: OSL
                // keeps Chromium's complete native accessibility provider on
                // for this already claimed, signed Discord channel. The bare
                // switch permits later mode changes and is insufficient for
                // the exact composer proof required by OSL.
                arguments: vec![OsString::from(DISCORD_ACCESSIBILITY_ARGUMENT)],
                profile,
            });
        }

        Err(if blocked_by_existing_profile {
            NativeWindowHostReason::ChannelNotOwned
        } else {
            NativeWindowHostReason::NoChannelAvailable
        })
    }

    fn newest_discord_channel_executable(
        install_root: &Path,
        executable_name: &str,
    ) -> Option<PathBuf> {
        let mut candidates = fs::read_dir(install_root)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let version = discord_version_key(name.to_str()?)?;
                let executable = entry.path().join(executable_name);
                executable.is_file().then_some((version, executable))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.0.cmp(&right.0));
        candidates.pop().map(|(_, executable)| executable)
    }

    fn discord_version_key(directory_name: &str) -> Option<Vec<u64>> {
        let version = directory_name.strip_prefix("app-")?;
        let components = version
            .split('.')
            .map(|component| {
                (!component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit()))
                    .then(|| component.parse::<u64>().ok())
                    .flatten()
            })
            .collect::<Option<Vec<_>>>()?;
        (components.len() >= 2).then_some(components)
    }

    fn telegram_executable() -> Option<std::path::PathBuf> {
        [
            known_folder(&FOLDERID_RoamingAppData)
                .map(|root| root.join("Telegram Desktop").join("Telegram.exe")),
            known_folder(&FOLDERID_LocalAppData).map(|root| {
                root.join("Programs")
                    .join("Telegram Desktop")
                    .join("Telegram.exe")
            }),
        ]
        .into_iter()
        .flatten()
        .find(|candidate| candidate.is_file())
    }

    fn signal_executable() -> Option<std::path::PathBuf> {
        known_folder(&FOLDERID_LocalAppData)
            .map(|root| {
                root.join("Programs")
                    .join("signal-desktop")
                    .join("Signal.exe")
            })
            .filter(|candidate| candidate.is_file())
    }

    fn known_folder(id: *const windows_sys::core::GUID) -> Option<std::path::PathBuf> {
        let mut raw = std::ptr::null_mut();
        let result = unsafe {
            SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut raw)
        };
        if result < 0 || raw.is_null() {
            return None;
        }
        let mut length = 0usize;
        unsafe {
            while *raw.add(length) != 0 {
                length += 1;
            }
        }
        let value = unsafe { std::slice::from_raw_parts(raw, length) };
        let path = std::path::PathBuf::from(OsString::from_wide(value));
        unsafe { CoTaskMemFree(raw.cast()) };
        Some(path)
    }

    fn prepare_profile(
        root: &Path,
        owner_osl_user_id: &str,
        id: NativeAppId,
    ) -> Result<std::path::PathBuf, NativeWindowHostReason> {
        if !root.is_absolute() {
            return Err(NativeWindowHostReason::ProfileUnavailable);
        }
        fs::create_dir_all(root).map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
        let canonical_root = root
            .canonicalize()
            .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
        let mut profile = canonical_root.clone();
        for component in profile_relative_components(owner_osl_user_id, id)? {
            profile.push(component);
            ensure_plain_profile_directory(&profile)?;
        }
        let canonical_profile = profile
            .canonicalize()
            .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
        canonical_profile
            .starts_with(&canonical_root)
            .then_some(canonical_profile)
            .ok_or(NativeWindowHostReason::ProfileUnavailable)
    }

    fn ensure_plain_profile_directory(path: &Path) -> Result<(), NativeWindowHostReason> {
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

        let verify = |metadata: fs::Metadata| {
            (metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0)
                .then_some(())
                .ok_or(NativeWindowHostReason::ProfileUnavailable)
        };

        match fs::symlink_metadata(path) {
            Ok(metadata) => verify(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(path).map_err(|_| NativeWindowHostReason::ProfileUnavailable)?;
                // Verify the created path again so a symlink or junction
                // substituted during creation never becomes a client profile.
                verify(
                    fs::symlink_metadata(path)
                        .map_err(|_| NativeWindowHostReason::ProfileUnavailable)?,
                )
            }
            Err(_) => Err(NativeWindowHostReason::ProfileUnavailable),
        }
    }

    fn launch_isolated(spec: &LaunchSpec) -> std::io::Result<(Child, JobHandle)> {
        debug_assert!(spec.profile.is_absolute());
        let job = JobHandle::new()?;
        let mut child = Command::new(spec.executable.path())
            .args(&spec.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_SUSPENDED)
            .spawn()?;
        if let Err(error) = job.assign_suspended(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok((child, job))
    }

    fn dedicated_window_stable_samples(id: NativeAppId) -> usize {
        if id == NativeAppId::Discord {
            DISCORD_STABLE_WINDOW_SAMPLES
        } else {
            STABLE_WINDOW_SAMPLES
        }
    }

    fn wait_for_process_window(
        id: NativeAppId,
        child: &mut Child,
        job: &JobHandle,
        publisher: ExecutablePublisher,
        timeout: Duration,
    ) -> Option<(HWND, u32, TrustedExecutable)> {
        let deadline = Instant::now() + timeout;
        let mut stable_identity: Option<(isize, u32, PathBuf)> = None;
        let mut stable_samples = 0usize;
        loop {
            if let Some(window) = find_process_window(id, job, publisher) {
                let identity = (window.0 as isize, window.1, window.2.path().to_owned());
                if stable_identity.as_ref() == Some(&identity)
                    && unsafe { single_visible_top_level_is_target(id, window.1, window.0) }
                {
                    stable_samples += 1;
                } else {
                    stable_identity = Some(identity);
                    stable_samples = 1;
                }
                if stable_samples >= dedicated_window_stable_samples(id)
                    && unsafe { single_visible_top_level_is_target(id, window.1, window.0) }
                {
                    return Some(window);
                }
            } else {
                stable_identity = None;
                stable_samples = 0;
            }
            if Instant::now() >= deadline {
                return None;
            }
            // Reap a launcher that exits after creating a contained Electron
            // child, but keep looking inside the job for that child's window.
            let _ = child.try_wait();
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn find_process_window(
        id: NativeAppId,
        job: &JobHandle,
        publisher: ExecutablePublisher,
    ) -> Option<(HWND, u32, TrustedExecutable)> {
        let mut search = WindowSearch {
            id,
            job,
            publisher,
            best: std::ptr::null_mut(),
            best_area: 0,
            best_pid: 0,
            best_executable: None,
        };
        unsafe {
            EnumWindows(
                Some(enum_window),
                (&mut search as *mut WindowSearch) as LPARAM,
            );
        }
        (!search.best.is_null()).then(|| {
            (
                search.best,
                search.best_pid,
                search
                    .best_executable
                    .expect("trusted window always retains its executable"),
            )
        })
    }

    unsafe extern "system" fn enum_window(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut WindowSearch);
        let Some(pid) = window_process_id(window) else {
            return 1;
        };
        if IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut class_name = [0u16; 64];
        let class_length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        if class_length <= 0
            || !dedicated_window_class_allowed(
                search.id,
                &String::from_utf16_lossy(&class_name[..class_length as usize]),
            )
        {
            return 1;
        }
        let job = &*search.job;
        let Some(path) = trusted_job_process_path(job, pid) else {
            return 1;
        };
        let Ok(executable) = verify_executable(&path, search.publisher) else {
            return 1;
        };
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(window, &mut rect) == 0 {
            return 1;
        }
        let area =
            i64::from((rect.right - rect.left).max(0)) * i64::from((rect.bottom - rect.top).max(0));
        if area > search.best_area {
            search.best = window;
            search.best_area = area;
            search.best_pid = pid;
            search.best_executable = Some(executable);
        }
        1
    }

    fn trusted_job_process_path(job: &JobHandle, pid: u32) -> Option<PathBuf> {
        if pid == 0 {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let result = (|| {
            if !job.contains_process(process) {
                return None;
            }
            let mut path = vec![0u16; 32_768];
            let mut length = path.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) }
                == 0
                || length == 0
            {
                return None;
            }
            path.truncate(length as usize);
            PathBuf::from(OsString::from_wide(&path))
                .canonicalize()
                .ok()
        })();
        let _ = unsafe { CloseHandle(process) };
        result
    }

    fn process_path_in_session(pid: u32, expected_session: u32) -> Option<PathBuf> {
        if pid == 0 {
            return None;
        }
        let mut session = 0u32;
        if unsafe { ProcessIdToSessionId(pid, &mut session) } == 0 || session != expected_session {
            return None;
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let result = (|| {
            let mut path = vec![0u16; 32_768];
            let mut length = path.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) }
                == 0
                || length == 0
            {
                return None;
            }
            path.truncate(length as usize);
            PathBuf::from(OsString::from_wide(&path))
                .canonicalize()
                .ok()
        })();
        let _ = unsafe { CloseHandle(process) };
        result
    }

    unsafe fn window_process_id(window: HWND) -> Option<u32> {
        if window.is_null() {
            return None;
        }
        let mut pid = 0;
        (GetWindowThreadProcessId(window, &mut pid) != 0 && pid != 0).then_some(pid)
    }

    fn certified_protected_child_binary(id: NativeAppId, path: &Path) -> bool {
        if id != NativeAppId::Telegram {
            return false;
        }
        let mut version = RtlOsVersionInfo {
            size: std::mem::size_of::<RtlOsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service_pack: [0; 128],
        };
        if unsafe { RtlGetVersion(&mut version) } != 0
            || version.build != CERTIFIED_TELEGRAM_WINDOWS_BUILD
        {
            return false;
        }
        let Ok(mut file) = fs::File::open(path) else {
            return false;
        };
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let Ok(read) = file.read(&mut buffer) else {
                return false;
            };
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hasher.finalize().as_slice() == CERTIFIED_TELEGRAM_SHA256
    }

    unsafe fn adopt_borderless_owned_window(
        generation: u64,
        id: NativeAppId,
        mode: DiscordSessionMode,
        owner_namespace: String,
        mut process: HostedProcess,
        window_process_id: u32,
        trusted_window_executable: TrustedWindowExecutable,
        window: HWND,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        if !protected_child_mode_allowed(mode) {
            stop_owned_process(&mut process);
            return Err(NativeWindowHostReason::WindowOperationRejected);
        }
        let previous_owner = GetWindowLongPtrW(window, GWLP_HWNDPARENT);
        let previous_style = GetWindowLongPtrW(window, GWL_STYLE);
        let previous_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let previous_iconic = IsIconic(window) != 0;
        let original_dpi_context = GetWindowDpiAwarenessContext(window) as isize;
        let mut previous_rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if original_dpi_context == 0
            || !trusted_capture_parent(parent)
            || GetWindowRect(window, &mut previous_rect) == 0
        {
            stop_owned_process(&mut process);
            return Err(NativeWindowHostReason::WindowOperationRejected);
        }
        if !single_visible_top_level_is_target(id, window_process_id, window) {
            stop_owned_process(&mut process);
            return Err(NativeWindowHostReason::ExistingSessionAmbiguous);
        }
        if previous_iconic {
            ShowWindow(window, SW_RESTORE);
            if matches!(id, NativeAppId::Signal | NativeAppId::Whatsapp) {
                // Electron windows can keep reporting their minimized sentinel
                // rectangle briefly after SW_RESTORE. Wait for the saved
                // placement to materialize before the first exact move;
                // subsequent bounded retries use the same delay below.
                thread::sleep(SIGNAL_RESTORE_SETTLE_DELAY);
            }
        }
        if let Err(reason) =
            attach_child_window(id, window, parent, window_process_id, original_dpi_context)
        {
            restore_original_presentation(
                window,
                previous_owner as HWND,
                previous_style,
                previous_ex_style,
                previous_rect,
                previous_iconic,
            );
            stop_owned_process(&mut process);
            return Err(reason);
        }
        let capture_certified =
            certified_protected_child_binary(id, trusted_window_executable.path());
        Ok(HostedWindow {
            generation,
            id,
            mode,
            owner_namespace,
            window_process_id,
            process,
            trusted_window_executable,
            window: window as isize,
            trusted_parent: parent as isize,
            previous_owner,
            previous_style,
            previous_ex_style,
            previous_rect: [
                previous_rect.left,
                previous_rect.top,
                previous_rect.right,
                previous_rect.bottom,
            ],
            previous_iconic,
            original_dpi_context,
            capture_certified,
            last_aligned_rect: parent_target_rect(parent).map(rect_array),
            borrowed_control_shield: None,
            borrowed_tether: None,
            borrowed_recovery_guardian: None,
            attached: true,
        })
    }

    unsafe fn adopt_existing_companion(
        generation: u64,
        id: NativeAppId,
        owner_namespace: String,
        process_id: u32,
        creation_time: u64,
        session_id: u32,
        process: ProcessHandle,
        trusted_window_executable: TrustedWindowExecutable,
        window: HWND,
        owner: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        let previous_owner = GetWindowLongPtrW(window, GWLP_HWNDPARENT);
        let previous_style = GetWindowLongPtrW(window, GWL_STYLE);
        let previous_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let previous_iconic = IsIconic(window) != 0;
        let previous_placement = capture_borrowed_placement(window)
            .ok_or(NativeWindowHostReason::BorrowedPlacementRejected)?;
        let previous_rect = borrowed_previous_rect(window, previous_iconic)
            .ok_or(NativeWindowHostReason::BorrowedPlacementRejected)?;
        if !borrowed_style_is_preserved(
            previous_style,
            previous_ex_style,
            GetWindowLongPtrW(window, GWL_STYLE),
            GetWindowLongPtrW(window, GWL_EXSTYLE),
        ) {
            return Err(NativeWindowHostReason::BorrowedStyleRejected);
        }
        let attached_owner = if id == NativeAppId::Discord {
            owner as isize
        } else {
            previous_owner
        };
        let recovery_snapshot = BorrowedRecoverySnapshot {
            id,
            window: window as isize,
            process_id,
            creation_time,
            session_id,
            expected_path: trusted_window_executable.path().to_owned(),
            owner: previous_owner,
            attached_owner,
            style: previous_style,
            ex_style: previous_ex_style,
            placement: previous_placement,
        };
        let recovery_guardian = BorrowedRecoveryGuardian::arm(&recovery_snapshot)
            .ok_or(NativeWindowHostReason::WindowOperationRejected)?;
        debug_assert!(borrowed_mutation_transition(
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed,
        ));
        let task_ex_style = borrowed_task_ex_style(
            previous_ex_style,
            WS_EX_APPWINDOW as isize,
            WS_EX_TOOLWINDOW as isize,
        );
        if id == NativeAppId::Discord {
            SetWindowLongPtrW(window, GWLP_HWNDPARENT, attached_owner);
            debug_assert!(borrowed_mutation_transition(
                BorrowedMutationStage::GuardianArmed,
                BorrowedMutationStage::OwnerLinked,
            ));
        }
        SetWindowLongPtrW(window, GWL_EXSTYLE, task_ex_style);
        let style_applied = SetWindowPos(
            window,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        ) != 0
            && GetWindowLongPtrW(window, GWL_EXSTYLE) == task_ex_style
            && GetWindowLongPtrW(window, GWL_STYLE) == previous_style
            && borrowed_owner_contract_unchanged(
                attached_owner,
                GetWindowLongPtrW(window, GWLP_HWNDPARENT),
            );
        if !style_applied {
            if restore_guardian_snapshot(&recovery_snapshot) {
                recovery_guardian.cancel_after_verified_restore();
            }
            return Err(NativeWindowHostReason::BorrowedStyleRejected);
        }
        if id == NativeAppId::Discord {
            debug_assert!(borrowed_mutation_transition(
                BorrowedMutationStage::OwnerLinked,
                BorrowedMutationStage::TaskStyleApplied,
            ));
        }
        if previous_iconic {
            ShowWindow(window, SW_RESTORE);
        }
        if let Err(reason) = present_borrowed_window(id, window, owner) {
            if restore_guardian_snapshot(&recovery_snapshot) {
                recovery_guardian.cancel_after_verified_restore();
            }
            return Err(reason);
        }
        let Some(borrowed_control_shield) = BorrowedControlShield::create(window, process_id)
            .filter(BorrowedControlShield::position)
        else {
            if restore_guardian_snapshot(&recovery_snapshot) {
                recovery_guardian.cancel_after_verified_restore();
            }
            return Err(NativeWindowHostReason::WindowOperationRejected);
        };
        let borrowed_tether = if id == NativeAppId::Discord {
            let tether_snapshot = BorrowedTetherSnapshot {
                generation,
                window: window as isize,
                parent: owner as isize,
                process_id,
                creation_time,
                session_id,
                expected_path: trusted_window_executable.path().to_owned(),
            };
            let Some(tether) = BorrowedWindowTether::create(tether_snapshot) else {
                drop(borrowed_control_shield);
                if restore_guardian_snapshot(&recovery_snapshot) {
                    recovery_guardian.cancel_after_verified_restore();
                }
                return Err(NativeWindowHostReason::WindowOperationRejected);
            };
            Some(tether)
        } else {
            None
        };
        Ok(HostedWindow {
            generation,
            id,
            mode: DiscordSessionMode::ExistingSession,
            owner_namespace,
            window_process_id: process_id,
            process: HostedProcess::Borrowed {
                process_id,
                creation_time,
                process,
            },
            trusted_window_executable,
            window: window as isize,
            trusted_parent: owner as isize,
            previous_owner,
            previous_style,
            previous_ex_style,
            previous_rect: [
                previous_rect.left,
                previous_rect.top,
                previous_rect.right,
                previous_rect.bottom,
            ],
            previous_iconic,
            original_dpi_context: 0,
            capture_certified: false,
            last_aligned_rect: parent_target_rect(owner).map(rect_array),
            borrowed_control_shield: Some(borrowed_control_shield),
            borrowed_tether,
            borrowed_recovery_guardian: Some(recovery_guardian),
            attached: true,
        })
    }

    fn stop_owned_process(process: &mut HostedProcess) {
        if let HostedProcess::Dedicated { child, job } = process {
            job.terminate();
            let _ = child.wait();
        }
    }

    unsafe fn parent_target_rect(parent: HWND) -> Option<RECT> {
        let mut client = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(parent, &mut client) == 0 {
            return None;
        }
        let mut origin = POINT {
            x: 0,
            y: TRUSTED_VERTICAL_RESERVE,
        };
        if ClientToScreen(parent, &mut origin) == 0 {
            return None;
        }
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - TRUSTED_VERTICAL_RESERVE).max(1);
        Some(RECT {
            left: origin.x,
            top: origin.y,
            right: origin.x + width,
            bottom: origin.y + height,
        })
    }

    /// Child HWND geometry is expressed in the parent client's coordinate
    /// space. Comparing its screen-virtualized `GetWindowRect` directly with a
    /// `ClientToScreen` result is not stable across mixed-DPI processes.
    unsafe fn parent_target_child_rect(parent: HWND) -> Option<[i32; 4]> {
        let mut client: RECT = std::mem::zeroed();
        if GetClientRect(parent, &mut client) == 0 {
            return None;
        }
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - TRUSTED_VERTICAL_RESERVE).max(1);
        Some([
            0,
            TRUSTED_VERTICAL_RESERVE,
            width,
            TRUSTED_VERTICAL_RESERVE + height,
        ])
    }

    unsafe fn child_rect_in_parent(window: HWND, parent: HWND) -> Option<[i32; 4]> {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(window, &mut rect) == 0 {
            return None;
        }
        let mut top_left = POINT {
            x: rect.left,
            y: rect.top,
        };
        let mut bottom_right = POINT {
            x: rect.right,
            y: rect.bottom,
        };
        // Map each point independently so RTL rectangle auto-swapping cannot
        // alter the security comparison.
        MapWindowPoints(std::ptr::null_mut(), parent, &mut top_left, 1);
        MapWindowPoints(std::ptr::null_mut(), parent, &mut bottom_right, 1);
        Some([top_left.x, top_left.y, bottom_right.x, bottom_right.y])
    }

    fn rect_array(rect: RECT) -> [i32; 4] {
        [rect.left, rect.top, rect.right, rect.bottom]
    }

    unsafe fn align_to_parent(window: HWND, parent: HWND) -> bool {
        let Some(target) = parent_target_rect(parent) else {
            return false;
        };
        let mut client: RECT = std::mem::zeroed();
        if GetClientRect(parent, &mut client) == 0 {
            return false;
        }
        SetWindowPos(
            window,
            HWND_TOP,
            0,
            TRUSTED_VERTICAL_RESERVE,
            target.right - target.left,
            target.bottom - target.top,
            SWP_FRAMECHANGED | SWP_SHOWWINDOW,
        ) != 0
    }

    unsafe fn trusted_capture_parent(parent: HWND) -> bool {
        if parent.is_null()
            || window_process_id(parent) != Some(std::process::id())
            || !GetParent(parent).is_null()
            || GetAncestor(parent, GA_ROOT) != parent
        {
            return false;
        }
        let mut affinity = 0u32;
        GetWindowDisplayAffinity(parent, &mut affinity) != 0 && affinity == WDA_EXCLUDEFROMCAPTURE
    }

    /// A minimized top-level window reports the Windows sentinel outer bounds
    /// and a tiny client area. Restore only the exact OSL-owned,
    /// capture-excluded parent before deriving child coverage. This does not
    /// discover, activate, or mutate any foreign application window.
    unsafe fn prepare_trusted_capture_parent(parent: HWND) -> bool {
        if !trusted_capture_parent(parent) {
            return false;
        }
        if IsIconic(parent) != 0 {
            ShowWindow(parent, SW_RESTORE);
        }
        let deadline = Instant::now() + PARENT_RESTORE_SETTLE;
        loop {
            let mut client: RECT = std::mem::zeroed();
            if trusted_capture_parent(parent)
                && IsWindowVisible(parent) != 0
                && IsIconic(parent) == 0
                && GetClientRect(parent, &mut client) != 0
                && client.right > client.left
                && client.bottom - client.top > TRUSTED_VERTICAL_RESERVE
            {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    unsafe fn attach_child_window(
        id: NativeAppId,
        window: HWND,
        parent: HWND,
        process_id: u32,
        original_dpi_context: isize,
    ) -> Result<(), NativeWindowHostReason> {
        if !trusted_capture_parent(parent) || original_dpi_context == 0 {
            return Err(NativeWindowHostReason::ChildHierarchyRejected);
        }
        let previous_style = GetWindowLongPtrW(window, GWL_STYLE);
        let previous_ex_style = GetWindowLongPtrW(window, GWL_EXSTYLE);
        let chrome =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        SetWindowLongPtrW(
            window,
            GWL_STYLE,
            (previous_style & !(chrome | WS_POPUP as isize)) | WS_CHILD as isize,
        );
        SetWindowLongPtrW(
            window,
            GWL_EXSTYLE,
            previous_ex_style & !((WS_EX_APPWINDOW | WS_EX_TOOLWINDOW) as isize),
        );
        SetLastError(ERROR_SUCCESS);
        let old_parent = SetParent(window, parent);
        if old_parent.is_null() && GetLastError() != ERROR_SUCCESS {
            SetWindowLongPtrW(window, GWL_STYLE, previous_style);
            SetWindowLongPtrW(window, GWL_EXSTYLE, previous_ex_style);
            return Err(NativeWindowHostReason::ChildHierarchyRejected);
        }
        let attempts = child_presentation_attempt_limit(id);
        let mut last_reason = NativeWindowHostReason::WindowOperationRejected;
        for attempt in 0..attempts {
            if attempt > 0 {
                thread::sleep(if id == NativeAppId::Signal {
                    SIGNAL_RESTORE_SETTLE_DELAY
                } else {
                    TELEGRAM_PRESENTATION_SETTLE_DELAY
                });
            }
            if !align_to_parent(window, parent) {
                last_reason = NativeWindowHostReason::ChildBoundsRejected;
                continue;
            }
            match child_presentation_failure(id, window, parent, process_id) {
                None => return Ok(()),
                Some(reason) => last_reason = reason,
            }
        }
        Err(last_reason)
    }

    unsafe fn present_verified_child(hosted: &HostedWindow, parent: HWND) -> bool {
        if parent.is_null()
            || parent as isize != hosted.trusted_parent
            || GetParent(hosted.window as HWND) != parent
        {
            return false;
        }
        let window = hosted.window as HWND;
        ShowWindow(window, SW_RESTORE);
        trusted_capture_parent(parent)
            && align_to_parent(window, parent)
            && child_presentation_is_verified(
                hosted.id,
                window,
                parent,
                hosted.window_process_id,
                hosted.original_dpi_context,
            )
    }

    unsafe fn realign_verified_child(hosted: &mut HostedWindow, parent: HWND) -> bool {
        if parent.is_null()
            || parent as isize != hosted.trusted_parent
            || GetParent(hosted.window as HWND) != parent
            || !trusted_capture_parent(parent)
        {
            return false;
        }
        let Some(expected) = parent_target_rect(parent) else {
            return false;
        };
        let window = hosted.window as HWND;
        let mut actual: RECT = std::mem::zeroed();
        if IsWindowVisible(window) == 0
            || IsIconic(window) != 0
            || GetWindowRect(window, &mut actual) == 0
        {
            return false;
        }
        let expected_array = rect_array(expected);
        let actual_array = rect_array(actual);
        if !aligned_geometry_is_current(hosted.last_aligned_rect, expected_array, actual_array)
            && actual_array != expected_array
            && SetWindowPos(
                window,
                std::ptr::null_mut(),
                0,
                TRUSTED_VERTICAL_RESERVE,
                expected.right - expected.left,
                expected.bottom - expected.top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) == 0
        {
            return false;
        }
        hosted.last_aligned_rect = Some(expected_array);
        child_presentation_is_verified(
            hosted.id,
            window,
            parent,
            hosted.window_process_id,
            hosted.original_dpi_context,
        )
    }

    unsafe fn child_presentation_is_verified(
        id: NativeAppId,
        window: HWND,
        parent: HWND,
        process_id: u32,
        _original_dpi_context: isize,
    ) -> bool {
        child_presentation_failure(id, window, parent, process_id).is_none()
    }

    /// Return only a bounded enum-like failure stage. No HWND, PID, title,
    /// path, geometry, or process error crosses IPC. DPI equality is
    /// deliberately not required: Windows may reset a cross-process child's
    /// awareness during SetParent. A valid non-null current context plus exact
    /// child hierarchy and bounds is the meaningful presentation invariant.
    unsafe fn child_presentation_failure(
        id: NativeAppId,
        window: HWND,
        parent: HWND,
        process_id: u32,
    ) -> Option<NativeWindowHostReason> {
        let Some(expected) = parent_target_child_rect(parent) else {
            return Some(NativeWindowHostReason::ChildBoundsRejected);
        };
        if GetParent(window) != parent
            || IsChild(parent, window) == 0
            || GetAncestor(window, GA_ROOT) != parent
        {
            return Some(NativeWindowHostReason::ChildHierarchyRejected);
        }
        if window_process_id(window) != Some(process_id) {
            return Some(NativeWindowHostReason::ChildProcessRejected);
        }
        if GetWindowDpiAwarenessContext(window).is_null() {
            return Some(NativeWindowHostReason::ChildDpiRejected);
        }
        let style = GetWindowLongPtrW(window, GWL_STYLE);
        if style & WS_CHILD as isize == 0 || style & WS_POPUP as isize != 0 {
            return Some(NativeWindowHostReason::ChildStyleRejected);
        }
        if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
            return Some(NativeWindowHostReason::ChildVisibilityRejected);
        }
        let Some(actual) = child_rect_in_parent(window, parent) else {
            return Some(NativeWindowHostReason::ChildBoundsRejected);
        };
        if !borrowed_presentation_matches(true, false, expected, actual) {
            return Some(NativeWindowHostReason::ChildBoundsRejected);
        }
        if !no_visible_top_level_for_process(id, process_id, window) {
            return Some(NativeWindowHostReason::ChildSiblingRejected);
        }
        None
    }

    struct VisibleSiblingSearch {
        id: NativeAppId,
        process_id: u32,
        target: HWND,
        target_seen: bool,
        other_seen: bool,
    }

    unsafe extern "system" fn enum_visible_sibling(window: HWND, parameter: LPARAM) -> BOOL {
        let search = &mut *(parameter as *mut VisibleSiblingSearch);
        if IsWindowVisible(window) != 0 && window_process_id(window) == Some(search.process_id) {
            if window == search.target {
                search.target_seen = true;
            } else if search.id == NativeAppId::Telegram
                && telegram_owned_frame_decoration(search.target, window)
            {
                return 1;
            } else {
                search.other_seen = true;
            }
        }
        1
    }

    unsafe fn telegram_owned_frame_decoration(target: HWND, candidate: HWND) -> bool {
        if target.is_null() || candidate.is_null() {
            return false;
        }
        let style = GetWindowLongPtrW(candidate, GWL_STYLE);
        let interactive_chrome =
            (WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize;
        let mut target_rect: RECT = std::mem::zeroed();
        let mut candidate_rect: RECT = std::mem::zeroed();
        GetWindowRect(target, &mut target_rect) != 0
            && GetWindowRect(candidate, &mut candidate_rect) != 0
            && telegram_frame_decoration_matches(
                window_process_id(target) == window_process_id(candidate),
                GetParent(candidate) == target,
                style & WS_POPUP as isize != 0,
                style & WS_CHILD as isize != 0,
                style & WS_CAPTION as isize != 0,
                style & interactive_chrome != 0,
                rect_array(target_rect),
                rect_array(candidate_rect),
            )
    }

    unsafe fn visible_top_level_process_state(
        id: NativeAppId,
        process_id: u32,
        target: HWND,
    ) -> (bool, bool) {
        let mut search = VisibleSiblingSearch {
            id,
            process_id,
            target,
            target_seen: false,
            other_seen: false,
        };
        EnumWindows(
            Some(enum_visible_sibling),
            (&mut search as *mut VisibleSiblingSearch) as LPARAM,
        );
        (search.target_seen, search.other_seen)
    }

    unsafe fn single_visible_top_level_is_target(
        id: NativeAppId,
        process_id: u32,
        target: HWND,
    ) -> bool {
        visible_top_level_process_state(id, process_id, target) == (true, false)
    }

    unsafe fn no_visible_top_level_for_process(
        id: NativeAppId,
        process_id: u32,
        target: HWND,
    ) -> bool {
        visible_top_level_process_state(id, process_id, target) == (false, false)
    }

    unsafe fn present_borrowed_window(
        id: NativeAppId,
        window: HWND,
        parent: HWND,
    ) -> Result<(), NativeWindowHostReason> {
        let Some(expected) = parent_target_rect(parent) else {
            return Err(NativeWindowHostReason::BorrowedBoundsRejected);
        };
        // Existing user sessions remain independent top-level windows. They
        // are aligned for continuity but never inherit OSL capture claims.
        let mut last_reason = NativeWindowHostReason::BorrowedBoundsRejected;
        for attempt in 0..borrowed_presentation_attempt_limit(id) {
            ShowWindow(window, SW_RESTORE);
            if attempt > 0 {
                thread::sleep(SIGNAL_RESTORE_SETTLE_DELAY);
            }
            if SetWindowPos(
                window,
                HWND_TOP,
                expected.left,
                expected.top,
                expected.right - expected.left,
                expected.bottom - expected.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            ) == 0
            {
                last_reason = NativeWindowHostReason::BorrowedBoundsRejected;
                continue;
            }
            if IsWindowVisible(window) == 0 || IsIconic(window) != 0 {
                last_reason = NativeWindowHostReason::BorrowedVisibilityRejected;
                continue;
            }
            let mut actual: RECT = std::mem::zeroed();
            if GetWindowRect(window, &mut actual) == 0
                || !borrowed_presentation_matches(
                    true,
                    false,
                    [expected.left, expected.top, expected.right, expected.bottom],
                    [actual.left, actual.top, actual.right, actual.bottom],
                )
            {
                last_reason = NativeWindowHostReason::BorrowedBoundsRejected;
                continue;
            }
            let _ = BringWindowToTop(window);
            let _ = SetForegroundWindow(window);
            return Ok(());
        }
        Err(last_reason)
    }

    unsafe fn borrowed_previous_rect(window: HWND, iconic: bool) -> Option<RECT> {
        let mut rect: RECT = std::mem::zeroed();
        let actual = (GetWindowRect(window, &mut rect) != 0).then_some(rect_array(rect));
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let normal = (GetWindowPlacement(window, &mut placement) != 0)
            .then_some(rect_array(placement.rcNormalPosition));
        borrowed_rect_choice(actual, iconic, normal).map(|[left, top, right, bottom]| RECT {
            left,
            top,
            right,
            bottom,
        })
    }

    unsafe fn capture_borrowed_placement(window: HWND) -> Option<BorrowedWindowPlacement> {
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        (GetWindowPlacement(window, &mut placement) != 0).then_some(BorrowedWindowPlacement {
            flags: placement.flags,
            show_cmd: placement.showCmd,
            min_position: [placement.ptMinPosition.x, placement.ptMinPosition.y],
            max_position: [placement.ptMaxPosition.x, placement.ptMaxPosition.y],
            normal_position: rect_array(placement.rcNormalPosition),
        })
    }

    unsafe fn realign_borrowed_window(hosted: &mut HostedWindow, parent: HWND) -> bool {
        if hosted.trusted_parent != parent as isize {
            return false;
        }
        if let Some(tether) = hosted.borrowed_tether.as_ref() {
            let aligned = tether.reconcile(hosted.generation);
            if aligned {
                hosted.last_aligned_rect = parent_target_rect(parent).map(rect_array);
                return hosted
                    .borrowed_control_shield
                    .as_ref()
                    .is_some_and(BorrowedControlShield::position);
            }
            return false;
        }
        let Some(expected) = parent_target_rect(parent) else {
            return false;
        };
        let window = hosted.window as HWND;
        let mut actual: RECT = std::mem::zeroed();
        if IsWindowVisible(window) == 0
            || IsIconic(window) != 0
            || GetWindowRect(window, &mut actual) == 0
        {
            return false;
        }
        let expected_array = rect_array(expected);
        let actual_array = rect_array(actual);
        if !aligned_geometry_is_current(hosted.last_aligned_rect, expected_array, actual_array)
            && actual_array != expected_array
            && SetWindowPos(
                window,
                std::ptr::null_mut(),
                expected.left,
                expected.top,
                expected.right - expected.left,
                expected.bottom - expected.top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) == 0
        {
            return false;
        }
        hosted.last_aligned_rect = Some(expected_array);
        let mut verified: RECT = std::mem::zeroed();
        GetWindowRect(window, &mut verified) != 0
            && borrowed_presentation_matches(
                IsWindowVisible(window) != 0,
                IsIconic(window) != 0,
                expected_array,
                rect_array(verified),
            )
            && hosted
                .borrowed_control_shield
                .as_ref()
                .is_some_and(BorrowedControlShield::position)
    }

    unsafe fn restore_borrowed_window(window: HWND, saved: BorrowedWindowPlacement) {
        let [left, top, right, bottom] = saved.normal_position;
        let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        placement.flags = saved.flags;
        placement.showCmd = saved.show_cmd;
        placement.ptMinPosition = POINT {
            x: saved.min_position[0],
            y: saved.min_position[1],
        };
        placement.ptMaxPosition = POINT {
            x: saved.max_position[0],
            y: saved.max_position[1],
        };
        placement.rcNormalPosition = RECT {
            left,
            top,
            right,
            bottom,
        };
        let _ = SetWindowPlacement(window, &placement);
    }

    unsafe fn restore_original_presentation(
        window: HWND,
        parent: HWND,
        style: isize,
        ex_style: isize,
        rect: RECT,
        iconic: bool,
    ) {
        SetLastError(ERROR_SUCCESS);
        // Restore top-level status before its original chrome and owner. A
        // prior owner is not a child parent and must be restored separately.
        let _ = SetParent(window, std::ptr::null_mut());
        SetWindowLongPtrW(window, GWL_STYLE, style);
        SetWindowLongPtrW(window, GWL_EXSTYLE, ex_style);
        SetWindowLongPtrW(window, GWLP_HWNDPARENT, parent as isize);
        if rect.right > rect.left && rect.bottom > rect.top {
            ShowWindow(window, SW_RESTORE);
            let _ = SetWindowPos(
                window,
                HWND_TOP,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_FRAMECHANGED | SWP_SHOWWINDOW,
            );
        }
        if iconic {
            ShowWindow(window, SW_MINIMIZE);
        }
    }

    fn hosted_window_is_valid(hosted: &HostedWindow) -> bool {
        if unsafe { window_process_id(hosted.window as HWND) } != Some(hosted.window_process_id) {
            return false;
        }
        if hosted.mode == DiscordSessionMode::ExistingSession
            && (hosted.trusted_parent == 0
                || unsafe {
                    GetWindowLongPtrW(hosted.window as HWND, GWLP_HWNDPARENT)
                        != if hosted.id == NativeAppId::Discord {
                            hosted.trusted_parent
                        } else {
                            hosted.previous_owner
                        }
                }
                || hosted
                    .borrowed_tether
                    .as_ref()
                    .is_some_and(|tether| !tether.is_healthy(hosted.generation)))
        {
            return false;
        }
        let mut osl_session = 0u32;
        let mut hosted_session = 0u32;
        if unsafe {
            ProcessIdToSessionId(std::process::id(), &mut osl_session) == 0
                || ProcessIdToSessionId(hosted.window_process_id, &mut hosted_session) == 0
        } || osl_session != hosted_session
        {
            return false;
        }
        match &hosted.process {
            HostedProcess::Dedicated { job, .. } => {
                trusted_job_process_path(job, hosted.window_process_id)
                    .is_some_and(|path| path == hosted.trusted_window_executable.path())
            }
            HostedProcess::Borrowed {
                process_id,
                creation_time,
                process,
            } => {
                *process_id == hosted.window_process_id
                    && borrowed_process_is_valid(
                        *process_id,
                        *creation_time,
                        process,
                        hosted.trusted_window_executable.path(),
                        hosted.id,
                    )
            }
        }
    }

    pub(super) fn current_discord_service_host(
        state: &NativeWindowHostState,
        owner_osl_user_id: &str,
    ) -> Result<crate::service_host::ActiveServiceHost, String> {
        let requested_owner = crate::service_host::owner_profile_namespace(owner_osl_user_id)
            .map_err(|_| "The trusted native Discord owner is unavailable".to_owned())?;
        let guard = state
            .inner
            .lock()
            .map_err(|_| "The trusted native Discord host is unavailable".to_owned())?;
        let hosted = guard
            .as_ref()
            .filter(|hosted| {
                native_context_matches(
                    hosted.attached,
                    hosted.id,
                    &hosted.owner_namespace,
                    &requested_owner,
                ) && hosted_window_is_valid(hosted)
            })
            .ok_or_else(|| "The trusted native Discord host is unavailable".to_owned())?;
        let account_id = native_discord_account_id(&requested_owner)
            .ok_or_else(|| "The trusted native Discord account is unavailable".to_owned())?;
        Ok(crate::service_host::ActiveServiceHost {
            service_id: "discord".to_owned(),
            account_id,
            generation: hosted.generation,
            owner_namespace: requested_owner,
        })
    }

    pub(super) fn with_current_discord_accessibility_target<T>(
        state: &NativeWindowHostState,
        owner_osl_user_id: &str,
        operation: impl FnOnce(
            NativeDiscordAccessibilityTarget,
            &dyn Fn(u32) -> bool,
        ) -> Result<T, String>,
    ) -> Result<T, String> {
        let requested_owner = crate::service_host::owner_profile_namespace(owner_osl_user_id)
            .map_err(|_| "The trusted native Discord owner is unavailable".to_owned())?;
        let guard = state
            .inner
            .lock()
            .map_err(|_| "The trusted native Discord host is unavailable".to_owned())?;
        let hosted = guard
            .as_ref()
            .filter(|hosted| {
                native_context_matches(
                    hosted.attached,
                    hosted.id,
                    &hosted.owner_namespace,
                    &requested_owner,
                ) && hosted_window_is_valid(hosted)
            })
            .ok_or_else(|| "The trusted native Discord host is unavailable".to_owned())?;
        let target = NativeDiscordAccessibilityTarget {
            generation: hosted.generation,
            window: hosted.window,
            process_id: hosted.window_process_id,
        };
        let mut borrowed_session = 0u32;
        let borrowed_cache = std::cell::RefCell::new(std::collections::HashMap::new());
        if matches!(hosted.process, HostedProcess::Borrowed { .. })
            && unsafe { ProcessIdToSessionId(hosted.window_process_id, &mut borrowed_session) } == 0
        {
            return Err("The trusted native Discord process session is unavailable".to_owned());
        }
        let process_is_trusted = |process_id: u32| match &hosted.process {
            HostedProcess::Dedicated { job, .. } => trusted_job_process_path(job, process_id)
                .is_some_and(|path| {
                    verify_executable(&path, ExecutablePublisher::Discord)
                        .is_ok_and(|trusted| trusted.path() == path)
                }),
            HostedProcess::Borrowed { .. } => *borrowed_cache
                .borrow_mut()
                .entry(process_id)
                .or_insert_with(|| {
                    process_path_in_session(process_id, borrowed_session).is_some_and(|path| {
                        path == hosted.trusted_window_executable.path()
                            && verify_executable(&path, ExecutablePublisher::Discord)
                                .is_ok_and(|trusted| trusted.path() == path)
                    })
                }),
        };
        let result = operation(target, &process_is_trusted)?;
        if !hosted_window_is_valid(hosted) || hosted.generation != target.generation {
            return Err("The trusted native Discord window changed".to_owned());
        }
        Ok(result)
    }

    pub(super) fn discord_overlay_target(
        state: &NativeWindowHostState,
        owner_osl_user_id: &str,
    ) -> Result<NativeDiscordOverlayTarget, String> {
        let requested_owner = crate::service_host::owner_profile_namespace(owner_osl_user_id)
            .map_err(|_| "The trusted native Discord owner is unavailable".to_owned())?;
        let guard = state
            .inner
            .lock()
            .map_err(|_| "The trusted native Discord window is unavailable".to_owned())?;
        let hosted = guard
            .as_ref()
            .filter(|hosted| {
                native_context_matches(
                    hosted.attached,
                    hosted.id,
                    &hosted.owner_namespace,
                    &requested_owner,
                ) && hosted_window_is_valid(hosted)
            })
            .ok_or_else(|| "The trusted native Discord window is unavailable".to_owned())?;
        let window = hosted.window as HWND;
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        if unsafe {
            IsWindowVisible(window) == 0
                || IsIconic(window) != 0
                || GetWindowRect(window, &mut rect) == 0
        } || rect.right <= rect.left
            || rect.bottom <= rect.top
        {
            return Err("The trusted native Discord window is unavailable".to_owned());
        }
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_root = if foreground.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe { GetAncestor(foreground, GA_ROOT) }
        };
        let target_root = unsafe { GetAncestor(window, GA_ROOT) };
        Ok(NativeDiscordOverlayTarget {
            generation: hosted.generation,
            rect: [rect.left, rect.top, rect.right, rect.bottom],
            foreground: !foreground_root.is_null() && foreground_root == target_root,
        })
    }

    pub(super) fn discord_accessibility_snapshot(
        state: &NativeWindowHostState,
    ) -> crate::native_discord_adapter::NativeDiscordAccessibilitySnapshot {
        use crate::native_discord_adapter::{
            DiscordSnapshotReason, NativeDiscordAccessibilitySnapshot,
        };

        let guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeDiscordAccessibilitySnapshot::unavailable(
                    0,
                    DiscordSnapshotReason::NotHosted,
                )
            }
        };
        let Some(hosted) = guard.as_ref() else {
            return NativeDiscordAccessibilitySnapshot::unavailable(
                0,
                DiscordSnapshotReason::NotHosted,
            );
        };
        if hosted.id != NativeAppId::Discord || !hosted_window_is_valid(hosted) {
            return NativeDiscordAccessibilitySnapshot::unavailable(
                hosted.generation,
                DiscordSnapshotReason::HostIdentityChanged,
            );
        }
        let target = NativeDiscordAccessibilityTarget {
            generation: hosted.generation,
            window: hosted.window,
            process_id: hosted.window_process_id,
        };
        let process_is_trusted = |process_id: u32| match &hosted.process {
            HostedProcess::Dedicated { job, .. } => trusted_job_process_path(job, process_id)
                .is_some_and(|path| {
                    verify_executable(&path, ExecutablePublisher::Discord)
                        .is_ok_and(|trusted| trusted.path() == path)
                }),
            HostedProcess::Borrowed {
                process_id: borrowed_pid,
                creation_time,
                process,
            } => {
                borrowed_snapshot_pid_matches(*borrowed_pid, process_id)
                    && borrowed_process_is_valid(
                        *borrowed_pid,
                        *creation_time,
                        process,
                        hosted.trusted_window_executable.path(),
                        NativeAppId::Discord,
                    )
            }
        };
        let snapshot =
            crate::native_discord_adapter::snapshot_claimed_window(target, &process_is_trusted);
        if !hosted_window_is_valid(hosted) || hosted.generation != snapshot.generation {
            return NativeDiscordAccessibilitySnapshot::unavailable(
                hosted.generation,
                DiscordSnapshotReason::HostIdentityChanged,
            );
        }
        snapshot
    }

    pub(super) unsafe fn shutdown_hosted(mut hosted: HostedWindow) {
        hosted.borrowed_tether.take();
        hosted.borrowed_control_shield.take();
        let _ = restore_window(&mut hosted);
        stop_owned_process(&mut hosted.process);
    }

    unsafe fn restore_window(hosted: &mut HostedWindow) -> bool {
        let window = hosted.window as HWND;
        if !hosted_window_is_valid(hosted) {
            return false;
        }
        if hosted.mode == DiscordSessionMode::ExistingSession {
            return hosted
                .borrowed_recovery_guardian
                .take()
                .is_some_and(|guardian| guardian.restore_and_cancel());
        }
        let [left, top, right, bottom] = hosted.previous_rect;
        restore_original_presentation(
            window,
            hosted.previous_owner as HWND,
            hosted.previous_style,
            hosted.previous_ex_style,
            RECT {
                left,
                top,
                right,
                bottom,
            },
            hosted.previous_iconic,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_roots(label: &str) -> (PathBuf, PathBuf, PathBuf) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "osl-native-discord-{label}-{}-{unique}",
            std::process::id()
        ));
        let osl = base.join("osl");
        let roaming = base.join("roaming");
        std::fs::create_dir_all(&osl).unwrap();
        std::fs::create_dir_all(&roaming).unwrap();
        (base, osl, roaming)
    }

    #[test]
    fn allowlist_and_profile_names_are_fixed_and_path_free() {
        let ids = [
            NativeAppId::Discord,
            NativeAppId::Telegram,
            NativeAppId::Signal,
            NativeAppId::Whatsapp,
            NativeAppId::Outlook,
        ];
        for id in ids {
            let components = profile_relative_components("owner-a", id).unwrap();
            assert_eq!(components[0], PROFILE_NAMESPACE);
            assert!(components[1].starts_with("owner-"));
            assert!(!components[2].is_empty());
            assert!(components.iter().all(|component| {
                !component.contains(['/', '\\'])
                    && component.as_str() != "."
                    && component.as_str() != ".."
            }));
        }
    }

    #[test]
    fn native_profiles_are_namespaced_by_osl_owner() {
        let owner_a = profile_relative_components("owner-a", NativeAppId::Telegram).unwrap();
        let owner_b = profile_relative_components("owner-b", NativeAppId::Telegram).unwrap();
        let path_a: std::path::PathBuf = owner_a.iter().collect();
        let path_b: std::path::PathBuf = owner_b.iter().collect();

        assert_eq!(owner_a[0], owner_b[0]);
        assert_ne!(owner_a[1], owner_b[1]);
        assert_eq!(owner_a[2], owner_b[2]);
        assert_ne!(path_a, path_b);
    }

    #[test]
    fn invalid_native_profile_owner_fails_closed() {
        assert_eq!(
            profile_relative_components("", NativeAppId::Telegram),
            Err(NativeWindowHostReason::ProfileUnavailable)
        );
        assert_eq!(
            profile_relative_components(&"x".repeat(129), NativeAppId::Telegram),
            Err(NativeWindowHostReason::ProfileUnavailable)
        );
    }

    #[test]
    fn only_locally_verified_secondary_instance_modes_are_enabled() {
        assert!(secondary_instance_verified(NativeAppId::Discord));
        assert!(secondary_instance_verified(NativeAppId::Telegram));
        assert!(!secondary_instance_verified(NativeAppId::Signal));
        assert!(!secondary_instance_verified(NativeAppId::Whatsapp));
        assert!(!secondary_instance_verified(NativeAppId::Outlook));
    }

    #[test]
    fn signal_dedicated_discovery_rejects_native_error_dialogs() {
        assert!(dedicated_window_class_allowed(
            NativeAppId::Signal,
            SIGNAL_PRIMARY_WINDOW_CLASS,
        ));
        assert!(!dedicated_window_class_allowed(
            NativeAppId::Signal,
            "#32770",
        ));
        assert!(dedicated_window_class_allowed(
            NativeAppId::Telegram,
            "Qt51514QWindowIcon",
        ));
    }

    #[test]
    fn existing_signal_and_whatsapp_accept_only_exact_primary_windows_even_when_hidden() {
        for visible in [true, false] {
            assert!(existing_window_identity_allowed(
                NativeAppId::Signal,
                visible,
                SIGNAL_PRIMARY_WINDOW_CLASS,
                SIGNAL_PRIMARY_WINDOW_TITLE,
            ));
            assert!(existing_window_identity_allowed(
                NativeAppId::Whatsapp,
                visible,
                WHATSAPP_PRIMARY_WINDOW_CLASS,
                WHATSAPP_PRIMARY_WINDOW_TITLE,
            ));
        }

        assert!(!existing_window_identity_allowed(
            NativeAppId::Signal,
            false,
            "Chrome_WidgetWin_0",
            "Signal",
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Signal,
            true,
            SIGNAL_PRIMARY_WINDOW_CLASS,
            "Signal error",
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Whatsapp,
            false,
            "GDI+ Hook Window Class",
            "WhatsApp",
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Whatsapp,
            true,
            WHATSAPP_PRIMARY_WINDOW_CLASS,
            "",
        ));

        assert!(existing_window_identity_allowed(
            NativeAppId::Discord,
            true,
            "Chrome_WidgetWin_1",
            "Discord",
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Telegram,
            false,
            "Qt51514QWindowIcon",
            "Telegram",
        ));
    }

    #[test]
    fn only_dedicated_telegram_gets_one_isolated_profile_retry() {
        assert_eq!(dedicated_launch_attempt_limit(NativeAppId::Telegram), 2);
        assert_eq!(dedicated_launch_attempt_limit(NativeAppId::Discord), 1);
        assert_eq!(dedicated_launch_attempt_limit(NativeAppId::Signal), 1);
        assert_eq!(dedicated_launch_attempt_limit(NativeAppId::Whatsapp), 1);
    }

    #[test]
    fn native_toolkits_get_only_their_bounded_presentation_retries() {
        assert_eq!(child_presentation_attempt_limit(NativeAppId::Telegram), 2);
        assert_eq!(child_presentation_attempt_limit(NativeAppId::Discord), 1);
        assert_eq!(child_presentation_attempt_limit(NativeAppId::Signal), 7);
        assert_eq!(child_presentation_attempt_limit(NativeAppId::Whatsapp), 1);
    }

    #[test]
    fn warm_host_is_reused_only_for_the_same_app_and_osl_owner() {
        assert_eq!(
            warm_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::Dedicated,
                "owner-a",
                true,
                NativeAppId::Discord,
                DiscordSessionMode::Dedicated,
                "owner-a",
            ),
            WarmHostAction::Reuse
        );
        assert_eq!(
            warm_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::Dedicated,
                "owner-a",
                true,
                NativeAppId::Discord,
                DiscordSessionMode::Dedicated,
                "owner-b",
            ),
            WarmHostAction::Replace
        );
        assert_eq!(
            warm_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::Dedicated,
                "owner-a",
                true,
                NativeAppId::Signal,
                DiscordSessionMode::Dedicated,
                "owner-a",
            ),
            WarmHostAction::Replace
        );
        assert_eq!(
            warm_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::Dedicated,
                "owner-a",
                true,
                NativeAppId::Discord,
                DiscordSessionMode::ExistingSession,
                "owner-a",
            ),
            WarmHostAction::Replace
        );
        assert_eq!(
            warm_host_action(
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                "owner-a",
                false,
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                "owner-a",
            ),
            WarmHostAction::Replace
        );
    }

    #[test]
    fn one_button_telegram_state_matrix_is_bounded_and_never_terminates_borrowed_session() {
        // No current host: first-run and already-initialized closed profiles
        // share the same fixed launch. First-run alone receives one retry.
        assert_eq!(
            cold_host_action(DiscordSessionMode::Dedicated),
            ColdHostAction::LaunchDedicated
        );
        assert_eq!(dedicated_launch_attempt_limit(NativeAppId::Telegram), 2);
        assert_eq!(child_presentation_attempt_limit(NativeAppId::Telegram), 2);

        // An already-open ordinary Telegram is claimed/focused, never owned.
        assert_eq!(
            cold_host_action(DiscordSessionMode::ExistingSession),
            ColdHostAction::ClaimExisting
        );
        assert!(!mode_owns_process(DiscordSessionMode::ExistingSession));
        assert!(should_relaunch_existing_session(
            NativeAppId::Telegram,
            NativeWindowHostReason::ExistingSessionUnavailable,
        ));
        assert!(should_relaunch_existing_session(
            NativeAppId::Discord,
            NativeWindowHostReason::ExistingSessionUnavailable,
        ));
        assert!(!should_relaunch_existing_session(
            NativeAppId::Telegram,
            NativeWindowHostReason::ExistingSessionAmbiguous,
        ));

        // A live matching OSL host is reused; an exited/stale one is replaced
        // and follows the same bounded dedicated launch path above.
        assert_eq!(
            warm_host_action(
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                "owner-a",
                true,
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                "owner-a",
            ),
            WarmHostAction::Reuse
        );
        assert_eq!(
            warm_host_action(
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                "owner-a",
                false,
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                "owner-a",
            ),
            WarmHostAction::Replace
        );
    }

    #[test]
    fn existing_session_contract_is_bounded_and_never_capture_claimed() {
        assert!(existing_session_supported(NativeAppId::Discord));
        assert!(existing_session_supported(NativeAppId::Telegram));
        assert!(existing_session_supported(NativeAppId::Signal));
        assert!(existing_session_supported(NativeAppId::Outlook));
        assert!(existing_session_supported(NativeAppId::Whatsapp));
        assert!(should_relaunch_existing_session(
            NativeAppId::Whatsapp,
            NativeWindowHostReason::ExistingSessionUnavailable,
        ));
        assert!(!secondary_instance_verified(NativeAppId::Whatsapp));
        assert_eq!(
            serde_json::to_string(&DiscordSessionMode::Dedicated).unwrap(),
            "\"dedicated\""
        );
        assert_eq!(
            serde_json::to_string(&DiscordSessionMode::ExistingSession).unwrap(),
            "\"existingSession\""
        );
        assert!(mode_owns_process(DiscordSessionMode::Dedicated));
        assert!(!mode_owns_process(DiscordSessionMode::ExistingSession));
        assert!(
            !NativeWindowHostResult::unsupported(
                NativeAppId::Discord,
                NativeWindowHostReason::ExistingSessionUnavailable,
            )
            .capture_protected
        );
        assert!(protected_child_mode_allowed(DiscordSessionMode::Dedicated));
        assert!(!protected_child_mode_allowed(
            DiscordSessionMode::ExistingSession
        ));
        assert!(
            !NativeWindowHostResult::success(
                NativeAppId::Discord,
                NativeWindowHostStatus::Hosted,
                DiscordSessionMode::Dedicated,
            )
            .capture_protected
        );
        assert!(
            NativeWindowHostResult::success_with_capture(
                NativeAppId::Telegram,
                NativeWindowHostStatus::Hosted,
                DiscordSessionMode::Dedicated,
                true,
            )
            .capture_protected
        );
        assert!(
            !NativeWindowHostResult::success_with_capture(
                NativeAppId::Telegram,
                NativeWindowHostStatus::Hosted,
                DiscordSessionMode::ExistingSession,
                true,
            )
            .capture_protected
        );
        assert!(
            !NativeWindowHostResult::success(
                NativeAppId::Discord,
                NativeWindowHostStatus::Hosted,
                DiscordSessionMode::ExistingSession,
            )
            .capture_protected
        );
        assert!(
            !NativeWindowHostResult::success(
                NativeAppId::Discord,
                NativeWindowHostStatus::Detached,
                DiscordSessionMode::Dedicated,
            )
            .capture_protected
        );
    }

    #[test]
    fn outlook_window_classes_and_dedicated_mode_are_bounded() {
        assert!(existing_window_identity_allowed(
            NativeAppId::Outlook,
            true,
            OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS,
            "ignored"
        ));
        assert!(existing_window_identity_allowed(
            NativeAppId::Outlook,
            true,
            OUTLOOK_NEW_PRIMARY_WINDOW_CLASS,
            "ignored"
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Outlook,
            true,
            "Chrome_WidgetWin_1",
            "ignored"
        ));
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Outlook),
            FixedSecondaryLaunch::Unsupported
        );
    }

    #[test]
    fn routine_alignment_skips_only_verified_cached_geometry() {
        let expected = [10, 20, 810, 620];
        assert!(aligned_geometry_is_current(
            Some(expected),
            expected,
            expected,
        ));
        assert!(!aligned_geometry_is_current(None, expected, expected));
        assert!(!aligned_geometry_is_current(
            Some(expected),
            expected,
            [11, 20, 811, 620],
        ));
        assert!(!aligned_geometry_is_current(
            Some([0, 0, 800, 600]),
            expected,
            expected,
        ));
    }

    #[test]
    fn borrowed_identity_rejects_pid_reuse_cross_session_and_wrong_image() {
        let expected = Path::new("C:/Discord/app-1/Discord.exe");
        assert!(borrowed_identity_fields_match(
            41, 41, 100, 100, 2, 2, expected, expected,
        ));
        assert!(!borrowed_identity_fields_match(
            41, 41, 100, 101, 2, 2, expected, expected,
        ));
        assert!(!borrowed_identity_fields_match(
            41, 41, 100, 100, 2, 3, expected, expected,
        ));
        assert!(!borrowed_identity_fields_match(
            41,
            42,
            100,
            100,
            2,
            2,
            expected,
            Path::new("C:/Other/Discord.exe"),
        ));
    }

    #[test]
    fn borrowed_task_style_and_original_owner_restore_are_exact() {
        const APP_WINDOW: isize = 0x0004_0000;
        const TOOL_WINDOW: isize = 0x0000_0080;
        const OTHER_BITS: isize = 0x0000_0108;
        let original = APP_WINDOW | OTHER_BITS;
        let transformed = borrowed_task_ex_style(original, APP_WINDOW, TOOL_WINDOW);
        assert_eq!(transformed & APP_WINDOW, 0);
        assert_eq!(transformed & TOOL_WINDOW, TOOL_WINDOW);
        assert_eq!(
            transformed & !(APP_WINDOW | TOOL_WINDOW),
            OTHER_BITS & !TOOL_WINDOW
        );
        // Recovery stores and reapplies the original value, not an inverse
        // bit operation that could lose app-owned flags.
        assert_eq!(original, APP_WINDOW | OTHER_BITS);
        assert!(borrowed_owner_contract_unchanged(451, 451));
        assert!(!borrowed_owner_contract_unchanged(451, 452));
        assert!(borrowed_owner_is_restorable(451, 900, 451));
        assert!(borrowed_owner_is_restorable(451, 900, 900));
        assert!(!borrowed_owner_is_restorable(451, 900, 901));
        assert!(borrowed_restore_contract_matches(
            451, 451, 0x10, 0x10, original, original, true
        ));
        assert!(!borrowed_restore_contract_matches(
            451,
            451,
            0x10,
            0x10,
            original,
            transformed,
            true
        ));
        assert!(!borrowed_restore_contract_matches(
            451, 451, 0x10, 0x10, original, original, false
        ));
    }

    #[test]
    fn borrowed_mutation_requires_guardian_before_task_style() {
        assert!(borrowed_mutation_transition(
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed
        ));
        assert!(borrowed_mutation_transition(
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::OwnerLinked
        ));
        assert!(borrowed_mutation_transition(
            BorrowedMutationStage::OwnerLinked,
            BorrowedMutationStage::TaskStyleApplied
        ));
        assert!(!borrowed_mutation_transition(
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::TaskStyleApplied
        ));
        assert!(!borrowed_mutation_transition(
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::TaskStyleApplied
        ));
        assert!(!borrowed_mutation_transition(
            BorrowedMutationStage::TaskStyleApplied,
            BorrowedMutationStage::GuardianArmed
        ));
    }

    #[test]
    fn borrowed_tether_retries_desktop_loss_but_fails_closed_on_identity_change() {
        assert_eq!(
            borrowed_tether_decision(BorrowedTetherObservation::Aligned, 0, 3),
            BorrowedTetherDecision::Continue
        );
        assert_eq!(
            borrowed_tether_decision(BorrowedTetherObservation::ParentMinimized, 0, 3),
            BorrowedTetherDecision::Continue
        );
        assert_eq!(
            borrowed_tether_decision(BorrowedTetherObservation::TransientDesktopUnavailable, 2, 3,),
            BorrowedTetherDecision::ContinueAfterTransient
        );
        assert_eq!(
            borrowed_tether_decision(BorrowedTetherObservation::TransientDesktopUnavailable, 3, 3,),
            BorrowedTetherDecision::FailClosed
        );
        assert_eq!(
            borrowed_tether_decision(BorrowedTetherObservation::IdentityChanged, 0, 3),
            BorrowedTetherDecision::FailClosed
        );
        assert!(!borrowed_tether_requires_repair(true, true, false));
        assert!(borrowed_tether_requires_repair(false, true, false));
        assert!(borrowed_tether_requires_repair(true, false, false));
        // Clicking or dragging the OSL shell proactively repairs z-order even
        // when the saved geometry and visibility still look correct.
        assert!(borrowed_tether_requires_repair(true, true, true));
    }

    #[test]
    fn borrowed_guardian_rejects_pid_reuse_time_session_and_path_changes() {
        let expected = Path::new("C:/Program Files/Signal/Signal.exe");
        assert!(borrowed_guardian_identity_matches(
            71, 71, 800, 800, 2, 2, expected, expected
        ));
        assert!(!borrowed_guardian_identity_matches(
            71, 72, 800, 800, 2, 2, expected, expected
        ));
        assert!(!borrowed_guardian_identity_matches(
            71, 71, 800, 801, 2, 2, expected, expected
        ));
        assert!(!borrowed_guardian_identity_matches(
            71, 71, 800, 800, 2, 3, expected, expected
        ));
        assert!(!borrowed_guardian_identity_matches(
            71,
            71,
            800,
            800,
            2,
            2,
            expected,
            Path::new("C:/Users/alice/Signal.exe")
        ));
    }

    #[test]
    fn existing_session_rejects_ambiguity_and_snapshot_pid_changes() {
        assert_eq!(existing_candidate_count(1), Ok(()));
        assert_eq!(
            existing_candidate_count(0),
            Err(NativeWindowHostReason::ExistingSessionUnavailable)
        );
        assert_eq!(
            existing_candidate_count(2),
            Err(NativeWindowHostReason::ExistingSessionAmbiguous)
        );
        assert!(borrowed_snapshot_pid_matches(51, 51));
        assert!(!borrowed_snapshot_pid_matches(51, 52));
    }

    #[test]
    fn existing_telegram_selects_one_main_window_behind_owned_qt_frames_only() {
        // Candidate 2 is the sole main window; every other candidate is one
        // of its verified thin owned Qt frame windows.
        assert_eq!(
            existing_primary_candidate_index(NativeAppId::Telegram, 5, |target, candidate| {
                target == 2 && candidate != 2
            }),
            Ok(2)
        );
        // A second real window or dialog remains ambiguous and fail-closed.
        assert_eq!(
            existing_primary_candidate_index(NativeAppId::Telegram, 6, |target, candidate| {
                target == 2 && candidate != 2 && candidate != 5
            }),
            Err(NativeWindowHostReason::ExistingSessionAmbiguous)
        );
        // Discord never receives Telegram's Qt-frame exception.
        assert_eq!(
            existing_primary_candidate_index(NativeAppId::Discord, 2, |_, _| true),
            Err(NativeWindowHostReason::ExistingSessionAmbiguous)
        );
        assert_eq!(
            existing_primary_candidate_index(NativeAppId::Telegram, 0, |_, _| false),
            Err(NativeWindowHostReason::ExistingSessionUnavailable)
        );
    }

    #[test]
    fn native_discord_context_is_owner_scoped_and_requires_attached_discord() {
        let owner = "owner-00112233445566778899aabbccddeeff0011223344556677";
        assert_eq!(
            native_discord_account_id(owner).as_deref(),
            Some("native-discord-00112233445566778899aabbccddeeff0011223344556677")
        );
        assert!(native_discord_account_id("owner-not-hex").is_none());
        assert!(native_context_matches(
            true,
            NativeAppId::Discord,
            owner,
            owner
        ));
        assert!(!native_context_matches(
            false,
            NativeAppId::Discord,
            owner,
            owner
        ));
        assert!(!native_context_matches(
            true,
            NativeAppId::Telegram,
            owner,
            owner
        ));
        assert!(!native_context_matches(
            true,
            NativeAppId::Discord,
            owner,
            "owner-ffeeddccbbaa99887766554433221100ffeeddccbbaa9988",
        ));
    }

    #[test]
    fn borrowed_presentation_requires_visible_restored_exact_bounds() {
        let expected = [100, 198, 1380, 900];
        assert_eq!(borrowed_presentation_attempt_limit(NativeAppId::Signal), 7);
        assert_eq!(
            borrowed_presentation_attempt_limit(NativeAppId::Telegram),
            3
        );
        assert_eq!(borrowed_presentation_attempt_limit(NativeAppId::Discord), 3);
        assert_eq!(
            borrowed_presentation_attempt_limit(NativeAppId::Whatsapp),
            7
        );
        assert!(borrowed_presentation_matches(
            true, false, expected, expected
        ));
        assert!(!borrowed_presentation_matches(
            false, false, expected, expected
        ));
        assert!(!borrowed_presentation_matches(
            true, true, expected, expected
        ));
        assert!(!borrowed_presentation_matches(
            true,
            false,
            expected,
            [101, 198, 1381, 900],
        ));
        assert!(borrowed_style_is_preserved(
            0x16cf0000, 0x00040100, 0x16cf0000, 0x00040100,
        ));
        assert!(!borrowed_style_is_preserved(
            0x16cf0000, 0x00040100, 0x96cf0000, 0x00000180,
        ));
        assert!(borrowed_focus_state_valid(true, false));
        assert!(!borrowed_focus_state_valid(false, false));
        assert!(!borrowed_focus_state_valid(true, true));
        assert_eq!(
            borrowed_rect_choice(None, true, Some(expected)),
            Some(expected)
        );
        assert_eq!(
            borrowed_rect_choice(Some([0, 0, 0, 0]), true, Some(expected)),
            Some(expected)
        );
        assert_eq!(borrowed_rect_choice(None, false, Some(expected)), None);
        assert_eq!(
            borrowed_rect_choice(Some(expected), false, Some([1, 2, 3, 4])),
            Some(expected)
        );
    }

    #[test]
    fn borrowed_control_shield_covers_outer_top_right_without_touching_app_styles() {
        assert_eq!(
            borrowed_control_shield_rect([240, 168], [1440, 802]),
            Some([1512, 168, 1680, 212])
        );
        assert_eq!(
            borrowed_control_shield_rect([-1200, 40], [900, 700]),
            Some([-468, 40, -300, 84])
        );
        assert_eq!(
            borrowed_control_shield_rect([10, 20], [80, 30]),
            Some([10, 20, 90, 50])
        );
        assert_eq!(borrowed_control_shield_rect([0, 0], [0, 100]), None);
        assert_eq!(borrowed_control_shield_rect([0, 0], [100, -1]), None);
    }

    #[test]
    fn borrowed_control_shield_accepts_only_uniform_dark_or_light_titlebar_samples() {
        assert_eq!(
            borrowed_control_shield_color(&[
                [31, 32, 36],
                [32, 33, 37],
                [30, 32, 36],
                [33, 34, 38],
            ]),
            [31, 32, 36]
        );
        assert_eq!(
            borrowed_control_shield_color(&[
                [238, 239, 241],
                [240, 241, 243],
                [239, 240, 242],
                [241, 242, 244],
            ]),
            [239, 240, 242]
        );
        // A gradient, accent, avatar, or other varied sample is never copied.
        assert_eq!(
            borrowed_control_shield_color(&[
                [20, 20, 22],
                [80, 42, 120],
                [12, 90, 44],
                [70, 18, 16],
            ]),
            [35, 36, 40]
        );
        assert_eq!(
            borrowed_control_shield_color(&[[220, 220, 220]; 3]),
            [35, 36, 40]
        );
    }

    #[test]
    fn telegram_ignores_only_thin_owned_captionless_qt_frame_decorations() {
        let target = [560, 196, 1360, 820];
        let frames = [
            [550, 186, 560, 830],
            [560, 186, 1360, 196],
            [1360, 186, 1370, 830],
            [560, 820, 1360, 830],
        ];
        for frame in frames {
            assert!(telegram_frame_decoration_matches(
                true, true, true, false, false, false, target, frame,
            ));
        }

        let modal = [720, 300, 1200, 700];
        assert!(!telegram_frame_decoration_matches(
            true, true, true, false, true, true, target, modal,
        ));
        assert!(!telegram_frame_decoration_matches(
            true,
            true,
            true,
            false,
            false,
            false,
            target,
            [520, 156, 560, 860],
        ));
        assert!(!telegram_frame_decoration_matches(
            true, false, true, false, false, false, target, frames[0],
        ));
        assert!(!telegram_frame_decoration_matches(
            false, true, true, false, false, false, target, frames[0],
        ));
        assert!(!telegram_frame_decoration_matches(
            true,
            true,
            true,
            false,
            false,
            false,
            target,
            [551, 186, 561, 830],
        ));
        assert!(!telegram_frame_decoration_matches(
            true, true, true, false, false, true, target, frames[0],
        ));
    }

    #[test]
    fn probe_specs_are_fixed_per_allowlisted_app() {
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Discord),
            FixedSecondaryLaunch::DiscordDedicatedChannel
        );
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Telegram),
            FixedSecondaryLaunch::TelegramManyWorkdir
        );
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Signal),
            FixedSecondaryLaunch::SignalUserDataDir
        );
        assert_eq!(
            fixed_secondary_launch(NativeAppId::Whatsapp),
            FixedSecondaryLaunch::Unsupported
        );
    }

    #[test]
    fn discord_channel_manifests_are_fixed_official_channels() {
        assert_eq!(DISCORD_CHANNELS.len(), 3);
        assert_eq!(
            DISCORD_CHANNELS
                .iter()
                .map(|channel| (
                    channel.channel,
                    channel.install_directory,
                    channel.executable_name,
                    channel.data_directory,
                    channel.package_id,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    DiscordChannel::Stable,
                    "Discord",
                    "Discord.exe",
                    "discord",
                    "Discord.Discord",
                ),
                (
                    DiscordChannel::Ptb,
                    "DiscordPTB",
                    "DiscordPTB.exe",
                    "discordptb",
                    "Discord.Discord.PTB",
                ),
                (
                    DiscordChannel::Canary,
                    "DiscordCanary",
                    "DiscordCanary.exe",
                    "discordcanary",
                    "Discord.Discord.Canary",
                ),
            ]
        );
    }

    #[test]
    fn dedicated_discord_host_uses_only_the_official_ptb_channel() {
        assert_eq!(
            dedicated_discord_channels()
                .map(|channel| (
                    channel.channel,
                    channel.install_directory,
                    channel.executable_name,
                    channel.data_directory,
                    channel.package_id,
                ))
                .collect::<Vec<_>>(),
            vec![(
                DiscordChannel::Ptb,
                "DiscordPTB",
                "DiscordPTB.exe",
                "discordptb",
                "Discord.Discord.PTB",
            )]
        );
    }

    #[test]
    fn existing_discord_session_considers_every_fixed_official_channel() {
        let paths = existing_discord_channel_executables(|channel| match channel.channel {
            DiscordChannel::Stable => None,
            DiscordChannel::Ptb => Some(PathBuf::from("C:/DiscordPTB/DiscordPTB.exe")),
            DiscordChannel::Canary => Some(PathBuf::from("C:/DiscordCanary/DiscordCanary.exe")),
        });
        assert_eq!(
            paths,
            vec![
                PathBuf::from("C:/DiscordPTB/DiscordPTB.exe"),
                PathBuf::from("C:/DiscordCanary/DiscordCanary.exe"),
            ]
        );
        assert_eq!(
            existing_primary_candidate_index(NativeAppId::Discord, paths.len(), |_, _| false),
            Err(NativeWindowHostReason::ExistingSessionAmbiguous)
        );
    }

    #[test]
    fn current_discord_prefers_stable_and_falls_back_in_fixed_channel_order() {
        let stable = preferred_existing_discord_channel_executable(|channel| {
            Some(PathBuf::from(format!(
                "C:/{}/{}",
                channel.install_directory, channel.executable_name
            )))
        });
        assert_eq!(stable, Some(PathBuf::from("C:/Discord/Discord.exe")));

        let ptb = preferred_existing_discord_channel_executable(|channel| match channel.channel {
            DiscordChannel::Stable => None,
            DiscordChannel::Ptb => Some(PathBuf::from("C:/DiscordPTB/DiscordPTB.exe")),
            DiscordChannel::Canary => Some(PathBuf::from("C:/DiscordCanary/DiscordCanary.exe")),
        });
        assert_eq!(ptb, Some(PathBuf::from("C:/DiscordPTB/DiscordPTB.exe")));
    }

    #[test]
    fn discord_accessibility_mode_stays_complete_for_the_process_lifetime() {
        assert_eq!(
            DISCORD_ACCESSIBILITY_ARGUMENT,
            "--force-renderer-accessibility=complete"
        );
        assert_ne!(
            DISCORD_ACCESSIBILITY_ARGUMENT,
            "--force-renderer-accessibility"
        );
    }

    #[test]
    fn fresh_stable_channel_claim_persists_for_same_owner() {
        let (base, osl, roaming) = test_roots("claim");
        let stable = &DISCORD_CHANNELS[0];
        let data_root = claim_discord_channel(&osl, &roaming, "owner-a", stable).unwrap();
        assert_eq!(data_root, roaming.join("discord"));
        std::fs::create_dir_all(&data_root).unwrap();
        std::fs::write(data_root.join("metadata-only-marker"), b"occupied").unwrap();

        assert_eq!(
            claim_discord_channel(&osl, &roaming, "owner-a", stable),
            Ok(data_root)
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn populated_unowned_discord_channel_fails_closed() {
        let (base, osl, roaming) = test_roots("populated");
        let stable = &DISCORD_CHANNELS[0];
        let data_root = roaming.join(stable.data_directory);
        std::fs::create_dir_all(&data_root).unwrap();
        std::fs::write(data_root.join("existing-profile-marker"), b"occupied").unwrap();

        assert_eq!(
            claim_discord_channel(&osl, &roaming, "owner-a", stable),
            Err(NativeWindowHostReason::ChannelNotOwned)
        );
        assert!(!osl.join(DISCORD_CLAIM_NAMESPACE).exists());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn discord_claim_cannot_move_between_osl_owners() {
        let (base, osl, roaming) = test_roots("owners");
        let stable = &DISCORD_CHANNELS[0];
        let data_root = claim_discord_channel(&osl, &roaming, "owner-a", stable).unwrap();
        assert_eq!(
            claim_discord_channel(&osl, &roaming, "owner-b", stable),
            Err(NativeWindowHostReason::ChannelNotOwned)
        );
        std::fs::create_dir_all(&data_root).unwrap();
        std::fs::write(data_root.join("metadata-only-marker"), b"occupied").unwrap();

        assert_eq!(
            claim_discord_channel(&osl, &roaming, "owner-b", stable),
            Err(NativeWindowHostReason::ChannelNotOwned)
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn malformed_discord_claim_fails_closed() {
        let (base, osl, roaming) = test_roots("malformed");
        let stable = &DISCORD_CHANNELS[0];
        let claim = osl.join(discord_claim_relative_path("owner-a", stable).unwrap());
        std::fs::create_dir_all(claim.parent().unwrap()).unwrap();
        std::fs::write(&claim, b"not-an-osl-claim\n").unwrap();

        assert_eq!(
            claim_discord_channel(&osl, &roaming, "owner-a", stable),
            Err(NativeWindowHostReason::ChannelNotOwned)
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn off_windows_host_actions_are_explicitly_unsupported() {
        let state = NativeWindowHostState::default();
        for result in [
            state.host(
                NativeAppId::Discord,
                Path::new("/trusted/osl-data"),
                "owner-a",
                1,
            ),
            state.resize(1),
            state.focus(),
            state.detach(),
            state.terminate(),
        ] {
            assert_eq!(result.status, NativeWindowHostStatus::Unsupported);
            assert_eq!(result.reason, NativeWindowHostReason::PlatformUnsupported);
            assert_eq!(result.mode, "none");
        }
    }
}
