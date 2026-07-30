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
#[cfg(any(target_os = "windows", test))]
use std::time::Duration;

/// Pixels of OSL chrome above a borrowed native window. This MUST equal the
/// rendered height of osl-hub-ui's `.desktop-top-row` (its `--chrome-row-height`
/// custom property, which also drives `.workspace-header`): the borrowed window
/// is placed directly below the chrome, so any surplus here shows up as a dead
/// band of empty page between the header and the borrowed window. It was 98
/// (a stale `44 + 54`) while a separate 44px titlebar strip existed above the
/// header; the controls are now docked into the header row itself, so the
/// reserve is just that one row.
#[cfg(all(target_os = "windows", not(feature = "discord-qa-shell")))]
const TRUSTED_VERTICAL_RESERVE: i32 = 54;
/// The Discord QA shell compacts `.workspace-header` to 48px
/// (`.discord-qa-shell` in styles.css), so the reserve tracks it.
#[cfg(all(target_os = "windows", feature = "discord-qa-shell"))]
const TRUSTED_VERTICAL_RESERVE: i32 = 48;
#[cfg(any(target_os = "windows", test))]
const PROFILE_NAMESPACE: &str = "native-window-profiles-v1";
#[cfg(any(target_os = "windows", test))]
const NATIVE_DISCORD_ACCOUNT_PREFIX: &str = "native-discord-";
const DISCORD_ACCESSIBILITY_ARGUMENT: &str = "--force-renderer-accessibility=complete";
const DISCORD_UIA_PROVIDER_ARGUMENT: &str = "--enable-features=UiaProvider";
/// Discord's own "create and show the main window without taking activation"
/// switch. In Discord's core it resolves to
/// `setWindowVisible(true, false, /*inactive*/ true)` -> `mainWindow.showInactive()`.
///
/// Every OSL relaunch of Discord passes it, because the window OSL is about to
/// adopt should never steal the operator's focus on its way to being adopted.
/// It does not suppress the window: `showInactive` still shows it, so the
/// presence probe (which requires `IsWindowVisible`) still finds it, and the
/// concealed-adoption path still gets its single reveal.
#[cfg(any(target_os = "windows", test))]
const DISCORD_START_INACTIVE_ARGUMENT: &str = "--start-inactive";
#[cfg(any(target_os = "windows", test))]
fn existing_session_launch_arguments(id: NativeAppId) -> &'static [&'static str] {
    if id == NativeAppId::Discord {
        &[
            DISCORD_ACCESSIBILITY_ARGUMENT,
            DISCORD_UIA_PROVIDER_ARGUMENT,
            DISCORD_START_INACTIVE_ARGUMENT,
        ]
    } else {
        &[]
    }
}
#[cfg(any(target_os = "windows", test))]
const DISCORD_PRIMARY_WINDOW_CLASS: &str = "Chrome_WidgetWin_1";
#[cfg(any(target_os = "windows", test))]
const SIGNAL_PRIMARY_WINDOW_CLASS: &str =
    adapter_profile::SIGNAL_DESKTOP_NATIVE_PRIMARY_WINDOW_CLASS;
#[cfg(any(target_os = "windows", test))]
const SIGNAL_PRIMARY_WINDOW_TITLE: &str = adapter_profile::SIGNAL_DESKTOP_NATIVE_WINDOW_TITLE;
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
    TakeOverExisting,
}

#[cfg(any(target_os = "windows", test))]
fn cold_host_action(
    id: NativeAppId,
    mode: DiscordSessionMode,
    takeover: DiscordTakeover,
) -> ColdHostAction {
    match mode {
        DiscordSessionMode::Dedicated => ColdHostAction::LaunchDedicated,
        DiscordSessionMode::ExistingSession => {
            // Defence in depth. `host` has already refused an unsupported
            // takeover outright with `TakeoverNotPermitted`; if that gate is ever
            // bypassed, the fall-through here is the non-destructive one.
            if takeover == DiscordTakeover::QuitAndRelaunch && takeover_supported(id, mode) {
                ColdHostAction::TakeOverExisting
            } else {
                ColdHostAction::ClaimExisting
            }
        }
    }
}

/// Whether OSL has a verified quit-and-relaunch contract for this client.
///
/// Deliberately narrow. Discord's argv vocabulary and single-instance behaviour
/// are established from its shipped code, and `ExistingSession` is the only mode
/// with anything to take over -- `Dedicated` launches into an OSL-owned profile
/// and never touches the operator's client at all.
#[cfg(any(target_os = "windows", test))]
fn takeover_supported(id: NativeAppId, mode: DiscordSessionMode) -> bool {
    id == NativeAppId::Discord && mode == DiscordSessionMode::ExistingSession
}

/// What happened to the client OSL asked to quit.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum TakeoverQuitOutcome {
    /// Nothing was running. There was never anything to consent to, and OSL is
    /// simply the thing that starts the client -- which still makes the window
    /// [`HostWindowOwnership::Spawned`].
    NothingToQuit,
    /// The client exited on its own after the posted `WM_CLOSE`. The relaunch
    /// may proceed.
    Exited,
    /// The client is still running after the budget. OSL never escalates to
    /// `TerminateProcess`, so the takeover is abandoned.
    StillRunning,
}

#[cfg(any(target_os = "windows", test))]
fn takeover_quit_outcome(client_was_running: bool, process_exited: bool) -> TakeoverQuitOutcome {
    match (client_was_running, process_exited) {
        (false, _) => TakeoverQuitOutcome::NothingToQuit,
        (true, true) => TakeoverQuitOutcome::Exited,
        (true, false) => TakeoverQuitOutcome::StillRunning,
    }
}

/// Whether the takeover may go on to relaunch the client.
///
/// Only the process actually being gone counts, which is deliberately stricter
/// than [`harnessed_close_landed`]. A merely *hidden* window means the client is
/// still running, and a running Discord answers OSL's relaunch through its own
/// single-instance handler -- which navigates and focuses the surviving window
/// instead of creating a new one. Relaunching against a live instance would
/// therefore hand OSL back the same window it just hid, with a focus steal.
#[cfg(any(target_os = "windows", test))]
fn takeover_may_relaunch(outcome: TakeoverQuitOutcome) -> bool {
    matches!(
        outcome,
        TakeoverQuitOutcome::NothingToQuit | TakeoverQuitOutcome::Exited
    )
}

/// Longest OSL waits for a consented quit to actually end the client process.
///
/// Bounded on purpose: expiring here is a normal outcome (Discord's own setting
/// can make close mean "stay in the tray"), and it costs only the takeover, not
/// the session -- the caller falls back to borrowing.
#[cfg(any(target_os = "windows", test))]
const TAKEOVER_QUIT_BUDGET: Duration = Duration::from_secs(6);

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

/// Whether `GWL_STYLE` still holds everything the adoption promised not to
/// touch.
///
/// The adoption's own conceal (`conceal_for_adoption`) clears `WS_VISIBLE` for
/// exactly as long as the owner link and task ex-style are being applied, and
/// the presentation immediately after puts it straight back. That single bit is
/// therefore excluded while the window is deliberately hidden -- without this
/// the verification below would read its own conceal as a foreign style change
/// and refuse every adoption. Every other bit must still be byte-identical to
/// what was captured, and when nothing was concealed the comparison is exact.
#[cfg(any(target_os = "windows", test))]
fn borrowed_concealed_style_is_preserved(
    original_style: isize,
    current_style: isize,
    visible_style: isize,
    concealed: bool,
) -> bool {
    if concealed {
        (original_style & !visible_style) == (current_style & !visible_style)
    } else {
        original_style == current_style
    }
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

/// Whether a captured recovery snapshot may be applied back to the live
/// window. Deliberately **not** a function of `GWL_STYLE`: this project's own
/// tether mirrors the borrowed window's iconic state onto the host
/// (`ShowWindow(..., SW_MINIMIZE)` / `SW_RESTORE`), which flips `WS_MINIMIZE`
/// in `GWL_STYLE` for entirely legitimate reasons unrelated to identity. A
/// gate that required that bit to still equal the value captured at claim
/// time silently skipped the *entire* restore -- owner and ex-style both --
/// on an ordinary graceful close whenever the window had been minimized and
/// restored even once during the session. Iconic/maximized presentation is
/// instead put back correctly by `restore_borrowed_window`, which replays
/// the captured `WINDOWPLACEMENT` (including `showCmd`) through the
/// dedicated placement API rather than by poking the raw style bit.
/// Identity is fully proven by the caller before this is consulted
/// (executable trust, PID, creation time, session, and expected path); this
/// gate only decides whether the *owner* is in a state the restore can
/// safely act on.
#[cfg(any(target_os = "windows", test))]
fn borrowed_recovery_restore_permitted(process_id_matches: bool, owner_restorable: bool) -> bool {
    process_id_matches && owner_restorable
}

const GUARDIAN_RESTORE_EX_STYLE_APPLIED: &str = "guardian_restore_ex_style_applied";
const GUARDIAN_RESTORE_EX_STYLE_ALREADY_NORMAL: &str = "guardian_restore_ex_style_already_normal";
const GUARDIAN_RESTORE_EX_STYLE_FAILED: &str = "guardian_restore_ex_style_failed";

/// Classify one recovery-snapshot restore attempt for the `ex_style` bits
/// specifically (`WS_EX_APPWINDOW` / `WS_EX_TOOLWINDOW` / anything else this
/// project may have changed while borrowing), independent of whether the
/// unrelated placement/style fields also verified. Always driven by the
/// *captured* `ex_style` value the caller passes in, never by a hardcoded
/// constant, so a window that legitimately started with unusual extended
/// styles is restored to exactly what it had, not to some assumed default.
#[cfg(any(target_os = "windows", test))]
fn guardian_restore_ex_style_outcome(
    restore_permitted: bool,
    ex_style_before: isize,
    ex_style_after: isize,
    captured_ex_style: isize,
) -> &'static str {
    if !restore_permitted || ex_style_after != captured_ex_style {
        GUARDIAN_RESTORE_EX_STYLE_FAILED
    } else if ex_style_before == captured_ex_style {
        GUARDIAN_RESTORE_EX_STYLE_ALREADY_NORMAL
    } else {
        GUARDIAN_RESTORE_EX_STYLE_APPLIED
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedMutationStage {
    Captured,
    GuardianArmed,
    /// The borrowed window has been taken off screen so the owner link and the
    /// task ex-style below are applied to a window nobody is looking at. The
    /// shell only decides taskbar membership when a window is *shown*, so this
    /// is also the only ordering in which those two mutations can actually take
    /// the window off the taskbar rather than merely reading as if they had.
    Concealed,
    OwnerLinked,
    TaskStyleApplied,
}

/// Legal adoption orderings.
///
/// `Concealed` is optional, not skippable-in-the-middle: a window that could
/// not be hidden is still adopted (`GuardianArmed -> OwnerLinked` stays legal,
/// and the operator keeps a usable Discord), but nothing may run before the
/// guardian is armed, and the task ex-style may never be applied before the
/// owner link.
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
            BorrowedMutationStage::Concealed
        ) | (
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::OwnerLinked
        ) | (
            BorrowedMutationStage::Concealed,
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
    measured_caption_buttons: [i32; 4],
) -> Option<[i32; 4]> {
    let [width, height] = window_size;
    if width <= 0 || height <= 0 {
        return None;
    }
    let [left, top, right, bottom] = measured_caption_buttons;
    let measured_width = right.checked_sub(left)?;
    let measured_height = bottom.checked_sub(top)?;
    let right_gap = width.checked_sub(right)?;
    if left < 0
        || top < 0
        || measured_width <= 0
        || measured_height <= 0
        || right > width
        || bottom > height
        // Caption controls are a compact top-right cluster. Reject a DWM
        // rectangle that could turn the shield into a broad titlebar overlay.
        || measured_width.checked_mul(2)? >= width
        || measured_height.checked_mul(2)? >= height
        || right_gap > (width / 100).max(1)
        || top > (height / 100).max(1)
        // The three guards below bound the shield in absolute terms rather
        // than relative to the window, which the two `* 2 >= ...` guards
        // above do not: on a large window they still admit a rectangle
        // hundreds of pixels tall or a quarter of the titlebar wide. A real
        // caption cluster is a short strip that is always wider than it is
        // tall (two or more side-by-side buttons), is never taller than a
        // caption even at 400% scaling, and is never more than
        // `CAPTION_CLUSTER_MAX_ASPECT` times as wide as it is tall. Anything
        // else is a measurement of something that is not the button cluster,
        // and covering it would cover unrelated UI.
        || measured_height >= measured_width
        || measured_height > CAPTION_CLUSTER_MAX_HEIGHT
        || measured_width > measured_height.checked_mul(CAPTION_CLUSTER_MAX_ASPECT)?
    {
        return None;
    }
    Some([
        window_origin[0].checked_add(left)?,
        window_origin[1].checked_add(top)?,
        window_origin[0].checked_add(right)?,
        window_origin[1].checked_add(bottom)?,
    ])
}

/// Tallest rectangle that can still be a caption-button cluster, in physical
/// pixels. Discord's own strip is 22px tall at 100% scaling and Windows' native
/// caption is 30-32px; 128 leaves headroom past 400% scaling while still
/// rejecting a titlebar-sized slab outright.
#[cfg(any(target_os = "windows", test))]
const CAPTION_CLUSTER_MAX_HEIGHT: i32 = 128;

/// Widest a caption cluster may be as a multiple of its own height. Three
/// square-ish buttons come out near 3-4x; 8x leaves room for a client that
/// shows more than three controls without admitting a titlebar slab.
#[cfg(any(target_os = "windows", test))]
const CAPTION_CLUSTER_MAX_ASPECT: i32 = 8;

/// A caption-button cluster that was really measured on the borrowed window,
/// stored relative to the window's **top-right corner** rather than as an
/// absolute rectangle.
///
/// That framing is what makes one measurement survive the thing the bug report
/// called "doesn't update": window controls stay flush to the right edge and
/// keep their size when the window is moved or resized, so a cluster recorded
/// as (gap from right edge, offset from top edge, size) stays correct through
/// every move and resize, and only a real scale/layout change invalidates it.
#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MeasuredCaptionButtons {
    /// Window width minus the cluster's right edge.
    right_gap: i32,
    /// Cluster top edge, relative to the window's top edge.
    top: i32,
    width: i32,
    height: i32,
}

/// Re-derive a measured cluster's window-relative rectangle at the window's
/// current size. Validation is deliberately left to
/// [`borrowed_control_shield_rect`], which is the single place the shield's
/// "compact top-right cluster only" contract is enforced.
#[cfg(any(target_os = "windows", test))]
fn measured_caption_button_bounds(
    window_size: [i32; 2],
    measured: MeasuredCaptionButtons,
) -> Option<[i32; 4]> {
    let [width, height] = window_size;
    if width <= 0 || height <= 0 {
        return None;
    }
    let MeasuredCaptionButtons {
        right_gap,
        top,
        width: cluster_width,
        height: cluster_height,
    } = measured;
    if right_gap < 0 || top < 0 || cluster_width <= 0 || cluster_height <= 0 {
        return None;
    }
    let right = width.checked_sub(right_gap)?;
    let left = right.checked_sub(cluster_width)?;
    Some([left, top, right, top.checked_add(cluster_height)?])
}

/// The rectangle the shield should occupy, in screen coordinates, plus the
/// window-relative caption bounds it was derived from (which the colour probe
/// samples beside).
///
/// A real accessibility measurement is preferred and a DWM reconstruction is
/// only the fallback, so the shield stops being a hardcoded guess the moment
/// the borrowed client exposes its own window controls. The fallback is still
/// tried when a measurement exists but does not survive validation, so a stale
/// or implausible measurement can never leave the caption buttons uncovered.
#[cfg(any(target_os = "windows", test))]
fn borrowed_control_shield_target(
    window_origin: [i32; 2],
    window_size: [i32; 2],
    dwm_caption_buttons: [i32; 4],
    measured: Option<MeasuredCaptionButtons>,
) -> Option<([i32; 4], [i32; 4])> {
    let resolve = |caption: [i32; 4]| {
        borrowed_control_shield_rect(window_origin, window_size, caption)
            .map(|screen| (screen, caption))
    };
    measured
        .and_then(|measured| measured_caption_button_bounds(window_size, measured))
        .and_then(resolve)
        .or_else(|| {
            normalized_caption_button_bounds(window_size, dwm_caption_buttons).and_then(resolve)
        })
}

/// MSAA role of a push button (`ROLE_SYSTEM_PUSHBUTTON`). Declared here rather
/// than imported because the pure geometry layer is compiled on non-Windows
/// hosts for its unit tests.
#[cfg(any(target_os = "windows", test))]
const MSAA_ROLE_PUSHBUTTON: u32 = 0x2B;

/// The only part of the borrowed window the caption probe is ever allowed to
/// look at: the titlebar band at the right-hand edge, window-relative.
///
/// This is what keeps the probe close to a point probe rather than a tree walk.
/// A container whose own bounds miss this region cannot contain a caption
/// button, so the probe never descends into it — the message list, the channel
/// sidebar and the member pane are all skipped after a single `accLocation`.
#[cfg(any(target_os = "windows", test))]
fn caption_button_search_region(window_size: [i32; 2], scale_percent: i32) -> Option<[i32; 4]> {
    let [window_width, window_height] = window_size;
    if window_width <= 0 || window_height <= 0 {
        return None;
    }
    let scale = scale_percent.clamp(100, 400);
    let scaled = |logical: i32| (logical * scale / 100).max(1);
    Some([
        (window_width - scaled(CAPTION_RIGHT_ZONE_WIDTH)).max(0),
        0,
        window_width,
        scaled(CAPTION_BAND_HEIGHT).min(window_height),
    ])
}

/// Whether a container is worth descending into. An unreadable rectangle is
/// walked (Chromium reports empty bounds for some structural nodes), anything
/// with a rectangle must overlap the search region.
#[cfg(any(target_os = "windows", test))]
fn caption_button_container_worth_walking(bounds: Option<[i32; 4]>, region: [i32; 4]) -> bool {
    let Some([left, top, right, bottom]) = bounds else {
        return true;
    };
    if right <= left || bottom <= top {
        return true;
    }
    left < region[2] && right > region[0] && top < region[3] && bottom > region[1]
}

/// Whether one accessibility node can be part of the borrowed window's caption
/// cluster. `node` is window-relative and `scale_percent` is the window's DPI
/// expressed as a percentage of 96, so every threshold below is written in
/// logical pixels and scaled once here.
///
/// The role test is the whole point: it is what makes this a measurement of the
/// client's *window controls* rather than of whatever happens to be drawn in
/// the corner. The geometry tests then confine the answer to the titlebar band
/// at the right edge, so even a mis-roled node cannot drag the cluster across
/// the titlebar.
#[cfg(any(target_os = "windows", test))]
fn caption_button_node_accepted(
    role: u32,
    window_size: [i32; 2],
    node: [i32; 4],
    scale_percent: i32,
) -> bool {
    if role != MSAA_ROLE_PUSHBUTTON {
        return false;
    }
    let Some(region) = caption_button_search_region(window_size, scale_percent) else {
        return false;
    };
    let scale = scale_percent.clamp(100, 400);
    let scaled = |logical: i32| (logical * scale / 100).max(1);
    let [left, top, right, bottom] = node;
    let width = right - left;
    let height = bottom - top;
    width > 0
        && height > 0
        // Wholly inside the titlebar band at the right-hand edge.
        && left >= region[0]
        && top >= region[1]
        && right <= region[2]
        && bottom <= region[3]
        // Button-sized, so a full-width titlebar container that happens to be
        // roled as a button is rejected rather than measured.
        && (scaled(12)..=scaled(80)).contains(&width)
        && (scaled(10)..=scaled(64)).contains(&height)
}

/// Height of the titlebar band a caption button must fit inside, in logical
/// pixels. Discord's titlebar is 22 logical px and Windows' is 32; 48 accepts
/// both plus padding without reaching into page content.
#[cfg(any(target_os = "windows", test))]
const CAPTION_BAND_HEIGHT: i32 = 48;

/// Width of the right-hand zone a caption button must start inside, in logical
/// pixels. Three 46px controls is 138; 320 accepts a wider control set without
/// admitting anything from the middle of the titlebar.
#[cfg(any(target_os = "windows", test))]
const CAPTION_RIGHT_ZONE_WIDTH: i32 = 320;

/// Fold accepted nodes into one cluster.
///
/// Only the right-most horizontally contiguous run is taken, and it must reach
/// the window's right edge and contain at least two buttons. That is what stops
/// a single unrelated toolbar button — or a button separated from the controls
/// by a gap — from stretching the shield across the titlebar, which is the
/// "covers other stuff too" half of the bug report.
#[cfg(any(target_os = "windows", test))]
fn caption_button_cluster(
    window_size: [i32; 2],
    nodes: &[[i32; 4]],
    scale_percent: i32,
) -> Option<MeasuredCaptionButtons> {
    let [window_width, window_height] = window_size;
    if window_width <= 0 || window_height <= 0 || nodes.len() < 2 {
        return None;
    }
    let scale = scale_percent.clamp(100, 400);
    let scaled = |logical: i32| (logical * scale / 100).max(1);
    let mut sorted = nodes.to_vec();
    sorted.sort_by_key(|node| node[0]);
    let gap_tolerance = scaled(12);
    // Walk right-to-left, stopping at the first gap wider than one button
    // separator. `run`'s last element is always the left-most member so far.
    let mut run: Vec<[i32; 4]> = Vec::new();
    for node in sorted.iter().rev() {
        if let Some(previous) = run.last() {
            if previous[0] - node[2] > gap_tolerance {
                break;
            }
        }
        run.push(*node);
    }
    if run.len() < 2 {
        return None;
    }
    let left = run.iter().map(|node| node[0]).min()?;
    let top = run.iter().map(|node| node[1]).min()?;
    let right = run.iter().map(|node| node[2]).max()?;
    let bottom = run.iter().map(|node| node[3]).max()?;
    if left < 0 || top < 0 || right > window_width || right < window_width - scaled(12) {
        return None;
    }
    let width = right.checked_sub(left)?;
    let height = bottom.checked_sub(top)?;
    (width > 0 && height > 0).then_some(MeasuredCaptionButtons {
        right_gap: window_width - right,
        top,
        width,
        height,
    })
}

/// Shortest gap between two accessibility probes of the same borrowed window.
/// Even a forced re-probe waits this long, so dragging a window cannot spawn
/// one cross-process walk per frame.
#[cfg(any(target_os = "windows", test))]
const CAPTION_PROBE_MIN_INTERVAL: Duration = Duration::from_millis(500);

/// Idle re-probe cadence. A measured cluster is already re-derived for the
/// current window size on every tick, so this only has to catch a real layout
/// or scale change (per-monitor DPI move, client update, zoom change).
#[cfg(any(target_os = "windows", test))]
const CAPTION_PROBE_IDLE_INTERVAL: Duration = Duration::from_secs(5);

/// Whether the shield worker should start another accessibility probe.
#[cfg(any(target_os = "windows", test))]
fn caption_button_probe_due(
    have_measurement: bool,
    geometry_changed: bool,
    since_last_probe: Option<Duration>,
) -> bool {
    let Some(elapsed) = since_last_probe else {
        return true;
    };
    if elapsed < CAPTION_PROBE_MIN_INTERVAL {
        return false;
    }
    !have_measurement || geometry_changed || elapsed >= CAPTION_PROBE_IDLE_INTERVAL
}

#[cfg(any(target_os = "windows", test))]
fn normalized_caption_button_bounds(
    window_size: [i32; 2],
    measured_caption_buttons: [i32; 4],
) -> Option<[i32; 4]> {
    let [width, height] = window_size;
    let [left, top, right, bottom] = measured_caption_buttons;
    if width <= 0 || height <= 0 {
        return None;
    }
    if right > left {
        return Some(measured_caption_buttons);
    }
    // Discord draws Chromium caption controls itself. On those windows DWM
    // can report the correct top/right edge and caption scale but a zero-width
    // button cluster. Reconstruct only the standard three-button strip from
    // that measured height; never fall back to a broad fixed titlebar slab.
    //
    // This is the *fallback*, reached only when the accessibility probe in
    // `caption_button_cluster` has not (yet) produced a real measurement. On a
    // live borrowed Discord that probe returns nothing — Discord does not role
    // its caption controls as `ROLE_SYSTEM_PUSHBUTTON` — so in practice this
    // path is what ships, and it is worth getting right.
    //
    // The ratio below is now derived from a screen measurement of Discord's
    // real controls (2026-07-25, Discord at 240,168,1680,970, taken with OSL
    // detached so the shield was not covering them): minimize/maximize/close
    // centres at x=1592/1628/1663 on a ~35px pitch, so the cluster spans
    // x=1575..1680 — 105px wide — and the buttons stand ~30px tall from the
    // window top. 110 is that width plus a small margin, which still stops
    // clear of the group separator at x=1567.
    //
    // The previous 138x22-at-30 reconstruction was wrong in both axes and
    // produced exactly the two defects reported against it: 33px too wide, so
    // it reached past the separator over Discord's help/inbox icons ("covers
    // other stuff"), and 8px too short, so it never fully covered the buttons
    // ("doesn't match perfectly"). Widening the height to the full measured
    // caption height also removes the under-coverage that mattered most —
    // an exposed close/minimize button tears the borrowed window out of OSL.
    //
    // Bounded regardless: every rectangle produced here still has to pass
    // `borrowed_control_shield_rect`, whose absolute height/aspect guards make
    // it impossible for this path to grow into a titlebar-wide slab.
    let measured_height = bottom.checked_sub(top)?;
    let right_gap = width.checked_sub(right)?;
    if left != right
        || measured_height < 20
        || measured_height > 60
        || top < 0
        || right_gap < 0
        || right_gap > measured_height
    {
        return None;
    }
    let strip_width = 110i32.checked_mul(measured_height)?.checked_div(30)?;
    let strip_height = measured_height;
    let normalized_left = width.checked_sub(strip_width)?;
    (strip_width > 0 && strip_height > 0).then_some([
        normalized_left,
        top,
        width,
        top.checked_add(strip_height)?,
    ])
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_control_shield_color(samples: &[[u8; 3]]) -> Option<[u8; 3]> {
    if samples.len() < 4 {
        return None;
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
    let low_variance = (0..3).all(|channel| maximum[channel] - minimum[channel] <= 20);
    if low_variance {
        // Discord themes and Nitro accents can legitimately use saturated
        // titlebars. Uniform adjacent native pixels are stronger evidence
        // than a hard-coded dark/light palette, so preserve their exact mean.
        return Some(average);
    }
    None
}

/// Fallback fill colour for the caption-button click-blocker shield, packed
/// as a `COLORREF` (`0x00BBGGRR`). Used until `paint_borrowed_control_shield`
/// has sampled Discord's own native titlebar pixels, and again any time
/// `GWLP_USERDATA` is ever found holding something outside a plain 24-bit
/// `COLORREF`. Black, not white: the shield sits directly over Discord's own
/// chrome, which on this deployment is a pure black theme, so an un-sampled
/// or corrupted shield still blends in rather than flashing the default
/// system white the bug report was about.
#[cfg(any(target_os = "windows", test))]
const BORROWED_CONTROL_SHIELD_DEFAULT_COLOR: u32 = 0x0000_0000;

/// Turn whatever is currently stored in the shield window's `GWLP_USERDATA`
/// into a safe paint colour. Split out as a pure function so the "never
/// white" guarantee is unit-testable without a real window.
#[cfg(any(target_os = "windows", test))]
fn borrowed_control_shield_stored_color(raw: isize) -> u32 {
    if (0..=0x00FF_FFFF).contains(&raw) {
        raw as u32
    } else {
        BORROWED_CONTROL_SHIELD_DEFAULT_COLOR
    }
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_control_shield_position_valid(
    positioned: bool,
    had_verified_paint: bool,
    painted_now: bool,
) -> bool {
    positioned && (had_verified_paint || painted_now)
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

/// What OSL's own shutdown must do with the harnessed window, per adoption
/// mode.
///
/// Deliberately separate from [`NativeWindowHostState::terminate`], which stays
/// an unconditional security teardown (identity switch, stealth, burn) and must
/// not grow a grace period. This decides only the application-exit path.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum HarnessedExitPlan {
    /// [`DiscordSessionMode::ExistingSession`] + [`HostWindowOwnership::Borrowed`]:
    /// the window is the operator's own, running client, opened before OSL
    /// existed. Put its taskbar presence, owner and placement back first, then
    /// *ask* it to close exactly as its own title-bar X would. Nothing about the
    /// process is ever touched, and a refusal is a perfectly good outcome.
    RestoreThenAskToClose,
    /// [`DiscordSessionMode::ExistingSession`] + [`HostWindowOwnership::Spawned`]:
    /// OSL started this client itself, so "it goes away with OSL" is a promise
    /// rather than a courtesy. Same restore-first ordering and same graceful
    /// `WM_CLOSE` -- the process is still never terminated, because it is the
    /// operator's real client holding their real session -- but a close that
    /// does not land keeps the recovery guardian armed to finish it.
    RestoreThenCloseSpawnedClient,
    /// [`DiscordSessionMode::Dedicated`]: OSL launched this client into its own
    /// profile and its own kill-on-job-close job object. Ask it to close
    /// gracefully first so it can flush its state, then fall back to the
    /// job-object teardown that already guaranteed no OSL-spawned client
    /// outlives the hub.
    RestoreThenAskToCloseThenStopOwnedProcess,
}

#[cfg(any(target_os = "windows", test))]
fn harnessed_exit_plan(
    mode: DiscordSessionMode,
    ownership: HostWindowOwnership,
) -> HarnessedExitPlan {
    match (mode, ownership) {
        // A dedicated guest is OSL-spawned by construction and is bounded by its
        // job object, so ownership adds nothing to decide here.
        (DiscordSessionMode::Dedicated, _) => {
            HarnessedExitPlan::RestoreThenAskToCloseThenStopOwnedProcess
        }
        (DiscordSessionMode::ExistingSession, HostWindowOwnership::Borrowed) => {
            HarnessedExitPlan::RestoreThenAskToClose
        }
        (DiscordSessionMode::ExistingSession, HostWindowOwnership::Spawned) => {
            HarnessedExitPlan::RestoreThenCloseSpawnedClient
        }
    }
}

/// Whether a close that does not land is a broken promise under this plan.
///
/// False only for a borrowed window: OSL asked a client it did not start, and
/// "no" is an acceptable answer. True for anything OSL spawned, which is what
/// makes the exit path retain the recovery guardian instead of cancelling it.
#[cfg(any(target_os = "windows", test))]
fn harnessed_exit_requires_close(plan: HarnessedExitPlan) -> bool {
    plan != HarnessedExitPlan::RestoreThenAskToClose
}

/// Whether teardown of this host must close the adopted window itself.
///
/// A dedicated guest is excluded because its job object already guarantees it,
/// and because closing is not what teardown owes it -- termination is.
#[cfg(any(target_os = "windows", test))]
fn teardown_closes_spawned_window(
    mode: DiscordSessionMode,
    ownership: HostWindowOwnership,
) -> bool {
    mode == DiscordSessionMode::ExistingSession && ownership == HostWindowOwnership::Spawned
}

/// What the out-of-process recovery guardian must do when OSL's process dies --
/// including when it dies by crashing, which is the case this whole mechanism
/// exists for.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum GuardianDisposition {
    /// Put the operator's own window back and leave it running.
    Restore,
    /// Put the window back *and then* close it. Restore still runs first, and
    /// unconditionally: if the close is refused, or the client answers it by
    /// living on in its tray, the operator is left an ordinary taskbar-listed
    /// window rather than an owner-linked, tool-styled orphan of a dead OSL.
    RestoreThenClose,
}

#[cfg(any(target_os = "windows", test))]
fn guardian_disposition(ownership: HostWindowOwnership) -> GuardianDisposition {
    match ownership {
        HostWindowOwnership::Borrowed => GuardianDisposition::Restore,
        HostWindowOwnership::Spawned => GuardianDisposition::RestoreThenClose,
    }
}

#[cfg(any(target_os = "windows", test))]
fn guardian_disposition_flag(disposition: GuardianDisposition) -> &'static str {
    match disposition {
        GuardianDisposition::Restore => "restore",
        GuardianDisposition::RestoreThenClose => "restore-then-close",
    }
}

#[cfg(any(target_os = "windows", test))]
fn parse_guardian_disposition(value: &str) -> Option<GuardianDisposition> {
    match value {
        "restore" => Some(GuardianDisposition::Restore),
        "restore-then-close" => Some(GuardianDisposition::RestoreThenClose),
        _ => None,
    }
}

/// Whether the guardian is still permitted to close after its restore attempt.
///
/// The restore result is deliberately *not* a precondition: a guardian that
/// could not put the window back is exactly the guardian whose window most needs
/// to stop existing. The close is still gated on a full identity re-verification
/// at the moment it runs, which is where the real safety lives.
#[cfg(any(target_os = "windows", test))]
fn guardian_closes_after_restore(disposition: GuardianDisposition) -> bool {
    disposition == GuardianDisposition::RestoreThenClose
}

/// Whether this plan may fall back to stopping the hosted process at all.
///
/// Only ever true for the process OSL itself launched. A borrowed client is
/// asked and never forced, so an operator's Discord can prompt about unsaved
/// state, save drafts, or minimize to tray the way it normally would.
#[cfg(any(target_os = "windows", test))]
fn harnessed_exit_plan_stops_owned_process(plan: HarnessedExitPlan) -> bool {
    plan == HarnessedExitPlan::RestoreThenAskToCloseThenStopOwnedProcess
}

/// Whether the harnessed window is gone from the operator's screen.
///
/// Both outcomes count, because both are what "closed together with OSL"
/// means to the operator: the window was destroyed, or the client handled
/// `WM_CLOSE` by hiding itself (Discord's default close behaviour is to
/// minimize to the tray rather than exit). `window_still_ours` is false as
/// soon as the handle stops resolving to the harnessed process id, which
/// covers destruction and handle reuse together.
#[cfg(any(target_os = "windows", test))]
fn harnessed_close_landed(window_still_ours: bool, window_visible: bool) -> bool {
    !window_still_ours || !window_visible
}

/// Whether one exit attempt left the operator unable to reach their window.
///
/// This is the failure this whole path exists to prevent: a window still on
/// screen that is still `WS_EX_TOOLWINDOW` (no taskbar button) and still owned
/// via `GWLP_HWNDPARENT` by an OSL frame that is about to be destroyed, with
/// nothing left that will ever put it back. It is unreachable as long as at
/// least one of the three holds -- the restore landed, the window is gone, or
/// the recovery guardian is still armed to repeat the restore when this
/// process dies.
#[cfg(any(target_os = "windows", test))]
fn harnessed_exit_leaves_unreachable_window(
    restored: bool,
    closed: bool,
    guardian_still_armed: bool,
) -> bool {
    !restored && !closed && !guardian_still_armed
}

/// Longest OSL will wait for the harnessed window to acknowledge `WM_CLOSE`.
///
/// The wait is only diagnostic: the restore has already landed before the
/// close is asked for, so expiring here leaves an ordinary taskbar-listed
/// window rather than anything broken.
#[cfg(any(target_os = "windows", test))]
const HARNESSED_CLOSE_BUDGET: Duration = Duration::from_millis(1_500);
/// Longest OSL will wait for an OSL-launched client to exit by itself before
/// the job-object backstop runs.
#[cfg(any(target_os = "windows", test))]
const HARNESSED_OWNED_EXIT_BUDGET: Duration = Duration::from_millis(1_500);
/// Longest exit will wait for the host slot. A concurrent host or resize
/// operation may legitimately hold it for seconds; shutdown must not inherit
/// that wait.
#[cfg(any(target_os = "windows", test))]
const HARNESSED_EXIT_LOCK_BUDGET: Duration = Duration::from_millis(2_000);
#[cfg(any(target_os = "windows", test))]
const HARNESSED_EXIT_POLL: Duration = Duration::from_millis(25);

/// Total worst case for the native-host half of shutdown. Pinned by test so
/// the individual budgets can never drift past the caller's own bound.
#[cfg(test)]
fn harnessed_exit_worst_case() -> Duration {
    HARNESSED_EXIT_LOCK_BUDGET + HARNESSED_CLOSE_BUDGET + HARNESSED_OWNED_EXIT_BUDGET
}

#[cfg(any(target_os = "windows", test))]
fn deadline_reached(elapsed: Duration, budget: Duration) -> bool {
    elapsed >= budget
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
        NativeAppId::Discord => visible && class_name == DISCORD_PRIMARY_WINDOW_CLASS,
        NativeAppId::Telegram => visible,
    }
}

/// Which clients' identity gate above actually consults the window title.
///
/// `GetWindowTextW` against another process's top-level window is a
/// cross-process `WM_GETTEXT`, so it can stall on a busy or hung app -- not
/// something to pay for every window on the desktop at the presence probe's
/// cadence. Everything but Signal and WhatsApp is decided from class and
/// visibility alone, which are both answered locally by the window manager.
/// `existing_window_identity_is_title_independent` pins that this stays true.
#[cfg(any(target_os = "windows", test))]
fn existing_window_identity_uses_title(id: NativeAppId) -> bool {
    matches!(id, NativeAppId::Signal | NativeAppId::Whatsapp)
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

/// Who is responsible for the adopted window's life.
///
/// Orthogonal to [`DiscordSessionMode`]. An `ExistingSession` window is the
/// operator's own install, profile and account either way -- OSL never copies,
/// reads or writes any of that. What differs is whether the window existed
/// before OSL did:
///
/// * [`Self::Borrowed`] -- the operator opened it. OSL puts it back and leaves
///   it running.
/// * [`Self::Spawned`] -- OSL created it (after a consented quit, or because
///   nothing was running). OSL puts the presentation back *and then closes it*,
///   on clean exit and on a crash alike.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostWindowOwnership {
    Borrowed,
    Spawned,
}

/// What the caller permits OSL to do to a client that is *already running*.
///
/// The destructive value can only be reached by naming it, and naming it is the
/// caller's assertion that the operator has already consented (see
/// [`NativeWindowHostState::takeover_requires_consent`]). Nothing in this module
/// prompts; nothing in this module infers consent from anything else.
#[derive(Debug, Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscordTakeover {
    /// Today's behaviour, and the answer whenever consent was refused or never
    /// asked for: adopt whatever window is already on screen, quit nothing.
    #[default]
    BorrowExisting,
    /// Option A. Gracefully quit the running client, relaunch the same signed
    /// executable -- same install, same profile, same account -- with
    /// `--start-inactive`, and adopt that window instead.
    QuitAndRelaunch,
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
    /// A consented takeover asked the running client to quit, and it was still
    /// running when the budget expired -- typically because it handled the
    /// close by staying in its tray. OSL never escalates past `WM_CLOSE`, so the
    /// takeover is abandoned here and the caller may retry as a plain borrow.
    ExistingSessionQuitRefused,
    /// A takeover was requested for a client or session mode that has no
    /// verified quit/relaunch contract. Fails closed rather than silently
    /// degrading, so a caller bug is visible instead of invisible.
    TakeoverNotPermitted,
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

#[derive(Default)]
pub struct NativeWindowHostState {
    #[cfg(target_os = "windows")]
    inner: Mutex<Option<HostedWindow>>,
    #[cfg(target_os = "windows")]
    next_generation: AtomicU64,
    /// Set while one Discord accessibility operation runs with `inner` released.
    /// See [`DiscordAccessibilityOperationGate`]: this replaces the mutual
    /// exclusion that holding `inner` across the operation used to give, without
    /// ever making a thread wait.
    #[cfg(target_os = "windows")]
    accessibility_operation_in_flight: AtomicBool,
}

impl std::fmt::Debug for NativeWindowHostState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeWindowHostState")
            .field("native_host", &"<redacted>")
            .finish()
    }
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
#[cfg(target_os = "windows")]
struct HostedWindow {
    generation: u64,
    id: NativeAppId,
    mode: DiscordSessionMode,
    /// Whether OSL found this window or created it. Decides teardown: a borrowed
    /// window is restored and left alone, a spawned one is restored and closed.
    ownership: HostWindowOwnership,
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

/// Fixed labels for the conditional-repair decisions. Every value is a compile
/// time string: no rect, title, path, or content is ever named by one.
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_REPAIR_SKIPPED_LABEL: &str = "tether_repair_skipped_already_aligned";
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_REPAIR_GEOMETRY_LABEL: &str = "tether_repair_geometry_corrected";
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_REPAIR_ZORDER_LABEL: &str = "tether_repair_zorder_corrected";
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_REPAIR_VISIBILITY_LABEL: &str = "tether_repair_visibility_corrected";
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_REPAIR_COALESCED_LABEL: &str = "tether_reconcile_coalesced";
/// One completed reconcile pass. Dividing this by the elapsed time the same
/// snapshot reports is the measured passes-per-second of the whole tether.
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_RECONCILE_PASS_LABEL: &str = "tether_reconcile_pass";
/// A pass that had to run the full cross-process identity verification
/// (`OpenProcess` + image name + process times + session id).
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_IDENTITY_VERIFIED_LABEL: &str = "tether_reconcile_identity_verified";
/// A pass that answered identity from the cache after the cheap window-manager
/// pid read agreed, so it opened no process handle at all.
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_IDENTITY_CACHED_LABEL: &str = "tether_reconcile_identity_cached";
/// A pass that forced Discord's compositor to rebuild after the host came back
/// from minimized.
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_RESTORE_REPAINT_LABEL: &str = "tether_reconcile_restore_repaint_applied";
/// A pass that widened the polling cadence because nothing was drifting.
#[cfg(any(target_os = "windows", feature = "discord-qa-shell", test))]
const TETHER_QUIET_PASS_LABEL: &str = "tether_reconcile_pass_quiet";

/// Every conditional-repair decision label, in the fixed order the QA counter
/// snapshot writes them.
#[cfg(any(feature = "discord-qa-shell", test))]
const TETHER_REPAIR_DECISION_LABELS: [&str; 10] = [
    TETHER_REPAIR_SKIPPED_LABEL,
    TETHER_REPAIR_GEOMETRY_LABEL,
    TETHER_REPAIR_ZORDER_LABEL,
    TETHER_REPAIR_VISIBILITY_LABEL,
    TETHER_REPAIR_COALESCED_LABEL,
    TETHER_RECONCILE_PASS_LABEL,
    TETHER_IDENTITY_VERIFIED_LABEL,
    TETHER_IDENTITY_CACHED_LABEL,
    TETHER_RESTORE_REPAINT_LABEL,
    TETHER_QUIET_PASS_LABEL,
];

/// Fixed single-slot host-stage labels for the one-shot restore repaint. These
/// go through `qa_discord_host_stage`, which owns exactly one file slot shared
/// with the `tether_stall_*` / `realign_tether_failed` signal, so a pass may
/// only write one when the decision actually *changes*. Steady state therefore
/// writes `restore_repaint_skipped_not_iconic` once and never again.
#[cfg(any(target_os = "windows", test))]
const RESTORE_REPAINT_APPLIED_STAGE: &str = "restore_repaint_applied";
#[cfg(any(target_os = "windows", test))]
const RESTORE_REPAINT_SKIPPED_STAGE: &str = "restore_repaint_skipped_not_iconic";
#[cfg(any(target_os = "windows", test))]
const RESTORE_REPAINT_DEFERRED_STAGE: &str = "restore_repaint_deferred_host_iconic";
#[cfg(any(target_os = "windows", test))]
const RESTORE_REPAINT_ABANDONED_STAGE: &str = "restore_repaint_abandoned";

/// How long a caller may wait for the tether worker to answer one reconcile.
///
/// `focus`, `host`, and `resize` reach the tether from synchronous
/// `#[tauri::command]` entry points, which run on OSL's own UI thread. A long
/// wait there freezes OSL itself, and their failure is cheap and
/// non-destructive: focus falls back to a local restore/raise pass, and resize
/// keeps the borrowed lease and reports a retryable rejection instead of tearing
/// it down.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_UI_THREAD_BUDGET: Duration = Duration::from_millis(500);

/// The protected-overlay guard reaches the tether from its own dedicated thread,
/// where a refused reconcile tears the whole protected session down. Discord's UI
/// thread is measured to stall for up to 2889 ms inside one composer
/// accessibility scan (865 ms for an MSAA row scan), and a reconcile's
/// cross-process `SetWindowPos` queues behind exactly that work, so a shorter
/// budget converts a slow Discord frame into a hard protection failure. This is
/// still bounded and still below the 5 s Windows hung-window threshold, so a
/// genuinely wedged Discord fails closed inside one user-visible beat.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_GUARD_BUDGET: Duration = Duration::from_millis(3_500);

/// Which thread is waiting on a reconcile. The two callers fail in opposite
/// directions, so they get different budgets.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherCaller {
    /// OSL's own UI thread, where blocking is the worse failure.
    UiThread,
    /// The dedicated protected-overlay guard thread, where timing out is the
    /// worse failure because it tears protection down.
    ProtectionGuard,
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_reconcile_budget(caller: BorrowedTetherCaller) -> Duration {
    match caller {
        BorrowedTetherCaller::UiThread => BORROWED_TETHER_UI_THREAD_BUDGET,
        BorrowedTetherCaller::ProtectionGuard => BORROWED_TETHER_GUARD_BUDGET,
    }
}

/// Bound on the read-only z-order walk. A desktop stacking more windows above
/// the borrowed window than this answers "undecided", which never mutates the
/// stack.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_STACK_WALK_LIMIT: usize = 128;

/// Whether the trusted OSL owner currently sits *above* the borrowed window.
///
/// That is the only stack state this tether has to correct: the borrowed body
/// would otherwise be hidden behind OSL's own background. The composer sibling's
/// order relative to the borrowed window is a different invariant, owned and
/// corrected by the protected-overlay guard, which requires the composer above
/// Discord.
///
/// `Some(true)` and `Some(false)` are both proof: the walk only answers `false`
/// after reaching the top of the chain without meeting the owner. Exhausting the
/// bound answers `None`, which the caller must treat as no drift, because
/// re-asserting a stack that is already correct is exactly what saturates the
/// borrowed window's UI thread.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_owner_is_above_target(
    owner: isize,
    target: isize,
    limit: usize,
    mut previous: impl FnMut(isize) -> isize,
) -> Option<bool> {
    if owner == 0 || target == 0 || owner == target {
        return None;
    }
    let mut cursor = target;
    for _ in 0..limit {
        cursor = previous(cursor);
        if cursor == 0 {
            return Some(false);
        }
        if cursor == owner {
            return Some(true);
        }
    }
    None
}

/// The exact bounded correction one reconcile pass will apply to the borrowed
/// window. Every field is a decision to *write* to a foreign window, so a plan
/// with nothing set is the steady state and must issue no cross-process call at
/// all.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
struct BorrowedTetherRepairPlan {
    /// Move/size the borrowed window back onto the parent-derived target rect.
    geometry: bool,
    /// Raise the borrowed window back above its trusted owner.
    zorder: bool,
    /// Show a borrowed window the desktop currently reports hidden.
    reveal: bool,
}

#[cfg(any(target_os = "windows", test))]
impl BorrowedTetherRepairPlan {
    /// Nothing is wrong, so nothing may be written.
    fn is_noop(self) -> bool {
        !self.geometry && !self.zorder && !self.reveal
    }

    /// Fixed labels naming this decision, so one QA run shows how often the
    /// repair was actually needed versus skipped.
    fn decision_labels(self) -> [Option<&'static str>; 3] {
        if self.is_noop() {
            return [Some(TETHER_REPAIR_SKIPPED_LABEL), None, None];
        }
        [
            self.geometry.then_some(TETHER_REPAIR_GEOMETRY_LABEL),
            self.zorder.then_some(TETHER_REPAIR_ZORDER_LABEL),
            self.reveal.then_some(TETHER_REPAIR_VISIBILITY_LABEL),
        ]
    }
}

/// Decide the bounded repair from cheap read-only facts only.
///
/// The historical predicate forced a repair on every reconcile while either half
/// of the composite held the foreground, which issued a cross-process
/// `SetWindowPos` at the tether cadence forever even when the borrowed frame rect
/// already equalled the parent-derived target rect. Those calls serialize on the
/// borrowed window's UI thread, so they starved exactly the accessibility work
/// the send path needs. Each decision is now taken only against a fact that
/// proves something is wrong.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_repair_plan(
    bounds_match: bool,
    visible: bool,
    composite_is_active: bool,
    owner_is_above_target: Option<bool>,
) -> BorrowedTetherRepairPlan {
    BorrowedTetherRepairPlan {
        geometry: !bounds_match,
        // Only a *proved* inversion of an on-screen composite mutates the stack.
        // `None` means the bounded read could not decide, and an undecidable read
        // must never write.
        zorder: composite_is_active && owner_is_above_target == Some(true),
        reveal: !visible,
    }
}

/// Whether this reconcile pass must touch the borrowed window at all.
///
/// This used to be the unconditional predicate `!bounds_match || !visible ||
/// composite_is_active`, which forced a cross-process `SetWindowPos` on every
/// pass for as long as the composite held the foreground. It now answers only
/// from a plan built out of proved facts, so the steady state issues no
/// cross-process call.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_requires_repair(plan: BorrowedTetherRepairPlan) -> bool {
    !plan.is_noop()
}

/// How long a full cross-process identity verification stays authoritative.
///
/// The verification itself is `OpenProcess` + `QueryFullProcessImageNameW` +
/// `GetProcessTimes` + `ProcessIdToSessionId`. Re-deriving it at the tether
/// cadence was ~60 process opens per second against the exact window whose UI
/// thread the send path needs.
///
/// Caching it is sound because every field it proves is pinned to facts the
/// *cheap* window-manager read already re-checks on every single pass:
/// - The bound HWND is fixed for the worker's lifetime.
/// - `GetWindowThreadProcessId` on that HWND is answered out of the window
///   manager's own table, costs no cross-process call, and is re-read every
///   pass. A destroyed window answers nothing, and a window cannot outlive its
///   process, so process death always fails the cheap check first.
/// - Creation time and image path can therefore only differ if the *same* pid
///   were recycled onto a different process while that pid's original top-level
///   HWND still resolved to it, which the window manager cannot produce.
/// The interval is a belt-and-braces ceiling on that reasoning, not the primary
/// guarantee, so it can be generous without weakening the check.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_IDENTITY_MAX_AGE: Duration = Duration::from_millis(2_000);

/// Whether one pass may answer process identity from the cache or must re-derive
/// it across the process boundary.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherIdentityCheck {
    /// Run the full cross-process verification.
    Verify,
    /// The cached verification still covers exactly this HWND and pid.
    Cached,
}

/// Decide from cheap facts only whether the cached identity verification still
/// applies. `age` is `None` when nothing has ever verified, which always forces
/// a full verification.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_identity_check(
    cached_window: isize,
    cached_process_id: u32,
    observed_window: isize,
    observed_process_id: u32,
    age: Option<Duration>,
    max_age: Duration,
) -> BorrowedTetherIdentityCheck {
    // A degenerate handle or pid is never a proof of anything, so it can never
    // be served from cache.
    if observed_window == 0 || observed_process_id == 0 {
        return BorrowedTetherIdentityCheck::Verify;
    }
    match age {
        Some(age)
            if age < max_age
                && cached_window == observed_window
                && cached_process_id == observed_process_id =>
        {
            BorrowedTetherIdentityCheck::Cached
        }
        _ => BorrowedTetherIdentityCheck::Verify,
    }
}

/// The one-shot compositor rebuild Discord needs after the host has been
/// minimized.
///
/// OSL owns Discord's top-level window through `GWLP_HWNDPARENT`, so minimizing
/// OSL minimizes Discord with it. Coming back, Chromium's compositor surface
/// does not return: `PrintWindow(PW_RENDERFULLCONTENT)` reports a single flat
/// colour (`distinct=1` against a healthy `distinct=201`). A plain
/// `RedrawWindow` is measurably not enough; the surface has to be torn down and
/// rebuilt.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherRestoreRepaint {
    /// The host is minimized right now. Remember it; write nothing.
    Defer,
    /// The host just came back from minimized. Force one rebuild.
    Apply,
    /// The host has not been minimized since the last rebuild landed.
    Skip,
    /// The bounded number of rebuild attempts did not bring the borrowed window
    /// back. Stop trying rather than loop on a foreign window forever.
    Abandon,
}

#[cfg(any(target_os = "windows", test))]
impl BorrowedTetherRestoreRepaint {
    /// The fixed single-slot host-stage label naming this decision.
    fn stage(self) -> &'static str {
        match self {
            Self::Defer => RESTORE_REPAINT_DEFERRED_STAGE,
            Self::Apply => RESTORE_REPAINT_APPLIED_STAGE,
            Self::Skip => RESTORE_REPAINT_SKIPPED_STAGE,
            Self::Abandon => RESTORE_REPAINT_ABANDONED_STAGE,
        }
    }
}

/// How many times one minimize/restore transition may attempt the rebuild before
/// giving up. The attempt is only retried when the borrowed window did not come
/// back out of iconic, which makes the whole path self-healing without ever
/// becoming unbounded.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_RESTORE_REPAINT_LIMIT: u32 = 3;

/// How many distinct colours `borrowed_window_content_is_healthy` requires out
/// of the fixed sample grid before a compositor rebuild counts as verified.
///
/// This is a distinct-*colour-count* threshold, deliberately not a
/// brightness/black-ratio test. The product owner's Discord runs a pure black
/// theme, so a fully healthy, freshly repainted channel sidebar, message
/// list, and member pane legitimately sample as mostly `#000000` — "mostly
/// black" is not evidence of anything broken. What the broken surface
/// actually looks like is *one single flat colour* everywhere (`distinct=1`,
/// measured empirically, against `distinct=201` on a healthy repaint of the
/// same window). Counting distinct colours catches that regardless of theme;
/// counting dark pixels would not, and would false-positive "broken" on every
/// correctly repainted dark-theme frame. Do not reintroduce a brightness test
/// here.
#[cfg(any(target_os = "windows", test))]
const BORROWED_WINDOW_HEALTHY_DISTINCT_COLOURS: usize = 4;

/// Pure threshold check, split out from the GDI capture in
/// `borrowed_window_content_distinct_colours` so the distinct-colour-count
/// contract is unit-testable without a real window.
#[cfg(any(target_os = "windows", test))]
fn borrowed_window_content_is_healthy(distinct_colours: usize) -> bool {
    distinct_colours >= BORROWED_WINDOW_HEALTHY_DISTINCT_COLOURS
}

/// Detect the minimized -> visible transition exactly once.
///
/// `host_was_iconic` is sticky: it is set by every pass that observes an iconic
/// host and cleared only by the pass that both applies the rebuild *and*
/// observes the borrowed window back out of iconic. So a transition fires one
/// rebuild, not one per tick, and a rebuild that did not take is retried up to
/// `limit` times instead of being silently lost.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_restore_repaint_decision(
    host_is_iconic: bool,
    host_was_iconic: bool,
    attempts: u32,
    limit: u32,
) -> BorrowedTetherRestoreRepaint {
    if host_is_iconic {
        return BorrowedTetherRestoreRepaint::Defer;
    }
    if !host_was_iconic {
        return BorrowedTetherRestoreRepaint::Skip;
    }
    if attempts >= limit {
        return BorrowedTetherRestoreRepaint::Abandon;
    }
    BorrowedTetherRestoreRepaint::Apply
}

/// The fast tier, held for the first few passes after anything moves. 16 ms is
/// one 60 Hz frame, which is what a live drag or resize needs.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_ACTIVE_INTERVAL: Duration = Duration::from_millis(16);
/// First widening, once a short burst of passes has found nothing wrong.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_SETTLING_INTERVAL: Duration = Duration::from_millis(48);
/// Second widening.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_IDLE_INTERVAL: Duration = Duration::from_millis(120);
/// Steady state for a composite nothing is touching: ~3 passes per second.
///
/// This is the *unsolicited polling* floor only. Every served request still runs
/// its own fresh pass the moment it arrives, and the protected-overlay guard
/// already calls in at least every 400 ms, so 320 ms never makes the
/// worst-case correction latency worse than the guard's own cadence.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_QUIET_INTERVAL: Duration = Duration::from_millis(320);

/// Passes of proved quiet before each widening. Reaching the slowest tier takes
/// `8*16 + 16*48 + 24*120 = 3776 ms` of a composite nothing is drifting, so a
/// drag or resize that pauses for a beat never falls off the fast tier.
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_SETTLING_AFTER: u32 = 8;
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_IDLE_AFTER: u32 = 24;
#[cfg(any(target_os = "windows", test))]
const BORROWED_TETHER_QUIET_AFTER: u32 = 48;

/// Progressive backoff for the tether's own polling timer.
///
/// The historical cadence ran a full pass every 16 ms while `active_ticks > 0`
/// (re-armed to 8 by every served request and every non-aligned tick) and every
/// 80 ms otherwise. Because the protected-overlay guard calls in unconditionally
/// at least every 400 ms, `active_ticks` was re-armed forever, giving a measured
/// floor of ~27 passes/second and a ceiling of 62.5/second on a composite that a
/// live run proved was already aligned every single time
/// (`tether_repair_skipped_already_aligned=575`, zero corrections).
///
/// Monotonic non-decreasing and capped, so the cadence can only ever widen while
/// nothing is drifting and snaps straight back to `BORROWED_TETHER_ACTIVE_INTERVAL`
/// on the first pass that has to write.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_poll_interval(consecutive_quiet: u32) -> Duration {
    if consecutive_quiet >= BORROWED_TETHER_QUIET_AFTER {
        BORROWED_TETHER_QUIET_INTERVAL
    } else if consecutive_quiet >= BORROWED_TETHER_IDLE_AFTER {
        BORROWED_TETHER_IDLE_INTERVAL
    } else if consecutive_quiet >= BORROWED_TETHER_SETTLING_AFTER {
        BORROWED_TETHER_SETTLING_INTERVAL
    } else {
        BORROWED_TETHER_ACTIVE_INTERVAL
    }
}

/// Whether one finished pass proves the composite is quiet, so the polling timer
/// may widen one step.
///
/// `wrote_to_target` is the decisive half: a pass that issued any cross-process
/// write to the borrowed window found real drift, so the cadence must snap back
/// to fast. A *served request* that finds nothing wrong is deliberately counted
/// as quiet — the request was already answered from its own fresh pass, so the
/// polling timer learns nothing from it except that the composite is settled.
/// Treating every served request as activity is exactly what pinned the old
/// cadence to the guard's 400 ms call-in and produced the measured floor.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_pass_is_quiet(
    observation: BorrowedTetherObservation,
    wrote_to_target: bool,
) -> bool {
    !wrote_to_target
        && matches!(
            observation,
            BorrowedTetherObservation::Aligned | BorrowedTetherObservation::ParentMinimized
        )
}

/// The next quiet-run length after one finished pass.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_next_quiet_run(
    consecutive_quiet: u32,
    observation: BorrowedTetherObservation,
    wrote_to_target: bool,
) -> u32 {
    if borrowed_tether_pass_is_quiet(observation, wrote_to_target) {
        consecutive_quiet.saturating_add(1)
    } else {
        0
    }
}

/// What the worker does with one drained batch of queued reconcile requests.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherBatch {
    /// Run exactly one reconcile pass and answer every waiting caller from it.
    /// `coalesced` counts the queued requests that pass superseded.
    ReconcileOnce { coalesced: usize },
    /// A stop arrived while draining (or nothing was pending): tear down without
    /// another cross-process pass and let every waiting caller observe the closed
    /// channel immediately.
    TearDown,
}

/// Queued reconciles are idempotent requests for the same composite state, so
/// only the newest matters and a backlog must never become a backlog of
/// cross-process passes.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_batch(pending: usize, saw_stop: bool) -> BorrowedTetherBatch {
    if saw_stop || pending == 0 {
        return BorrowedTetherBatch::TearDown;
    }
    BorrowedTetherBatch::ReconcileOnce {
        coalesced: pending - 1,
    }
}

/// Whether a reply belongs to the request that is waiting for it. Every request
/// carries its own sequence number, so a reply an abandoned request left behind
/// can never be read as a later request's answer.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_reply_is_current(request: u64, answered: u64) -> bool {
    request == answered
}

/// Exactly which branch stopped a borrowed-window tether reconcile from
/// reporting an aligned composite. These are diagnostic-only facts about window
/// state; no title, path, draft, or message content is represented.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
enum BorrowedTetherStall {
    /// The tether worker already failed closed and stored `healthy = false`.
    WorkerUnhealthy,
    /// The worker is healthy but is bound to a different host generation.
    GenerationMismatch,
    /// The worker thread is gone, so the reconcile request cannot be delivered.
    WorkerGone,
    /// The worker did not answer this reconcile request inside its budget.
    ReconcileTimedOut,
    /// The exact HWND/PID/creation-time/session/path identity no longer matches.
    TargetIdentityChanged,
    /// The trusted OSL parent is missing, foreign, or no longer a root window.
    ParentInvalid,
    /// Re-attaching the already-bound owner was rejected by the window manager.
    OwnerRepairRejected,
    /// The trusted OSL parent is neither minimized nor visible.
    ParentHidden,
    /// The trusted OSL parent client geometry could not be read.
    ParentRectUnavailable,
    /// The bounded repair `SetWindowPos` call was rejected.
    RepairSetWindowPosRejected,
    /// The borrowed window is still hidden after the repair pass.
    TargetHidden,
    /// The borrowed window is still minimized after the repair pass.
    TargetMinimized,
    /// The borrowed window rectangle could not be read after the repair pass.
    TargetRectUnavailable,
    /// The borrowed window rectangle still differs from the expected rectangle.
    TargetRectMismatch,
}

/// One fixed label per stall branch. Fixed `&'static str` values only, so a QA
/// run names the exact branch without ever recording window titles or content.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_stall_stage(stall: BorrowedTetherStall) -> &'static str {
    match stall {
        BorrowedTetherStall::WorkerUnhealthy => "tether_stall_worker_unhealthy",
        BorrowedTetherStall::GenerationMismatch => "tether_stall_generation_mismatch",
        BorrowedTetherStall::WorkerGone => "tether_stall_worker_gone",
        BorrowedTetherStall::ReconcileTimedOut => "tether_stall_reconcile_timed_out",
        BorrowedTetherStall::TargetIdentityChanged => "tether_stall_target_identity_changed",
        BorrowedTetherStall::ParentInvalid => "tether_stall_parent_invalid",
        BorrowedTetherStall::OwnerRepairRejected => "tether_stall_owner_repair_rejected",
        BorrowedTetherStall::ParentHidden => "tether_stall_parent_hidden",
        BorrowedTetherStall::ParentRectUnavailable => "tether_stall_parent_rect_unavailable",
        BorrowedTetherStall::RepairSetWindowPosRejected => {
            "tether_stall_repair_setwindowpos_rejected"
        }
        BorrowedTetherStall::TargetHidden => "tether_stall_target_hidden",
        BorrowedTetherStall::TargetMinimized => "tether_stall_target_minimized",
        BorrowedTetherStall::TargetRectUnavailable => "tether_stall_target_rect_unavailable",
        BorrowedTetherStall::TargetRectMismatch => "tether_stall_target_rect_mismatch",
    }
}

/// Classify the caller-side gate without touching any window. The generation
/// check stays exactly as strict: a worker bound to a different generation is
/// still refused, it is only now named.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_health_stall(
    healthy: bool,
    worker_generation: u64,
    expected_generation: u64,
) -> Option<BorrowedTetherStall> {
    if !healthy {
        return Some(BorrowedTetherStall::WorkerUnhealthy);
    }
    (worker_generation != expected_generation).then_some(BorrowedTetherStall::GenerationMismatch)
}

/// Result of one reconcile request: whether the composite is aligned and, when
/// it is not, exactly which branch refused.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
struct BorrowedTetherReconcileReport {
    aligned: bool,
    stall: Option<BorrowedTetherStall>,
}

#[cfg(any(target_os = "windows", test))]
impl BorrowedTetherReconcileReport {
    fn aligned() -> Self {
        Self {
            aligned: true,
            stall: None,
        }
    }

    fn stalled(stall: BorrowedTetherStall) -> Self {
        Self {
            aligned: false,
            stall: Some(stall),
        }
    }

    /// The QA label for this report, falling back to the historical catch-all so
    /// no path can become silent.
    fn stage(self) -> &'static str {
        self.stall
            .map_or("realign_tether_failed", borrowed_tether_stall_stage)
    }
}

/// Alignment verdict for one observed reconcile. This keeps the historical
/// contract exactly: only `Aligned` and `ParentMinimized` count as aligned, and
/// every other observation reports its own branch label.
#[cfg(any(target_os = "windows", test))]
fn borrowed_tether_reconcile_report(
    observation: BorrowedTetherObservation,
    stall: Option<BorrowedTetherStall>,
) -> BorrowedTetherReconcileReport {
    if matches!(
        observation,
        BorrowedTetherObservation::Aligned | BorrowedTetherObservation::ParentMinimized
    ) {
        return BorrowedTetherReconcileReport::aligned();
    }
    BorrowedTetherReconcileReport {
        aligned: false,
        stall,
    }
}

#[cfg(any(target_os = "windows", test))]
fn borrowed_owner_requires_repair(current_owner: isize, expected_owner: isize) -> bool {
    current_owner != expected_owner
}

#[derive(Clone, Copy)]
#[cfg(target_os = "windows")]
pub(crate) struct NativeDiscordAccessibilityTarget {
    pub(crate) generation: u64,
    pub(crate) window: isize,
    pub(crate) process_id: u32,
}

/// Exactly the facts a Discord accessibility operation is allowed to know,
/// copied out of `NativeWindowHostState::inner` so that lock can be released
/// before the operation runs.
///
/// The lock must not be held across the operation. The adapter makes
/// synchronous cross-process accessibility calls (`AccessibleObjectFromPoint`,
/// `WM_GETOBJECT`) that take no timeout and wait on Discord's UI thread; OSL's
/// UI thread needs this same mutex roughly every second for its host reconcile;
/// and Discord's UI thread is coupled to OSL's because OSL owns Discord's
/// top-level window through `GWLP_HWNDPARENT`. Holding the lock across the call
/// therefore closes a wait cycle that nothing inside the process can break.
///
/// Every field is a plain integer or boolean: `Copy + Send + 'static`, no
/// borrow of host state, no path, title, handle authority, or content.
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg(any(target_os = "windows", test))]
struct LockedDiscordHostFacts {
    generation: u64,
    window: isize,
    window_process_id: u32,
    /// What the host's own trust predicate answered for `window_process_id`,
    /// asked once while the lock was still held and for no other process id.
    target_process_trusted: bool,
}

#[cfg(any(target_os = "windows", test))]
impl std::fmt::Debug for LockedDiscordHostFacts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LockedDiscordHostFacts")
            .field("generation", &self.generation)
            .field("window", &"<redacted-hwnd>")
            .field("window_process_id", &self.window_process_id)
            .field("target_process_trusted", &self.target_process_trusted)
            .finish()
    }
}

#[cfg(any(target_os = "windows", test))]
impl LockedDiscordHostFacts {
    /// Copy the facts out of the locked state, refusing outright unless the
    /// identity they would describe is fully proven. `target_process_trusted`
    /// must already be the host predicate's own answer for `window_process_id`.
    fn copy_from_locked(
        generation: u64,
        window: isize,
        window_process_id: u32,
        target_process_trusted: bool,
    ) -> Option<Self> {
        if window == 0 || window_process_id == 0 || !target_process_trusted {
            return None;
        }
        Some(Self {
            generation,
            window,
            window_process_id,
            target_process_trusted,
        })
    }

    /// A narrowed, owned trust predicate for use with the lock released.
    ///
    /// It admits exactly one process id -- the one the host's own predicate
    /// already approved while the lock was held -- and rejects every other id
    /// including zero. It can therefore never accept a process the borrowed
    /// predicate would have rejected, only fewer. It captures nothing by
    /// reference, so it cannot reach host state at all.
    fn pinned_process_trust(&self) -> impl Fn(u32) -> bool + Send + 'static {
        let trusted = self.target_process_trusted;
        let trusted_process_id = self.window_process_id;
        move |process_id| trusted && process_id != 0 && process_id == trusted_process_id
    }

    /// Whether host state re-read after the operation still describes the exact
    /// same hosted window, owning process and generation these facts were copied
    /// from. A `false` here means the host changed underneath the operation and
    /// its result must be discarded rather than written back or returned.
    fn still_describes(&self, generation: u64, window: isize, window_process_id: u32) -> bool {
        self.generation == generation
            && self.window == window
            && self.window_process_id == window_process_id
    }
}

#[cfg(target_os = "windows")]
impl LockedDiscordHostFacts {
    fn target(&self) -> NativeDiscordAccessibilityTarget {
        NativeDiscordAccessibilityTarget {
            generation: self.generation,
            window: self.window,
            process_id: self.window_process_id,
        }
    }
}

/// Single-flight gate for cross-process Discord accessibility operations.
///
/// `NativeWindowHostState::inner` used to provide this mutual exclusion by being
/// held across the whole operation, which is exactly the deadlock. This gate is
/// a non-blocking compare-exchange instead: a caller either takes it
/// immediately or fails closed immediately. No thread ever waits on it, so it
/// cannot be an edge of any wait cycle, and it introduces no lock ordering
/// against `inner` beyond "taken strictly before, released strictly after".
#[cfg(target_os = "windows")]
struct DiscordAccessibilityOperationGate<'a> {
    in_flight: &'a AtomicBool,
}

#[cfg(target_os = "windows")]
impl<'a> DiscordAccessibilityOperationGate<'a> {
    fn acquire(state: &'a NativeWindowHostState) -> Option<Self> {
        state
            .accessibility_operation_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self {
                in_flight: &state.accessibility_operation_in_flight,
            })
    }
}

#[cfg(target_os = "windows")]
impl Drop for DiscordAccessibilityOperationGate<'_> {
    fn drop(&mut self) {
        self.in_flight.store(false, Ordering::Release);
    }
}

/// Credential-free presentation facts for the exact signed Discord window
/// already claimed by `NativeWindowHostState`. This contains no title, account,
/// conversation, accessibility, or process data and grants no authority to
/// operate the foreign window.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct NativeDiscordOverlayTarget {
    pub generation: u64,
    /// Exact already-claimed Discord HWND, retained only for native sibling
    /// stacking. It never crosses IPC and grants no discovery authority.
    pub window: isize,
    pub rect: [i32; 4],
    pub foreground: bool,
    pub trusted_parent: isize,
}

impl std::fmt::Debug for NativeDiscordOverlayTarget {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeDiscordOverlayTarget")
            .field("generation", &self.generation)
            .field("window", &"<redacted-hwnd>")
            .field("rect", &self.rect)
            .field("foreground", &self.foreground)
            .field("trusted_parent", &"<redacted-hwnd>")
            .finish()
    }
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

    /// Revalidate the exact already-claimed Discord identity after the QA
    /// overlay takes focus, without asking the borrowed-window tether to
    /// mutate presentation order underneath that overlay.
    #[cfg(all(target_os = "windows", feature = "discord-qa-shell"))]
    pub fn validate_current_discord_overlay_target_identity(
        &self,
        owner_osl_user_id: &str,
        expected: NativeDiscordOverlayTarget,
    ) -> Result<(), String> {
        self.with_current_discord_accessibility_target(
            owner_osl_user_id,
            |confirmed, process_is_trusted| {
                if confirmed.generation != expected.generation
                    || confirmed.window != expected.window
                    || !process_is_trusted(confirmed.process_id)
                {
                    return Err("The trusted native Discord window changed".to_owned());
                }
                Ok(())
            },
        )
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
        self.host_mode_with_takeover(
            id,
            osl_profile_root,
            owner_osl_user_id,
            trusted_parent,
            mode,
            DiscordTakeover::BorrowExisting,
        )
    }

    /// Whether starting an `ExistingSession` host with
    /// [`DiscordTakeover::QuitAndRelaunch`] would have to quit something the
    /// operator is currently using.
    ///
    /// **This is the consent hook.** The UI must call it *before* it offers a
    /// takeover, and must only pass [`DiscordTakeover::QuitAndRelaunch`] to
    /// [`Self::host_mode_with_takeover`] after the operator has explicitly
    /// agreed. When this returns false there is nothing running, so there is
    /// nothing to consent to and no prompt is warranted -- OSL is simply the
    /// thing that starts the client.
    ///
    /// It is a read-only `EnumWindows` presence probe against the same identity
    /// gate the real claim uses: no `OpenProcess`, no signature verification, no
    /// profile access, and nothing is mutated. Safe to call on any thread and at
    /// any cadence.
    pub fn takeover_requires_consent(&self, id: NativeAppId) -> bool {
        #[cfg(target_os = "windows")]
        {
            windows::existing_client_is_running(id)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = id;
            false
        }
    }

    /// Host an app, optionally taking ownership of the operator's running client
    /// instead of borrowing its window.
    ///
    /// `takeover` is a *consent receipt*, not a request to obtain consent.
    /// Nothing below this call prompts. Passing
    /// [`DiscordTakeover::QuitAndRelaunch`] asserts that the operator was asked
    /// (see [`Self::takeover_requires_consent`]) and said yes; if they said no,
    /// or were never asked, the caller passes
    /// [`DiscordTakeover::BorrowExisting`] and gets exactly today's behaviour.
    ///
    /// The takeover never touches the operator's Discord data. It reuses their
    /// account by relaunching the same signed executable from the same install,
    /// which reopens the same profile -- no profile directory is read, written
    /// or copied, and no session token is ever extracted.
    pub fn host_mode_with_takeover(
        &self,
        id: NativeAppId,
        osl_profile_root: &Path,
        owner_osl_user_id: &str,
        trusted_parent: isize,
        mode: DiscordSessionMode,
        takeover: DiscordTakeover,
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
                takeover,
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (
                osl_profile_root,
                owner_osl_user_id,
                trusted_parent,
                mode,
                takeover,
            );
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

    /// Close the harnessed window together with OSL, on the application-exit
    /// path only.
    ///
    /// OSL adopts the window by owner-linking it (`GWLP_HWNDPARENT`) and
    /// taking it off the taskbar (`WS_EX_TOOLWINDOW`), so it reads as part of
    /// OSL rather than as a separate app. Exiting without undoing that would
    /// leave the operator a window with no taskbar button and an owner that no
    /// longer exists, so this always restores *first* and only then asks the
    /// window to close. The close is a posted `WM_CLOSE` and never a process
    /// kill: the client runs its own close handler exactly as it would for its
    /// own title-bar X, and no login, profile or conversation data is touched.
    ///
    /// Every wait inside is bounded (see [`harnessed_exit_worst_case`]), and
    /// every failure path is safe: a restore that does not verify deliberately
    /// leaves the recovery guardian armed, so the window is put back when this
    /// process dies -- including when it dies by crashing.
    ///
    /// Use [`Self::terminate`] instead for security transitions; that path is
    /// unconditional and grants no grace period.
    pub fn shutdown_with_app(&self) -> NativeWindowHostResult {
        #[cfg(target_os = "windows")]
        {
            windows::shutdown_with_app(self)
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

/// Diagnostics-only startup breadcrumb trace, local to this crate.
///
/// `main.rs` defines its own copy of this same append-only writer (it lives in
/// a separate binary-crate compilation unit and cannot be called from here),
/// targeting the same file so both interleave into one trace. TEMPORARY: every
/// call site is marked `// STARTUP-TRACE` for easy removal.
#[cfg(target_os = "windows")]
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
    F: FnMut(&DiscordChannelManifest) -> Vec<PathBuf>,
{
    DISCORD_CHANNELS.iter().flat_map(&mut resolve).collect()
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
        GetLastError, SetLastError, BOOL, FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
    };
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CAPTION_BUTTON_BOUNDS};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush,
        DeleteDC, DeleteObject, EndPaint, FillRect, GetDC, GetPixel, GetWindowDC, MapWindowPoints,
        RedrawWindow, ReleaseDC, SelectObject, HDC, PAINTSTRUCT, RDW_ALLCHILDREN, RDW_ERASE,
        RDW_INVALIDATE, RDW_UPDATENOW,
    };
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetWindowDpiAwarenessContext};
    // The caption-button probe is the only thing in this module that speaks
    // COM/MSAA, and it needs the typed `windows` crate rather than
    // `windows-sys` because `IAccessible` is a real COM interface. Paths are
    // rooted at `::windows` because this module is itself named `windows`.
    use ::windows::core::{Interface, VARIANT};
    use ::windows::Win32::Foundation::HWND as ComHwnd;
    use ::windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, IDispatch, COINIT_MULTITHREADED,
    };
    use ::windows::Win32::UI::Accessibility::{
        AccessibleChildren, AccessibleObjectFromWindow, IAccessible,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_RoamingAppData, SHGetKnownFolderPath, ShellExecuteW,
        KF_FLAG_DEFAULT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        EnumChildWindows, EnumWindows, GetAncestor, GetClassNameW, GetClientRect,
        GetForegroundWindow, GetParent,
        GetWindow, GetWindowDisplayAffinity, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
        GetWindowTextW, GetWindowThreadProcessId, IsChild, IsHungAppWindow, IsIconic,
        IsWindowVisible, PeekMessageW, PostMessageW,
        SendMessageTimeoutW,
        SetForegroundWindow, SetParent, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos,
        ShowWindow, ShowWindowAsync, TranslateMessage, GA_ROOT, GWLP_HWNDPARENT, GWLP_USERDATA,
        GWLP_WNDPROC, GWL_EXSTYLE, GWL_STYLE, GW_HWNDPREV, HWND_TOP, MSG, OBJID_CLIENT, PM_REMOVE,
        PW_RENDERFULLCONTENT, SMTO_ABORTIFHUNG, SMTO_BLOCK, SWP_FRAMECHANGED, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE,
        SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_MINIMIZE, SW_RESTORE, SW_SHOW,
        WDA_EXCLUDEFROMCAPTURE,
        WINDOWPLACEMENT, WM_CLOSE, WM_ERASEBKGND, WM_NULL, WM_PAINT, WS_CAPTION, WS_CHILD,
        WS_EX_APPWINDOW,
        WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
        WS_THICKFRAME, WS_VISIBLE,
    };

    // `PrintWindow` ships in `windows-sys` under the `Storage::Xps` module and
    // therefore behind the `Win32_Storage_Xps` feature, which this crate does
    // not otherwise need. Declare it directly instead of widening the feature
    // set for one function; the signature matches the documented Win32 ABI
    // exactly (`user32.dll`, stdcall/"system").
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(hwnd: HWND, hdc_blt: HDC, flags: u32) -> BOOL;
    }
    const WINDOW_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(8);
    // Cold desktop clients can spend several seconds in updater/bootstrap
    // work before their first real top-level window exists. Keep this below
    // the renderer's 30-second host deadline, but long enough that one click
    // remains sufficient after a Windows or app update.
    const EXISTING_SESSION_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
    // How often the relaunch wait asks the cheap presence probe whether a
    // claimable window exists yet. Every one of these ticks that passes after
    // the relaunched client has shown itself is a tick during which the operator
    // is looking at an unowned, taskbar-listed Discord, so it is deliberately
    // much shorter than the old fixed 100ms claim cadence; the probe is a single
    // `EnumWindows` walk with no cross-process call, so the cost is negligible.
    const EXISTING_SESSION_PRESENCE_POLL: Duration = Duration::from_millis(25);
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
    const TAKEOVER_QUIT_REQUESTED: &str = "takeover_quit_requested";
    const TAKEOVER_QUIT_LANDED: &str = "takeover_quit_landed";
    const TAKEOVER_QUIT_REFUSED: &str = "takeover_quit_refused_client_still_running";
    const TAKEOVER_RELAUNCHED: &str = "takeover_relaunched_start_inactive";
    const TAKEOVER_ADOPT_FAILED: &str = "takeover_relaunched_client_not_adopted";
    const THREAD_SUSPEND_RESUME: u32 = 0x0000_0002;
    const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    const CERTIFIED_TELEGRAM_WINDOWS_BUILD: u32 = 19_045;
    const CERTIFIED_TELEGRAM_SHA256: [u8; 32] = [
        0xd4, 0x67, 0x8b, 0x4c, 0x81, 0x5e, 0x60, 0x7f, 0x27, 0x69, 0x0d, 0x08, 0xab, 0xa4, 0xb0,
        0xa1, 0x9e, 0x3f, 0x22, 0xbd, 0x4e, 0x76, 0x5a, 0x06, 0xa0, 0xcb, 0x36, 0x96, 0xb5, 0xc4,
        0x26, 0xab,
    ];

    fn qa_discord_host_stage(stage: &'static str) {
        #[cfg(feature = "discord-qa-shell")]
        {
            let _ = fs::write(
                std::env::temp_dir().join("osl-discord-qa-host-stage.txt"),
                stage,
            );
        }
        #[cfg(not(feature = "discord-qa-shell"))]
        let _ = stage;
    }

    /// Monotonic counters for the conditional-repair decisions, in the fixed
    /// order of `TETHER_REPAIR_DECISION_LABELS`.
    #[cfg(feature = "discord-qa-shell")]
    static TETHER_REPAIR_DECISION_COUNTS: [AtomicU64; 10] = [
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
        AtomicU64::new(0),
    ];

    /// When the first and the most recent counter snapshots were written. The
    /// span between them is what turns `tether_reconcile_pass` into a
    /// passes-per-second figure.
    #[cfg(feature = "discord-qa-shell")]
    #[derive(Debug, Clone, Copy)]
    struct TetherRepairReportClock {
        first: Instant,
        last: Instant,
    }

    /// When the counter snapshot was last written.
    #[cfg(feature = "discord-qa-shell")]
    static TETHER_REPAIR_LAST_REPORT: Mutex<Option<TetherRepairReportClock>> = Mutex::new(None);

    /// The counters are monotonic, so a slow rewrite is enough to show how often
    /// the repair was needed versus skipped. The tether cadence must never carry
    /// a file write of its own.
    #[cfg(feature = "discord-qa-shell")]
    const TETHER_REPAIR_REPORT_INTERVAL: Duration = Duration::from_millis(250);

    /// Record one conditional-repair decision. This never touches the single-slot
    /// host stage file, so the existing `tether_stall_*` / `realign_tether_failed`
    /// signal keeps its exact meaning. Only fixed labels and counts are written.
    fn qa_tether_repair_decision(label: &'static str, count: u64) {
        #[cfg(feature = "discord-qa-shell")]
        {
            use std::fmt::Write as _;

            if count == 0 {
                return;
            }
            let Some(index) = TETHER_REPAIR_DECISION_LABELS
                .iter()
                .position(|known| *known == label)
            else {
                return;
            };
            TETHER_REPAIR_DECISION_COUNTS[index].fetch_add(count, Ordering::Relaxed);
            // The lock is held for two `Instant` comparisons only. It is never
            // held across the file write below, and never across any
            // cross-process call, so it can never participate in the
            // UI-thread/Discord-UI-thread deadlock this file is careful about.
            let elapsed = match TETHER_REPAIR_LAST_REPORT.lock() {
                Ok(mut clock) => {
                    let now = Instant::now();
                    match *clock {
                        Some(previous)
                            if now.duration_since(previous.last)
                                >= TETHER_REPAIR_REPORT_INTERVAL =>
                        {
                            *clock = Some(TetherRepairReportClock {
                                first: previous.first,
                                last: now,
                            });
                            Some(now.duration_since(previous.first))
                        }
                        Some(_) => None,
                        None => {
                            *clock = Some(TetherRepairReportClock {
                                first: now,
                                last: now,
                            });
                            Some(Duration::ZERO)
                        }
                    }
                }
                Err(_) => None,
            };
            let Some(elapsed) = elapsed else {
                return;
            };
            let mut snapshot = String::new();
            for (name, counter) in TETHER_REPAIR_DECISION_LABELS
                .iter()
                .zip(TETHER_REPAIR_DECISION_COUNTS.iter())
            {
                let _ = writeln!(snapshot, "{name}={}", counter.load(Ordering::Relaxed));
            }
            // Counts and one elapsed span. No rect, handle, title, path, or
            // content is ever written here.
            let _ = writeln!(snapshot, "tether_report_elapsed_ms={}", elapsed.as_millis());
            let _ = fs::write(
                std::env::temp_dir().join("osl-discord-qa-tether-repair.txt"),
                snapshot,
            );
        }
        #[cfg(not(feature = "discord-qa-shell"))]
        {
            let _ = label;
            let _ = count;
        }
    }

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

    pub(super) struct JobHandle(RawHandle);

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

    pub(super) struct ProcessHandle(RawHandle);

    enum BorrowedWindowTetherCommand {
        /// One reconcile request: its sequence number, and its own reply channel.
        /// The worker echoes the sequence number back so an abandoned request can
        /// never hand its answer to a later one.
        Reconcile(u64, mpsc::SyncSender<(u64, BorrowedTetherReconcileReport)>),
        Stop,
    }

    pub(super) struct BorrowedWindowTether {
        commands: mpsc::Sender<BorrowedWindowTetherCommand>,
        worker: Option<JoinHandle<()>>,
        healthy: Arc<AtomicBool>,
        generation: Arc<AtomicU64>,
        /// Monotonic reconcile request sequence.
        requests: AtomicU64,
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
                requests: AtomicU64::new(0),
            })
        }

        fn reconcile(&self, generation: u64, budget: Duration) -> bool {
            self.reconcile_reported(generation, budget).aligned
        }

        /// Same bounded request as `reconcile`, but it also names the exact
        /// branch that refused. No additional window or accessibility work is
        /// performed: the worker already computes this on the existing cadence.
        ///
        /// `budget` comes from `borrowed_tether_reconcile_budget` for the exact
        /// calling thread, because a UI-thread caller and the protected-overlay
        /// guard fail in opposite directions.
        fn reconcile_reported(
            &self,
            generation: u64,
            budget: Duration,
        ) -> BorrowedTetherReconcileReport {
            if let Some(stall) = borrowed_tether_health_stall(
                self.healthy.load(Ordering::Acquire),
                self.generation.load(Ordering::Acquire),
                generation,
            ) {
                return BorrowedTetherReconcileReport::stalled(stall);
            }
            // Each request carries its own sequence number and its own reply
            // channel, so a reply abandoned by a timed-out request can never be
            // read as this request's answer.
            let request = self
                .requests
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
            let (send, receive) = mpsc::sync_channel(1);
            if self
                .commands
                .send(BorrowedWindowTetherCommand::Reconcile(request, send))
                .is_err()
            {
                return BorrowedTetherReconcileReport::stalled(BorrowedTetherStall::WorkerGone);
            }
            let deadline = Instant::now() + budget;
            loop {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    return BorrowedTetherReconcileReport::stalled(
                        BorrowedTetherStall::ReconcileTimedOut,
                    );
                };
                match receive.recv_timeout(remaining) {
                    Ok((answered, report))
                        if borrowed_tether_reply_is_current(request, answered) =>
                    {
                        return report
                    }
                    // Stamped for another request: stale by construction, so it
                    // is discarded rather than answered.
                    Ok(_) => continue,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return BorrowedTetherReconcileReport::stalled(
                            BorrowedTetherStall::WorkerGone,
                        )
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        return BorrowedTetherReconcileReport::stalled(
                            BorrowedTetherStall::ReconcileTimedOut,
                        )
                    }
                }
            }
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

    #[derive(Clone)]
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

    /// The last accessibility measurement of the borrowed window's caption
    /// buttons, shared between the shield worker, the detached probe thread
    /// that produces it, and the UI-thread alignment check that has to agree
    /// with the worker about where the shield belongs.
    ///
    /// The lock is only ever held around a copy of a four-integer struct. No
    /// cross-process call is ever made while holding it, which is the standing
    /// rule for accessibility work in this codebase.
    type CaptionMeasurement = Arc<Mutex<Option<MeasuredCaptionButtons>>>;

    pub(super) struct BorrowedControlShield {
        commands: mpsc::Sender<BorrowedControlShieldCommand>,
        worker: Option<JoinHandle<()>>,
        target: isize,
        shield: isize,
        expected_process_id: u32,
        measurement: CaptionMeasurement,
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
            let measurement: CaptionMeasurement = Arc::new(Mutex::new(None));
            let worker_measurement = Arc::clone(&measurement);
            let worker = thread::spawn(move || unsafe {
                borrowed_control_shield_worker(
                    target_value as HWND,
                    expected_process_id,
                    receiver,
                    ready_send,
                    worker_measurement,
                )
            });
            let Some(shield) = ready_receive
                .recv_timeout(Duration::from_secs(1))
                .ok()
                .flatten()
            else {
                let _ = commands.send(BorrowedControlShieldCommand::Stop);
                let _ = worker.join();
                return None;
            };
            Some(Self {
                commands,
                worker: Some(worker),
                target: target_value,
                shield,
                expected_process_id,
                measurement,
            })
        }

        fn position(&self) -> bool {
            let (send, receive) = mpsc::sync_channel(1);
            let acknowledged = self
                .commands
                .send(BorrowedControlShieldCommand::Position(send))
                .is_ok()
                && receive
                    .recv_timeout(Duration::from_millis(500))
                    .unwrap_or(false);
            acknowledged
                || unsafe {
                    // This fallback runs on the *caller's* thread, which is
                    // OSL's UI thread on most call sites. It therefore reads
                    // the worker's last measurement out of the shared slot
                    // instead of measuring anything itself: no accessibility
                    // call is ever issued from here.
                    borrowed_control_shield_is_aligned(
                        self.target as HWND,
                        self.shield as HWND,
                        self.expected_process_id,
                        self.measurement
                            .lock()
                            .ok()
                            .and_then(|measured| *measured),
                    )
                }
        }

        fn is_healthy(&self) -> bool {
            self.worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
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

    #[derive(Clone)]
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
        /// Restore only (borrowed), or restore and then close (spawned).
        disposition: GuardianDisposition,
    }

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
                .arg(guardian_disposition_flag(snapshot.disposition))
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

        /// Replay the captured snapshot *without* standing the guardian down.
        ///
        /// Used by the application-exit path for a window OSL spawned: the
        /// guardian is the only thing that can still honour "it goes away with
        /// OSL" if the posted close is refused, so it stays armed until the
        /// close has actually landed. Dropping this struct without cancelling
        /// leaves the subprocess running and still waiting on OSL's process
        /// handle, which is precisely the wanted fallback.
        unsafe fn restore_retaining(&self) -> bool {
            restore_guardian_snapshot(&self.snapshot)
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
        if arguments.len() != 24 || arguments.first()?.as_str() != BORROWED_GUARDIAN_MARKER {
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
            disposition: parse_guardian_disposition(arguments.get(23)?)?,
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
            qa_guardian_restore_ex_style_stage(snapshot.id, GUARDIAN_RESTORE_EX_STYLE_FAILED);
            return false;
        };
        let restore_permitted = borrowed_recovery_restore_permitted(
            window_process_id(window) == Some(snapshot.process_id),
            borrowed_owner_is_restorable(
                snapshot.owner,
                snapshot.attached_owner,
                GetWindowLongPtrW(window, GWLP_HWNDPARENT),
            ),
        );
        if !restore_permitted {
            qa_guardian_restore_ex_style_stage(snapshot.id, GUARDIAN_RESTORE_EX_STYLE_FAILED);
            return false;
        }
        let ex_style_before = GetWindowLongPtrW(window, GWL_EXSTYLE);
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
        let ex_style_after = GetWindowLongPtrW(window, GWL_EXSTYLE);
        qa_guardian_restore_ex_style_stage(
            snapshot.id,
            guardian_restore_ex_style_outcome(
                restore_permitted,
                ex_style_before,
                ex_style_after,
                snapshot.ex_style,
            ),
        );
        window_process_id(window) == Some(snapshot.process_id)
            && borrowed_restore_contract_matches(
                snapshot.owner,
                GetWindowLongPtrW(window, GWLP_HWNDPARENT),
                snapshot.style,
                GetWindowLongPtrW(window, GWL_STYLE),
                snapshot.ex_style,
                ex_style_after,
                capture_borrowed_placement(window) == Some(snapshot.placement),
            )
    }

    /// Publish the ex-style restore outcome to the single-slot host-stage
    /// file, but only for Discord: that is the only client this QA signal
    /// exists to protect (the taskbar-button regression), and the other
    /// guardian-eligible apps must not compete for the one file slot.
    fn qa_guardian_restore_ex_style_stage(id: NativeAppId, stage: &'static str) {
        if id == NativeAppId::Discord {
            qa_discord_host_stage(stage);
        }
    }

    /// Mutable state owned exclusively by one tether worker thread.
    ///
    /// This is a plain `&mut` local, not shared state: it is created inside
    /// `borrowed_window_tether_worker` and never escapes it, so it needs no lock
    /// at all. That matters — nothing here can ever be acquired by OSL's UI
    /// thread, which is the invariant that keeps the reconcile path out of the
    /// deadlock `LockedDiscordHostFacts` / `DiscordAccessibilityOperationGate`
    /// were introduced to cure.
    #[derive(Clone, Copy)]
    struct BorrowedTetherWorkerState {
        /// When the last full cross-process identity verification succeeded.
        identity_verified_at: Option<Instant>,
        /// The exact HWND that verification covered.
        identity_verified_window: isize,
        /// The exact pid that verification covered.
        identity_verified_process_id: u32,
        /// Sticky: set by any pass that observes an iconic host, cleared only by
        /// the pass whose compositor rebuild actually brought Discord back.
        host_was_iconic: bool,
        /// Bounded rebuild attempts for the current transition.
        restore_repaint_attempts: u32,
        /// The last restore-repaint stage label this worker published, so the
        /// single-slot host-stage file is written only on a real change.
        restore_repaint_stage: Option<&'static str>,
        /// Whether the pass currently running has issued any cross-process write
        /// to the borrowed window. Reset at the start of every pass.
        wrote_to_target: bool,
    }

    impl BorrowedTetherWorkerState {
        fn new() -> Self {
            Self {
                identity_verified_at: None,
                identity_verified_window: 0,
                identity_verified_process_id: 0,
                host_was_iconic: false,
                restore_repaint_attempts: 0,
                restore_repaint_stage: None,
                wrote_to_target: false,
            }
        }

        /// Drop the cached verification so the next pass re-derives it in full.
        fn invalidate_identity(&mut self) {
            self.identity_verified_at = None;
            self.identity_verified_window = 0;
            self.identity_verified_process_id = 0;
        }

        /// Publish a restore-repaint decision to the single host-stage slot only
        /// when it differs from the last one. Steady state therefore writes
        /// `restore_repaint_skipped_not_iconic` once and then never competes with
        /// the `tether_stall_*` / `realign_tether_failed` signal again.
        fn publish_restore_repaint(&mut self, decision: BorrowedTetherRestoreRepaint) {
            let stage = decision.stage();
            if self.restore_repaint_stage == Some(stage) {
                return;
            }
            self.restore_repaint_stage = Some(stage);
            qa_discord_host_stage(stage);
        }
    }

    /// Verify that the bound HWND still belongs to the exact trusted Discord
    /// process, re-deriving the full cross-process identity only when the cheap
    /// window-manager read cannot already vouch for the cached one.
    ///
    /// Every check that existed before still runs; only its *frequency* changed.
    /// The cheap half — HWND non-null and `GetWindowThreadProcessId` matching the
    /// bound pid — runs on every single pass and is what makes the cache sound
    /// (see [`BORROWED_TETHER_IDENTITY_MAX_AGE`]). A failed verification wipes the
    /// cache, so a rejection can never be papered over by a later cache hit.
    unsafe fn borrowed_tether_target_identity_is_valid(
        snapshot: &BorrowedTetherSnapshot,
        state: &mut BorrowedTetherWorkerState,
    ) -> bool {
        let window = snapshot.window as HWND;
        if window.is_null() {
            state.invalidate_identity();
            return false;
        }
        // Answered from the window manager's own table: no cross-process call,
        // and it fails closed for a destroyed window or a dead process.
        if window_process_id(window) != Some(snapshot.process_id) {
            state.invalidate_identity();
            return false;
        }
        if borrowed_tether_identity_check(
            state.identity_verified_window,
            state.identity_verified_process_id,
            snapshot.window,
            snapshot.process_id,
            state.identity_verified_at.map(|at| at.elapsed()),
            BORROWED_TETHER_IDENTITY_MAX_AGE,
        ) == BorrowedTetherIdentityCheck::Cached
        {
            qa_tether_repair_decision(TETHER_IDENTITY_CACHED_LABEL, 1);
            return true;
        }
        qa_tether_repair_decision(TETHER_IDENTITY_VERIFIED_LABEL, 1);
        let verified = ProcessHandle::open(snapshot.process_id)
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
            });
        if verified {
            state.identity_verified_at = Some(Instant::now());
            state.identity_verified_window = snapshot.window;
            state.identity_verified_process_id = snapshot.process_id;
        } else {
            state.invalidate_identity();
        }
        verified
    }

    unsafe fn borrowed_tether_parent_is_valid(snapshot: &BorrowedTetherSnapshot) -> bool {
        let parent = snapshot.parent as HWND;
        !parent.is_null()
            && window_process_id(parent) == Some(std::process::id())
            && GetAncestor(parent, GA_ROOT) == parent
    }

    /// One-shot identity gate for callers that hold no worker state, such as
    /// `BorrowedWindowTether::create`. A fresh state has never verified anything,
    /// so this always runs the full cross-process verification.
    unsafe fn borrowed_tether_identity_is_valid(snapshot: &BorrowedTetherSnapshot) -> bool {
        let mut state = BorrowedTetherWorkerState::new();
        borrowed_tether_target_identity_is_valid(snapshot, &mut state)
            && borrowed_tether_parent_is_valid(snapshot)
    }

    /// Cheap, bounded sample of the borrowed window's own client content.
    ///
    /// Renders into a small off-screen bitmap via
    /// `PrintWindow(PW_RENDERFULLCONTENT)` — the exact call this bug was
    /// diagnosed with — and counts distinct colours across a fixed 6x6 grid
    /// (36 `GetPixel` reads against a local in-process bitmap, never against
    /// the foreign window's own DC). No accessibility/UIA/MSAA API is used:
    /// those can hard-deadlock this process against a foreign UI thread,
    /// which is exactly the failure mode the rest of this file works around.
    /// Every GDI object created here is released before returning, so this is
    /// bounded no matter what state the borrowed window is in and safe to
    /// call from the tether worker thread on every repair attempt.
    unsafe fn borrowed_window_content_distinct_colours(window: HWND, size: [i32; 2]) -> usize {
        let [width, height] = size;
        if width <= 0 || height <= 0 {
            return 0;
        }
        let screen_dc = GetDC(std::ptr::null_mut());
        if screen_dc.is_null() {
            return 0;
        }
        let memory_dc = CreateCompatibleDC(screen_dc);
        let bitmap = if memory_dc.is_null() {
            std::ptr::null_mut()
        } else {
            CreateCompatibleBitmap(screen_dc, width, height)
        };
        ReleaseDC(std::ptr::null_mut(), screen_dc);
        if memory_dc.is_null() || bitmap.is_null() {
            if !bitmap.is_null() {
                DeleteObject(bitmap);
            }
            if !memory_dc.is_null() {
                DeleteDC(memory_dc);
            }
            return 0;
        }
        let previous = SelectObject(memory_dc, bitmap);
        let rendered = PrintWindow(window, memory_dc, PW_RENDERFULLCONTENT) != 0;
        let mut distinct: Vec<u32> = Vec::new();
        if rendered {
            const GRID: i32 = 6;
            for row in 0..GRID {
                let y = (height * (row * 2 + 1)) / (GRID * 2);
                for col in 0..GRID {
                    let x = (width * (col * 2 + 1)) / (GRID * 2);
                    if (0..width).contains(&x) && (0..height).contains(&y) {
                        let color = GetPixel(memory_dc, x, y);
                        if color != u32::MAX && !distinct.contains(&color) {
                            distinct.push(color);
                        }
                    }
                }
            }
        }
        SelectObject(memory_dc, previous);
        DeleteObject(bitmap);
        DeleteDC(memory_dc);
        distinct.len()
    }

    /// Force Chromium's compositor surface to be torn down and rebuilt on the
    /// exact already-verified borrowed window.
    ///
    /// After OSL is minimized (which minimizes Discord with it through
    /// `GWLP_HWNDPARENT`) and restored, Discord's client area renders a single
    /// flat colour — `PrintWindow(PW_RENDERFULLCONTENT)` measures `distinct=1`
    /// against a healthy `distinct=201` — and it does not recover on its own. A
    /// plain `RedrawWindow` was measured to be insufficient. What works is
    /// minimize/restore plus a real size nudge plus a forced synchronous
    /// redraw, in that order.
    ///
    /// `target` is the parent-derived rect this pass is going to hold the window
    /// at anyway, so the nudge grows off it and shrinks straight back onto it:
    /// the pass ends with the window exactly where the conditional repair
    /// wants it, and no extra corrective pass is needed. Nothing here
    /// activates the composite beyond what `SW_RESTORE` does, and every call is
    /// against the HWND whose pid, creation time, session, signed path, and OSL
    /// parent were all revalidated earlier in this same pass.
    ///
    /// Returns whether the content actually came back, verified by sampling
    /// distinct colours in the repainted client area — *not* by `IsIconic`.
    /// `IsIconic` cannot be the success signal here: `SW_RESTORE` already ran
    /// once earlier in the same reconcile pass, and a second time right above
    /// in this function, so `IsIconic(window) == 0` was true on the very first
    /// attempt regardless of whether the compositor surface actually came
    /// back, which made the caller's bounded retry unreachable in practice.
    /// Chromium hosts its GPU swap chain in a child window of this class. It is
    /// a sibling of `Chrome_RenderWidgetHostHWND`, not a parent of it, and it is
    /// the surface that is actually presented.
    const BORROWED_COMPOSITOR_SURFACE_CLASS: &str = "Intermediate D3D Window";

    struct BorrowedCompositorSurfaceRealign {
        client_width: i32,
        client_height: i32,
        realigned: usize,
    }

    unsafe extern "system" fn enum_borrowed_compositor_surface(
        window: HWND,
        parameter: LPARAM,
    ) -> BOOL {
        let state = &mut *(parameter as *mut BorrowedCompositorSurfaceRealign);
        let mut class_name = [0u16; 64];
        let class_length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        if class_length <= 0 {
            return 1;
        }
        if String::from_utf16_lossy(&class_name[..class_length as usize])
            != BORROWED_COMPOSITOR_SURFACE_CLASS
        {
            return 1;
        }
        // Child geometry is parent-client relative, so the only correct
        // placement is the client origin at the full client size. Writing it
        // unconditionally is idempotent: when the surface is already aligned
        // this resolves to a no-op move rather than a visible change.
        if SetWindowPos(
            window,
            std::ptr::null_mut(),
            0,
            0,
            state.client_width,
            state.client_height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        ) != 0
        {
            state.realigned = state.realigned.saturating_add(1);
        }
        1
    }

    /// Put the borrowed window's compositor surface back onto its client rect.
    ///
    /// After the OSL host is minimized and restored, Chromium leaves this child
    /// misplaced relative to the borrowed top-level window, so the composited
    /// frame is presented outside the visible client area: the window reads as
    /// solid black even though its renderer is alive and its own
    /// `Chrome_RenderWidgetHostHWND` child is still correctly positioned.
    ///
    /// Measured against a live borrowed Discord on 2026-07-25 — parent at
    /// `240,118,1680,970`, surface at `480,-776,1920,76`: the right 1440x852
    /// size at the wrong origin, almost entirely off-screen. Every other
    /// Chromium window on the same desktop (CurseForge, WhatsApp Web, Mullvad,
    /// and a second *unborrowed* Discord window) had the two rects matching
    /// exactly, so the misplacement is specific to the window OSL borrows.
    /// Repositioning it took the borrowed window from 1 distinct sampled colour
    /// to 102.
    ///
    /// Neither the resize nudge below nor `RedrawWindow` can substitute for
    /// this: Chromium does not reposition this window in response to a size
    /// change it did not originate, which is why the bounded repaint repair
    /// could exhaust its retry limit and still verify unhealthy every time.
    unsafe fn realign_borrowed_compositor_surface(window: HWND) -> usize {
        let mut client: RECT = std::mem::zeroed();
        if GetClientRect(window, &mut client) == 0 {
            return 0;
        }
        let mut state = BorrowedCompositorSurfaceRealign {
            client_width: (client.right - client.left).max(1),
            client_height: (client.bottom - client.top).max(1),
            realigned: 0,
        };
        EnumChildWindows(
            window,
            Some(enum_borrowed_compositor_surface),
            &mut state as *mut BorrowedCompositorSurfaceRealign as LPARAM,
        );
        state.realigned
    }

    unsafe fn force_borrowed_compositor_rebuild(window: HWND, target: [i32; 4]) -> bool {
        // Tear the surface down and bring it back. This is the part a plain
        // invalidate cannot substitute for.
        ShowWindow(window, SW_MINIMIZE);
        ShowWindow(window, SW_RESTORE);
        let width = target[2] - target[0];
        let height = target[3] - target[1];
        // A real size delta, not a one-pixel nudge: a 1px grow/shrink was
        // measured insufficient to make Chromium re-layout and
        // re-virtualize its scrolling lists (channel sidebar, message list,
        // member pane) — that is the exact partial-failure mode this repair
        // exists for. Tens of pixels is enough to force it, and the window
        // still ends this function exactly on `target` either way.
        const REPAINT_NUDGE_PX: i32 = 40;
        let nudge = SWP_NOACTIVATE | SWP_NOZORDER;
        let _ = SetWindowPos(
            window,
            std::ptr::null_mut(),
            target[0],
            target[1],
            width + REPAINT_NUDGE_PX,
            height + REPAINT_NUDGE_PX,
            nudge,
        );
        // Let a frame actually run between the grow and the shrink instead of
        // issuing both `SetWindowPos` calls back-to-back with no yield (also
        // measured insufficient). This is a single bounded sleep of one frame
        // — the same duration the tether's own fast polling tier already
        // uses — on this dedicated tether worker thread only; it never blocks
        // the borrowed window's own UI thread or OSL's UI thread, and it runs
        // at most once per minimize/restore transition, not on every pass.
        thread::sleep(BORROWED_TETHER_ACTIVE_INTERVAL);
        let _ = SetWindowPos(
            window,
            std::ptr::null_mut(),
            target[0],
            target[1],
            width,
            height,
            nudge,
        );
        // Runs after the window is back on `target` so the client rect this
        // reads is the final one, and before the invalidate below so the
        // repaint lands on a surface that is already in the right place. This
        // is the step that actually fixes the black window; everything above
        // only re-runs Chromium's layout.
        let _ = realign_borrowed_compositor_surface(window);
        let _ = RedrawWindow(
            window,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
        if IsIconic(window) != 0 {
            // Still iconic: definitely not repainted, and sampling it would
            // be meaningless. Skip the capture rather than pay for it.
            return false;
        }
        let distinct_colours = borrowed_window_content_distinct_colours(window, [width, height]);
        borrowed_window_content_is_healthy(distinct_colours)
    }

    /// One reconcile observation plus the exact branch that produced it. The
    /// branch is a fixed diagnostic label only; it grants no authority and
    /// records no window title, path, or content.
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    struct BorrowedTetherOutcome {
        observation: BorrowedTetherObservation,
        stall: Option<BorrowedTetherStall>,
    }

    impl BorrowedTetherOutcome {
        fn aligned() -> Self {
            Self {
                observation: BorrowedTetherObservation::Aligned,
                stall: None,
            }
        }

        fn parent_minimized() -> Self {
            Self {
                observation: BorrowedTetherObservation::ParentMinimized,
                stall: None,
            }
        }

        fn identity_changed(stall: BorrowedTetherStall) -> Self {
            Self {
                observation: BorrowedTetherObservation::IdentityChanged,
                stall: Some(stall),
            }
        }

        fn transient(stall: BorrowedTetherStall) -> Self {
            Self {
                observation: BorrowedTetherObservation::TransientDesktopUnavailable,
                stall: Some(stall),
            }
        }
    }

    unsafe fn reconcile_borrowed_tether(
        snapshot: &BorrowedTetherSnapshot,
        state: &mut BorrowedTetherWorkerState,
    ) -> BorrowedTetherOutcome {
        qa_tether_repair_decision(TETHER_RECONCILE_PASS_LABEL, 1);
        // Every pass starts having written nothing. Any cross-process write below
        // sets this, and the worker's backoff reads it to decide whether the
        // composite is quiet enough to widen the polling cadence.
        state.wrote_to_target = false;
        if !borrowed_tether_target_identity_is_valid(snapshot, state) {
            return BorrowedTetherOutcome::identity_changed(
                BorrowedTetherStall::TargetIdentityChanged,
            );
        }
        if !borrowed_tether_parent_is_valid(snapshot) {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::ParentInvalid);
        }
        let window = snapshot.window as HWND;
        let parent = snapshot.parent as HWND;
        if borrowed_owner_requires_repair(
            GetWindowLongPtrW(window, GWLP_HWNDPARENT),
            snapshot.parent,
        ) {
            state.wrote_to_target = true;
            // Discord can clear its owner while internally recreating its
            // Electron presentation. The exact HWND, PID, creation time,
            // signed path, session, and OSL parent were all revalidated above,
            // so restoring only the already-bound owner is safe and keeps the
            // borrowed window attached without activating it.
            SetLastError(ERROR_SUCCESS);
            let previous_owner = SetWindowLongPtrW(window, GWLP_HWNDPARENT, snapshot.parent);
            if previous_owner == 0 && GetLastError() != ERROR_SUCCESS {
                return BorrowedTetherOutcome::transient(
                    BorrowedTetherStall::OwnerRepairRejected,
                );
            }
            let _ = SetWindowPos(
                window,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            );
        }
        // The host being iconic is the *only* thing that arms the restore
        // repaint, and it is observed here on the same pass that keeps the
        // borrowed window minimized with it.
        let host_is_iconic = IsIconic(parent) != 0;
        let repaint = borrowed_tether_restore_repaint_decision(
            host_is_iconic,
            state.host_was_iconic,
            state.restore_repaint_attempts,
            BORROWED_TETHER_RESTORE_REPAINT_LIMIT,
        );
        state.publish_restore_repaint(repaint);
        if host_is_iconic {
            // Sticky arm: cleared only once a rebuild has actually landed.
            state.host_was_iconic = true;
            state.restore_repaint_attempts = 0;
            if IsIconic(window) == 0 {
                state.wrote_to_target = true;
                ShowWindow(window, SW_MINIMIZE);
            }
            return BorrowedTetherOutcome::parent_minimized();
        }
        if IsWindowVisible(parent) == 0 {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::ParentHidden);
        }
        if IsIconic(window) != 0 {
            state.wrote_to_target = true;
            ShowWindow(window, SW_RESTORE);
        }
        let Some(expected) = parent_target_rect(parent) else {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::ParentRectUnavailable);
        };
        let expected = rect_array(expected);
        match repaint {
            BorrowedTetherRestoreRepaint::Apply => {
                // Fires once per minimize -> restore transition. It runs before
                // the conditional repair reads the rect, and it lands the window
                // on exactly the parent-derived target rect, so it never fights
                // the repair plan: the plan that follows sees an aligned window
                // and stays a no-op.
                state.wrote_to_target = true;
                state.restore_repaint_attempts = state.restore_repaint_attempts.saturating_add(1);
                qa_tether_repair_decision(TETHER_RESTORE_REPAINT_LABEL, 1);
                if force_borrowed_compositor_rebuild(window, expected) {
                    // The surface is verified repainted (distinct-colour
                    // sample, not `IsIconic` — see that function's doc
                    // comment for why `IsIconic` alone could never fail
                    // here). Disarm until the next transition.
                    state.host_was_iconic = false;
                    state.restore_repaint_attempts = 0;
                }
                // Else: stay armed. `restore_repaint_attempts` was already
                // incremented above, so the next pass that still finds
                // `host_was_iconic` set retries through the same bounded
                // path, up to `BORROWED_TETHER_RESTORE_REPAINT_LIMIT`.
            }
            BorrowedTetherRestoreRepaint::Abandon => {
                // Bounded: stop re-minimizing a window that will not come back.
                state.host_was_iconic = false;
                state.restore_repaint_attempts = 0;
            }
            BorrowedTetherRestoreRepaint::Defer | BorrowedTetherRestoreRepaint::Skip => {}
        }
        let mut actual: RECT = std::mem::zeroed();
        let actual_matches =
            GetWindowRect(window, &mut actual) != 0 && rect_array(actual) == expected;
        // Read-only, bounded, and off the borrowed window's UI thread: `GetWindow`
        // is answered from the window manager's own z-order list. The borrowed
        // window is owned by the trusted OSL parent, so the only stack state this
        // tether must correct is the owner sitting above it, which would hide the
        // borrowed body behind OSL's background. The composer sibling's order
        // relative to the borrowed window is the protected-overlay guard's
        // invariant (it requires the composer *above* Discord and corrects that on
        // its own read-gated probe cadence); unconditionally raising the borrowed
        // window to `HWND_TOP` here both fought that guard and serialized a
        // cross-process write on the borrowed UI thread on every single pass.
        let owner_is_above = borrowed_tether_owner_is_above_target(
            snapshot.parent,
            snapshot.window,
            BORROWED_TETHER_STACK_WALK_LIMIT,
            |cursor| GetWindow(cursor as HWND, GW_HWNDPREV) as isize,
        );
        let foreground = GetForegroundWindow();
        let foreground_root = if foreground.is_null() {
            std::ptr::null_mut()
        } else {
            GetAncestor(foreground, GA_ROOT)
        };
        // Both reads above are answered by the window manager, not by the borrowed
        // window's message loop. A proved stack inversion is only worth a write
        // while either half of the composite is active: a background composite is
        // not on screen, and activating it realigns through this same path.
        let composite_is_active = foreground_root == parent || foreground_root == window;
        let plan = borrowed_tether_repair_plan(
            actual_matches,
            IsWindowVisible(window) != 0,
            composite_is_active,
            owner_is_above,
        );
        for label in plan.decision_labels().into_iter().flatten() {
            qa_tether_repair_decision(label, 1);
        }
        if borrowed_tether_requires_repair(plan) {
            state.wrote_to_target = true;
            // Write exactly the corrections the plan proved necessary, and
            // nothing else: geometry without touching z-order, z-order without
            // moving the window, a reveal only for a window reported hidden.
            let mut flags = if plan.reveal {
                SWP_NOACTIVATE | SWP_SHOWWINDOW
            } else {
                SWP_NOACTIVATE
            };
            if !plan.geometry {
                flags |= SWP_NOMOVE | SWP_NOSIZE;
            }
            if !plan.zorder {
                flags |= SWP_NOZORDER;
            }
            let insert_after: HWND = if plan.zorder {
                HWND_TOP
            } else {
                std::ptr::null_mut()
            };
            if SetWindowPos(
                window,
                insert_after,
                expected[0],
                expected[1],
                expected[2] - expected[0],
                expected[3] - expected[1],
                flags,
            ) == 0
            {
                return BorrowedTetherOutcome::transient(
                    BorrowedTetherStall::RepairSetWindowPosRejected,
                );
            }
        }
        let mut verified: RECT = std::mem::zeroed();
        if IsWindowVisible(window) == 0 {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::TargetHidden);
        }
        if IsIconic(window) != 0 {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::TargetMinimized);
        }
        if GetWindowRect(window, &mut verified) == 0 {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::TargetRectUnavailable);
        }
        if rect_array(verified) != expected {
            return BorrowedTetherOutcome::transient(BorrowedTetherStall::TargetRectMismatch);
        }
        BorrowedTetherOutcome::aligned()
    }

    unsafe fn borrowed_window_tether_worker(
        snapshot: BorrowedTetherSnapshot,
        commands: mpsc::Receiver<BorrowedWindowTetherCommand>,
        ready: mpsc::SyncSender<bool>,
        healthy: Arc<AtomicBool>,
    ) {
        // Owned by this thread only, never shared, never locked.
        let mut state = BorrowedTetherWorkerState::new();
        let initial = reconcile_borrowed_tether(&snapshot, &mut state).observation;
        let ready_value = !matches!(initial, BorrowedTetherObservation::IdentityChanged);
        let _ = ready.send(ready_value);
        if !ready_value {
            healthy.store(false, Ordering::Release);
            return;
        }
        // Consecutive passes that proved the composite quiet. Zero means "snap
        // back to the 16 ms tier"; the ladder in `borrowed_tether_poll_interval`
        // widens only while this keeps growing.
        let mut consecutive_quiet = 0u32;
        loop {
            let interval = borrowed_tether_poll_interval(consecutive_quiet);
            let command = commands.recv_timeout(interval);
            match command {
                Ok(BorrowedWindowTetherCommand::Stop) => break,
                Ok(BorrowedWindowTetherCommand::Reconcile(request, result)) => {
                    // Queued reconciles are idempotent requests for the same
                    // composite state, so a slow pass must never turn into a
                    // backlog of cross-process passes. Drain the queue, run one
                    // pass, and answer every waiting caller from that one pass.
                    let mut pending = vec![(request, result)];
                    let mut saw_stop = false;
                    loop {
                        match commands.try_recv() {
                            Ok(BorrowedWindowTetherCommand::Reconcile(queued, sender)) => {
                                pending.push((queued, sender));
                            }
                            Ok(BorrowedWindowTetherCommand::Stop) => {
                                saw_stop = true;
                                break;
                            }
                            Err(mpsc::TryRecvError::Empty)
                            | Err(mpsc::TryRecvError::Disconnected) => break,
                        }
                    }
                    let coalesced = match borrowed_tether_batch(pending.len(), saw_stop) {
                        BorrowedTetherBatch::ReconcileOnce { coalesced } => coalesced,
                        // Dropping the reply channels lets every waiting caller
                        // observe the torn-down worker at once instead of waiting
                        // out its own budget.
                        BorrowedTetherBatch::TearDown => break,
                    };
                    qa_tether_repair_decision(TETHER_REPAIR_COALESCED_LABEL, coalesced as u64);
                    let outcome = reconcile_borrowed_tether(&snapshot, &mut state);
                    let observation = outcome.observation;
                    let valid = !matches!(observation, BorrowedTetherObservation::IdentityChanged);
                    let report = borrowed_tether_reconcile_report(observation, outcome.stall);
                    for (answered, sender) in pending {
                        let _ = sender.send((answered, report));
                    }
                    if !valid {
                        healthy.store(false, Ordering::Release);
                        break;
                    }
                    // The caller was already answered from this pass, so the
                    // request itself carries no cadence information. Only a pass
                    // that had to write snaps the polling timer back to fast.
                    consecutive_quiet = borrowed_tether_next_quiet_run(
                        consecutive_quiet,
                        observation,
                        state.wrote_to_target,
                    );
                    if borrowed_tether_pass_is_quiet(observation, state.wrote_to_target) {
                        qa_tether_repair_decision(TETHER_QUIET_PASS_LABEL, 1);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let outcome = reconcile_borrowed_tether(&snapshot, &mut state);
                    let observation = outcome.observation;
                    if borrowed_tether_decision(observation, 0, usize::MAX)
                        == BorrowedTetherDecision::FailClosed
                    {
                        healthy.store(false, Ordering::Release);
                        break;
                    }
                    consecutive_quiet = borrowed_tether_next_quiet_run(
                        consecutive_quiet,
                        observation,
                        state.wrote_to_target,
                    );
                    if borrowed_tether_pass_is_quiet(observation, state.wrote_to_target) {
                        qa_tether_repair_decision(TETHER_QUIET_PASS_LABEL, 1);
                    }
                }
            }
        }
    }

    pub(super) fn run_borrowed_guardian_if_requested(arguments: &[String]) -> bool {
        startup_breadcrumb("guardian_subprocess_check_enter"); // STARTUP-TRACE
        if arguments.get(1).map(String::as_str) != Some(BORROWED_GUARDIAN_MARKER) {
            startup_breadcrumb("guardian_subprocess_not_requested"); // STARTUP-TRACE
            return false;
        }
        startup_breadcrumb("guardian_subprocess_marker_matched"); // STARTUP-TRACE
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
        startup_breadcrumb("guardian_subprocess_ready_write_before"); // STARTUP-TRACE
        let _ = std::io::stdout().write_all(b"ready\n");
        let _ = std::io::stdout().flush();
        startup_breadcrumb("guardian_subprocess_ready_write_after"); // STARTUP-TRACE
        startup_breadcrumb("guardian_subprocess_wait_for_parent_before"); // STARTUP-TRACE
        if unsafe { WaitForSingleObject(parent.0, INFINITE) } == WAIT_OBJECT_0 {
            startup_breadcrumb("guardian_subprocess_wait_for_parent_after_signaled"); // STARTUP-TRACE
            let _ = unsafe { restore_guardian_snapshot(&snapshot) };
            if guardian_closes_after_restore(snapshot.disposition) {
                // OSL spawned this window, so it does not outlive OSL -- and OSL
                // is now gone, however it went. The restore above already ran, so
                // a refused close (Discord's own close-to-tray setting, a modal
                // "unsaved changes" prompt, a hung pump) leaves an ordinary,
                // taskbar-listed, self-owned window instead of an orphan.
                //
                // `guardian_target_process` re-proves window-to-pid, creation
                // time, session, image path and Authenticode publisher, and its
                // handle is held across the post so the pid cannot be recycled
                // underneath it. The post itself is `WM_CLOSE` via
                // `PostMessageW`: the client runs its own close handler, no
                // process is ever terminated, and no profile data is touched.
                startup_breadcrumb("guardian_subprocess_close_spawned_before"); // STARTUP-TRACE
                let closed = unsafe {
                    guardian_target_process(&snapshot).is_some_and(|_process| {
                        request_graceful_close(snapshot.window as HWND, snapshot.process_id)
                    })
                };
                startup_breadcrumb(if closed {
                    "guardian_subprocess_close_spawned_posted" // STARTUP-TRACE
                } else {
                    "guardian_subprocess_close_spawned_unavailable" // STARTUP-TRACE
                });
            }
        }
        startup_breadcrumb("guardian_subprocess_done"); // STARTUP-TRACE
        true
    }

    /// The shield's window procedure. Installed via `SetWindowLongPtrW(shield,
    /// GWLP_WNDPROC, ...)` immediately after creation, replacing `STATIC`'s
    /// own procedure entirely.
    ///
    /// This is the robust half of the white-bar fix: `paint_borrowed_control_shield`
    /// below still does an immediate fill the moment a colour is sampled, but
    /// that only runs when this worker's own message loop happens to service
    /// a `Position` command or timeout tick. Any other repaint this window is
    /// asked for by the window manager — including the implicit one that
    /// creation/`ShowWindow` itself can generate, and any DWM-driven repaint
    /// while this thread's loop is busy or starved — goes through
    /// `WM_ERASEBKGND`/`WM_PAINT` here instead, and both are answered with the
    /// last colour this shield was told to use (falling back to
    /// `BORROWED_CONTROL_SHIELD_DEFAULT_COLOR`, never the default class
    /// brush). That is what a live capture caught: the plain `STATIC` class
    /// answering one of these messages with the default white
    /// `COLOR_WINDOW` brush before the worker's own explicit paint ever ran.
    /// Every other message is handled by `DefWindowProcW`; this window never
    /// shows text or uses any other `STATIC` style, so nothing else it needs
    /// is lost by not chaining to `STATIC`'s original procedure.
    unsafe extern "system" fn borrowed_control_shield_wnd_proc(
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match message {
            WM_ERASEBKGND => {
                let hdc = wparam as HDC;
                let mut client: RECT = std::mem::zeroed();
                if !hdc.is_null() && GetClientRect(window, &mut client) != 0 {
                    let color =
                        borrowed_control_shield_stored_color(GetWindowLongPtrW(window, GWLP_USERDATA));
                    let brush = CreateSolidBrush(color);
                    if !brush.is_null() {
                        FillRect(hdc, &client, brush);
                        DeleteObject(brush);
                    }
                }
                // Non-zero: this window procedure erased the background
                // itself. Never fall through to `DefWindowProcW` here, which
                // for the `STATIC` class would erase with the default system
                // white brush -- the exact bug being fixed.
                1
            }
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(window, &mut paint);
                if !hdc.is_null() {
                    let color =
                        borrowed_control_shield_stored_color(GetWindowLongPtrW(window, GWLP_USERDATA));
                    let brush = CreateSolidBrush(color);
                    if !brush.is_null() {
                        // Belt-and-suspenders: fill again even though
                        // `WM_ERASEBKGND` above should already have painted
                        // this exact colour into the same region. Harmless if
                        // redundant, and a second line of defence if some
                        // future refactor ever changes the erase behaviour.
                        FillRect(hdc, &paint.rcPaint, brush);
                        DeleteObject(brush);
                    }
                }
                EndPaint(window, &paint);
                0
            }
            _ => DefWindowProcW(window, message, wparam, lparam),
        }
    }

    unsafe fn borrowed_control_shield_worker(
        target: HWND,
        expected_process_id: u32,
        commands: mpsc::Receiver<BorrowedControlShieldCommand>,
        ready: mpsc::SyncSender<Option<isize>>,
        measurement: CaptionMeasurement,
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
        // Subclass immediately, before anything else touches this window and
        // before it is ever handed to a caller. `STATIC`'s own window
        // procedure erases with the default system `COLOR_WINDOW` brush
        // (white) on any `WM_ERASEBKGND`/`WM_PAINT` it receives, including
        // the ones generated by window creation/show itself; a live capture
        // caught exactly that white rectangle sitting over Discord's own
        // caption buttons. Installing our own procedure right here, before
        // the message loop below ever runs, means no message this window
        // ever receives can be answered by the white-erasing default
        // procedure. `GWLP_USERDATA` starts at its implicit default of 0,
        // which already packs to black, but it is set explicitly below so
        // the intent is not left to an implicit default.
        SetWindowLongPtrW(
            shield,
            GWLP_USERDATA,
            BORROWED_CONTROL_SHIELD_DEFAULT_COLOR as isize,
        );
        SetWindowLongPtrW(
            shield,
            GWLP_WNDPROC,
            borrowed_control_shield_wnd_proc as usize as isize,
        );
        let _ = ready.send(Some(shield as isize));
        let mut has_verified_paint = false;
        let mut running = true;
        // Everything below runs on this dedicated worker thread. It owns the
        // shield window and its message pump, and it is the *only* thread that
        // is ever allowed to schedule an accessibility probe of the borrowed
        // client -- and even then the probe itself runs on a further detached
        // thread (see `CaptionButtonProbe`), so a hung Discord can never wedge
        // this loop, `Stop`, or the `join` in `BorrowedControlShield::stop`.
        let mut probe = CaptionButtonProbe::new(measurement);
        while running {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            match commands.recv_timeout(Duration::from_millis(16)) {
                Ok(BorrowedControlShieldCommand::Position(result)) => {
                    let (positioned, painted) = if window_process_id(target)
                        == Some(expected_process_id)
                        && GetWindowLongPtrW(shield, GWLP_HWNDPARENT) == target as isize
                    {
                        position_borrowed_control_shield(
                            target,
                            shield,
                            expected_process_id,
                            &mut probe,
                            !has_verified_paint,
                        )
                    } else {
                        (false, false)
                    };
                    let valid = borrowed_control_shield_position_valid(
                        positioned,
                        has_verified_paint,
                        painted,
                    );
                    has_verified_paint |= painted;
                    let _ = result.send(valid);
                }
                Ok(BorrowedControlShieldCommand::Stop) => running = false,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if window_process_id(target) == Some(expected_process_id)
                        && GetWindowLongPtrW(shield, GWLP_HWNDPARENT) == target as isize
                    {
                        let (_, painted) = position_borrowed_control_shield(
                            target,
                            shield,
                            expected_process_id,
                            &mut probe,
                            !has_verified_paint,
                        );
                        has_verified_paint |= painted;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => running = false,
            }
        }
        DestroyWindow(shield);
    }

    /// Schedules and caches the accessibility measurement of the borrowed
    /// window's caption buttons.
    ///
    /// Owned by the shield worker thread, but every measurement runs on a
    /// short-lived **detached** thread. That is deliberate: MSAA calls are
    /// cross-process and have no timeout, so a Discord UI thread that stops
    /// pumping messages blocks whichever thread is calling into it, forever.
    /// Detaching means the worst case is one leaked thread that is never
    /// joined, instead of a wedged shield worker -- and, because
    /// `BorrowedControlShield::stop` joins that worker from OSL's UI thread, a
    /// wedged worker would be a wedged UI thread. `in_flight` guarantees at
    /// most one such thread exists at a time, so a permanently hung client
    /// leaks exactly one thread, not one per tick.
    struct CaptionButtonProbe {
        measurement: CaptionMeasurement,
        in_flight: Arc<AtomicBool>,
        last_probe: Option<Instant>,
        last_geometry: Option<([i32; 2], u32)>,
    }

    impl CaptionButtonProbe {
        fn new(measurement: CaptionMeasurement) -> Self {
            Self {
                measurement,
                in_flight: Arc::new(AtomicBool::new(false)),
                last_probe: None,
                last_geometry: None,
            }
        }

        /// The last successful measurement, or `None` before the first one
        /// lands. Never blocks on anything but the four-integer slot.
        fn measured(&self) -> Option<MeasuredCaptionButtons> {
            self.measurement.lock().ok().and_then(|measured| *measured)
        }

        /// Start a probe if one is due. Cheap and non-blocking: this is called
        /// from the worker's 16ms tick, and all it ever does synchronously is
        /// compare a couple of integers.
        fn poll(&mut self, target: HWND, expected_process_id: u32, geometry: ([i32; 2], u32)) {
            if self.in_flight.load(Ordering::SeqCst) {
                return;
            }
            let geometry_changed = self.last_geometry.is_some_and(|last| last != geometry);
            self.last_geometry = Some(geometry);
            if !caption_button_probe_due(
                self.measured().is_some(),
                geometry_changed,
                self.last_probe.map(|last| last.elapsed()),
            ) {
                return;
            }
            self.last_probe = Some(Instant::now());
            if self.in_flight.swap(true, Ordering::SeqCst) {
                return;
            }
            let measurement = Arc::clone(&self.measurement);
            let in_flight = Arc::clone(&self.in_flight);
            let target_value = target as isize;
            let spawned = thread::Builder::new()
                .name("osl-shield-caption-probe".to_owned())
                .spawn(move || {
                    let measured = unsafe {
                        measure_caption_buttons(target_value as HWND, expected_process_id)
                    };
                    // A failed probe keeps the previous measurement rather
                    // than dropping back to the reconstruction: the cluster is
                    // stored relative to the right edge, so the old one is
                    // still correct for the new window size.
                    if let (Some(measured), Ok(mut slot)) = (measured, measurement.lock()) {
                        *slot = Some(measured);
                    }
                    in_flight.store(false, Ordering::SeqCst);
                });
            if spawned.is_err() {
                self.in_flight.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Wall-clock cap on one caption probe, re-tested before every
    /// cross-process call. It cannot bound a single call that never returns —
    /// nothing can — but it stops a slow tree from being walked to the end.
    const CAPTION_PROBE_TIMEOUT: Duration = Duration::from_millis(250);
    /// Hard cap on nodes read by one probe. Deliberately small: this is a
    /// corner probe, not a tree walk (see `measure_caption_buttons`).
    const CAPTION_PROBE_MAX_NODES: usize = 192;
    /// Hard cap on how deep the probe descends.
    const CAPTION_PROBE_MAX_DEPTH: usize = 14;
    /// Containers with more children than this are skipped outright rather than
    /// truncated. Chromium's message list and member list are far past it; the
    /// titlebar and its ancestors are far below it.
    const CAPTION_PROBE_MAX_CHILDREN: usize = 96;
    /// How long the liveness ping below waits for the borrowed client.
    const CAPTION_PROBE_PING_TIMEOUT_MS: u32 = 120;

    struct CaptionProbeComGuard(bool);

    impl Drop for CaptionProbeComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    /// Whether the borrowed client is currently pumping messages.
    ///
    /// Every MSAA call below is cross-process and has no timeout, so a client
    /// that has stopped pumping blocks the caller forever. `SMTO_ABORTIFHUNG`
    /// plus an explicit timeout is the one bounded question that can be asked
    /// first, and a `WM_NULL` is the cheapest thing to ask it with.
    unsafe fn borrowed_window_answers_ping(window: HWND) -> bool {
        if IsHungAppWindow(window) != 0 {
            return false;
        }
        let mut result: usize = 0;
        SendMessageTimeoutW(
            window,
            WM_NULL,
            0,
            0,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            CAPTION_PROBE_PING_TIMEOUT_MS,
            &mut result,
        ) != 0
    }

    /// Measure the borrowed window's real window-control buttons over MSAA.
    ///
    /// Discord publishes its accessibility tree over MSAA/`oleacc` only (UI
    /// Automation sees nothing on it), and it draws its own HTML titlebar, so
    /// its window controls are ordinary `ROLE_SYSTEM_PUSHBUTTON` nodes in that
    /// tree with real `accLocation` rectangles. Those rectangles are the
    /// measurement this shield has been missing: everything before this asked
    /// DWM, which reports a *zero-width* cluster for a custom-titlebar Chromium
    /// window, and then reconstructed a fixed 138x22-at-30px strip from the
    /// caption height — a guess, and the reason the shield "doesn't match
    /// perfectly" and "covers other stuff too".
    ///
    /// ## Why this is not the tree walk that was already reverted
    ///
    /// `native_discord_adapter.rs` records that a whole-tree walk down from
    /// `OBJID_CLIENT` was tried for transcript discovery and reverted, because
    /// live it wedged the app and no wall-clock budget can bound a walk whose
    /// individual calls can each block forever. That verdict is respected here:
    ///
    /// * The descent is **geometry-pruned** by `caption_button_search_region`.
    ///   Only containers overlapping the top-right titlebar band are entered,
    ///   so the message list, sidebar and member pane each cost one
    ///   `accLocation` and are then skipped. In practice this reads tens of
    ///   nodes, not thousands.
    /// * It is capped at `CAPTION_PROBE_MAX_NODES` / `CAPTION_PROBE_MAX_DEPTH`
    ///   / `CAPTION_PROBE_TIMEOUT`, and containers wider than
    ///   `CAPTION_PROBE_MAX_CHILDREN` are refused outright.
    /// * It runs at most once per `CAPTION_PROBE_MIN_INTERVAL` and, at rest,
    ///   once per `CAPTION_PROBE_IDLE_INTERVAL`.
    /// * It is preceded by a bounded liveness ping, so a client that has
    ///   already stopped pumping is never called into at all.
    /// * It runs on a **detached** thread owned by nothing. If it does block
    ///   forever, one thread leaks and every OSL thread — including the shield
    ///   worker that `BorrowedControlShield::stop` joins from the UI thread —
    ///   carries on. No lock is held across any call here; the shared
    ///   measurement slot is only locked after the last one returns.
    ///
    /// Returning `None` is always safe: the shield falls back to the DWM
    /// reconstruction, i.e. to exactly the behaviour that shipped before.
    unsafe fn measure_caption_buttons(
        target: HWND,
        expected_process_id: u32,
    ) -> Option<MeasuredCaptionButtons> {
        if target.is_null()
            || window_process_id(target) != Some(expected_process_id)
            || IsWindowVisible(target) == 0
            || IsIconic(target) != 0
            || !borrowed_window_answers_ping(target)
        {
            return None;
        }
        let mut window: RECT = std::mem::zeroed();
        if GetWindowRect(target, &mut window) == 0 {
            return None;
        }
        let window_size = [window.right - window.left, window.bottom - window.top];
        let scale_percent = i32::try_from(GetDpiForWindow(target).max(1))
            .ok()?
            .saturating_mul(100)
            / 96;
        let region = caption_button_search_region(window_size, scale_percent)?;

        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _com = CaptionProbeComGuard(initialized.is_ok());
        let mut object: *mut c_void = std::ptr::null_mut();
        AccessibleObjectFromWindow(
            ComHwnd(target as _),
            OBJID_CLIENT as u32,
            &IAccessible::IID,
            &mut object,
        )
        .ok()?;
        if object.is_null() {
            return None;
        }
        let root = IAccessible::from_raw(object);

        let deadline = Instant::now() + CAPTION_PROBE_TIMEOUT;
        let self_child = VARIANT::from(0i32);
        let mut nodes: Vec<[i32; 4]> = Vec::new();
        let mut stack = vec![(root, 0usize)];
        let mut visited = 0usize;
        while let Some((container, depth)) = stack.pop() {
            if visited >= CAPTION_PROBE_MAX_NODES || Instant::now() >= deadline {
                break;
            }
            let Some(count) = container
                .accChildCount()
                .ok()
                .and_then(|count| usize::try_from(count).ok())
            else {
                continue;
            };
            if count == 0 || count > CAPTION_PROBE_MAX_CHILDREN {
                continue;
            }
            let mut children = vec![VARIANT::default(); count];
            let mut obtained = 0i32;
            if AccessibleChildren(&container, 0, &mut children, &mut obtained).is_err() {
                continue;
            }
            let obtained = usize::try_from(obtained).unwrap_or(0).min(count);
            children.truncate(obtained);
            for child in children {
                visited += 1;
                if visited >= CAPTION_PROBE_MAX_NODES || Instant::now() >= deadline {
                    break;
                }
                let object = IDispatch::try_from(&child)
                    .ok()
                    .and_then(|dispatch| dispatch.cast::<IAccessible>().ok());
                let (reader, child_id) = match object.as_ref() {
                    Some(object) => (object, &self_child),
                    None => (&container, &child),
                };
                let bounds = msaa_node_window_bounds(
                    reader,
                    child_id,
                    [window.left, window.top],
                );
                if let Some(bounds) = bounds {
                    let role = reader
                        .get_accRole(child_id)
                        .ok()
                        .and_then(|value| i32::try_from(&value).ok())
                        .and_then(|value| u32::try_from(value).ok())
                        .unwrap_or(0);
                    if caption_button_node_accepted(role, window_size, bounds, scale_percent) {
                        nodes.push(bounds);
                    }
                }
                if depth + 1 < CAPTION_PROBE_MAX_DEPTH
                    && caption_button_container_worth_walking(bounds, region)
                {
                    if let Some(object) = object {
                        stack.push((object, depth + 1));
                    }
                }
            }
        }
        caption_button_cluster(window_size, &nodes, scale_percent)
    }

    /// One MSAA node's rectangle, converted from screen coordinates into
    /// coordinates relative to the borrowed window's own origin.
    unsafe fn msaa_node_window_bounds(
        reader: &IAccessible,
        child: &VARIANT,
        window_origin: [i32; 2],
    ) -> Option<[i32; 4]> {
        let (mut left, mut top, mut width, mut height) = (0i32, 0i32, 0i32, 0i32);
        reader
            .accLocation(&mut left, &mut top, &mut width, &mut height, child)
            .ok()?;
        Some([
            left.checked_sub(window_origin[0])?,
            top.checked_sub(window_origin[1])?,
            left.checked_add(width)?.checked_sub(window_origin[0])?,
            top.checked_add(height)?.checked_sub(window_origin[1])?,
        ])
    }

    /// Read the borrowed window's live geometry and resolve where the shield
    /// belongs right now. Cheap: one `GetWindowRect`, one `DwmGetWindowAttribute`
    /// and one `GetDpiForWindow`, all of which are safe from any thread.
    ///
    /// Returns the shield's screen rectangle, the window-relative caption
    /// bounds it came from (the colour probe samples beside them), the window
    /// size and the window DPI.
    unsafe fn borrowed_control_shield_plan(
        target: HWND,
        measured: Option<MeasuredCaptionButtons>,
    ) -> Option<([i32; 4], [i32; 4], [i32; 2], u32)> {
        if IsIconic(target) != 0 || IsWindowVisible(target) == 0 {
            return None;
        }
        let mut window: RECT = std::mem::zeroed();
        if GetWindowRect(target, &mut window) == 0 {
            return None;
        }
        let mut caption: RECT = std::mem::zeroed();
        if DwmGetWindowAttribute(
            target,
            DWMWA_CAPTION_BUTTON_BOUNDS as u32,
            (&mut caption as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        ) < 0
        {
            return None;
        }
        let window_size = [window.right - window.left, window.bottom - window.top];
        let (screen, bounds) = borrowed_control_shield_target(
            [window.left, window.top],
            window_size,
            [caption.left, caption.top, caption.right, caption.bottom],
            measured,
        )?;
        Some((screen, bounds, window_size, GetDpiForWindow(target)))
    }

    /// Re-measure the borrowed window and move/repaint the shield onto it.
    ///
    /// This runs on the shield worker's 16ms tick, so it coalesces: when the
    /// shield already occupies the rectangle the current geometry calls for and
    /// its colour has been verified, the move and the cross-process pixel probe
    /// are both skipped. That keeps the shield tracking every move, resize and
    /// re-measure of the borrowed window without issuing a `SetWindowPos` plus
    /// six `GetPixel` calls sixty times a second at rest.
    unsafe fn position_borrowed_control_shield(
        target: HWND,
        shield: HWND,
        expected_process_id: u32,
        probe: &mut CaptionButtonProbe,
        force: bool,
    ) -> (bool, bool) {
        let Some((screen, bounds, window_size, dpi)) =
            borrowed_control_shield_plan(target, probe.measured())
        else {
            return (false, false);
        };
        // Scheduling only; the measurement itself never runs on this thread.
        probe.poll(target, expected_process_id, (window_size, dpi));
        let mut shield_rect: RECT = std::mem::zeroed();
        if !force
            && GetWindowRect(shield, &mut shield_rect) != 0
            && rect_array(shield_rect) == screen
            && IsWindowVisible(shield) != 0
        {
            return (true, false);
        }
        let [left, top, right, bottom] = screen;
        let positioned = SetWindowPos(
            shield,
            HWND_TOP,
            left,
            top,
            right - left,
            bottom - top,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        ) != 0;
        let painted = positioned && paint_borrowed_control_shield(target, shield, bounds);
        (positioned, painted)
    }

    /// Whether the shield is already sitting exactly where the borrowed
    /// window's current geometry says it should.
    ///
    /// Called from OSL's UI thread when the worker did not acknowledge in time,
    /// so it must stay free of anything that can block: `measured` is the
    /// worker's last cached measurement, passed in, never re-measured here.
    unsafe fn borrowed_control_shield_is_aligned(
        target: HWND,
        shield: HWND,
        expected_process_id: u32,
        measured: Option<MeasuredCaptionButtons>,
    ) -> bool {
        if target.is_null()
            || shield.is_null()
            || window_process_id(target) != Some(expected_process_id)
            || GetWindowLongPtrW(shield, GWLP_HWNDPARENT) != target as isize
            || IsWindowVisible(target) == 0
            || IsIconic(target) != 0
            || IsWindowVisible(shield) == 0
        {
            return false;
        }
        let mut shield_rect: RECT = std::mem::zeroed();
        if GetWindowRect(shield, &mut shield_rect) == 0 {
            return false;
        }
        borrowed_control_shield_plan(target, measured)
            .is_some_and(|(screen, _, _, _)| screen == rect_array(shield_rect))
    }

    unsafe fn paint_borrowed_control_shield(
        target: HWND,
        shield: HWND,
        measured_caption_buttons: [i32; 4],
    ) -> bool {
        let [left, top, _right, bottom] = measured_caption_buttons;
        let measured_height = bottom - top;
        if left <= 0 || measured_height <= 0 {
            return false;
        }
        // Probe only the verified target's own window DC immediately beside
        // the DWM-measured caption-button cluster. This includes native
        // non-client/custom-titlebar pixels and cannot sample the owned shield.
        let target_dc = GetWindowDC(target);
        if target_dc.is_null() {
            return false;
        }
        let mut samples = Vec::with_capacity(6);
        for x_offset in [8, 20] {
            let x = left - x_offset;
            for fraction in [1, 2, 3] {
                let y = top + measured_height * fraction / 4;
                if x >= 0 && y >= 0 {
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
        let Some([red, green, blue]) = borrowed_control_shield_color(&samples) else {
            return false;
        };
        let color = u32::from(red) | (u32::from(green) << 8) | (u32::from(blue) << 16);
        // Persist the sampled colour for `borrowed_control_shield_wnd_proc`.
        // The immediate fill below is the fast path for this call site; the
        // stored value is what every future `WM_ERASEBKGND`/`WM_PAINT` this
        // window ever receives on its own -- without this worker's message
        // loop being the one to drive it -- will use, so the shield never
        // regresses back to the default white brush once a colour has been
        // measured, even if this worker later stalls.
        SetWindowLongPtrW(shield, GWLP_USERDATA, color as isize);
        let brush = CreateSolidBrush(color);
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
        takeover: DiscordTakeover,
    ) -> NativeWindowHostResult {
        let trace_qa_host =
            id == NativeAppId::Discord && mode == DiscordSessionMode::ExistingSession;
        if trace_qa_host {
            qa_discord_host_stage("host_entered");
        }
        // A takeover for a client with no verified quit/relaunch contract fails
        // closed instead of quietly degrading to a borrow, so a caller that asked
        // for the wrong thing finds out.
        if takeover == DiscordTakeover::QuitAndRelaunch && !takeover_supported(id, mode) {
            return NativeWindowHostResult::unsupported(
                id,
                NativeWindowHostReason::TakeoverNotPermitted,
            );
        }
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
        if trace_qa_host {
            qa_discord_host_stage("parent_ready");
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
        if trace_qa_host {
            qa_discord_host_stage("host_lock_acquired");
        }
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
        let hosted = match cold_host_action(id, mode, takeover) {
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
                if trace_qa_host {
                    qa_discord_host_stage("claim_existing_started");
                }
                claim_or_relaunch_existing_host(generation, id, owner_namespace, parent as HWND)
            },
            ColdHostAction::TakeOverExisting => unsafe {
                if trace_qa_host {
                    qa_discord_host_stage("takeover_started");
                }
                take_over_existing_host(generation, id, owner_namespace, parent as HWND)
            },
        };
        let hosted = match hosted {
            Ok(hosted) => hosted,
            Err(reason) => {
                return NativeWindowHostResult::failed(id, reason);
            }
        };
        if trace_qa_host {
            qa_discord_host_stage("claim_existing_complete");
        }
        let capture_certified = hosted.capture_certified;
        *guard = Some(hosted);
        if trace_qa_host {
            qa_discord_host_stage("host_complete");
        }
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
        ownership: HostWindowOwnership,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        let (candidate, expected_session) = find_existing_candidate(id)?;
        adopt_existing_companion(
            generation,
            id,
            owner_namespace,
            ownership,
            candidate.process_id,
            candidate.creation_time,
            expected_session,
            candidate.process,
            candidate.executable,
            candidate.window,
            parent,
        )
    }

    /// Locate the single trusted, claimable window for `id` and return it
    /// together with OSL's own session id, having re-proved the window-to-process
    /// binding after selection.
    ///
    /// Split out of [`claim_existing_host`] because the takeover path needs the
    /// same fully verified candidate for a different purpose: to ask it to quit.
    /// Nothing here mutates anything.
    unsafe fn find_existing_candidate(
        id: NativeAppId,
    ) -> Result<(ExistingWindowCandidate, u32), NativeWindowHostReason> {
        if id == NativeAppId::Discord {
            qa_discord_host_stage("claim_scan_started");
        }
        let executable_paths =
            match id {
                NativeAppId::Discord => {
                    let local = known_folder(&FOLDERID_LocalAppData)
                        .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
                    let paths = existing_discord_channel_executables(|channel| {
                        discord_channel_executables(
                            &local.join(channel.install_directory),
                            channel.executable_name,
                        )
                    });
                    if paths.is_empty() {
                        return Err(NativeWindowHostReason::ExistingSessionUnavailable);
                    }
                    paths
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
        if id == NativeAppId::Discord {
            qa_discord_host_stage("claim_paths_ready");
        }
        let mut expected_session = 0u32;
        if ProcessIdToSessionId(std::process::id(), &mut expected_session) == 0 {
            return Err(NativeWindowHostReason::ExistingSessionUnavailable);
        }
        let mut candidates = Vec::with_capacity(8);
        for executable_path in executable_paths {
            let expected = trust_existing_executable(id, &executable_path)
                .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
            if id == NativeAppId::Discord {
                qa_discord_host_stage("claim_executable_trusted");
            }
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
            if id == NativeAppId::Discord {
                qa_discord_host_stage("claim_windows_scanned");
            }
            drop(expected);
            if search.overflowed {
                return Err(NativeWindowHostReason::ExistingSessionAmbiguous);
            }
            if id == NativeAppId::Discord && !search.candidates.is_empty() {
                // Stable, PTB, and Canary are enumerated in fixed preference
                // order. Select the first channel with a live trusted window;
                // lower-priority installed channels do not make that choice
                // ambiguous. Multiple windows within the selected channel
                // still fail closed in `take_existing_candidate`.
                candidates = search.candidates;
                break;
            }
            if candidates.len().saturating_add(search.candidates.len())
                > MAX_EXISTING_WINDOW_CANDIDATES
            {
                return Err(NativeWindowHostReason::ExistingSessionAmbiguous);
            }
            candidates.extend(search.candidates);
        }
        let candidate = take_existing_candidate(id, candidates)?;
        if id == NativeAppId::Discord {
            qa_discord_host_stage("claim_candidate_selected");
        }
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
        Ok((candidate, expected_session))
    }

    unsafe fn claim_or_relaunch_existing_host(
        generation: u64,
        id: NativeAppId,
        owner_namespace: String,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        match claim_existing_host(
            generation,
            id,
            owner_namespace.clone(),
            parent,
            // A window that was already on screen is the operator's, not OSL's.
            HostWindowOwnership::Borrowed,
        ) {
            Ok(hosted) => return Ok(hosted),
            Err(reason) if should_relaunch_existing_session(id, reason) => {}
            Err(reason) => return Err(reason),
        }
        // Everything below relaunches the client, so every window it can end up
        // adopting is one OSL created and therefore owes a close to.

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
                return wait_for_relaunched_existing_host(
                    generation,
                    id,
                    &owner_namespace,
                    parent,
                    Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT,
                );
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
                    return wait_for_relaunched_existing_host(
                        generation,
                        id,
                        &owner_namespace,
                        parent,
                        Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT,
                    );
                }
                (Some(path.to_owned()), ExecutablePublisher::Microsoft)
            }
            _ => return Err(NativeWindowHostReason::ExistingSessionUnavailable),
        };
        let executable_path =
            executable_path.ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
        let executable = verify_executable(&executable_path, publisher)
            .map_err(|_| NativeWindowHostReason::ExistingSessionUnavailable)?;
        let mut command = Command::new(executable.path());
        command.args(existing_session_launch_arguments(id));
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| NativeWindowHostReason::LaunchFailed)?;
        drop(executable);

        let claimed = wait_for_relaunched_existing_host(
            generation,
            id,
            &owner_namespace,
            parent,
            Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT,
        );
        if claimed.is_err() {
            // Reap the launcher process this path spawned. The client itself is
            // never terminated here: OSL relaunched the operator's own signed
            // client, and an unadoptable client must still be left running and
            // usable rather than killed.
            let _ = child.try_wait();
        }
        claimed
    }

    /// Option A: OSL takes ownership of the operator's client instead of
    /// borrowing a window they opened.
    ///
    /// The operator's consent has already been obtained by the caller (see
    /// [`NativeWindowHostState::host_mode_with_takeover`]); nothing here prompts.
    ///
    /// Sequence, and why each step is where it is:
    ///
    /// 1. Find the running client through the same fully verified scan the
    ///    ordinary claim uses. If none is running there is nothing to quit, and
    ///    step 2 is skipped entirely.
    /// 2. Ask it to quit with a posted `WM_CLOSE` -- the exact message its own
    ///    title-bar X sends -- and wait, bounded, for the *process* to end.
    ///    Never `TerminateProcess`: the client is the operator's real Discord,
    ///    holding their real session, and it must be allowed to run its normal
    ///    shutdown and persist its state. If it is still running when the budget
    ///    expires (Discord can be configured to close to the tray) the takeover
    ///    is abandoned: the window is put back on screen if the close hid it, and
    ///    `ExistingSessionQuitRefused` tells the caller to retry as a borrow.
    /// 3. Relaunch *the same executable that was just running* -- same install,
    ///    same profile, same account -- with `--start-inactive`, so the new
    ///    window is created and shown without taking activation and can be
    ///    adopted before the operator ever notices it.
    /// 4. Adopt it through the shared relaunch wait, marked
    ///    [`HostWindowOwnership::Spawned`], which is what makes teardown close it
    ///    rather than restore it.
    ///
    /// No profile directory, credential store or session token is read, written
    /// or copied at any point. Reusing the operator's account here means reusing
    /// their install, by starting it again.
    unsafe fn take_over_existing_host(
        generation: u64,
        id: NativeAppId,
        owner_namespace: String,
        parent: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        if !takeover_supported(id, DiscordSessionMode::ExistingSession) {
            return Err(NativeWindowHostReason::TakeoverNotPermitted);
        }
        let running = match find_existing_candidate(id) {
            Ok((candidate, _)) => Some(candidate),
            // Nothing claimable is running, so there is nothing to quit and the
            // relaunch below simply starts the client.
            Err(NativeWindowHostReason::ExistingSessionUnavailable) => None,
            // Ambiguity is a decision, not a transient state, and quitting one of
            // two indistinguishable windows is exactly what must never happen.
            Err(reason) => return Err(reason),
        };
        // The executable to relaunch is the one that was actually running, so the
        // relaunch cannot drift to a different installed channel and a different
        // profile. With nothing running, fall back to the ordinary preference
        // order.
        let relaunch_path = match &running {
            Some(candidate) => Some(candidate.executable.path().to_owned()),
            None => {
                let local = known_folder(&FOLDERID_LocalAppData)
                    .ok_or(NativeWindowHostReason::ExistingSessionUnavailable)?;
                preferred_existing_discord_channel_executable(|channel| {
                    newest_discord_channel_executable(
                        &local.join(channel.install_directory),
                        channel.executable_name,
                    )
                })
            }
        };
        let outcome = match &running {
            None => takeover_quit_outcome(false, false),
            Some(candidate) => {
                qa_discord_host_stage(TAKEOVER_QUIT_REQUESTED);
                let exited = request_graceful_close(candidate.window, candidate.process_id)
                    && wait_for_existing_client_exit(
                        candidate.process_id,
                        candidate.creation_time,
                        TAKEOVER_QUIT_BUDGET,
                    );
                takeover_quit_outcome(true, exited)
            }
        };
        if !takeover_may_relaunch(outcome) {
            qa_discord_host_stage(TAKEOVER_QUIT_REFUSED);
            // The close may have been answered by hiding. Put the operator's own
            // window back where they can see it before handing the failure up.
            if let Some(candidate) = &running {
                reveal_client_that_refused_to_quit(candidate.window, candidate.process_id);
            }
            return Err(NativeWindowHostReason::ExistingSessionQuitRefused);
        }
        qa_discord_host_stage(TAKEOVER_QUIT_LANDED);
        drop(running);
        let executable_path = relaunch_path.ok_or(NativeWindowHostReason::AppNotInstalled)?;
        let executable = verify_executable(&executable_path, ExecutablePublisher::Discord)
            .map_err(|_| NativeWindowHostReason::ExistingSessionUnavailable)?;
        let mut command = Command::new(executable.path());
        command.args(existing_session_launch_arguments(id));
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| NativeWindowHostReason::LaunchFailed)?;
        drop(executable);
        qa_discord_host_stage(TAKEOVER_RELAUNCHED);

        let claimed = wait_for_relaunched_existing_host(
            generation,
            id,
            &owner_namespace,
            parent,
            Instant::now() + EXISTING_SESSION_DISCOVERY_TIMEOUT,
        );
        if claimed.is_err() {
            // Reap the launcher process only. The client itself is left running:
            // OSL just quit the operator's Discord on their behalf, so a client
            // it could not adopt must at least still be there for them to use.
            let _ = child.try_wait();
            qa_discord_host_stage(TAKEOVER_ADOPT_FAILED);
        }
        claimed
    }

    /// Bounded wait for the exact client process OSL just asked to quit to
    /// actually end.
    ///
    /// Identity-pinned: the wait is on a handle opened for that pid whose
    /// creation time still matches, so a recycled pid can never be mistaken for
    /// the original still being alive, and a `WAIT_OBJECT_0` can only come from
    /// the process OSL addressed. Never signals, terminates or otherwise touches
    /// the process -- it only observes.
    unsafe fn wait_for_existing_client_exit(
        process_id: u32,
        creation_time: u64,
        budget: Duration,
    ) -> bool {
        let Some(process) = ProcessHandle::open_waitable(process_id) else {
            // The pid can no longer be opened at all, which for a process that
            // was open a moment ago means it is gone.
            return true;
        };
        if process_identity_from_handle(&process, process_id)
            .is_none_or(|(_, current_creation, _)| current_creation != creation_time)
        {
            return true;
        }
        let started = Instant::now();
        loop {
            if WaitForSingleObject(process.0, 0) == WAIT_OBJECT_0 {
                return true;
            }
            if deadline_reached(started.elapsed(), budget) {
                return false;
            }
            thread::sleep(HARNESSED_EXIT_POLL);
        }
    }

    /// Put a client that refused to quit back on the operator's screen.
    ///
    /// A posted `WM_CLOSE` that a client answers by hiding leaves the operator
    /// staring at nothing, which would be a strictly worse outcome than the
    /// takeover simply not happening. Nothing has been mutated by the takeover at
    /// this point -- no owner link, no extended style -- so putting the window
    /// back is only a show.
    unsafe fn reveal_client_that_refused_to_quit(window: HWND, expected_process_id: u32) {
        if window.is_null()
            || window_process_id(window) != Some(expected_process_id)
            || IsWindowVisible(window) != 0
        {
            return;
        }
        ShowWindow(window, SW_SHOW);
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
        if search.id == NativeAppId::Discord {
            qa_discord_host_stage("candidate_window_visible");
        }
        let Some(process_id) = window_process_id(window) else {
            return 1;
        };
        let Some((process, path, creation_time, session_id)) = process_identity(process_id) else {
            return 1;
        };
        if search.id == NativeAppId::Discord {
            qa_discord_host_stage("candidate_process_identified");
        }
        if session_id != search.expected_session || path != search.expected_path {
            return 1;
        }
        if search.id == NativeAppId::Discord {
            qa_discord_host_stage("candidate_path_matched");
        }
        let Some(executable) = trust_existing_executable(search.id, &path) else {
            return 1;
        };
        if search.id == NativeAppId::Discord {
            qa_discord_host_stage("candidate_executable_trusted");
        }
        let Some(bounds) = borrowed_previous_rect(window, IsIconic(window) != 0) else {
            return 1;
        };
        if search.id == NativeAppId::Discord {
            qa_discord_host_stage("candidate_bounds_ready");
        }
        search.candidates.push(ExistingWindowCandidate {
            window,
            area: i64::from(bounds.right - bounds.left) * i64::from(bounds.bottom - bounds.top),
            process_id,
            creation_time,
            process,
            executable,
        });
        if search.id == NativeAppId::Discord {
            qa_discord_host_stage("candidate_accepted");
        }
        1
    }

    struct ExistingWindowPresence {
        id: NativeAppId,
        found: bool,
    }

    unsafe extern "system" fn enum_existing_window_presence(
        window: HWND,
        parameter: LPARAM,
    ) -> BOOL {
        let presence = &mut *(parameter as *mut ExistingWindowPresence);
        let visible = IsWindowVisible(window) != 0;
        let mut class_name = [0u16; 64];
        let class_length = GetClassNameW(window, class_name.as_mut_ptr(), class_name.len() as i32);
        if class_length <= 0 {
            return 1;
        }
        // Only pay the cross-process title read for the two clients whose gate
        // is actually a function of it; for everyone else the value is inert.
        let mut title = [0u16; 128];
        let title = if existing_window_identity_uses_title(presence.id) {
            let title_length = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
            if title_length < 0 {
                return 1;
            }
            String::from_utf16_lossy(&title[..title_length as usize])
        } else {
            String::new()
        };
        if existing_window_identity_allowed(
            presence.id,
            visible,
            &String::from_utf16_lossy(&class_name[..class_length as usize]),
            &title,
        ) {
            presence.found = true;
            return 0;
        }
        1
    }

    /// Cheap read-only "is there anything worth claiming yet?" probe.
    ///
    /// Answers the same `existing_window_identity_allowed` gate as the real scan
    /// -- skipping only the title read where the gate provably ignores it -- and
    /// then stops: no `OpenProcess`, no image-path canonicalization, and above
    /// all no Authenticode verification of every installed channel executable.
    /// The trusted claim below still proves all of that before touching
    /// anything; this only decides whether it is worth starting.
    ///
    /// That distinction is the whole point. The relaunch wait used to run the
    /// full trusted scan on every poll, so the interval between the relaunched
    /// client's own first show and OSL adopting it was one entire signature
    /// verification pass -- during which the operator is looking at a plain,
    /// taskbar-listed, unowned Discord. Gating on this probe collapses that to
    /// one poll period plus a single claim.
    /// Read-only answer to "would a takeover have to quit something?".
    ///
    /// Backs [`NativeWindowHostState::takeover_requires_consent`]. Reuses the
    /// cheap presence probe below, so it opens no process, verifies no
    /// signature, reads no profile and mutates nothing.
    pub(super) fn existing_client_is_running(id: NativeAppId) -> bool {
        existing_session_supported(id) && unsafe { existing_candidate_window_present(id) }
    }

    unsafe fn existing_candidate_window_present(id: NativeAppId) -> bool {
        let mut presence = ExistingWindowPresence { id, found: false };
        EnumWindows(
            Some(enum_existing_window_presence),
            (&mut presence as *mut ExistingWindowPresence) as LPARAM,
        );
        presence.found
    }

    /// Wait for a client OSL just relaunched to publish a claimable window, then
    /// adopt it as soon as it exists.
    ///
    /// Shared by every relaunch route so none of them can drift back to polling
    /// the expensive scan on a desktop that has no candidate window yet.
    unsafe fn wait_for_relaunched_existing_host(
        generation: u64,
        id: NativeAppId,
        owner_namespace: &str,
        parent: HWND,
        deadline: Instant,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        let mut last = NativeWindowHostReason::ExistingSessionUnavailable;
        loop {
            if existing_candidate_window_present(id) {
                match claim_existing_host(
                    generation,
                    id,
                    owner_namespace.to_owned(),
                    parent,
                    // Only ever reached after OSL itself started the client.
                    HostWindowOwnership::Spawned,
                ) {
                    Ok(hosted) => return Ok(hosted),
                    // Ambiguity is a decision, not a transient state: more
                    // polling cannot make a second window go away, and adopting
                    // under ambiguity is exactly what must never happen.
                    Err(NativeWindowHostReason::ExistingSessionAmbiguous) => {
                        return Err(NativeWindowHostReason::ExistingSessionAmbiguous)
                    }
                    Err(reason) => last = reason,
                }
            }
            if Instant::now() >= deadline {
                return Err(last);
            }
            thread::sleep(EXISTING_SESSION_PRESENCE_POLL);
        }
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
            if hosted.attached
                && parent != 0
                && hosted.id == NativeAppId::Discord
                && hosted.mode == DiscordSessionMode::ExistingSession
            {
                // Preserve the exact borrowed lease long enough for the
                // caller's bounded retry and diagnostic path to distinguish a
                // transient tether/presentation problem from process identity
                // loss. A fresh host request still tears down and reclaims an
                // invalid lease through the normal cold path.
                return NativeWindowHostResult::failed(
                    id,
                    NativeWindowHostReason::WindowIdentityChanged,
                );
            }
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
            let retryable_discord_presentation = hosted.id == NativeAppId::Discord
                && hosted.mode == DiscordSessionMode::ExistingSession
                && hosted
                    .borrowed_tether
                    .as_ref()
                    .is_some_and(|tether| tether.is_healthy(hosted.generation));
            let _ = hosted;
            if retryable_discord_presentation {
                // Keep the exact signed Discord lease while its tether retries
                // a transient Electron/desktop presentation refresh. A later
                // resize must still revalidate and realign before reporting
                // success; no stale HWND or process identity is accepted.
                return NativeWindowHostResult::failed(
                    id,
                    NativeWindowHostReason::WindowOperationRejected,
                );
            }
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
                    |tether| {
                        // `focus` runs on OSL's own UI thread.
                        tether.reconcile(
                            hosted.generation,
                            borrowed_tether_reconcile_budget(BorrowedTetherCaller::UiThread),
                        )
                    },
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
                        |tether| {
                            tether.reconcile(
                                hosted.generation,
                                borrowed_tether_reconcile_budget(BorrowedTetherCaller::UiThread),
                            )
                        },
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

    /// Single-slot QA host-stage labels for the application-exit teardown.
    const EXIT_HOST_SLOT_UNAVAILABLE: &str = "exit_host_slot_unavailable";
    const EXIT_RESTORE_APPLIED: &str = "exit_restore_applied";
    const EXIT_RESTORE_FAILED: &str = "exit_restore_failed_guardian_retained";
    const EXIT_CLOSE_LANDED: &str = "exit_close_landed";
    const EXIT_CLOSE_PENDING: &str = "exit_close_not_acknowledged";
    /// A close OSL promised would happen did not: the recovery guardian is
    /// deliberately left armed so it happens when this process dies.
    const EXIT_SPAWNED_CLOSE_PENDING: &str = "exit_spawned_close_pending_guardian_retained";

    /// Bounded, exit-only acquisition of the host slot.
    ///
    /// `Err(())` means the slot could not be claimed inside the budget, which
    /// is a safe outcome rather than a broken one: the armed recovery guardian
    /// still restores the borrowed window when this process dies. A poisoned
    /// lock is recovered rather than refused -- the alternative is leaving the
    /// operator's window adopted because some unrelated thread panicked.
    fn take_hosted_for_exit(
        state: &NativeWindowHostState,
        budget: Duration,
    ) -> Result<Option<HostedWindow>, ()> {
        let started = Instant::now();
        loop {
            match state.inner.try_lock() {
                Ok(mut guard) => return Ok(guard.take()),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                    return Ok(poisoned.into_inner().take())
                }
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
            if deadline_reached(started.elapsed(), budget) {
                return Err(());
            }
            thread::sleep(HARNESSED_EXIT_POLL);
        }
    }

    pub(super) fn shutdown_with_app(state: &NativeWindowHostState) -> NativeWindowHostResult {
        let hosted = match take_hosted_for_exit(state, HARNESSED_EXIT_LOCK_BUDGET) {
            Err(()) => {
                qa_discord_host_stage(EXIT_HOST_SLOT_UNAVAILABLE);
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::HostWindowUnavailable,
                );
            }
            Ok(None) => {
                return NativeWindowHostResult::failed(
                    NativeAppId::Discord,
                    NativeWindowHostReason::NotHosted,
                )
            }
            Ok(Some(hosted)) => hosted,
        };
        let id = hosted.id;
        let mode = hosted.mode;
        unsafe { close_hosted_with_app(hosted) };
        NativeWindowHostResult::success(id, NativeWindowHostStatus::Detached, mode)
    }

    /// Restore, then ask the harnessed window to close, then (only for an
    /// OSL-launched client) stop the process OSL itself started.
    ///
    /// Order is the whole point. Restoring first means every later step can
    /// fail, hang, or be refused and still leave the operator an ordinary,
    /// taskbar-listed, self-owned window. Closing first would open a window of
    /// time in which the client is tool-styled and owner-linked to a frame
    /// that is already being torn down.
    unsafe fn close_hosted_with_app(mut hosted: HostedWindow) {
        let plan = harnessed_exit_plan(hosted.mode, hosted.ownership);
        let requires_close = harnessed_exit_requires_close(plan);
        let window = hosted.window as HWND;
        let window_process_id = hosted.window_process_id;
        hosted.borrowed_tether.take();
        hosted.borrowed_control_shield.take();
        // `restore_window` cancels the recovery guardian only after the
        // restore has verified against the captured snapshot. A restore that
        // does not verify therefore leaves the guardian subprocess alive and
        // still waiting on this process handle, which is exactly the fallback
        // wanted here.
        //
        // For a window OSL spawned there is a second reason to keep it: the
        // guardian is also the only thing that can still close the window if the
        // posted close below is refused, so it is retained across a *successful*
        // restore too and stood down only once the close has landed.
        let restored = if requires_close && hosted.mode == DiscordSessionMode::ExistingSession {
            hosted
                .borrowed_recovery_guardian
                .as_ref()
                .is_some_and(|guardian| guardian.restore_retaining())
        } else {
            restore_window(&mut hosted)
        };
        qa_discord_host_stage(if restored {
            EXIT_RESTORE_APPLIED
        } else {
            EXIT_RESTORE_FAILED
        });
        let closed = request_graceful_close(window, window_process_id)
            && wait_for_harnessed_close(window, window_process_id, HARNESSED_CLOSE_BUDGET);
        if closed {
            // The promise has been kept; nothing is left for the guardian to do.
            if let Some(guardian) = hosted.borrowed_recovery_guardian.take() {
                guardian.cancel_after_verified_restore();
            }
        }
        qa_discord_host_stage(match (closed, requires_close) {
            (true, _) => EXIT_CLOSE_LANDED,
            (false, true) => EXIT_SPAWNED_CLOSE_PENDING,
            (false, false) => EXIT_CLOSE_PENDING,
        });
        if harnessed_exit_plan_stops_owned_process(plan) {
            // The graceful request came first; this only guarantees that no
            // OSL-spawned client survives the hub, which the kill-on-job-close
            // job object already promised even on a crash.
            wait_for_owned_exit(&mut hosted.process, HARNESSED_OWNED_EXIT_BUDGET);
            stop_owned_process(&mut hosted.process);
        }
    }

    /// Ask the harnessed window to close the way its own title-bar X would.
    ///
    /// `PostMessageW`, never `SendMessage*`: a posted `WM_CLOSE` cannot block
    /// OSL's shutdown behind a hung message pump or a modal "unsaved changes"
    /// dialog, while the client still gets to run its full close handler. The
    /// process is never terminated and no data is destroyed.
    ///
    /// The window is re-resolved to the harnessed process id immediately
    /// before the post, so a recycled handle can never receive this message.
    unsafe fn request_graceful_close(window: HWND, expected_process_id: u32) -> bool {
        if window.is_null() || window_process_id(window) != Some(expected_process_id) {
            return false;
        }
        PostMessageW(window, WM_CLOSE, 0, 0) != 0
    }

    /// Bounded poll for the close taking effect. Never blocks on the target.
    unsafe fn wait_for_harnessed_close(
        window: HWND,
        expected_process_id: u32,
        budget: Duration,
    ) -> bool {
        let started = Instant::now();
        loop {
            let still_ours = window_process_id(window) == Some(expected_process_id);
            let visible = still_ours && IsWindowVisible(window) != 0;
            if harnessed_close_landed(still_ours, visible) {
                return true;
            }
            if deadline_reached(started.elapsed(), budget) {
                return false;
            }
            thread::sleep(HARNESSED_EXIT_POLL);
        }
    }

    /// Bounded wait for an OSL-launched client to exit on its own after the
    /// graceful request, so its ordinary shutdown work completes before the
    /// job-object backstop runs. Borrowed clients are not waited on at all.
    fn wait_for_owned_exit(process: &mut HostedProcess, budget: Duration) -> bool {
        let HostedProcess::Dedicated { child, .. } = process else {
            return false;
        };
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) => {}
                Err(_) => return false,
            }
            if deadline_reached(started.elapsed(), budget) {
                return false;
            }
            thread::sleep(HARNESSED_EXIT_POLL);
        }
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
                arguments: vec![
                    OsString::from(DISCORD_ACCESSIBILITY_ARGUMENT),
                    OsString::from(DISCORD_UIA_PROVIDER_ARGUMENT),
                ],
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
        discord_channel_executables(install_root, executable_name)
            .into_iter()
            .next()
    }

    fn discord_channel_executables(install_root: &Path, executable_name: &str) -> Vec<PathBuf> {
        let mut candidates = fs::read_dir(install_root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let version = discord_version_key(name.to_str()?)?;
                let executable = entry.path().join(executable_name);
                executable.is_file().then_some((version, executable))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.0.cmp(&left.0));
        candidates
            .into_iter()
            .map(|(_, executable)| executable)
            .collect()
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

    /// Take a window off screen for the duration of an adoption mutation.
    ///
    /// The shell decides whether a window gets a taskbar button when that window
    /// is *shown*, not when its styles change. Setting `WS_EX_TOOLWINDOW` and the
    /// `GWLP_HWNDPARENT` owner link on a window that is already visible therefore
    /// leaves the button the shell handed out at the client's own first show
    /// sitting there for the rest of the session: afterwards the extended styles
    /// read exactly right (`WS_EX_TOOLWINDOW` set, `WS_EX_APPWINDOW` clear, owner
    /// == the OSL frame) while the operator still sees a separate, independent
    /// Discord beside OSL. That is precisely the first-launch report.
    ///
    /// Hiding first makes the single presentation that follows the first show the
    /// shell evaluates, by which point the window is both owned and tool-styled,
    /// so no taskbar button is ever created and the first thing the operator sees
    /// is a window already sitting inside the OSL frame.
    ///
    /// Returns whether the window is actually hidden now. A window that refuses
    /// to hide is still adopted -- it just keeps whatever taskbar button it
    /// already had -- because failing the adoption over presentation polish would
    /// leave the operator worse off than the defect does.
    ///
    /// This runs exactly once, during adoption. Nothing else in this module may
    /// conceal a window the operator is already using: the continuous tether only
    /// ever *reveals* (`BorrowedTetherRepairPlan::reveal`), never hides.
    unsafe fn conceal_for_adoption(window: HWND) -> bool {
        ShowWindow(window, SW_HIDE);
        IsWindowVisible(window) == 0
    }

    /// Put a concealed borrowed window back on screen after an adoption that did
    /// not complete.
    ///
    /// `restore_guardian_snapshot` already replays the captured
    /// `WINDOWPLACEMENT`, which shows the window in every case it verifies. This
    /// is the belt-and-braces path for the case where it does not: a Discord the
    /// operator can neither see nor reach is a far worse outcome than one whose
    /// owner or extended styles were only partly put back, so visibility is
    /// restored unconditionally here and the failure is still reported upwards.
    unsafe fn reveal_after_failed_adoption(snapshot: &BorrowedRecoverySnapshot) {
        let window = snapshot.window as HWND;
        if IsWindowVisible(window) != 0 {
            return;
        }
        restore_borrowed_window(window, snapshot.placement);
        if IsWindowVisible(window) == 0 {
            ShowWindow(window, SW_SHOW);
        }
    }

    /// Unwind an adoption that failed after the recovery guardian was armed,
    /// never leaving the operator's own client half-adopted or off screen.
    unsafe fn abandon_adoption(
        concealed: bool,
        snapshot: &BorrowedRecoverySnapshot,
        guardian: BorrowedRecoveryGuardian,
    ) {
        if restore_guardian_snapshot(snapshot) {
            guardian.cancel_after_verified_restore();
        }
        if concealed {
            reveal_after_failed_adoption(snapshot);
        }
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
        // OSL launched and owns this process, and its window is about to become
        // a `WS_CHILD` of the OSL frame. Take it off screen first so the style
        // rewrite, `SetParent`, and alignment never run on a window the operator
        // can see as an independent app; `align_to_parent` inside
        // `attach_child_window` shows it again with `SWP_SHOWWINDOW`, by which
        // point it is already a correctly positioned child of OSL and can no
        // longer be given a taskbar button. Discord only -- every other
        // dedicated guest keeps its existing presentation behaviour untouched.
        // `single_visible_top_level_is_target` above has already run against the
        // still-visible window, so no identity gate is weakened by this.
        if id == NativeAppId::Discord {
            ShowWindow(window, SW_HIDE);
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
            // A dedicated guest is OSL-launched by construction.
            ownership: HostWindowOwnership::Spawned,
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
        ownership: HostWindowOwnership,
        process_id: u32,
        creation_time: u64,
        session_id: u32,
        process: ProcessHandle,
        trusted_window_executable: TrustedWindowExecutable,
        window: HWND,
        owner: HWND,
    ) -> Result<HostedWindow, NativeWindowHostReason> {
        if id == NativeAppId::Discord {
            qa_discord_host_stage("adopt_started");
        }
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
            // Armed before a single style or owner bit is touched, so the
            // spawned-vs-borrowed contract survives a crash from here onwards --
            // not merely a clean exit.
            disposition: guardian_disposition(ownership),
        };
        let recovery_guardian = BorrowedRecoveryGuardian::arm(&recovery_snapshot)
            .ok_or(NativeWindowHostReason::WindowOperationRejected)?;
        if id == NativeAppId::Discord {
            qa_discord_host_stage("adopt_guardian_armed");
        }
        debug_assert!(borrowed_mutation_transition(
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed,
        ));
        // Conceal before any owner or style mutation, and only after the
        // guardian is armed, so a crash anywhere below still hands the operator
        // their window back. Discord only: it is the single borrowed client this
        // path owner-links, and the only one whose taskbar absence is a shipped
        // criterion. Every other borrowed client keeps its exact existing
        // visible-mutation behaviour.
        let concealed = id == NativeAppId::Discord && conceal_for_adoption(window);
        if id == NativeAppId::Discord {
            qa_discord_host_stage(if concealed {
                "adopt_concealed"
            } else {
                "adopt_conceal_unavailable"
            });
            debug_assert!(borrowed_mutation_transition(
                BorrowedMutationStage::GuardianArmed,
                BorrowedMutationStage::Concealed,
            ));
        }
        let task_ex_style = borrowed_task_ex_style(
            previous_ex_style,
            WS_EX_APPWINDOW as isize,
            WS_EX_TOOLWINDOW as isize,
        );
        if id == NativeAppId::Discord {
            SetWindowLongPtrW(window, GWLP_HWNDPARENT, attached_owner);
            debug_assert!(borrowed_mutation_transition(
                if concealed {
                    BorrowedMutationStage::Concealed
                } else {
                    BorrowedMutationStage::GuardianArmed
                },
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
            && borrowed_concealed_style_is_preserved(
                previous_style,
                GetWindowLongPtrW(window, GWL_STYLE),
                WS_VISIBLE as isize,
                concealed,
            )
            && borrowed_owner_contract_unchanged(
                attached_owner,
                GetWindowLongPtrW(window, GWLP_HWNDPARENT),
            );
        if !style_applied {
            abandon_adoption(concealed, &recovery_snapshot, recovery_guardian);
            return Err(NativeWindowHostReason::BorrowedStyleRejected);
        }
        if id == NativeAppId::Discord {
            debug_assert!(borrowed_mutation_transition(
                BorrowedMutationStage::OwnerLinked,
                BorrowedMutationStage::TaskStyleApplied,
            ));
        }
        if previous_iconic && !concealed {
            ShowWindow(window, SW_RESTORE);
        }
        if concealed {
            // Land the hidden window on the exact rect the presentation below is
            // going to verify, so the first show paints it already inside OSL's
            // frame instead of flashing once at its old desktop position.
            // `SWP_SHOWWINDOW` is deliberately absent: this must not be the show.
            if let Some(target) = parent_target_rect(owner) {
                let _ = SetWindowPos(
                    window,
                    std::ptr::null_mut(),
                    target.left,
                    target.top,
                    target.right - target.left,
                    target.bottom - target.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
        }
        if let Err(reason) = present_borrowed_window(id, window, owner) {
            abandon_adoption(concealed, &recovery_snapshot, recovery_guardian);
            return Err(reason);
        }
        if concealed {
            // A hide/show cycle drives Chromium's native occlusion tracking, and
            // Chromium can leave its compositor surface child parked off the
            // client rect afterwards -- the window then reads as solid black even
            // though its renderer is alive. This is the same idempotent
            // repositioning the tether's restore repair depends on; against a
            // healthy surface it resolves to a no-op move.
            let _ = realign_borrowed_compositor_surface(window);
        }
        if id == NativeAppId::Discord {
            qa_discord_host_stage("adopt_presented");
        }
        let Some(borrowed_control_shield) = BorrowedControlShield::create(window, process_id)
            .filter(BorrowedControlShield::position)
        else {
            abandon_adoption(concealed, &recovery_snapshot, recovery_guardian);
            return Err(NativeWindowHostReason::WindowOperationRejected);
        };
        if id == NativeAppId::Discord {
            qa_discord_host_stage("adopt_shield_ready");
        }
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
                abandon_adoption(concealed, &recovery_snapshot, recovery_guardian);
                return Err(NativeWindowHostReason::WindowOperationRejected);
            };
            Some(tether)
        } else {
            None
        };
        if id == NativeAppId::Discord {
            qa_discord_host_stage("adopt_tether_ready");
        }
        Ok(HostedWindow {
            generation,
            id,
            mode: DiscordSessionMode::ExistingSession,
            ownership,
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
        if GetWindowDisplayAffinity(parent, &mut affinity) == 0 {
            return false;
        }
        #[cfg(feature = "discord-qa-shell")]
        {
            // The disposable visual-QA build deliberately permits capture so
            // the no-focus harness can inspect it. Exact HWND ownership and
            // top-level identity remain mandatory above; production retains
            // the capture-exclusion requirement below.
            affinity == 0 || affinity == WDA_EXCLUDEFROMCAPTURE
        }
        #[cfg(not(feature = "discord-qa-shell"))]
        {
            affinity == WDA_EXCLUDEFROMCAPTURE
        }
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
            // The host runs on a blocking worker while the HWND belongs to
            // Tauri's UI thread. A synchronous restore can wait on that thread
            // and deadlock the native claim before Discord discovery starts.
            // Queue the restore and verify its result in the bounded loop.
            ShowWindowAsync(parent, SW_RESTORE);
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
            if hosted.id == NativeAppId::Discord {
                qa_discord_host_stage("realign_parent_changed");
            }
            return false;
        }
        if let Some(tether) = hosted.borrowed_tether.as_ref() {
            // `host` and `resize` both reach this from OSL's own UI thread, and
            // a refusal here keeps the borrowed lease for a bounded retry.
            let report = tether.reconcile_reported(
                hosted.generation,
                borrowed_tether_reconcile_budget(BorrowedTetherCaller::UiThread),
            );
            if report.aligned {
                if hosted.id == NativeAppId::Discord {
                    qa_discord_host_stage("realign_tether_ready");
                }
                hosted.last_aligned_rect = parent_target_rect(parent).map(rect_array);
                let shield_ready = hosted
                    .borrowed_control_shield
                    .as_ref()
                    .is_some_and(BorrowedControlShield::position);
                if hosted.id == NativeAppId::Discord {
                    qa_discord_host_stage(if shield_ready {
                        "realign_shield_ready"
                    } else {
                        "realign_shield_failed"
                    });
                }
                return shield_ready;
            }
            if hosted.id == NativeAppId::Discord {
                // One fixed label per refusing branch, with the historical
                // catch-all retained so no path can become silent.
                qa_discord_host_stage(report.stage());
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

    fn hosted_window_validation_error(hosted: &HostedWindow) -> Option<&'static str> {
        if unsafe { window_process_id(hosted.window as HWND) } != Some(hosted.window_process_id) {
            return Some("The trusted native Discord window PID changed");
        }
        if hosted.mode == DiscordSessionMode::ExistingSession {
            if hosted.trusted_parent == 0 {
                return Some("The trusted native Discord parent is unavailable");
            }
            if hosted.id == NativeAppId::Discord {
                // Electron may clear its Win32 owner during an internal
                // presentation refresh. The tether still proves the exact
                // HWND/PID/creation-time/path/session plus the exact OSL parent
                // and repairs geometry continuously, so a cleared owner is a
                // presentation quirk rather than an identity change.
                if !hosted
                    .borrowed_tether
                    .as_ref()
                    .is_some_and(|tether| tether.is_healthy(hosted.generation))
                {
                    return Some("The trusted native Discord tether is unavailable");
                }
            } else if unsafe {
                GetWindowLongPtrW(hosted.window as HWND, GWLP_HWNDPARENT) != hosted.previous_owner
            } {
                return Some("The trusted borrowed native owner changed");
            }
        }
        let mut osl_session = 0u32;
        let mut hosted_session = 0u32;
        if unsafe {
            ProcessIdToSessionId(std::process::id(), &mut osl_session) == 0
                || ProcessIdToSessionId(hosted.window_process_id, &mut hosted_session) == 0
        } || osl_session != hosted_session
        {
            return Some("The trusted native Discord Windows session changed");
        }
        let process_valid = match &hosted.process {
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
        };
        if !process_valid {
            return Some("The trusted native Discord process identity changed");
        }
        None
    }

    fn hosted_window_is_valid(hosted: &HostedWindow) -> bool {
        hosted_window_validation_error(hosted).is_none()
    }

    pub(super) fn current_discord_service_host(
        state: &NativeWindowHostState,
        owner_osl_user_id: &str,
    ) -> Result<crate::service_host::ActiveServiceHost, String> {
        qa_discord_host_stage("context_host_entered");
        let requested_owner = crate::service_host::owner_profile_namespace(owner_osl_user_id)
            .map_err(|_| "The trusted native Discord owner is unavailable".to_owned())?;
        let guard = state
            .inner
            .lock()
            .map_err(|_| "The trusted native Discord host is unavailable".to_owned())?;
        qa_discord_host_stage("context_host_lock_acquired");
        let hosted = guard
            .as_ref()
            .ok_or_else(|| "The trusted native Discord host state is missing".to_owned())?;
        if !native_context_matches(
            hosted.attached,
            hosted.id,
            &hosted.owner_namespace,
            &requested_owner,
        ) {
            return Err("The trusted native Discord owner binding is unavailable".to_owned());
        }
        qa_discord_host_stage("context_host_owner_matched");
        if let Some(error) = hosted_window_validation_error(hosted) {
            return Err(error.to_owned());
        }
        qa_discord_host_stage("context_host_validated");
        let account_id = native_discord_account_id(&requested_owner)
            .ok_or_else(|| "The trusted native Discord account is unavailable".to_owned())?;
        qa_discord_host_stage("context_host_complete");
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
        // Only one Discord accessibility operation may be in flight, exactly as
        // before. It is enforced by a non-blocking gate rather than by holding
        // `inner` across the operation, because the operation performs
        // synchronous cross-process accessibility calls with no timeout that wait
        // on Discord's UI thread, while OSL's UI thread needs `inner` every
        // second and Discord's UI thread cannot make progress once OSL's stops
        // pumping. Nothing waits on this gate, so it cannot close such a cycle.
        let Some(_operation_gate) = DiscordAccessibilityOperationGate::acquire(state) else {
            qa_discord_host_stage("place_refused_operation_in_flight");
            return Err(
                "A trusted native Discord accessibility operation is already in flight".to_owned(),
            );
        };
        let facts = {
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
            let mut borrowed_session = 0u32;
            if matches!(hosted.process, HostedProcess::Borrowed { .. })
                && unsafe { ProcessIdToSessionId(hosted.window_process_id, &mut borrowed_session) }
                    == 0
            {
                return Err("The trusted native Discord process session is unavailable".to_owned());
            }
            // The host's own predicate, unchanged, asked under the lock about
            // exactly the one process id the operation is allowed to touch. It is
            // the same job-membership / session-and-path / Authenticode chain the
            // borrowed predicate ran; it is simply answered once, here, instead of
            // lazily from inside the operation.
            let target_process_trusted = match &hosted.process {
                HostedProcess::Dedicated { job, .. } => {
                    trusted_job_process_path(job, hosted.window_process_id).is_some_and(|path| {
                        verify_executable(&path, ExecutablePublisher::Discord)
                            .is_ok_and(|trusted| trusted.path() == path)
                    })
                }
                HostedProcess::Borrowed { .. } => {
                    process_path_in_session(hosted.window_process_id, borrowed_session).is_some_and(
                        |path| {
                            path == hosted.trusted_window_executable.path()
                                && verify_executable(&path, ExecutablePublisher::Discord)
                                    .is_ok_and(|trusted| trusted.path() == path)
                        },
                    )
                }
            };
            LockedDiscordHostFacts::copy_from_locked(
                hosted.generation,
                hosted.window,
                hosted.window_process_id,
                target_process_trusted,
            )
            .ok_or_else(|| "The trusted native Discord host is unavailable".to_owned())?
        };
        // `inner` is released here, before any cross-process work begins.
        qa_discord_host_stage("place_lock_released_for_operation");
        let target = facts.target();
        let process_is_trusted = facts.pinned_process_trust();
        let result = operation(target, &process_is_trusted)?;
        // The operation ran with no host lock held, so the host could have been
        // detached, torn down, or re-hosted underneath it. `place` re-proves the
        // exact window handle, its owning process id, foreground identity and
        // composer binding from inside, but it cannot see the host slot itself:
        // a generation bump, an owner-namespace rebind, or a tether/publisher
        // failure are only visible here. Re-prove all of them and fail closed,
        // so no result from a superseded host is ever returned to the caller.
        let guard = state
            .inner
            .lock()
            .map_err(|_| "The trusted native Discord window changed".to_owned())?;
        let Some(hosted) = guard.as_ref().filter(|hosted| {
            native_context_matches(
                hosted.attached,
                hosted.id,
                &hosted.owner_namespace,
                &requested_owner,
            ) && hosted_window_is_valid(hosted)
        }) else {
            qa_discord_host_stage("place_state_changed_during_operation");
            return Err("The trusted native Discord window changed".to_owned());
        };
        if !facts.still_describes(hosted.generation, hosted.window, hosted.window_process_id) {
            qa_discord_host_stage("place_writeback_rejected_stale");
            return Err("The trusted native Discord window changed".to_owned());
        }
        Ok(result)
    }

    pub(super) fn discord_overlay_target(
        state: &NativeWindowHostState,
        owner_osl_user_id: &str,
    ) -> Result<NativeDiscordOverlayTarget, String> {
        qa_discord_host_stage("overlay_target_entered");
        let requested_owner = crate::service_host::owner_profile_namespace(owner_osl_user_id)
            .map_err(|_| "The trusted native Discord owner is unavailable".to_owned())?;
        let guard = state
            .inner
            .lock()
            .map_err(|_| "The trusted native Discord window is unavailable".to_owned())?;
        qa_discord_host_stage("overlay_target_lock_acquired");
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
        qa_discord_host_stage("overlay_target_host_validated");
        if matches!(hosted.process, HostedProcess::Borrowed { .. }) {
            let presentation_ready = hosted
                .borrowed_tether
                .as_ref()
                .is_some_and(|tether| {
                    // The protected-overlay guard thread reaches the tether here,
                    // and a refusal tears the protected session down, so it waits
                    // out the longer budget instead of failing on a slow frame.
                    tether.reconcile(
                        hosted.generation,
                        borrowed_tether_reconcile_budget(BorrowedTetherCaller::ProtectionGuard),
                    )
                })
                && hosted
                    .borrowed_control_shield
                    .as_ref()
                    .is_some_and(BorrowedControlShield::is_healthy);
            if !presentation_ready {
                return Err(
                    "The trusted native Discord window presentation is unavailable".to_owned(),
                );
            }
        }
        qa_discord_host_stage("overlay_target_presentation_reconciled");
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
        qa_discord_host_stage("overlay_target_geometry_validated");
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_root = if foreground.is_null() {
            std::ptr::null_mut()
        } else {
            unsafe { GetAncestor(foreground, GA_ROOT) }
        };
        let target_root = unsafe { GetAncestor(window, GA_ROOT) };
        qa_discord_host_stage("overlay_target_complete");
        Ok(NativeDiscordOverlayTarget {
            generation: hosted.generation,
            window: hosted.window,
            rect: [rect.left, rect.top, rect.right, rect.bottom],
            foreground: !foreground_root.is_null() && foreground_root == target_root,
            trusted_parent: hosted.trusted_parent,
        })
    }

    pub(super) fn discord_accessibility_snapshot(
        state: &NativeWindowHostState,
    ) -> crate::native_discord_adapter::NativeDiscordAccessibilitySnapshot {
        use crate::native_discord_adapter::{
            DiscordSnapshotReason, NativeDiscordAccessibilitySnapshot,
        };

        // Identical rule to `with_current_discord_accessibility_target`: the
        // cross-process snapshot must not run while `inner` is held.
        let Some(_operation_gate) = DiscordAccessibilityOperationGate::acquire(state) else {
            qa_discord_host_stage("snapshot_refused_operation_in_flight");
            return NativeDiscordAccessibilitySnapshot::unavailable(
                0,
                DiscordSnapshotReason::NotHosted,
            );
        };
        let facts = {
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
            let target_process_trusted = match &hosted.process {
                HostedProcess::Dedicated { job, .. } => {
                    trusted_job_process_path(job, hosted.window_process_id).is_some_and(|path| {
                        verify_executable(&path, ExecutablePublisher::Discord)
                            .is_ok_and(|trusted| trusted.path() == path)
                    })
                }
                HostedProcess::Borrowed {
                    process_id: borrowed_pid,
                    creation_time,
                    process,
                } => {
                    borrowed_snapshot_pid_matches(*borrowed_pid, hosted.window_process_id)
                        && borrowed_process_is_valid(
                            *borrowed_pid,
                            *creation_time,
                            process,
                            hosted.trusted_window_executable.path(),
                            NativeAppId::Discord,
                        )
                }
            };
            match LockedDiscordHostFacts::copy_from_locked(
                hosted.generation,
                hosted.window,
                hosted.window_process_id,
                target_process_trusted,
            ) {
                Some(facts) => facts,
                None => {
                    return NativeDiscordAccessibilitySnapshot::unavailable(
                        hosted.generation,
                        DiscordSnapshotReason::HostIdentityChanged,
                    )
                }
            }
        };
        // `inner` is released here, before any cross-process work begins.
        qa_discord_host_stage("snapshot_lock_released_for_operation");
        let process_is_trusted = facts.pinned_process_trust();
        let snapshot = crate::native_discord_adapter::snapshot_claimed_window(
            facts.target(),
            &process_is_trusted,
        );
        let guard = match state.inner.lock() {
            Ok(guard) => guard,
            Err(_) => {
                return NativeDiscordAccessibilitySnapshot::unavailable(
                    facts.generation,
                    DiscordSnapshotReason::HostIdentityChanged,
                )
            }
        };
        let Some(hosted) = guard.as_ref() else {
            qa_discord_host_stage("snapshot_state_changed_during_operation");
            return NativeDiscordAccessibilitySnapshot::unavailable(
                facts.generation,
                DiscordSnapshotReason::HostIdentityChanged,
            );
        };
        if hosted.id != NativeAppId::Discord
            || !hosted_window_is_valid(hosted)
            || !facts.still_describes(hosted.generation, hosted.window, hosted.window_process_id)
            || hosted.generation != snapshot.generation
        {
            qa_discord_host_stage("snapshot_state_changed_during_operation");
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
        // A window OSL spawned does not outlive the host that spawned it, and
        // this is the teardown that `Drop for NativeWindowHostState` reaches --
        // an exit that never went through `shutdown_with_app` would otherwise
        // restore the guardian away and leave the client running forever.
        //
        // Restore already ran, so a refused close leaves an ordinary,
        // taskbar-listed window rather than an orphan, and this is still a
        // posted `WM_CLOSE`: the operator's own client is never terminated and
        // none of its data is touched.
        if teardown_closes_spawned_window(hosted.mode, hosted.ownership) {
            let _ = request_graceful_close(hosted.window as HWND, hosted.window_process_id);
        }
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
    fn application_exit_asks_both_modes_to_close_and_kills_only_an_osl_launched_client() {
        // The owner's request: the harnessed window goes away with OSL in both
        // adoption modes. The trade-off is that a *borrowed* `ExistingSession`
        // closes a client the operator started themselves, so it is only ever
        // *asked*.
        assert_eq!(
            harnessed_exit_plan(
                DiscordSessionMode::ExistingSession,
                HostWindowOwnership::Borrowed
            ),
            HarnessedExitPlan::RestoreThenAskToClose
        );
        assert_eq!(
            harnessed_exit_plan(DiscordSessionMode::Dedicated, HostWindowOwnership::Spawned),
            HarnessedExitPlan::RestoreThenAskToCloseThenStopOwnedProcess
        );
        assert!(!harnessed_exit_plan_stops_owned_process(
            harnessed_exit_plan(
                DiscordSessionMode::ExistingSession,
                HostWindowOwnership::Borrowed
            )
        ));
        assert!(harnessed_exit_plan_stops_owned_process(harnessed_exit_plan(
            DiscordSessionMode::Dedicated,
            HostWindowOwnership::Spawned
        )));
        // Whichever mode may stop a process is exactly the mode that owns one.
        for mode in [
            DiscordSessionMode::Dedicated,
            DiscordSessionMode::ExistingSession,
        ] {
            assert_eq!(
                harnessed_exit_plan_stops_owned_process(harnessed_exit_plan(
                    mode,
                    HostWindowOwnership::Borrowed
                )),
                mode_owns_process(mode)
            );
        }
    }

    #[test]
    fn a_window_osl_spawned_is_closed_on_exit_while_a_borrowed_one_is_only_asked() {
        // Option A's whole point: OSL quit the operator's Discord and started it
        // again itself, so "it goes away with OSL" is a promise, not a courtesy.
        let spawned = harnessed_exit_plan(
            DiscordSessionMode::ExistingSession,
            HostWindowOwnership::Spawned,
        );
        let borrowed = harnessed_exit_plan(
            DiscordSessionMode::ExistingSession,
            HostWindowOwnership::Borrowed,
        );
        assert_eq!(spawned, HarnessedExitPlan::RestoreThenCloseSpawnedClient);
        assert_ne!(spawned, borrowed);
        // A close that does not land is a broken promise only for the spawned
        // window, and that is what keeps the guardian armed on the way out.
        assert!(harnessed_exit_requires_close(spawned));
        assert!(!harnessed_exit_requires_close(borrowed));
        // ... and it is still never escalated to a process kill. Only the
        // dedicated guest, which lives in an OSL-owned profile inside an
        // OSL-owned job object, may ever be stopped.
        assert!(!harnessed_exit_plan_stops_owned_process(spawned));
        assert!(harnessed_exit_plan_stops_owned_process(
            harnessed_exit_plan(DiscordSessionMode::Dedicated, HostWindowOwnership::Spawned)
        ));

        // Teardown that never reaches the application-exit path (the host state
        // simply being dropped, or a stale host being replaced) must still close
        // a spawned window, or an OSL-started Discord outlives OSL forever.
        assert!(teardown_closes_spawned_window(
            DiscordSessionMode::ExistingSession,
            HostWindowOwnership::Spawned
        ));
        assert!(!teardown_closes_spawned_window(
            DiscordSessionMode::ExistingSession,
            HostWindowOwnership::Borrowed
        ));
        // The dedicated guest is excluded: its job object already guarantees it,
        // and termination rather than closing is what teardown owes it.
        assert!(!teardown_closes_spawned_window(
            DiscordSessionMode::Dedicated,
            HostWindowOwnership::Spawned
        ));
    }

    #[test]
    fn the_crash_guardian_restores_a_borrowed_window_and_closes_a_spawned_one() {
        assert_eq!(
            guardian_disposition(HostWindowOwnership::Borrowed),
            GuardianDisposition::Restore
        );
        assert_eq!(
            guardian_disposition(HostWindowOwnership::Spawned),
            GuardianDisposition::RestoreThenClose
        );
        assert!(!guardian_closes_after_restore(GuardianDisposition::Restore));
        assert!(guardian_closes_after_restore(
            GuardianDisposition::RestoreThenClose
        ));
        // The disposition crosses a process boundary as argv, so the encoding
        // has to round-trip exactly -- a mis-parse would silently downgrade a
        // spawned window's guardian to restore-only, which is the one failure
        // this whole mechanism exists to prevent.
        for disposition in [
            GuardianDisposition::Restore,
            GuardianDisposition::RestoreThenClose,
        ] {
            assert_eq!(
                parse_guardian_disposition(guardian_disposition_flag(disposition)),
                Some(disposition)
            );
        }
        // And an unrecognized or absent value is refused rather than defaulted,
        // so an older guardian binary can never be handed a newer snapshot and
        // guess at what to do with the operator's window.
        assert_eq!(parse_guardian_disposition(""), None);
        assert_eq!(parse_guardian_disposition("close"), None);
        assert_eq!(parse_guardian_disposition("Restore"), None);
    }

    #[test]
    fn a_takeover_only_relaunches_once_the_client_process_is_actually_gone() {
        // Nothing running: nothing to consent to, nothing to quit, and the
        // relaunch still produces a window OSL owns.
        assert_eq!(
            takeover_quit_outcome(false, false),
            TakeoverQuitOutcome::NothingToQuit
        );
        assert!(takeover_may_relaunch(TakeoverQuitOutcome::NothingToQuit));
        // The client exited: proceed.
        assert_eq!(
            takeover_quit_outcome(true, true),
            TakeoverQuitOutcome::Exited
        );
        assert!(takeover_may_relaunch(TakeoverQuitOutcome::Exited));
        // Still running after the budget -- Discord's close-to-tray setting is
        // the ordinary cause. OSL never escalates, so the takeover is abandoned.
        assert_eq!(
            takeover_quit_outcome(true, false),
            TakeoverQuitOutcome::StillRunning
        );
        assert!(!takeover_may_relaunch(TakeoverQuitOutcome::StillRunning));
        // This is deliberately stricter than the exit path's notion of "closed".
        // A hidden-but-running client counts as closed at OSL's own shutdown,
        // and must *not* count here: relaunching against a live instance hands
        // OSL back the same window through Discord's single-instance handler.
        assert!(harnessed_close_landed(true, false));
        assert!(!takeover_may_relaunch(takeover_quit_outcome(true, false)));
    }

    #[test]
    fn a_takeover_is_refused_for_every_client_without_a_verified_quit_contract() {
        assert!(takeover_supported(
            NativeAppId::Discord,
            DiscordSessionMode::ExistingSession
        ));
        // A dedicated host launches into an OSL-owned profile and has nothing to
        // take over.
        assert!(!takeover_supported(
            NativeAppId::Discord,
            DiscordSessionMode::Dedicated
        ));
        for id in [
            NativeAppId::Telegram,
            NativeAppId::Signal,
            NativeAppId::Whatsapp,
            NativeAppId::Outlook,
        ] {
            assert!(!takeover_supported(id, DiscordSessionMode::ExistingSession));
            // ... and asking for one anyway degrades to the non-destructive
            // action rather than quitting something on a guess.
            assert_eq!(
                cold_host_action(
                    id,
                    DiscordSessionMode::ExistingSession,
                    DiscordTakeover::QuitAndRelaunch
                ),
                ColdHostAction::ClaimExisting
            );
        }
    }

    #[test]
    fn consent_is_a_receipt_the_caller_must_present_and_never_a_default() {
        // The permissive value is the default, so a caller that says nothing --
        // including an older UI that does not know this field exists -- gets
        // today's borrow and never quits anything the operator is using.
        assert_eq!(DiscordTakeover::default(), DiscordTakeover::BorrowExisting);
        assert_eq!(
            cold_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::ExistingSession,
                DiscordTakeover::default()
            ),
            ColdHostAction::ClaimExisting
        );
        // Refused consent is expressed the same way, so the refusal path is the
        // default path: adopt whatever is on screen.
        assert_eq!(
            cold_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::ExistingSession,
                DiscordTakeover::BorrowExisting
            ),
            ColdHostAction::ClaimExisting
        );
        // Only an explicit grant reaches the destructive action.
        assert_eq!(
            cold_host_action(
                NativeAppId::Discord,
                DiscordSessionMode::ExistingSession,
                DiscordTakeover::QuitAndRelaunch
            ),
            ColdHostAction::TakeOverExisting
        );
        // The wire form the UI will send, pinned so a rename cannot silently
        // turn a refusal into a grant.
        assert_eq!(
            serde_json::to_string(&DiscordTakeover::QuitAndRelaunch).unwrap(),
            "\"quitAndRelaunch\""
        );
        assert_eq!(
            serde_json::from_str::<DiscordTakeover>("\"borrowExisting\"").unwrap(),
            DiscordTakeover::BorrowExisting
        );
    }

    #[test]
    fn osl_relaunches_discord_without_letting_it_steal_the_operators_focus() {
        // `--start-inactive` maps to `mainWindow.showInactive()` in Discord's
        // core: the window is created and shown, but takes no activation, so the
        // concealed adoption can own and style it before it is ever presented.
        let arguments = existing_session_launch_arguments(NativeAppId::Discord);
        assert!(arguments.contains(&DISCORD_START_INACTIVE_ARGUMENT));
        // It is additive: the accessibility switches the whole adapter depends
        // on are still passed.
        assert!(arguments.contains(&DISCORD_ACCESSIBILITY_ARGUMENT));
        assert!(arguments.contains(&DISCORD_UIA_PROVIDER_ARGUMENT));
        // Discord is the only client whose argv vocabulary is established, so no
        // other client is handed invented switches.
        for id in [
            NativeAppId::Telegram,
            NativeAppId::Signal,
            NativeAppId::Whatsapp,
            NativeAppId::Outlook,
        ] {
            assert!(existing_session_launch_arguments(id).is_empty());
        }
    }

    #[test]
    fn a_destroyed_or_hidden_harnessed_window_both_count_as_closed() {
        // Destroyed, or the handle no longer resolves to the harnessed process.
        assert!(harnessed_close_landed(false, false));
        assert!(harnessed_close_landed(false, true));
        // Still ours but hidden: Discord's own close handler minimizes to the
        // tray rather than exiting, and that is a window the operator no
        // longer sees.
        assert!(harnessed_close_landed(true, false));
        // Still ours and still on screen: the close has not landed.
        assert!(!harnessed_close_landed(true, true));
    }

    #[test]
    fn application_exit_never_leaves_an_unreachable_window() {
        // The only unsafe combination: nothing restored it, it is still on
        // screen, and no guardian remains to put it back. Every other outcome
        // leaves the operator a reachable window.
        for restored in [false, true] {
            for closed in [false, true] {
                for guardian in [false, true] {
                    assert_eq!(
                        harnessed_exit_leaves_unreachable_window(restored, closed, guardian),
                        !restored && !closed && !guardian,
                        "restored={restored} closed={closed} guardian={guardian}"
                    );
                }
            }
        }
        // Stated positively, for the three real exit outcomes.
        assert!(!harnessed_exit_leaves_unreachable_window(true, true, false));
        assert!(!harnessed_exit_leaves_unreachable_window(true, false, false));
        assert!(!harnessed_exit_leaves_unreachable_window(false, false, true));
    }

    #[test]
    fn every_application_exit_wait_is_bounded() {
        for budget in [
            HARNESSED_CLOSE_BUDGET,
            HARNESSED_OWNED_EXIT_BUDGET,
            HARNESSED_EXIT_LOCK_BUDGET,
        ] {
            assert!(budget > Duration::ZERO);
            // Every wait must make progress in finitely many polls.
            assert!(HARNESSED_EXIT_POLL > Duration::ZERO);
            assert!(HARNESSED_EXIT_POLL < budget);
        }
        // Must stay comfortably below the unconditional 15s shutdown watchdog
        // armed by the main window's close handler.
        assert!(harnessed_exit_worst_case() <= Duration::from_secs(6));
    }

    #[test]
    fn exit_deadlines_expire_exactly_at_the_budget() {
        assert!(!deadline_reached(
            Duration::from_millis(0),
            HARNESSED_CLOSE_BUDGET
        ));
        assert!(!deadline_reached(
            HARNESSED_CLOSE_BUDGET - Duration::from_millis(1),
            HARNESSED_CLOSE_BUDGET
        ));
        assert!(deadline_reached(
            HARNESSED_CLOSE_BUDGET,
            HARNESSED_CLOSE_BUDGET
        ));
        assert!(deadline_reached(
            HARNESSED_CLOSE_BUDGET + Duration::from_secs(60),
            HARNESSED_CLOSE_BUDGET
        ));
    }

    #[test]
    fn locked_host_facts_copy_only_the_operation_inputs() {
        let facts = LockedDiscordHostFacts::copy_from_locked(7, 0x4321, 4242, true)
            .expect("a fully proven host must yield copied facts");
        assert_eq!(
            facts,
            LockedDiscordHostFacts {
                generation: 7,
                window: 0x4321,
                window_process_id: 4242,
                target_process_trusted: true,
            }
        );
    }

    #[test]
    fn locked_host_facts_refuse_unproven_identities() {
        assert!(LockedDiscordHostFacts::copy_from_locked(7, 0, 4242, true).is_none());
        assert!(LockedDiscordHostFacts::copy_from_locked(7, 0x4321, 0, true).is_none());
        assert!(LockedDiscordHostFacts::copy_from_locked(7, 0x4321, 4242, false).is_none());
    }

    #[test]
    fn pinned_trust_admits_exactly_one_process_id() {
        let facts = LockedDiscordHostFacts::copy_from_locked(7, 0x4321, 4242, true).unwrap();
        let trusted = facts.pinned_process_trust();
        assert!(trusted(4242));
        assert!(!trusted(0));
        assert!(!trusted(4241));
        assert!(!trusted(4243));
        assert!(!trusted(u32::MAX));
    }

    #[test]
    fn pinned_trust_admits_nothing_when_the_lock_answered_no() {
        let facts = LockedDiscordHostFacts {
            generation: 7,
            window: 0x4321,
            window_process_id: 4242,
            target_process_trusted: false,
        };
        let trusted = facts.pinned_process_trust();
        assert!(!trusted(4242));
        assert!(!trusted(0));
        assert!(!trusted(4243));
    }

    #[test]
    fn stale_host_state_rejects_the_operation_result() {
        let facts = LockedDiscordHostFacts::copy_from_locked(7, 0x4321, 4242, true).unwrap();
        assert!(facts.still_describes(7, 0x4321, 4242));
        // Generation advanced: the host was re-hosted mid-operation.
        assert!(!facts.still_describes(8, 0x4321, 4242));
        // A different window now occupies the host slot.
        assert!(!facts.still_describes(7, 0x8765, 4242));
        // Same handle value, different owning process: handle reuse.
        assert!(!facts.still_describes(7, 0x4321, 4243));
    }

    #[test]
    fn native_target_debug_redacts_window_handles() {
        let facts = LockedDiscordHostFacts::copy_from_locked(7, 0x4321, 4242, true).unwrap();
        let facts_debug = format!("{facts:?}");
        assert!(facts_debug.contains("LockedDiscordHostFacts"));
        assert!(facts_debug.contains("<redacted-hwnd>"));
        assert!(!facts_debug.contains("17185"));
        assert!(!facts_debug.contains("0x4321"));

        let overlay = NativeDiscordOverlayTarget {
            generation: 9,
            window: 0x7777,
            rect: [1, 2, 3, 4],
            foreground: true,
            trusted_parent: 0x8888,
        };
        let overlay_debug = format!("{overlay:?}");
        assert!(overlay_debug.contains("NativeDiscordOverlayTarget"));
        assert!(overlay_debug.contains("<redacted-hwnd>"));
        assert!(!overlay_debug.contains("30583"));
        assert!(!overlay_debug.contains("34952"));
        assert!(!overlay_debug.contains("0x7777"));
        assert!(!overlay_debug.contains("0x8888"));
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
    fn signal() {
        const PROFILE_TEST_NOW: u64 = 1_800_000_000;
        let profile = adapter_profile::signal_default_profile();
        let payload = adapter_profile::verify_profile_doc(
            &profile,
            adapter_profile::signal_default_trusted_signing_key_b64(),
            PROFILE_TEST_NOW,
        )
        .expect("compiled Signal adapter profile must verify");

        assert_eq!(
            SIGNAL_PRIMARY_WINDOW_CLASS,
            adapter_profile::SIGNAL_DESKTOP_NATIVE_PRIMARY_WINDOW_CLASS
        );
        assert_eq!(
            SIGNAL_PRIMARY_WINDOW_TITLE,
            adapter_profile::SIGNAL_DESKTOP_NATIVE_WINDOW_TITLE
        );
        assert_eq!(payload.app.stable_id, "signal");
        assert_eq!(payload.canary.expected_text, SIGNAL_PRIMARY_WINDOW_TITLE);
        assert!(payload.selectors.iter().any(|selector| matches!(
            &selector.strategy,
            adapter_profile::SelectorStrategy::Accessibility { role, name, .. }
                if selector.kind == adapter_profile::SelectorKind::AppRoot
                    && role == adapter_profile::SIGNAL_DESKTOP_NATIVE_APP_ROOT_ROLE
                    && name.as_deref() == Some(SIGNAL_PRIMARY_WINDOW_TITLE)
        )));

        assert!(dedicated_window_class_allowed(
            NativeAppId::Signal,
            SIGNAL_PRIMARY_WINDOW_CLASS,
        ));
        assert!(existing_window_identity_allowed(
            NativeAppId::Signal,
            false,
            SIGNAL_PRIMARY_WINDOW_CLASS,
            SIGNAL_PRIMARY_WINDOW_TITLE,
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Signal,
            true,
            SIGNAL_PRIMARY_WINDOW_CLASS,
            "Signal error",
        ));
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
            DISCORD_PRIMARY_WINDOW_CLASS,
            "@Deckard - Discord",
        ));
        assert!(existing_window_identity_allowed(
            NativeAppId::Discord,
            true,
            DISCORD_PRIMARY_WINDOW_CLASS,
            "Discord PTB",
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Discord,
            false,
            DISCORD_PRIMARY_WINDOW_CLASS,
            "Discord",
        ));
        assert!(!existing_window_identity_allowed(
            NativeAppId::Discord,
            true,
            "DiscordDesktopOverlayInputTrap",
            "",
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
            cold_host_action(
                NativeAppId::Telegram,
                DiscordSessionMode::Dedicated,
                DiscordTakeover::BorrowExisting
            ),
            ColdHostAction::LaunchDedicated
        );
        assert_eq!(dedicated_launch_attempt_limit(NativeAppId::Telegram), 2);
        assert_eq!(child_presentation_attempt_limit(NativeAppId::Telegram), 2);

        // An already-open ordinary Telegram is claimed/focused, never owned.
        assert_eq!(
            cold_host_action(
                NativeAppId::Telegram,
                DiscordSessionMode::ExistingSession,
                DiscordTakeover::BorrowExisting
            ),
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
    fn borrowed_recovery_restore_gate_ignores_style_drift() {
        // The gate is a pure function of process identity and owner
        // restorability -- it must never depend on `GWL_STYLE`, because this
        // project's own tether legitimately flips `WS_MINIMIZE` in that
        // field while mirroring the host's iconic state. A style-equality
        // gate here previously caused a fully-identified, fully-restorable
        // window to be skipped entirely (owner AND ex-style both) on an
        // ordinary graceful close after any minimize/restore cycle.
        assert!(borrowed_recovery_restore_permitted(true, true));
        assert!(!borrowed_recovery_restore_permitted(false, true));
        assert!(!borrowed_recovery_restore_permitted(true, false));
        assert!(!borrowed_recovery_restore_permitted(false, false));
    }

    #[test]
    fn guardian_restore_ex_style_outcome_uses_the_captured_value_not_a_constant() {
        const CAPTURED_UNUSUAL: isize = 0x0000_0080; // e.g. WS_EX_TOOLWINDOW, legitimately original
        const TASK_STYLE: isize = 0x0004_0000; // e.g. WS_EX_APPWINDOW, applied while borrowed
        // A window whose true original ex-style already carried the
        // "unusual" bit must be restored to exactly that bit, not "fixed"
        // into some assumed-normal constant.
        assert_eq!(
            guardian_restore_ex_style_outcome(
                true,
                TASK_STYLE,
                CAPTURED_UNUSUAL,
                CAPTURED_UNUSUAL
            ),
            GUARDIAN_RESTORE_EX_STYLE_APPLIED
        );
        // Idempotent: running the same restore again when the live value
        // already matches the capture reports "already normal", not
        // "applied" again.
        assert_eq!(
            guardian_restore_ex_style_outcome(
                true,
                CAPTURED_UNUSUAL,
                CAPTURED_UNUSUAL,
                CAPTURED_UNUSUAL
            ),
            GUARDIAN_RESTORE_EX_STYLE_ALREADY_NORMAL
        );
        // The gate refusing the restore (identity/owner not restorable) is
        // always reported as failed, regardless of what the live bits
        // happen to read as.
        assert_eq!(
            guardian_restore_ex_style_outcome(
                false,
                TASK_STYLE,
                CAPTURED_UNUSUAL,
                CAPTURED_UNUSUAL
            ),
            GUARDIAN_RESTORE_EX_STYLE_FAILED
        );
        // The write not sticking (post-write read still differs from the
        // capture) is reported as failed even when the gate allowed the
        // attempt.
        assert_eq!(
            guardian_restore_ex_style_outcome(true, TASK_STYLE, TASK_STYLE, CAPTURED_UNUSUAL),
            GUARDIAN_RESTORE_EX_STYLE_FAILED
        );
    }

    #[test]
    fn guardian_restore_ex_style_round_trip_and_idempotence() {
        const APP_WINDOW: isize = 0x0004_0000;
        const TOOL_WINDOW: isize = 0x0000_0080;
        const OTHER_BITS: isize = 0x0000_0108;
        let original = OTHER_BITS; // no APPWINDOW, no TOOLWINDOW: an ordinary window.
        let claimed = borrowed_task_ex_style(original, APP_WINDOW, TOOL_WINDOW);
        assert_ne!(claimed, original);
        // First restore: the live value is still the claimed (borrowed) style.
        let first = guardian_restore_ex_style_outcome(true, claimed, original, original);
        assert_eq!(first, GUARDIAN_RESTORE_EX_STYLE_APPLIED);
        // Second restore of the same snapshot (e.g. `terminate()` racing the
        // guardian subprocess, or `detach()` followed by `terminate()`) must
        // be a safe no-op, not a re-corruption.
        let second = guardian_restore_ex_style_outcome(true, original, original, original);
        assert_eq!(second, GUARDIAN_RESTORE_EX_STYLE_ALREADY_NORMAL);
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
        // The window is taken off screen between arming the guardian and the
        // first mutation, so the owner link and the task ex-style are never
        // applied to a window the operator is looking at -- which is also the
        // only ordering in which they can actually take it off the taskbar.
        assert!(borrowed_mutation_transition(
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::Concealed
        ));
        assert!(borrowed_mutation_transition(
            BorrowedMutationStage::Concealed,
            BorrowedMutationStage::OwnerLinked
        ));
        // Concealment is never allowed to run ahead of the guardian: a crash
        // with the window hidden and nothing armed to put it back is the one
        // outcome worse than the defect.
        assert!(!borrowed_mutation_transition(
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::Concealed
        ));
        // Nor may it stand in for either mutation.
        assert!(!borrowed_mutation_transition(
            BorrowedMutationStage::Concealed,
            BorrowedMutationStage::TaskStyleApplied
        ));
        assert!(!borrowed_mutation_transition(
            BorrowedMutationStage::OwnerLinked,
            BorrowedMutationStage::Concealed
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
    fn existing_window_identity_is_title_independent_except_for_signal_and_whatsapp() {
        // The relaunch presence probe skips the cross-process title read for
        // every client this reports `false` for, so the gate's answer must not
        // depend on the title for any of them -- otherwise the probe and the
        // real scan could disagree about whether a window is claimable.
        for id in [
            NativeAppId::Discord,
            NativeAppId::Telegram,
            NativeAppId::Signal,
            NativeAppId::Whatsapp,
            NativeAppId::Outlook,
        ] {
            if existing_window_identity_uses_title(id) {
                continue;
            }
            for class_name in [
                DISCORD_PRIMARY_WINDOW_CLASS,
                SIGNAL_PRIMARY_WINDOW_CLASS,
                WHATSAPP_PRIMARY_WINDOW_CLASS,
                OUTLOOK_CLASSIC_PRIMARY_WINDOW_CLASS,
                OUTLOOK_NEW_PRIMARY_WINDOW_CLASS,
                "Some.Other.Class",
            ] {
                for visible in [true, false] {
                    assert_eq!(
                        existing_window_identity_allowed(id, visible, class_name, ""),
                        existing_window_identity_allowed(id, visible, class_name, "Anything"),
                    );
                }
            }
        }
        assert!(existing_window_identity_uses_title(NativeAppId::Signal));
        assert!(existing_window_identity_uses_title(NativeAppId::Whatsapp));
    }

    #[test]
    fn adoption_conceals_before_it_mutates_and_never_after_it_presents() {
        fn sequence_is_legal(stages: &[BorrowedMutationStage]) -> bool {
            stages
                .windows(2)
                .all(|pair| borrowed_mutation_transition(pair[0], pair[1]))
        }

        // The shipped ordering: nothing the operator can see is ever mutated,
        // and the first show the shell evaluates already has the owner link and
        // the tool-window style on it, so no taskbar button is ever handed out.
        assert!(sequence_is_legal(&[
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::Concealed,
            BorrowedMutationStage::OwnerLinked,
            BorrowedMutationStage::TaskStyleApplied,
        ]));
        // A window that refuses to hide is still adopted rather than failed:
        // the operator keeps a working Discord, it just keeps its button too.
        assert!(sequence_is_legal(&[
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::OwnerLinked,
            BorrowedMutationStage::TaskStyleApplied,
        ]));
        // Concealing a window that is already adopted and on screen would be a
        // hide/show cycle in the operator's face, which is exactly what the
        // continuous tether must never do.
        assert!(!sequence_is_legal(&[
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::GuardianArmed,
            BorrowedMutationStage::OwnerLinked,
            BorrowedMutationStage::TaskStyleApplied,
            BorrowedMutationStage::Concealed,
        ]));
        // And concealing before anything is armed to undo it is never legal.
        assert!(!sequence_is_legal(&[
            BorrowedMutationStage::Captured,
            BorrowedMutationStage::Concealed,
            BorrowedMutationStage::GuardianArmed,
        ]));
        // The mutation the conceal exists to make effective is unchanged: drop
        // WS_EX_APPWINDOW, add WS_EX_TOOLWINDOW, disturb nothing else.
        const APP_WINDOW: isize = 0x0004_0000;
        const TOOL_WINDOW: isize = 0x0000_0080;
        assert_eq!(
            borrowed_task_ex_style(0x0000_0100 | APP_WINDOW, APP_WINDOW, TOOL_WINDOW),
            0x0000_0100 | TOOL_WINDOW,
        );
        // Hiding the window clears WS_VISIBLE out of GWL_STYLE. The post-mutation
        // verification has to forgive exactly that one bit while concealed, or
        // the adoption reads its own conceal as foreign tampering and fails every
        // time -- and it must forgive nothing else, and nothing at all when the
        // window was never hidden.
        const VISIBLE: isize = 0x1000_0000;
        const OTHER: isize = 0x00cf_0000;
        assert!(borrowed_concealed_style_is_preserved(
            OTHER | VISIBLE,
            OTHER,
            VISIBLE,
            true
        ));
        assert!(borrowed_concealed_style_is_preserved(
            OTHER | VISIBLE,
            OTHER | VISIBLE,
            VISIBLE,
            true
        ));
        assert!(!borrowed_concealed_style_is_preserved(
            OTHER | VISIBLE,
            (OTHER | VISIBLE) & !0x0080_0000,
            VISIBLE,
            true
        ));
        assert!(!borrowed_concealed_style_is_preserved(
            OTHER | VISIBLE,
            OTHER,
            VISIBLE,
            false
        ));
        assert!(borrowed_concealed_style_is_preserved(
            OTHER | VISIBLE,
            OTHER | VISIBLE,
            VISIBLE,
            false
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
        assert!(!borrowed_owner_requires_repair(42, 42));
        assert!(borrowed_owner_requires_repair(0, 42));
    }

    #[test]
    fn borrowed_tether_repair_is_skipped_unless_something_is_provably_wrong() {
        let aligned = borrowed_tether_repair_plan(true, true, true, Some(false));
        assert!(aligned.is_noop());
        assert!(!borrowed_tether_requires_repair(aligned));
        assert_eq!(
            aligned.decision_labels(),
            [Some(TETHER_REPAIR_SKIPPED_LABEL), None, None]
        );
        // An undecidable z-order read is treated exactly like a correct one: it
        // must never issue a write.
        let undecided = borrowed_tether_repair_plan(true, true, true, None);
        assert!(undecided.is_noop());
        assert!(!borrowed_tether_requires_repair(undecided));
        // An active composite on its own is no longer a reason to write anything:
        // that unconditional branch is exactly what saturated the borrowed UI
        // thread, and a background composite is never raised either.
        assert!(!borrowed_tether_requires_repair(borrowed_tether_repair_plan(
            true,
            true,
            false,
            Some(false)
        )));
        assert!(!borrowed_tether_requires_repair(borrowed_tether_repair_plan(
            true,
            true,
            false,
            Some(true)
        )));
        assert_eq!(
            undecided.decision_labels(),
            [Some(TETHER_REPAIR_SKIPPED_LABEL), None, None]
        );
        // Geometry drift corrects geometry only, and never touches the stack.
        let geometry = borrowed_tether_repair_plan(false, true, true, Some(false));
        assert_eq!(
            geometry,
            BorrowedTetherRepairPlan {
                geometry: true,
                zorder: false,
                reveal: false,
            }
        );
        assert_eq!(
            geometry.decision_labels(),
            [Some(TETHER_REPAIR_GEOMETRY_LABEL), None, None]
        );
        // A proved inversion (the trusted owner above the borrowed window) is the
        // only z-order state that writes.
        let zorder = borrowed_tether_repair_plan(true, true, true, Some(true));
        assert_eq!(
            zorder,
            BorrowedTetherRepairPlan {
                geometry: false,
                zorder: true,
                reveal: false,
            }
        );
        assert_eq!(
            zorder.decision_labels(),
            [None, Some(TETHER_REPAIR_ZORDER_LABEL), None]
        );
        // A hidden borrowed window is revealed, and a hidden *and* inverted one is
        // revealed and raised in the same single call.
        let reveal = borrowed_tether_repair_plan(true, false, false, Some(false));
        assert_eq!(
            reveal,
            BorrowedTetherRepairPlan {
                geometry: false,
                zorder: false,
                reveal: true,
            }
        );
        assert_eq!(
            reveal.decision_labels(),
            [None, None, Some(TETHER_REPAIR_VISIBILITY_LABEL)]
        );
        let everything = borrowed_tether_repair_plan(false, false, true, Some(true));
        assert!(borrowed_tether_requires_repair(everything));
        assert_eq!(
            everything.decision_labels(),
            [
                Some(TETHER_REPAIR_GEOMETRY_LABEL),
                Some(TETHER_REPAIR_ZORDER_LABEL),
                Some(TETHER_REPAIR_VISIBILITY_LABEL),
            ]
        );
        // Every decision label is distinct and fixed, so a QA run can count them.
        let labels: std::collections::BTreeSet<&str> =
            TETHER_REPAIR_DECISION_LABELS.iter().copied().collect();
        assert_eq!(labels.len(), TETHER_REPAIR_DECISION_LABELS.len());
        for label in TETHER_REPAIR_DECISION_LABELS {
            assert!(label.starts_with("tether_repair_") || label.starts_with("tether_reconcile_"));
        }
    }

    #[test]
    fn borrowed_tether_restore_repaint_fires_once_per_transition() {
        use BorrowedTetherRestoreRepaint as R;
        let limit = BORROWED_TETHER_RESTORE_REPAINT_LIMIT;

        // A host that was never minimized never forces a compositor rebuild, no
        // matter how many passes run.
        for _ in 0..1_000 {
            assert_eq!(
                borrowed_tether_restore_repaint_decision(false, false, 0, limit),
                R::Skip
            );
        }

        // Every pass while the host is iconic only arms the transition.
        assert_eq!(
            borrowed_tether_restore_repaint_decision(true, false, 0, limit),
            R::Defer
        );
        assert_eq!(
            borrowed_tether_restore_repaint_decision(true, true, 0, limit),
            R::Defer
        );

        // Replay the exact worker state machine over a minimize/restore cycle and
        // count how many rebuilds it asks for. The rebuild is assumed to succeed,
        // which is what clears the sticky flag.
        // A rebuild that lands never retries, so the bounded attempt counter
        // stays at zero throughout this replay; the second replay below drives
        // the retry path.
        let attempts = 0u32;
        let mut was_iconic = false;
        let mut applied = 0usize;
        let iconic_timeline = [
            false, false, true, true, true, true, false, false, false, false, false, false,
        ];
        for host_is_iconic in iconic_timeline {
            let decision = borrowed_tether_restore_repaint_decision(
                host_is_iconic,
                was_iconic,
                attempts,
                limit,
            );
            match decision {
                R::Defer => was_iconic = true,
                // Rebuild landed: disarm.
                R::Apply => {
                    applied += 1;
                    was_iconic = false;
                }
                R::Abandon => was_iconic = false,
                R::Skip => {}
            }
        }
        assert_eq!(applied, 1, "one transition must force exactly one rebuild");

        // A rebuild that does not bring the window back is retried, but only a
        // bounded number of times, and then abandoned rather than looping.
        let mut was_iconic = true;
        let mut attempts = 0u32;
        let mut applied = 0usize;
        let mut abandoned = 0usize;
        for _ in 0..(limit as usize + 5) {
            match borrowed_tether_restore_repaint_decision(false, was_iconic, attempts, limit) {
                R::Apply => {
                    applied += 1;
                    attempts += 1;
                }
                R::Abandon => {
                    abandoned += 1;
                    was_iconic = false;
                    attempts = 0;
                }
                R::Skip => {}
                R::Defer => unreachable!("host is not iconic in this replay"),
            }
        }
        assert_eq!(applied, limit as usize);
        assert_eq!(abandoned, 1);

        // Every decision names itself with a fixed, distinct label.
        let stages: std::collections::BTreeSet<&str> =
            [R::Defer, R::Apply, R::Skip, R::Abandon]
                .into_iter()
                .map(R::stage)
                .collect();
        assert_eq!(stages.len(), 4);
        assert_eq!(R::Apply.stage(), "restore_repaint_applied");
        assert_eq!(R::Skip.stage(), "restore_repaint_skipped_not_iconic");
    }

    #[test]
    fn borrowed_window_content_health_is_a_distinct_colour_count_not_a_brightness_test() {
        // The regression this threshold exists for: a window that is *not*
        // iconic but whose compositor surface never actually repainted
        // samples as one single flat colour. That must never count as
        // healthy no matter which colour it is -- including black, which is
        // also what a correctly repainted pure-black-theme Discord looks
        // like almost everywhere. Distinct-colour-count, not a black-pixel
        // ratio, is what tells the two apart.
        assert!(!borrowed_window_content_is_healthy(0));
        assert!(!borrowed_window_content_is_healthy(1));
        assert!(!borrowed_window_content_is_healthy(
            BORROWED_WINDOW_HEALTHY_DISTINCT_COLOURS - 1
        ));
        assert!(borrowed_window_content_is_healthy(
            BORROWED_WINDOW_HEALTHY_DISTINCT_COLOURS
        ));
        assert!(borrowed_window_content_is_healthy(201));
    }

    #[test]
    fn borrowed_tether_restore_repair_stays_armed_until_paint_is_verified() {
        // Encodes the fix for the false-positive success signal: the old
        // `force_borrowed_compositor_rebuild` reported success from
        // `IsIconic` alone, which was always true by the time it was checked
        // (`SW_RESTORE` already ran once earlier in the same reconcile pass,
        // and again inside the rebuild itself), so the bounded retry this
        // test drives could never actually re-fire in practice.
        //
        // Replay the same worker state machine as
        // `borrowed_tether_restore_repaint_fires_once_per_transition`, but
        // this time the simulated rebuild only *verifies* (flat-colour vs.
        // real content, exactly like `borrowed_window_content_is_healthy`)
        // on the very last attempt the bounded limit allows. A regression
        // back to "disarm whenever `Apply` merely ran" would disarm on the
        // first attempt and never reach the limit; a regression back to
        // "IsIconic is the success signal" would look identical to this test
        // succeeding immediately at attempt 1 regardless of the simulated
        // verification outcome, which the assertion on `applied` below
        // rules out.
        use BorrowedTetherRestoreRepaint as R;
        let limit = BORROWED_TETHER_RESTORE_REPAINT_LIMIT;

        let mut was_iconic = true;
        let mut attempts = 0u32;
        let mut applied = 0usize;
        let mut disarmed_on_attempt = None;
        for attempt in 1..=(limit as usize + 5) {
            match borrowed_tether_restore_repaint_decision(false, was_iconic, attempts, limit) {
                R::Apply => {
                    applied += 1;
                    attempts += 1;
                    // Only the last allowed attempt samples as verified
                    // content; every earlier one is still a flat colour.
                    let sampled_distinct_colours = if attempt as u32 == limit { 201 } else { 1 };
                    if borrowed_window_content_is_healthy(sampled_distinct_colours) {
                        // Mirrors `reconcile_borrowed_tether`'s Apply arm,
                        // which only resets this state inside
                        // `if force_borrowed_compositor_rebuild(...)`.
                        was_iconic = false;
                        attempts = 0;
                        disarmed_on_attempt = Some(attempt);
                    }
                }
                R::Abandon => panic!("must disarm via verified paint before the limit is reached"),
                R::Skip => {}
                R::Defer => unreachable!("host is not iconic in this replay"),
            }
        }
        assert_eq!(applied, limit as usize, "must retry up to the bounded limit");
        assert_eq!(disarmed_on_attempt, Some(limit as usize));
    }

    #[test]
    fn borrowed_tether_backoff_widens_only_while_nothing_drifts() {
        // The fast tier covers the first burst of passes after anything moves.
        assert_eq!(
            borrowed_tether_poll_interval(0),
            BORROWED_TETHER_ACTIVE_INTERVAL
        );
        assert_eq!(
            borrowed_tether_poll_interval(BORROWED_TETHER_SETTLING_AFTER - 1),
            BORROWED_TETHER_ACTIVE_INTERVAL
        );
        assert_eq!(
            borrowed_tether_poll_interval(BORROWED_TETHER_SETTLING_AFTER),
            BORROWED_TETHER_SETTLING_INTERVAL
        );
        assert_eq!(
            borrowed_tether_poll_interval(BORROWED_TETHER_IDLE_AFTER - 1),
            BORROWED_TETHER_SETTLING_INTERVAL
        );
        assert_eq!(
            borrowed_tether_poll_interval(BORROWED_TETHER_IDLE_AFTER),
            BORROWED_TETHER_IDLE_INTERVAL
        );
        assert_eq!(
            borrowed_tether_poll_interval(BORROWED_TETHER_QUIET_AFTER - 1),
            BORROWED_TETHER_IDLE_INTERVAL
        );
        assert_eq!(
            borrowed_tether_poll_interval(BORROWED_TETHER_QUIET_AFTER),
            BORROWED_TETHER_QUIET_INTERVAL
        );
        // Monotonic non-decreasing and capped: the cadence can only widen, and it
        // never widens past the quiet tier.
        let mut previous = Duration::ZERO;
        for quiet in 0..4_096u32 {
            let interval = borrowed_tether_poll_interval(quiet);
            assert!(interval >= previous);
            assert!(interval <= BORROWED_TETHER_QUIET_INTERVAL);
            previous = interval;
        }
        assert_eq!(
            borrowed_tether_poll_interval(u32::MAX),
            BORROWED_TETHER_QUIET_INTERVAL
        );

        // A pass that wrote to the borrowed window is real drift and resets the
        // ladder, whatever it observed.
        for observation in [
            BorrowedTetherObservation::Aligned,
            BorrowedTetherObservation::ParentMinimized,
            BorrowedTetherObservation::TransientDesktopUnavailable,
            BorrowedTetherObservation::IdentityChanged,
        ] {
            assert!(!borrowed_tether_pass_is_quiet(observation, true));
            assert_eq!(borrowed_tether_next_quiet_run(500, observation, true), 0);
        }
        // A pass that wrote nothing and found the composite aligned (or the host
        // simply minimized) is quiet.
        assert!(borrowed_tether_pass_is_quiet(
            BorrowedTetherObservation::Aligned,
            false
        ));
        assert!(borrowed_tether_pass_is_quiet(
            BorrowedTetherObservation::ParentMinimized,
            false
        ));
        // A stall or an identity change is never quiet even without a write.
        assert!(!borrowed_tether_pass_is_quiet(
            BorrowedTetherObservation::TransientDesktopUnavailable,
            false
        ));
        assert!(!borrowed_tether_pass_is_quiet(
            BorrowedTetherObservation::IdentityChanged,
            false
        ));
        assert_eq!(
            borrowed_tether_next_quiet_run(7, BorrowedTetherObservation::Aligned, false),
            8
        );
        assert_eq!(
            borrowed_tether_next_quiet_run(u32::MAX, BorrowedTetherObservation::Aligned, false),
            u32::MAX
        );

        // The wall-clock cost of reaching the slowest tier: a drag that pauses
        // for less than this never falls off the fast tier entirely.
        let settle_ms = u128::from(BORROWED_TETHER_SETTLING_AFTER)
            * BORROWED_TETHER_ACTIVE_INTERVAL.as_millis()
            + u128::from(BORROWED_TETHER_IDLE_AFTER - BORROWED_TETHER_SETTLING_AFTER)
                * BORROWED_TETHER_SETTLING_INTERVAL.as_millis()
            + u128::from(BORROWED_TETHER_QUIET_AFTER - BORROWED_TETHER_IDLE_AFTER)
                * BORROWED_TETHER_IDLE_INTERVAL.as_millis();
        assert_eq!(settle_ms, 3_776);
        // The steady-state polling floor must be a large multiple cheaper than
        // the historical measured ~27 passes/second floor.
        assert!(BORROWED_TETHER_QUIET_INTERVAL.as_millis() >= 8 * 37);
    }

    #[test]
    fn borrowed_tether_identity_cache_invalidates_on_identity_change() {
        use BorrowedTetherIdentityCheck as C;
        let max_age = BORROWED_TETHER_IDENTITY_MAX_AGE;
        let fresh = Duration::from_millis(1);

        // Nothing has ever verified: always verify in full.
        assert_eq!(
            borrowed_tether_identity_check(0, 0, 0x1234, 4242, None, max_age),
            C::Verify
        );
        // A fresh verification of exactly this HWND and pid is reusable.
        assert_eq!(
            borrowed_tether_identity_check(0x1234, 4242, 0x1234, 4242, Some(fresh), max_age),
            C::Cached
        );
        // A different HWND is a different window: re-verify.
        assert_eq!(
            borrowed_tether_identity_check(0x1234, 4242, 0x9999, 4242, Some(fresh), max_age),
            C::Verify
        );
        // A different pid behind the same HWND is a different process: re-verify.
        assert_eq!(
            borrowed_tether_identity_check(0x1234, 4242, 0x1234, 4243, Some(fresh), max_age),
            C::Verify
        );
        // Past the ceiling, re-verify even though nothing observable changed.
        assert_eq!(
            borrowed_tether_identity_check(0x1234, 4242, 0x1234, 4242, Some(max_age), max_age),
            C::Verify
        );
        assert_eq!(
            borrowed_tether_identity_check(
                0x1234,
                4242,
                0x1234,
                4242,
                Some(max_age + fresh),
                max_age
            ),
            C::Verify
        );
        // Right up to the ceiling it is still reusable.
        assert_eq!(
            borrowed_tether_identity_check(
                0x1234,
                4242,
                0x1234,
                4242,
                Some(max_age - fresh),
                max_age
            ),
            C::Cached
        );
        // Degenerate handles and pids are never served from cache.
        assert_eq!(
            borrowed_tether_identity_check(0, 4242, 0, 4242, Some(fresh), max_age),
            C::Verify
        );
        assert_eq!(
            borrowed_tether_identity_check(0x1234, 0, 0x1234, 0, Some(fresh), max_age),
            C::Verify
        );
        // The ceiling is generous but finite, so a cached verification can never
        // outlive one user-visible beat by much.
        assert!(max_age >= Duration::from_millis(1_000));
        assert!(max_age <= Duration::from_millis(5_000));
    }

    #[test]
    fn borrowed_tether_zorder_walk_only_answers_from_proof() {
        // owner -> target chain: walking up from the target meets the owner, so
        // the owner is provably above it.
        let chain = |cursor: isize| match cursor {
            3 => 2,
            2 => 1,
            _ => 0,
        };
        assert_eq!(
            borrowed_tether_owner_is_above_target(1, 3, 8, chain),
            Some(true)
        );
        // Reaching the top of the chain without meeting the owner proves the
        // borrowed window is already above it.
        assert_eq!(
            borrowed_tether_owner_is_above_target(9, 3, 8, chain),
            Some(false)
        );
        // A bound that cannot reach the owner answers undecided rather than
        // guessing, and an undecided answer never writes.
        assert_eq!(borrowed_tether_owner_is_above_target(1, 3, 1, chain), None);
        // Degenerate handles are never a proof of anything.
        assert_eq!(borrowed_tether_owner_is_above_target(0, 3, 8, chain), None);
        assert_eq!(borrowed_tether_owner_is_above_target(1, 0, 8, chain), None);
        assert_eq!(borrowed_tether_owner_is_above_target(3, 3, 8, chain), None);
        // A cycle cannot spin: the bound still terminates the walk.
        assert_eq!(
            borrowed_tether_owner_is_above_target(9, 3, 4, |cursor| match cursor {
                3 => 2,
                _ => 3,
            }),
            None
        );
        assert_eq!(BORROWED_TETHER_STACK_WALK_LIMIT, 128);
    }

    #[test]
    fn borrowed_tether_coalesces_a_backlog_into_one_pass() {
        // A single request runs one pass and supersedes nothing.
        assert_eq!(
            borrowed_tether_batch(1, false),
            BorrowedTetherBatch::ReconcileOnce { coalesced: 0 }
        );
        // Only the newest of a backlog matters, and the whole backlog is answered
        // from that one pass.
        assert_eq!(
            borrowed_tether_batch(5, false),
            BorrowedTetherBatch::ReconcileOnce { coalesced: 4 }
        );
        // A stop seen while draining wins: no further cross-process pass runs.
        assert_eq!(borrowed_tether_batch(5, true), BorrowedTetherBatch::TearDown);
        assert_eq!(borrowed_tether_batch(0, false), BorrowedTetherBatch::TearDown);
    }

    #[test]
    fn borrowed_tether_replies_are_matched_to_their_own_request() {
        assert!(borrowed_tether_reply_is_current(7, 7));
        // An answer left behind by an abandoned request is never accepted as a
        // later request's answer, in either direction.
        assert!(!borrowed_tether_reply_is_current(7, 6));
        assert!(!borrowed_tether_reply_is_current(7, 8));
        assert!(!borrowed_tether_reply_is_current(0, 7));
    }

    #[test]
    fn borrowed_tether_budgets_cover_measured_stalls_without_becoming_unbounded() {
        // Measured on this machine: one MSAA carrier-row scan and one composer
        // accessibility scan, both of which occupy Discord's UI thread while a
        // reconcile's cross-process window call waits behind them.
        let measured_msaa_row_scan = Duration::from_millis(865);
        let measured_composer_scan = Duration::from_millis(2_889);
        let guard = borrowed_tether_reconcile_budget(BorrowedTetherCaller::ProtectionGuard);
        // A refusal on the guard thread tears protection down, so its budget must
        // outlast the worst measured stall.
        assert!(guard > measured_composer_scan);
        assert!(guard > measured_msaa_row_scan);
        // Still bounded, and still under the 5 s Windows hung-window threshold, so
        // a genuinely wedged Discord fails closed instead of hanging.
        assert!(guard <= Duration::from_secs(4));
        // A UI-thread caller must never block OSL itself for that long; it has a
        // cheap local fallback and a bounded retry instead.
        let ui = borrowed_tether_reconcile_budget(BorrowedTetherCaller::UiThread);
        assert_eq!(ui, Duration::from_millis(500));
        assert!(ui < guard);
    }

    #[test]
    fn borrowed_tether_health_stall_names_worker_loss_before_generation_drift() {
        // A healthy worker bound to the exact host generation is the only
        // accepted state; the gate itself is unchanged, only named.
        assert_eq!(borrowed_tether_health_stall(true, 7, 7), None);
        assert_eq!(
            borrowed_tether_health_stall(true, 6, 7),
            Some(BorrowedTetherStall::GenerationMismatch)
        );
        assert_eq!(
            borrowed_tether_health_stall(true, 8, 7),
            Some(BorrowedTetherStall::GenerationMismatch)
        );
        // A worker that already failed closed is reported as unhealthy even when
        // the generation still matches, so the stronger fact wins the label.
        assert_eq!(
            borrowed_tether_health_stall(false, 7, 7),
            Some(BorrowedTetherStall::WorkerUnhealthy)
        );
        assert_eq!(
            borrowed_tether_health_stall(false, 1, 7),
            Some(BorrowedTetherStall::WorkerUnhealthy)
        );
    }

    #[test]
    fn borrowed_tether_reports_keep_alignment_contract_and_name_every_branch() {
        // Alignment verdicts are exactly the historical ones.
        assert_eq!(
            borrowed_tether_reconcile_report(BorrowedTetherObservation::Aligned, None),
            BorrowedTetherReconcileReport::aligned()
        );
        assert_eq!(
            borrowed_tether_reconcile_report(BorrowedTetherObservation::ParentMinimized, None),
            BorrowedTetherReconcileReport::aligned()
        );
        assert!(
            !borrowed_tether_reconcile_report(
                BorrowedTetherObservation::TransientDesktopUnavailable,
                Some(BorrowedTetherStall::TargetRectMismatch),
            )
            .aligned
        );
        assert!(
            !borrowed_tether_reconcile_report(
                BorrowedTetherObservation::IdentityChanged,
                Some(BorrowedTetherStall::TargetIdentityChanged),
            )
            .aligned
        );
        // An aligned observation never carries a stall label forward.
        assert_eq!(
            borrowed_tether_reconcile_report(
                BorrowedTetherObservation::Aligned,
                Some(BorrowedTetherStall::TargetHidden),
            ),
            BorrowedTetherReconcileReport {
                aligned: true,
                stall: None,
            }
        );
        // A refusal keeps the exact branch that produced it.
        assert_eq!(
            borrowed_tether_reconcile_report(
                BorrowedTetherObservation::TransientDesktopUnavailable,
                Some(BorrowedTetherStall::ParentHidden),
            )
            .stage(),
            "tether_stall_parent_hidden"
        );
    }

    #[test]
    fn borrowed_tether_stall_labels_are_distinct_and_fall_back_to_the_catch_all() {
        let stalls = [
            BorrowedTetherStall::WorkerUnhealthy,
            BorrowedTetherStall::GenerationMismatch,
            BorrowedTetherStall::WorkerGone,
            BorrowedTetherStall::ReconcileTimedOut,
            BorrowedTetherStall::TargetIdentityChanged,
            BorrowedTetherStall::ParentInvalid,
            BorrowedTetherStall::OwnerRepairRejected,
            BorrowedTetherStall::ParentHidden,
            BorrowedTetherStall::ParentRectUnavailable,
            BorrowedTetherStall::RepairSetWindowPosRejected,
            BorrowedTetherStall::TargetHidden,
            BorrowedTetherStall::TargetMinimized,
            BorrowedTetherStall::TargetRectUnavailable,
            BorrowedTetherStall::TargetRectMismatch,
        ];
        let labels: std::collections::BTreeSet<&'static str> = stalls
            .iter()
            .copied()
            .map(borrowed_tether_stall_stage)
            .collect();
        assert_eq!(labels.len(), stalls.len());
        for stall in stalls {
            let label = borrowed_tether_stall_stage(stall);
            assert!(label.starts_with("tether_stall_"));
            assert_eq!(BorrowedTetherReconcileReport::stalled(stall).stage(), label);
        }
        // An unnamed refusal still reports the historical catch-all rather than
        // going silent.
        assert_eq!(
            BorrowedTetherReconcileReport {
                aligned: false,
                stall: None,
            }
            .stage(),
            "realign_tether_failed"
        );
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
    fn borrowed_control_shield_covers_only_measured_caption_controls() {
        assert_eq!(
            normalized_caption_button_bounds([1440, 802], [1433, 0, 1433, 30]),
            Some([1330, 0, 1440, 30])
        );
        assert_eq!(
            normalized_caption_button_bounds([1728, 962], [1720, 0, 1720, 36]),
            Some([1596, 0, 1728, 36])
        );
        assert_eq!(
            normalized_caption_button_bounds([1440, 802], [1294, 0, 1440, 22]),
            Some([1294, 0, 1440, 22])
        );
        assert_eq!(
            normalized_caption_button_bounds([1440, 802], [700, 0, 700, 30]),
            None
        );
        assert_eq!(
            borrowed_control_shield_rect([240, 168], [1440, 802], [1302, 0, 1440, 44]),
            Some([1542, 168, 1680, 212])
        );
        assert_eq!(
            borrowed_control_shield_rect([-1200, 40], [900, 700], [804, 2, 894, 34]),
            Some([-396, 42, -306, 74])
        );
        assert_eq!(
            borrowed_control_shield_rect([10, 20], [320, 180], [224, 0, 320, 30]),
            Some([234, 20, 330, 50])
        );
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [1440, 802], [700, 0, 1440, 44]),
            None
        );
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [1440, 802], [1302, 0, 1430, 44]),
            Some([1302, 0, 1430, 44])
        );
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [0, 100], [0, 0, 10, 10]),
            None
        );
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [100, -1], [0, 0, 10, 10]),
            None
        );
    }

    #[test]
    fn borrowed_control_shield_rect_rejects_slabs_that_are_not_button_clusters() {
        // A cluster is always wider than it is tall: two or more side-by-side
        // buttons. A tall rectangle in the corner is not a caption cluster,
        // and the pre-existing `height * 2 >= window_height` guard alone lets
        // one through on a large window (300 tall on an 852-tall window).
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [1440, 852], [1340, 0, 1440, 300]),
            None
        );
        // Never taller than a caption, even where the window is tall enough
        // that the relative guard would allow it.
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [1440, 2400], [1140, 0, 1440, 200]),
            None
        );
        // Never more than CAPTION_CLUSTER_MAX_ASPECT times as wide as tall:
        // 300 wide over 30 tall is a titlebar section, not three buttons.
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [3000, 852], [2700, 0, 3000, 30]),
            None
        );
        // A real cluster at the same scale still passes.
        assert_eq!(
            borrowed_control_shield_rect([0, 0], [3000, 852], [2862, 0, 3000, 30]),
            Some([2862, 0, 3000, 30])
        );
    }

    #[test]
    fn measured_caption_buttons_survive_moves_and_resizes() {
        // Measured live on 2026-07-25: borrowed window 240,118,1680,970
        // (1440x852), with the shield's own observed 138x22 top-right strip.
        let measured = MeasuredCaptionButtons {
            right_gap: 0,
            top: 0,
            width: 138,
            height: 22,
        };
        assert_eq!(
            measured_caption_button_bounds([1440, 852], measured),
            Some([1302, 0, 1440, 22])
        );
        // The cluster is stored relative to the right edge, so a resize moves
        // only its left edge -- this is the "doesn't update" defect: a widened
        // window must not leave the shield behind at the old x.
        assert_eq!(
            measured_caption_button_bounds([1920, 852], measured),
            Some([1782, 0, 1920, 22])
        );
        assert_eq!(
            measured_caption_button_bounds([900, 600], measured),
            Some([762, 0, 900, 22])
        );
        // Degenerate inputs never produce a rectangle.
        assert_eq!(measured_caption_button_bounds([0, 852], measured), None);
        assert_eq!(
            measured_caption_button_bounds(
                [1440, 852],
                MeasuredCaptionButtons {
                    right_gap: -1,
                    ..measured
                }
            ),
            None
        );
        assert_eq!(
            measured_caption_button_bounds(
                [1440, 852],
                MeasuredCaptionButtons {
                    width: 0,
                    ..measured
                }
            ),
            None
        );
    }

    #[test]
    fn borrowed_control_shield_prefers_measurement_over_reconstruction() {
        // The live case: DWM reports a zero-width cluster for Discord's custom
        // titlebar, and the accessibility probe returns nothing on a real
        // Discord, so this reconstruction is what actually ships. It was
        // 138x22-at-30 (1542,118,1680,140 on screen), which measurement showed
        // was 33px too wide -- reaching past the group separator at x=1567 over
        // the help/inbox icons -- and 8px too short to cover the buttons. The
        // corrected 110-wide, full-caption-height strip lands on 1570,118,1680,148,
        // against real controls measured at x=1575..1680, ~30px tall.
        let dwm_zero_width = [1433, 0, 1433, 30];
        assert_eq!(
            borrowed_control_shield_target([240, 118], [1440, 852], dwm_zero_width, None),
            Some(([1570, 118, 1680, 148], [1330, 0, 1440, 30]))
        );
        // With a real measurement the shield uses it instead of the guess.
        let measured = MeasuredCaptionButtons {
            right_gap: 0,
            top: 1,
            width: 84,
            height: 22,
        };
        assert_eq!(
            borrowed_control_shield_target(
                [240, 118],
                [1440, 852],
                dwm_zero_width,
                Some(measured)
            ),
            Some(([1596, 119, 1680, 141], [1356, 1, 1440, 23]))
        );
        // A measurement that cannot survive validation must not leave the
        // caption buttons uncovered: the reconstruction is still tried.
        let implausible = MeasuredCaptionButtons {
            right_gap: 0,
            top: 0,
            width: 900,
            height: 22,
        };
        assert_eq!(
            borrowed_control_shield_target(
                [240, 118],
                [1440, 852],
                dwm_zero_width,
                Some(implausible)
            ),
            Some(([1570, 118, 1680, 148], [1330, 0, 1440, 30]))
        );
        // And when neither route yields anything, nothing is positioned.
        assert_eq!(
            borrowed_control_shield_target([240, 118], [1440, 852], [700, 0, 700, 30], None),
            None
        );
    }

    #[test]
    fn caption_button_nodes_are_confined_to_the_top_right_corner() {
        const WINDOW: [i32; 2] = [1440, 852];
        // Discord's three window controls, 28x22 flush to the right edge.
        assert!(caption_button_node_accepted(
            MSAA_ROLE_PUSHBUTTON,
            WINDOW,
            [1412, 0, 1440, 22],
            100
        ));
        // Anything that is not a push button is not a window control.
        assert!(!caption_button_node_accepted(
            0x2A,
            WINDOW,
            [1412, 0, 1440, 22],
            100
        ));
        // Correct role, wrong place: a button in the middle of the titlebar
        // (Discord's search/help/inbox icons) is outside the right-hand zone.
        assert!(!caption_button_node_accepted(
            MSAA_ROLE_PUSHBUTTON,
            WINDOW,
            [700, 0, 728, 22],
            100
        ));
        // Correct role, below the titlebar band: a toolbar button.
        assert!(!caption_button_node_accepted(
            MSAA_ROLE_PUSHBUTTON,
            WINDOW,
            [1412, 60, 1440, 82],
            100
        ));
        // Correct role and place, but titlebar-sized: a roled container, not a
        // button. Accepting it is exactly "covers other stuff too".
        assert!(!caption_button_node_accepted(
            MSAA_ROLE_PUSHBUTTON,
            WINDOW,
            [1240, 0, 1440, 22],
            100
        ));
        // Every threshold is in logical pixels, so a 300% window's 84x66
        // buttons are accepted at scale 300 and rejected as oversized at 100.
        assert!(caption_button_node_accepted(
            MSAA_ROLE_PUSHBUTTON,
            [2880, 1704],
            [2796, 0, 2880, 66],
            300
        ));
        assert!(!caption_button_node_accepted(
            MSAA_ROLE_PUSHBUTTON,
            [2880, 1704],
            [2796, 0, 2880, 66],
            100
        ));
    }

    #[test]
    fn caption_button_containers_are_pruned_to_the_search_region() {
        const WINDOW: [i32; 2] = [1440, 852];
        let region = caption_button_search_region(WINDOW, 100).expect("region");
        assert_eq!(region, [1120, 0, 1440, 48]);
        // A structural node with no usable rectangle is still walked.
        assert!(caption_button_container_worth_walking(None, region));
        assert!(caption_button_container_worth_walking(
            Some([0, 0, 0, 0]),
            region
        ));
        // The document root overlaps the region and is walked.
        assert!(caption_button_container_worth_walking(
            Some([0, 0, 1440, 852]),
            region
        ));
        // The message list, sidebar and member pane do not, and cost one
        // `accLocation` each instead of a subtree walk.
        assert!(!caption_button_container_worth_walking(
            Some([240, 48, 1440, 852]),
            region
        ));
        assert!(!caption_button_container_worth_walking(
            Some([0, 0, 240, 852]),
            region
        ));
        assert_eq!(caption_button_search_region([0, 852], 100), None);
    }

    #[test]
    fn caption_button_cluster_takes_only_the_right_most_contiguous_run() {
        const WINDOW: [i32; 2] = [1440, 852];
        // Discord's three controls, contiguous and flush to the right edge.
        let controls = [
            [1356, 0, 1384, 22],
            [1384, 0, 1412, 22],
            [1412, 0, 1440, 22],
        ];
        assert_eq!(
            caption_button_cluster(WINDOW, &controls, 100),
            Some(MeasuredCaptionButtons {
                right_gap: 0,
                top: 0,
                width: 84,
                height: 22,
            })
        );
        // An unrelated titlebar button far to the left must not be folded in:
        // the union would be 340 wide and would cover unrelated UI.
        let with_stray = [
            [1100, 0, 1128, 22],
            [1356, 0, 1384, 22],
            [1384, 0, 1412, 22],
            [1412, 0, 1440, 22],
        ];
        assert_eq!(
            caption_button_cluster(WINDOW, &with_stray, 100),
            Some(MeasuredCaptionButtons {
                right_gap: 0,
                top: 0,
                width: 84,
                height: 22,
            })
        );
        // A single button is never enough to claim a cluster.
        assert_eq!(
            caption_button_cluster(WINDOW, &controls[2..], 100),
            None
        );
        // A run that does not reach the right edge is not the window controls.
        let inset = [[1200, 0, 1228, 22], [1228, 0, 1256, 22]];
        assert_eq!(caption_button_cluster(WINDOW, &inset, 100), None);
        // Order of discovery is irrelevant.
        let shuffled = [controls[2], controls[0], controls[1]];
        assert_eq!(
            caption_button_cluster(WINDOW, &shuffled, 100),
            caption_button_cluster(WINDOW, &controls, 100)
        );
        assert_eq!(caption_button_cluster([0, 852], &controls, 100), None);
    }

    #[test]
    fn caption_button_probe_is_rate_limited_but_follows_geometry() {
        // No probe has ever run: measure immediately.
        assert!(caption_button_probe_due(false, false, None));
        // Nothing is ever probed twice inside the minimum interval, so a drag
        // cannot spawn one cross-process walk per frame.
        assert!(!caption_button_probe_due(
            false,
            true,
            Some(Duration::from_millis(100))
        ));
        // Past the minimum interval, a missing measurement or a real geometry
        // change (resize, DPI move) re-probes rather than waiting out the idle
        // cadence -- this is the "doesn't update" defect.
        assert!(caption_button_probe_due(
            false,
            false,
            Some(Duration::from_millis(600))
        ));
        assert!(caption_button_probe_due(
            true,
            true,
            Some(Duration::from_millis(600))
        ));
        // At rest with a good measurement, only the idle cadence probes.
        assert!(!caption_button_probe_due(
            true,
            false,
            Some(Duration::from_millis(600))
        ));
        assert!(caption_button_probe_due(
            true,
            false,
            Some(Duration::from_secs(6))
        ));
    }

    #[test]
    fn borrowed_control_shield_accepts_uniform_native_titlebar_samples() {
        assert!(borrowed_control_shield_position_valid(true, false, true));
        assert!(borrowed_control_shield_position_valid(true, true, false));
        assert!(!borrowed_control_shield_position_valid(true, false, false));
        assert!(!borrowed_control_shield_position_valid(false, true, true));
        assert_eq!(
            borrowed_control_shield_color(&[
                [31, 32, 36],
                [32, 33, 37],
                [30, 32, 36],
                [33, 34, 38],
            ]),
            Some([31, 32, 36])
        );
        assert_eq!(
            borrowed_control_shield_color(&[
                [238, 239, 241],
                [240, 241, 243],
                [239, 240, 242],
                [241, 242, 244],
            ]),
            Some([239, 240, 242])
        );
        assert_eq!(
            borrowed_control_shield_color(&[
                [85, 98, 238],
                [86, 99, 241],
                [87, 100, 239],
                [84, 97, 240],
            ]),
            Some([85, 98, 239])
        );
        // A gradient, avatar, or other varied sample is never copied.
        assert_eq!(
            borrowed_control_shield_color(&[
                [20, 20, 22],
                [80, 42, 120],
                [12, 90, 44],
                [70, 18, 16],
            ]),
            None
        );
        assert_eq!(borrowed_control_shield_color(&[[220, 220, 220]; 3]), None);
    }

    #[test]
    fn borrowed_control_shield_stored_color_never_falls_back_to_white() {
        // Valid COLORREF values pass straight through...
        assert_eq!(borrowed_control_shield_stored_color(0x0000_0000), 0);
        assert_eq!(borrowed_control_shield_stored_color(0x0001_0203), 0x0001_0203);
        assert_eq!(
            borrowed_control_shield_stored_color(0x00FF_FFFF),
            0x00FF_FFFF
        );
        // ...but anything outside a plain 24-bit COLORREF -- an
        // uninitialized sentinel, a negative value, corrupted state -- falls
        // back to the explicit dark default. The bug this guards against is
        // the shield window's own `STATIC` window procedure silently
        // erasing with the default system white brush; this fallback must
        // never itself resolve to white.
        assert_eq!(
            borrowed_control_shield_stored_color(-1),
            BORROWED_CONTROL_SHIELD_DEFAULT_COLOR
        );
        assert_eq!(
            borrowed_control_shield_stored_color(0x0100_0000),
            BORROWED_CONTROL_SHIELD_DEFAULT_COLOR
        );
        assert_eq!(BORROWED_CONTROL_SHIELD_DEFAULT_COLOR, 0, "black, not white");
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
        let source = include_str!("native_window_host.rs");
        let forbidden_debug_derive = [
            "#[derive(Debug, Clone, Copy)]",
            "\n#[cfg(target_os = \"windows\")]\n",
            "pub(crate) struct NativeDiscordAccessibilityTarget",
        ]
        .join("");
        assert!(
            !source.contains(&forbidden_debug_derive),
            "NativeDiscordAccessibilityTarget carries a native window handle and must not derive Debug"
        );

        let paths = existing_discord_channel_executables(|channel| match channel.channel {
            DiscordChannel::Stable => vec![
                PathBuf::from("C:/Discord/app-2/Discord.exe"),
                PathBuf::from("C:/Discord/app-1/Discord.exe"),
            ],
            DiscordChannel::Ptb => vec![PathBuf::from("C:/DiscordPTB/DiscordPTB.exe")],
            DiscordChannel::Canary => vec![PathBuf::from("C:/DiscordCanary/DiscordCanary.exe")],
        });
        assert_eq!(
            paths,
            vec![
                PathBuf::from("C:/Discord/app-2/Discord.exe"),
                PathBuf::from("C:/Discord/app-1/Discord.exe"),
                PathBuf::from("C:/DiscordPTB/DiscordPTB.exe"),
                PathBuf::from("C:/DiscordCanary/DiscordCanary.exe"),
            ]
        );
        assert_eq!(
            existing_primary_candidate_index(NativeAppId::Discord, 1, |_, _| false),
            Ok(0)
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
        assert_eq!(
            DISCORD_UIA_PROVIDER_ARGUMENT,
            "--enable-features=UiaProvider"
        );
    }

    #[test]
    fn existing_discord_session_launch_keeps_complete_accessibility_mode() {
        assert_eq!(
            existing_session_launch_arguments(NativeAppId::Discord),
            &[
                DISCORD_ACCESSIBILITY_ARGUMENT,
                DISCORD_UIA_PROVIDER_ARGUMENT,
                DISCORD_START_INACTIVE_ARGUMENT
            ]
        );
        assert!(existing_session_launch_arguments(NativeAppId::Telegram).is_empty());
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
            state.shutdown_with_app(),
        ] {
            assert_eq!(result.status, NativeWindowHostStatus::Unsupported);
            assert_eq!(result.reason, NativeWindowHostReason::PlatformUnsupported);
            assert_eq!(result.mode, "none");
        }
    }
}
