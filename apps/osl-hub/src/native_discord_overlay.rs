//! OSL-owned, capability-minimal native Discord composer overlay.
//!
//! The overlay is a separate local Tauri window. It never adopts, reparents,
//! scrapes message history, or accesses Discord credentials, cookies, tokens,
//! private APIs, or process memory. The native adapter reads bounded visible
//! accessibility names/focus/bounds and checks only whether the current
//! composer is empty or contains its own fixed marker. It never retains or
//! returns that value. After one explicit gesture, it types the fixed
//! non-secret marker through ordinary Windows input.

use osl_privacy_hub::service_host::ActiveServiceHost;
use osl_privacy_hub::{
    broker::HubBrokerState,
    core_bridge::HubCoreState,
    native_window_host::{NativeDiscordOverlayTarget, NativeWindowHostState},
};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{webview::NewWindowResponse, window::Color, Emitter, Manager, WebviewUrl};
// Tauri's own placement types, used only by the non-Windows placement stub: on
// Windows every protected placement goes through `DeferWindowPos`.
#[cfg(not(target_os = "windows"))]
use tauri::{PhysicalPosition, PhysicalSize};
use zeroize::Zeroizing;

use osl_privacy_hub::native_discord_adapter::{
    rehydrated_row_rects, AccessibilityBounds, NativeDiscordComposerState,
};

pub(crate) const OVERLAY_LABEL: &str = "native-discord-overlay";
pub(crate) const SHIELD_LABEL: &str = "native-discord-shield";
const OVERLAY_ASSET: &str = "overlay.html";
const SHIELD_ASSET: &str = "shield.html";
const FIRST_GUARD_GRACE: Duration = Duration::from_secs(3);
const NATIVE_SURFACE_CHANGED_EVENT: &str = "osl://native-surface-changed";
const OVERLAY_REFOCUS_EVENT: &str = "osl://native-discord-overlay-refocus";
const OVERLAY_CLOSED_EVENT: &str = "osl://native-discord-overlay-closed";
/// Retained-WebView session boundary. `false` orders the protected renderer to
/// discard every byte of the ended session; `true` tells it a freshly verified
/// session is ready so it can paint without waiting for its own retry backoff.
/// The payload carries no context token, identity, or conversation content.
const OVERLAY_SESSION_EVENT: &str = "osl://native-discord-overlay-session";
/// The protected composer is on screen and cannot receive the operator's
/// keystrokes, or can again. `{ reason, unreachable }`: which condition's edge
/// produced the message, and whether the composer is unreachable *right now*
/// across every condition. Both fields are facts about window ordering and
/// keyboard focus; neither carries a context token, identity, or conversation
/// content, and `reason` is always one of the fixed strings below.
///
/// This exists because the alternative was silence: the one state in which the
/// protected composer is on screen, owned, positioned and still unreachable for
/// input used to be reported to nobody at all.
///
/// The payload used to be a bare boolean published independently by two latches,
/// so a retraction could not be attributed to the condition that sent it and the
/// renderer reconstructed the answer by counting raises against retractions.
/// That was only ever correct because both setters happened to be edge-only --
/// an implicit invariant nothing stated and nothing tested. `unreachable` is now
/// the *level*, computed here from both latches, so the renderer sets a boolean
/// and a duplicated or dropped edge cannot desynchronise it.
const OVERLAY_COMPOSER_UNREACHABLE_EVENT: &str = "osl://native-discord-composer-unreachable";
/// Discord is drawing above OSL's composer: the two windows are in different
/// z-order bands, so no `SetWindowPos` insertion can decide their order.
const COMPOSER_UNREACHABLE_ZORDER_BAND: &str = "zorder-band";
/// Windows refused OSL's composer the keyboard, so it is visible but the
/// keystrokes are going somewhere else.
const COMPOSER_UNREACHABLE_KEYBOARD_FOCUS: &str = "keyboard-focus";
/// The session ended, so there is no composer to be unreachable. Only ever sent
/// with `unreachable: false`.
const COMPOSER_UNREACHABLE_SESSION_ENDED: &str = "session-ended";
/// Whether OSL has surrendered the band of screen Discord's real message box
/// occupies. `true` means the protected surface covers the transcript rows only:
/// Discord's own box is fully exposed, receives the keystrokes, and the
/// protected renderer must not draw a composer into a window that no longer
/// covers one. `false` is the ordinary engaged surface.
///
/// A bare boolean about geometry, edges only, no token, identity or content.
///
/// Deliberately a native fact on its own event rather than the renderer
/// re-deriving it from the lock. The renderer hiding its composer while the
/// window still covers Discord's message box is the previously-rejected failure
/// -- an invisible surface swallowing the clicks aimed at Discord's real box --
/// so the signal has to come from the writer that actually vacated the band.
const OVERLAY_COMPOSER_BAND_EVENT: &str = "osl://native-discord-composer-band-surrendered";
/// The band of Discord rows OSL is painting over moved. The renderer cannot see
/// this for itself: a `wheel` event only fires over pixels OSL owns, so
/// scrolling or resizing Discord silently invalidates every row rectangle the
/// renderer is holding.
///
/// Emitted with NO payload -- it is a hint to re-ask, never a description of
/// what moved -- and strictly on the edge: `TranscriptBandTracker` answers true
/// only on the tick where the band's bounds differ from the last emitted ones,
/// or where a Discord move/resize the loop was already tracking has just come to
/// rest. A steady tick emits nothing at all.
const NATIVE_DISCORD_ROWS_MOVED_EVENT: &str = "osl://native-discord-rows-moved";

/// Fixed labels for the legs of one transcript-rehydration command that run
/// BEFORE the accessibility read.
///
/// Every one of these used to be a silent `Err` straight back to the renderer,
/// where it landed in a memory-only journal. The result was a display feature
/// that could refuse an operator's eye on any of six preconditions and leave not
/// one byte of evidence anywhere -- indistinguishable, from outside, from a
/// conversation with nothing to paint. `REHYDRATE_ENTERED` is the load-bearing
/// one: with it, "the renderer never asked" and "the backend refused" stop being
/// the same observation.
///
/// PRIVACY: `&'static str` chosen here, plus at most one `usize`. The scope name
/// is never one of them -- Discord puts the conversation name in the composer's
/// accessible name, so no name may reach a trail -- and neither is any
/// rectangle, identifier or error string.
pub const REHYDRATE_ENTERED: &str = "rehydrate_entered";
pub const REHYDRATE_REFUSED_CALLER: &str = "rehydrate_refused_caller";
pub const REHYDRATE_REFUSED_SCOPE: &str = "rehydrate_refused_scope";
pub const REHYDRATE_CONTEXT_UNAVAILABLE: &str = "rehydrate_context_unavailable";
pub const REHYDRATE_OWNER_UNAVAILABLE: &str = "rehydrate_owner_unavailable";
pub const REHYDRATE_SCOPE_BINDING_UNAVAILABLE: &str = "rehydrate_scope_binding_unavailable";
/// The bounded accessibility read itself could not be started or was refused --
/// including by the single non-blocking Discord accessibility operation gate,
/// which any other in-flight accessibility operation holds.
pub const REHYDRATE_READ_UNAVAILABLE: &str = "rehydrate_read_unavailable";
pub const REHYDRATE_CONTEXT_CHANGED: &str = "rehydrate_context_changed";
/// The overlay window's own frame could not be measured, so no row can be
/// expressed in the renderer's coordinate space and nothing may be painted.
pub const REHYDRATE_FRAME_ABSENT: &str = "rehydrate_frame_absent";
/// Rows carrying plaintext that the overlay window really does contain. This is
/// the number the eye can paint, and the last count before the DTO ships.
pub const REHYDRATE_ROWS_PLACED: &str = "rehydrate_rows_placed";
/// Rows carrying plaintext that OSL deliberately refused to place, because the
/// overlay window is not over them yet. Painting these anywhere would be
/// decrypted text over the wrong Discord row.
pub const REHYDRATE_ROWS_UNPLACEABLE: &str = "rehydrate_rows_unplaceable";
pub const REHYDRATE_SHIPPED: &str = "rehydrate_shipped";

pub(crate) fn same_native_surface_target(
    current: NativeDiscordOverlayTarget,
    expected: NativeDiscordOverlayTarget,
) -> bool {
    current.generation == expected.generation
        && current.window == expected.window
        && current.rect == expected.rect
        && current.trusted_parent == expected.trusted_parent
}

#[cfg(all(feature = "discord-qa-shell", target_os = "windows"))]
fn active_confirm_overlay_target_after_focus(
    host: &NativeWindowHostState,
    owner: &str,
    expected: NativeDiscordOverlayTarget,
) -> Result<NativeDiscordOverlayTarget, String> {
    host.validate_current_discord_overlay_target_identity(owner, expected)?;
    Ok(NativeDiscordOverlayTarget {
        foreground: exact_window_is_foreground(expected.window),
        ..expected
    })
}

#[cfg(not(all(feature = "discord-qa-shell", target_os = "windows")))]
fn active_confirm_overlay_target_after_focus(
    host: &NativeWindowHostState,
    owner: &str,
    _expected: NativeDiscordOverlayTarget,
) -> Result<NativeDiscordOverlayTarget, String> {
    host.discord_overlay_target(owner)
}

#[cfg(feature = "discord-qa-shell")]
fn active_overlay_capture_protection() -> runtime::ScreenshotProtection {
    runtime::ScreenshotProtection::Off
}

#[cfg(not(feature = "discord-qa-shell"))]
fn active_overlay_capture_protection() -> runtime::ScreenshotProtection {
    runtime::ScreenshotProtection::On
}

/// The last label written, so a stage the guard restates on every 16 ms pass
/// costs an atomic compare rather than a write syscall.
#[cfg(feature = "discord-qa-shell")]
static QA_LAST_OVERLAY_WINDOW_STAGE: Mutex<Option<&'static str>> = Mutex::new(None);

/// Append one fixed label to the QA overlay-window trail.
///
/// This used to `std::fs::write`, which truncates: the file was a
/// latest-value marker, not a trail, so a later pass's `guard_started` erased
/// the `focus_failed` that preceded it and the focus outcome of an open could
/// not be read back at all -- it had to be inferred from the *absence* of
/// renderer breadcrumbs in a different file. Appending makes the sequence
/// directly observable.
///
/// Still `&'static str`, still fixed labels chosen at the call site: no draft,
/// no plaintext, no conversation content, nothing operator-derived can reach
/// this file. Consecutive repeats are collapsed, because several of these
/// labels are restated on every guard pass and an append per tick would be both
/// unbounded growth and a per-tick write on the guard thread.
#[cfg(feature = "discord-qa-shell")]
fn qa_overlay_window_stage(stage: &'static str) {
    {
        let mut last = QA_LAST_OVERLAY_WINDOW_STAGE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if *last == Some(stage) {
            return;
        }
        *last = Some(stage);
    }
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("osl-discord-qa-overlay-window-stage.txt"))
    {
        let _ = writeln!(file, "{stage}");
    }
}

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, CallWindowProcW, DefWindowProcW, DeferWindowPos, EndDeferWindowPos,
    GetAncestor, GetCursorPos, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, GA_ROOT, GWLP_HWNDPARENT, GWLP_WNDPROC, GWL_EXSTYLE,
    GWL_STYLE, GW_HWNDNEXT, GW_HWNDPREV, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_FRAMECHANGED,
    SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    WM_NCCALCSIZE, WM_NCDESTROY, WM_NCPAINT, WNDPROC, WS_EX_APPWINDOW, WS_VISIBLE,
};
// Windows refuses a foreground change from a process that does not own the
// foreground and did not receive the last input event: `SetForegroundWindow`
// returns 0 and does nothing at all. Sharing the foreground thread's input queue
// for the duration of that one call is the documented, non-injecting way out of
// it -- it moves focus without moving the operator's cursor and without
// synthesizing a single keystroke.
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
// Production forbids a topmost protected window outright, while the disposable
// QA shell deliberately raises its composer into that band. Both builds read the
// bit: it is the cheap proof of which z-order band a window currently sits in,
// and correcting the stack must never move a window between bands.
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::WS_EX_TOPMOST;

#[cfg(any(target_os = "windows", test))]
const NATIVE_OVERLAY_FRAME_STYLE_MASK: isize = 0x00cf_0000;

/// The extended-style half of the same frame. `WS_EX_DLGMODALFRAME`,
/// `WS_EX_WINDOWEDGE`, `WS_EX_CLIENTEDGE` and `WS_EX_STATICEDGE` are the only
/// `WS_EX_*` bits that draw non-client edges, and a raised or client edge alone
/// paints a light line along the top of an otherwise frameless surface. tao
/// rebuilds `GWL_EXSTYLE` with `WS_EX_WINDOWEDGE` every time it applies its
/// cached window flags, so this word drifts exactly as `GWL_STYLE` does.
///
/// Deliberately disjoint from every bit this crate reasons about elsewhere:
/// `WS_EX_TOPMOST` (0x8) and `WS_EX_APPWINDOW` (0x4_0000) carry the ownership
/// contract and `WS_EX_NOACTIVATE` (0x800_0000) keeps the shield from stealing
/// typing focus. `WS_EX_NOREDIRECTIONBITMAP` (0x20_0000) is listed with them
/// only because it must never be cleared here either -- nothing in this crate
/// or in tao sets it, so it is not what makes the protected composer
/// see-through: that comes from tao's creation-time DWM blur-behind region plus
/// WebView2's transparent default background colour, both driven by the
/// builder's `transparent(true)`. None of these bits are cleared here.
#[cfg(any(target_os = "windows", test))]
const NATIVE_OVERLAY_FRAME_EX_STYLE_MASK: isize = 0x0002_0301;

/// How many times a frame correction may be re-read before it is a failure.
/// tao rewrites both style words from the event loop while this runs on an
/// overlay worker, so a single lost race must be retried rather than fail the
/// session closed and take the composer off screen.
#[cfg(target_os = "windows")]
const NATIVE_OVERLAY_FRAME_ATTEMPTS: usize = 4;

#[cfg(target_os = "windows")]
const NATIVE_OVERLAY_FRAME_RETRY_DELAY: Duration = Duration::from_millis(4);

/// Bounded wait for a queued Tauri reveal to actually reach the screen.
/// Mirrors the hide path's budget: generous enough for a busy event loop,
/// short enough that no caller waits on a wedged one.
const PROTECTED_REVEAL_SETTLE_ATTEMPTS: usize = 60;

const PROTECTED_REVEAL_SETTLE_DELAY: Duration = Duration::from_millis(10);

#[cfg(any(target_os = "windows", test))]
fn frameless_native_overlay_style(style: isize) -> isize {
    style & !NATIVE_OVERLAY_FRAME_STYLE_MASK
}

#[cfg(any(target_os = "windows", test))]
fn frameless_native_overlay_ex_style(ex_style: isize) -> isize {
    ex_style & !NATIVE_OVERLAY_FRAME_EX_STYLE_MASK
}

#[derive(Debug)]
struct OverlaySession {
    epoch: u64,
    context_token: Zeroizing<String>,
    host: ActiveServiceHost,
    phase: OverlayPhase,
}

#[derive(Debug)]
struct OverlaySessionSnapshot {
    epoch: u64,
    context_token: Zeroizing<String>,
    host: ActiveServiceHost,
    phase: OverlayPhase,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OverlayPhase {
    Guarding,
    Ready,
    #[cfg(any(test, feature = "discord-qa-shell"))]
    Dormant,
}

#[cfg(any(test, feature = "discord-qa-shell"))]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum QaDormantReuse {
    None,
    Exact,
    Changed,
}

pub(crate) struct OverlaySessionState {
    inner: Mutex<Option<OverlaySession>>,
    next_epoch: AtomicU64,
    covertext_enabled: AtomicBool,
    carrier_placement_active: AtomicBool,
    /// The eye, as the guard last resolved it from the authoritative scope
    /// policy. This is the ONLY thing that decides whether OSL displays
    /// anything over Discord's message rows, and therefore the only thing that
    /// decides whether an opaque capture shield exists at all.
    ///
    /// Starts closed. A session that has not yet proven the eye is on paints
    /// nothing and shields nothing, which is exactly unmodified Discord.
    ///
    /// A cache of the authoritative per-scope policy, never the policy itself:
    /// the guard re-resolves it, and a torn-down session leaves it closed
    /// because nothing is on screen to be right or wrong about.
    protected_display_visible: AtomicBool,
    /// The lock: whether what the operator types is encrypted, and therefore
    /// whether OSL owns a composer over Discord's real message box.
    ///
    /// Deliberately a flag INSIDE the session rather than the session's own
    /// existence. Lowering the lock used to end the session, which took the
    /// decrypted display down with it -- and display is the eye's business, not
    /// the lock's.
    lock_engaged: AtomicBool,
    /// Whether the composer is currently in a **z-order** band the guard cannot
    /// correct: the composer and the borrowed Discord window are in different
    /// bands, so no `SetWindowPos` insertion can decide their order and the
    /// composer may be sitting behind Discord with the operator's clicks and
    /// keystrokes landing in Discord's own message box.
    ///
    /// A latch, not a counter: the surrender is re-evaluated on the guard's
    /// bounded probe cadence, and only the edges are reported so a state that
    /// persists cannot become a message storm. Nothing here is content: it is
    /// one boolean about window ordering.
    ///
    /// NOT the same thing as surrendering the composer *band of screen*
    /// (`ProtectedSurfacePresence::RowsOnly`), and the two must never be
    /// conflated -- it was called `composer_band_surrendered` and that name was
    /// the whole of the trap. This one is a hazard: the composer is over
    /// Discord's message box, the lock is up, and the keystrokes are going to
    /// Discord anyway. The other is the intended state with the lock down: OSL
    /// has deliberately vacated that box so Discord can have it. Raising this
    /// warning for that state would tell the operator their typing is leaking at
    /// exactly the moment typing into Discord is what they asked for.
    composer_zorder_surrendered: AtomicBool,
    /// Whether OSL is, right now, actually painting decrypted rows over
    /// Discord's message list -- published by the guard from the same
    /// `painted_rows` it derives the opaque shield from.
    ///
    /// The eye is a *setting*; this is the *fact*. `disengage_lock` is the one
    /// caller, and it needs the fact: "the operator has decrypt-display enabled"
    /// is true by default and says nothing about whether there is a protected
    /// pixel left on screen once the composer is switched off.
    protected_rows_painted: AtomicBool,
    /// Whether Windows refused to give the protected composer the keyboard, so
    /// the composer is on screen but the operator's keystrokes are still going
    /// somewhere else -- which, at the moment they engage from Discord's own
    /// message box, means going to Discord in the clear.
    ///
    /// A latch on the same contract as `composer_zorder_surrendered`: one boolean
    /// about *input reachability*, edges only, no token, identity or content.
    /// It exists because the alternative to reporting it was the alternative
    /// this file already rejected once -- ending the session, which takes the
    /// composer off screen and leaves the operator typing into Discord anyway,
    /// only now with nothing on screen to notice.
    protected_focus_refused: AtomicBool,
}

pub(crate) struct CarrierPlacementGuard<'a> {
    state: &'a OverlaySessionState,
}

impl Drop for CarrierPlacementGuard<'_> {
    fn drop(&mut self) {
        self.state
            .carrier_placement_active
            .store(false, Ordering::Release);
    }
}

impl Default for OverlaySessionState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            next_epoch: AtomicU64::new(0),
            covertext_enabled: AtomicBool::new(true),
            carrier_placement_active: AtomicBool::new(false),
            protected_display_visible: AtomicBool::new(false),
            lock_engaged: AtomicBool::new(false),
            composer_zorder_surrendered: AtomicBool::new(false),
            protected_rows_painted: AtomicBool::new(false),
            protected_focus_refused: AtomicBool::new(false),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct OverlayRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct AdaptiveGeometryKey {
    discord_rect: [i32; 4],
    surface_bounds: AccessibilityBounds,
    input_bounds: AccessibilityBounds,
    presentation_bounds: AccessibilityBounds,
    overlay_rect: OverlayRect,
    scale_milli: u32,
}

fn bounded_scale_milli(scale: f64) -> Option<u32> {
    if !scale.is_finite() || !(0.5..=8.0).contains(&scale) {
        return None;
    }
    let scaled = (scale * 1_000.0).round();
    (scaled >= 500.0 && scaled <= 8_000.0).then_some(scaled as u32)
}

/// `overlay_rect` is the surface the caller has already resolved for this pass,
/// so the identity the guard compares includes what OSL is actually covering --
/// the lock, the eye and every painted row are all folded into it.
fn adaptive_geometry_key(
    discord_rect: [i32; 4],
    surface_bounds: AccessibilityBounds,
    input_bounds: AccessibilityBounds,
    presentation_bounds: AccessibilityBounds,
    overlay_rect: OverlayRect,
    scale_milli: u32,
) -> Option<AdaptiveGeometryKey> {
    let surface_valid = surface_bounds.right > surface_bounds.left
        && surface_bounds.bottom > surface_bounds.top
        && surface_bounds.left >= discord_rect[0]
        && surface_bounds.top >= discord_rect[1]
        && surface_bounds.right <= discord_rect[2]
        && surface_bounds.bottom <= discord_rect[3];
    let input_valid = input_bounds.right > input_bounds.left
        && input_bounds.bottom > input_bounds.top
        && input_bounds.left >= surface_bounds.left
        && input_bounds.top >= surface_bounds.top
        && input_bounds.right <= surface_bounds.right
        && input_bounds.bottom <= surface_bounds.bottom;
    let presentation_valid = presentation_bounds.right > presentation_bounds.left
        && presentation_bounds.bottom > presentation_bounds.top
        && presentation_bounds.left >= surface_bounds.left
        && presentation_bounds.top >= surface_bounds.top
        && presentation_bounds.right <= surface_bounds.right
        && presentation_bounds.bottom <= surface_bounds.bottom;
    (surface_valid && input_valid && presentation_valid && (500..=8_000).contains(&scale_milli))
        .then_some(AdaptiveGeometryKey {
            discord_rect,
            surface_bounds,
            input_bounds,
            presentation_bounds,
            overlay_rect,
            scale_milli,
        })
}

/// The protected surface OSL puts over Discord.
///
/// Its floor is exactly Discord's measured composer rectangle and nothing else,
/// because the composer is the only place OSL *must* own: the operator's
/// plaintext may never enter Discord's real message box. There is deliberately
/// no fallback that spans the message band. The previous header-to-composer
/// surface is what made the lock black out the conversation, fight Discord for
/// geometry, and swallow the scrollback -- eye off is now literally unmodified
/// Discord because OSL has no window there at all.
///
/// `painted_top` is the top of the highest Discord row OSL is currently painting
/// decrypted text over. It extends this surface upward by exactly that much and
/// by nothing more. `None` -- which is every production session today, and every
/// session with the eye off -- leaves the surface the size of the composer.
fn protected_overlay_rect(
    discord: [i32; 4],
    composer: Option<AccessibilityBounds>,
    painted_top: Option<i32>,
) -> Option<OverlayRect> {
    let rect = verified_composer_overlay_rect(discord, composer?)?;
    let Some(top) = painted_top else {
        return Some(rect);
    };
    let top = top.max(discord[1]);
    if top >= rect.y {
        return Some(rect);
    }
    let grown = rect.y.checked_sub(top)?;
    let height = i32::try_from(rect.height).ok()?.checked_add(grown)?;
    Some(OverlayRect {
        x: rect.x,
        y: top,
        width: rect.width,
        height: height.try_into().ok()?,
    })
}

/// The whole protected surface: the composer rectangle, extended upward over the
/// rows OSL paints.
///
/// Deliberately lock-free, and that is the product model rather than a
/// simplification. The lock controls **encryption** and nothing else; it does
/// not decide whether OSL has a composer on screen. This function used to take
/// it and answer "painted rows only" when it was down -- which, in the common
/// case of nothing painted, was `None`, and `None` here is the protected
/// composer leaving the screen. That made the operator's typing surface
/// conditional on a setting that has nothing to do with typing, and it is one of
/// the two halves of the composer being able to disappear at all (the other was
/// the guard bail this used to pair with).
///
/// The composer is now present whenever a session is, full stop.
fn protected_surface_rect(
    discord: [i32; 4],
    composer: Option<AccessibilityBounds>,
    painted: &[[i32; 4]],
) -> Option<OverlayRect> {
    protected_overlay_rect(discord, composer, painted_rows_top(painted))
}

/// The same surface with the composer band cut off its bottom: everything OSL
/// paints **above** Discord's real message box, and not one pixel of the box.
///
/// This is the display half of the product model. The eye is the only control
/// over what OSL paints, and the lock is the only control over encryption, so a
/// lock that is down must not take the eye's painted rows with it -- and the
/// composer and the painted rows are the same window. Splitting the window at
/// the composer's own top edge is what lets one surface serve both rules: the
/// rows stay covered and painted, and Discord's message box is handed back
/// whole.
///
/// Lock-free, exactly like `protected_surface_rect`, and derived from the same
/// two functions rather than from a second measurement -- so the band this
/// returns and the composer band it surrenders can never disagree about where
/// the boundary is. *Which* of the two the guard asks for is a presence question
/// (`ProtectedSurfacePresence`), and presence is the guard's.
///
/// `None` means there is nothing above the message box to own: no row is
/// painted, or every painted row starts at or below the composer's top edge. The
/// caller takes the pair off screen rather than treating that as invalid
/// geometry -- with the lock down, "no rows to display" really is "nothing to be
/// on screen for".
///
/// The bottom edge is exclusive by construction: the returned rectangle's
/// `y + height` is exactly the composer's `top`, so the covered rows are
/// `[top, composer.top - 1]` and the composer band starts at the first row this
/// surface does not occupy. That equality is the safety property, and it is
/// asserted rather than described.
fn protected_rows_band_rect(
    discord: [i32; 4],
    composer: Option<AccessibilityBounds>,
    painted: &[[i32; 4]],
) -> Option<OverlayRect> {
    let composer_rect = verified_composer_overlay_rect(discord, composer?)?;
    let full = protected_surface_rect(discord, composer, painted)?;
    let height = composer_rect.y.checked_sub(full.y)?;
    (height > 0).then_some(OverlayRect {
        x: full.x,
        y: full.y,
        width: full.width,
        height: height.try_into().ok()?,
    })
}

/// Every painted row, clipped so none of them reaches into the composer band.
///
/// The shield is a second window and it is placed from these rectangles, so
/// clamping the surface alone would leave Discord's message box uncovered by the
/// composer and covered by the shield -- which is the same leak with a different
/// window on top of it. One clamp, applied before anything downstream reads the
/// rows, so the surface, the shield, its clipping region and the geometry key all
/// describe the same band.
///
/// Rows are dropped rather than kept degenerate: a zero-height rectangle is not
/// a row OSL can paint, and `painted_rows_bounds` would still grow the shield to
/// contain it.
fn rows_above_the_composer_band(painted: &[[i32; 4]], composer_top: i32) -> Vec<[i32; 4]> {
    painted
        .iter()
        .filter_map(|rect| {
            let bottom = rect[3].min(composer_top);
            (bottom > rect[1]).then_some([rect[0], rect[1], rect[2], bottom])
        })
        .collect()
}

fn verified_composer_overlay_rect(
    discord: [i32; 4],
    composer: AccessibilityBounds,
) -> Option<OverlayRect> {
    let discord_width = discord[2].checked_sub(discord[0])?;
    let discord_height = discord[3].checked_sub(discord[1])?;
    let width = composer.right.checked_sub(composer.left)?;
    let height = composer.bottom.checked_sub(composer.top)?;
    if composer.left < discord[0]
        || composer.top < discord[1] + discord_height / 2
        || composer.right > discord[2]
        || composer.bottom > discord[3]
        || width < 320
        || width > discord_width
        || !(24..=200).contains(&height)
    {
        return None;
    }
    Some(OverlayRect {
        x: composer.left,
        y: composer.top,
        width: width.try_into().ok()?,
        height: height.try_into().ok()?,
    })
}

/// One shape for both builds. The QA shell differs only in which rows it can
/// locate, never in how the surface is derived from them.
fn active_overlay_rect_with_composer(
    discord: [i32; 4],
    composer: Option<AccessibilityBounds>,
    painted_top: Option<i32>,
) -> Option<OverlayRect> {
    protected_overlay_rect(discord, composer, painted_top)
}

fn bundled_overlay_navigation(url: &url::Url) -> bool {
    let local_origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    local_origin
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "/overlay.html" | "/overlay.html/")
}

fn bundled_shield_navigation(url: &url::Url) -> bool {
    let local_origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    local_origin
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "/shield.html" | "/shield.html/")
}

impl OverlaySessionState {
    fn snapshot(session: &OverlaySession) -> OverlaySessionSnapshot {
        OverlaySessionSnapshot {
            epoch: session.epoch,
            context_token: Zeroizing::new(session.context_token.as_str().to_owned()),
            host: session.host.clone(),
            phase: session.phase,
        }
    }

    pub(crate) fn covertext_enabled(&self) -> bool {
        self.covertext_enabled.load(Ordering::Acquire)
    }

    pub(crate) fn set_covertext_enabled(&self, enabled: bool) {
        self.covertext_enabled.store(enabled, Ordering::Release);
    }

    pub(crate) fn protected_display_visible(&self) -> bool {
        self.protected_display_visible.load(Ordering::Acquire)
    }

    fn set_protected_display_visible(&self, visible: bool) {
        self.protected_display_visible
            .store(visible, Ordering::Release);
    }

    pub(crate) fn lock_engaged(&self) -> bool {
        self.lock_engaged.load(Ordering::Acquire)
    }

    /// Whether OSL currently has a protected pixel on screen over Discord's
    /// message rows: the eye is on **and** the guard is actually painting rows
    /// through it.
    ///
    /// Both halves are load-bearing and only one of them used to be read. The
    /// eye is a per-scope setting that defaults to *enabled*
    /// (`security::scope_security`), so "the eye is on" is true in essentially
    /// every session and says nothing whatsoever about whether anything is being
    /// displayed. Every production session paints no rows at all until a
    /// transcript rehydration has completed for this exact scope and window
    /// generation, and `painted_message_row_rects` answers empty until then.
    fn protected_pixels_on_screen(&self) -> bool {
        self.protected_display_visible() && self.protected_rows_painted.load(Ordering::Acquire)
    }

    /// Record whether the guard is painting decrypted rows this pass.
    fn set_protected_rows_painted(&self, painted: bool) {
        self.protected_rows_painted
            .store(painted, Ordering::Release);
    }

    /// Whether the last full pass actually had rows to paint.
    ///
    /// The *fact*, and the presence rule needs the fact rather than the eye
    /// setting: the eye defaults to enabled, so a session displaying nothing at
    /// all would otherwise hold a surface on screen with the lock down. An atomic
    /// load, so the 16 ms tick can ask it.
    ///
    /// Deliberately the raw fact and deliberately separate from
    /// `protected_pixels_on_screen`, which is the conjunction with the eye
    /// setting. Once the composer band can be surrendered while the rows band
    /// keeps painting, "the eye has rows on screen" and "OSL owns Discord's
    /// message box" are independent, and a caller that wants one must not be
    /// handed the other.
    pub(crate) fn protected_rows_painted(&self) -> bool {
        self.protected_rows_painted.load(Ordering::Acquire)
    }

    /// Lower the lock without ending the session, and answer whether OSL is
    /// still displaying anything -- which is the only reason to keep the
    /// protected surface alive at all.
    ///
    /// The caller turns a `false` here into a full teardown and a `true` into
    /// "keep the session, the guard and the surface alive". That is why this
    /// predicate is the whole of the owner-reported defect: it answered with the
    /// eye *setting*, which is on by default, so switching the composer off left
    /// a live session whose guard kept the composer window sitting exactly over
    /// Discord's real message box -- with the hub rendering "Protected composer
    /// off". `protected_surface_rect` is deliberately lock-free, so nothing
    /// downstream was ever going to take that window off screen; the only thing
    /// that can is this answer, and it has to be about pixels rather than
    /// preferences.
    pub(crate) fn disengage_lock(&self) -> bool {
        self.lock_engaged.store(false, Ordering::Release);
        self.protected_pixels_on_screen()
    }

    pub(crate) fn carrier_placement_active(&self) -> bool {
        self.carrier_placement_active.load(Ordering::Acquire)
    }

    /// Record whether the composer's order is currently undecidable, and answer
    /// whether that is a *change*. Only a change is worth telling anyone about:
    /// the caller runs on a bounded probe cadence, so reporting the level
    /// instead of the edge would be a message every 200 ms for as long as the
    /// operator leaves the two windows in different bands.
    fn set_composer_zorder_surrendered(&self, surrendered: bool) -> bool {
        self.composer_zorder_surrendered
            .swap(surrendered, Ordering::AcqRel)
            != surrendered
    }

    pub(crate) fn composer_zorder_surrendered(&self) -> bool {
        self.composer_zorder_surrendered.load(Ordering::Acquire)
    }

    /// Whether the protected composer is on screen and unreachable for the
    /// operator's keystrokes, for any reason at all.
    ///
    /// One question, so one answer, computed where both latches live. The wire
    /// used to carry the two latches' edges independently, which left the only
    /// reader having to reconstruct this by counting raises against retractions
    /// -- correct only for as long as both setters stayed edge-only, which
    /// nothing stated and nothing tested.
    fn composer_is_unreachable(&self) -> bool {
        self.composer_zorder_surrendered.load(Ordering::Acquire)
            || self.protected_focus_refused.load(Ordering::Acquire)
    }

    /// Forget both unreachability latches.
    ///
    /// They are app-lifetime atomics and both setters are edge-only, so a
    /// session that ended while one was raised used to poison the next one: the
    /// next raise would be a no-op, nothing would be emitted, and the hub would
    /// show no warning while the composer really was unreachable.
    fn clear_composer_unreachable_latches(&self) {
        self.composer_zorder_surrendered
            .store(false, Ordering::Release);
        self.protected_focus_refused.store(false, Ordering::Release);
    }

    /// Record whether the composer currently holds the keyboard, and answer
    /// whether that is a *change*. Same edge-only discipline as the band latch,
    /// for the same reason: the focus outcome is re-evaluated on the guard's
    /// bounded open/reclaim cadence, and the level would be a repeat.
    fn set_protected_focus_refused(&self, refused: bool) -> bool {
        self.protected_focus_refused.swap(refused, Ordering::AcqRel) != refused
    }

    pub(crate) fn protected_focus_refused(&self) -> bool {
        self.protected_focus_refused.load(Ordering::Acquire)
    }

    pub(crate) fn begin_carrier_placement(&self) -> Result<CarrierPlacementGuard<'_>, String> {
        self.carrier_placement_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| CarrierPlacementGuard { state: self })
            .map_err(|_| "A protected Discord carrier is already being placed".to_owned())
    }

    pub(crate) fn activate(
        &self,
        context_token: String,
        host: ActiveServiceHost,
    ) -> Result<u64, String> {
        if context_token.is_empty() || context_token.len() > 256 {
            return Err("The native Discord protection context is invalid".to_owned());
        }
        let epoch = self
            .next_epoch
            .fetch_add(1, Ordering::AcqRel)
            .wrapping_add(1);
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        #[cfg(any(test, feature = "discord-qa-shell"))]
        if guard.as_ref().is_some_and(|session| {
            session.phase == OverlayPhase::Dormant
                && (session.context_token.as_str() != context_token || session.host != host)
        }) {
            return Err("The retained Discord overlay belongs to a different context".to_owned());
        }
        *guard = Some(OverlaySession {
            epoch,
            context_token: Zeroizing::new(context_token),
            host,
            phase: OverlayPhase::Guarding,
        });
        self.lock_engaged.store(true, Ordering::Release);
        self.set_protected_rows_painted(false);
        // A fresh session starts with nothing proved about reachability, so it
        // may not inherit the previous session's latch. Both setters are
        // edge-only, so an inherited `true` would swallow the next genuine raise.
        self.clear_composer_unreachable_latches();
        Ok(epoch)
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        // An ended session displays nothing, so it shields nothing either.
        self.set_protected_display_visible(false);
        self.set_protected_rows_painted(false);
        self.lock_engaged.store(false, Ordering::Release);
        // An ended session has no composer, so it cannot have an unreachable
        // one. `retract_composer_unreachable` is what tells the hub; this is the
        // state truth, and it has to be here as well because `clear` is reached
        // from paths that have no `AppHandle` to emit on.
        self.clear_composer_unreachable_latches();
        self.next_epoch.fetch_add(1, Ordering::AcqRel);
    }

    #[cfg(any(test, feature = "discord-qa-shell"))]
    pub(crate) fn qa_dormant_reuse(
        &self,
        context_token: &str,
        host: &ActiveServiceHost,
    ) -> Result<QaDormantReuse, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        Ok(match guard.as_ref() {
            Some(session) if session.phase == OverlayPhase::Dormant => {
                if session.context_token.as_str() == context_token && &session.host == host {
                    QaDormantReuse::Exact
                } else {
                    QaDormantReuse::Changed
                }
            }
            _ => QaDormantReuse::None,
        })
    }

    #[cfg(any(test, feature = "discord-qa-shell"))]
    fn suspend_for_qa_toggle(&self) -> Result<(), String> {
        if self.carrier_placement_active() {
            return Err("A protected Discord message is still being placed".to_owned());
        }
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_mut()
            .filter(|session| session.phase == OverlayPhase::Ready)
            .ok_or_else(|| "The native Discord overlay is not ready to close".to_owned())?;
        session.phase = OverlayPhase::Dormant;
        // The lock is down either way. The disposable QA harness additionally
        // parks the whole session, which is the one place a lock toggle still
        // takes the display with it; the shipping path above does not.
        let _ = self.disengage_lock();
        self.next_epoch.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    pub(crate) fn with_bootstrap_context<T>(
        &self,
        operation: impl FnOnce(&str, &ActiveServiceHost) -> Result<T, String>,
    ) -> Result<T, String> {
        let snapshot = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
            let session = guard
                .as_ref()
                .filter(|session| {
                    #[cfg(any(test, feature = "discord-qa-shell"))]
                    {
                        session.phase != OverlayPhase::Dormant
                    }
                    #[cfg(not(any(test, feature = "discord-qa-shell")))]
                    {
                        let _ = session;
                        true
                    }
                })
                .ok_or_else(|| "The native Discord overlay is not active".to_owned())?;
            Self::snapshot(session)
        };
        operation(snapshot.context_token.as_str(), &snapshot.host)
    }

    pub(crate) fn with_context<T>(
        &self,
        operation: impl FnOnce(&str, &ActiveServiceHost) -> Result<T, String>,
    ) -> Result<T, String> {
        let snapshot = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
            let session = guard
                .as_ref()
                .filter(|session| session.phase == OverlayPhase::Ready)
                .ok_or_else(|| {
                    "The native Discord overlay has not passed its safety check".to_owned()
                })?;
            Self::snapshot(session)
        };
        debug_assert_eq!(snapshot.phase, OverlayPhase::Ready);
        operation(snapshot.context_token.as_str(), &snapshot.host)
    }

    fn mark_ready(&self, epoch: u64, expected_host: &ActiveServiceHost) -> Result<(), String> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
        let session = guard
            .as_mut()
            .filter(|session| session.epoch == epoch && &session.host == expected_host)
            .ok_or_else(|| "The native Discord overlay context changed".to_owned())?;
        session.phase = OverlayPhase::Ready;
        Ok(())
    }

    pub(crate) fn validated_marker(
        &self,
        validate: impl FnOnce(&str, &ActiveServiceHost) -> Result<(), String>,
    ) -> Result<(u64, ActiveServiceHost), String> {
        let snapshot = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
            let session = guard
                .as_ref()
                .filter(|session| session.phase == OverlayPhase::Ready)
                .ok_or_else(|| "The native Discord overlay is not active".to_owned())?;
            Self::snapshot(session)
        };
        debug_assert_eq!(snapshot.phase, OverlayPhase::Ready);
        validate(snapshot.context_token.as_str(), &snapshot.host)?;
        Ok((snapshot.epoch, snapshot.host))
    }

    pub(crate) fn validate_marker(
        &self,
        epoch: u64,
        expected_host: &ActiveServiceHost,
        validate: impl FnOnce(&str, &ActiveServiceHost) -> Result<(), String>,
    ) -> Result<(), String> {
        let snapshot = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
            let session = guard
                .as_ref()
                .filter(|session| {
                    session.phase == OverlayPhase::Ready
                        && session.epoch == epoch
                        && &session.host == expected_host
                })
                .ok_or_else(|| "The native Discord overlay context changed".to_owned())?;
            Self::snapshot(session)
        };
        debug_assert_eq!(snapshot.phase, OverlayPhase::Ready);
        validate(snapshot.context_token.as_str(), &snapshot.host)
    }

    fn is_epoch(&self, epoch: u64) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| {
                guard.as_ref().map(|session| {
                    session.epoch == epoch && {
                        #[cfg(any(test, feature = "discord-qa-shell"))]
                        {
                            session.phase != OverlayPhase::Dormant
                        }
                        #[cfg(not(any(test, feature = "discord-qa-shell")))]
                        {
                            true
                        }
                    }
                })
            })
            .unwrap_or(false)
    }

    fn is_ready(&self, epoch: u64) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|session| session.epoch == epoch && session.phase == OverlayPhase::Ready)
            })
            .unwrap_or(false)
    }

    #[cfg(feature = "discord-qa-shell")]
    pub(crate) fn wait_until_ready(
        &self,
        epoch: u64,
        expected_host: &ActiveServiceHost,
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            let phase = {
                let guard = self
                    .inner
                    .lock()
                    .map_err(|_| "The native Discord overlay state is unavailable".to_owned())?;
                let session = guard
                    .as_ref()
                    .filter(|session| {
                        session.epoch == epoch
                            && &session.host == expected_host
                            && session.phase != OverlayPhase::Dormant
                    })
                    .ok_or_else(|| "The native Discord overlay context changed".to_owned())?;
                session.phase
            };
            if phase == OverlayPhase::Ready {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(
                    "The native Discord overlay did not pass its safety check in time".to_owned(),
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

/// How often the guard may re-read the authoritative eye setting.
///
/// Deliberately not a per-tick read: resolving it opens OSL's own encrypted
/// scope policy. It is not a cadence over Discord either -- nothing here touches
/// an accessibility tree, a window style, a position or a z-order, and the
/// session mutex is snapshotted and released before the read runs.
const PROTECTED_DISPLAY_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

/// Resolve the eye from the authoritative per-scope policy.
///
/// `None` means it could not be resolved right now, which the caller treats as
/// "leave the last proven answer alone" rather than as a display change.
///
/// DEADLOCK: `with_context` copies the session facts out and releases the
/// session mutex before the closure runs, and nothing below takes a lock OSL's
/// UI thread holds while a cross-process call is in flight.
fn resolve_protected_display_visible(app: &tauri::AppHandle) -> Option<bool> {
    let broker = app.state::<HubBrokerState>();
    let scope = app
        .state::<OverlaySessionState>()
        .with_context(|context_token, _host| broker.scope_for_context(context_token))
        .ok()?;
    super::security::scope_security(scope)
        .ok()
        .map(|security| security.decrypt_display_enabled)
}

/// The Discord rows OSL is currently painting decrypted text over, in screen
/// coordinates. Empty means OSL is displaying nothing over the message list, so
/// there is nothing on screen to shield and no reason to own a pixel there.
///
/// Production sources this from `native_discord_adapter::rehydrated_row_rects`,
/// a cache written in exactly one place: inside the bounded, deadline-checked,
/// detached-thread one-shot row reader that already runs on a transcript
/// rehydration. Reading the cache costs a lock and a comparison, never
/// accessibility work, so the per-tick guard loop that calls this function
/// every tick never has to touch Discord's accessibility tree itself. The
/// cache answers empty until the first rehydration for the current scope and
/// window generation completes, and it is not refreshed by scrolling alone --
/// it reflects whatever that last completed read saw, exactly like the row
/// text it was read alongside. The QA shell adds its own just-sent carrier rows
/// on top of that same cache, so the QA build exercises both the per-row carrier
/// path and history rows this client never sent -- which is the only way the eye
/// can paint a row the operator scrolled back to, and the build the owner tests
/// in is exactly the one where that has to work.
fn painted_message_row_rects(
    app: &tauri::AppHandle,
    generation: u64,
    display_visible: bool,
) -> Vec<[i32; 4]> {
    if !display_visible {
        return Vec::new();
    }
    let Ok(scope_binding) = super::native_discord_scope_binding(app) else {
        return Vec::new();
    };
    #[cfg(feature = "discord-qa-shell")]
    {
        // Union, never replacement. The just-sent carriers are rows the QA shell
        // has proven for itself and may know about before any rehydration has
        // run; the rehydrated cache is every row the bounded transcript reader
        // actually found, including history this client never sent. Returning
        // only the first is what stopped the overlay window from ever being
        // grown over a history row, which made `overlay_relative_row_rect`
        // refuse it and shipped `row: null` for every row the eye should paint.
        let carriers = app
            .state::<NativeDiscordComposerState>()
            .verified_sent_carriers(&scope_binding, generation)
            .into_iter()
            .map(|row| {
                [
                    row.bounds.left,
                    row.bounds.top,
                    row.bounds.right,
                    row.bounds.bottom,
                ]
            })
            .collect::<Vec<_>>();
        return union_painted_row_rects(carriers, rehydrated_row_rects(&scope_binding, generation));
    }
    #[cfg(not(feature = "discord-qa-shell"))]
    {
        union_painted_row_rects(Vec::new(), rehydrated_row_rects(&scope_binding, generation))
    }
}

/// The screen area a row rectangle covers, and `0` for a rectangle that covers
/// nothing at all. Widened to `i64` so a garbage rectangle from either source
/// cannot overflow the comparison below.
fn painted_row_area(rect: [i32; 4]) -> i64 {
    let width = i64::from(rect[2]) - i64::from(rect[0]);
    let height = i64::from(rect[3]) - i64::from(rect[1]);
    if width <= 0 || height <= 0 {
        0
    } else {
        width * height
    }
}

/// Whether two rectangles describe the same Discord row. The two sources measure
/// the same row at different moments and can disagree by a pixel or two, so
/// equality is too strict; adjacent rows in a transcript touch but do not
/// meaningfully overlap, so any intersection at all is too loose. Half of the
/// smaller rectangle sits comfortably between the two.
fn same_painted_row(left: [i32; 4], right: [i32; 4]) -> bool {
    let overlap_width = i64::from(left[2].min(right[2])) - i64::from(left[0].max(right[0]));
    let overlap_height = i64::from(left[3].min(right[3])) - i64::from(left[1].max(right[1]));
    if overlap_width <= 0 || overlap_height <= 0 {
        return false;
    }
    let smaller = painted_row_area(left).min(painted_row_area(right));
    smaller > 0 && overlap_width * overlap_height * 2 >= smaller
}

/// Every distinct row either source proved, with degenerate rectangles dropped
/// and the same row counted once. `primary` keeps its own rectangle when both
/// sources describe one row, so the QA shell's own just-sent measurement still
/// wins for the rows it sent.
fn union_painted_row_rects(primary: Vec<[i32; 4]>, extra: Vec<[i32; 4]>) -> Vec<[i32; 4]> {
    let mut merged: Vec<[i32; 4]> = Vec::with_capacity(primary.len() + extra.len());
    for rect in primary.into_iter().chain(extra) {
        if painted_row_area(rect) <= 0 {
            continue;
        }
        if merged.iter().any(|kept| same_painted_row(*kept, rect)) {
            continue;
        }
        merged.push(rect);
    }
    merged
}

/// The top of the highest row OSL is painting, or `None` when it paints none.
fn painted_rows_top(rects: &[[i32; 4]]) -> Option<i32> {
    rects.iter().map(|rect| rect[1]).min()
}

/// The smallest rectangle containing everything OSL is painting over Discord's
/// message rows.
fn painted_rows_bounds(rects: &[[i32; 4]]) -> Option<OverlayRect> {
    let left = rects.iter().map(|rect| rect[0]).min()?;
    let top = rects.iter().map(|rect| rect[1]).min()?;
    let right = rects.iter().map(|rect| rect[2]).max()?;
    let bottom = rects.iter().map(|rect| rect[3]).max()?;
    let width = right.checked_sub(left)?;
    let height = bottom.checked_sub(top)?;
    (width > 0 && height > 0).then_some(OverlayRect {
        x: left,
        y: top,
        width: width.try_into().ok()?,
        height: height.try_into().ok()?,
    })
}

/// Edge detector for the transcript band the eye paints into. It holds only the
/// two facts the guard loop already has -- the band's bounds and whether
/// Discord's own rectangle is currently at rest -- so observing a change costs
/// no window call, no accessibility work and nothing cross-process at all.
///
/// It is deliberately the only thing allowed to decide that
/// `NATIVE_DISCORD_ROWS_MOVED_EVENT` may be emitted, because the failure mode
/// being designed against is not a missed notification, it is a per-tick one:
/// a 16 ms IPC drumbeat at the protected renderer is the shape that once froze
/// this app for 19,207 ms.
struct TranscriptBandTracker {
    /// The bounds the renderer was last told about. `None` is "nothing painted",
    /// which is a state worth reporting once (the eye going off) but never
    /// repeatedly.
    last_bounds: Option<OverlayRect>,
    /// Whether Discord's rectangle was at rest on the previous tick. Starts
    /// true, matching the backdated settle the guard opens a session with, so an
    /// already-still window does not manufacture an edge on the first pass.
    was_settled: bool,
}

impl TranscriptBandTracker {
    fn new(initial_bounds: Option<OverlayRect>) -> Self {
        Self {
            last_bounds: initial_bounds,
            was_settled: true,
        }
    }

    /// True exactly on the ticks where the band's geometry changed. Feeding it
    /// the same tick twice answers false the second time, by construction: both
    /// halves are transitions, and both are recorded before the answer is
    /// returned.
    ///
    /// The Discord half fires on the *rising* edge of "settled" rather than on
    /// the rectangle changing, so a drag -- which changes that rectangle on
    /// every one of its 16 ms ticks -- produces exactly one emit, when it stops.
    /// It is also gated on the eye, because with nothing painted there is no
    /// band to have moved.
    fn observe(
        &mut self,
        bounds: Option<OverlayRect>,
        discord_geometry_settled: bool,
        display_visible: bool,
    ) -> bool {
        let bounds_changed = bounds != self.last_bounds;
        let came_to_rest = discord_geometry_settled && !self.was_settled;
        self.last_bounds = bounds;
        self.was_settled = discord_geometry_settled;
        bounds_changed || (came_to_rest && display_visible)
    }
}

/// Retain both warm WebViews. Destroying them here is what made the lock
/// toggle a WebView create+navigate; the protected renderer is instead reset
/// by `OVERLAY_SESSION_EVENT` so no conversation state can survive into the
/// next session, and every identity, generation, geometry and context check in
/// `show`/`start_guard` still runs before the retained window is revealed.
fn hide_window(app: &tauri::AppHandle) {
    for label in [OVERLAY_LABEL, SHIELD_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.hide();
        }
    }
}

/// Destroy the retained pair outright. Reserved for the paths that must not
/// reuse the renderer at all, such as a changed protection context.
#[cfg(feature = "discord-qa-shell")]
fn close_window(app: &tauri::AppHandle) {
    dismiss_protected_pair();
    for label in [OVERLAY_LABEL, SHIELD_LABEL] {
        if let Some(window) = app.get_webview_window(label) {
            let _ = window.hide();
            let _ = window.close();
        }
    }
    wait_for_freed_protected_labels(app, &[OVERLAY_LABEL, SHIELD_LABEL]);
}

/// Tauri destroys a window and frees its label on the event loop, after
/// `close()` has already returned. A rebuild that starts inside that gap is
/// either rejected as a duplicate label or handed an already destroyed window,
/// so every closing path waits, bounded, for the labels it closed to be free.
/// Called only from the overlay workers, never from the event-loop thread.
fn wait_for_freed_protected_labels(app: &tauri::AppHandle, labels: &[&str]) {
    // The single choke point every destroying path goes through, and therefore
    // the only place a remembered handle can outlive the window it named.
    forget_protected_hwnds();
    for _ in 0..100 {
        if labels
            .iter()
            .all(|label| app.get_webview_window(label).is_none())
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Hide the retained pair and prove it actually left the screen, destroying a
/// window outright when the hide cannot be proven.
///
/// `hide_window` only asks Tauri to hide, and Tauri dispatches that to the main
/// thread. When the event loop is wedged, or when the session that owned the
/// pair is already gone, a best-effort hide can silently never take effect --
/// which is how a visible protected composer that no guard is managing was left
/// sitting over Discord until the process was killed. A retained WebView is
/// worth keeping, but never at the cost of a stranded visible window, so an
/// unproven hide escalates to destroying the window and freeing its label.
///
/// Called only from the overlay worker threads, never from the event-loop
/// thread: it waits, bounded, for a main-thread hide to land.
fn hide_protected_pair_or_destroy(app: &tauri::AppHandle) {
    // Latched first, so a guard pass already in flight cannot reveal the pair
    // between this hide and the proof below.
    dismiss_protected_pair();
    hide_window(app);
    let mut destroyed: Vec<&str> = Vec::new();
    for label in [OVERLAY_LABEL, SHIELD_LABEL] {
        let Some(window) = app.get_webview_window(label) else {
            continue;
        };
        let mut hidden = false;
        // Generous enough that a merely busy main thread keeps its retained
        // WebView, short enough that a wedged one cannot keep a protected
        // surface on screen.
        for _ in 0..60 {
            // A window that can no longer report its visibility is already gone.
            if window.is_visible().map(|visible| !visible).unwrap_or(true) {
                hidden = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !hidden {
            // Tauri's own dispatch queue can be starved by long main-thread work
            // while Windows still services window changes, so ask Windows
            // directly before giving up a retained window.
            #[cfg(target_os = "windows")]
            {
                hidden = force_hide_protected_window(&window);
            }
        }
        if !hidden {
            let _ = window.close();
            destroyed.push(label);
        }
    }
    if !destroyed.is_empty() {
        wait_for_freed_protected_labels(app, &destroyed);
    }
}

/// Take a protected window off screen without going through Tauri's dispatch
/// queue, and prove it from the window's own style bits. Hiding is never a
/// weaker state than showing, so this last resort cannot loosen any check.
#[cfg(target_os = "windows")]
fn force_hide_protected_window(window: &tauri::WebviewWindow) -> bool {
    let Ok(handle) = window.hwnd() else {
        return false;
    };
    let hwnd = handle.0 as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_ASYNCWINDOWPOS
                | SWP_HIDEWINDOW
                | SWP_NOACTIVATE
                | SWP_NOMOVE
                | SWP_NOSIZE
                | SWP_NOZORDER,
        );
    }
    for _ in 0..40 {
        if unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } & WS_VISIBLE as isize == 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

/// Whether the protected pair has been *dismissed*: taken off screen by
/// something that also intends it to stay off screen.
///
/// This is the file's one structural answer to the bug that keeps coming back
/// here -- two writers disagreeing about whether the composer is meant to be
/// visible. A dismiss runs on an IPC worker or on Tauri's own window-event
/// thread and can only *ask* for a hide; the guard thread runs a full pass every
/// 16 ms and its reveal is the thing that puts the pair on screen. Between the
/// two there is no ordering at all, so a dismiss that lands while a pass is in
/// flight used to be answered by that pass revealing the pair again, and the
/// composer stayed over Discord's real message box with the operator told it was
/// off. Proving the hide harder does not fix that: the reveal comes afterwards.
///
/// So presence is stated as a fact rather than raced for. Every dismissing path
/// latches this *before* it issues its hide, and every path that can put a
/// protected window on screen refuses while it is latched. Only
/// `OverlaySessionState::activate` -- the operator turning the composer back on
/// -- clears it.
///
/// Deliberately NOT set by `hide_window` on its own: the guard's minimize and
/// re-sample hides are transient, the guard owns their restore, and latching
/// them would make the composer unrecoverable without a new session.
static PROTECTED_PAIR_DISMISSED: AtomicBool = AtomicBool::new(false);

/// State that the pair must be off screen and must stay there. Called before the
/// hide, never after it, so no reveal can slip in between.
fn dismiss_protected_pair() {
    PROTECTED_PAIR_DISMISSED.store(true, Ordering::Release);
}

/// The operator asked for a composer again. The only writer that clears the
/// dismissal, and it runs before any window of the new session is revealed.
fn admit_protected_pair() {
    PROTECTED_PAIR_DISMISSED.store(false, Ordering::Release);
}

pub(crate) fn protected_pair_is_dismissed() -> bool {
    PROTECTED_PAIR_DISMISSED.load(Ordering::Acquire)
}

/// The refusal every reveal path answers with while the pair is dismissed.
///
/// It is an error rather than a silent skip on purpose: the guard fails closed
/// on it, which ends the session it was still guarding and runs the *proven*
/// hide in `ProtectedWindowGuardOwnership::drop`. A dismiss therefore always
/// converges on a window Windows itself reports as hidden, even when the
/// best-effort hide that started it never landed.
const PROTECTED_PAIR_DISMISSED_ERROR: &str = "The OSL protected composer has been closed";

fn refuse_dismissed_protected_pair() -> Result<(), String> {
    if protected_pair_is_dismissed() {
        return Err(PROTECTED_PAIR_DISMISSED_ERROR.to_owned());
    }
    Ok(())
}

/// Which guard generation currently owns the two retained protected windows.
///
/// A guard thread can stop for reasons that are not its own fail-closed exit: a
/// changed epoch, an early return, or a panic unwinding the thread. Whatever the
/// reason, whatever it may have revealed must not stay on screen. Epochs start
/// at 1, so 0 means the retained pair is unowned.
static PROTECTED_WINDOW_GUARD_EPOCH: AtomicU64 = AtomicU64::new(0);

fn claim_protected_window_guard(epoch: u64) {
    PROTECTED_WINDOW_GUARD_EPOCH.store(epoch, Ordering::Release);
}

/// True when `epoch` still owned the retained pair, so taking those windows off
/// screen is this guard's responsibility. False once a newer guard has claimed
/// them: hiding then would blank a live session whose own guard believes it
/// revealed the pair and would never show it again.
fn release_protected_window_guard(epoch: u64) -> bool {
    PROTECTED_WINDOW_GUARD_EPOCH
        .compare_exchange(epoch, 0, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// Cleanup guarantee for the guard thread. Held for the whole life of the guard,
/// so every way out of its loop -- changed epoch, fail-closed error, early
/// return, or a panic -- ends the session and takes the retained pair off
/// screen, unless a newer guard has taken ownership in the meantime.
struct ProtectedWindowGuardOwnership {
    app: tauri::AppHandle,
    epoch: u64,
}

impl ProtectedWindowGuardOwnership {
    fn claim(app: &tauri::AppHandle, epoch: u64) -> Self {
        claim_protected_window_guard(epoch);
        Self {
            app: app.clone(),
            epoch,
        }
    }
}

impl Drop for ProtectedWindowGuardOwnership {
    fn drop(&mut self) {
        if !release_protected_window_guard(self.epoch) {
            return;
        }
        // A guard that stops while its own session is still active ended
        // abnormally. End the session too, so no IPC path keeps operating on a
        // session nothing is watching and the main window learns protection
        // closed. The ordinary fail-closed exit has already cleared it.
        if self.app.state::<OverlaySessionState>().is_epoch(self.epoch) {
            clear_and_hide(&self.app);
        }
        hide_protected_pair_or_destroy(&self.app);
    }
}

#[cfg(feature = "discord-qa-shell")]
fn hide_window_for_qa_toggle(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(OVERLAY_LABEL)
        .ok_or_else(|| "The native Discord overlay window is unavailable".to_owned())?;
    let shield = app
        .get_webview_window(SHIELD_LABEL)
        .ok_or_else(|| "The native Discord overlay shield is unavailable".to_owned())?;
    window
        .hide()
        .map_err(|_| "The native Discord overlay could not be hidden".to_owned())?;
    shield
        .hide()
        .map_err(|_| "The native Discord overlay shield could not be hidden".to_owned())
}

pub(crate) fn clear_and_hide(app: &tauri::AppHandle) {
    // First, and before the session is even cleared. This function is called
    // from IPC workers and from Tauri's own window-event thread, so it may not
    // block waiting for its hide to land -- which is exactly why the hide alone
    // was never enough. Latching here makes the dismissal a fact the guard
    // cannot argue with: its next reveal refuses, it fails closed, and the
    // *proven* hide in `ProtectedWindowGuardOwnership::drop` finishes the job.
    dismiss_protected_pair();
    // Before `clear`, which resets the latches as state truth: the retraction has
    // to observe the raised value to know there is an edge to announce, or the
    // hub keeps a "your typing is going to Discord" warning up for a session that
    // no longer exists.
    retract_composer_unreachable(app);
    app.state::<OverlaySessionState>().clear();
    app.state::<NativeDiscordComposerState>().clear();
    app.state::<crate::native_surface_capture::NativeSurfaceCaptureState>()
        .clear();
    hide_window(app);
    // The retained renderer must forget every byte of this session before the
    // warm WebView can ever be revealed for a different protection context.
    let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_SESSION_EVENT, false);
    let _ = app.emit_to("main", OVERLAY_CLOSED_EVENT, ());
}

#[cfg(feature = "discord-qa-shell")]
pub(crate) fn suspend_and_hide_for_qa_toggle(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<OverlaySessionState>();
    state.suspend_for_qa_toggle()?;
    dismiss_protected_pair();
    // A parked session has no composer on screen either.
    retract_composer_unreachable(app);
    app.state::<NativeDiscordComposerState>().clear();
    app.state::<crate::native_surface_capture::NativeSurfaceCaptureState>()
        .clear();
    if let Err(error) = hide_window_for_qa_toggle(app) {
        clear_and_hide(app);
        return Err(error);
    }
    Ok(())
}

#[cfg(feature = "discord-qa-shell")]
pub(crate) fn discard_changed_qa_toggle(app: &tauri::AppHandle) {
    retract_composer_unreachable(app);
    app.state::<OverlaySessionState>().clear();
    app.state::<NativeDiscordComposerState>().clear();
    app.state::<crate::native_surface_capture::NativeSurfaceCaptureState>()
        .clear();
    close_window(app);
}

/// Tauri's own placement, and the last caller of it is the non-Windows stub.
///
/// On Windows nothing places a protected window this way any more: `set_size`
/// and `set_position` are queued to the event loop, so a placement issued from
/// one worker thread and a reveal issued from another have no ordering between
/// them, and the reveal winning is a composer that appears at its previous
/// rectangle before snapping onto Discord's. Windows placement is the single
/// deferred `DeferWindowPos` batch in `position_window_pair`, issued by the
/// guard on the guard's own thread immediately before it reveals.
#[cfg(not(target_os = "windows"))]
fn position_exact_window(window: &tauri::WebviewWindow, rect: OverlayRect) -> Result<(), String> {
    window
        .set_size(PhysicalSize::new(rect.width, rect.height))
        .map_err(|_| "The native Discord overlay size could not be verified".to_owned())?;
    window
        .set_position(PhysicalPosition::new(rect.x, rect.y))
        .map_err(|_| "The native Discord overlay position could not be verified".to_owned())
}

/// The protected pair's native handles, remembered so the guard never has to
/// ask Tauri for them.
///
/// Every Tauri *getter* -- `hwnd()`, `scale_factor()`, `is_minimized()` -- is a
/// blocking round trip through the event-loop FIFO (`settle_protected_window`
/// exists precisely because that is true). The operator dragging OSL is the one
/// moment that thread is unavailable: a caption drag runs inside Windows' modal
/// move loop, so every one of those round trips waits out the gesture's own
/// message pump. A single full guard pass made seven of them -- two for
/// `is_minimized`/`scale_factor`, two inside `osl_process_is_foreground`, two
/// inside the placement, two more inside `verify_owned_overlay_pair` -- and the
/// pass could therefore not answer a move for over half a second. That is the
/// measured 630 ms composer follow-lag, and it is the whole of it: the answers
/// themselves were never expensive.
///
/// None of those questions need Tauri. A window handle does not change for the
/// life of the window, and owner, style, DPI, iconified state and position are
/// all plain Win32 reads off the handle, answered on the calling thread in
/// nanoseconds. So the handle is resolved through Tauri exactly once and every
/// steady-state read after that is local.
///
/// The cache is re-proved on every read rather than trusted: a handle that is no
/// longer a window, or no longer one of this process's windows, is discarded and
/// re-resolved. Handle reuse cannot smuggle a stranger's window past that check
/// and into a `SetWindowPos`, and the guard additionally re-proves owner and
/// extended style on these exact handles once per full pass
/// (`verify_owned_overlay_pair`), which is a stronger coupling than the previous
/// code had: it used to verify one freshly fetched handle and move another.
#[cfg(target_os = "windows")]
static PROTECTED_OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);
#[cfg(target_os = "windows")]
static PROTECTED_SHIELD_HWND: AtomicIsize = AtomicIsize::new(0);
#[cfg(target_os = "windows")]
static TRUSTED_OWNER_HWND: AtomicIsize = AtomicIsize::new(0);

/// Whether `hwnd` is still a live window belonging to this process.
///
/// Both halves matter. `IsWindow` alone would accept a recycled handle, and a
/// recycled handle is the one way a cached value could name someone else's
/// window; a window this process does not own can never be one of the two
/// protected windows.
#[cfg(target_os = "windows")]
fn window_is_live_in_this_process(hwnd: isize) -> bool {
    let handle = hwnd as HWND;
    if hwnd == 0 || handle.is_null() || unsafe { IsWindow(handle) } == 0 {
        return false;
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(handle, &mut process_id) };
    process_id == std::process::id()
}

#[cfg(target_os = "windows")]
fn protected_hwnd_cache(label: &str) -> Option<&'static AtomicIsize> {
    match label {
        OVERLAY_LABEL => Some(&PROTECTED_OVERLAY_HWND),
        SHIELD_LABEL => Some(&PROTECTED_SHIELD_HWND),
        "main" => Some(&TRUSTED_OWNER_HWND),
        _ => None,
    }
}

/// This window's handle, from the cache when it is still provably valid and from
/// Tauri exactly once otherwise.
#[cfg(target_os = "windows")]
fn cached_window_hwnd(window: &tauri::WebviewWindow) -> Option<isize> {
    // `label()` is a local string on the caller's own thread, not a getter.
    let Some(cache) = protected_hwnd_cache(window.label()) else {
        return window.hwnd().ok().map(|handle| handle.0 as isize);
    };
    let cached = cache.load(Ordering::Acquire);
    if window_is_live_in_this_process(cached) {
        return Some(cached);
    }
    let hwnd = window.hwnd().ok()?.0 as isize;
    if !window_is_live_in_this_process(hwnd) {
        return None;
    }
    cache.store(hwnd, Ordering::Release);
    Some(hwnd)
}

/// Same answer, addressed by label, for the readers that hold an `AppHandle`
/// rather than a window. `get_webview_window` is a map lookup under a local
/// mutex, not a round trip, so this stays local whenever the cache is warm.
#[cfg(target_os = "windows")]
fn cached_label_hwnd(app: &tauri::AppHandle, label: &str) -> Option<isize> {
    if let Some(cache) = protected_hwnd_cache(label) {
        let cached = cache.load(Ordering::Acquire);
        if window_is_live_in_this_process(cached) {
            return Some(cached);
        }
    }
    cached_window_hwnd(&app.get_webview_window(label)?)
}

/// Forget every remembered handle. Called from the one place a protected label
/// is actually destroyed, so nothing can outlive the window it named.
#[cfg(target_os = "windows")]
fn forget_protected_hwnds() {
    PROTECTED_OVERLAY_HWND.store(0, Ordering::Release);
    PROTECTED_SHIELD_HWND.store(0, Ordering::Release);
    // The shield's clip is remembered against the handle it was installed on, so
    // it has to be forgotten in the same breath: a rebuilt shield that inherited
    // this cache would be believed to already hold a region it has never been
    // given, and would paint -- and shield -- its whole rectangle.
    *SHIELD_REGION_OFFSETS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

#[cfg(not(target_os = "windows"))]
fn forget_protected_hwnds() {}

/// Clip the opaque shield to exactly the rows OSL is painting.
///
/// The shield used to be one slab the size of the whole protected surface, so
/// turning the lock on blacked out every ordinary message, every undecodable
/// row and all of the history behind it. A window region is the narrowest thing
/// Windows offers: outside it the window does not exist -- it does not paint,
/// it does not hit-test, and a capture of that area is simply Discord.
/// The region the shield needs, expressed the way the shield holds it: in the
/// shield window's own coordinates.
///
/// This is the form that makes the region survive a move for free -- outside the
/// window the region does not exist, and neither the window's contents nor its
/// clip move relative to the window because the window moved. So a drag frame,
/// which translates the bounds and every row by the same delta, produces exactly
/// the same offsets, and a `SetWindowRgn` for it would be a cross-thread call and
/// a full repaint of the shield to install a region Windows already has.
#[cfg(any(target_os = "windows", test))]
fn shield_region_offsets(bounds: OverlayRect, painted: &[[i32; 4]]) -> Vec<[i32; 4]> {
    painted
        .iter()
        .map(|rect| {
            [
                rect[0] - bounds.x,
                rect[1] - bounds.y,
                rect[2] - bounds.x,
                rect[3] - bounds.y,
            ]
        })
        .collect()
}

/// The window-relative region the shield was last given, and the handle it was
/// given to. Cleared with the handles themselves in `forget_protected_hwnds`, so a
/// rebuilt shield can never inherit the previous one's clip.
#[cfg(target_os = "windows")]
static SHIELD_REGION_OFFSETS: Mutex<Option<(isize, Vec<[i32; 4]>)>> = Mutex::new(None);

/// Whether Windows still holds exactly the region this process last installed on
/// `hwnd`.
///
/// Two answers, and both are needed. The cached offsets are the only thing that
/// can tell one row set from another with the same bounding box -- a row appearing
/// between two others moves no edge of the union. And `GetWindowRgnBox` is the
/// read that keeps the cache from being an assumption: it is a local GDI query
/// that reports the extent of the region the window actually has, so a region that
/// was never installed, was dropped, or belongs to a different shape is re-applied
/// rather than trusted.
#[cfg(target_os = "windows")]
fn shield_region_is_already_installed(hwnd: HWND, offsets: &[[i32; 4]]) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{GetWindowRgnBox, RGN_ERROR};

    let cached = SHIELD_REGION_OFFSETS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !cached
        .as_ref()
        .is_some_and(|(window, cached)| *window == hwnd as isize && cached.as_slice() == offsets)
    {
        return false;
    }
    let mut box_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRgnBox(hwnd, &mut box_rect) } == RGN_ERROR {
        return false;
    }
    let Some(expected) = painted_rows_bounds(offsets) else {
        return false;
    };
    box_rect.left == expected.x
        && box_rect.top == expected.y
        && box_rect.right - box_rect.left == expected.width as i32
        && box_rect.bottom - box_rect.top == expected.height as i32
}

#[cfg(target_os = "windows")]
fn clip_capture_shield_to_painted_rows(
    shield: &tauri::WebviewWindow,
    bounds: OverlayRect,
    painted: &[[i32; 4]],
) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, RGN_OR,
    };

    let hwnd = cached_window_hwnd(shield)
        .ok_or_else(|| "The OSL capture shield could not be limited safely".to_owned())?
        as HWND;
    if hwnd.is_null() || painted.is_empty() {
        return Err("The OSL capture shield could not be limited safely".to_owned());
    }
    // Read before written, exactly as the shield's own visibility is. `SetWindowRgn`
    // is a cross-thread call to the UI thread that owns this window and it redraws
    // the whole shield, and the placement above calls it on every frame of a drag --
    // to install, over and over, a region a translation cannot have changed.
    let offsets = shield_region_offsets(bounds, painted);
    if shield_region_is_already_installed(hwnd, &offsets) {
        return Ok(());
    }
    let region = unsafe { CreateRectRgn(0, 0, 0, 0) };
    if region.is_null() {
        return Err("The OSL capture shield could not be limited safely".to_owned());
    }
    for rect in &offsets {
        // Window coordinates, so the region follows the shield wherever the
        // conversation moves without ever being recomputed from screen space.
        let piece = unsafe { CreateRectRgn(rect[0], rect[1], rect[2], rect[3]) };
        if piece.is_null() {
            unsafe { DeleteObject(region.cast()) };
            return Err("The OSL capture shield could not be limited safely".to_owned());
        }
        let combined = unsafe { CombineRgn(region, region, piece, RGN_OR as i32) };
        unsafe { DeleteObject(piece.cast()) };
        if combined == 0 {
            unsafe { DeleteObject(region.cast()) };
            return Err("The OSL capture shield could not be limited safely".to_owned());
        }
    }
    // Windows owns the region after this call succeeds, so it must not be freed
    // here; a failure keeps ownership and must free it.
    if unsafe { SetWindowRgn(hwnd, region, 1) } == 0 {
        unsafe { DeleteObject(region.cast()) };
        return Err("The OSL capture shield could not be limited safely".to_owned());
    }
    // Recorded only after Windows has taken it, and re-proved by
    // `GetWindowRgnBox` before it is ever believed again.
    *SHIELD_REGION_OFFSETS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((hwnd as isize, offsets));
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn clip_capture_shield_to_painted_rows(
    _shield: &tauri::WebviewWindow,
    _bounds: OverlayRect,
    _painted: &[[i32; 4]],
) -> Result<(), String> {
    Err("The OSL capture shield requires Windows".to_owned())
}

#[cfg(target_os = "windows")]
fn position_window_pair(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    rect: OverlayRect,
    painted: &[[i32; 4]],
) -> Result<(), String> {
    let overlay_hwnd = cached_window_hwnd(overlay)
        .ok_or_else(|| "The native Discord overlay position could not be verified".to_owned())?
        as windows_sys::Win32::Foundation::HWND;
    let shield_hwnd = cached_window_hwnd(shield)
        .ok_or_else(|| "The OSL capture shield position could not be verified".to_owned())?
        as windows_sys::Win32::Foundation::HWND;
    if overlay_hwnd.is_null() || shield_hwnd.is_null() || overlay_hwnd == shield_hwnd {
        return Err("The OSL protected window pair is unavailable".to_owned());
    }
    // Nothing painted means nothing to shield: the shield leaves the screen and
    // is not given a rectangle over Discord at all. That is a *visibility*
    // question, and it is the only one here -- the composer's own move is issued
    // through the same deferred batch either way.
    //
    // It used to be a different placement path entirely: `position_exact_window`,
    // i.e. Tauri's `set_size` + `set_position`, two more messages queued behind
    // the two blocking `hwnd()` getters this function had already paid for. So
    // the common case -- production paints no rows at all, and neither does a QA
    // session that has sent nothing -- never reached the cheap deferred write
    // this function exists to make, and the composer was moved by the same
    // event-loop thread the operator's drag was holding.
    let shield_rect = painted_rows_bounds(painted);
    let deferred = unsafe { BeginDeferWindowPos(if shield_rect.is_some() { 2 } else { 1 }) };
    if deferred.is_null() {
        return Err("The OSL protected window pair could not be moved safely".to_owned());
    }
    let deferred = match shield_rect {
        Some(shield_rect) => {
            let deferred = unsafe {
                DeferWindowPos(
                    deferred,
                    shield_hwnd,
                    std::ptr::null_mut(),
                    shield_rect.x,
                    shield_rect.y,
                    shield_rect.width as i32,
                    shield_rect.height as i32,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
            };
            if deferred.is_null() {
                return Err("The OSL capture shield could not follow Discord safely".to_owned());
            }
            deferred
        }
        None => deferred,
    };
    let deferred = unsafe {
        DeferWindowPos(
            deferred,
            overlay_hwnd,
            std::ptr::null_mut(),
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
    if deferred.is_null() || unsafe { EndDeferWindowPos(deferred) } == 0 {
        return Err("The OSL protected window pair could not follow Discord safely".to_owned());
    }
    let Some(shield_rect) = shield_rect else {
        // Read before written: a shield that is already off screen needs no
        // message at all, so a steady session -- and every frame of a drag --
        // queues nothing on the event loop. Only the composer's rule forbids
        // hiding; the shield owns pixels only where OSL paints, and here it
        // paints nowhere.
        if unsafe { IsWindowVisible(shield_hwnd) } != 0 {
            let _ = shield.hide();
        }
        return Ok(());
    };
    clip_capture_shield_to_painted_rows(shield, shield_rect, painted)
}

#[cfg(not(target_os = "windows"))]
fn position_window_pair(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    rect: OverlayRect,
    painted: &[[i32; 4]],
) -> Result<(), String> {
    let _ = (shield, painted);
    position_exact_window(overlay, rect)
}

fn exact_shield_stack(
    overlay: isize,
    shield: isize,
    immediately_below_overlay: isize,
    immediately_above_shield: isize,
) -> bool {
    overlay != 0
        && shield != 0
        && overlay != shield
        && immediately_below_overlay == shield
        && immediately_above_shield == overlay
}

#[cfg(target_os = "windows")]
fn ensure_shield_stack(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    shielded: bool,
) -> Result<(), String> {
    // Eye off, or nothing OSL can locate to paint over: there is no protected
    // pixel on screen, so the opaque shield must not be on screen either.
    if !shielded {
        return shield
            .hide()
            .map_err(|_| "The OSL capture shield could not be hidden safely".to_owned());
    }
    let overlay_hwnd = overlay
        .hwnd()
        .map_err(|_| "The OSL capture shield stack is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    let shield_hwnd = shield
        .hwnd()
        .map_err(|_| "The OSL capture shield stack is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    if overlay_hwnd.is_null() || shield_hwnd.is_null() || overlay_hwnd == shield_hwnd {
        return Err("The OSL capture shield stack is unavailable".to_owned());
    }
    // A hide asked of Tauri earlier can still be queued. Landing after the Win32
    // reveal below, it would hide this window again and reapply tao's captioned
    // style with nothing left to strip it, so the queue is drained first.
    settle_protected_window(shield, "The OSL capture shield stack is unavailable")?;
    // Put the opaque shield immediately behind the capture-excluded overlay.
    // SWP_NOACTIVATE ensures the shield can never take typing focus.
    if unsafe {
        SetWindowPos(
            shield_hwnd,
            overlay_hwnd,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    } == 0
    {
        return Err("The OSL capture shield could not be stacked safely".to_owned());
    }
    // SWP_SHOWWINDOW above is a reveal that does not go through Tauri, and the
    // hide that preceded it left this window's cached tao style -- which always
    // carries WS_CAPTION|WS_SYSMENU -- reapplied to the HWND. Stripping the
    // frame after it is therefore part of showing, exactly as at every other
    // reveal.
    qa_label_overlay_frame_stage("shield_stack_after_win32_show");
    enforce_native_frameless_overlay(shield)?;
    let below_overlay = unsafe { GetWindow(overlay_hwnd, GW_HWNDNEXT) };
    let above_shield = unsafe { GetWindow(shield_hwnd, GW_HWNDPREV) };
    if !exact_shield_stack(
        overlay_hwnd as isize,
        shield_hwnd as isize,
        below_overlay as isize,
        above_shield as isize,
    ) {
        return Err("The OSL capture shield stacking could not be verified".to_owned());
    }
    Ok(())
}

/// Which z-order band a window currently sits in. Correcting the protected
/// stack may reorder windows inside a band but must never move one between
/// bands: production forbids a topmost protected window outright.
#[cfg(target_os = "windows")]
fn window_is_topmost(window: isize) -> bool {
    let hwnd = window as windows_sys::Win32::Foundation::HWND;
    !hwnd.is_null() && unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST != 0
}

/// Bound on the z-order walk below. A desktop with more windows than this above
/// Discord is answered "undecided", which never mutates the stack.
const PROTECTED_STACK_WALK_LIMIT: usize = 128;

/// How often the guard may spend that bounded read proving the composer is still
/// above Discord. Slow enough that a correct stack is never touched on a timer,
/// fast enough that clicking into Discord cannot leave the composer behind it.
const PROTECTED_STACK_PROBE_INTERVAL: Duration = Duration::from_millis(200);

/// Whether `target` sits above `from` in a z-order chain, walking upwards.
///
/// `Some(false)` is only ever returned after reaching the top of the chain
/// without meeting `target`, so a "below" answer is proof, not a guess.
/// Exhausting the bound answers `None`, which the caller treats as no drift:
/// raising an already-correct window on a timer is what interrupted WebView2
/// keyboard delivery, so an undecidable read must never mutate anything.
fn resolve_window_is_above(
    target: isize,
    from: isize,
    limit: usize,
    mut previous: impl FnMut(isize) -> isize,
) -> Option<bool> {
    if target == 0 || from == 0 || target == from {
        return None;
    }
    let mut cursor = from;
    for _ in 0..limit {
        cursor = previous(cursor);
        if cursor == 0 {
            return Some(false);
        }
        if cursor == target {
            return Some(true);
        }
    }
    None
}

/// Cheap, read-only answer to "is the protected composer still stacked above the
/// borrowed Discord window?".
///
/// Discord is borrowed as a top-level window owned by the same trusted OSL
/// parent, so it is an ordinary z-order sibling of the composer: activating it
/// raises it above the composer, which then sits behind Discord while Windows
/// still reports it visible. `None` means the question could not be answered, or
/// that the answer cannot be acted on without joining a band this build may not
/// hold; both are treated as no drift.
#[cfg(target_os = "windows")]
fn protected_composer_is_above_discord(
    app: &tauri::AppHandle,
    discord_window: isize,
) -> Option<bool> {
    let overlay = cached_label_hwnd(app, OVERLAY_LABEL)?;
    let discord_root = unsafe {
        GetAncestor(
            discord_window as windows_sys::Win32::Foundation::HWND,
            GA_ROOT,
        )
    } as isize;
    if !window_is_topmost(overlay) && window_is_topmost(discord_root) {
        return None;
    }
    resolve_window_is_above(
        overlay,
        discord_root,
        PROTECTED_STACK_WALK_LIMIT,
        |cursor| unsafe {
            GetWindow(cursor as windows_sys::Win32::Foundation::HWND, GW_HWNDPREV) as isize
        },
    )
}

/// The one word a live QA run must be able to state about the composer's order
/// without anyone having to photograph the screen and reason about pixels.
///
/// Deliberately three-valued and never optimistic: `unknown` is what an
/// exhausted walk or a cross-band comparison answers, and it must never be read
/// as "above". A run that reports `below-discord` or `unknown` has not proved
/// the composer is where the operator types.
#[cfg(any(feature = "discord-qa-shell", test))]
fn composer_zorder_label(above: Option<bool>) -> &'static str {
    match above {
        Some(true) => "above-discord",
        Some(false) => "below-discord",
        None => "unknown",
    }
}

/// The breadcrumb a live QA run reads instead of inferring z-order from a
/// screenshot.
///
/// Visibility is recorded next to the order for a reason: the failure this
/// breadcrumb exists to name was a composer that was *above* Discord in the
/// z-order the whole time and simply not on screen, which every order-only
/// reading of the stack reports as healthy. `visible=false` and
/// `composer_zorder=above-discord` together are the exact signature of that
/// bug, and neither half says it alone.
///
/// Read-only and truncating: one line, always the current answer, no growth.
#[cfg(all(feature = "discord-qa-shell", target_os = "windows"))]
fn qa_record_composer_zorder(app: &tauri::AppHandle, discord_window: isize, stage: &str) {
    use std::io::Write as _;

    let order = composer_zorder_label(protected_composer_is_above_discord(app, discord_window));
    let (visible, topmost) = cached_label_hwnd(app, OVERLAY_LABEL)
        .map(|hwnd| {
            let style = unsafe {
                GetWindowLongPtrW(hwnd as windows_sys::Win32::Foundation::HWND, GWL_STYLE)
            };
            (style as u32 & WS_VISIBLE != 0, window_is_topmost(hwnd))
        })
        .unwrap_or((false, false));
    // Window state only: no composer text, identity or token can reach here.
    let path = std::env::temp_dir().join("osl-discord-qa-composer-zorder.txt");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "composer_zorder={order} visible={visible} topmost={topmost} stage={stage}"
        );
    }
}

#[cfg(all(feature = "discord-qa-shell", not(target_os = "windows")))]
fn qa_record_composer_zorder(_app: &tauri::AppHandle, _discord_window: isize, _stage: &str) {}

#[cfg(target_os = "windows")]
fn active_protected_stack_drifted(app: &tauri::AppHandle, discord_window: isize) -> bool {
    protected_composer_is_above_discord(app, discord_window) == Some(false)
}

#[cfg(not(target_os = "windows"))]
fn active_protected_stack_drifted(_app: &tauri::AppHandle, _discord_window: isize) -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
fn ensure_shield_stack(
    _overlay: &tauri::WebviewWindow,
    _shield: &tauri::WebviewWindow,
    _shielded: bool,
) -> Result<(), String> {
    Err("The OSL capture shield requires Windows".to_owned())
}

#[cfg(feature = "discord-qa-shell")]
#[cfg(target_os = "windows")]
fn active_ensure_carrier_stack(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    discord_window: isize,
    shielded: bool,
) -> Result<(), String> {
    // Discord is borrowed as a top-level window owned by the trusted OSL parent
    // while the Tauri composer is an owned popup, so the two are ordinary
    // z-order siblings and Discord's hosted Chromium child can still paint above
    // a normal-band popup. QA explicitly raises only this exact composer HWND.
    //
    // `SWP_SHOWWINDOW` below is a reveal that does not go through
    // `reveal_protected_pair`, so it owes the same dismissal refusal.
    refuse_dismissed_protected_pair()?;
    show_capture_shield(shield, shielded)?;
    let overlay_hwnd = overlay
        .hwnd()
        .map_err(|_| "The OSL composer stack is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    // Drain any still-queued Tauri visibility change for this window before the
    // Win32 reveal, so tao cannot rewrite the style after the strip below.
    settle_protected_window(overlay, "The OSL composer stack is unavailable")?;
    if overlay_hwnd.is_null()
        || unsafe {
            SetWindowPos(
                overlay_hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        } == 0
    {
        return Err("The OSL composer could not be raised above its owner".to_owned());
    }
    // SWP_SHOWWINDOW makes this window visible without going through Tauri, and
    // the preceding hide reapplied tao's cached style, which always carries
    // WS_CAPTION|WS_SYSMENU. Strip it after showing, never before.
    qa_label_overlay_frame_stage("qa_composer_raise_after_win32_show");
    enforce_native_frameless_overlay(overlay)?;
    // A reveal is a frame change, so the alpha has to be put back here for the
    // same reason `reveal_protected_pair` puts it back after its own Tauri
    // reveal: this is the composer's only other reveal, it does not go through
    // that function, and it is the *last* writer to touch this HWND on opening,
    // on a rebuilt geometry (a resize or a DPI change), on a restored composer
    // and on corrected stack drift. Without it the composer stays opaque from the
    // carrier-stack pass onward and every alpha-0 pixel it owns composites
    // against WebView2's white backing -- which, on a window sized to exactly the
    // native composer rectangle, is just the four rounded corners of
    // `.composer-box`: the reported white slivers.
    //
    // Unconditional, like the reveal and unlike a frame-only correction: it is
    // showing the window that drops tao's creation-time blur-behind region, so a
    // strip that had nothing to correct is not evidence the alpha survived. Still
    // not a cadence -- the guard only reaches this call at those four
    // transitions, never on a steady pass.
    enforce_transparent_protected_composer(overlay)?;
    // The topmost raise above wins over Discord only while Discord is in the
    // ordinary band, which is an assumption nothing in this build ever checked.
    // Stating the relationship explicitly costs nothing in the normal case --
    // the band guard inside makes it a no-op the moment the composer is topmost
    // and Discord is not -- and turns "the composer happens to be topmost" into
    // "the composer is deliberately above this exact Discord window".
    raise_protected_composer_above_discord(overlay, discord_window)?;
    Ok(())
}

/// Whether a same-band reorder can decide the composer's order at all.
///
/// A `SetWindowPos` insertion moves the composer inside one z-order list, which
/// is only a correction while both windows are in the same band. Across bands it
/// is not a reorder but a band change, in whichever direction it is issued:
///
/// * composer topmost, Discord not -- the composer is already above Discord by
///   band, and inserting it after a non-topmost window would *demote* it out of
///   the topmost band. That is the QA build's steady state, so the write there
///   would take away the very thing that keeps the composer on top.
/// * Discord topmost, composer not -- production forbids `WS_EX_TOPMOST` on a
///   protected window outright, so promoting the composer would make the very
///   next ownership check fail closed. The drift is left alone instead.
///
/// Only the two same-band cases are reorderable, and both are answered `true`.
#[cfg(any(target_os = "windows", test))]
fn composer_raise_is_a_same_band_reorder(overlay_topmost: bool, discord_topmost: bool) -> bool {
    overlay_topmost == discord_topmost
}

/// Which handle `SetWindowPos` must be given as `hWndInsertAfter` so that the
/// composer ends up immediately **above** the borrowed Discord window.
///
/// `hWndInsertAfter` names the window that will *precede* the positioned window
/// in the z-order -- that is, the window that ends up immediately **above** it.
/// Passing Discord's own handle is therefore the instruction "put the composer
/// immediately BELOW Discord", which is the exact opposite of what this function
/// is named for, of every comment that surrounded it, and of the invariant the
/// guard's own probe measures. That is what it did: the composer was pushed
/// under Discord on every correction, the 200 ms probe then proved it was below
/// and corrected it into the same place again, forever -- and re-asserting the
/// stack on a cadence is what this file elsewhere identifies as the thing that
/// interrupts WebView2 keyboard delivery.
///
/// This file already proves the semantics it was misusing: `ensure_shield_stack`
/// passes the overlay as `hWndInsertAfter` *deliberately*, to put the shield
/// "immediately behind the capture-excluded overlay", and then verifies exactly
/// that with `GW_HWNDNEXT`/`GW_HWNDPREV`.
///
/// So the correct handle is whatever currently sits immediately above Discord,
/// which is what the composer must displace. `0` is `HWND_TOP`: Discord is
/// already at the top of its band, so the composer goes above the whole band.
/// `None` means no write is needed -- either the question is degenerate, or the
/// composer is already the window immediately above Discord, and re-issuing a
/// correct stack is the cadence that eats keystrokes.
#[cfg(any(target_os = "windows", test))]
fn composer_insert_after_above_discord(
    overlay: isize,
    discord_root: isize,
    window_above_discord: isize,
) -> Option<isize> {
    if overlay == 0 || discord_root == 0 || overlay == discord_root {
        return None;
    }
    if window_above_discord == overlay {
        return None;
    }
    Some(window_above_discord)
}

/// The `osl://native-discord-composer-unreachable` payload.
///
/// `reason` names the condition whose edge produced this message and is always
/// one of the three fixed `COMPOSER_UNREACHABLE_*` strings above -- never
/// anything derived from a draft, a conversation, a token or an identity.
/// `unreachable` is the *level* across every condition, so a reader sets a
/// boolean from it rather than reconstructing one from a sequence of edges.
#[derive(Clone, Copy, serde::Serialize)]
struct ComposerUnreachable {
    reason: &'static str,
    unreachable: bool,
}

/// Announce a change in whether the protected composer can receive keystrokes.
///
/// `was_unreachable` is the aggregate read *before* the calling latch was
/// written, so this emits on the aggregate's edges rather than on either latch's
/// own: a second condition raising on top of the first changes nothing the
/// operator can see, and one condition retracting while the other still holds
/// must not clear the warning. That reconciliation used to be the renderer's
/// problem and it could only guess, because the wire said nothing about which
/// condition had spoken.
///
/// Emitting the level rather than a delta is also what makes the two writers
/// safe. The z-order latch is written from the guard thread and the retraction
/// below from an IPC worker, so two edges can interleave; a duplicated or
/// dropped level is idempotent, where a duplicated delta is a stuck warning.
fn publish_composer_unreachable(
    app: &tauri::AppHandle,
    reason: &'static str,
    was_unreachable: bool,
) {
    let unreachable = app.state::<OverlaySessionState>().composer_is_unreachable();
    if unreachable == was_unreachable {
        return;
    }
    let _ = app.emit_to(
        "main",
        OVERLAY_COMPOSER_UNREACHABLE_EVENT,
        ComposerUnreachable {
            reason,
            unreachable,
        },
    );
}

/// An ended session has no composer, so it cannot have an unreachable one.
///
/// Both latches outlive every session, and both setters are edge-only, so a
/// session that ended while one was raised left the hub warning on screen with
/// nothing able to retract it and left the *next* session unable to raise it:
/// the next `swap` would find the value already there and report no edge.
fn retract_composer_unreachable(app: &tauri::AppHandle) {
    let state = app.state::<OverlaySessionState>();
    let was_unreachable = state.composer_is_unreachable();
    state.clear_composer_unreachable_latches();
    publish_composer_unreachable(app, COMPOSER_UNREACHABLE_SESSION_ENDED, was_unreachable);
}

/// Make a cross-band composer surrender observable instead of silent.
///
/// Latched on the session so any later reader (an IPC caller, a QA run, the
/// teardown path) can ask whether the composer's order is currently
/// undecidable, and announced to the hub window on the edges only so a state
/// that persists across the 200 ms probe cadence cannot become a message storm.
///
/// Deliberately not fatal and deliberately not a hide: a cross-band pair is a
/// presentation state, and the two reactions that would "fix" the hit test --
/// ending the session, or taking the composer off screen -- are exactly the two
/// this file must never take.
///
/// This is a *z-order* band and nothing to do with the composer's band of
/// screen. Vacating that band with the lock down is intended and correct; being
/// stuck behind Discord while the lock is up is the hazard this reports.
#[cfg(target_os = "windows")]
fn report_composer_zorder_surrender(app: &tauri::AppHandle, surrendered: bool) {
    let state = app.state::<OverlaySessionState>();
    // Only a hazard while OSL owns the composer band. With the lock down the
    // operator's keystrokes are *meant* to reach Discord's own message box, OSL has
    // vacated that band deliberately, and the window Discord is drawing above holds
    // painted transcript rows rather than a composer -- a display problem, not a
    // leak. Raising "your typing is going to Discord, not OSL" there would be the
    // one warning in this file that must never cry wolf, and it would fire in
    // exactly the state the operator asked for.
    //
    // It is also the band latch's third retraction path: lowering the lock retires
    // a raised surrender on the next probe rather than leaving it to a restart.
    let surrendered = surrendered && state.lock_engaged();
    let was_unreachable = state.composer_is_unreachable();
    if !state.set_composer_zorder_surrendered(surrendered) {
        return;
    }
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage(if surrendered {
        "composer_zorder_surrendered"
    } else {
        "composer_zorder_reorderable"
    });
    publish_composer_unreachable(app, COMPOSER_UNREACHABLE_ZORDER_BAND, was_unreachable);
}

/// Place the protected composer directly above the borrowed Discord window
/// without changing its z-order band.
///
/// Compiled into **both** builds. It used to be `not(feature =
/// "discord-qa-shell")`, which left the QA build with no writer that related the
/// composer to Discord at all: its carrier stack raised the composer to
/// `HWND_TOPMOST` and simply assumed that won, so a Discord window that ever
/// reached the topmost band had no correction behind it. The band guard above
/// makes the same call safe in both builds, and a no-op in exactly the states
/// where a write would move a window between bands.
#[cfg(target_os = "windows")]
fn raise_protected_composer_above_discord(
    overlay: &tauri::WebviewWindow,
    discord_window: isize,
) -> Result<(), String> {
    let overlay_hwnd = overlay
        .hwnd()
        .map_err(|_| "The OSL composer stack is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    if overlay_hwnd.is_null() {
        return Err("The OSL composer stack is unavailable".to_owned());
    }
    let discord_root = unsafe {
        GetAncestor(
            discord_window as windows_sys::Win32::Foundation::HWND,
            GA_ROOT,
        )
    };
    if discord_root.is_null() || discord_root == overlay_hwnd {
        return Ok(());
    }
    // A cross-band pair is the one state this writer cannot correct, and it is
    // also the state in which the composer can be fully alive -- owned,
    // positioned, stacked, painting -- and still behind Discord, so every click
    // and keystroke aimed at it lands in Discord's own message box instead.
    //
    // Returning `Ok(())` here is still right: it is not an identity failure, it
    // must not end the session, and it must certainly not answer a lost hit
    // test by taking the composer off screen. What was wrong was that it was
    // *silent*. The surrender is now latched on the session and announced to the
    // hub on its edges only, so the operator is told the composer is behind
    // Discord instead of being left to discover it by typing into the clear.
    let surrendered = !composer_raise_is_a_same_band_reorder(
        window_is_topmost(overlay_hwnd as isize),
        window_is_topmost(discord_root as isize),
    );
    report_composer_zorder_surrender(overlay.app_handle(), surrendered);
    if surrendered {
        return Ok(());
    }
    // The window the composer has to displace, not the window it has to clear.
    let window_above_discord = unsafe { GetWindow(discord_root, GW_HWNDPREV) } as isize;
    let Some(insert_after) = composer_insert_after_above_discord(
        overlay_hwnd as isize,
        discord_root as isize,
        window_above_discord,
    ) else {
        return Ok(());
    };
    // A reorder inside one band, never a reveal: nothing here asks Windows to
    // show a window, so it can neither restore a non-client frame nor put a
    // hidden protected surface on screen.
    if unsafe {
        SetWindowPos(
            overlay_hwnd,
            insert_after as windows_sys::Win32::Foundation::HWND,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    } == 0
    {
        return Err(
            "The OSL composer could not be raised above the borrowed Discord window".to_owned(),
        );
    }
    // Read back what landed, exactly as `ensure_shield_stack` does, because a
    // successful `SetWindowPos` proves only that Windows accepted the call --
    // never that the argument meant what the caller thought. An inverted
    // `hWndInsertAfter` returns success on every single call, which is precisely
    // how this survived review, and no source-text assertion can catch it.
    //
    // Adjacency is the wrong question here, unlike for the shield: the shield is
    // restacked immediately behind the composer straight after this, so it sits
    // between the composer and Discord by design. The bounded upward walk the
    // guard's own probe uses is the right one, and only a *proved* inversion
    // fails -- an undecidable read is never treated as one.
    if resolve_window_is_above(
        overlay_hwnd as isize,
        discord_root as isize,
        PROTECTED_STACK_WALK_LIMIT,
        |cursor| unsafe {
            GetWindow(cursor as windows_sys::Win32::Foundation::HWND, GW_HWNDPREV) as isize
        },
    ) == Some(false)
    {
        return Err(
            "The OSL composer could not be verified above the borrowed Discord window".to_owned(),
        );
    }
    Ok(())
}

#[cfg(not(feature = "discord-qa-shell"))]
#[cfg(target_os = "windows")]
fn active_ensure_carrier_stack(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    discord_window: isize,
    shielded: bool,
) -> Result<(), String> {
    // Preserve the reviewed production contract exactly: capture-excluded
    // overlay immediately above its opaque shield, and never in the topmost
    // band. Activating the borrowed Discord window raises that sibling above the
    // composer, so the composer is first re-inserted immediately above Discord
    // and the shield is then restacked immediately behind the composer.
    //
    // `ensure_shield_stack` reveals the shield with `SWP_SHOWWINDOW`, so this
    // variant owes the same dismissal refusal as the QA one.
    refuse_dismissed_protected_pair()?;
    raise_protected_composer_above_discord(overlay, discord_window)?;
    ensure_shield_stack(overlay, shield, shielded)?;
    // Same four transitions the QA variant restores at, and for the same reason:
    // this is the last writer to touch the pair when opening, on a rebuilt
    // geometry (a resize or a DPI change), on a restored composer and on
    // corrected stack drift. Neither of the two calls above is a reveal or a
    // frame change on *this* HWND today, so this is defence rather than a known
    // hole -- but the blur-behind region has no getter, a geometry rebuild is
    // exactly where DWM re-creates a window's redirection surface, and the two
    // feature variants must not be able to drift apart on a contract the
    // operator can see. Still not a cadence: the guard reaches this call only at
    // those four transitions and never on a steady pass.
    enforce_transparent_protected_composer(overlay)
}

#[cfg(not(target_os = "windows"))]
fn active_ensure_carrier_stack(
    _overlay: &tauri::WebviewWindow,
    _shield: &tauri::WebviewWindow,
    _discord_window: isize,
    _shielded: bool,
) -> Result<(), String> {
    Err("The OSL protected window stack requires Windows".to_owned())
}

#[cfg(feature = "discord-qa-shell")]
fn show_capture_shield(shield: &tauri::WebviewWindow, _shielded: bool) -> Result<(), String> {
    shield
        .hide()
        .map_err(|_| "The temporary QA capture shield could not be disabled".to_owned())
}

/// The shield exists only while OSL is displaying decrypted text over Discord's
/// own rows, and only over the rows it is actually painting. With the eye off
/// there is no protected pixel anywhere on the message list -- the operator is
/// looking at Discord -- so an opaque window there would hide their real
/// conversation for no protection at all. That was the black band.
#[cfg(not(feature = "discord-qa-shell"))]
fn show_capture_shield(shield: &tauri::WebviewWindow, shielded: bool) -> Result<(), String> {
    if !shielded {
        return shield
            .hide()
            .map_err(|_| "The OSL capture shield could not be hidden safely".to_owned());
    }
    shield
        .show()
        .map_err(|_| "The OSL capture shield could not be shown safely".to_owned())
}

/// The protected composer is a transparent WebView whose only opaque content is
/// the sampled native composer background for this exact session and host
/// generation. With no sample there is nothing to paint, and revealing it puts a
/// see-through window over Discord -- the measured ghost. Reveal is therefore
/// gated on the sample existing, and the guard stays hidden when it does not.
fn native_surface_is_paintable(app: &tauri::AppHandle, epoch: u64, host_generation: u64) -> bool {
    app.state::<crate::native_surface_capture::NativeSurfaceCaptureState>()
        .current(crate::native_surface_capture::NativeSurfaceKey {
            session_epoch: epoch,
            host_generation,
        })
        .is_some()
}

/// The only place a protected window is ever revealed.
///
/// Two invariants that were each previously spread over separate call sites are
/// expressed here once: nothing is revealed without a native background to paint,
/// and nothing stays revealed with a Windows non-client frame. Tauri re-applies
/// its cached decoration state inside its own show path, so the frameless
/// contract is only durable when it is asserted after the reveal has actually
/// landed, for both the composer and the shield that is revealed with it. Tauri
/// returns from `show` long before the event loop has shown anything, so
/// "after the reveal" has to mean after a barrier that proves it, not after the
/// call: asserting the contract on the return of `show` strips a frame that tao
/// has not restored yet, and the restore then stands.
///
/// **The order of the two windows inside that is the flash.** From the instant
/// the composer's own reveal lands until `enforce_transparent_protected_composer` runs,
/// the composer is on screen *opaque*: tao's show path rebuilds this window's
/// cached flags, and that rebuild costs the per-pixel alpha DWM needs to honour
/// an alpha-0 pixel. Everything in that interval is frames the operator sees as
/// a pale slab sitting in Discord's message box. It used to contain a blocking
/// `settle_protected_window` round trip to the event loop *and* the shield's own
/// frame strip -- which retries with sleeps -- for work that has nothing to do
/// with the window that is flashing. The composer's frame and alpha are
/// therefore settled first and the shield's afterwards; the shield is opaque
/// black by design and has no alpha to lose, so nothing about it can flash.
fn reveal_protected_pair(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    epoch: u64,
    host_generation: u64,
    shielded: bool,
    show_error: &'static str,
) -> Result<(), String> {
    // A dismissed pair is not "hidden until the guard says otherwise", it is
    // off. Refusing here is what stops a pass that was already in flight when
    // the operator switched the composer off from putting it back on screen.
    refuse_dismissed_protected_pair()?;
    if !native_surface_is_paintable(app, epoch, host_generation) {
        return Err("The native Discord composer surface is not ready to paint".to_owned());
    }
    show_capture_shield(shield, shielded)?;
    window.show().map_err(|_| show_error.to_owned())?;
    // Both reveals above are queued, so wait for them to land before stripping.
    wait_for_revealed_protected_window(window, show_error)?;
    // Composer first, shield after. See the note above this function: the
    // interval between the show landing and the alpha restore is the flash.
    qa_label_overlay_frame_stage("reveal_composer_after_show_landed");
    #[cfg(target_os = "windows")]
    enforce_native_frameless_overlay(window)?;
    // Tauri's own show path re-applies this window's cached flags, so a reveal is
    // a frame change like any other and can leave the composer opaque. Restoring
    // the alpha here is what keeps a revealed surface see-through where it paints
    // nothing -- which is what lets hiding OSL's transcript layer reveal
    // Discord's own rows again instead of a slab.
    #[cfg(target_os = "windows")]
    enforce_transparent_protected_composer(window)?;
    settle_protected_window(shield, show_error)?;
    qa_label_overlay_frame_stage("reveal_shield_after_show_landed");
    #[cfg(target_os = "windows")]
    enforce_native_frameless_overlay(shield)?;
    Ok(())
}

fn verified_overlay_owner(
    app: &tauri::AppHandle,
    trusted_parent: isize,
) -> Result<tauri::WebviewWindow, String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "The trusted OSL overlay owner is unavailable".to_owned())?;
    #[cfg(target_os = "windows")]
    {
        let owner = main
            .hwnd()
            .map_err(|_| "The trusted OSL overlay owner is unavailable".to_owned())?
            .0 as isize;
        if owner == 0 || owner != trusted_parent {
            return Err("The trusted OSL overlay owner changed".to_owned());
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = trusted_parent;
    Ok(main)
}

#[cfg(target_os = "windows")]
fn verify_owned_overlay_window(
    window: &tauri::WebviewWindow,
    trusted_parent: isize,
) -> Result<(), String> {
    let hwnd = cached_window_hwnd(window)
        .ok_or_else(|| "The OSL protected window owner is unavailable".to_owned())?
        as windows_sys::Win32::Foundation::HWND;
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32;
    #[cfg(feature = "discord-qa-shell")]
    let forbidden_ex_style = WS_EX_APPWINDOW;
    #[cfg(not(feature = "discord-qa-shell"))]
    let forbidden_ex_style = WS_EX_TOPMOST | WS_EX_APPWINDOW;
    if hwnd.is_null()
        || unsafe { GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) } != trusted_parent
        || ex_style & forbidden_ex_style != 0
    {
        return Err("The OSL protected window owner could not be verified".to_owned());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn verify_owned_overlay_pair(
    overlay: &tauri::WebviewWindow,
    shield: &tauri::WebviewWindow,
    trusted_parent: isize,
) -> Result<(), String> {
    verify_owned_overlay_window(overlay, trusted_parent)?;
    verify_owned_overlay_window(shield, trusted_parent)
}

#[cfg(not(target_os = "windows"))]
fn verify_owned_overlay_pair(
    _overlay: &tauri::WebviewWindow,
    _shield: &tauri::WebviewWindow,
    _trusted_parent: isize,
) -> Result<(), String> {
    Err("The OSL protected window owner requires Windows".to_owned())
}

#[cfg(all(target_os = "windows", feature = "discord-qa-shell"))]
thread_local! {
    /// Which transition asked for the next frame enforcement on this thread.
    static QA_OVERLAY_FRAME_STAGE: std::cell::Cell<&'static str> =
        std::cell::Cell::new("unstaged");
}

/// Name the transition that is about to assert the frameless frame contract.
///
/// Every enforcement previously wrote one indistinguishable line, which is how
/// a real post-reveal caption was misread as a transient creation-time one: the
/// trace could not say which path had produced it. QA-only and thread-local, so
/// production pays nothing and concurrent overlay workers cannot mislabel each
/// other.
fn qa_label_overlay_frame_stage(stage: &'static str) {
    #[cfg(all(target_os = "windows", feature = "discord-qa-shell"))]
    QA_OVERLAY_FRAME_STAGE.with(|cell| cell.set(stage));
    #[cfg(not(all(target_os = "windows", feature = "discord-qa-shell")))]
    let _ = stage;
}

#[cfg(all(target_os = "windows", feature = "discord-qa-shell"))]
fn qa_overlay_frame_stage_label() -> &'static str {
    QA_OVERLAY_FRAME_STAGE.with(|cell| cell.get())
}

#[cfg(target_os = "windows")]
#[cfg(feature = "discord-qa-shell")]
fn qa_record_overlay_style(
    label: &str,
    stage: &str,
    hwnd: isize,
    before: (isize, isize),
    after: (isize, isize),
) {
    use std::io::Write as _;

    // Window labels and style bits only; no composer content reaches this path.
    let path = std::env::temp_dir().join("osl-discord-qa-overlay-style.txt");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        // Both words, and the value actually read back rather than the value
        // that was requested: a requested style proves only that this thread
        // asked, never that the frame ended up without one.
        let _ = writeln!(
            file,
            "{stage} {label} hwnd={hwnd} style_before=0x{:08X} style_after=0x{:08X} \
             ex_before=0x{:08X} ex_after=0x{:08X}",
            before.0, after.0, before.1, after.1
        );
    }
}

/// One installed frame hook: the window it belongs to, and the window procedure
/// it displaced.
#[cfg(target_os = "windows")]
struct ProtectedFrameHook {
    window: AtomicIsize,
    previous: AtomicIsize,
}

#[cfg(target_os = "windows")]
impl ProtectedFrameHook {
    const NEW: Self = Self {
        window: AtomicIsize::new(0),
        previous: AtomicIsize::new(0),
    };
}

/// Exactly one slot per retained protected window; the single-instance build
/// gate guarantees there are never more than these two.
#[cfg(target_os = "windows")]
static PROTECTED_FRAME_HOOKS: [ProtectedFrameHook; 2] =
    [ProtectedFrameHook::NEW, ProtectedFrameHook::NEW];

/// Give a protected window no non-client area at all, permanently.
///
/// This replaces re-stripping style bits as the thing that keeps the Windows
/// title bar off the protected composer, because re-stripping demonstrably did
/// not: the bits are rebuilt by tao from its cached window flags on the event
/// loop, every `show`, every `set_decorations`, every restore and every frame
/// change, while the strip runs on an overlay worker, so the strip is a race
/// that can be lost -- and losing it once ships a visible caption.
///
/// `WM_NCCALCSIZE` is not a style bit and cannot be rebuilt. Windows asks this
/// procedure, every single time it recomputes the frame, how much of the window
/// is non-client; answering "none" makes the client area the entire window
/// rectangle. A caption, a border, a sizing frame and a shadow are all drawn in
/// that area, so with the area at zero size there is nothing for any of them to
/// occupy no matter what `WS_CAPTION`, `WS_THICKFRAME` or `WS_EX_WINDOWEDGE`
/// say. `WM_NCPAINT` is answered the same way, so even a mis-sized frame could
/// not paint. The hook is installed once, immediately after the window is
/// created and before it has ever been shown, and is never removed while the
/// window lives, so every later show/hide/restore/geometry change is already
/// behind it.
#[cfg(target_os = "windows")]
unsafe extern "system" fn protected_frameless_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handle = hwnd as isize;
    let hook = PROTECTED_FRAME_HOOKS
        .iter()
        .find(|hook| hook.window.load(Ordering::Acquire) == handle);
    // The whole window is client area. Returning zero here without touching the
    // proposed rectangle is the documented way to say "no non-client frame".
    if message == WM_NCCALCSIZE && wparam != 0 {
        return 0;
    }
    // Nothing may paint outside the client area either.
    if message == WM_NCPAINT {
        return 0;
    }
    // Only reachable in the instant between claiming a slot and publishing the
    // displaced procedure, and after the window has been destroyed. Never call
    // through a null procedure.
    let Some(hook) = hook.filter(|hook| hook.previous.load(Ordering::Acquire) != 0) else {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    let previous = hook.previous.load(Ordering::Acquire);
    let result = unsafe {
        CallWindowProcW(
            std::mem::transmute::<isize, WNDPROC>(previous),
            hwnd,
            message,
            wparam,
            lparam,
        )
    };
    if message == WM_NCDESTROY {
        hook.window.store(0, Ordering::Release);
        hook.previous.store(0, Ordering::Release);
    }
    result
}

/// Install the frame hook on a freshly created protected window.
#[cfg(target_os = "windows")]
fn install_protected_frame_hook(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window
        .hwnd()
        .map_err(|_| "The OSL protected window frame is unavailable".to_owned())?
        .0 as HWND;
    if hwnd.is_null() {
        return Err("The OSL protected window frame is unavailable".to_owned());
    }
    let handle = hwnd as isize;
    if PROTECTED_FRAME_HOOKS
        .iter()
        .any(|hook| hook.window.load(Ordering::Acquire) == handle)
    {
        return Ok(());
    }
    let slot = PROTECTED_FRAME_HOOKS
        .iter()
        .find(|hook| {
            hook.window
                .compare_exchange(0, handle, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        })
        .ok_or_else(|| "The OSL protected window frame could not be claimed".to_owned())?;
    let previous = unsafe {
        SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            protected_frameless_window_proc as usize as isize,
        )
    };
    if previous == 0 {
        slot.window.store(0, Ordering::Release);
        return Err("The OSL protected window frame could not be replaced".to_owned());
    }
    // Published before the hook can be reached through the window: the slot's
    // handle is what the procedure above matches on, and it is already set.
    slot.previous.store(previous, Ordering::Release);
    // Make Windows recompute the frame now, so the window is frameless before it
    // is ever shown rather than at its first reveal.
    unsafe {
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn install_protected_frame_hook(_window: &tauri::WebviewWindow) -> Result<(), String> {
    Ok(())
}

/// Non-Windows twin. The frame enforcement above is pure Win32
/// (`GWL_STYLE` / `GWL_EXSTYLE` read-write-verify), so there is nothing to
/// enforce here. It REFUSES rather than returning Ok: the return value feeds a
/// path that treats success as "this window is a verified frameless protected
/// overlay", and claiming that on a platform where no style word was ever
/// checked would be a false green. OSL ships on Windows only; non-Windows
/// builds exist so the binary can be built and screenshotted locally, because
/// capture protection is compile-time and Windows-only.
#[cfg(not(target_os = "windows"))]
fn enforce_native_frameless_overlay(_window: &tauri::WebviewWindow) -> Result<bool, String> {
    Err("The native Discord overlay frame is only enforced on Windows".to_owned())
}

/// Strip and prove the entire non-client frame on a protected window.
///
/// Both style words are corrected here. tao rebuilds `GWL_STYLE` from its own
/// cached window flags and that rebuild unconditionally carries
/// `WS_CAPTION|WS_SYSMENU`; in the same call it rewrites `GWL_EXSTYLE` with
/// `WS_EX_WINDOWEDGE`. Only `GWL_STYLE` was ever cleared or verified, so a run
/// could report a verified frameless protected window that still wore an
/// extended-style edge, and no trace could contradict it.
///
/// The rebuild happens on the event loop while this runs on an overlay worker,
/// so a correction can lose the race. Losing it must not end the session and
/// take the composer off screen, so the read/write/verify is retried a bounded
/// number of times and only the last attempt can fail. The frame change itself
/// is issued only when a bit actually drifted: re-asserting the frame of an
/// already-correct WebView2 window on a cadence is what previously interrupted
/// keyboard delivery and saturated the borrowed app's UI thread.
/// Returns whether this call actually had to rewrite a style word. A frame
/// change is what can clear the composer's per-pixel alpha, so the transparency
/// contract below is re-asserted exactly when one was issued and never on a
/// cadence.
#[cfg(target_os = "windows")]
fn enforce_native_frameless_overlay(window: &tauri::WebviewWindow) -> Result<bool, String> {
    let hwnd = window
        .hwnd()
        .map_err(|_| "The native Discord overlay frame is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return Err("The native Discord overlay frame is unavailable".to_owned());
    }
    let mut corrected = false;
    for attempt in 0..NATIVE_OVERLAY_FRAME_ATTEMPTS {
        let style = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
        let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        let frameless = frameless_native_overlay_style(style);
        let frameless_ex = frameless_native_overlay_ex_style(ex_style);
        if frameless != style || frameless_ex != ex_style {
            corrected = true;
            unsafe {
                SetWindowLongPtrW(hwnd, GWL_STYLE, frameless);
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, frameless_ex);
            }
            if unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                )
            } == 0
            {
                return Err("The native Discord overlay frame could not be removed".to_owned());
            }
        }
        let verified = unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) };
        let verified_ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
        #[cfg(feature = "discord-qa-shell")]
        qa_record_overlay_style(
            window.label(),
            qa_overlay_frame_stage_label(),
            hwnd as isize,
            (style, ex_style),
            (verified, verified_ex),
        );
        if verified & NATIVE_OVERLAY_FRAME_STYLE_MASK == 0
            && verified_ex & NATIVE_OVERLAY_FRAME_EX_STYLE_MASK == 0
        {
            return Ok(corrected);
        }
        if attempt + 1 == NATIVE_OVERLAY_FRAME_ATTEMPTS {
            break;
        }
        // The only thing that puts these bits back is tao applying its cached
        // flags on the event loop. Yield once so that write lands before the
        // retry reads the style again, instead of spinning against it.
        std::thread::sleep(NATIVE_OVERLAY_FRAME_RETRY_DELAY);
    }
    Err("The native Discord overlay frame could not be verified".to_owned())
}

/// Restore the per-pixel alpha the frame contract can silently take away.
///
/// The protected composer is made see-through exactly once, by tao, at creation:
/// `transparent(true)` makes it enable a DWM blur-behind with an *empty* region,
/// which is the documented way to have DWM honour a normal window's alpha
/// channel. Nothing in tao, wry or Tauri ever re-applies it, and the builder flag
/// is therefore not a durable contract -- it is a single creation-time call.
/// (`WS_EX_NOREDIRECTIONBITMAP` is not involved: nothing sets it.)
///
/// Three things this crate does can take that alpha away again. Two of them are
/// the frame fix:
/// * every `SWP_FRAMECHANGED`, `set_decorations` and `set_shadow` makes DWM
///   re-evaluate the window frame, and
/// * `suppress_accent_border` falls back, when `DWMWA_BORDER_COLOR` is rejected
///   (Windows 10), to `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED`, which turns
///   DWM rendering off for this window outright.
///
/// The third is not a frame change at all and is why the corners kept coming
/// back white after every frame-side restore was in place:
/// * `SetWindowDisplayAffinity` moves the window into a separate DWM
///   composition path, and the blur-behind region does not survive that move.
///   It is issued on every `Focused(true)`, i.e. every click into the composer.
///   `apply_protected_composer_capture_protection` is the only caller allowed to
///   issue it on this HWND, and it pairs the restore with the write.
///
/// A protected window that has lost per-pixel alpha is opaque, and the whole
/// area above the composer strip paints nothing of its own, so every alpha-0
/// pixel composites against WebView2's own default backing -- which is white.
/// That is the measured band: `lightPixels=98%` across the top of the surface,
/// with the web layer provably painting nothing above the composer.
///
/// Composer only, deliberately. The capture shield is built opaque black on
/// purpose and must stay that way.
///
/// Not a cadence, by construction. The readable half is only written when the
/// read proves it drifted, and the contract as a whole is only asserted at the
/// transitions that can have cleared it: creation, the pre-reveal frame
/// contract, a landed reveal, the carrier stack, and an affinity write that
/// actually changed the window's composition path. Every one of those is a
/// transition; none of them is a poll. Neither call touches a window style, a
/// position, a z-order or the capture affinity, so nothing here can disturb
/// hit-testing, keyboard delivery or the capture shield -- and both are
/// in-process DWM calls that issue no window message and block on no other
/// process, so the focus handler may make them on the UI thread.
/// Set by the only writer that moves the composer between DWM composition
/// paths, and consumed by the very next alpha restore.
///
/// DWM has no getter for blur-behind, and it *elides* a repeat
/// `DwmEnableBlurBehindWindow(fEnable = TRUE)` whose parameters it believes are
/// already in force -- which is exactly the state after a composition-path move:
/// DWM still has the window recorded as blurred while the redirection surface
/// underneath it has been recreated without the region. The re-enable therefore
/// returns `S_OK` and changes nothing, which is why the corners kept coming back
/// white on a build whose restore call sites were all in the right places.
///
/// The documented way to make the re-enable land is a `FALSE` -> `TRUE` pair
/// with a freshly created region. That pair is confined to this latch on
/// purpose: at a composition-path change the window has *already* lost its
/// alpha, so a disable cannot make any frame worse than it already is, whereas
/// issuing one at a transition where the alpha is intact could put a single
/// opaque frame on screen -- the exact flash this file must not produce.
#[cfg(target_os = "windows")]
static COMPOSER_COMPOSITION_PATH_CHANGED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
fn enforce_transparent_protected_composer(window: &tauri::WebviewWindow) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmEnableBlurBehindWindow, DwmGetWindowAttribute, DWMNCRP_ENABLED,
        DWMWA_NCRENDERING_ENABLED, DWMWA_NCRENDERING_POLICY, DWM_BB_BLURREGION, DWM_BB_ENABLE,
        DWM_BLURBEHIND,
    };
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, DeleteObject};

    let hwnd = window
        .hwnd()
        .map_err(|_| "The native Discord overlay transparency is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return Err("The native Discord overlay transparency is unavailable".to_owned());
    }
    // The one half of this contract Windows will read back. DWM composition is
    // what honours the alpha channel at all, so a window it has stopped
    // rendering can never be see-through no matter what region is set below.
    let mut rendering: i32 = 0;
    let read = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_ENABLED as u32,
            (&mut rendering as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
        )
    };
    if read >= 0 && rendering == 0 {
        // Written only because the read above proved it drifted. The frame masks
        // have already removed WS_CAPTION and WS_THICKFRAME, so this window has
        // no non-client area left for DWM to draw an accent outline into: the
        // border suppression that disabled this keeps its effect and only the
        // composition this surface needs comes back.
        let policy = DWMNCRP_ENABLED;
        let restored = unsafe {
            windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY as u32,
                (&policy as *const i32).cast(),
                std::mem::size_of::<i32>() as u32,
            )
        };
        if restored < 0 {
            return Err("The native Discord overlay transparency could not be restored".to_owned());
        }
    }
    // An empty region: blur nothing, but have DWM honour this window's alpha
    // channel. Byte-for-byte the call tao makes for `transparent(true)`, so this
    // only ever restores the creation-time state and never invents a new one.
    // `DwmEnableBlurBehindWindow` has no getter, so it cannot be read-gated; it
    // is instead confined to the transitions above, and it is a DWM-side call
    // that issues no window message.
    //
    // Every region below is created fresh and released immediately after the
    // call it was made for: DWM copies what it needs and the caller keeps
    // ownership of the handle, so re-using one would only let DWM compare it
    // against the handle it already has and elide the write again.
    let blur_behind = |enable: i32| {
        let region = unsafe { CreateRectRgn(0, 0, -1, -1) };
        let blur = DWM_BLURBEHIND {
            dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
            fEnable: enable,
            hRgnBlur: region,
            fTransitionOnMaximized: 0,
        };
        let result = unsafe { DwmEnableBlurBehindWindow(hwnd, &blur) };
        if !region.is_null() {
            unsafe { DeleteObject(region.cast()) };
        }
        result
    };
    // Consumed, never merely read: one composition-path change owes exactly one
    // toggle, and a restore that runs for any other reason must not issue one.
    let recomposited = COMPOSER_COMPOSITION_PATH_CHANGED.swap(false, Ordering::AcqRel);
    if recomposited {
        // Failure here is not fatal on its own: the window is already opaque at
        // this point, so a refused disable leaves it exactly as it was and the
        // enable below is still the call that has to land.
        let _ = blur_behind(0);
    }
    let mut enabled = blur_behind(1);
    if enabled < 0 && recomposited {
        // The one case where this function can have made things worse than it
        // found them -- a disable that landed followed by an enable that did
        // not. Retry once before reporting, so a single refused DWM call cannot
        // turn a working composer into an opaque one.
        enabled = blur_behind(1);
    }
    if enabled < 0 {
        return Err("The native Discord overlay transparency could not be restored".to_owned());
    }
    Ok(())
}

/// The message every capture-affinity failure on the composer reports.
const OVERLAY_CAPTURE_PROTECTION_ERROR: &str =
    "The native Discord overlay could not enable capture resistance";

/// Set the composer's capture affinity, and put back the alpha that write takes
/// with it.
///
/// This is the writer the frame contract above could never cover, because it is
/// not a frame change at all. `SetWindowDisplayAffinity` implements window
/// content protection by moving the window into a *separate DWM composition
/// path*, and a window that changes composition path loses the blur-behind
/// region that is the only reason DWM honours its alpha channel. tao installs
/// that region exactly once, at creation
/// (`tao/src/platform_impl/windows/window.rs`, `DwmEnableBlurBehindWindow` under
/// `attributes.transparent`), and its own event loop carries a standing FIXME
/// that nothing re-applies it when composition changes underneath the window.
/// Neither tao, wry nor Tauri has any other writer of it.
///
/// Why this and not the frame writers is what the operator kept seeing: every
/// other writer on this HWND happens at a transition a guard drives, and each of
/// those already restores the alpha as its last act. This one is installed by
/// `build_overlay_window` on **every** `Focused(true)`, so it fires again every
/// single time the operator clicks into the composer -- after the reveal, after
/// the frame strip, after the carrier stack, with nothing left to run behind it.
/// That is precisely the reported shape: the corners turn white during ordinary
/// use and stay white, on a build whose reveal paths all restore correctly.
///
/// Two things happen here, in this order:
/// * the affinity is read first and written only when it actually differs, so
///   the hundredth click into the composer issues no Win32 write at all. This is
///   not a weakening of the capture contract: the previous code assumed its
///   write had landed, whereas a successful read proves the window's affinity
///   already equals the required one, and any failed read falls straight through
///   to the write.
/// * when a write did happen the alpha is restored immediately, inside the same
///   call, so no caller can observe the window between the two.
#[cfg(target_os = "windows")]
fn apply_protected_composer_capture_protection(
    window: &tauri::WebviewWindow,
) -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };

    let protection = active_overlay_capture_protection();
    let required = match protection {
        runtime::ScreenshotProtection::On => WDA_EXCLUDEFROMCAPTURE,
        runtime::ScreenshotProtection::Off => WDA_NONE,
    };
    let hwnd = window
        .hwnd()
        .map_err(|_| OVERLAY_CAPTURE_PROTECTION_ERROR.to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    let mut current: u32 = 0;
    if !hwnd.is_null()
        && unsafe { GetWindowDisplayAffinity(hwnd, &mut current) } != 0
        && current == required
    {
        return Ok(());
    }
    super::screenshot::apply_to_window(window, protection)
        .map_err(|_| OVERLAY_CAPTURE_PROTECTION_ERROR.to_owned())?;
    // Armed only here, because this is the only write on this HWND that moves it
    // between DWM composition paths. The restore below consumes it and answers
    // with a disable/enable pair, which is the only form of the call DWM will
    // not elide once it already believes blur-behind is on.
    COMPOSER_COMPOSITION_PATH_CHANGED.store(true, Ordering::Release);
    // Paired with the write above and never issued without it: the composition
    // path only moves when the affinity actually changes.
    enforce_transparent_protected_composer(window)
}

/// Non-Windows builds have no display affinity and no DWM alpha to lose; the
/// call is kept so the shape of the protected path is identical everywhere.
#[cfg(not(target_os = "windows"))]
fn apply_protected_composer_capture_protection(
    window: &tauri::WebviewWindow,
) -> Result<(), String> {
    super::screenshot::apply_to_window(window, active_overlay_capture_protection())
        .map_err(|_| OVERLAY_CAPTURE_PROTECTION_ERROR.to_owned())
}

/// Prove that every window change already asked of Tauri has actually landed.
///
/// `show`, `hide`, `set_decorations` and `set_shadow` are queued to the event
/// loop and return immediately, while every Tauri getter is a blocking round
/// trip through that same FIFO queue. One getter therefore proves that all of
/// them have already been handled.
///
/// Measured symptom of not waiting: the style trace for a reveal recorded
/// `before=0x84C80000`, a style carrying no `WS_VISIBLE` at all, so the
/// "post-reveal" frame strip had in fact run against a window Tauri had not yet
/// shown. tao's own show then reapplied `WS_CAPTION|WS_SYSMENU` and rewrote the
/// extended style, with nothing left to remove either -- which is the Windows
/// title bar the owner kept seeing on the protected composer.
///
/// Called only from the overlay worker threads. Safe from the event loop too:
/// Tauri handles a getter inline when it is already on the main thread.
fn settle_protected_window(
    window: &tauri::WebviewWindow,
    error: &'static str,
) -> Result<(), String> {
    window
        .is_visible()
        .map(|_| ())
        .map_err(|_| error.to_owned())
}

/// Reveal barrier: the window is on screen and the reveal Tauri performed --
/// including tao's reapplication of its cached, captioned window flags -- is
/// already done, so the frame contract asserted next is the last writer.
fn wait_for_revealed_protected_window(
    window: &tauri::WebviewWindow,
    error: &'static str,
) -> Result<(), String> {
    for _ in 0..PROTECTED_REVEAL_SETTLE_ATTEMPTS {
        if window.is_visible().map_err(|_| error.to_owned())? {
            return Ok(());
        }
        std::thread::sleep(PROTECTED_REVEAL_SETTLE_DELAY);
    }
    Err(error.to_owned())
}

/// Exactly one retained window per protected label, in every path.
///
/// Tauri's duplicate-label rejection (`WindowManager::prepare_window`) and its
/// manager insert (`WindowManager::attach_window`) are not atomic: the label
/// only becomes visible to `get_webview_window` after the native window and its
/// WebView have been created, which takes hundreds of milliseconds. Two callers
/// that both pass an `is_none()` check inside that gap therefore both create a
/// real HWND, and the second insert silently replaces the first in Tauri's
/// manager. The orphan keeps the same window title, keeps the Windows caption it
/// was born with, and can never again be found by label, hidden, reframed, or
/// closed. Every construction of the pair goes through the lock below with the
/// existence check repeated inside it, so no path can build a second one.
///
/// The lock is held across the build, which blocks on the event loop. Every
/// caller (`prewarm` from the page-load worker, `show` from the overlay command
/// worker) runs off the main thread, so the event loop never waits on this lock.
static PROTECTED_WINDOW_BUILD_LOCK: Mutex<()> = Mutex::new(());

/// The two retained protected surfaces, so the single-instance rule and the
/// frameless contract are each expressed once for both windows.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ProtectedSurface {
    Composer,
    Shield,
}

impl ProtectedSurface {
    fn label(self) -> &'static str {
        match self {
            Self::Composer => OVERLAY_LABEL,
            Self::Shield => SHIELD_LABEL,
        }
    }

    fn frame_error(self) -> &'static str {
        match self {
            Self::Composer => "The native Discord overlay frame could not be removed",
            Self::Shield => "The OSL capture shield frame could not be removed",
        }
    }

    fn shadow_error(self) -> &'static str {
        match self {
            Self::Composer => "The native Discord overlay shadow could not be removed",
            Self::Shield => "The OSL capture shield shadow could not be removed",
        }
    }

    fn accent_border_error(self) -> &'static str {
        match self {
            Self::Composer => "The native Discord overlay accent border could not be removed",
            Self::Shield => "The OSL capture shield accent border could not be removed",
        }
    }
}

/// Strip the non-client frame, shadow and accent border from a retained
/// protected window. Tauri's builder flag alone leaves WS_CAPTION on these owned
/// transparent windows, and a retained window can additionally carry stale
/// non-client styles, which is what put a Windows title bar around the protected
/// surface. Applied at creation and again by the session path before any reveal.
///
/// Order matters and is not the order the calls are written in. Both Tauri
/// setters below are queued to the event loop and each one makes tao rebuild
/// this window's whole style from its cached flags -- which always carry
/// `WS_CAPTION|WS_SYSMENU`. Stripping the frame between them ran before either
/// had landed, so the strip was simply overwritten. Both setters are issued
/// first, the queue is drained, and only then is the frame stripped and proven.
fn apply_protected_frame_contract(
    window: &tauri::WebviewWindow,
    surface: ProtectedSurface,
) -> Result<(), String> {
    // First, and once: after this the window has no non-client area for any of
    // the writers below -- or for tao's own cached-flag rebuild -- to put a
    // caption, border or sizing frame into.
    install_protected_frame_hook(window).map_err(|_| surface.frame_error().to_owned())?;
    window
        .set_decorations(false)
        .map_err(|_| surface.frame_error().to_owned())?;
    window
        .set_shadow(false)
        .map_err(|_| surface.shadow_error().to_owned())?;
    settle_protected_window(window, surface.frame_error())?;
    qa_label_overlay_frame_stage("frame_contract_after_setters_landed");
    #[cfg(target_os = "windows")]
    enforce_native_frameless_overlay(window)?;
    #[cfg(target_os = "windows")]
    super::window_border::suppress_accent_border(window.as_ref())
        .map_err(|_| surface.accent_border_error().to_owned())?;
    // Last, because both writers above can clear this window's per-pixel alpha
    // and the border suppression is one of them. Composer only: the shield is
    // opaque black by design and is what ordinary Windows capture sees.
    #[cfg(target_os = "windows")]
    if matches!(surface, ProtectedSurface::Composer) {
        enforce_transparent_protected_composer(window)?;
    }
    Ok(())
}

/// The only place a protected window is ever constructed. Reuses the retained
/// window when one already exists, and serializes construction so the pre-warm
/// worker and a concurrent lock toggle cannot each create one.
fn ensure_retained_protected_window(
    app: &tauri::AppHandle,
    surface: ProtectedSurface,
    build: impl FnOnce() -> Result<tauri::WebviewWindow, String>,
) -> Result<tauri::WebviewWindow, String> {
    // Poisoning would only mean an earlier builder panicked; this lock guards an
    // existence check against Tauri's registration gap, not shared data.
    crate::startup_breadcrumb("overlay_build_lock_before"); // STARTUP-TRACE
    let _build_lock = PROTECTED_WINDOW_BUILD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::startup_breadcrumb("overlay_build_lock_acquired"); // STARTUP-TRACE
    if let Some(window) = app.get_webview_window(surface.label()) {
        crate::startup_breadcrumb("overlay_build_lock_reused_existing_window"); // STARTUP-TRACE
        return Ok(window);
    }
    crate::startup_breadcrumb("overlay_build_window_before"); // STARTUP-TRACE
    let window = build()?;
    crate::startup_breadcrumb("overlay_build_window_after"); // STARTUP-TRACE
                                                             // A pre-warmed window is created long before any session exists, so its
                                                             // frame is stripped here too: nothing that can ever reach the screen may
                                                             // wear a Windows caption. The post-reveal enforcement still runs every time
                                                             // because Tauri re-applies its cached decorations while revealing.
    if let Err(error) = apply_protected_frame_contract(&window, surface) {
        // A window that cannot be proven frameless must not stay registered
        // under this label. Retaining it would hand the next caller a captioned
        // protected surface that it would then reveal.
        let _ = window.hide();
        let _ = window.close();
        wait_for_freed_protected_labels(app, &[surface.label()]);
        return Err(error);
    }
    Ok(window)
}

/// Create the opaque capture shield exactly once. Every builder flag here is
/// the reviewed production contract; only the rectangle is a parameter so the
/// same window can be built eagerly before a Discord rectangle exists.
fn build_shield_window(
    app: &tauri::AppHandle,
    rect: OverlayRect,
    trusted_parent: isize,
) -> Result<tauri::WebviewWindow, String> {
    let main = verified_overlay_owner(app, trusted_parent)?;
    tauri::WebviewWindowBuilder::new(
        app,
        SHIELD_LABEL,
        WebviewUrl::App(PathBuf::from(SHIELD_ASSET)),
    )
    .parent(&main)
    .map_err(|_| "The OSL capture shield owner could not be bound safely".to_owned())?
    .title("OSL capture shield")
    .position(f64::from(rect.x), f64::from(rect.y))
    .inner_size(f64::from(rect.width), f64::from(rect.height))
    .transparent(false)
    .background_color(Color(0, 0, 0, 255))
    .decorations(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .always_on_top(false)
    .skip_taskbar(true)
    .shadow(false)
    .focused(false)
    .focusable(false)
    .visible(false)
    .devtools(false)
    .on_navigation(bundled_shield_navigation)
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .build()
    .map_err(|_| "The OSL capture shield could not be created safely".to_owned())
}

/// Create the protected composer WebView exactly once. Same contract as the
/// shield: only the rectangle is a parameter, so an eagerly built window is
/// byte-for-byte the window the open path would have built on demand.
fn build_overlay_window(
    app: &tauri::AppHandle,
    rect: OverlayRect,
    trusted_parent: isize,
) -> Result<tauri::WebviewWindow, String> {
    let main = verified_overlay_owner(app, trusted_parent)?;
    let builder = tauri::WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        WebviewUrl::App(PathBuf::from(OVERLAY_ASSET)),
    );
    let builder = builder
        .parent(&main)
        .map_err(|_| "The native Discord overlay owner could not be bound safely".to_owned())?;
    let window = builder
        .title("OSL private composer")
        .position(f64::from(rect.x), f64::from(rect.y))
        .inner_size(f64::from(rect.width), f64::from(rect.height))
        .transparent(true)
        .decorations(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .always_on_top(false)
        .skip_taskbar(true)
        .shadow(false)
        // A minimized or disconnected RDP desktop has no foreground queue.
        // Asking WebView2 to take initial focus there can block window
        // creation indefinitely. Focus is requested after creation only when
        // Windows reports an interactive foreground desktop.
        .focused(false)
        .visible(false)
        .devtools(false)
        .on_navigation(bundled_overlay_navigation)
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()
        .map_err(|_| "The native Discord overlay could not be created safely".to_owned())?;
    let focus_window = window.clone();
    let focus_app = app.clone();
    window.on_window_event(move |event| {
        // Capture affinity *and* the per-pixel alpha that re-applying it would
        // otherwise drop. This handler is the only writer on this HWND that a
        // person triggers directly -- one click into the composer is one
        // Focused(true) -- so it is also the only one that can leave the
        // composer opaque long after every guard-driven transition has settled.
        if matches!(event, tauri::WindowEvent::Focused(true))
            && apply_protected_composer_capture_protection(&focus_window).is_err()
        {
            clear_and_hide(&focus_app);
        }
        // A newly shown WebView can emit Focused(false) before Windows
        // exposes its HWND to the foreground queue. The signed-host,
        // owner, generation, context, geometry, and focus guard below
        // performs the authoritative fail-closed check every 500 ms.
        // Closing here races legitimate initial focus on VM/RDP desktops.
    });
    Ok(window)
}

/// Rectangle used only to build the retained pair before Discord is known.
/// Both windows stay hidden here and are repositioned onto the verified
/// composer rectangle by the guard's own `position_window_pair` -- the single
/// placement writer -- before anything is ever revealed.
const PREWARM_OVERLAY_RECT: OverlayRect = OverlayRect {
    x: 0,
    y: 0,
    width: 640,
    height: 400,
};

/// Build both protected WebViews once, eagerly and hidden, so that toggling
/// the lock is a `show()`/`hide()` instead of a WebView create+navigate.
/// Nothing here activates a session, grants a context, applies a geometry, or
/// reveals a pixel; `show`/`start_guard` still perform every identity,
/// generation, geometry, context, process and frameless check before the
/// retained windows become visible.
pub(crate) fn prewarm(app: &tauri::AppHandle) -> Result<(), String> {
    crate::startup_breadcrumb("overlay_prewarm_enter"); // STARTUP-TRACE
    if app.get_webview_window(OVERLAY_LABEL).is_some()
        && app.get_webview_window(SHIELD_LABEL).is_some()
    {
        crate::startup_breadcrumb("overlay_prewarm_already_retained"); // STARTUP-TRACE
        return Ok(());
    }
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "The trusted OSL overlay owner is unavailable".to_owned())?;
    #[cfg(target_os = "windows")]
    let trusted_parent = main
        .hwnd()
        .map_err(|_| "The trusted OSL overlay owner is unavailable".to_owned())?
        .0 as isize;
    #[cfg(not(target_os = "windows"))]
    let trusted_parent = {
        let _ = main;
        0isize
    };
    // Same labels and the same single construction point as the session path, so
    // `ensure_shield_window`/`ensure_window` reuse exactly these two windows.
    crate::startup_breadcrumb("overlay_prewarm_shield_build_before"); // STARTUP-TRACE
    ensure_retained_protected_window(app, ProtectedSurface::Shield, || {
        build_shield_window(app, PREWARM_OVERLAY_RECT, trusted_parent)
    })?;
    crate::startup_breadcrumb("overlay_prewarm_shield_build_after"); // STARTUP-TRACE
    crate::startup_breadcrumb("overlay_prewarm_composer_build_before"); // STARTUP-TRACE
    ensure_retained_protected_window(app, ProtectedSurface::Composer, || {
        build_overlay_window(app, PREWARM_OVERLAY_RECT, trusted_parent)
    })?;
    crate::startup_breadcrumb("overlay_prewarm_composer_build_after"); // STARTUP-TRACE
    crate::startup_breadcrumb("overlay_prewarm_done"); // STARTUP-TRACE
    Ok(())
}

fn ensure_shield_window(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
    trusted_parent: isize,
) -> Result<tauri::WebviewWindow, String> {
    // Reuses the pre-warmed shield; only a cold path with no retained shield
    // builds one, and only ever one, under the shared construction lock.
    let shield = ensure_retained_protected_window(app, ProtectedSurface::Shield, || {
        let composer_bounds = app
            .state::<NativeDiscordComposerState>()
            .verified_composer_bounds();
        let rect = active_overlay_rect_with_composer(discord_rect, composer_bounds, None)
            .ok_or_else(|| {
                "The native Discord window is too small for safe protection".to_owned()
            })?;
        build_shield_window(app, rect, trusted_parent)
    })?;
    // The shield carries the same frameless contract as the composer it backs.
    apply_protected_frame_contract(&shield, ProtectedSurface::Shield)?;
    // Deliberately not positioned over Discord here. A shield only ever gets a
    // rectangle once the guard knows which rows OSL is actually painting, and
    // only while the eye is on; until then it stays where it was built, hidden.
    Ok(shield)
}

fn ensure_window(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
    trusted_parent: isize,
) -> Result<tauri::WebviewWindow, String> {
    // Reuses the pre-warmed composer; only a cold path with no retained overlay
    // builds one, and only ever one, under the shared construction lock.
    let window = ensure_retained_protected_window(app, ProtectedSurface::Composer, || {
        let composer_bounds = app
            .state::<NativeDiscordComposerState>()
            .verified_composer_bounds();
        let rect = active_overlay_rect_with_composer(discord_rect, composer_bounds, None)
            .ok_or_else(|| {
                "The native Discord window is too small for safe protection".to_owned()
            })?;
        build_overlay_window(app, rect, trusted_parent)
    })?;
    // Reassert the frameless contract for both newly-created and retained
    // overlay HWNDs. A reused WebView window can otherwise keep stale
    // non-client styles from an earlier QA build and show a Windows titlebar.
    apply_protected_frame_contract(&window, ProtectedSurface::Composer)?;
    // Deliberately NOT positioned here any more, exactly like the shield above.
    //
    // This used to be a Tauri `set_size` + `set_position` pair, queued to the
    // event loop from *this* worker thread, while the reveal that follows is
    // queued from the guard thread. Two threads posting to one event loop have
    // no ordering between them, so the reveal could be serviced before the
    // placement: the retained composer became visible at whatever rectangle it
    // last held -- the 640x400 pre-warm rectangle in the screen's top-left
    // corner on the first engage of a session, or the previous conversation's
    // composer rectangle afterwards -- and was only then moved onto Discord's
    // message box. That is the reported flash, and it is also why it is
    // intermittent: it is an interleaving, not a state.
    //
    // The guard now owns placement outright and places before it reveals, in the
    // same pass, on the same thread, through the one deferred `DeferWindowPos`
    // batch. One writer, one appearance, already in the right place.
    Ok(window)
}

/// Open the retained pair for a verified session.
///
/// Every failure between the first window handle and a running guard must leave
/// nothing visible and no session active: an aborted open must not be able to
/// return while a retained window is still on screen with no guard left to hide
/// it. The whole fallible path therefore runs inside `show_guarded_overlay` and
/// any error ends the session and proves the pair is off screen.
pub(crate) fn show(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
    discord_window: isize,
    trusted_parent: isize,
    epoch: u64,
) -> Result<(), String> {
    match show_guarded_overlay(app, discord_rect, discord_window, trusted_parent, epoch) {
        Ok(()) => Ok(()),
        Err(error) => {
            app.state::<OverlaySessionState>().clear();
            hide_protected_pair_or_destroy(app);
            Err(error)
        }
    }
}

fn show_guarded_overlay(
    app: &tauri::AppHandle,
    discord_rect: [i32; 4],
    discord_window: isize,
    trusted_parent: isize,
    epoch: u64,
) -> Result<(), String> {
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage("show_entered");
    // The operator asked for a composer, so the pair is admitted on screen
    // again. This is the ONLY writer that clears the dismissal, it runs once per
    // verified open, and it runs before any window of this session is touched --
    // so nothing between here and the guard's first reveal can be refused by a
    // latch the previous session set. Every failure below re-dismisses through
    // `show`'s own error path.
    admit_protected_pair();
    let shield = ensure_shield_window(app, discord_rect, trusted_parent)?;
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage("shield_ready");
    let window = ensure_window(app, discord_rect, trusted_parent)?;
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage("overlay_window_ready");
    // OSL plaintext may appear only after the new HWND is capture-resistant.
    if apply_protected_composer_capture_protection(&window).is_err() {
        let _ = window.hide();
        let _ = window.close();
        // Fail closed, then make the retry deterministic: the next open must be
        // able to build a replacement instead of colliding with this label while
        // Tauri is still destroying it on the event loop.
        wait_for_freed_protected_labels(app, &[OVERLAY_LABEL]);
        app.state::<OverlaySessionState>().clear();
        return Err("The native Discord overlay could not enable capture resistance".to_owned());
    }
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage("overlay_affinity_ready");
    // Both HWNDs remain hidden until the first complete host/context/focus
    // guard succeeds. Renderer IPC is phase-gated independently, so a hidden
    // WebView cannot fetch or render protected plaintext while it initializes.
    verify_owned_overlay_pair(&window, &shield, trusted_parent)?;
    // A session must never exist without a guard watching it, so a guard that
    // cannot even be started fails the open.
    start_guard(
        app.clone(),
        epoch,
        discord_rect,
        discord_window,
        trusted_parent,
    )?;
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage("guard_started");
    Ok(())
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum FirstGuardDecision {
    WaitHidden,
    Reveal,
    Close,
}

fn first_guard_decision(
    desktop_has_foreground: bool,
    discord_foreground: bool,
    osl_foreground: bool,
    startup_grace_active: bool,
) -> FirstGuardDecision {
    if !desktop_has_foreground || discord_foreground || osl_foreground {
        FirstGuardDecision::Reveal
    } else if startup_grace_active {
        FirstGuardDecision::WaitHidden
    } else {
        FirstGuardDecision::Close
    }
}

#[cfg(feature = "discord-qa-shell")]
fn active_first_guard_decision(
    desktop_has_foreground: bool,
    discord_foreground: bool,
    osl_foreground: bool,
    startup_grace_active: bool,
) -> FirstGuardDecision {
    first_guard_decision(
        desktop_has_foreground,
        discord_foreground,
        osl_foreground,
        startup_grace_active,
    )
}

#[cfg(not(feature = "discord-qa-shell"))]
fn active_first_guard_decision(
    desktop_has_foreground: bool,
    discord_foreground: bool,
    osl_foreground: bool,
    startup_grace_active: bool,
) -> FirstGuardDecision {
    first_guard_decision(
        desktop_has_foreground,
        discord_foreground,
        osl_foreground,
        startup_grace_active,
    )
}

// `trusted_foreground` used to live here: the same
// `active_trusted_focus_state` question asked a second time, from the host's
// cached `target.foreground` snapshot instead of a live read, so that an
// already-open session could be ended when the operator switched away. Both it
// and its call site are gone with the foreign-focus hide in `start_guard`: the
// steady-state foreground is now read once per tick and reacted to nowhere, and
// nothing may take the protected composer off screen while a session is open.
// Opening still fails closed on an untrusted foreground through
// `active_first_guard_decision`.

#[cfg(target_os = "windows")]
fn exact_window_is_foreground(window: isize) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    let target = window as windows_sys::Win32::Foundation::HWND;
    if foreground.is_null() || target.is_null() {
        return false;
    }
    let foreground_root = unsafe { GetAncestor(foreground, GA_ROOT) };
    let target_root = unsafe { GetAncestor(target, GA_ROOT) };
    foreground == target
        || (!foreground_root.is_null() && !target_root.is_null() && foreground_root == target_root)
}

#[cfg(not(target_os = "windows"))]
fn exact_window_is_foreground(_window: isize) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn exact_window_rect_matches(window: isize, expected: [i32; 4]) -> bool {
    let hwnd = window as windows_sys::Win32::Foundation::HWND;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    !hwnd.is_null()
        && unsafe { GetWindowRect(hwnd, &mut rect) } != 0
        && [rect.left, rect.top, rect.right, rect.bottom] == expected
}

#[cfg(not(target_os = "windows"))]
fn exact_window_rect_matches(_window: isize, _expected: [i32; 4]) -> bool {
    false
}

fn should_reclaim_composer_focus(
    ready: bool,
    discord_foreground: bool,
    overlay_foreground: bool,
    cursor_inside_composer: bool,
) -> bool {
    ready && discord_foreground && !overlay_foreground && cursor_inside_composer
}

fn focus_reclaim_attempt_due(pending: bool, elapsed: Duration) -> bool {
    pending && elapsed >= Duration::from_millis(250)
}

/// While a session has never revealed a protected surface, the "full guard"
/// body below has nothing on screen to manage: it exists only to discover the
/// Discord window, refresh composer bounds (which drives Electron's
/// accessibility tree while holding the native host lock), and evaluate the
/// first-guard reveal decision. None of that is throttled by the steady-state
/// `last_full_guard` skip, which only applies once `ready` is true, so
/// without this bound it re-runs on every 16 ms tick for as long as the first
/// reveal keeps failing -- an unbounded retry that repeatedly takes a lock
/// shared with OSL's own window-focus handling. Bounding it to the same
/// cadence as the focus-reclaim retry above keeps that exposure rare instead
/// of constant while staying imperceptible to a legitimate first open.
const FIRST_OPEN_RETRY_INTERVAL: Duration = Duration::from_millis(250);

fn first_open_attempt_due(elapsed: Duration) -> bool {
    elapsed >= FIRST_OPEN_RETRY_INTERVAL
}

#[cfg(feature = "discord-qa-shell")]
fn active_should_reclaim_composer_focus(
    ready: bool,
    discord_foreground: bool,
    overlay_foreground: bool,
    cursor_inside_composer: bool,
) -> bool {
    should_reclaim_composer_focus(
        ready,
        discord_foreground,
        overlay_foreground,
        cursor_inside_composer,
    )
}

/// Production used to answer this `false` unconditionally, which is the second
/// half of "the protected composer does not receive input" -- and the half that
/// bites the real operator rather than an automated run.
///
/// Engaging the lock deliberately brings the borrowed Discord window forward
/// first (`apps/osl-hub-ui/src/main.ts`), so the composer is revealed while
/// Discord holds the keyboard. With no reclaim at all, production had nothing
/// that ever gave the caret back: the composer sat correctly on top and every
/// keystroke went into Discord's own message box in the clear. Being above
/// Discord and receiving Discord's keyboard focus are two different contracts,
/// and only the first one was ever asserted.
///
/// It is the same narrow predicate the QA build already used, and it is narrow
/// on purpose -- it cannot pull focus away from an operator who is doing
/// something else. All four must hold: the session is ready, the foreground
/// belongs to the borrowed window's own root, the composer does not already
/// have it, and the pointer is still inside the verified composer rectangle.
/// Clicking into Discord's channel list or its message history moves the
/// pointer out of that rectangle, so the operator can still use Discord
/// normally.
///
/// `discord_foreground` is deliberately *not* "Discord, and not some third
/// app". The guard supplies it from `exact_window_is_foreground`, which accepts
/// the exact HWND **or** a window sharing its `GA_ROOT` ancestor -- and the
/// borrowed window's root is the trusted OSL host that owns it, so this is true
/// for OSL's own main window as well. That is the behaviour this predicate
/// wants: the caret is reclaimed whenever the foreground is inside the OSL
/// composite and the pointer is over the protected rectangle, whichever half of
/// the composite took it. A genuinely foreign application is still excluded,
/// because its root is neither. The narrowing that keeps this safe is the
/// pointer test, not the identity of the window that holds the foreground.
#[cfg(not(feature = "discord-qa-shell"))]
fn active_should_reclaim_composer_focus(
    ready: bool,
    discord_foreground: bool,
    overlay_foreground: bool,
    cursor_inside_composer: bool,
) -> bool {
    should_reclaim_composer_focus(
        ready,
        discord_foreground,
        overlay_foreground,
        cursor_inside_composer,
    )
}

#[cfg(any(feature = "discord-qa-shell", test))]
fn qa_should_refresh_composer_bounds(
    ready: bool,
    osl_foreground: bool,
    discord_geometry_settled: bool,
    recovering_from_hidden: bool,
    carrier_in_flight: bool,
) -> bool {
    // While OSL is placing a carrier -- or calibrating for one -- the composer it
    // would re-measure is the composer it is typing into. Driving Electron's
    // accessibility tree there mid-gesture is what breaks `may_continue_input`
    // between the carrier and its Enter, and a composer OSL cannot reach to press
    // Enter is also one it cannot reach to clear, so the carrier strands and
    // every later protection attempt inherits it. Suppressed first and
    // unconditionally, matching `geometry_transition_forces_refresh` and
    // `periodic_backstop_refresh_due`; this predicate used to invert the bit and
    // was the only gate in the file that re-measured *because* a send was in
    // flight.
    if carrier_in_flight {
        return false;
    }
    if ready && osl_foreground && discord_geometry_settled && !recovering_from_hidden {
        return false;
    }
    !ready || !discord_geometry_settled || recovering_from_hidden
}

/// A host-window maximize, restore, move or resize moves Discord's composer in
/// screen space, and the cached composer rectangle is absolute. Re-measuring on
/// that transition is the only thing that keeps the protected surface derived
/// from the rectangle Discord occupies *now*; without it the surface is rebuilt
/// from the pre-transition bounds and only recovers whenever the slow blind
/// backstop happens to fire, which is how a maximize left the protected composer
/// off Discord's composer for seconds at a time.
///
/// Transition-scoped by construction, and never a cadence: it is false again on
/// the first pass after the guard has positioned against the rectangle it now
/// reads, `geometry_refresh_allowed` bounds how often a still-moving rectangle
/// may pay for a measurement, and a background OSL never measures at all
/// (driving Electron's accessibility tree from a background app wedges
/// Discord's UI thread while nothing of ours is even on screen).
fn geometry_transition_forces_refresh(
    osl_foreground: bool,
    discord_geometry_settled: bool,
    recovering_from_hidden: bool,
    carrier_in_flight: bool,
    geometry_refresh_allowed: bool,
) -> bool {
    osl_foreground
        && !carrier_in_flight
        && geometry_refresh_allowed
        && (!discord_geometry_settled || recovering_from_hidden)
}

#[cfg(feature = "discord-qa-shell")]
fn active_should_refresh_composer_bounds(
    ready: bool,
    osl_foreground: bool,
    discord_geometry_settled: bool,
    recovering_from_hidden: bool,
    carrier_in_flight: bool,
    geometry_refresh_allowed: bool,
    periodic_refresh_due: bool,
) -> bool {
    (geometry_refresh_allowed
        && qa_should_refresh_composer_bounds(
            ready,
            osl_foreground,
            discord_geometry_settled,
            recovering_from_hidden,
            carrier_in_flight,
        ))
        || geometry_transition_forces_refresh(
            osl_foreground,
            discord_geometry_settled,
            recovering_from_hidden,
            carrier_in_flight,
            geometry_refresh_allowed,
        )
        || periodic_backstop_refresh_due(osl_foreground, carrier_in_flight, periodic_refresh_due)
}

#[cfg(not(feature = "discord-qa-shell"))]
fn active_should_refresh_composer_bounds(
    _ready: bool,
    osl_foreground: bool,
    discord_geometry_settled: bool,
    recovering_from_hidden: bool,
    carrier_in_flight: bool,
    geometry_refresh_allowed: bool,
    periodic_refresh_due: bool,
) -> bool {
    geometry_transition_forces_refresh(
        osl_foreground,
        discord_geometry_settled,
        recovering_from_hidden,
        carrier_in_flight,
        geometry_refresh_allowed,
    ) || periodic_backstop_refresh_due(osl_foreground, carrier_in_flight, periodic_refresh_due)
}

/// Whether the exact composer rectangle may be re-measured on this pass at all.
///
/// Never while Discord's own window rectangle is still moving, and this gate is
/// applied *over* every other refresh trigger rather than inside one of them --
/// a drag must not be able to buy an accessibility walk through any door.
///
/// A measurement is a cross-process accessibility walk into Electron, and during
/// a drag it costs a walk per throttle interval -- five a second, for as long as
/// the operator holds the mouse down -- to learn something the guard already
/// knows. The composer travels with the window it lives in, so applying the
/// window's own translation to the cached rectangle (`host_rect_translation`,
/// just below) is not an approximation of the measurement: it is the same
/// answer, for free. Paying for the walk anyway is what saturates Discord's UI
/// thread, and a saturated Discord UI thread is what the operator feels as the
/// drag lagging.
///
/// Nothing is skipped, only deferred. `DISCORD_GEOMETRY_SETTLE` keeps the
/// rectangle "unsettled" for a full second after it stops moving, so the first
/// pass after the operator lets go still measures -- and that is the only pass
/// whose answer can differ from the translation, because it is the only one
/// where Discord has had a chance to relay its own layout out.
///
/// A session that is still opening is exempt. It has no cached rectangle of its
/// own to translate yet, and the cost this exists to avoid is a cost paid *per
/// tick for the length of a gesture* -- which is not a thing an open does. The
/// exemption is what keeps this a pure drag optimisation: it cannot defer, and
/// therefore cannot fail, the one pass that puts the composer on screen.
fn composer_measurement_allowed(ready: bool, discord_rect_moving: bool) -> bool {
    !ready || !discord_rect_moving
}

/// Re-measuring the exact composer drives Electron's accessibility tree, which
/// is expensive enough that a short backstop interval keeps Discord's UI thread
/// permanently saturated. Real geometry changes are already covered by the
/// trigger predicate above, so the blind backstop can be slow.
const COMPOSER_BACKSTOP_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// A host-window maximize or restore finishes in two stages: the Discord window
/// rectangle stops moving first, and Discord relays its own composer out
/// afterwards. Measuring once at stage one therefore latches the pre-transition
/// composer rectangle, and the guard then has no trigger left until the blind
/// backstop above -- measured symptom: after `SW_MAXIMIZE` the protected surface
/// no longer covered Discord's composer at all, while a plain move and a plain
/// resize (whose stage two is imperceptible) still tracked. The rectangle is
/// therefore only treated as settled once it has held still for this long.
const DISCORD_GEOMETRY_SETTLE: Duration = Duration::from_millis(1_000);

/// Smallest gap between two geometry-driven composer re-measurements. A live
/// drag-resize or a maximize animation moves the Discord rectangle on every
/// frame, and one accessibility walk per 16 ms tick is exactly what saturates
/// Discord's UI thread. The transition still resolves promptly because the
/// settle window above keeps re-measuring after the rectangle stops moving.
const GEOMETRY_REFRESH_MIN_INTERVAL: Duration = Duration::from_millis(200);

/// Colour-scheme changes move no rectangle at all, so nothing but a blind
/// re-sample can observe one. Everything else that used to justify a re-sample
/// -- a move, a drag, a reposition -- is answered without one now, so this is
/// the only remaining periodic capture and it stays deliberately slow.
const NATIVE_SURFACE_BACKSTOP_INTERVAL: Duration = Duration::from_secs(120);

/// How long a run of failed composer measurements has to last before it is read
/// as "this conversation is no longer on screen" rather than "one measurement
/// did not land".
///
/// The measurement is a cross-process accessibility walk into Electron that
/// shares probe pixels with OSL's own surface, so single failures are ordinary
/// and self-correcting. Treating the first one as terminal is what took the
/// composer off screen -- and then back on -- on nothing worse than a transient.
const COMPOSER_MEASUREMENT_GRACE: Duration = Duration::from_millis(1_000);

/// Consecutive failures required alongside the grace above. Both, never either:
/// a slow single failure is still a single failure, and a burst inside one
/// refresh interval is still a burst.
const COMPOSER_MEASUREMENT_GRACE_FAILURES: u32 = 3;

/// Whether a run of failed composer measurements has gone on long enough to
/// mean the conversation being protected is no longer the one on screen.
///
/// Below this, the last verified rectangle is still the best answer available
/// and the composer stays exactly where it is. Above it, the display genuinely
/// has nothing to be derived from any more.
fn composer_measurement_failure_ends_display(
    consecutive_failures: u32,
    since_first_failure: Duration,
) -> bool {
    consecutive_failures >= COMPOSER_MEASUREMENT_GRACE_FAILURES
        && since_first_failure >= COMPOSER_MEASUREMENT_GRACE
}

/// The screen-space delta between two host rectangles that differ **only** by
/// position.
///
/// `None` when the size changed, when nothing moved, or when the arithmetic
/// would overflow. A resize invalidates every cached measurement; a translation
/// invalidates none, because a window's contents do not move relative to the
/// window just because the window moved relative to the screen.
fn host_rect_translation(measured_against: [i32; 4], current: [i32; 4]) -> Option<(i32, i32)> {
    if measured_against == current {
        return None;
    }
    let dx = current[0].checked_sub(measured_against[0])?;
    let dy = current[1].checked_sub(measured_against[1])?;
    (current[2].checked_sub(measured_against[2])? == dx
        && current[3].checked_sub(measured_against[3])? == dy)
        .then_some((dx, dy))
}

/// Apply a host translation to a placement rectangle. Size never changes: a
/// translation is the whole of what a move does.
fn translated_overlay_rect(rect: OverlayRect, (dx, dy): (i32, i32)) -> Option<OverlayRect> {
    Some(OverlayRect {
        x: rect.x.checked_add(dx)?,
        y: rect.y.checked_add(dy)?,
        width: rect.width,
        height: rect.height,
    })
}

/// How far the borrowed window has moved since the rectangle a pass computed
/// against, measured at the instant of the write.
///
/// A guard pass reads Discord's rectangle once, at the top, and then does real
/// work -- a host reconcile, a scope read, a row read -- before it reaches the
/// placement. Under a drag that opening read is already stale by the time the
/// write is issued, so the pair was being placed where Discord *was*. The next
/// pass then placed it somewhere else that was also already stale, and the
/// composer trailed the gesture instead of following it: two writers, one of
/// them always behind, which is exactly the shape of a double-move.
///
/// `None` means there is nothing to correct -- the window has not moved, could
/// not be read, or did not merely translate (a resize is a different question
/// and is answered by re-deriving the surface, not by shifting it).
#[cfg(target_os = "windows")]
fn host_translation_since(
    discord_window: isize,
    computed_against: [i32; 4],
) -> Option<((i32, i32), [i32; 4])> {
    let hwnd = discord_window as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return None;
    }
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let live = [rect.left, rect.top, rect.right, rect.bottom];
    host_rect_translation(computed_against, live).map(|delta| (delta, live))
}

#[cfg(not(target_os = "windows"))]
fn host_translation_since(
    _discord_window: isize,
    _computed_against: [i32; 4],
) -> Option<((i32, i32), [i32; 4])> {
    None
}

/// Apply a host translation to the painted row rectangles.
///
/// They are absolute screen rectangles measured against one position of the
/// borrowed window, exactly like the composer rectangle, and they decide two
/// things that must travel with it: where the opaque shield goes, and which part
/// of the shield exists at all. Placing a translated composer against untranslated
/// rows is what let the pair be seen apart mid-drag -- the composer landed on
/// Discord's new position and the shield stayed on the old one -- which is the one
/// thing the single `DeferWindowPos` batch exists to make impossible.
///
/// All or nothing: a row that cannot be translated without overflowing would
/// otherwise clip the shield to a region the guard never derived.
fn translated_painted_rows(painted: &[[i32; 4]], (dx, dy): (i32, i32)) -> Option<Vec<[i32; 4]>> {
    painted
        .iter()
        .map(|rect| {
            Some([
                rect[0].checked_add(dx)?,
                rect[1].checked_add(dy)?,
                rect[2].checked_add(dx)?,
                rect[3].checked_add(dy)?,
            ])
        })
        .collect()
}

/// The complete answer to "the borrowed window moved": where it is now, where the
/// protected surface goes, and where the rows the shield covers went.
///
/// `None` is "this is not a move" -- the window did not shift, could not be read,
/// or changed size, all of which are questions only a full pass can answer.
fn translated_drag_placement(
    discord_window: isize,
    computed_against: [i32; 4],
    placed: Option<OverlayRect>,
    painted: &[[i32; 4]],
) -> Option<([i32; 4], OverlayRect, Vec<[i32; 4]>)> {
    let placed = placed?;
    let (delta, live) = host_translation_since(discord_window, computed_against)?;
    let moved = translated_overlay_rect(placed, delta)?;
    let rows = translated_painted_rows(painted, delta)?;
    Some((live, moved, rows))
}

/// Move the pair to a rectangle a translation derived, having first re-proved that
/// both windows are still this process's own and still owned by the trusted parent.
///
/// The ownership proof is not optional just because the write is cheap: a
/// translation is still a `SetWindowPos` on two windows, and the full pass makes
/// exactly this proof (`verify_owned_overlay_pair`) before its own placement. Both
/// reads are local style queries on the cached handles.
///
/// There is still one writer of this pair's geometry, and it is still one
/// `DeferWindowPos` batch per write: the guard body keeps its single
/// `position_window_pair` call site and the drag path reaches the same batch
/// through here.
fn place_moved_protected_pair(
    app: &tauri::AppHandle,
    trusted_parent: isize,
    rect: OverlayRect,
    painted: &[[i32; 4]],
) -> Result<(), String> {
    let window = app
        .get_webview_window(OVERLAY_LABEL)
        .ok_or_else(|| "The native Discord overlay closed".to_owned())?;
    let shield = app
        .get_webview_window(SHIELD_LABEL)
        .ok_or_else(|| "The OSL capture shield closed".to_owned())?;
    verify_owned_overlay_pair(&window, &shield, trusted_parent)?;
    position_window_pair(&window, &shield, rect, painted)
}

/// How long the guard may answer from what its last complete pass proved.
///
/// The same budget for both cheap paths, deliberately: the steady one skips the
/// pass because nothing moved, the drag one because only the position did, and
/// neither may let a session go longer than this without re-proving its identity,
/// its context, its ownership and its geometry.
const PROTECTED_FULL_GUARD_BUDGET: Duration = Duration::from_millis(400);

/// Apply a host translation to a cached absolute rectangle.
fn translated_bounds(
    bounds: AccessibilityBounds,
    (dx, dy): (i32, i32),
) -> Option<AccessibilityBounds> {
    Some(AccessibilityBounds {
        left: bounds.left.checked_add(dx)?,
        top: bounds.top.checked_add(dy)?,
        right: bounds.right.checked_add(dx)?,
        bottom: bounds.bottom.checked_add(dy)?,
    })
}

/// Everything the sampled native background actually depends on: the pixel
/// dimensions of the captured strip, and of the input rectangle inside it.
///
/// Deliberately position-free. Discord does not repaint its composer because
/// its window moved, and the capture is consumed as an image plus relative
/// insets, so a pure translation can keep the sample it already has. That is
/// what lets a drag stop costing a capture -- and therefore stop costing the
/// hide the capture used to need.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeSurfaceShape {
    width: i32,
    height: i32,
    input_left: i32,
    input_top: i32,
    input_width: i32,
    input_height: i32,
}

fn native_surface_shape(
    outer: AccessibilityBounds,
    input: AccessibilityBounds,
) -> Option<NativeSurfaceShape> {
    let width = outer.right.checked_sub(outer.left)?;
    let height = outer.bottom.checked_sub(outer.top)?;
    let input_width = input.right.checked_sub(input.left)?;
    let input_height = input.bottom.checked_sub(input.top)?;
    (width > 0 && height > 0 && input_width > 0 && input_height > 0).then_some(NativeSurfaceShape {
        width,
        height,
        input_left: input.left.checked_sub(outer.left)?,
        input_top: input.top.checked_sub(outer.top)?,
        input_width,
        input_height,
    })
}

/// Whether the sampled native background has to be taken again.
///
/// Was "any geometry change at all", which made a drag one capture per frame and
/// -- because a capture used to require the protected pair to leave the screen
/// first -- made a drag one hide/reveal per frame. That is the reported
/// disappearing composer, and the several hundred milliseconds each of those
/// cycles blocks the guard thread for is the reported drag lag.
///
/// The three real reasons, and nothing else:
/// * there is no sample yet, so there is nothing to paint;
/// * the strip's pixel dimensions changed, which no cached image can answer;
/// * the slow blind backstop, which is the only way a colour-only theme change
///   can ever be observed.
///
/// The shape trigger additionally waits for the rectangle to settle: a live
/// resize changes the shape on every frame, and re-sampling per frame is the
/// same defect in a different gesture. The sample is briefly the wrong width
/// during the gesture, which the renderer scales; it is exact again one settle
/// later.
fn native_surface_resample_required(
    has_sample: bool,
    shape_changed: bool,
    geometry_settled: bool,
    backstop_due: bool,
) -> bool {
    !has_sample || backstop_due || (shape_changed && geometry_settled)
}

/// Whether the protected pair has to leave the screen before the sampler can see
/// Discord's own pixels.
///
/// The sampler is a `BitBlt` of the desktop DC over the composer's own
/// rectangle, so anything of OSL's that is *in* that capture would be fed back
/// as the background. Two things can be, and both are answered by measurement
/// rather than assumption:
///
/// * the composer itself, unless it is excluded from capture. Production sets
///   `WDA_EXCLUDEFROMCAPTURE` on this exact HWND, whose entire purpose is that
///   the window is removed from screen captures and the pixels behind it are
///   what a capture returns -- so production does not have to hide anything.
///   The QA shell deliberately runs with capture protection off so a test
///   harness can screenshot it, and there the hide is still the only answer.
/// * the opaque capture shield, which is normally nowhere near the composer
///   strip -- it only ever covers painted message rows above it -- but is
///   checked rather than assumed, because it is opaque black and would poison
///   the sample outright.
///
/// Fails safe in both directions: an affinity that cannot be read, or a shield
/// that does overlap, hides exactly as before.
fn sampling_requires_the_pair_to_leave_the_screen(
    composer_excluded_from_capture: bool,
    shield_overlaps_sample: bool,
) -> bool {
    !composer_excluded_from_capture || shield_overlaps_sample
}

/// Whether a capture that needs the protected pair off screen is allowed to put
/// it there.
///
/// Only when it is already off screen -- either because this session has never
/// revealed it, or because it is currently down for a reason the guard already
/// owns (a minimized owner, a recovery in progress). There is no third case, and
/// deliberately so: the owner's rule for this surface is that it is *active 100%
/// of the time* and *does not flash*, and "hide, BitBlt, reveal" violates both
/// every time it runs.
///
/// What the caller does instead is nothing: it keeps the sample it already has.
/// That is not a degraded mode. The sample is Discord's composer pixels, and
/// Discord does not repaint its composer because a window moved -- the only
/// thing that can invalidate the sample without changing its pixel dimensions is
/// a colour-scheme change, which is a colour being one backstop interval late.
/// Weighed against a composer that blinks out from under the operator's cursor,
/// the stale colour wins every time.
///
/// This is what makes a drag safe on a build whose capture protection is off.
/// With it on, `sampling_requires_the_pair_to_leave_the_screen` already answers
/// `false` and this is never consulted; with it off -- the QA shell, or an
/// operator who turned it off -- this is the only thing standing between a
/// re-sample and the reported disappearing composer.
/// Whether this pass owes the protected pair a placement before it may reveal
/// it.
///
/// The third arm is the one that makes "appears exactly once, already in the
/// right place" a property of the code. `last_overlay_rect` and
/// `last_geometry_key` are seeded from the calibration the session opened with,
/// so a composer that has not moved since calibration reaches its very first
/// reveal with `geometry_changed == false` -- and, with nothing else placing it,
/// becomes visible at whatever rectangle the retained window last held: the
/// 640x400 pre-warm rectangle in the screen's top-left corner on a cold start,
/// or the previous conversation's composer rectangle afterwards.
///
/// A placement is one deferred `SetWindowPos` batch, so paying for it on every
/// pass until one reveal has succeeded costs nothing measurable and removes the
/// whole class of "revealed before positioned" from the open path.
fn protected_placement_required(
    geometry_changed: bool,
    resampled_native_surface: bool,
    ever_revealed: bool,
) -> bool {
    geometry_changed || resampled_native_surface || !ever_revealed
}

fn resample_may_take_the_pair_off_screen(ever_revealed: bool, currently_hidden: bool) -> bool {
    !ever_revealed || currently_hidden
}

/// Whether Windows itself reports that this exact HWND is removed from screen
/// captures. Read, never assumed: an unreadable affinity, or any value other
/// than the exclusion one, answers "no" and the caller hides as before.
#[cfg(target_os = "windows")]
fn composer_is_excluded_from_capture(window: &tauri::WebviewWindow) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };

    // Cached handle, never a Tauri getter: `hwnd()` is a blocking round trip
    // through the event-loop FIFO, and this runs inside a guard pass.
    let Some(handle) = cached_window_hwnd(window) else {
        return false;
    };
    let hwnd = handle as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return false;
    }
    let mut affinity: u32 = 0;
    // The comparison lives inside the `unsafe` block on purpose: at statement
    // position `unsafe { .. } != 0` parses the block as a statement and orphans
    // the `!=`, which is a parse error rather than a type error.
    let read = unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) != 0 };
    read && affinity == WDA_EXCLUDEFROMCAPTURE
}

#[cfg(not(target_os = "windows"))]
fn composer_is_excluded_from_capture(_window: &tauri::WebviewWindow) -> bool {
    false
}

/// Whether the opaque capture shield covers any pixel the sampler is about to
/// read. `painted` is empty whenever the eye is closed, and the shield is off
/// screen entirely in that case.
fn shield_overlaps_sampled_surface(painted: &[[i32; 4]], sample: AccessibilityBounds) -> bool {
    let Some(shield) = painted_rows_bounds(painted) else {
        return false;
    };
    let (Ok(width), Ok(height)) = (i32::try_from(shield.width), i32::try_from(shield.height))
    else {
        return true;
    };
    let (Some(right), Some(bottom)) = (shield.x.checked_add(width), shield.y.checked_add(height))
    else {
        return true;
    };
    shield.x < sample.right
        && right > sample.left
        && shield.y < sample.bottom
        && bottom > sample.top
}

#[cfg(feature = "discord-qa-shell")]
fn qa_record_composer_refresh_cost(elapsed: Duration, failure: Option<&str>) {
    use std::fmt::Write as _;

    // Every value written here is a fixed adapter error string. Composer text
    // never reaches this path, so no draft content can be recorded.
    let path = std::env::temp_dir().join("osl-discord-qa-composer-refresh-cost.txt");
    let mut line = String::new();
    let _ = writeln!(
        line,
        "{}ms {}",
        elapsed.as_millis().min(u128::from(u32::MAX)),
        match failure {
            None => "ok".to_owned(),
            Some(error) => error.chars().take(120).collect::<String>(),
        }
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write as _;
        let _ = file.write_all(line.as_bytes());
    }
}

/// The exact composer can move without the Discord window rectangle changing
/// (sidebar, member list, or thread panel), so a slow backstop re-measurement
/// stays necessary. It must never run while OSL is in the background: the
/// protected composer is hidden then, and driving Electron's accessibility
/// tree from a background app wedges Discord's UI thread for no benefit.
fn periodic_backstop_refresh_due(
    osl_foreground: bool,
    carrier_in_flight: bool,
    periodic_refresh_due: bool,
) -> bool {
    osl_foreground && !carrier_in_flight && periodic_refresh_due
}

#[cfg(target_os = "windows")]
fn cursor_is_inside_overlay_rect(rect: OverlayRect) -> bool {
    let mut cursor = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return false;
    }
    point_is_inside_overlay_rect(cursor.x, cursor.y, rect)
}

#[cfg(not(target_os = "windows"))]
fn cursor_is_inside_overlay_rect(_rect: OverlayRect) -> bool {
    false
}

fn point_is_inside_overlay_rect(x: i32, y: i32, rect: OverlayRect) -> bool {
    let Ok(width) = i32::try_from(rect.width) else {
        return false;
    };
    let Ok(height) = i32::try_from(rect.height) else {
        return false;
    };
    let Some(right) = rect.x.checked_add(width) else {
        return false;
    };
    let Some(bottom) = rect.y.checked_add(height) else {
        return false;
    };
    x >= rect.x && x < right && y >= rect.y && y < bottom
}

#[cfg(target_os = "windows")]
fn overlay_window_is_foreground(app: &tauri::AppHandle) -> bool {
    cached_label_hwnd(app, OVERLAY_LABEL).is_some_and(exact_window_is_foreground)
}

/// Whether the trusted OSL owner is minimized, read from the window.
///
/// Windows hides owned popups with their owner, so this is the guard's own
/// bookkeeping question and it is asked on every 16 ms tick. `is_minimized()` is
/// a blocking event-loop round trip for the answer `IsIconic` gives locally --
/// and asking it on every tick means asking it repeatedly while the operator's
/// drag owns that thread, which is the one time the question is both cheapest to
/// answer and most expensive to ask.
#[cfg(target_os = "windows")]
fn owner_window_is_minimized(main: &tauri::WebviewWindow) -> Option<bool> {
    let hwnd = cached_window_hwnd(main)?;
    Some(unsafe { IsIconic(hwnd as HWND) } != 0)
}

#[cfg(not(target_os = "windows"))]
fn owner_window_is_minimized(main: &tauri::WebviewWindow) -> Option<bool> {
    main.is_minimized().ok()
}

/// How much of Discord this pass may cover.
///
/// Three states, because the lock and the eye control different things and the
/// composer and the painted rows are the same window. Anything less than three
/// makes one control speak for the other.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ProtectedSurfacePresence {
    /// Nothing on screen. Neither window is shown and nothing is painted.
    OffScreen,
    /// The composer band, extended upward over whatever rows the eye is
    /// painting. The lock is engaged, so OSL owns Discord's message box.
    ComposerAndRows,
    /// The painted rows only, with the composer band surrendered outright.
    /// The lock is down but the eye is painting, so OSL owns the rows it is
    /// displaying over and Discord owns its own message box.
    RowsOnly,
}

/// What the protected pair may cover this pass.
///
/// Presence is the one thing about this surface the operator states directly, so
/// it answers to what they asked for and to nothing else -- and they ask with two
/// separate controls:
///
/// * **minimized owner.** There is nothing behind the composer and nothing it is
///   protecting, so a composer still floating over the desktop is orphaned rather
///   than active. Nothing may be on screen.
/// * **the lock.** Encryption only. On, OSL owns Discord's message box, because
///   the operator's plaintext may never enter it. Off, their keystrokes are
///   *meant* to reach that box in the clear, and OSL standing on top of it is the
///   configuration that has already put plaintext into a real conversation.
/// * **the eye.** The only control over display. Painting rows keeps the surface
///   on screen over exactly those rows, whatever the lock says.
///
/// The lock's half was the owner-reported defect and it was missing outright:
/// nothing anywhere read the lock, so the composer stayed on Discord's message
/// box for the rest of the session with the hub rendering "off". The first fix
/// for it took the whole pair off screen, which then made the *lock* decide
/// whether the eye could display anything -- "the lock changes nothing about
/// display" is the rule it broke. `RowsOnly` is what makes both true at once.
///
/// The engaged arm is just as load-bearing and unchanged. With the lock up this
/// answers `ComposerAndRows` for every other reason a pass might want to hide --
/// lost foreground, occlusion, a drag, a pending sample -- because the composer is
/// then "active 100% of the time and does not flash". All three inputs are stable
/// rather than sampled: `IsIconic` is not true of a window being dragged,
/// `lock_engaged` is written twice per session (`activate` raises it,
/// `disengage_lock` and `clear` lower it), and `rows_painted` is the fact the last
/// full pass published, so none of them can flicker under the guard.
///
/// Geometry stays lock-free either way. This decides *which* rectangle the pass
/// asks for; `protected_surface_rect` and `protected_rows_band_rect` derive both
/// of them from a measurement and neither reads the lock.
fn protected_surface_presence(
    owner_minimized: bool,
    lock_engaged: bool,
    rows_painted: bool,
) -> ProtectedSurfacePresence {
    if owner_minimized {
        return ProtectedSurfacePresence::OffScreen;
    }
    if lock_engaged {
        return ProtectedSurfacePresence::ComposerAndRows;
    }
    if rows_painted {
        return ProtectedSurfacePresence::RowsOnly;
    }
    ProtectedSurfacePresence::OffScreen
}

/// The rectangle a presence answer asks for, and the only place the two geometry
/// functions are chosen between.
///
/// One mapping, so the surface the pass measures against and the surface it
/// actually writes cannot be derived by different rules -- the bug that put the
/// composer at the live rectangle and the shield at the stale one was exactly two
/// derivations of one answer.
fn protected_presence_surface_rect(
    presence: ProtectedSurfacePresence,
    discord: [i32; 4],
    composer: Option<AccessibilityBounds>,
    painted: &[[i32; 4]],
) -> Option<OverlayRect> {
    match presence {
        ProtectedSurfacePresence::OffScreen => None,
        ProtectedSurfacePresence::ComposerAndRows => {
            protected_surface_rect(discord, composer, painted)
        }
        ProtectedSurfacePresence::RowsOnly => protected_rows_band_rect(discord, composer, painted),
    }
}

/// Whether both retained protected windows are provably off screen right now.
///
/// Read off the cached handles with `IsWindowVisible`, so this is a local Win32
/// read on the calling thread and never a blocking event-loop round trip -- the
/// same reason `owner_window_is_minimized` exists in this form.
///
/// This is what turns the guard's transient hides from bookkeeping into a fact.
/// `hide_window` only *asks* Tauri to hide, on the event loop, and returns
/// immediately; recording "the pair is hidden" on the strength of that request
/// is exactly the failure the minimize fix was written for -- the bookkeeping
/// described a state that was not true, and because the bookkeeping also
/// suppresses the retry, nothing ever asked again.
#[cfg(target_os = "windows")]
fn protected_pair_is_off_screen(app: &tauri::AppHandle) -> bool {
    [OVERLAY_LABEL, SHIELD_LABEL].iter().all(|label| {
        // A window that no longer has a live handle cannot be on screen.
        cached_label_hwnd(app, label)
            .is_none_or(|hwnd| unsafe { IsWindowVisible(hwnd as HWND) } == 0)
    })
}

#[cfg(not(target_os = "windows"))]
fn protected_pair_is_off_screen(app: &tauri::AppHandle) -> bool {
    [OVERLAY_LABEL, SHIELD_LABEL].iter().all(|label| {
        app.get_webview_window(label)
            .is_none_or(|window| window.is_visible().is_ok_and(|visible| !visible))
    })
}

/// Ask for the pair to leave the screen, and answer whether it actually has.
///
/// One behaviour, two callers, because the presence rule is asked before the pass
/// has measured anything and there is one state it therefore cannot see: a
/// surrendered composer band with no painted row left above Discord's message
/// box, which is only discoverable once the rows have been read. Both callers
/// must hide the same way and, critically, must record the same *read* rather
/// than the request -- `hide_window` only asks Tauri, on the event loop, and
/// returns before anything has moved, and latching the flag on the strength of
/// that request is what once left a composer on the desktop over a minimized
/// owner with every retry suppressed by bookkeeping that was simply wrong.
fn leave_the_screen(app: &tauri::AppHandle, already_asked: bool) -> bool {
    if !already_asked {
        hide_window(app);
    }
    protected_pair_is_off_screen(app)
}

/// The composer's current scale, read from the window instead of from Tauri.
///
/// `scale_factor()` is a blocking event-loop round trip for a number tao itself
/// derives from `GetDpiForWindow` and caches at `WM_DPICHANGED`. The guard asks
/// for it up to three times per pass -- the read-only fast path, the geometry
/// key, and the placement -- so on a drag it was three of the seven waits that
/// made a pass unable to answer a move. Reading the DPI off the handle is the
/// same answer from the same source, on this thread.
#[cfg(target_os = "windows")]
fn current_overlay_scale_milli(app: &tauri::AppHandle) -> Option<u32> {
    use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;

    let hwnd = cached_label_hwnd(app, OVERLAY_LABEL)?;
    let dpi = unsafe { GetDpiForWindow(hwnd as HWND) };
    if dpi == 0 {
        return None;
    }
    bounded_scale_milli(f64::from(dpi) / 96.0)
}

#[cfg(not(target_os = "windows"))]
fn current_overlay_scale_milli(app: &tauri::AppHandle) -> Option<u32> {
    app.get_webview_window(OVERLAY_LABEL)
        .and_then(|window| window.scale_factor().ok())
        .and_then(bounded_scale_milli)
}

#[cfg(not(target_os = "windows"))]
fn overlay_window_is_foreground(_app: &tauri::AppHandle) -> bool {
    false
}

fn refresh_adaptive_native_surface(
    app: &tauri::AppHandle,
    owner: &str,
    epoch: u64,
    expected_target: NativeDiscordOverlayTarget,
) -> Result<(), String> {
    let composer = app.state::<NativeDiscordComposerState>();
    let (outer, input) = composer
        .verified_surface_bounds()
        .ok_or_else(|| "The verified native composer surface is unavailable".to_owned())?;
    let capture = crate::native_surface_capture::capture_verified_surface_guarded(
        [outer.left, outer.top, outer.right, outer.bottom],
        [input.left, input.top, input.right, input.bottom],
        composer.verified_text_presentation(),
        || {
            app.state::<NativeWindowHostState>()
                .discord_overlay_target(owner)
                .is_ok_and(|target| same_native_surface_target(target, expected_target))
                && composer.verified_surface_bounds() == Some((outer, input))
        },
    )?;
    let presentation = capture
        .presentation_bounds([outer.left, outer.top, outer.right, outer.bottom])
        .ok_or_else(|| "The adaptive native composer geometry is invalid".to_owned())?;
    composer.apply_adaptive_presentation_bounds(
        outer,
        AccessibilityBounds {
            left: presentation[0],
            top: presentation[1],
            right: presentation[2],
            bottom: presentation[3],
        },
    )?;
    app.state::<crate::native_surface_capture::NativeSurfaceCaptureState>()
        .replace(
            crate::native_surface_capture::NativeSurfaceKey {
                session_epoch: epoch,
                host_generation: expected_target.generation,
            },
            capture,
        )?;
    let _ = app.emit_to(OVERLAY_LABEL, NATIVE_SURFACE_CHANGED_EVENT, ());
    Ok(())
}

#[cfg(feature = "discord-qa-shell")]
fn active_overlay_requires_focus_acquisition() -> bool {
    desktop_has_foreground_window()
}

#[cfg(not(feature = "discord-qa-shell"))]
fn active_overlay_requires_focus_acquisition() -> bool {
    true
}

/// Borrow the foreground thread's input queue for the length of one
/// `SetForegroundWindow` call, and give it back on every path out.
///
/// Windows refuses a foreground change from a process that neither owns the
/// foreground nor received the last input event. The operator's reported gesture
/// -- caret in Discord's own message box, then engage -- guarantees exactly that
/// state, so the call was refused, the composer never took focus, and every
/// keystroke after it went into Discord in the clear.
///
/// Sharing the input queue is the documented escape and the only one this file
/// is allowed to use. `keybd_event`, `SendInput` and `SetCursorPos` would all
/// "work" and are all forbidden: they synthesize input the operator did not
/// give, and the cursor must never move.
///
/// The attachment is held on the guard thread, never on OSL's UI thread, and
/// only across the single call below -- OSL's own message loop is never coupled
/// to Discord's.
#[cfg(target_os = "windows")]
struct ForegroundInputAttachment {
    foreground_thread: u32,
    this_thread: u32,
}

#[cfg(target_os = "windows")]
impl ForegroundInputAttachment {
    /// `None` when there is nothing to attach to, or when the attach itself was
    /// refused. Both mean "carry on unattached": the caller still issues the
    /// call and still proves the outcome by reading the foreground back, so a
    /// missing attachment can only cost an attempt, never correctness.
    fn acquire() -> Option<Self> {
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() {
            return None;
        }
        let mut process_id = 0u32;
        let foreground_thread = unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
        let this_thread = unsafe { GetCurrentThreadId() };
        if foreground_thread == 0 || foreground_thread == this_thread {
            return None;
        }
        (unsafe { AttachThreadInput(foreground_thread, this_thread, 1) } != 0).then_some(Self {
            foreground_thread,
            this_thread,
        })
    }
}

#[cfg(target_os = "windows")]
impl Drop for ForegroundInputAttachment {
    /// The detach lives here and only here. An early return, a `?`, or a panic
    /// unwinding this thread all run it; a hand-written detach after the call
    /// would not, and a leaked attachment permanently couples OSL's input queue
    /// to Discord's.
    fn drop(&mut self) {
        unsafe { AttachThreadInput(self.foreground_thread, self.this_thread, 0) };
    }
}

/// Whether the window rooted at `expected_root` is the foreground window right
/// now. Read back from Windows rather than inferred from any call's return
/// value: `SetForegroundWindow` answering non-zero is not the same fact.
#[cfg(target_os = "windows")]
fn overlay_root_is_foreground(expected_root: windows_sys::Win32::Foundation::HWND) -> bool {
    !expected_root.is_null()
        && unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT) } == expected_root
}

/// How many times focus acquisition may ask, and how long it waits between
/// asks. The total is the same bounded ~200 ms budget this path has always had.
#[cfg(target_os = "windows")]
const FOCUS_ACQUISITION_ATTEMPTS: u32 = 10;
#[cfg(target_os = "windows")]
const FOCUS_ACQUISITION_RETRY: Duration = Duration::from_millis(20);

/// Make a refused keyboard focus observable instead of silent.
///
/// The composer stays exactly where it is -- visible, owned, positioned, above
/// Discord. What changes is that the hub is told, on the edges only, that OSL is
/// not the window receiving keystrokes right now, so "I am typing into OSL" can
/// never be a belief the operator holds with nothing to correct it.
///
/// Announced on the same event as the z-order surrender because it is the same
/// contract: the protected composer is on screen and unreachable for input. The
/// payload now says *which* of the two conditions spoke, so the reader no longer
/// has to infer it. No token, no identity, no content.
fn report_protected_focus_refused(app: &tauri::AppHandle, refused: bool) {
    let state = app.state::<OverlaySessionState>();
    let was_unreachable = state.composer_is_unreachable();
    if !state.set_protected_focus_refused(refused) {
        return;
    }
    #[cfg(feature = "discord-qa-shell")]
    qa_overlay_window_stage(if refused {
        "focus_refused_composer_unreachable"
    } else {
        "focus_reacquired"
    });
    publish_composer_unreachable(app, COMPOSER_UNREACHABLE_KEYBOARD_FOCUS, was_unreachable);
}

#[cfg(target_os = "windows")]
fn active_focus_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    let hwnd = window
        .hwnd()
        .map_err(|_| "The native Discord overlay input window is unavailable".to_owned())?
        .0 as windows_sys::Win32::Foundation::HWND;
    let expected_root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    if hwnd.is_null() || expected_root.is_null() {
        return Err("The native Discord overlay input window is unavailable".to_owned());
    }
    // Tauri first, exactly as before: it marshals to the thread that owns the
    // window, which is the only place the WebView's own focus can be set. It is
    // no longer fatal on its own, because it is not the fact being proven --
    // the foreground read-back below is.
    let _ = window.set_focus();
    for attempt in 0..FOCUS_ACQUISITION_ATTEMPTS {
        if overlay_root_is_foreground(expected_root) {
            return Ok(());
        }
        if attempt == 0 {
            // The common case: OSL already owns the foreground, or received the
            // last input event, and is permitted to do this unaided. Nothing is
            // attached to anything.
            unsafe { SetForegroundWindow(hwnd) };
        } else {
            // The refused case. The attachment exists only for this statement
            // and is released before the wait below, so the queues are shared
            // for microseconds rather than for the length of the retry loop.
            let _attachment = ForegroundInputAttachment::acquire();
            unsafe { SetForegroundWindow(hwnd) };
        }
        std::thread::sleep(FOCUS_ACQUISITION_RETRY);
    }
    if overlay_root_is_foreground(expected_root) {
        return Ok(());
    }
    Err("The native Discord overlay input focus could not be verified".to_owned())
}

#[cfg(not(target_os = "windows"))]
fn active_focus_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    window
        .set_focus()
        .map_err(|_| "The native Discord overlay could not receive trusted input focus".to_owned())
}

#[cfg(target_os = "windows")]
fn osl_process_is_foreground(app: &tauri::AppHandle) -> bool {
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.is_null() {
        return false;
    }
    let foreground_root = unsafe { GetAncestor(foreground, GA_ROOT) };
    for label in ["main", OVERLAY_LABEL] {
        let Some(hwnd) = cached_label_hwnd(app, label) else {
            continue;
        };
        let osl_root = unsafe { GetAncestor(hwnd as HWND, GA_ROOT) };
        if !foreground_root.is_null() && foreground_root == osl_root {
            return true;
        }
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
    process_id == std::process::id()
}

#[cfg(not(target_os = "windows"))]
fn osl_process_is_foreground(app: &tauri::AppHandle) -> bool {
    app.get_webview_window(OVERLAY_LABEL)
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
        || app
            .get_webview_window("main")
            .and_then(|window| window.is_focused().ok())
            .unwrap_or(false)
}

// `trusted_focus_state` / `active_trusted_focus_state` used to live here: the
// steady-state answer to "does a trusted window hold the foreground". Both are
// gone with the last thing that could act on the answer. The guard's remaining
// foreground questions are `first_guard_decision` (may this session open at
// all), `should_reclaim_composer_focus` (has Discord taken the caret back) and
// the plain `osl_process_is_foreground` gate on the eye refresh and the surface
// backstop -- each of which reads the foreground for a reason and none of which
// runs on the 16 ms path.

#[cfg(target_os = "windows")]
fn desktop_has_foreground_window() -> bool {
    // GetForegroundWindow legitimately returns NULL while an RDP desktop is
    // minimized/disconnected. That is not evidence that a foreign app took
    // focus, so the protected OSL window may open or remain open there.
    // As soon as the desktop has a foreground window again, the normal exact
    // Discord/OSL focus checks above resume and fail closed on foreign focus.
    !unsafe { GetForegroundWindow() }.is_null()
}

#[cfg(not(target_os = "windows"))]
fn desktop_has_foreground_window() -> bool {
    true
}

fn start_guard(
    app: tauri::AppHandle,
    epoch: u64,
    mut last_rect: [i32; 4],
    discord_window: isize,
    trusted_parent: isize,
) -> Result<(), String> {
    let first_guard_deadline = Instant::now() + FIRST_GUARD_GRACE;
    let composer_state = app.state::<NativeDiscordComposerState>();
    let initial_presentation_bounds = composer_state.verified_composer_bounds();
    let mut last_overlay_rect = initial_presentation_bounds
        .and_then(|bounds| active_overlay_rect_with_composer(last_rect, Some(bounds), None));
    let mut last_geometry_key = composer_state
        .verified_surface_bounds()
        .zip(initial_presentation_bounds)
        .and_then(|((surface, input), presentation)| {
            adaptive_geometry_key(
                last_rect,
                surface,
                input,
                presentation,
                active_overlay_rect_with_composer(last_rect, Some(presentation), None)?,
                current_overlay_scale_milli(&app)?,
            )
        });
    // Backdated by exactly the geometry-refresh floor so the first pass of a new
    // session is never throttled by it, while every later geometry-driven
    // measurement still has to wait out that floor.
    let mut last_composer_refresh = Instant::now()
        .checked_sub(GEOMETRY_REFRESH_MIN_INTERVAL)
        .unwrap_or_else(Instant::now);
    // When the Discord rectangle last differed from the one the guard has
    // already positioned against. Backdated so a session that opens over an
    // already-still Discord window counts as settled on its first pass and pays
    // for no extra measurement at all.
    let mut last_discord_rect_change = Instant::now()
        .checked_sub(DISCORD_GEOMETRY_SETTLE)
        .unwrap_or_else(Instant::now);
    // A Discord theme change (Nitro colour scheme, light/dark, high contrast)
    // repaints the composer without moving a single rectangle, so no geometry
    // trigger can ever observe it. Re-sampling the native surface on the same
    // foreground-gated backstop cadence is the only signal that catches a pure
    // colour change. The re-sample is a bounded BitBlt of the already-verified
    // composer rectangle; it never touches Discord's accessibility tree.
    let mut last_native_surface_refresh = Instant::now();
    let mut last_full_guard = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let mut last_focus_reclaim_attempt = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    // Backdated so the very first tick is always allowed to attempt the
    // first open immediately; only the retries after a failure are throttled.
    let mut last_first_open_attempt = Instant::now()
        .checked_sub(FIRST_OPEN_RETRY_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut composer_temporarily_hidden = false;
    // Backdated so the eye is resolved on the very first pass of a session
    // instead of being assumed, and only re-read on a slow, foreground-gated
    // cadence after that.
    let mut last_protected_display_refresh = Instant::now()
        .checked_sub(PROTECTED_DISPLAY_REFRESH_INTERVAL)
        .unwrap_or_else(Instant::now);
    // Whether this session has ever actually shown the protected pair. Once
    // true, the full guard body must keep running every tick as it always
    // has -- this only bounds the retry cadence for a surface that has never
    // been visible, never the responsiveness of one that is.
    let mut ever_revealed = false;
    // When the bounded z-order read below last ran. The read itself is cheap,
    // but it must not run on every 16 ms tick, and the correction it can trigger
    // must not run more often than that read.
    let mut last_stack_probe = Instant::now();
    // Whether this session has a sampled native background to paint. Cheap to
    // keep here: only the passes that would otherwise skip re-sampling consult
    // the capture state, and only until one sample is confirmed.
    let mut native_surface_sampled = false;
    // The pixel dimensions the current sample was taken at. A move does not
    // change them and therefore does not cost a capture; a resize does.
    let mut last_sampled_surface_shape: Option<NativeSurfaceShape> = None;
    // The Discord rectangle the cached composer measurement was taken against.
    // The cached rectangle is absolute, so while the window is being dragged the
    // only correct reading of it is "the same rectangle, moved by the same
    // delta". Without this the guard compared a pre-drag composer rectangle
    // against a mid-drag window rectangle, the containment check in
    // `verified_composer_overlay_rect` failed, and the whole session ended --
    // which is the composer vanishing mid-drag, with no hide involved at all.
    let mut measured_against_rect = last_rect;
    // A run of failed composer measurements, and when it started. One failure is
    // an ordinary transient on a cross-process accessibility walk; a run that
    // outlasts the grace is the conversation itself no longer being on screen.
    let mut consecutive_composer_measurement_failures: u32 = 0;
    let mut first_composer_measurement_failure = Instant::now();
    // Edge detector for `NATIVE_DISCORD_ROWS_MOVED_EVENT`. Opens with no band,
    // so the first tick that actually paints rows tells the renderer once.
    let mut transcript_band = TranscriptBandTracker::new(None);
    // The painted rows the last placement was issued against, in the same screen
    // space as `last_overlay_rect`. Kept so a move can be answered by translating
    // them instead of rebuilding them, which is a broker lock, a JSON
    // serialisation and a transcript-cache read the position cannot have changed.
    let mut last_painted_rows: Vec<[i32; 4]> = Vec::new();
    // Edge detector for `OVERLAY_COMPOSER_BAND_EVENT`. `None` means the renderer
    // has not been told yet, so the first pass that decides presence tells it
    // once; every reveal resets it to `None`, because a retained WebView that was
    // not listening at the last edge would otherwise draw a composer into a
    // window that covers nothing but Discord's transcript.
    let mut last_band_surrendered: Option<bool> = None;
    std::thread::Builder::new()
        .name("osl-protected-overlay-guard".to_owned())
        .spawn(move || {
            // Cleanup guarantee. Every exit from the loop below -- changed epoch,
            // fail-closed error, early return, or a panic unwinding this thread --
            // drops this and takes the retained pair off screen.
            let _pair_ownership = ProtectedWindowGuardOwnership::claim(&app, epoch);
            loop {
                std::thread::sleep(Duration::from_millis(16));
                let overlay_state = app.state::<OverlaySessionState>();
                if !overlay_state.is_epoch(epoch) {
                    return;
                }
                let ready = overlay_state.is_ready(epoch);
                let verified: Result<bool, String> = (|| {
                    // Whether the borrowed window is still exactly where the last
                    // placement put it. One local `GetWindowRect` for the whole
                    // tick: this used to be asked twice, once for the focus
                    // reclaim and once for the read-only fast path, and it is the
                    // fact three separate decisions on a drag turn on.
                    let host_rect_is_still = exact_window_rect_matches(discord_window, last_rect);
                    let focus_reclaim_pending = last_overlay_rect.is_some_and(|rect| {
                        !overlay_state.carrier_placement_active()
                            && !composer_temporarily_hidden
                            // A composer the operator has switched off must not
                            // take the keyboard back. With the lock down their
                            // keystrokes belong to Discord, so reclaiming focus
                            // here would type them into a surface that is on its
                            // way off screen instead.
                            && overlay_state.lock_engaged()
                            && host_rect_is_still
                            && active_should_reclaim_composer_focus(
                                ready,
                                exact_window_is_foreground(discord_window),
                                overlay_window_is_foreground(&app),
                                cursor_is_inside_overlay_rect(rect),
                            )
                    });
                    // Foreign focus is presentation state, not an identity
                    // question -- this branch has always said so in its own first
                    // line, and that is exactly why it may now answer it with
                    // nothing at all.
                    //
                    // It used to hide the composer *and* the shield and return
                    // early, on every tick, in both builds. That single branch is
                    // why the protected composer "exists but does not receive
                    // input": the window stays owned, stays positioned on the
                    // native composer rectangle and stays stacked above the
                    // borrowed Discord window -- it is simply not visible, so
                    // `WindowFromPoint` over its own rectangle answers Discord's
                    // hosted Chromium child. A click aimed at the protected
                    // composer therefore lands in Discord's real message box and
                    // every keystroke after it is typed, and sent, in the clear.
                    //
                    // Live QA drives this build from a third process (a terminal,
                    // a script host), so "neither OSL nor Discord is foreground"
                    // is the steady state of an automated run rather than a
                    // transient, and the composer was off screen for entire
                    // sessions.
                    //
                    // What the hide was defending is a state neither build can be
                    // in. Its own comment scopes it to a *topmost* protected
                    // surface lingering after Alt-Tab; in production
                    // `verify_owned_overlay_window` fails closed on
                    // `WS_EX_TOPMOST`, so the composer is always an ordinary
                    // owned popup of the trusted OSL parent and any window the
                    // operator switches to is raised above it -- and above
                    // Discord and the shield -- by ordinary z-order, with nothing
                    // for a hide to add. The QA build does hold the topmost band
                    // deliberately, and accepts a composer that stays on screen
                    // over a foreign foreground: that is the whole reason an
                    // automated run can type into it at all.
                    //
                    // The steady-state read itself is now gone as well, not just
                    // its reaction. Deleting the hide left the call behind with
                    // its answer immediately discarded, which still bought three
                    // foreground queries -- and, inside `osl_process_is_foreground`,
                    // two lookups in Tauri's window map -- on every 16 ms tick, for
                    // nothing. A value nothing may act on does not need computing,
                    // and a drag is exactly when that per-tick cost is felt.
                    //
                    // The foreground answers this guard genuinely uses all survive
                    // and all sit behind their own gates: the first-open decision
                    // (`active_first_guard_decision`), the focus reclaim above, and
                    // the foreground gate on the eye refresh and the surface
                    // backstop, none of which run on the 16 ms path.
                    //
                    // Nothing is recorded from here either: the z-order breadcrumb
                    // below already states the composer's proved order and
                    // visibility on the bounded 200 ms probe cadence.
                    //
                    // Discord can reclaim the foreground queue a few hundred
                    // milliseconds after the WebView initially receives focus. Do
                    // the bounded presentation-only correction before slower
                    // identity/Tauri validation so a typed draft cannot split across
                    // the protected and native composers. This path is QA-only and
                    // requires the already-ready epoch, exact hosted Discord HWND,
                    // unchanged native window rectangle, lost overlay focus, and the
                    // pointer still inside the last fully verified adaptive composer
                    // rectangle. The complete guard below still runs immediately and
                    // fails closed on any context drift.
                    let focus_reclaim_due = focus_reclaim_attempt_due(
                        focus_reclaim_pending,
                        last_focus_reclaim_attempt.elapsed(),
                    );
                    if focus_reclaim_due {
                        last_focus_reclaim_attempt = Instant::now();
                        let window = app
                            .get_webview_window(OVERLAY_LABEL)
                            .ok_or_else(|| "The native Discord overlay closed".to_owned())?;
                        // Deliberately not fatal, unlike the first-open focus
                        // acquisition. Windows refuses a foreground change for
                        // reasons that have nothing to do with this session --
                        // another process holds the foreground lock, a menu is
                        // open, the pointer left the rectangle between the read
                        // and the write -- and ending the session on one of those
                        // takes the composer off screen, which is the one outcome
                        // that must never follow from a presentation-only
                        // correction. The retry cadence above already bounds it,
                        // and the complete guard below still fails closed on
                        // anything that is actually an identity question.
                        if active_focus_overlay(&window).is_ok() {
                            // The composer has the keyboard again, which retires
                            // any refusal reported at open.
                            report_protected_focus_refused(&app, false);
                            let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_REFOCUS_EVENT, ());
                        }
                    }
                    // There is no visible protected surface to manage yet: the
                    // retained pair is still the hidden pre-warmed pair, and
                    // nothing below this point can be observed on screen
                    // until a reveal actually succeeds. Discovering the
                    // Discord window and refreshing composer bounds is real
                    // work -- a host-state lock plus, in QA/adversarial
                    // builds, an accessibility-tree walk into Discord -- and
                    // regaining OSL's own foreground is exactly the moment
                    // that work can overlap with OSL's own focus-change
                    // handling. Retry on a bounded cadence instead of every
                    // 16 ms tick; once a reveal has ever succeeded this session
                    // this early return never triggers again.
                    if !ready && !ever_revealed {
                        if !first_open_attempt_due(last_first_open_attempt.elapsed()) {
                            return Ok(false);
                        }
                        last_first_open_attempt = Instant::now();
                    }
                    let main = app
                        .get_webview_window("main")
                        .ok_or_else(|| "The trusted OSL overlay owner is unavailable".to_owned())?;
                    // How much of Discord this pass may cover, keyed on what the
                    // operator asked for rather than on where their attention is:
                    // whether they have taken the whole application off screen,
                    // whether the lock is up, and whether the eye is painting. A
                    // composer still floating over the desktop with its owner
                    // minimized is not "active", it is orphaned -- there is nothing
                    // behind it, nothing it is protecting, and the surface it exists
                    // to cover is not on screen at all. A composer still covering
                    // Discord's real message box after the operator switched the
                    // lock off is worse than orphaned: the lock is down, so their
                    // keystrokes are meant to go to Discord in the clear, and OSL is
                    // standing in the way of exactly that.
                    //
                    // But taking the *pair* off screen for a lowered lock made the
                    // lock decide what is displayed, and it decides only what is
                    // encrypted. `RowsOnly` is the answer to both at once: the
                    // painted rows stay covered and painted, and the composer band
                    // is surrendered whole. See `protected_surface_presence`.
                    //
                    // Asked before the pass does any work at all -- before the
                    // identity read, the host reconcile and the row read below --
                    // because all of that work exists to keep a composer on
                    // screen, and neither of these two states wants one. A
                    // switched-off session therefore costs two local window reads
                    // per tick and nothing else, instead of two `readiness()`
                    // resolutions (a file read each, in production) for a surface
                    // that must not be there.
                    //
                    // The hides that used to be here were removed on the grounds
                    // that Windows hides owned popups with their owner. It does
                    // not do so reliably for this pair -- the reported symptom is
                    // exactly a composer left on the desktop after a minimize --
                    // so the write is back, and it is the *whole* of the fix: the
                    // bookkeeping alone described a state that was not true.
                    //
                    // No reason here may ever be confused with the four states that
                    // also "take OSL's attention away" and must NOT hide anything:
                    // losing the foreground, being occluded, being dragged, and a
                    // pending sample. The tests are `IsIconic` on the trusted owner,
                    // the operator's own lock and the eye's painted rows, and
                    // nothing else -- a window being dragged, alt-tabbed away from,
                    // or covered is none of those, so a drag cannot reach this branch
                    // and the measured zero presence transitions across one stand.
                    //
                    // Nothing here writes per tick either: the pass returns early
                    // the moment it has hidden, and `composer_temporarily_hidden`
                    // keeps every later tick from re-issuing the same two hides.
                    // On restore, `composer_restored` drives the full
                    // `reveal_protected_pair` -- background, frame, alpha -- and
                    // is one of the four reasons the carrier stack is re-asserted,
                    // so the composer comes back above Discord rather than behind
                    // it, in one transition rather than a flash.
                    //
                    // The bookkeeping is a *read*, not an assumption. `hide_window`
                    // asks Tauri, which dispatches to the event loop and returns
                    // before anything has left the screen, so latching the flag on
                    // the strength of the request repeats the original defect in
                    // miniature: the flag would say "hidden", suppress every later
                    // retry, and the composer would sit on the desktop over a
                    // minimized owner with nothing left to ask again. Recording
                    // what `IsWindowVisible` answers on the cached handles instead
                    // costs no event-loop round trip, and a hide that has not
                    // landed yet simply gets re-issued on the next 16 ms tick until
                    // it has.
                    let presence = protected_surface_presence(
                        owner_window_is_minimized(&main).ok_or_else(|| {
                            "The trusted OSL overlay owner state is unavailable".to_owned()
                        })?,
                        overlay_state.lock_engaged(),
                        // The *fact*, not the eye setting: the last full pass
                        // published whether it actually had rows to paint. The eye
                        // is on by default, so the setting alone would hold a
                        // surface on screen for a session displaying nothing. An
                        // atomic load, so this costs the tick nothing.
                        overlay_state.protected_rows_painted(),
                    );
                    if presence == ProtectedSurfacePresence::OffScreen {
                        composer_temporarily_hidden =
                            leave_the_screen(&app, composer_temporarily_hidden);
                        return Ok(false);
                    }
                    // The band split, published to the protected renderer on its
                    // edges only. The window it is drawing into no longer covers a
                    // message box, so it must stop drawing a composer into it --
                    // and it cannot see that for itself: the surface it lays out in
                    // is a window whose size and origin it is never told.
                    //
                    // Announced here rather than at the placement below so the
                    // renderer's own repaint and the window's move are issued from
                    // the same pass, which is the shortest either can be apart.
                    let band_surrendered = presence == ProtectedSurfacePresence::RowsOnly;
                    if last_band_surrendered != Some(band_surrendered) {
                        last_band_surrendered = Some(band_surrendered);
                        let _ = app.emit_to(
                            OVERLAY_LABEL,
                            OVERLAY_COMPOSER_BAND_EVENT,
                            band_surrendered,
                        );
                    }
                    let owner = super::active_unlocked_osl_user_id(&app.state::<HubCoreState>())?;
                    // Clicking into Discord raises that borrowed window above its
                    // owned composer sibling: the composer is still there, just
                    // behind Discord. That is presentation drift, not an identity
                    // question, so it is detected with a bounded read-only
                    // z-order walk on a slow probe cadence and corrected only
                    // when the order is provably wrong. Re-asserting a stack that
                    // is already correct on every tick is what interrupted
                    // WebView2 keyboard delivery, so the read gates the write.
                    // Never while the borrowed window is moving, and that gate is
                    // the drag half of the owner's report. The correction this read
                    // can trigger -- `active_ensure_carrier_stack` -- is the most
                    // expensive thing the guard can do: a `settle_protected_window`
                    // round trip through the event-loop FIFO, an `hwnd()` getter
                    // through the same FIFO, a `SetWindowPos` with SWP_SHOWWINDOW, a
                    // style rewrite and a pair of DWM calls. A caption drag runs
                    // inside Windows' modal move loop, so every one of those round
                    // trips waits out the operator's own gesture, and clicking OSL's
                    // header to start the drag is exactly what reorders these
                    // siblings and makes the probe find drift.
                    //
                    // Deferred, never skipped: the read is re-armed the moment the
                    // rectangle stops, so a genuine inversion is corrected within
                    // one 200 ms probe of the operator letting go, and the composer
                    // is being re-placed on every frame in the meantime either way.
                    // A same-band reorder is not worth a stalled gesture.
                    let stack_probe_due = ready
                        && !composer_temporarily_hidden
                        && host_rect_is_still
                        && last_stack_probe.elapsed() >= PROTECTED_STACK_PROBE_INTERVAL;
                    if stack_probe_due {
                        last_stack_probe = Instant::now();
                    }
                    let stack_drifted =
                        stack_probe_due && active_protected_stack_drifted(&app, discord_window);
                    // Same read-only walk, same 200 ms cadence: a live run can
                    // state the composer's proved order and its visibility
                    // outright instead of inferring either from a screenshot.
                    #[cfg(feature = "discord-qa-shell")]
                    if stack_probe_due {
                        qa_record_composer_zorder(&app, discord_window, "probe");
                    }
                    let full_guard_budget_intact =
                        last_full_guard.elapsed() < PROTECTED_FULL_GUARD_BUDGET;
                    let presentation_is_undisturbed = ready
                        && ever_revealed
                        && !composer_temporarily_hidden
                        && !focus_reclaim_due
                        && !stack_drifted
                        && !protected_pair_is_dismissed()
                        && full_guard_budget_intact;
                    if presentation_is_undisturbed
                        && host_rect_is_still
                        && current_overlay_scale_milli(&app).is_some_and(|scale| {
                            last_geometry_key.is_some_and(|geometry| geometry.scale_milli == scale)
                        })
                    {
                        return Ok(true);
                    }
                    // The drag path, and the whole of what a drag needs. A move
                    // changes exactly one thing about this surface -- where it is --
                    // and the answer is already in hand: the composer travels with
                    // the window it lives in, so translating the placement the last
                    // full pass derived is not an approximation of re-deriving it,
                    // it is the same rectangle (`anchoring_a_placement_at_the_write_
                    // lands_where_a_fresh_pass_would`).
                    //
                    // What the full pass below charges for that answer is the drag
                    // half of the owner's report. Per 16 ms tick, for as long as the
                    // mouse is down, it resolves the identity twice through
                    // `readiness()` -- `cmd_status` plus, in production, a password
                    // status read off the filesystem -- reconciles the broker's host,
                    // takes the broker mutex again for the scope binding and
                    // serialises it to JSON, and rebuilds the painted-row set. None
                    // of those answers can change because a window moved, and none of
                    // them is what places the composer.
                    //
                    // So a moving window is answered from cached facts and local
                    // reads only, and the follow interval collapses to the tick plus
                    // one deferred `SetWindowPos` batch. Nothing is skipped: the
                    // budget above still forces a complete pass -- identity, context,
                    // ownership, geometry -- at least every 400 ms, exactly as the
                    // steady read-only path above already does, and any change that
                    // is not a pure translation (a resize, a DPI change, a
                    // re-measurement) falls straight through to it.
                    if presentation_is_undisturbed {
                        if let Some((live_rect, moved_rect, moved_rows)) = translated_drag_placement(
                            discord_window,
                            last_rect,
                            last_overlay_rect,
                            &last_painted_rows,
                        ) {
                            place_moved_protected_pair(
                                &app,
                                trusted_parent,
                                moved_rect,
                                &moved_rows,
                            )?;
                            last_rect = live_rect;
                            last_overlay_rect = Some(moved_rect);
                            last_painted_rows = moved_rows;
                            return Ok(true);
                        }
                    }
                    last_full_guard = Instant::now();
                    let target = match app
                        .state::<NativeWindowHostState>()
                        .discord_overlay_target(&owner)
                    {
                        Ok(target) => target,
                        Err(_) if composer_temporarily_hidden => return Ok(false),
                        Err(error) => {
                            #[cfg(feature = "discord-qa-shell")]
                            if ready
                                && osl_process_is_foreground(&app)
                                && exact_window_rect_matches(discord_window, last_rect)
                            {
                                return Ok(true);
                            }
                            return Err(error);
                        }
                    };
                    if target.trusted_parent != trusted_parent || target.window != discord_window {
                        return Err("The trusted OSL overlay owner changed".to_owned());
                    }
                    let composer_state = app.state::<NativeDiscordComposerState>();
                    let carrier_in_flight = composer_state.carrier_in_flight();
                    let osl_foreground = osl_process_is_foreground(&app);
                    // The eye, and only the eye, decides whether OSL displays
                    // anything over Discord's rows. Re-read on a slow cadence,
                    // never per tick, and only while OSL is foreground -- the
                    // pair is off screen otherwise, so nothing can be wrong.
                    // This reads OSL's own scope policy; it never touches
                    // Discord, its accessibility tree, or any window.
                    if osl_foreground
                        && last_protected_display_refresh.elapsed()
                            >= PROTECTED_DISPLAY_REFRESH_INTERVAL
                    {
                        last_protected_display_refresh = Instant::now();
                        if let Some(visible) = resolve_protected_display_visible(&app) {
                            overlay_state.set_protected_display_visible(visible);
                        }
                    }
                    // Exactly the rows OSL is painting decrypted text over, and
                    // therefore exactly what the opaque shield must cover and
                    // nothing else. Empty with the eye off; otherwise every row
                    // either source has proven, whether this client sent it this
                    // session or the bounded transcript reader found it in the
                    // history already on screen.
                    let display_visible = overlay_state.protected_display_visible();
                    let painted_rows =
                        painted_message_row_rects(&app, target.generation, display_visible);
                    let shielded = !painted_rows.is_empty();
                    // Published so the dismiss path can ask what is actually on
                    // screen instead of what the eye setting says. This is the
                    // fact `disengage_lock` needs: a session with the eye on but
                    // nothing painted has no protected pixel to keep alive, and
                    // switching the composer off must take it down.
                    overlay_state.set_protected_rows_painted(shielded);
                    // The lock is encryption only. It used to be read here as
                    // well, and an idle lock-off session took the protected pair
                    // off screen on the strength of it -- which is the composer
                    // being conditional on a setting that decides what happens to
                    // the operator's keystrokes, not whether they have somewhere
                    // to put them. `protected_surface_rect` no longer takes the
                    // lock either, so there is exactly one answer left for a live
                    // session: the composer is on screen.
                    //
                    // The cost this bail-out was protecting against is still paid
                    // for, elsewhere and better: the measurement it skipped is
                    // gated by `composer_measurement_allowed` and throttled by
                    // `GEOMETRY_REFRESH_MIN_INTERVAL`, and a steady session that
                    // is not moving reaches the read-only fast path above without
                    // measuring anything at all.
                    // Both halves of "unchanged" are read, not just the cached
                    // one: the host rectangle Tauri last reported and the
                    // rectangle Windows reports for that exact HWND right now.
                    let discord_rect_unchanged = target.rect == last_rect
                        && exact_window_rect_matches(discord_window, last_rect);
                    if !discord_rect_unchanged {
                        last_discord_rect_change = Instant::now();
                    }
                    // A maximize/restore is only over once Discord has stopped
                    // moving *and* had time to relay its own composer out, so
                    // the surface is re-derived from a measurement taken after
                    // that, never from the one taken the instant the rectangle
                    // stopped.
                    let discord_geometry_settled = discord_rect_unchanged
                        && last_discord_rect_change.elapsed() >= DISCORD_GEOMETRY_SETTLE;
                    // The renderer owns pixels only where OSL does, so a `wheel`
                    // inside Discord's window -- and a Discord move or resize --
                    // is invisible to it and leaves every row rectangle it holds
                    // stale. This is the one native signal it gets: no payload,
                    // and only on the tick where something actually moved. Both
                    // inputs are facts this tick already computed for other
                    // reasons, so detecting the edge adds no window call, no
                    // accessibility read and nothing cross-process; a steady
                    // session emits nothing, and a drag emits once, on the tick
                    // it comes to rest.
                    if transcript_band.observe(
                        painted_rows_bounds(&painted_rows),
                        discord_geometry_settled,
                        display_visible,
                    ) {
                        let _ = app.emit_to(OVERLAY_LABEL, NATIVE_DISCORD_ROWS_MOVED_EVENT, ());
                    }
                    let refresh_composer_bounds = composer_measurement_allowed(
                        ready,
                        !discord_rect_unchanged,
                    ) && active_should_refresh_composer_bounds(
                        ready,
                        osl_foreground,
                        discord_geometry_settled,
                        composer_temporarily_hidden,
                        carrier_in_flight,
                        last_composer_refresh.elapsed() >= GEOMETRY_REFRESH_MIN_INTERVAL,
                        last_composer_refresh.elapsed() >= COMPOSER_BACKSTOP_REFRESH_INTERVAL,
                    );
                    // Colour-only theme changes never move a rectangle, so the same
                    // foreground-gated backstop re-samples the native surface. No
                    // additional accessibility work is added here.
                    let native_surface_backstop_due = periodic_backstop_refresh_due(
                        osl_foreground,
                        carrier_in_flight,
                        last_native_surface_refresh.elapsed() >= NATIVE_SURFACE_BACKSTOP_INTERVAL,
                    );
                    let composer_bounds = if refresh_composer_bounds {
                        let scope_binding = super::native_discord_scope_binding(&app)?;
                        let refresh_started = Instant::now();
                        let refreshed = composer_state.refresh_verified_bounds(
                            &app.state::<NativeWindowHostState>(),
                            &owner,
                            &scope_binding,
                        );
                        #[cfg(feature = "discord-qa-shell")]
                        qa_record_composer_refresh_cost(
                            refresh_started.elapsed(),
                            refreshed.as_ref().err().map(String::as_str),
                        );
                        #[cfg(not(feature = "discord-qa-shell"))]
                        let _ = refresh_started;
                        last_composer_refresh = Instant::now();
                        match refreshed {
                            Ok(bounds) => {
                                consecutive_composer_measurement_failures = 0;
                                measured_against_rect = target.rect;
                                bounds
                            }
                            // Two very different things arrive here and they used
                            // to get the same answer -- take the composer off
                            // screen, wait for the hide to land, try again.
                            //
                            // The common one is a single failed measurement. The
                            // walk is a cross-process accessibility read whose
                            // probe points are the pixels this very surface sits
                            // on, and the adapter already answers that for itself:
                            // `ProbeClickThrough` marks OSL's own occluding windows
                            // hit-test transparent for the duration of the probe and
                            // restores the exact previous style on every exit path,
                            // including unwind. Hiding as well was a second
                            // mitigation for a problem the first one already owns,
                            // and it cost the operator the composer -- and, on the
                            // way back, a full reveal -- on nothing worse than a
                            // transient.
                            //
                            // The rare one is a conversation change. It reports
                            // "binding changed" because the verified conversation
                            // hash no longer matches, and the display genuinely has
                            // to come down: the binding is deliberately NOT rebound
                            // here, so protection can only ever re-engage over the
                            // exact conversation it was originally bound to.
                            //
                            // They are told apart by persistence rather than by
                            // matching an error string owned by another module: a
                            // transient resolves on the next attempt, a conversation
                            // change does not. Until the run outlasts the grace the
                            // last verified rectangle is still the best answer
                            // available and the composer stays exactly where it is.
                            Err(_) => {
                                if consecutive_composer_measurement_failures == 0 {
                                    first_composer_measurement_failure = Instant::now();
                                }
                                consecutive_composer_measurement_failures =
                                    consecutive_composer_measurement_failures.saturating_add(1);
                                let ends_display = composer_measurement_failure_ends_display(
                                    consecutive_composer_measurement_failures,
                                    first_composer_measurement_failure.elapsed(),
                                );
                                let cached = composer_state.verified_composer_bounds();
                                match (ends_display, cached) {
                                    (false, Some(bounds)) => bounds,
                                    // Either the conversation being protected is no
                                    // longer the one on screen, or there has never
                                    // been a verified rectangle to fall back to. In
                                    // both cases there is nothing left to derive a
                                    // surface from, so the display comes down and
                                    // the session stays alive to pick it up again.
                                    _ => {
                                        let hidden = app.get_webview_window(OVERLAY_LABEL);
                                        if let Some(window) = hidden.as_ref() {
                                            let _ = window.hide();
                                        }
                                        if let Some(shield) = app.get_webview_window(SHIELD_LABEL) {
                                            let _ = shield.hide();
                                        }
                                        if let Some(window) = hidden.as_ref() {
                                            for _ in 0..20 {
                                                if window.is_visible().is_ok_and(|visible| !visible)
                                                {
                                                    break;
                                                }
                                                std::thread::sleep(Duration::from_millis(15));
                                            }
                                        }
                                        composer_temporarily_hidden = true;
                                        return Ok(false);
                                    }
                                }
                            }
                        }
                    } else {
                        app.state::<NativeDiscordComposerState>()
                            .verified_composer_bounds()
                            .ok_or_else(|| {
                                "The verified Discord composer bounds are unavailable".to_owned()
                            })?
                    };
                    // Dragging OSL moves the borrowed Discord window under a
                    // cached composer rectangle that is absolute, and between two
                    // throttled measurements that rectangle is simply the old one.
                    // Compared against the window's *current* position it fails
                    // `verified_composer_overlay_rect`'s containment check, which
                    // returns `None`, which this guard reports as invalid geometry
                    // and ends the session on -- the composer disappearing mid-drag
                    // with no hide anywhere in the story.
                    //
                    // A move is not a measurement question. The composer travels
                    // with the window it lives in, so applying the same delta is
                    // not an approximation of the answer, it is the answer, and it
                    // costs no accessibility work at all.
                    let composer_bounds = host_rect_translation(measured_against_rect, target.rect)
                        .and_then(|delta| translated_bounds(composer_bounds, delta))
                        .unwrap_or(composer_bounds);
                    // One clamp, here, before anything downstream reads the rows:
                    // the surface, the shield, the shield's clipping region, the
                    // geometry key and the drag path's translation source all come
                    // from this one set. Clamping the surface alone would leave the
                    // shield -- a second window, placed from these rectangles --
                    // reaching into the message box, which is the same leak with a
                    // different window on top of it.
                    //
                    // A no-op unless the band is surrendered: with the lock up OSL
                    // owns the composer band, so a row that overlaps it is covered
                    // by the composer anyway and clipping it would only lose paint.
                    let painted_rows = if band_surrendered {
                        rows_above_the_composer_band(&painted_rows, composer_bounds.top)
                    } else {
                        painted_rows
                    };
                    // Rebound from the clamped set, because it is what the shield
                    // and the reveal are about. The published *fact* above stays
                    // the unclamped answer: "the eye has rows" is what presence
                    // turns on, and it must not be zeroed by a clip.
                    let shielded = !painted_rows.is_empty();
                    let Some(current_overlay_rect) = protected_presence_surface_rect(
                        presence,
                        target.rect,
                        Some(composer_bounds),
                        &painted_rows,
                    ) else {
                        // A surrendered band with nothing above the message box is
                        // not invalid geometry, it is nothing to be on screen for:
                        // the lock is down, so OSL owns no composer, and the eye has
                        // no row left to own either. Ending the session on that
                        // would take a display down over a scroll.
                        if band_surrendered {
                            composer_temporarily_hidden =
                                leave_the_screen(&app, composer_temporarily_hidden);
                            return Ok(false);
                        }
                        return Err("The native Discord composer geometry is invalid".to_owned());
                    };
                    let (surface_bounds, input_bounds) =
                        composer_state.verified_surface_bounds().ok_or_else(|| {
                            "The verified native composer surface is unavailable".to_owned()
                        })?;
                    // Same delta, same reason: these are absolute too, and the
                    // geometry key built from them has to describe where the
                    // surface is now, not where it was measured.
                    let (surface_bounds, input_bounds) =
                        match host_rect_translation(measured_against_rect, target.rect) {
                            Some(delta) => (
                                translated_bounds(surface_bounds, delta).unwrap_or(surface_bounds),
                                translated_bounds(input_bounds, delta).unwrap_or(input_bounds),
                            ),
                            None => (surface_bounds, input_bounds),
                        };
                    let current_scale_milli = current_overlay_scale_milli(&app)
                        .ok_or_else(|| "The native Discord display scale is invalid".to_owned())?;
                    // Read before the placement below rewrites `last_geometry_key`.
                    // A DPI change is the one geometry event that makes DWM
                    // re-create this window's redirection surface, so it is the
                    // one that still owes the stack pass; an ordinary move or
                    // resize does not, and must not pay for one.
                    let overlay_scale_changed = last_geometry_key
                        .is_some_and(|geometry| geometry.scale_milli != current_scale_milli);
                    let current_geometry_key = adaptive_geometry_key(
                        target.rect,
                        surface_bounds,
                        input_bounds,
                        composer_bounds,
                        current_overlay_rect,
                        current_scale_milli,
                    )
                    .ok_or_else(|| {
                        "The adaptive Discord composer geometry is invalid".to_owned()
                    })?;
                    let (guarded, ready_host) =
                overlay_state.with_bootstrap_context(|context_token, stored_host| {
                    let current = super::require_current_context_host(
                        &app,
                        &app.state::<HubCoreState>(),
                        &app.state::<HubBrokerState>(),
                        context_token,
                    )?;
                    if &current != stored_host || current.generation != target.generation {
                        return Err("The native Discord protection context changed".to_owned());
                    }
                    if !ready {
                        match active_first_guard_decision(
                            desktop_has_foreground_window(),
                            target.foreground,
                            osl_process_is_foreground(&app),
                            Instant::now() < first_guard_deadline,
                        ) {
                            FirstGuardDecision::WaitHidden => return Ok((false, None)),
                            FirstGuardDecision::Close => {
                                return Err(
                                    "The native Discord window is no longer foreground".to_owned()
                                )
                            }
                            FirstGuardDecision::Reveal => {}
                        }
                    }
                    // The second copy of the same presentation question used to
                    // live here: production ended the session on a foreground it
                    // did not trust and the QA build hid the pair. Both are gone.
                    //
                    // It has to go with the hide above rather than instead of it,
                    // or the fix would only move the failure: the branch above
                    // returned early, so this one was all but unreachable, and
                    // removing only the hide would have promoted a near-dead
                    // check into production's actual Alt-Tab behaviour -- the
                    // composer would stop hiding and start ending the session,
                    // which takes it off screen just the same. The two differ
                    // only in reading Discord's foreground from the host snapshot
                    // (`target.foreground`) instead of live, which is not a
                    // second question and never was.
                    //
                    // The steady-state foreground is now read exactly once per
                    // tick, above, and reacted to nowhere. The first-open
                    // foreground policy (`active_first_guard_decision`, just
                    // above) is untouched and still refuses to open, or closes, on
                    // an untrusted foreground -- this only stops an already-open
                    // session from taking its composer away.
                    let geometry_changed = last_geometry_key != Some(current_geometry_key)
                        || target.rect != last_rect
                        || last_overlay_rect != Some(current_overlay_rect);
                    // A session with no sampled background has nothing to paint,
                    // so it must sample one here rather than reveal a
                    // see-through window over Discord.
                    if !native_surface_sampled {
                        native_surface_sampled =
                            native_surface_is_paintable(&app, epoch, target.generation);
                    }
                    let window = app
                        .get_webview_window(OVERLAY_LABEL)
                        .ok_or_else(|| "The native Discord overlay closed".to_owned())?;
                    let shield = app
                        .get_webview_window(SHIELD_LABEL)
                        .ok_or_else(|| "The OSL capture shield closed".to_owned())?;
                    // The sampler reads a screen rectangle, so it may only run
                    // against a rectangle that was actually measured where the
                    // window is now. A translated rectangle is exact for
                    // *placement* -- the composer really did move with its window
                    // -- but sampling one would BitBlt whatever has since moved
                    // under it. A measurement is at most one refresh interval
                    // away, so this only ever defers a capture, never skips one.
                    let sampled_geometry_is_current = measured_against_rect == target.rect;
                    let current_surface_shape = native_surface_shape(surface_bounds, input_bounds);
                    let resample_wanted = sampled_geometry_is_current
                        && native_surface_resample_required(
                            native_surface_sampled,
                            current_surface_shape.is_none()
                                || last_sampled_surface_shape != current_surface_shape,
                            discord_geometry_settled,
                            native_surface_backstop_due,
                        );
                    // Measured, not assumed. Production excludes this exact HWND
                    // from screen capture, which is precisely the contract that
                    // lets a desktop-DC BitBlt over the composer's own rectangle
                    // return Discord's pixels rather than OSL's -- so production
                    // has nothing to hide. The QA shell deliberately runs with
                    // capture protection off so a harness can screenshot it, and
                    // the operator can turn it off in a shipping build too. An
                    // unreadable affinity, or a shield that does reach the sampled
                    // strip, answers the same way.
                    let sample_needs_the_screen =
                        resample_wanted
                            && sampling_requires_the_pair_to_leave_the_screen(
                                composer_is_excluded_from_capture(&window),
                                shield_overlaps_sampled_surface(&painted_rows, surface_bounds),
                            );
                    // And this is where that answer stops being allowed to cost
                    // the operator their composer. A capture that needs the pair
                    // off screen may only run while the pair is already off
                    // screen; otherwise the capture is abandoned and the sample
                    // already in hand is kept. A background strip that is one
                    // theme change stale is a colour; a composer that blinks out
                    // is a lost keystroke, and the owner's rule is that it is
                    // never off screen.
                    let resample_native_surface = resample_wanted
                        && (!sample_needs_the_screen
                            || resample_may_take_the_pair_off_screen(
                                ever_revealed,
                                composer_temporarily_hidden,
                            ));
                    // Re-sampling must never cost the user their caret, so a
                    // surface-only pass restores focus after it repaints.
                    let mut restore_overlay_focus = false;
                    if resample_native_surface {
                        let must_leave_the_screen = sample_needs_the_screen;
                        last_native_surface_refresh = Instant::now();
                        if must_leave_the_screen {
                            restore_overlay_focus =
                                !geometry_changed && overlay_window_is_foreground(&app);
                            let _ = window.hide();
                            let _ = shield.hide();
                            composer_temporarily_hidden = true;
                            // Tauri dispatches hide() to the main thread. Sampling
                            // before it lands would capture this protected surface
                            // instead of Discord's composer and then feed those
                            // pixels back as the native background. Wait for the
                            // pair to actually leave the screen, bounded.
                            for _ in 0..20 {
                                if window.is_visible().is_ok_and(|visible| !visible)
                                    && shield.is_visible().is_ok_and(|visible| !visible)
                                {
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(15));
                            }
                        }
                        #[cfg(feature = "discord-qa-shell")]
                        composer_state.invalidate_sent_carrier_rows();
                        app.state::<crate::native_surface_capture::NativeSurfaceCaptureState>()
                            .clear();
                        refresh_adaptive_native_surface(&app, &owner, epoch, target)?;
                        native_surface_sampled = true;
                        last_sampled_surface_shape = composer_state
                            .verified_surface_bounds()
                            .and_then(|(outer, input)| native_surface_shape(outer, input));
                    }
                    // One reposition site, reached by both paths. It used to live
                    // inside the re-sample above, which is why a move could not be
                    // answered without a capture -- and therefore, before this,
                    // without a hide. Repositioning is a single deferred
                    // `SetWindowPos` pair and costs nothing a drag can feel.
                    // The third arm of this predicate -- "nothing has been
                    // revealed yet" -- is what makes the first appearance land in
                    // the right place instead of snapping into it. See
                    // `protected_placement_required`.
                    if protected_placement_required(
                        geometry_changed,
                        resample_native_surface,
                        ever_revealed,
                    ) {
                        let (placed_bounds, placed_surface, placed_input) =
                            if resample_native_surface {
                                // A capture republishes the presentation bounds it
                                // derived from the pixels it just took, so the
                                // placement has to use those and not the ones this
                                // pass started with.
                                let bounds =
                                    composer_state.verified_composer_bounds().ok_or_else(|| {
                                        "The adaptive Discord composer bounds are unavailable"
                                            .to_owned()
                                    })?;
                                let (surface, input) = composer_state
                                    .verified_surface_bounds()
                                    .ok_or_else(|| {
                                        "The adaptive Discord composer surface is unavailable"
                                            .to_owned()
                                    })?;
                                (bounds, surface, input)
                            } else {
                                (composer_bounds, surface_bounds, input_bounds)
                            };
                        let placed_scale = current_overlay_scale_milli(&app).ok_or_else(|| {
                            "The adaptive Discord display scale is invalid".to_owned()
                        })?;
                        // Re-clamped against the bounds this placement is actually
                        // issued with. A re-sample republishes the composer
                        // rectangle it derived from the pixels it just took, so a
                        // clamp taken against the rectangle this pass opened with
                        // could put the shield a few pixels into the message box --
                        // and a few pixels of the box is the whole leak.
                        let placed_painted_rows = if band_surrendered {
                            rows_above_the_composer_band(&painted_rows, placed_bounds.top)
                        } else {
                            painted_rows.clone()
                        };
                        let placed_rect = protected_presence_surface_rect(
                            presence,
                            target.rect,
                            Some(placed_bounds),
                            &placed_painted_rows,
                        );
                        let Some(placed_rect) = placed_rect else {
                            // Same reasoning as the surface above: a surrendered
                            // band with no row left above the message box is
                            // nothing to be on screen for, not a broken session.
                            if band_surrendered {
                                composer_temporarily_hidden =
                                    leave_the_screen(&app, composer_temporarily_hidden);
                                // The complete-guard closure's "nothing revealed
                                // this pass" answer, which the loop turns into a
                                // plain `continue`.
                                return Ok((false, None));
                            }
                            return Err(
                                "The adaptive Discord composer geometry is invalid".to_owned()
                            );
                        };
                        // Anchored to where the borrowed window is *now*, not to
                        // where it was when this pass opened. Everything above --
                        // the host reconcile, the scope read, the row read -- is
                        // real work done between that read and this write, and a
                        // moving window does not wait for it. Placing against the
                        // opening read therefore writes the composer to a position
                        // the window has already left, and the next pass writes it
                        // to another one it has also already left: the composer
                        // trails the gesture instead of following it. Correcting
                        // the residual translation at the write is what makes this
                        // the only writer, and one writer cannot fight itself.
                        //
                        // All six values move together or none of them do, so the
                        // geometry key can never describe a surface half-shifted --
                        // and neither can the batch below. The rows were the one
                        // value left out of this correction: the composer was placed
                        // at the live rectangle while the shield was placed, and
                        // clipped, at the rows as measured against the opening one,
                        // so mid-drag the two could be seen apart by exactly the
                        // residual delta.
                        let (
                            placed_host_rect,
                            placed_rect,
                            placed_bounds,
                            placed_surface,
                            placed_input,
                            placed_rows,
                        ) = host_translation_since(discord_window, target.rect)
                            .and_then(|(delta, live)| {
                                Some((
                                    live,
                                    translated_overlay_rect(placed_rect, delta)?,
                                    translated_bounds(placed_bounds, delta)?,
                                    translated_bounds(placed_surface, delta)?,
                                    translated_bounds(placed_input, delta)?,
                                    translated_painted_rows(&placed_painted_rows, delta)?,
                                ))
                            })
                            .unwrap_or((
                                target.rect,
                                placed_rect,
                                placed_bounds,
                                placed_surface,
                                placed_input,
                                placed_painted_rows,
                            ));
                        position_window_pair(&window, &shield, placed_rect, &placed_rows)?;
                        last_rect = placed_host_rect;
                        last_overlay_rect = Some(placed_rect);
                        // What the next move translates from, in the same screen
                        // space as the placement it was issued with.
                        last_painted_rows = placed_rows;
                        last_geometry_key = adaptive_geometry_key(
                            placed_host_rect,
                            placed_surface,
                            placed_input,
                            placed_bounds,
                            placed_rect,
                            placed_scale,
                        );
                        if last_geometry_key.is_none() {
                            return Err(
                                "The adaptive Discord composer geometry is invalid".to_owned()
                            );
                        }
                    }
                    verify_owned_overlay_pair(&window, &shield, trusted_parent)?;
                    // Deliberately NOT re-asserted here any more. The frame
                    // contract is no longer a style word this guard has to win a
                    // race for: `install_protected_frame_hook` leaves these
                    // windows with no non-client area at all, so a rebuilt
                    // WS_CAPTION has nowhere to be drawn and there is nothing for
                    // a steady pass to correct. Removing this also removes the
                    // last per-pass Win32 write from the guard.
                    let composer_restored = composer_temporarily_hidden;
                    if composer_restored {
                        // Restoring is a reveal: it must have a native
                        // background to paint and must end up frameless.
                        reveal_protected_pair(
                            &app,
                            &window,
                            &shield,
                            epoch,
                            target.generation,
                            shielded,
                            "The native Discord overlay could not be restored safely",
                        )?;
                        if restore_overlay_focus {
                            // Bounded and non-fatal: the authoritative focus
                            // guard below still runs on every pass.
                            let _ = window.set_focus();
                        }
                        composer_temporarily_hidden = false;
                        // A reveal is also the moment a retained WebView may have
                        // missed the last band edge, so it is re-announced with the
                        // session announcement below rather than left to the next
                        // change -- which, in a steady session, never comes.
                        last_band_surrendered = None;
                        if ready {
                            // The renderer learns a session is readable from one
                            // announcement at the phase transition below. A
                            // retained WebView that was not listening yet at that
                            // instant would otherwise refuse every keystroke for
                            // the whole session, so every reveal of an
                            // already-ready session repeats it. The payload still
                            // carries no token, identity, or content.
                            let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_SESSION_EVENT, true);
                        }
                    }
                    if !ready {
                        apply_protected_composer_capture_protection(&window)?;
                        // Capture resistance is proven above, before the first
                        // reveal; the reveal itself additionally proves there is
                        // a native background to paint and strips the frame the
                        // reveal restores.
                        reveal_protected_pair(
                            &app,
                            &window,
                            &shield,
                            epoch,
                            target.generation,
                            shielded,
                            "The native Discord overlay could not be shown safely",
                        )?;
                        last_band_surrendered = None;
                        // The keyboard belongs to whoever owns the composer band.
                        // With the band surrendered it is Discord's, and taking the
                        // foreground here would type the operator's next keystroke
                        // into a surface that is deliberately not a message box. The
                        // reclaim path above is gated on the same fact.
                        if active_overlay_requires_focus_acquisition() && !band_surrendered {
                            #[cfg(feature = "discord-qa-shell")]
                            qa_overlay_window_stage("focus_requested");
                            // A refused foreground is no longer fatal. It used
                            // to `return Err`, which this loop turns into
                            // `clear_and_hide` -- so the single most likely way
                            // to engage protection (caret already in Discord's
                            // own message box, which is exactly the state in
                            // which Windows refuses a foreground change) tore
                            // the whole session down and left the operator
                            // typing into Discord in the clear, with no
                            // composer on screen to notice was missing.
                            //
                            // A composer that is visible but does not yet hold
                            // the keyboard is strictly better than no composer:
                            // it can be clicked, the reclaim path above retries
                            // it every time the pointer is over it, and the
                            // refusal itself is announced rather than silent.
                            // Nothing below this branch depends on focus.
                            let focus_acquired = active_focus_overlay(&window).is_ok();
                            #[cfg(feature = "discord-qa-shell")]
                            qa_overlay_window_stage(if focus_acquired {
                                "focus_confirmed"
                            } else {
                                "focus_failed"
                            });
                            report_protected_focus_refused(&app, !focus_acquired);
                            // Native focus is on the window; the caret is not.
                            // The renderer puts it in the draft when it hears
                            // this, and until now the only emitter was the
                            // focus-reclaim path, which by construction cannot
                            // fire at engage -- it requires an already-ready
                            // session. So the one moment the operator is most
                            // likely to start typing was the one moment nothing
                            // told the renderer to be ready for it, and a
                            // keystroke that misses OSL's composer lands in
                            // Discord's real message box in the clear. Carries
                            // no token, identity, or content.
                            //
                            // Only on a proven focus, though: this event tells
                            // the renderer to put the caret in the draft, which
                            // is a claim about where keystrokes are going. Sent
                            // after a refusal it would be the exact false
                            // reassurance this branch exists to prevent.
                            if focus_acquired {
                                let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_REFOCUS_EVENT, ());
                            }
                        }
                    }
                    // Raising an already-correct topmost WebView on every full
                    // guard interrupts WebView2 keyboard delivery on Windows.
                    // The exact stack therefore only needs mutation when opening,
                    // after a DPI change, on a restored composer, or when the
                    // bounded read-only probe above proved Discord is now above
                    // the composer; steady, correct guards stay read-only so
                    // typing is never interrupted.
                    //
                    // `geometry_changed` used to be one of these, and that is the
                    // reported drag lag. A drag makes it true on essentially every
                    // 16 ms tick, and this call is not cheap: it is a blocking
                    // `settle_protected_window` round trip to the UI thread, a
                    // `SetWindowPos` with `SWP_SHOWWINDOW`, a style rewrite and a
                    // pair of DWM calls, all issued at exactly the moment that
                    // thread is busiest -- and, because the DWM half re-asserts
                    // the blur-behind region, all of it re-run per frame is also a
                    // way to make the surface flicker.
                    //
                    // It buys nothing. The placement above is a `DeferWindowPos`
                    // pair issued with `SWP_NOZORDER`, so a move or a resize
                    // provably cannot change the order this call exists to
                    // correct, and anything that *does* reorder the stack is
                    // caught by `stack_drifted` within one 200 ms probe. What a
                    // geometry event can genuinely invalidate is the redirection
                    // surface DWM composites this window through, and only on a
                    // DPI change -- which is what `overlay_scale_changed` is.
                    if !ready || overlay_scale_changed || composer_restored || stack_drifted {
                        active_ensure_carrier_stack(&window, &shield, discord_window, shielded)?;
                        last_stack_probe = Instant::now();
                        // The correction has to prove it landed, not just that it
                        // was issued: this is the only writer of the composer's
                        // order, so its own read-back is what a live run quotes.
                        #[cfg(feature = "discord-qa-shell")]
                        qa_record_composer_zorder(&app, discord_window, "carrier_stack");
                    }
                    #[cfg(feature = "discord-qa-shell")]
                    qa_overlay_window_stage("carrier_stack_confirmed");
                    if !ready {
                        let confirmed = active_confirm_overlay_target_after_focus(
                            &app.state::<NativeWindowHostState>(),
                            &owner,
                            target,
                        )?;
                        #[cfg(feature = "discord-qa-shell")]
                        qa_overlay_window_stage("post_focus_host_confirmed");
                        let confirmed_current = super::require_current_context_host(
                            &app,
                            &app.state::<HubCoreState>(),
                            &app.state::<HubBrokerState>(),
                            context_token,
                        )?;
                        #[cfg(feature = "discord-qa-shell")]
                        qa_overlay_window_stage("post_focus_context_confirmed");
                        if &confirmed_current != stored_host
                            || confirmed.generation != target.generation
                            || !exact_window_rect_matches(confirmed.window, target.rect)
                            || active_first_guard_decision(
                                desktop_has_foreground_window(),
                                confirmed.foreground,
                                osl_process_is_foreground(&app),
                                Instant::now() < first_guard_deadline,
                            ) != FirstGuardDecision::Reveal
                        {
                            return Err(
                                "The native Discord protection context changed while opening"
                                    .to_owned(),
                            );
                        }
                        #[cfg(feature = "discord-qa-shell")]
                        qa_overlay_window_stage("first_guard_confirmed");
                    }
                    // Reached only once every check above passed, which means
                    // the pair is now genuinely on screen (either just
                    // restored above, or just revealed for the first time).
                    // From here on the first-open throttle above must never
                    // gate this session again.
                    ever_revealed = true;
                    Ok((true, (!ready).then(|| stored_host.clone())))
                })?;
                    if let Some(host) = ready_host {
                        // Do not attempt to reacquire the session mutex from inside
                        // with_bootstrap_context; the complete guard result is first
                        // copied out, then the phase transition is committed.
                        overlay_state.mark_ready(epoch, &host)?;
                        // A retained renderer would otherwise wait out its own retry
                        // backoff before it could read the newly verified session.
                        let _ = app.emit_to(OVERLAY_LABEL, OVERLAY_SESSION_EVENT, true);
                    }
                    Ok(guarded)
                })();
                if let Err(error) = verified {
                    #[cfg(feature = "discord-qa-shell")]
                    let _ = osl_privacy_hub::discord_qa_inbound_receipt::record_overlay_open_stage(
                        "error",
                        Some(&error),
                    );
                    clear_and_hide(&app);
                    return;
                }
                if verified == Ok(false) {
                    continue;
                }
            }
        })
        .map(|_| ())
        .map_err(|_| "The native Discord overlay guard could not be started".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use osl_privacy_hub::service_host::ServiceHostState;
    use std::sync::{mpsc, Arc};

    fn test_host(account_id: &str) -> ActiveServiceHost {
        ServiceHostState::default()
            .begin_open("owner-test", "discord", account_id, "discord.com")
            .expect("test host")
    }

    /// `PROTECTED_PAIR_DISMISSED` is process-global by design -- it has to be
    /// readable from the guard thread, from IPC workers and from Tauri's own
    /// window-event thread -- so the tests that drive it are serialized against
    /// each other. Nothing else in this module writes it: the only clearer is
    /// `show_guarded_overlay`, which no unit test reaches.
    static PRESENCE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn presence_test_guard() -> std::sync::MutexGuard<'static, ()> {
        let guard = PRESENCE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        admit_protected_pair();
        guard
    }

    /// C8. Switching the protected composer off must take it off screen, and the
    /// only thing that decides whether the session survives that is whether OSL
    /// still has a protected pixel on Discord's message rows.
    ///
    /// The defect this pins is that the predicate used to be the *eye setting*,
    /// which `security::scope_security` defaults to enabled. So in a default
    /// configuration "close the protected composer" lowered a flag, kept the
    /// session, kept the guard, and left the composer window sitting exactly over
    /// Discord's real message box while the hub rendered "Protected composer
    /// off". `protected_surface_rect` is deliberately lock-free and was never
    /// going to take it down; this answer is the only thing that can.
    #[test]
    fn switching_the_composer_off_survives_only_while_rows_are_actually_painted() {
        let _presence = presence_test_guard();
        let state = OverlaySessionState::default();
        let host = test_host("composer-off");

        // The reported defect, exactly: the eye is on (its default) and nothing
        // is painted. Nothing is on screen but the composer itself, so switching
        // the composer off has to end the session that keeps it there.
        state
            .activate("ctx.composer-off".to_owned(), host.clone())
            .expect("epoch");
        state.set_protected_display_visible(true);
        state.set_protected_rows_painted(false);
        assert!(
            !state.disengage_lock(),
            "a session displaying nothing must not survive its composer being switched off"
        );
        assert!(!state.lock_engaged());

        // The one state that legitimately survives: OSL is painting decrypted
        // rows through this surface, so tearing it down would take the display
        // away -- which is the eye's business, not the lock's.
        state
            .activate("ctx.composer-off".to_owned(), host.clone())
            .expect("epoch");
        state.set_protected_display_visible(true);
        state.set_protected_rows_painted(true);
        assert!(state.disengage_lock());

        // And the eye still overrides: rows recorded from an earlier pass cannot
        // keep a session alive once the operator has closed the eye.
        state
            .activate("ctx.composer-off".to_owned(), host.clone())
            .expect("epoch");
        state.set_protected_display_visible(false);
        state.set_protected_rows_painted(true);
        assert!(!state.disengage_lock());

        // A fresh session starts from "painting nothing", never from whatever the
        // previous one happened to leave behind: a stale `true` here is a
        // composer that survives being switched off for a session that has not
        // yet painted a single row.
        state.set_protected_rows_painted(true);
        state
            .activate("ctx.composer-off".to_owned(), host)
            .expect("epoch");
        state.set_protected_display_visible(true);
        assert!(!state.disengage_lock());
        state.clear();
        assert!(!state.protected_pixels_on_screen());
    }

    /// C8, second half. A dismiss and a guard pass are on different threads with
    /// no ordering between them, so the dismiss states a fact instead of racing
    /// the reveal for the last write.
    #[test]
    fn a_dismissed_pair_cannot_be_revealed_by_a_pass_that_was_already_in_flight() {
        let _presence = presence_test_guard();
        let state = OverlaySessionState::default();
        let host = test_host("dismiss");

        state
            .activate("ctx.dismiss".to_owned(), host.clone())
            .expect("epoch");
        assert!(!protected_pair_is_dismissed());
        assert!(refuse_dismissed_protected_pair().is_ok());

        // The operator switches the composer off. The hide it issues is
        // best-effort and asynchronous; this latch is not.
        dismiss_protected_pair();
        assert!(protected_pair_is_dismissed());
        assert_eq!(
            refuse_dismissed_protected_pair().unwrap_err(),
            PROTECTED_PAIR_DISMISSED_ERROR
        );
        // Repeated dismisses are idempotent, and nothing on the guard's side may
        // clear it -- not a new epoch, not a restored composer, not a re-sample.
        dismiss_protected_pair();
        assert!(protected_pair_is_dismissed());

        // Activating a session is NOT what admits one: a verified open has to
        // reach its window path first, and that is the single clearer.
        state
            .activate("ctx.dismiss".to_owned(), host)
            .expect("epoch");
        assert!(protected_pair_is_dismissed());
        assert_eq!(
            refuse_dismissed_protected_pair().unwrap_err(),
            PROTECTED_PAIR_DISMISSED_ERROR
        );

        // Only the operator asking for a composer again admits one.
        admit_protected_pair();
        assert!(!protected_pair_is_dismissed());
        assert!(refuse_dismissed_protected_pair().is_ok());
    }

    /// C9. The very first reveal of a session is always preceded by a placement,
    /// so the composer appears once, already over Discord's message box.
    #[test]
    fn nothing_is_ever_revealed_before_it_has_been_placed() {
        // Until one reveal has succeeded, every pass places -- including the pass
        // that reveals. This is the arm that was missing: the guard seeds its
        // geometry key from the calibration the session opened with, so an
        // unchanged composer reaches its first reveal with no geometry change at
        // all and used to place nothing.
        assert!(protected_placement_required(false, false, false));
        assert!(protected_placement_required(true, false, false));
        // Once something is on screen, only a real reason moves it. A steady
        // session -- and every frame of a drag that has not moved the window --
        // writes nothing.
        assert!(!protected_placement_required(false, false, true));
        assert!(protected_placement_required(true, false, true));
        // A capture republishes the presentation bounds it derived, so the
        // placement has to follow it even when the key did not change.
        assert!(protected_placement_required(false, true, true));
    }

    #[test]
    fn native_overlay_style_removes_only_non_client_frame_bits() {
        let application_bits = 0x1000_0000isize | 0x0000_0080isize;
        let decorated = application_bits | NATIVE_OVERLAY_FRAME_STYLE_MASK;
        assert_eq!(frameless_native_overlay_style(decorated), application_bits);
        assert_eq!(
            frameless_native_overlay_style(application_bits),
            application_bits
        );
        assert_eq!(
            frameless_native_overlay_style(decorated) & NATIVE_OVERLAY_FRAME_STYLE_MASK,
            0
        );
    }

    #[test]
    fn native_overlay_ex_style_removes_only_non_client_edges() {
        // WS_EX_TOPMOST, WS_EX_APPWINDOW, WS_EX_NOREDIRECTIONBITMAP,
        // WS_EX_NOACTIVATE, WS_EX_LAYERED and WS_EX_TRANSPARENT carry the
        // ownership, transparency, focus and capture contracts of the protected
        // pair. Clearing any of them here would silently weaken one of them, so
        // every one is asserted to survive.
        let contract_bits = 0x0000_0008isize // WS_EX_TOPMOST
            | 0x0004_0000isize // WS_EX_APPWINDOW
            | 0x0020_0000isize // WS_EX_NOREDIRECTIONBITMAP
            | 0x0800_0000isize // WS_EX_NOACTIVATE
            | 0x0008_0000isize // WS_EX_LAYERED
            | 0x0000_0020isize; // WS_EX_TRANSPARENT
        let edged = contract_bits | NATIVE_OVERLAY_FRAME_EX_STYLE_MASK;
        assert_eq!(frameless_native_overlay_ex_style(edged), contract_bits);
        assert_eq!(
            frameless_native_overlay_ex_style(contract_bits),
            contract_bits
        );
        assert_eq!(
            frameless_native_overlay_ex_style(edged) & NATIVE_OVERLAY_FRAME_EX_STYLE_MASK,
            0
        );
        // WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE |
        // WS_EX_STATICEDGE, and nothing else.
        assert_eq!(
            NATIVE_OVERLAY_FRAME_EX_STYLE_MASK,
            0x1 | 0x100 | 0x200 | 0x2_0000
        );
        assert_eq!(
            NATIVE_OVERLAY_FRAME_EX_STYLE_MASK & contract_bits,
            0,
            "no contract bit may live in the frame mask"
        );
    }

    fn key_rect(discord: [i32; 4], presentation: AccessibilityBounds) -> OverlayRect {
        protected_overlay_rect(discord, Some(presentation), None).unwrap_or(PREWARM_OVERLAY_RECT)
    }

    fn composer_bounds(discord: [i32; 4]) -> AccessibilityBounds {
        AccessibilityBounds {
            left: discord[0] + 336,
            top: discord[3] - 84,
            right: discord[2] - 32,
            bottom: discord[3] - 28,
        }
    }

    #[test]
    fn the_protected_surface_is_exactly_the_measured_composer() {
        let target = [100, 80, 1_380, 800];
        let composer = composer_bounds(target);
        let rect = protected_overlay_rect(target, Some(composer), None)
            .expect("verified composer geometry");
        assert_eq!(rect.x, composer.left);
        assert_eq!(rect.y, composer.top);
        assert_eq!(rect.width, (composer.right - composer.left) as u32);
        assert_eq!(rect.height, (composer.bottom - composer.top) as u32);
        assert!(rect.x >= target[0]);
        assert!(rect.y >= target[1]);
        assert!(rect.x + rect.width as i32 <= target[2]);
        assert!(rect.y + rect.height as i32 <= target[3]);
    }

    #[test]
    fn nothing_painted_leaves_discords_whole_message_list_untouched() {
        let target = [0, 0, 1_920, 1_080];
        let composer = composer_bounds(target);
        let rect = protected_overlay_rect(target, Some(composer), None).expect("verified composer");
        // The message list starts above the composer, and OSL owns none of it:
        // no header inset, no sidebar inset, no band. Eye off is raw Discord
        // because there is no OSL window over the conversation at all.
        assert_eq!(rect.y, composer.top);
        assert!(rect.y > target[1] + 900);
        assert_eq!(painted_rows_top(&[]), None);
        assert_eq!(painted_rows_bounds(&[]), None);
    }

    #[test]
    fn an_unmeasured_composer_gets_no_protected_surface_at_all() {
        let target = [0, 0, 1_920, 1_080];
        // Deliberately no fallback. A surface derived from anything other than
        // the exact measured composer is what covered the conversation.
        assert_eq!(protected_overlay_rect(target, None, None), None);
        assert_eq!(protected_overlay_rect(target, None, Some(200)), None);
        // Implausible bounds fail closed rather than widening to a band.
        assert_eq!(
            protected_overlay_rect(
                target,
                Some(AccessibilityBounds {
                    left: 20,
                    top: 40,
                    right: 200,
                    bottom: 80,
                }),
                None,
            ),
            None
        );
    }

    #[test]
    fn the_surface_grows_only_as_far_as_the_rows_osl_actually_paints() {
        let target = [0, 0, 1_920, 1_080];
        let composer = composer_bounds(target);
        let painted = [
            [composer.left + 8, 700, composer.right - 8, 740],
            [composer.left + 8, 760, composer.right - 8, 800],
        ];
        assert_eq!(painted_rows_top(&painted), Some(700));
        let rect = protected_overlay_rect(target, Some(composer), painted_rows_top(&painted))
            .expect("verified composer");
        assert_eq!(rect.y, 700);
        assert_eq!(rect.y + rect.height as i32, composer.bottom);
        // Never below the highest painted row, and never outside Discord.
        assert!(rect.y > target[1]);
        let bounds = painted_rows_bounds(&painted).expect("painted bounds");
        assert_eq!(bounds.y, 700);
        assert_eq!(bounds.y + bounds.height as i32, 800);
        assert_eq!(bounds.x, composer.left + 8);
        assert_eq!(bounds.width, (composer.right - composer.left - 16) as u32);
        // The gap between the two rows is inside the bounding box but is NOT
        // shielded: the region built from these rectangles excludes it.
        assert!(painted.iter().all(|row| row[1] >= bounds.y));
    }

    #[test]
    fn a_painted_row_can_never_push_the_surface_outside_discord() {
        let target = [100, 80, 1_380, 800];
        let composer = composer_bounds(target);
        let rect = protected_overlay_rect(target, Some(composer), Some(-4_000))
            .expect("verified composer");
        assert_eq!(rect.y, target[1]);
        assert_eq!(rect.y + rect.height as i32, composer.bottom);
        // A row below the composer cannot shrink the surface either.
        assert_eq!(
            protected_overlay_rect(target, Some(composer), Some(composer.top + 10)),
            protected_overlay_rect(target, Some(composer), None)
        );
    }

    #[test]
    fn adaptive_geometry_key_tracks_every_native_layout_dimension() {
        let discord = [0, 0, 1_920, 1_080];
        let surface = AccessibilityBounds {
            left: 336,
            top: 984,
            right: 1_888,
            bottom: 1_052,
        };
        let input = AccessibilityBounds {
            left: 392,
            top: 996,
            right: 1_824,
            bottom: 1_040,
        };
        let presentation = AccessibilityBounds {
            left: 336,
            top: 984,
            right: 1_888,
            bottom: 1_052,
        };
        let baseline = adaptive_geometry_key(
            discord,
            surface,
            input,
            presentation,
            key_rect(discord, presentation),
            1_000,
        )
        .expect("baseline");

        assert_ne!(
            adaptive_geometry_key(
                [20, 10, 1_940, 1_090],
                surface,
                input,
                presentation,
                key_rect([20, 10, 1_940, 1_090], presentation),
                1_000,
            ),
            Some(baseline)
        );
        assert_ne!(
            adaptive_geometry_key(
                discord,
                surface,
                AccessibilityBounds {
                    top: input.top + 1,
                    ..input
                },
                presentation,
                key_rect(discord, presentation),
                1_000
            ),
            Some(baseline)
        );
        assert_ne!(
            adaptive_geometry_key(
                discord,
                surface,
                input,
                presentation,
                key_rect(discord, presentation),
                1_250
            ),
            Some(baseline)
        );
    }

    #[test]
    fn adaptive_geometry_fails_closed_at_invalid_scales_and_unbounded_inputs() {
        assert_eq!(bounded_scale_milli(0.499), None);
        assert_eq!(bounded_scale_milli(8.001), None);
        assert_eq!(bounded_scale_milli(f64::NAN), None);
        assert_eq!(bounded_scale_milli(1.25), Some(1_250));

        let discord = [0, 0, 1_920, 1_080];
        let surface = AccessibilityBounds {
            left: 336,
            top: 984,
            right: 1_888,
            bottom: 1_052,
        };
        let presentation = surface;
        assert!(adaptive_geometry_key(
            discord,
            surface,
            AccessibilityBounds {
                left: 300,
                top: 996,
                right: 1_824,
                bottom: 1_040,
            },
            presentation,
            key_rect(discord, presentation),
            1_000,
        )
        .is_none());
    }

    #[test]
    fn adaptive_geometry_accepts_bounded_small_and_large_discord_layouts() {
        for (discord, surface, input) in [
            (
                [0, 0, 640, 400],
                AccessibilityBounds {
                    left: 252,
                    top: 328,
                    right: 628,
                    bottom: 388,
                },
                AccessibilityBounds {
                    left: 288,
                    top: 338,
                    right: 596,
                    bottom: 378,
                },
            ),
            (
                [0, 0, 7_680, 4_320],
                AccessibilityBounds {
                    left: 2_400,
                    top: 4_176,
                    right: 7_600,
                    bottom: 4_288,
                },
                AccessibilityBounds {
                    left: 2_480,
                    top: 4_192,
                    right: 7_520,
                    bottom: 4_272,
                },
            ),
        ] {
            assert!(
                adaptive_geometry_key(
                    discord,
                    surface,
                    input,
                    surface,
                    key_rect(discord, surface),
                    1_000
                )
                .is_some(),
                "bounded Discord geometry should remain adaptable: {discord:?}"
            );
        }
    }

    #[test]
    fn verified_composer_bounds_drive_exact_horizontal_anchor() {
        let target = [0, 0, 1_920, 1_080];
        let composer = AccessibilityBounds {
            left: 336,
            top: 996,
            right: 1_888,
            bottom: 1_052,
        };
        let rect = protected_overlay_rect(target, Some(composer), None).expect("verified composer");
        assert_eq!(rect.x, composer.left);
        assert_eq!(rect.width, (composer.right - composer.left) as u32);
        assert_eq!(rect.y, composer.top);
        assert_eq!(rect.y + rect.height as i32, composer.bottom);
        assert_eq!(rect.height, 56);
    }

    #[test]
    fn both_builds_derive_the_surface_from_exactly_the_same_shape() {
        let target = [0, 0, 1_920, 1_080];
        let composer = AccessibilityBounds {
            left: 336,
            top: 996,
            right: 1_888,
            bottom: 1_052,
        };
        for painted_top in [None, Some(600), Some(-10), Some(1_040)] {
            assert_eq!(
                active_overlay_rect_with_composer(target, Some(composer), painted_top),
                protected_overlay_rect(target, Some(composer), painted_top)
            );
        }
        assert_eq!(active_overlay_rect_with_composer(target, None, None), None);
    }

    #[test]
    fn overlay_rejects_unusable_native_geometry() {
        // A composer that is not inside the lower half of the borrowed window,
        // is too narrow, or is an implausible height is never protected.
        let target = [0, 0, 1_920, 1_080];
        for composer in [
            AccessibilityBounds {
                left: 336,
                top: 400,
                right: 1_888,
                bottom: 456,
            },
            AccessibilityBounds {
                left: 336,
                top: 996,
                right: 600,
                bottom: 1_052,
            },
            AccessibilityBounds {
                left: 336,
                top: 1_040,
                right: 1_888,
                bottom: 1_052,
            },
            AccessibilityBounds {
                left: 336,
                top: 800,
                right: 1_888,
                bottom: 1_052,
            },
        ] {
            assert_eq!(
                protected_overlay_rect(target, Some(composer), None),
                None,
                "unusable composer geometry must not be protected: {composer:?}"
            );
        }
    }

    #[test]
    fn the_eye_owns_the_display_and_nothing_owns_the_composer_away() {
        let target = [0, 0, 1_920, 1_080];
        let composer = composer_bounds(target);
        let painted = [[composer.left + 8, 700, composer.right - 8, 740]];

        // Nothing displayed: exactly the composer, nothing else. Never `None`.
        assert_eq!(
            protected_surface_rect(target, Some(composer), &[]),
            protected_overlay_rect(target, Some(composer), None)
        );
        assert!(protected_surface_rect(target, Some(composer), &[]).is_some());
        // Rows displayed: the composer, grown only over those rows. The eye is
        // still the only thing that decides how far up the surface reaches.
        let grown =
            protected_surface_rect(target, Some(composer), &painted).expect("grown surface");
        assert_eq!(grown.y, 700);
        assert_eq!(grown.y + grown.height as i32, composer.bottom);
        // And the composer's own rows are always part of the answer. A surface
        // that stops above `composer.top` is one the operator cannot type into,
        // which is exactly what the lock-off branch used to return -- or, with
        // nothing painted, `None`, which is the composer off screen entirely.
        assert!(grown.y + grown.height as i32 > composer.top);
    }

    #[test]
    fn the_lock_decides_presence_and_the_geometry_still_does_not() {
        // This test used to assert the opposite of its second half: that the guard
        // "must not read the lock at all". That is what shipped the owner's
        // measured defect -- lock state `off`, click delivered, and the composer
        // still covering Discord's real message box, with zero presence
        // transitions in 600 samples over 1.5 s, because nothing anywhere read the
        // lock. The lock-off IPC path does not hide either: `disengage_lock`
        // answers `true` while the eye is painting rows and its caller then
        // deliberately does nothing, on the understanding that the guard's next
        // pass would stop covering the composer. No pass did.
        //
        // The half that was right stays exactly as it was. The *geometry* must not
        // depend on the lock: `protected_surface_rect` answering "no surface" is a
        // composer that vanishes while the operator is typing into it, which is
        // the defect this file fixed before this one.
        let source = overlay_source();
        let surface = function_body(source, "fn protected_surface_rect(");
        assert!(
            !surface.contains("lock_engaged"),
            "the protected surface must not depend on the lock"
        );
        // Both geometry functions, for the same reason. The band split is a
        // presence answer choosing between two lock-free rectangles, never a
        // rectangle that reads a setting for itself.
        let band = function_body(source, "fn protected_rows_band_rect(");
        assert!(
            !band.contains("lock_engaged"),
            "the rows band must not depend on the lock either"
        );
        // And it is total for a measured composer: there is no arm left that a
        // live session can reach which answers "nothing on screen".
        assert!(
            surface
                .contains("protected_overlay_rect(discord, composer, painted_rows_top(painted))"),
            "the surface is the composer, grown over the painted rows, and nothing else"
        );
        assert!(!surface.contains("painted_rows_bounds(painted)?"));
        // Presence is the other question, and it is the operator's. Answered by
        // one predicate, from the guard, on the pass that reads it -- so a lock
        // that comes down is off the screen within one 16 ms tick rather than at
        // the mercy of whether some other path happened to ask for a hide.
        let guard = function_body(source, "fn start_guard(");
        assert!(
            guard.contains("protected_surface_presence("),
            "the guard is the only writer that can state presence, so it must ask"
        );
        assert_eq!(
            guard.matches("protected_surface_presence(").count(),
            1,
            "one presence rule, one call site: two would be two answers"
        );
        // The eye is still re-resolved, and still not per tick: it is the one
        // thing that legitimately decides what is DISPLAYED over Discord's rows.
        assert!(guard.contains("resolve_protected_display_visible(&app)"));
        assert!(guard.contains("last_protected_display_refresh.elapsed()"));
        assert!(guard.contains(">= PROTECTED_DISPLAY_REFRESH_INTERVAL"));
    }

    /// The product model as a truth table. The lock is encryption only and "the
    /// lock changes nothing about display"; the eye is the only control over
    /// display. All four combinations, because getting any one of them wrong is
    /// either a plaintext leak or a display the operator did not switch off.
    #[test]
    fn the_lock_and_the_eye_each_control_exactly_their_own_half() {
        use ProtectedSurfacePresence::*;

        // Lock on, eye painting: OSL owns Discord's message box and the rows it
        // is painting over. This is the arm that must survive every future
        // "attention" predicate -- an engaged composer whose owner is not iconic
        // stays, whatever else is true of the foreground, the z-order or a drag,
        // because it is "active 100% of the time" and must not flash.
        assert_eq!(
            protected_surface_presence(false, true, true),
            ComposerAndRows
        );
        // Lock on, eye off: still the composer, because the operator's plaintext
        // may never enter Discord's real box. Nothing is painted and, downstream,
        // nothing is shielded -- but presence is unchanged, which is the whole of
        // "the eye does not decide whether you have somewhere to type".
        assert_eq!(
            protected_surface_presence(false, true, false),
            ComposerAndRows
        );
        // Lock off, eye painting: the one new answer. Discord's message box is
        // handed back -- the lock is down, so the keystrokes are meant to reach it
        // in the clear -- and the eye keeps its rows, because the lock does not
        // get a vote on display. Taking the whole pair off screen here is the bug
        // this arm exists to prevent.
        assert_eq!(protected_surface_presence(false, false, true), RowsOnly);
        // Lock off, eye off: nothing encrypted, nothing displayed, nothing on
        // screen. No shield, no composer, plain native Discord.
        assert_eq!(protected_surface_presence(false, false, false), OffScreen);

        // And the owner's window, which outranks all of it: a surface floating
        // over a minimized application is orphaned, not active.
        for lock in [true, false] {
            for rows in [true, false] {
                assert_eq!(
                    protected_surface_presence(true, lock, rows),
                    OffScreen,
                    "a minimized owner leaves nothing on screen (lock {lock}, rows {rows})"
                );
            }
        }

        // The same four combinations as rectangles. The eye off means
        // `painted_message_row_rects` answers empty, and empty means no shield at
        // all -- the shield is placed from `painted_rows_bounds`, which then has no
        // rectangle to give -- whatever the lock says.
        let target = [0, 0, 1_920, 1_080];
        let composer = composer_bounds(target);
        let painted = [[composer.left + 8, 700, composer.right - 8, 740]];
        assert_eq!(
            painted_rows_bounds(&[]),
            None,
            "the eye off must leave the shield with nothing to cover"
        );
        // Lock on, eye off: the composer exactly, and no shield.
        let engaged_eye_off =
            protected_presence_surface_rect(ComposerAndRows, target, Some(composer), &[])
                .expect("an engaged composer is present with the eye off");
        assert_eq!(engaged_eye_off.y, composer.top);
        assert_eq!(
            engaged_eye_off.y + engaged_eye_off.height as i32,
            composer.bottom
        );
        // Lock on, eye on: the same composer, grown over the rows and nothing more.
        let engaged_eye_on =
            protected_presence_surface_rect(ComposerAndRows, target, Some(composer), &painted)
                .expect("an engaged composer grows over what the eye paints");
        assert_eq!(engaged_eye_on.y, 700);
        assert_eq!(
            engaged_eye_on.y + engaged_eye_on.height as i32,
            composer.bottom
        );
        // Lock off, eye on: the rows, and provably not the message box.
        let surrendered =
            protected_presence_surface_rect(RowsOnly, target, Some(composer), &painted)
                .expect("the eye keeps its rows with the lock down");
        assert_eq!(surrendered.y, 700);
        assert_eq!(surrendered.y + surrendered.height as i32, composer.top);
        // Lock off, eye off: no rectangle anywhere.
        assert_eq!(
            protected_presence_surface_rect(OffScreen, target, Some(composer), &[]),
            None
        );
    }

    /// The safety property, proved by geometry rather than by intent: with the
    /// lock off, Discord's real message box is not covered by *anything* OSL owns.
    ///
    /// Both windows are checked. The shield is placed from the painted rows, so a
    /// surface clamped above the box with rows that still reach into it is the
    /// same leak with a different window on top of it.
    #[test]
    fn a_surrendered_composer_band_leaves_discords_message_box_uncovered() {
        use ProtectedSurfacePresence::*;
        let target = [0, 0, 1_920, 1_080];
        let composer = composer_bounds(target);
        // One row well above the box, and one that reaches down into it -- which
        // is what a partially scrolled transcript looks like.
        let painted = [
            [composer.left + 8, 700, composer.right - 8, 740],
            [
                composer.left + 8,
                composer.top - 10,
                composer.right - 8,
                composer.top + 30,
            ],
        ];

        // Lock on: OSL owns the box, so the surface still reaches its bottom edge.
        let engaged =
            protected_presence_surface_rect(ComposerAndRows, target, Some(composer), &painted)
                .expect("engaged surface");
        assert_eq!(engaged.y + engaged.height as i32, composer.bottom);

        // Lock off: clamp the rows first, exactly as the guard does, then derive.
        let rows = rows_above_the_composer_band(&painted, composer.top);
        let surrendered = protected_presence_surface_rect(RowsOnly, target, Some(composer), &rows)
            .expect("rows band");
        // The surface stops exactly at the top of Discord's box. `y + height` is
        // exclusive, so the last row it covers is `composer.top - 1`.
        assert_eq!(surrendered.y + surrendered.height as i32, composer.top);
        assert!(
            surrendered.y < composer.top,
            "the rows band must cover rows"
        );
        // Zero overlap with the box, stated as the overlap itself rather than as
        // an inequality that could be read the other way round.
        let overlap = (surrendered.y + surrendered.height as i32).min(composer.bottom)
            - surrendered.y.max(composer.top);
        assert!(
            overlap <= 0,
            "the protected surface still overlaps Discord's message box by {overlap}px"
        );
        // And the shield, placed from the same rows, is inside the same band.
        let shield = painted_rows_bounds(&rows).expect("shield bounds");
        assert!(
            shield.y + shield.height as i32 <= composer.top,
            "the capture shield still reaches into Discord's message box"
        );
        // The row that straddled the boundary was clipped, not dropped: the eye
        // keeps painting the part of it that is above the box.
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|row| row[3] <= composer.top));
        // A row entirely inside the box is dropped outright, and a band with
        // nothing left above the box is "nothing to be on screen for" rather than
        // a rectangle over the box.
        let inside = [[
            composer.left + 8,
            composer.top + 4,
            composer.right - 8,
            composer.bottom,
        ]];
        assert!(rows_above_the_composer_band(&inside, composer.top).is_empty());
        assert_eq!(
            protected_presence_surface_rect(RowsOnly, target, Some(composer), &[]),
            None
        );
        // Off screen owns no rectangle at all, whatever is painted.
        assert_eq!(
            protected_presence_surface_rect(OffScreen, target, Some(composer), &painted),
            None
        );
    }

    /// The presence rule has to be asked before the pass can decide to keep the
    /// composer, or the two cheap paths would hold a switched-off composer on
    /// screen for the whole of their budget.
    #[test]
    fn a_switched_off_composer_cannot_be_held_on_screen_by_a_cheap_pass() {
        let guard = function_body(overlay_source(), "fn start_guard(");
        let presence = guard
            .find("protected_surface_presence(")
            .expect("the presence rule");
        let steady = guard
            .find("let presentation_is_undisturbed = ready")
            .expect("the read-only fast path");
        let drag = guard
            .find("translated_drag_placement(")
            .expect("the drag fast path");
        assert!(
            presence < steady && presence < drag,
            "presence is decided before either path can answer 'nothing to do'"
        );
        // And the focus reclaim, which runs before all three, may not hand the
        // keyboard to a composer that is on its way off screen: with the lock down
        // those keystrokes belong to Discord.
        let reclaim = guard
            .find("let focus_reclaim_pending = last_overlay_rect.is_some_and(|rect| {")
            .expect("the reclaim gate");
        let reclaim_body = &guard[reclaim..presence];
        assert!(
            reclaim_body.contains("overlay_state.lock_engaged()"),
            "a switched-off composer must not reclaim the caret"
        );
        // The budget is shared by both cheap paths, so neither can drift into
        // being the slow one.
        assert_eq!(PROTECTED_FULL_GUARD_BUDGET, Duration::from_millis(400));
        assert_eq!(
            guard.matches("PROTECTED_FULL_GUARD_BUDGET").count(),
            1,
            "one budget, read once, shared by both cheap paths"
        );
    }

    /// The guard's half of the band split: every rectangle it writes comes from
    /// the presence answer, and the row clamp is applied once, before anything
    /// downstream can read the unclamped set.
    #[test]
    fn the_guard_derives_every_rectangle_from_the_presence_answer() {
        let source = overlay_source();
        let guard = strip_line_comments(function_body(source, "fn start_guard("));
        // Two derivations, both through the one mapping: the surface this pass
        // measures against, and the rectangle it actually writes. Calling
        // `protected_surface_rect` directly here would be a pass that covers
        // Discord's message box with the lock down.
        assert_eq!(guard.matches("protected_presence_surface_rect(").count(), 2);
        assert!(
            !guard.contains("protected_surface_rect("),
            "the guard must not reach past the presence mapping to a fixed geometry"
        );
        assert!(!guard.contains("protected_rows_band_rect("));
        // The clamp is applied before the surface is derived from the rows, and the
        // shield is placed from the same clamped set -- a surface clamped above the
        // message box with a shield still reaching into it is the same leak.
        let clamp = guard
            .find("rows_above_the_composer_band(&painted_rows, composer_bounds.top)")
            .expect("the rows must be clamped to the surrendered band");
        let derive = guard
            .find("let Some(current_overlay_rect) = protected_presence_surface_rect(")
            .expect("the surface derivation");
        assert!(clamp < derive, "the clamp must precede every reader");
        // Re-clamped at the write against the bounds that write is issued with: a
        // re-sample republishes the composer rectangle, and a clamp taken against
        // the rectangle the pass opened with could leave the shield a few pixels
        // inside the box.
        assert!(guard.contains("rows_above_the_composer_band(&painted_rows, placed_bounds.top)"));
        assert!(
            guard.contains("position_window_pair(&window, &shield, placed_rect, &placed_rows)?;")
        );
        // Presence is decided from the operator's three facts and nothing else.
        assert!(guard.contains("overlay_state.lock_engaged(),"));
        assert!(guard.contains("overlay_state.protected_rows_painted(),"));
        assert!(guard.contains("owner_window_is_minimized(&main)"));
        // The renderer is told, on the edges only, that the window it draws into no
        // longer covers a message box -- and re-told on every reveal, because a
        // retained WebView that missed the last edge would otherwise draw a
        // composer over Discord's transcript for the rest of the session.
        assert_eq!(guard.matches("OVERLAY_COMPOSER_BAND_EVENT").count(), 1);
        assert!(guard.contains("OVERLAY_LABEL,\n                            OVERLAY_COMPOSER_BAND_EVENT,\n                            band_surrendered,"));
        assert!(guard.contains("if last_band_surrendered != Some(band_surrendered) {"));
        assert_eq!(guard.matches("last_band_surrendered = None;").count(), 2);
        // And a surrendered band never takes the keyboard: those keystrokes are
        // Discord's, which is the whole point of handing the box back.
        assert!(
            guard.contains("if active_overlay_requires_focus_acquisition() && !band_surrendered {")
        );
    }

    #[test]
    fn painted_message_row_rects_production_reads_the_cached_rehydrated_rectangles() {
        let source = overlay_source();
        let painted = function_body(source, "fn painted_message_row_rects(");
        let qa = painted
            .split("#[cfg(feature = \"discord-qa-shell\")]")
            .nth(1)
            .expect("QA branch")
            .split("#[cfg(not(feature = \"discord-qa-shell\"))]")
            .next()
            .expect("QA branch body");
        // The QA branch keeps its own just-sent carriers AND adds the rows the
        // transcript reader found. Returning only the carriers is what made a
        // history row impossible to paint in the one build the owner tests in.
        assert!(qa.contains("verified_sent_carriers(&scope_binding, generation)"));
        assert!(qa.contains("rehydrated_row_rects(&scope_binding, generation)"));
        assert!(qa.contains("union_painted_row_rects("));
        let production = painted
            .split("#[cfg(not(feature = \"discord-qa-shell\"))]")
            .nth(1)
            .expect("production branch");
        // Production must no longer discard `app` and `generation` and return
        // nothing on every tick: it must resolve the scope and ask the adapter's
        // cache for the rectangles the last rehydration actually read.
        assert!(!production.contains("let _ = (app, generation);"));
        assert!(production.contains("rehydrated_row_rects(&scope_binding, generation)"));
    }

    #[test]
    fn painted_rows_union_keeps_history_rows_and_counts_a_shared_row_once() {
        // A row this session sent, measured by the QA shell's own carrier path.
        let sent = [100, 300, 900, 340];
        // The same row as the transcript reader measured it a moment later --
        // one pixel off, which equality would treat as a second row and paint
        // twice.
        let sent_again = [100, 301, 900, 341];
        // A history row this client never sent. This is the one the QA build
        // used to drop on the floor, which kept the overlay window from ever
        // being grown over it.
        let history = [100, 200, 900, 240];
        let merged = union_painted_row_rects(vec![sent], vec![sent_again, history]);
        assert_eq!(merged, vec![sent, history]);
        // And the union is what decides how far the overlay window has to grow:
        // carriers alone stop at the sent row, so the history row above it could
        // never be inside the protected surface.
        assert_eq!(painted_rows_top(&[sent]), Some(300));
        assert_eq!(painted_rows_top(&merged), Some(200));
        let bounds = painted_rows_bounds(&merged).expect("union bounds");
        assert_eq!(bounds.y, 200);
        assert_eq!(bounds.height, 140);
    }

    #[test]
    fn painted_rows_union_drops_degenerate_rectangles_from_either_source() {
        let real = [10, 10, 110, 50];
        let inverted = [110, 50, 10, 10];
        let empty = [10, 10, 10, 50];
        assert_eq!(
            union_painted_row_rects(vec![inverted, real], vec![empty]),
            vec![real]
        );
        assert!(union_painted_row_rects(Vec::new(), Vec::new()).is_empty());
    }

    #[test]
    fn painted_rows_union_keeps_two_rows_that_merely_touch() {
        // Adjacent transcript rows share an edge, and a strip of a taller row
        // can clip its neighbour. Neither is the same row, so neither may be
        // dropped -- the shield would stop covering a row it is painting over.
        let upper = [100, 200, 900, 240];
        let lower = [100, 240, 900, 280];
        let clipping = [100, 238, 900, 280];
        assert_eq!(
            union_painted_row_rects(vec![upper], vec![lower]),
            vec![upper, lower]
        );
        assert_eq!(
            union_painted_row_rects(vec![upper], vec![clipping]),
            vec![upper, clipping]
        );
        assert!(!same_painted_row(upper, lower));
        assert!(!same_painted_row(upper, clipping));
    }

    #[test]
    fn the_transcript_band_signal_is_an_edge_and_never_fires_per_tick() {
        let band = |y: i32| {
            Some(OverlayRect {
                x: 100,
                y,
                width: 800,
                height: 140,
            })
        };
        let mut tracker = TranscriptBandTracker::new(None);
        // First painted band: one emit.
        assert!(tracker.observe(band(200), true, true));
        // Then a hundred identical ticks. This is the whole point: a 16 ms
        // drumbeat of IPC at the protected renderer is what once froze this app,
        // so an unchanged band must answer false every single time.
        for _ in 0..100 {
            assert!(!tracker.observe(band(200), true, true));
        }
        // Scrolling moves the band: one emit, then silence again.
        assert!(tracker.observe(band(140), true, true));
        assert!(!tracker.observe(band(140), true, true));
        // The eye going off empties the band -- reported once, not repeatedly.
        assert!(tracker.observe(None, true, false));
        assert!(!tracker.observe(None, true, false));
    }

    #[test]
    fn a_discord_drag_emits_once_when_it_stops_not_on_every_tick() {
        let band = Some(OverlayRect {
            x: 100,
            y: 200,
            width: 800,
            height: 140,
        });
        let mut tracker = TranscriptBandTracker::new(None);
        assert!(tracker.observe(band, true, true));
        // A drag: Discord's rectangle is unsettled for as long as it lasts. The
        // cached row rectangles do not move with it, so the band's bounds are
        // unchanged for every one of these ticks and nothing may be emitted.
        for _ in 0..60 {
            assert!(!tracker.observe(band, false, true));
        }
        // It stops. Exactly one emit, on the tick it comes to rest.
        assert!(tracker.observe(band, true, true));
        assert!(!tracker.observe(band, true, true));
        // With nothing painted, a Discord move has no band to have moved.
        let mut idle = TranscriptBandTracker::new(None);
        assert!(!idle.observe(None, false, false));
        assert!(!idle.observe(None, true, false));
    }

    #[test]
    fn focus_acquisition_never_synthesizes_input_or_moves_the_cursor() {
        let source = overlay_source();
        // The operator has twice objected to cursor theft, and the
        // `keybd_event` foreground-lock trick is the exact shape of it. None of
        // these may appear in this file at all -- not in the focus path, not
        // anywhere.
        let code = strip_line_comments(source);
        for forbidden in ["SendInput", "keybd_event", "mouse_event", "SetCursorPos"] {
            assert!(
                !code.contains(forbidden),
                "focus must never be taken by synthesizing input"
            );
        }
        let focus = function_body(source, "fn active_focus_overlay(");
        assert!(focus.contains("ForegroundInputAttachment::acquire()"));
        // Success is a read-back, never `SetForegroundWindow`'s return value.
        assert!(focus.contains("overlay_root_is_foreground(expected_root)"));
        assert!(!focus.contains("SetForegroundWindow(hwnd) } == 0"));
    }

    #[test]
    fn the_input_attachment_is_detached_by_drop_and_only_by_drop() {
        let source = overlay_source();
        // Exactly two calls exist: the attach in `acquire` and the detach in
        // `drop`. A third would be a hand-written detach, which is the one that
        // an early return, a `?` or a panic can skip -- and a skipped detach
        // couples OSL's input queue to Discord's for the life of the process.
        assert_eq!(source.matches("AttachThreadInput(").count(), 2);
        let at = source
            .find("impl Drop for ForegroundInputAttachment")
            .expect("the attachment must release itself");
        let dropped = function_body(&source[at..], "fn drop(");
        assert!(dropped.contains("AttachThreadInput(self.foreground_thread, self.this_thread, 0)"));
        // And the attachment is a value with a lifetime, not a pair of calls:
        // it is bound in the retry loop so it is released before the wait.
        let focus = function_body(source, "fn active_focus_overlay(");
        assert!(focus.contains("let _attachment = ForegroundInputAttachment::acquire();"));
    }

    #[test]
    fn a_refused_focus_is_reported_once_and_retired_once() {
        // The composer stays on screen either way; what must not happen is
        // silence. Edge-only, so a refusal that persists is one message rather
        // than one per open attempt.
        let state = OverlaySessionState::default();
        assert!(!state.protected_focus_refused());
        assert!(state.set_protected_focus_refused(true));
        assert!(!state.set_protected_focus_refused(true));
        assert!(state.protected_focus_refused());
        assert!(state.set_protected_focus_refused(false));
        assert!(!state.set_protected_focus_refused(false));
        assert!(!state.protected_focus_refused());
    }

    /// "The composer is unreachable" is one question with two causes, so the wire
    /// has to carry the answer and the cause -- not two independent edges the only
    /// reader has to reconcile by counting.
    #[test]
    fn composer_unreachability_is_one_level_with_two_named_causes() {
        let state = OverlaySessionState::default();
        assert!(!state.composer_is_unreachable());

        // Either cause alone raises the aggregate.
        state.set_composer_zorder_surrendered(true);
        assert!(state.composer_is_unreachable());
        // The second cause raising on top of the first changes nothing the operator
        // can see -- which is exactly why the aggregate, and not either latch's own
        // edge, is what may be announced.
        state.set_protected_focus_refused(true);
        assert!(state.composer_is_unreachable());
        // And one cause retracting while the other still holds must NOT clear it.
        // The renderer used to have to infer this by counting raises against
        // retractions, correct only for as long as both setters stayed edge-only.
        state.set_composer_zorder_surrendered(false);
        assert!(
            state.composer_is_unreachable(),
            "a composer that still cannot receive the keyboard is still unreachable"
        );
        state.set_protected_focus_refused(false);
        assert!(!state.composer_is_unreachable());

        // Teardown. Both latches are app-lifetime atomics and both setters are
        // edge-only, so a session that ended while one was raised used to poison
        // the next one: the next raise found the value already there, reported no
        // edge, emitted nothing, and the hub showed no warning while the composer
        // really was unreachable.
        state.set_protected_focus_refused(true);
        state.set_composer_zorder_surrendered(true);
        state.clear();
        assert!(!state.composer_is_unreachable());
        assert!(!state.protected_focus_refused());
        assert!(!state.composer_zorder_surrendered());
        // Which is what makes the next session's first refusal an edge again.
        assert!(state.set_protected_focus_refused(true));

        // And a fresh session may not inherit a latch either, for the same reason.
        let _ = state.activate("ctx-unreachable".to_owned(), test_host("unreachable"));
        assert!(!state.composer_is_unreachable());
    }

    /// The two reporters must publish the same aggregate through the same writer,
    /// or the discriminated payload is two contracts wearing one event name.
    #[test]
    fn both_unreachability_reporters_publish_the_aggregate_with_their_own_reason() {
        let source = overlay_source();
        // Exactly one emitter of this event on the raise/retract path, plus the
        // teardown retraction, and nothing else may name it: an independent
        // `emit_to(.., OVERLAY_COMPOSER_UNREACHABLE_EVENT, ..)` is how the payload
        // became a bare bool with no reason on it in the first place.
        assert_eq!(
            source.matches("OVERLAY_COMPOSER_UNREACHABLE_EVENT").count(),
            2,
            "the declaration and exactly one writer: a second emitter is how this \
             payload became a bare bool with no reason on it"
        );
        let publish = function_body(source, "fn publish_composer_unreachable(");
        // The level, computed from both latches, and only on the aggregate's edge.
        assert!(publish.contains("composer_is_unreachable()"));
        assert!(publish.contains("if unreachable == was_unreachable {"));
        assert!(publish.contains("reason,"));
        assert!(publish.contains("unreachable,"));

        for (reporter, reason) in [
            (
                "fn report_composer_zorder_surrender(",
                "COMPOSER_UNREACHABLE_ZORDER_BAND",
            ),
            (
                "fn report_protected_focus_refused(",
                "COMPOSER_UNREACHABLE_KEYBOARD_FOCUS",
            ),
        ] {
            let body = function_body(source, reporter);
            // The aggregate has to be sampled BEFORE the latch is written, or the
            // "was it already unreachable" question answers with the value this
            // call just stored and every edge looks like a no-op.
            let before = body
                .find("let was_unreachable = state.composer_is_unreachable();")
                .unwrap_or_else(|| panic!("{reporter} must sample the aggregate first"));
            let write = body
                .find(".set_")
                .unwrap_or_else(|| panic!("{reporter} must write its own latch"));
            assert!(before < write, "{reporter} sampled the aggregate too late");
            assert!(
                body.contains(&format!(
                    "publish_composer_unreachable(app, {reason}, was_unreachable)"
                )),
                "{reporter} must name its own cause"
            );
        }

        // The teardown retraction clears the latches and then announces, so the
        // level it publishes is the cleared one.
        let retract =
            strip_line_comments(function_body(source, "fn retract_composer_unreachable("));
        let cleared = retract
            .find("clear_composer_unreachable_latches()")
            .expect("teardown must clear both latches");
        let announced = retract
            .find("publish_composer_unreachable(app, COMPOSER_UNREACHABLE_SESSION_ENDED")
            .expect("teardown must announce the retraction");
        assert!(cleared < announced);
        // Reached from every path that ends or parks a session, and before the
        // state is cleared -- `clear` resets the latches, so a retraction issued
        // after it would find nothing to retract.
        let teardown = function_body(source, "pub(crate) fn clear_and_hide(");
        let retracted = teardown
            .find("retract_composer_unreachable(app)")
            .expect("session teardown must retract");
        let clear = teardown
            .find("OverlaySessionState>().clear()")
            .expect("session teardown must clear");
        assert!(retracted < clear);

        // Every reason is a fixed string. Nothing on this wire may be derived from
        // a draft, a conversation, a token or an identity.
        for reason in [
            COMPOSER_UNREACHABLE_ZORDER_BAND,
            COMPOSER_UNREACHABLE_KEYBOARD_FOCUS,
            COMPOSER_UNREACHABLE_SESSION_ENDED,
        ] {
            assert!(!reason.is_empty());
            assert!(reason.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        }
    }

    /// The z-order latch's reclaim path: the only writer reports both edges from
    /// one computed value, so a surrender that becomes reorderable again is
    /// retracted rather than needing a restart to clear.
    #[test]
    fn a_zorder_surrender_can_be_retracted_without_restarting() {
        let raise = strip_line_comments(function_body(
            overlay_source(),
            "fn raise_protected_composer_above_discord(",
        ));
        // One call, given the computed value -- not `if surrendered { report(true) }`,
        // which is the shape that can only ever latch on.
        assert_eq!(
            raise
                .matches("report_composer_zorder_surrender(overlay.app_handle(), surrendered)")
                .count(),
            1
        );
        assert!(!raise.contains("report_composer_zorder_surrender(overlay.app_handle(), true)"));
        // And it is computed fresh on every call, from the two windows' current
        // bands, so the retraction happens on the same probe cadence as the raise.
        let computed = raise
            .find("let surrendered = !composer_raise_is_a_same_band_reorder(")
            .expect("the surrender must be re-derived, not remembered");
        assert!(
            computed
                < raise
                    .find("report_composer_zorder_surrender(")
                    .expect("the report")
        );
        // Both edges are also proved on the latch itself.
        let state = OverlaySessionState::default();
        assert!(state.set_composer_zorder_surrendered(true));
        assert!(!state.set_composer_zorder_surrendered(true));
        assert!(state.set_composer_zorder_surrendered(false));
        assert!(!state.composer_zorder_surrendered());

        // And the report is gated on the lock, which is the third retraction path
        // and the one that stops the leak warning from crying wolf: with the band
        // surrendered on purpose there is no composer for Discord to be above, so
        // "your typing is going to Discord, not OSL" would be telling the operator
        // their own setting is a leak.
        let reporter = strip_line_comments(function_body(
            overlay_source(),
            "fn report_composer_zorder_surrender(",
        ));
        let gated = reporter
            .find("let surrendered = surrendered && state.lock_engaged();")
            .expect("a surrendered band is only a hazard while the lock owns it");
        assert!(
            gated
                < reporter
                    .find(".set_composer_zorder_surrendered(")
                    .expect("the latch write"),
            "the lock gate must be applied before the latch is written"
        );
    }

    #[test]
    fn a_refused_focus_never_ends_the_session_and_never_claims_the_caret() {
        let guard = function_body(overlay_source(), "fn start_guard(");
        let open = guard
            .find("let focus_acquired = active_focus_overlay(&window).is_ok();")
            .expect("the first-open focus acquisition");
        // Self-delimiting: the branch runs from the acquisition to the refocus
        // announcement that ends it, so this cannot accidentally read the
        // fail-closed identity checks that follow the block.
        let after = &guard[open..];
        let refocus = after
            .find("OVERLAY_REFOCUS_EVENT")
            .expect("the open path still announces a proven focus");
        let block = &after[..refocus];
        // The old `return Err(error)` here is what `clear_and_hide`s the pair:
        // engaging from Discord's own message box refused the foreground, tore
        // the session down, and left the operator typing into Discord with no
        // composer on screen to be missing.
        assert!(
            !block.contains("return Err"),
            "a refused foreground must not end the session"
        );
        assert!(block.contains("report_protected_focus_refused(&app, !focus_acquired)"));
        // And the refocus event -- which tells the renderer to put the caret in
        // the draft -- may only follow a focus that was actually proven.
        assert!(block.contains("if focus_acquired {"));
    }

    #[cfg(feature = "discord-qa-shell")]
    #[test]
    fn the_qa_overlay_window_stage_trail_appends_instead_of_truncating() {
        // The trail was `std::fs::write`, so a later pass's label erased the one
        // before it and the focus outcome of an open could not be read back at
        // all. Every label here is a fixed `&'static str` chosen at the call
        // site -- there is no path by which draft or conversation content can
        // reach this file.
        let path = std::env::temp_dir().join("osl-discord-qa-overlay-window-stage.txt");
        let _ = std::fs::remove_file(&path);
        qa_overlay_window_stage("unit_test_stage_alpha");
        // Repeats are collapsed: several of these labels are restated on every
        // 16 ms guard pass, and an append per tick would be unbounded growth and
        // a write syscall on the guard thread.
        qa_overlay_window_stage("unit_test_stage_alpha");
        qa_overlay_window_stage("unit_test_stage_beta");
        qa_overlay_window_stage("unit_test_stage_alpha");
        let trail = std::fs::read_to_string(&path).expect("the QA stage trail");
        assert_eq!(
            trail.lines().collect::<Vec<_>>(),
            vec![
                "unit_test_stage_alpha",
                "unit_test_stage_beta",
                "unit_test_stage_alpha"
            ]
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn the_rows_moved_signal_is_emitted_from_the_guard_with_no_payload() {
        let guard = function_body(overlay_source(), "fn start_guard(");
        assert!(guard.contains("transcript_band.observe("));
        assert!(guard.contains("app.emit_to(OVERLAY_LABEL, NATIVE_DISCORD_ROWS_MOVED_EVENT, ())"));
        // The emit is reachable only through the edge detector; a second emit
        // site anywhere in this loop is how it becomes the per-tick shape.
        let emits = guard
            .matches("emit_to(OVERLAY_LABEL, NATIVE_DISCORD_ROWS_MOVED_EVENT")
            .count();
        assert_eq!(emits, 1);
        assert_eq!(
            NATIVE_DISCORD_ROWS_MOVED_EVENT,
            "osl://native-discord-rows-moved"
        );
    }

    #[test]
    fn navigation_is_bundled_overlay_only() {
        assert!(bundled_overlay_navigation(
            &url::Url::parse("tauri://localhost/overlay.html").unwrap()
        ));
        assert!(!bundled_overlay_navigation(
            &url::Url::parse("tauri://localhost/index.html").unwrap()
        ));
        assert!(!bundled_overlay_navigation(
            &url::Url::parse("https://discord.com/overlay.html").unwrap()
        ));
        assert!(!bundled_overlay_navigation(
            &url::Url::parse("tauri://localhost/overlay.html?token=secret").unwrap()
        ));
        assert!(bundled_shield_navigation(
            &url::Url::parse("tauri://localhost/shield.html").unwrap()
        ));
        assert!(!bundled_shield_navigation(
            &url::Url::parse("https://discord.com/shield.html").unwrap()
        ));
    }

    #[test]
    fn adaptive_surface_identity_ignores_only_volatile_foreground_state() {
        let expected = NativeDiscordOverlayTarget {
            generation: 7,
            window: 11,
            rect: [100, 100, 900, 700],
            foreground: false,
            trusted_parent: 13,
        };
        assert!(same_native_surface_target(
            NativeDiscordOverlayTarget {
                foreground: true,
                ..expected
            },
            expected,
        ));
        assert!(!same_native_surface_target(
            NativeDiscordOverlayTarget {
                generation: 8,
                ..expected
            },
            expected,
        ));
    }

    #[test]
    fn shield_must_be_immediately_behind_overlay() {
        assert!(exact_shield_stack(10, 20, 20, 10));
        assert!(!exact_shield_stack(10, 20, 30, 10));
        assert!(!exact_shield_stack(10, 20, 20, 30));
        assert!(!exact_shield_stack(10, 10, 10, 10));
    }

    #[test]
    fn first_guard_waits_hidden_without_a_foreground_desktop() {
        assert_eq!(
            first_guard_decision(false, false, false, true),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(false, false, false, false),
            FirstGuardDecision::Reveal
        );
    }

    #[test]
    fn first_guard_reveals_only_for_discord_or_osl_focus() {
        assert_eq!(
            first_guard_decision(true, true, false, true),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(true, false, true, false),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
    }

    #[test]
    fn foreign_focus_can_only_wait_hidden_during_startup_grace() {
        assert_eq!(
            first_guard_decision(true, false, false, true),
            FirstGuardDecision::WaitHidden
        );
        assert_eq!(
            first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
    }

    #[test]
    fn no_steady_state_foreground_predicate_may_come_back() {
        // `trusted_focus_state` / `active_trusted_focus_state` were the
        // steady-state answer to "does a trusted window hold the foreground",
        // and a ready session used to hide the composer on it -- which is the
        // composer disappearing every time the operator touched another app.
        // The predicate and its only caller are both gone; this is what stops
        // either coming back. (`overlay_source()` splits this module off, so
        // these needles cannot satisfy themselves.)
        let source = overlay_source();
        assert!(!source.contains("fn trusted_focus_state("));
        assert!(!source.contains("fn active_trusted_focus_state("));
        // The FIRST-OPEN foreground policy is a different question and stays:
        // refusing to open over a third app is not the same as taking an
        // already-open composer away.
        assert_eq!(
            first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
        assert_eq!(
            first_guard_decision(true, true, false, false),
            FirstGuardDecision::Reveal
        );
        assert_eq!(
            first_guard_decision(true, false, true, false),
            FirstGuardDecision::Reveal
        );
    }

    #[test]
    fn composer_focus_is_reclaimed_only_for_ready_exact_discord_under_cursor() {
        assert!(should_reclaim_composer_focus(true, true, false, true));
        assert!(!should_reclaim_composer_focus(false, true, false, true));
        assert!(!should_reclaim_composer_focus(true, false, false, true));
        assert!(!should_reclaim_composer_focus(true, true, true, true));
        assert!(!should_reclaim_composer_focus(true, true, false, false));
    }

    #[test]
    fn composer_focus_reclaim_attempts_are_bounded() {
        assert!(!focus_reclaim_attempt_due(false, Duration::from_secs(1)));
        assert!(!focus_reclaim_attempt_due(true, Duration::from_millis(249)));
        assert!(focus_reclaim_attempt_due(true, Duration::from_millis(250)));
    }

    #[test]
    fn first_open_retries_are_bounded_and_never_stall() {
        // A session that has never revealed anything must retry discovery on
        // a bounded cadence -- never on every 16 ms tick, and never so slowly
        // that a legitimate first open feels delayed. Regressing this back to
        // an unconditional per-tick retry is exactly what let a slow
        // accessibility query into Discord overlap with OSL's own
        // focus-change handling on every single guard tick.
        assert!(!first_open_attempt_due(Duration::from_millis(0)));
        assert!(!first_open_attempt_due(Duration::from_millis(249)));
        assert!(first_open_attempt_due(Duration::from_millis(250)));
        assert!(first_open_attempt_due(Duration::from_secs(5)));
        // The bound must stay finite and small: an implementation that grew
        // this unboundedly (or dropped it entirely by always returning true)
        // would still pass a naive "it eventually becomes due" check, so pin
        // the exact interval instead.
        assert_eq!(FIRST_OPEN_RETRY_INTERVAL, Duration::from_millis(250));
    }

    #[test]
    fn qa_remeasures_periodically_without_refreshing_during_send() {
        assert!(!qa_should_refresh_composer_bounds(
            true, true, true, false, false
        ));
        assert!(qa_should_refresh_composer_bounds(
            true, true, false, false, false
        ));
        assert!(qa_should_refresh_composer_bounds(
            true, true, true, true, false
        ));
        // A carrier in flight must SUPPRESS the re-measure, never cause one: this
        // assertion used to demand the opposite of the test's own name.
        assert!(!qa_should_refresh_composer_bounds(
            true, true, true, false, true
        ));
        assert!(qa_should_refresh_composer_bounds(
            false, true, true, false, false
        ));
    }

    #[test]
    fn a_carrier_in_flight_never_re_measures_the_composer_it_is_typing_into() {
        // Regression for the stranded ~117-character carrier that refused at
        // `place_refused_enter_focus`: the guard was re-measuring the composer
        // mid-placement, which broke the `may_continue_input` proof between the
        // carrier and its Enter, and the same broken proof then blocked the
        // cleanup keystrokes, so the strand persisted into every later attempt.
        //
        // In flight wins over every other input, in both the QA predicate and the
        // active one, so no combination of readiness, foreground, settle state or
        // recovery can reintroduce it.
        for ready in [false, true] {
            for osl_foreground in [false, true] {
                for settled in [false, true] {
                    for recovering in [false, true] {
                        assert!(
                            !qa_should_refresh_composer_bounds(
                                ready,
                                osl_foreground,
                                settled,
                                recovering,
                                true
                            ),
                            "a carrier in flight must never re-measure the composer"
                        );
                        for allowed in [false, true] {
                            for periodic in [false, true] {
                                assert!(
                                    !active_should_refresh_composer_bounds(
                                        ready,
                                        osl_foreground,
                                        settled,
                                        recovering,
                                        true,
                                        allowed,
                                        periodic
                                    ),
                                    "no refresh path may fire while a carrier is in flight"
                                );
                            }
                        }
                    }
                }
            }
        }
        // Not in flight: unchanged. A settled, foreground, ready, non-recovering
        // pass still measures nothing, and each of the real triggers still fires.
        assert!(!qa_should_refresh_composer_bounds(
            true, true, true, false, false
        ));
        assert!(qa_should_refresh_composer_bounds(
            true, true, false, false, false
        ));
        assert!(qa_should_refresh_composer_bounds(
            true, true, true, true, false
        ));
        assert!(qa_should_refresh_composer_bounds(
            false, true, true, false, false
        ));
    }

    #[test]
    fn the_composer_keeps_its_per_pixel_alpha_across_every_frame_change() {
        let source = overlay_source();
        // tao enables the blur-behind region once, at creation, and nothing in
        // tao/wry/Tauri ever re-applies it. Every writer that makes DWM
        // re-evaluate this window's frame can therefore leave it opaque, and an
        // opaque protected window renders every pixel it does not paint as
        // WebView2's white default backing.
        let contract = function_body(source, "fn apply_protected_frame_contract(");
        let border = contract
            .find("suppress_accent_border")
            .expect("the contract suppresses the accent border");
        let transparency = contract
            .find("enforce_transparent_protected_composer(window)")
            .expect("the contract must restore the alpha its own writers can clear");
        // Last writer wins: the border suppression is one of the two things that
        // can disable DWM rendering for this window.
        assert!(border < transparency);
        // The shield is opaque black on purpose and must never be handed this.
        assert!(contract.contains("if matches!(surface, ProtectedSurface::Composer) {"));

        // A reveal is a frame change too: Tauri re-applies cached flags inside it.
        let reveal = function_body(source, "fn reveal_protected_pair(");
        let stripped = reveal
            .find("enforce_native_frameless_overlay(window)")
            .expect("the reveal strips the frame it restores");
        let restored = reveal
            .find("enforce_transparent_protected_composer(window)")
            .expect("the reveal must restore the alpha it can clear");
        assert!(stripped < restored);

        // Frequency: the steady guard does not touch this at all any more. The
        // frame is held by `install_protected_frame_hook`, so no guard pass
        // rewrites a style word and no guard pass has an alpha to restore. The
        // guard's only remaining route to a restore is through the capture
        // helper, which is on the `!ready` open path and is itself read-gated.
        let guard = function_body(source, "fn start_guard(");
        assert!(!guard.contains("enforce_transparent_protected_composer("));
        assert!(!guard.contains("enforce_native_frameless_overlay("));

        // The QA carrier stack reveals the composer with SWP_SHOWWINDOW instead
        // of `window.show()`, so it never reaches `reveal_protected_pair` and its
        // frame strip is the last writer on this HWND at open, at a rebuilt
        // geometry, at a restored composer and at corrected stack drift. It
        // therefore owes the same restore the reveal owes.
        let carrier = function_body(source, "fn active_ensure_carrier_stack(");
        let shown = carrier
            .find("SWP_SHOWWINDOW")
            .expect("the QA carrier stack reveals the composer itself");
        let carrier_stripped = carrier
            .find("enforce_native_frameless_overlay(overlay)")
            .expect("the QA carrier stack strips the frame its reveal restores");
        let carrier_restored = carrier
            .find("enforce_transparent_protected_composer(overlay)")
            .expect("the QA carrier stack must restore the alpha its reveal clears");
        assert!(shown < carrier_stripped);
        assert!(carrier_stripped < carrier_restored);

        // The production carrier stack is a second definition of the same
        // function, so `function_body` (which takes the first match) can never
        // reach it. It is sliced explicitly, because the two feature variants
        // must not be able to drift apart on a contract the operator can see.
        let production_carrier = {
            let at = source
                .find(concat!(
                    "#[cfg(not(feature = \"discord-qa-shell\"))]\n",
                    "#[cfg(target_os = \"windows\")]\n",
                    "fn active_ensure_carrier_stack("
                ))
                .expect("the production carrier stack");
            function_body(&source[at..], "fn active_ensure_carrier_stack(")
        };
        assert!(
            production_carrier.contains("enforce_transparent_protected_composer(overlay)"),
            "production must restore the composer alpha at the same transitions QA does"
        );
        // Production must not reveal the composer itself -- but the needle has to
        // be a call, not the word. Matching the bare identifier made a *comment*
        // about `ensure_shield_stack`'s reveal fail this test, which is the
        // assertion measuring the prose rather than the code.
        assert!(!strip_line_comments(production_carrier).contains("SWP_SHOWWINDOW"));

        // Composer only, in every shipping path. The needles in this test must
        // not be able to satisfy their own assertions, and they cannot:
        // `overlay_source()` already splits this module off at
        // `#[cfg(test)]\nmod tests {`, so it never returns any of the source
        // below. This used to re-slice at `"mod tests {"` and `expect` it,
        // which could only ever panic -- that separator is exactly what
        // `overlay_source()` consumed, so the needle was provably absent from
        // its own haystack and every assertion after it was unreachable.
        let shipped = source;
        // Was 3, then 4 -- one definition plus one transition call site per
        // writer that can clear the alpha. The contract changed again rather
        // than the baseline, twice over:
        //
        // * `SetWindowDisplayAffinity` was found to be a writer at all. It is
        //   not a frame change, so no frame-side restore covered it, and it is
        //   re-issued on every `Focused(true)` -- every click into the composer,
        //   with no guard transition behind it. It gets the fifth call site,
        //   inside `apply_protected_composer_capture_protection`, which is the
        //   only place allowed to issue that write on this HWND.
        // * the production carrier stack gets the sixth, so that the shipping
        //   and QA variants restore at exactly the same four transitions.
        //
        // Still not a cadence: every one of these is reached only at a
        // transition, and the affinity one only when the affinity actually had
        // to be written.
        assert_eq!(
            shipped
                .matches("enforce_transparent_protected_composer(")
                .count(),
            6
        );
        for handed_the_shield in ["_composer(shield)", "_composer(&shield)"] {
            assert!(
                !shipped.contains(&format!("enforce_transparent_protected{handed_the_shield}")),
                "the opaque capture shield must never be made see-through"
            );
        }
    }

    #[test]
    fn the_alpha_restore_toggles_instead_of_being_elided() {
        // The third attempt at the white corners, and the one that had been
        // named but not tried. Every restore call site was already in the right
        // place; the restore itself was inert. DWM elides a repeat
        // `DwmEnableBlurBehindWindow(fEnable = TRUE)` whose parameters it
        // believes are already in force, and that is exactly the state after a
        // composition-path move: DWM still has the window recorded as blurred
        // while the redirection surface underneath was re-created without the
        // region. The documented way to make it land is a FALSE -> TRUE pair
        // with a freshly created region.
        let source = overlay_source();
        let restore = function_body(source, "fn enforce_transparent_protected_composer(");
        // Fresh region per call. DWM compares the handle it was given, so
        // re-using one is another way to be elided.
        assert_eq!(restore.matches("CreateRectRgn(0, 0, -1, -1)").count(), 1);
        assert!(restore.contains("let blur_behind = |enable: i32| {"));
        assert!(restore.contains("DeleteObject(region.cast())"));
        // Consumed, never merely read: one composition-path change owes exactly
        // one toggle, and a restore that runs for any other reason owes none.
        let consumed = restore
            .find("COMPOSER_COMPOSITION_PATH_CHANGED.swap(false, Ordering::AcqRel)")
            .expect("the latch must be consumed, not read");
        let disable = restore.find("blur_behind(0)").expect("the disable half");
        let enable = restore
            .find("let mut enabled = blur_behind(1);")
            .expect("the enable half");
        assert!(consumed < disable);
        assert!(disable < enable);
        // Defensive in both directions. The disable is gated on the latch --
        // issuing one where the alpha is intact would put a single opaque frame
        // on screen, which is the flash this file must not produce -- and an
        // enable that fails after a disable that landed is retried, so a single
        // refused DWM call cannot turn a working composer into an opaque one.
        assert!(restore.contains("if recomposited {"));
        assert!(restore.contains("if enabled < 0 && recomposited {"));
        // Armed by exactly one writer, and by the only write that can move this
        // HWND between composition paths.
        assert_eq!(
            source
                .matches("COMPOSER_COMPOSITION_PATH_CHANGED.store(true, Ordering::Release)")
                .count(),
            1
        );
        let affinity = function_body(source, "fn apply_protected_composer_capture_protection(");
        let written = affinity
            .find("super::screenshot::apply_to_window(")
            .expect("the affinity write");
        let armed = affinity
            .find("COMPOSER_COMPOSITION_PATH_CHANGED.store(true, Ordering::Release)")
            .expect("the write arms the latch");
        let restored = affinity
            .find("enforce_transparent_protected_composer(window)")
            .expect("and the same call consumes it");
        assert!(written < armed);
        assert!(armed < restored);
    }

    #[test]
    fn every_composer_affinity_write_puts_the_alpha_back() {
        let source = overlay_source();
        // `SetWindowDisplayAffinity` moves the window into a separate DWM
        // composition path and the blur-behind region does not survive the move.
        // It is therefore a writer of the transparency contract, and the only
        // one that is not a frame change -- which is why every frame-side
        // restore being correct still left the operator with white corners.
        let helper = function_body(source, "fn apply_protected_composer_capture_protection(");
        let read = helper
            .find("GetWindowDisplayAffinity(")
            .expect("a redundant affinity write must not be issued at all");
        let write = helper
            .find("super::screenshot::apply_to_window(")
            .expect("the affinity is still written when it actually differs");
        let restored = helper
            .find("enforce_transparent_protected_composer(window)")
            .expect("an affinity write must put the alpha back");
        // Read, then write, then restore. Nothing may observe the window
        // between the write and the restore.
        assert!(read < write);
        assert!(write < restored);
        // Read-gated, never skipped: a failed read must fall through to the
        // write rather than leave an unprotected window looking protected.
        assert!(helper.contains("&& current == required"));

        // Single choke point. Every other path that used to set the composer's
        // affinity by hand is a path that could leave it opaque.
        assert_eq!(
            source.matches("screenshot::apply_to_window(").count(),
            2,
            "only the two cfg variants of the composer helper may set its affinity"
        );
        for owner in [
            "fn build_overlay_window(",
            "fn show_guarded_overlay(",
            "fn start_guard(",
        ] {
            assert!(
                !function_body(source, owner).contains("screenshot::apply_to_window("),
                "{owner} must go through apply_protected_composer_capture_protection"
            );
        }
        // The focus handler is the one writer a person triggers directly, so it
        // is the one that has to be paired: a click into the composer is a
        // Focused(true), and it lands after every guard-driven transition.
        let builder = function_body(source, "fn build_overlay_window(");
        let focused = builder
            .find("tauri::WindowEvent::Focused(true)")
            .expect("the composer re-applies its affinity on focus");
        let paired = builder
            .find("apply_protected_composer_capture_protection(&focus_window)")
            .expect("the focus handler must restore the alpha its write clears");
        assert!(focused < paired);
    }

    #[test]
    fn a_host_geometry_transition_forces_a_fresh_composer_measurement() {
        // The maximize regression: the rectangle changed, so the cached absolute
        // composer bounds cannot be reused for the surface geometry.
        assert!(geometry_transition_forces_refresh(
            true, false, false, false, true
        ));
        // Un-maximize/restore is the same transition in the other direction.
        assert!(geometry_transition_forces_refresh(
            true, false, true, false, true
        ));
        // Coming back from a hidden protected pair re-measures once too: the
        // composer can have moved inside an unchanged Discord window.
        assert!(geometry_transition_forces_refresh(
            true, true, true, false, true
        ));
        // Steady state writes nothing and measures nothing.
        assert!(!geometry_transition_forces_refresh(
            true, true, false, false, true
        ));
        // A still-moving rectangle may not pay for a measurement per tick, and a
        // background OSL or an in-flight carrier never measures at all.
        assert!(!geometry_transition_forces_refresh(
            true, false, false, false, false
        ));
        assert!(!geometry_transition_forces_refresh(
            false, false, false, false, true
        ));
        assert!(!geometry_transition_forces_refresh(
            true, false, false, true, true
        ));
    }

    #[test]
    fn the_guard_re_derives_geometry_after_a_maximize_has_settled() {
        let guard = function_body(overlay_source(), "fn start_guard(");
        // The rectangle is re-read every full pass and the settle clock is reset
        // by the change itself, so nothing latches a mid-transition measurement.
        assert!(guard.contains("if !discord_rect_unchanged {"));
        assert!(guard.contains("last_discord_rect_change = Instant::now();"));
        assert!(guard.contains(
            "let discord_geometry_settled = discord_rect_unchanged\n                        && last_discord_rect_change.elapsed() >= DISCORD_GEOMETRY_SETTLE;"
        ));
        // The settled flag, not the raw "rectangle stopped moving" read, is what
        // decides whether the composer is re-measured.
        assert!(guard.contains(
            "discord_geometry_settled,\n                        composer_temporarily_hidden,"
        ));
        // Still bounded: a moving rectangle cannot buy one walk per tick.
        assert!(guard.contains("last_composer_refresh.elapsed() >= GEOMETRY_REFRESH_MIN_INTERVAL"));
        // And the full pass still has exactly one placement expression, so no new
        // per-tick window write was introduced. The drag path shares the same
        // deferred batch through `place_moved_protected_pair`, and the two are
        // mutually exclusive within a tick: the drag path returns before the full
        // pass begins. See `the_drag_path_writes_through_the_same_single_batch`.
        assert_eq!(guard.matches("position_window_pair(").count(), 1);
    }

    #[test]
    fn composer_focus_hit_test_uses_half_open_verified_bounds() {
        let rect = OverlayRect {
            x: 100,
            y: 200,
            width: 320,
            height: 56,
        };
        assert!(point_is_inside_overlay_rect(100, 200, rect));
        assert!(point_is_inside_overlay_rect(419, 255, rect));
        assert!(!point_is_inside_overlay_rect(420, 255, rect));
        assert!(!point_is_inside_overlay_rect(419, 256, rect));
        assert!(!point_is_inside_overlay_rect(99, 200, rect));
    }

    // Named for the predicates, not for a reaction: the QA build no longer hides
    // anything on foreign focus, and neither does production. Both builds still
    // share the same focus *answers*, which is what this pins.
    #[cfg(feature = "discord-qa-shell")]
    #[test]
    fn qa_shell_shares_the_production_foreground_predicates() {
        assert_eq!(
            active_first_guard_decision(true, false, false, true),
            FirstGuardDecision::WaitHidden
        );
        assert_eq!(
            active_first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
        assert!(active_overlay_requires_focus_acquisition());
        assert!(active_should_reclaim_composer_focus(
            true, true, false, true
        ));
        assert!(active_should_refresh_composer_bounds(
            true, true, true, false, false, true, true
        ));
        assert!(!active_should_refresh_composer_bounds(
            true, true, true, false, true, true, true
        ));
        assert!(active_should_refresh_composer_bounds(
            true, true, false, false, false, true, false
        ));
        // An unsettled rectangle still may not buy one accessibility walk per
        // 16 ms guard tick while a maximize animation or a drag is in progress.
        assert!(!active_should_refresh_composer_bounds(
            true, true, false, false, false, false, false
        ));
    }

    #[cfg(not(feature = "discord-qa-shell"))]
    #[test]
    fn production_active_guard_preserves_fail_closed_foreground_policy() {
        assert_eq!(
            active_first_guard_decision(true, false, false, true),
            FirstGuardDecision::WaitHidden
        );
        assert_eq!(
            active_first_guard_decision(true, false, false, false),
            FirstGuardDecision::Close
        );
        assert!(active_overlay_requires_focus_acquisition());
        // Production used to answer this `false` unconditionally, which is why
        // the real operator's keystrokes went into Discord's own message box:
        // the composer sat correctly on top and nothing ever gave it the caret.
        // It is the same narrow predicate the QA build uses -- all four
        // conditions must hold -- and it now shares that answer.
        assert!(active_should_reclaim_composer_focus(
            true, true, false, true
        ));
        assert!(!active_should_reclaim_composer_focus(
            true, true, true, true
        ));
        assert!(!active_should_reclaim_composer_focus(
            true, true, false, false
        ));
        assert!(!active_should_reclaim_composer_focus(
            false, true, false, true
        ));
        assert!(active_should_refresh_composer_bounds(
            true, true, true, false, false, true, true
        ));
        assert!(!active_should_refresh_composer_bounds(
            true, true, true, false, true, true, true
        ));
        // The maximize regression: production used to discard the geometry-change
        // trigger entirely and wait out the five-second blind backstop, so the
        // surface was rebuilt from the pre-maximize composer rectangle.
        assert!(active_should_refresh_composer_bounds(
            true, true, false, false, false, true, false
        ));
        assert!(!active_should_refresh_composer_bounds(
            true, true, false, false, false, false, false
        ));
        // Still never measures from the background, and never during a send.
        assert!(!active_should_refresh_composer_bounds(
            true, false, false, false, false, true, true
        ));
        assert!(!active_should_refresh_composer_bounds(
            true, true, false, false, true, true, false
        ));
    }

    #[test]
    fn overlay_context_callbacks_can_reenter_state_without_deadlock() {
        let state = Arc::new(OverlaySessionState::default());
        let host = test_host("discord-test");
        let epoch = state
            .activate("context-test".to_owned(), host.clone())
            .expect("active overlay");
        state.mark_ready(epoch, &host).expect("ready overlay");

        let callback_state = Arc::clone(&state);
        let expected_host = host.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = (|| {
                callback_state.with_bootstrap_context(|token, callback_host| {
                    assert_eq!(token, "context-test");
                    assert_eq!(callback_host, &expected_host);
                    assert!(callback_state.is_epoch(epoch));
                    Ok(())
                })?;
                callback_state.with_context(|token, callback_host| {
                    assert_eq!(token, "context-test");
                    assert_eq!(callback_host, &expected_host);
                    assert!(callback_state.is_ready(epoch));
                    Ok(())
                })?;
                let marker = callback_state.validated_marker(|token, callback_host| {
                    assert_eq!(token, "context-test");
                    assert_eq!(callback_host, &expected_host);
                    assert!(callback_state.is_ready(epoch));
                    Ok(())
                })?;
                assert_eq!(marker, (epoch, expected_host.clone()));
                callback_state.validate_marker(epoch, &expected_host, |token, callback_host| {
                    assert_eq!(token, "context-test");
                    assert_eq!(callback_host, &expected_host);
                    assert!(callback_state.is_ready(epoch));
                    Ok(())
                })
            })();
            sender.send(result).expect("test receiver");
        });

        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("overlay callback must not retain the state mutex")
            .expect("reentrant callbacks");
        worker.join().expect("callback worker");
    }

    #[test]
    fn carrier_placement_guard_is_exclusive_and_clears_on_drop() {
        let state = OverlaySessionState::default();
        assert!(!state.carrier_placement_active());

        let guard = state
            .begin_carrier_placement()
            .expect("first carrier placement");
        assert!(state.carrier_placement_active());
        assert!(state.begin_carrier_placement().is_err());

        drop(guard);
        assert!(!state.carrier_placement_active());
        drop(
            state
                .begin_carrier_placement()
                .expect("carrier placement after guard drop"),
        );
        assert!(!state.carrier_placement_active());
    }

    #[test]
    fn qa_dormant_toggle_reuses_only_the_exact_context_and_rejects_all_ipc() {
        let state = OverlaySessionState::default();
        let host = test_host("qa-dormant");
        let epoch = state
            .activate("ctx.qa-dormant".to_owned(), host.clone())
            .expect("active overlay");
        state.mark_ready(epoch, &host).expect("ready overlay");

        state.suspend_for_qa_toggle().expect("dormant overlay");
        assert_eq!(
            state
                .qa_dormant_reuse("ctx.qa-dormant", &host)
                .expect("reuse state"),
            QaDormantReuse::Exact
        );
        assert_eq!(
            state
                .qa_dormant_reuse("ctx.qa-other", &host)
                .expect("changed context"),
            QaDormantReuse::Changed
        );
        assert_eq!(
            state
                .qa_dormant_reuse("ctx.qa-dormant", &test_host("qa-other-host"))
                .expect("changed host"),
            QaDormantReuse::Changed
        );
        assert!(!state.is_epoch(epoch));
        assert!(!state.is_ready(epoch));
        assert!(state.with_bootstrap_context(|_, _| Ok(())).is_err());
        assert!(state.with_context(|_, _| Ok(())).is_err());
        assert!(state.validated_marker(|_, _| Ok(())).is_err());
        assert!(state
            .activate("ctx.qa-other".to_owned(), host.clone())
            .is_err());
        assert_eq!(
            state
                .qa_dormant_reuse("ctx.qa-dormant", &host)
                .expect("retained after rejected activation"),
            QaDormantReuse::Exact
        );

        let mut previous_epoch = epoch;
        for _ in 0..3 {
            let next_epoch = state
                .activate("ctx.qa-dormant".to_owned(), host.clone())
                .expect("same context reactivated");
            assert_ne!(next_epoch, previous_epoch);
            assert!(state.is_epoch(next_epoch));
            state
                .mark_ready(next_epoch, &host)
                .expect("reactivated overlay ready");
            assert!(state.with_context(|_, _| Ok(())).is_ok());
            state.suspend_for_qa_toggle().expect("rapid dormant toggle");
            assert_eq!(
                state
                    .qa_dormant_reuse("ctx.qa-dormant", &host)
                    .expect("same retained renderer"),
                QaDormantReuse::Exact
            );
            assert!(state.with_context(|_, _| Ok(())).is_err());
            previous_epoch = next_epoch;
        }
        assert!(state.activate("ctx.qa-other".to_owned(), host).is_err());
    }

    #[test]
    fn qa_dormant_toggle_refuses_while_a_carrier_is_in_flight() {
        let state = OverlaySessionState::default();
        let host = test_host("qa-dormant-busy");
        let epoch = state
            .activate("ctx.qa-dormant-busy".to_owned(), host.clone())
            .expect("active overlay");
        state.mark_ready(epoch, &host).expect("ready overlay");
        let placement = state.begin_carrier_placement().expect("carrier placement");

        assert!(state.suspend_for_qa_toggle().is_err());
        assert!(state.is_ready(epoch));
        drop(placement);
        state
            .suspend_for_qa_toggle()
            .expect("toggle after placement");
    }

    #[cfg(feature = "discord-qa-shell")]
    #[test]
    fn qa_open_waits_for_the_same_epoch_to_be_ready() {
        let state = Arc::new(OverlaySessionState::default());
        let host = test_host("qa-ready");
        let epoch = state
            .activate("ctx.qa-ready".to_owned(), host.clone())
            .unwrap();
        let ready_state = state.clone();
        let ready_host = host.clone();
        let ready = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            ready_state.mark_ready(epoch, &ready_host).unwrap();
        });

        state
            .wait_until_ready(epoch, &host, Duration::from_secs(1))
            .unwrap();
        ready.join().unwrap();
    }

    #[cfg(feature = "discord-qa-shell")]
    #[test]
    fn qa_open_rejects_epoch_or_context_change_while_waiting() {
        let state = OverlaySessionState::default();
        let host = test_host("qa-changed");
        let epoch = state
            .activate("ctx.qa-changed".to_owned(), host.clone())
            .unwrap();
        state.clear();
        assert!(state
            .wait_until_ready(epoch, &host, Duration::from_millis(1))
            .unwrap_err()
            .contains("context changed"));
    }

    #[cfg(feature = "discord-qa-shell")]
    #[test]
    fn qa_open_times_out_while_the_overlay_is_still_guarding() {
        let state = OverlaySessionState::default();
        let host = test_host("qa-timeout");
        let epoch = state
            .activate("ctx.qa-timeout".to_owned(), host.clone())
            .unwrap();
        assert!(state
            .wait_until_ready(epoch, &host, Duration::ZERO)
            .unwrap_err()
            .contains("safety check in time"));
        assert!(!state.is_ready(epoch));
    }

    #[test]
    fn marker_is_confirmed_before_snapshot_and_callback_mutation() {
        let state = OverlaySessionState::default();
        let host = test_host("discord-original");
        let epoch = state
            .activate("context-original".to_owned(), host.clone())
            .expect("active overlay");
        state.mark_ready(epoch, &host).expect("ready overlay");

        let callback_ran = AtomicBool::new(false);
        let wrong_host = test_host("discord-wrong");
        assert!(state
            .validate_marker(epoch, &wrong_host, |_, _| {
                callback_ran.store(true, Ordering::Release);
                Ok(())
            })
            .is_err());
        assert!(!callback_ran.load(Ordering::Acquire));

        state
            .validate_marker(epoch, &host, |token, callback_host| {
                assert_eq!(token, "context-original");
                assert_eq!(callback_host, &host);
                state.clear();
                Ok(())
            })
            .expect("validated snapshot");
        assert!(!state.is_epoch(epoch));
        assert!(state.validate_marker(epoch, &host, |_, _| Ok(())).is_err());
    }
    /// Source of this module with its own test text removed, for the
    /// single-instance invariants that cannot be observed without a live Tauri
    /// app handle and two racing threads.
    fn overlay_source() -> &'static str {
        include_str!("native_discord_overlay.rs")
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("module source")
    }

    /// The code of a slice with its `//` comment lines removed, for assertions
    /// that are about what a function *does* rather than what it says about
    /// itself.
    fn strip_line_comments(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn function_body(source: &'static str, signature: &str) -> &'static str {
        let start = source
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} is missing"));
        let body = &source[start..];
        let end = body
            .find("\n}\n")
            .unwrap_or_else(|| panic!("{signature} is unterminated"));
        &body[..end]
    }

    #[test]
    fn prewarm_and_session_paths_address_the_same_two_windows() {
        // A pre-warm that used any other label would leave the session path
        // building a second window with the same title.
        assert_eq!(ProtectedSurface::Composer.label(), OVERLAY_LABEL);
        assert_eq!(ProtectedSurface::Shield.label(), SHIELD_LABEL);
        assert_ne!(OVERLAY_LABEL, SHIELD_LABEL);

        let source = overlay_source();
        let composer_builder = function_body(source, "fn build_overlay_window(");
        assert!(composer_builder.contains("OVERLAY_LABEL,"));
        assert!(composer_builder.contains(".title(\"OSL private composer\")"));
        let shield_builder = function_body(source, "fn build_shield_window(");
        assert!(shield_builder.contains("SHIELD_LABEL,"));
        assert!(shield_builder.contains(".title(\"OSL capture shield\")"));
        // Only these two builders may ever exist.
        assert_eq!(source.matches("WebviewWindowBuilder::new(").count(), 2);
    }

    #[test]
    fn no_path_builds_a_protected_window_outside_the_single_instance_gate() {
        let source = overlay_source();
        // One definition plus exactly one pre-warm call and one session call for
        // each builder. A new call site is a new way to duplicate a window.
        assert_eq!(source.matches("build_overlay_window(").count(), 3);
        assert_eq!(source.matches("build_shield_window(").count(), 3);
        assert_eq!(
            source.matches("ensure_retained_protected_window(").count(),
            5
        );
        for signature in [
            "pub(crate) fn prewarm(",
            "fn ensure_shield_window(",
            "fn ensure_window(",
        ] {
            let body = function_body(source, signature);
            let gate = body.find("ensure_retained_protected_window(");
            for builder in ["build_overlay_window(", "build_shield_window("] {
                if let Some(call) = body.find(builder) {
                    let gate =
                        gate.unwrap_or_else(|| panic!("{signature} builds outside the gate"));
                    assert!(gate < call, "{signature} builds outside the gate");
                }
            }
        }
    }

    #[test]
    fn the_single_instance_gate_rechecks_under_the_build_lock() {
        let gate = function_body(overlay_source(), "fn ensure_retained_protected_window(");
        let lock = gate
            .find("PROTECTED_WINDOW_BUILD_LOCK")
            .expect("construction must be serialized");
        let recheck = gate
            .find("app.get_webview_window(surface.label())")
            .expect("existence must be rechecked inside the lock");
        let build = gate.find("build()?").expect("gated construction");
        assert!(lock < recheck, "the recheck must happen under the lock");
        assert!(
            recheck < build,
            "a window must never be built when one exists"
        );
        // A pre-warmed window can be revealed later, so it is frameless from
        // creation and not only from its first session.
        assert!(gate.contains("apply_protected_frame_contract(&window, surface)"));
    }

    #[test]
    fn every_reveal_still_enforces_the_frameless_frame_after_show() {
        let source = overlay_source();
        let contract = function_body(source, "fn apply_protected_frame_contract(");
        assert!(contract.contains("set_decorations(false)"));
        assert!(contract.contains("enforce_native_frameless_overlay(window)"));
        assert!(contract.contains("set_shadow(false)"));
        assert!(contract.contains("suppress_accent_border"));
        // Both Tauri setters are queued to the event loop and each makes tao
        // rewrite the whole style from its cached, captioned flags. A strip
        // between them is overwritten by the later one, so both are issued
        // first, the queue is drained, and only then is the frame stripped.
        let decorations = contract
            .find("set_decorations(false)")
            .expect("the contract sets decorations");
        let shadow = contract
            .find("set_shadow(false)")
            .expect("the contract sets the shadow");
        let settled = contract
            .find("settle_protected_window(window, surface.frame_error())")
            .expect("the contract must drain its own queued setters before stripping");
        let stripped = contract
            .find("enforce_native_frameless_overlay(window)")
            .expect("the contract strips the frame");
        assert!(decorations < settled);
        assert!(shadow < settled);
        assert!(settled < stripped);
        // Tauri restores cached decorations while revealing, so creation-time
        // enforcement never replaces the post-reveal enforcement. Exactly one
        // place reveals a protected window, so the invariant has one home.
        let mut reveals = 0;
        for (at, _) in source.match_indices("window.show()") {
            let after = &source[at..];
            let after = &after[..after.len().min(700)];
            assert!(
                after.contains("enforce_native_frameless_overlay(window)"),
                "a reveal lost its post-show frameless enforcement"
            );
            reveals += 1;
        }
        assert_eq!(reveals, 1);
        let reveal = function_body(source, "fn reveal_protected_pair(");
        // The shield is revealed with the composer and is shown by the same
        // Tauri path, so it is re-stripped too.
        let shield_reveal = reveal
            .find("show_capture_shield(shield, shielded)")
            .expect("the shield is revealed with the composer");
        let shield_frame = reveal
            .find("enforce_native_frameless_overlay(shield)")
            .expect("the shield must be re-stripped after it is shown");
        assert!(shield_reveal < shield_frame);
        // Regression for the persistent Windows title bar: Tauri returns from
        // `show` before the event loop has shown anything, so an enforcement
        // written after the call still ran before tao restored the caption. The
        // measured proof was a reveal-time style trace of 0x84C80000 -- a style
        // with no WS_VISIBLE at all. Both strips must therefore be ordered after
        // a barrier that proves the queued reveal has landed.
        let composer_show = reveal.find("window.show()").expect("the single reveal");
        let landed = reveal
            .find("wait_for_revealed_protected_window(window, show_error)")
            .expect("a reveal must be proven on screen before its frame is stripped");
        let shield_settled = reveal
            .find("settle_protected_window(shield, show_error)")
            .expect("the shield reveal is queued too and must be drained");
        let composer_frame = reveal
            .find("enforce_native_frameless_overlay(window)")
            .expect("the composer must be re-stripped after it is shown");
        assert!(composer_show < landed);
        assert!(landed < shield_frame);
        assert!(shield_settled < shield_frame);
        assert!(landed < composer_frame);
    }

    #[test]
    fn a_protected_window_has_no_non_client_area_for_a_caption_to_live_in() {
        // The owner reported the "OSL private composer" title bar three times
        // while every style assertion in this file passed, because a style bit is
        // not a durable contract: tao rebuilds GWL_STYLE from its own cached
        // flags -- which always carry WS_CAPTION|WS_SYSMENU -- on the event loop,
        // at every show, hide, restore, set_decorations and frame change, while
        // the strip runs on an overlay worker. That is a race, and losing it once
        // ships a caption.
        //
        // The mechanism is now the window's nature, not its bits: the window
        // procedure answers WM_NCCALCSIZE with "no non-client area", so the
        // client area is the whole window rectangle and a caption, border,
        // sizing frame or shadow has zero space to be drawn in no matter what
        // the style words say.
        let source = overlay_source();
        let hook = function_body(
            source,
            "unsafe extern \"system\" fn protected_frameless_window_proc(",
        );
        assert!(hook.contains("if message == WM_NCCALCSIZE && wparam != 0 {"));
        assert!(hook.contains("if message == WM_NCPAINT {"));
        // Never call through a null displaced procedure.
        assert!(hook.contains("DefWindowProcW(hwnd, message, wparam, lparam)"));
        // Installed before the window has ever been shown, and never removed
        // while it lives, so every later show/hide/restore/geometry change is
        // already behind it.
        let install = function_body(source, "fn install_protected_frame_hook(");
        assert!(install.contains("GWLP_WNDPROC"));
        assert!(install.contains("SWP_FRAMECHANGED"));
        assert!(!install.contains("RemoveWindowSubclass"));
        let contract = function_body(source, "fn apply_protected_frame_contract(");
        let hooked = contract
            .find("install_protected_frame_hook(window)")
            .expect("the frame contract installs the hook");
        for later_writer in [
            "set_decorations(false)",
            "set_shadow(false)",
            "suppress_accent_border",
        ] {
            assert!(
                hooked
                    < contract
                        .find(later_writer)
                        .expect("the contract keeps its own writers"),
                "the hook must be installed before anything that can restore a frame"
            );
        }
        // Both retained protected windows go through the one contract, so the
        // shield cannot wear a caption either.
        let gate = function_body(source, "fn ensure_retained_protected_window(");
        assert!(gate.contains("apply_protected_frame_contract(&window, surface)"));
        // Exactly two slots, because the single-instance build gate guarantees
        // exactly two protected windows.
        assert_eq!(PROTECTED_FRAME_HOOKS.len(), 2);
    }

    #[test]
    fn a_never_revealed_session_throttles_discovery_instead_of_retrying_every_tick() {
        // Regression for the regain-foreground stall: while a session has
        // never shown a protected surface, `start_guard` must not run
        // `discord_overlay_target` / composer-bounds discovery on every
        // 16 ms tick. That work takes a lock shared with OSL's own
        // window-focus handling, and Alt-Tab returning foreground to OSL is
        // exactly when the two can overlap; hammering it unthrottled is how
        // OSL's own message pump previously stalled with no visible
        // protected surface anywhere on screen.
        let guard = function_body(overlay_source(), "fn start_guard(");
        let throttle_condition = guard
            .find("if !ready && !ever_revealed {")
            .expect("the first-open path must be gated on having never revealed anything");
        let throttle_check = guard
            .find("if !first_open_attempt_due(last_first_open_attempt.elapsed())")
            .expect("the gate must consult the bounded retry cadence");
        let throttle_return = guard[throttle_check..]
            .find("return Ok(false);")
            .map(|offset| throttle_check + offset)
            .expect("an attempt that is not yet due must return early, not proceed");
        let owner_lookup = guard
            .find("active_unlocked_osl_user_id(&app.state::<HubCoreState>())")
            .expect("the guard still discovers the owner once the throttle allows it");
        let target_lookup = guard
            .find("discord_overlay_target(&owner)")
            .expect("the guard still discovers the Discord window once the throttle allows it");
        // The throttle must gate the expensive discovery calls, not run
        // alongside or after them.
        assert!(throttle_condition < throttle_check);
        assert!(throttle_check < throttle_return);
        assert!(throttle_return < owner_lookup);
        assert!(owner_lookup < target_lookup);

        // The throttle must never fire once a surface has actually been
        // shown: `ever_revealed` is set exactly once, and only after both
        // possible reveal paths (restoring a temporarily hidden pair, and
        // the very first reveal) have already succeeded.
        assert_eq!(guard.matches("ever_revealed = true;").count(), 1);
        let ever_revealed_set = guard
            .find("ever_revealed = true;")
            .expect("ever_revealed must be recorded somewhere in the guard");
        for reveal_call in guard.match_indices("reveal_protected_pair(") {
            assert!(
                reveal_call.0 < ever_revealed_set,
                "ever_revealed must be set only after a reveal call, never before one"
            );
        }
        // It must be set before the pass reports success back to the caller,
        // so a session that just opened is never throttled again on its very
        // next tick.
        let final_ok = guard
            .rfind("Ok((true, (!ready).then(|| stored_host.clone())))")
            .expect("the guard's success return must still hand back the ready host");
        assert!(ever_revealed_set < final_ok);

        // The throttle gates DISCOVERY, never presentation. Nothing that can
        // take an already-visible surface off screen may sit behind it, and the
        // steady-state foreground read that used to run in front of it is gone
        // entirely -- see
        // `losing_the_foreground_never_takes_the_protected_composer_off_screen`.
        let before_the_gate = &guard[..throttle_condition];
        assert!(!before_the_gate.contains(".hide()"));
    }

    #[test]
    fn a_visible_composer_is_never_taken_off_screen_to_sample_behind_it() {
        // The owner's rule for this surface is that it is active 100% of the
        // time and does not flash. "Hide, BitBlt, reveal" breaks both, so the
        // only state in which it is allowed is one where the pair is already
        // off screen.
        assert!(resample_may_take_the_pair_off_screen(false, false));
        assert!(resample_may_take_the_pair_off_screen(false, true));
        assert!(resample_may_take_the_pair_off_screen(true, true));
        assert!(
            !resample_may_take_the_pair_off_screen(true, false),
            "a composer the operator can see must never be hidden to sample"
        );

        // With capture protection on, the question is never even asked: the
        // BitBlt reads straight through a window Windows has excluded from
        // capture. This is what the gate above backstops.
        assert!(!sampling_requires_the_pair_to_leave_the_screen(true, false));
        assert!(sampling_requires_the_pair_to_leave_the_screen(false, false));
        assert!(sampling_requires_the_pair_to_leave_the_screen(true, true));

        // Wiring: the guard must consult the gate, and the only `hide()` inside
        // the re-sample branch must sit behind it.
        let guard = function_body(overlay_source(), "fn start_guard(");
        let gate = guard
            .find("resample_may_take_the_pair_off_screen(")
            .expect("the re-sample must ask whether it may leave the screen");
        let decided = guard
            .find("let resample_native_surface = resample_wanted")
            .expect("the gate must fold into the re-sample decision itself");
        assert!(
            decided < gate,
            "the gate belongs inside the decision, not after the capture has started"
        );
        let sample_branch = guard
            .split("if resample_native_surface {")
            .nth(1)
            .expect("the re-sample branch");
        let hide = sample_branch
            .find("let _ = window.hide();")
            .expect("the QA/unprotected path still hides");
        let leaves = sample_branch
            .find("if must_leave_the_screen {")
            .expect("and only when it must");
        assert!(leaves < hide);
        // `must_leave_the_screen` inside that branch is the *already gated*
        // answer, not a fresh unconditional read.
        assert!(sample_branch.contains("let must_leave_the_screen = sample_needs_the_screen;"));
    }

    #[test]
    fn a_moving_discord_window_never_costs_an_accessibility_walk() {
        // A drag is answered by translating the cached rectangle, which is the
        // same answer for free. Measuring anyway is five cross-process walks a
        // second into Electron for as long as the mouse is down, and that is
        // what the operator feels as the drag lagging.
        assert!(composer_measurement_allowed(true, false));
        assert!(!composer_measurement_allowed(true, true));
        // A session that is still opening is exempt, so this can only ever be a
        // drag optimisation and never a deferred first reveal.
        assert!(composer_measurement_allowed(false, true));
        assert!(composer_measurement_allowed(false, false));

        let guard = function_body(overlay_source(), "fn start_guard(");
        // Applied over every trigger, not inside one of them: a drag must not be
        // able to buy a walk through the backstop, the transition or the QA door.
        assert!(guard.contains(
            "let refresh_composer_bounds = composer_measurement_allowed(\n                        ready,\n                        !discord_rect_unchanged,\n                    ) && active_should_refresh_composer_bounds("
        ));
        // And the translation that makes this safe is still there.
        assert!(guard.contains("host_rect_translation(measured_against_rect, target.rect)"));
        // Deferred, never skipped: the settle window is longer than the throttle,
        // so the first pass after the drag stops still measures.
        assert!(DISCORD_GEOMETRY_SETTLE > GEOMETRY_REFRESH_MIN_INTERVAL);
    }

    /// The drag half of the owner's report. A move is answered from what the last
    /// complete pass proved, so the follow interval is the tick plus one deferred
    /// batch instead of the tick plus a full pass.
    #[test]
    fn a_move_is_answered_by_translating_everything_the_last_pass_proved() {
        let placed = OverlayRect {
            x: 2_540,
            y: 904,
            width: 736,
            height: 58,
        };
        let rows = [[2_540, 700, 3_276, 740], [2_540, 760, 3_276, 800]];
        // The rows travel with the placement or the pair can be seen apart: the
        // composer would land on the window's new position while the shield stayed,
        // and clipped, on the old one.
        let moved = translated_painted_rows(&rows, (-64, 12)).expect("a move");
        assert_eq!(
            moved,
            vec![[2_476, 712, 3_212, 752], [2_476, 772, 3_212, 812]]
        );
        let moved_placement = translated_overlay_rect(placed, (-64, 12)).expect("a move");
        // Same delta, so the shield's bounds keep the same relationship to the
        // composer's rectangle that the full pass derived.
        assert_eq!(
            painted_rows_bounds(&moved).expect("bounds").y - moved_placement.y,
            painted_rows_bounds(&rows).expect("bounds").y - placed.y
        );
        // All or nothing: a row that would overflow cannot clip the shield to a
        // region the guard never derived.
        assert_eq!(
            translated_painted_rows(&[[i32::MAX - 1, 0, i32::MAX, 10]], (2, 0)),
            None
        );
        assert_eq!(
            translated_painted_rows(&[], (10, 10)),
            Some(Vec::new()),
            "a session painting no rows still has a placement to move"
        );
        // A drag frame must never re-clip the shield. The region is held in the
        // shield's own coordinates, so translating the bounds and every row by the
        // same delta is the identical region -- and `SetWindowRgn` is a
        // cross-thread call that redraws the whole shield.
        let bounds = painted_rows_bounds(&rows).expect("bounds");
        let moved_bounds = painted_rows_bounds(&moved).expect("bounds");
        assert_eq!(
            shield_region_offsets(bounds, &rows),
            shield_region_offsets(moved_bounds, &moved),
            "a move cannot change the region, so it must not be able to rewrite it"
        );
        // A row appearing between two others moves no edge of the union, so the
        // bounding box cannot be what tells one region from another.
        let extra = [rows[0], [2_540, 745, 3_276, 755], rows[1]];
        assert_eq!(
            painted_rows_bounds(&extra).expect("bounds"),
            bounds,
            "the union is unchanged, which is exactly why the offsets are compared"
        );
        assert_ne!(
            shield_region_offsets(bounds, &rows),
            shield_region_offsets(bounds, &extra)
        );
    }

    /// What a drag frame actually costs, measured against real windows owned by a
    /// real message-pumping thread rather than reasoned about.
    ///
    /// The composer follows the borrowed window by polling, so its follow error is
    /// the tick plus whatever a frame costs, times how fast the operator's hand is
    /// moving. Every number the harness quoted for that error was derived from a
    /// 512 ms sampler, which cannot measure a 16 ms loop; this measures the writes
    /// themselves, on the calling thread, with the owning thread both responsive
    /// and deliberately busy -- because a caption drag runs inside Windows' modal
    /// move loop, so "busy" is the case that matters.
    ///
    /// Ignored and env-gated: it creates windows, so it must never run inside an
    /// ordinary suite.
    ///
    /// `OSL_NATIVE_DRAG_COST=1 cargo test --target x86_64-pc-windows-gnu \
    ///   --features desktop,discord-qa-shell --bin osl-privacy-hub \
    ///   -- --ignored --nocapture native_drag_frame_cost`
    #[test]
    #[ignore = "native probe: set OSL_NATIVE_DRAG_COST=1 to run"]
    fn native_drag_frame_cost() {
        use std::sync::atomic::AtomicBool;
        use windows_sys::Win32::Graphics::Gdi::{
            CombineRgn, CreateRectRgn, DeleteObject, GetWindowRgnBox, SetWindowRgn, RGN_ERROR,
            RGN_OR,
        };
        use windows_sys::Win32::System::Threading::GetCurrentThreadId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, PostThreadMessageW,
            RegisterClassW, MSG, WM_QUIT, WNDCLASSW, WS_POPUP,
        };

        if std::env::var_os("OSL_NATIVE_DRAG_COST").is_none() {
            return;
        }
        static BUSY: AtomicBool = AtomicBool::new(false);
        let (handles_tx, handles_rx) = mpsc::channel::<(isize, isize, u32)>();
        // The windows are created on -- and therefore owned by -- this thread, so
        // every write below is the cross-thread write the guard makes.
        let ui = std::thread::spawn(move || unsafe {
            let class: Vec<u16> = "OslProtectedDragCostProbe\0".encode_utf16().collect();
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(DefWindowProcW);
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            let make = || {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    WS_POPUP,
                    0,
                    0,
                    736,
                    58,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            let overlay = make();
            let shield = make();
            handles_tx
                .send((overlay as isize, shield as isize, GetCurrentThreadId()))
                .expect("probe handles");
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                DispatchMessageW(&msg);
                if BUSY.load(Ordering::Acquire) {
                    // A UI thread that is doing per-frame work of its own, which
                    // is what a modal move loop is.
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            DestroyWindow(overlay);
            DestroyWindow(shield);
        });
        let (overlay, shield, ui_thread) = handles_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the probe windows");
        let overlay_hwnd = overlay as HWND;
        let shield_hwnd = shield as HWND;
        let rows = [[0, 0, 736, 40], [0, 60, 736, 100]];

        let median = |mut samples: Vec<u128>| -> u128 {
            samples.sort_unstable();
            samples[samples.len() / 2]
        };
        // Exactly what `clip_capture_shield_to_painted_rows` used to do on every
        // frame of a drag: build the region and install it.
        let reclip = || unsafe {
            let region = CreateRectRgn(0, 0, 0, 0);
            for rect in &rows {
                let piece = CreateRectRgn(rect[0], rect[1], rect[2], rect[3]);
                CombineRgn(region, region, piece, RGN_OR as i32);
                DeleteObject(piece.cast());
            }
            assert_ne!(SetWindowRgn(shield_hwnd, region, 1), 0);
        };
        // And what it does now instead, on a frame that only moved.
        let proof = || unsafe {
            let mut box_rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            assert_ne!(GetWindowRgnBox(shield_hwnd, &mut box_rect), RGN_ERROR);
            let offsets = shield_region_offsets(painted_rows_bounds(&rows).expect("bounds"), &rows);
            assert_eq!(offsets.len(), rows.len());
        };
        let batch = |frame: i32| unsafe {
            let deferred = BeginDeferWindowPos(2);
            let deferred = DeferWindowPos(
                deferred,
                shield_hwnd,
                std::ptr::null_mut(),
                frame,
                frame,
                736,
                100,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            let deferred = DeferWindowPos(
                deferred,
                overlay_hwnd,
                std::ptr::null_mut(),
                frame,
                frame,
                736,
                58,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
            assert_ne!(EndDeferWindowPos(deferred), 0);
        };
        let local = || unsafe {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetWindowRect(overlay_hwnd, &mut rect);
            IsIconic(overlay_hwnd);
            GetWindowLongPtrW(overlay_hwnd, GWL_EXSTYLE);
            GetWindowLongPtrW(shield_hwnd, GWL_EXSTYLE);
        };
        // One of the two things `core_bridge::readiness()` does, and the reason the
        // full pass is not something a drag frame should pay for: the guard
        // resolved the identity twice per tick (once directly, once inside
        // `require_current_context_host`), and in production each resolution stats
        // the password marker on disk. Nothing about it can change because a window
        // moved.
        let identity_half = || {
            let _ = ipc::commands::cmd_osl_password_status();
        };

        reclip();
        for busy in [false, true] {
            BUSY.store(busy, Ordering::Release);
            let frames = if busy { 40 } else { 200 };
            let mut reclips = Vec::new();
            let mut proofs = Vec::new();
            let mut batches = Vec::new();
            let mut locals = Vec::new();
            let mut identities = Vec::new();
            for frame in 0..frames {
                let at = Instant::now();
                reclip();
                reclips.push(at.elapsed().as_micros());
                let at = Instant::now();
                proof();
                proofs.push(at.elapsed().as_micros());
                let at = Instant::now();
                batch(frame);
                batches.push(at.elapsed().as_micros());
                let at = Instant::now();
                local();
                locals.push(at.elapsed().as_micros());
                let at = Instant::now();
                identity_half();
                identity_half();
                identities.push(at.elapsed().as_micros());
            }
            println!(
                "owning thread {}: median us -- shield reclip (before) {}, region proof (after) {}, deferred batch {}, local reads {}, identity halves x2 {}",
                if busy { "busy" } else { "responsive" },
                median(reclips),
                median(proofs),
                median(batches),
                median(locals),
                median(identities),
            );
        }
        BUSY.store(false, Ordering::Release);
        unsafe { PostThreadMessageW(ui_thread, WM_QUIT, 0, 0) };
        ui.join().expect("the probe UI thread");
    }

    /// A drag frame's write is still an ownership-proved write of both windows in
    /// one batch, and the guard still has exactly one placement expression.
    #[test]
    fn the_drag_path_writes_through_the_same_single_batch() {
        let source = overlay_source();
        let helper = function_body(source, "fn place_moved_protected_pair(");
        let verify = helper
            .find("verify_owned_overlay_pair(&window, &shield, trusted_parent)?")
            .expect("a translation is still a SetWindowPos on two owned windows");
        let write = helper
            .find("position_window_pair(&window, &shield, rect, painted)")
            .expect("and it goes through the one deferred batch");
        assert!(verify < write, "prove the pair before moving it");
        // One batch primitive for every writer in the file: two windows when there
        // is a shield to move, one when there is not, and never two calls.
        assert_eq!(source.matches("BeginDeferWindowPos(").count(), 1);
        assert_eq!(
            source.matches("fn position_window_pair(").count(),
            2,
            "the Windows placement and its non-Windows stub"
        );
    }

    #[test]
    fn a_move_translates_the_placement_and_never_resizes_it() {
        let rect = OverlayRect {
            x: 100,
            y: 200,
            width: 320,
            height: 56,
        };
        let moved = translated_overlay_rect(rect, (-40, 17)).expect("an ordinary move");
        assert_eq!(moved.x, 60);
        assert_eq!(moved.y, 217);
        // A move is a translation and nothing else. A placement that changed size
        // here would be a resize the guard never derived, on a surface whose size
        // is the measured composer's.
        assert_eq!(moved.width, rect.width);
        assert_eq!(moved.height, rect.height);
        assert_eq!(
            translated_overlay_rect(rect, (0, 0)).expect("identity"),
            rect
        );
        // Overflow is not silently wrapped into a placement on the other side of
        // the desktop.
        assert_eq!(
            translated_overlay_rect(
                OverlayRect {
                    x: i32::MAX,
                    y: 0,
                    width: 10,
                    height: 10
                },
                (1, 0)
            ),
            None
        );
    }

    #[test]
    fn anchoring_a_placement_at_the_write_lands_where_a_fresh_pass_would() {
        // The property that makes the re-anchor safe rather than merely fast:
        // correcting a stale placement by the window's own residual translation
        // is not an approximation of re-deriving it against the current
        // rectangle, it is the same rectangle.
        //
        // This is what stops the two writers fighting. A guard pass reads
        // Discord's rectangle at the top and writes hundreds of microseconds --
        // or, before this, hundreds of milliseconds -- later. Without the
        // correction it writes the composer where the window *was*, so every pass
        // undoes the last one's progress and the composer trails the gesture.
        let composer = AccessibilityBounds {
            left: 320,
            top: 700,
            right: 1200,
            bottom: 760,
        };
        let opened_against = [200, 100, 1400, 900];
        let stale = protected_surface_rect(opened_against, Some(composer), &[])
            .expect("the pass's own placement");

        for (dx, dy) in [(37, 0), (0, -21), (-140, 96), (200, 200)] {
            let live = [
                opened_against[0] + dx,
                opened_against[1] + dy,
                opened_against[2] + dx,
                opened_against[3] + dy,
            ];
            let delta = host_rect_translation(opened_against, live).expect("a pure move");
            assert_eq!(delta, (dx, dy));

            // What the guard writes: the stale placement, corrected at the write.
            let anchored = translated_overlay_rect(stale, delta).expect("corrected placement");
            // What a pass that had read the window at the write instant would
            // have derived from scratch, measurement and all.
            let fresh = protected_surface_rect(
                live,
                Some(translated_bounds(composer, delta).expect("the composer moved with it")),
                &[],
            )
            .expect("the same surface, one move later");
            assert_eq!(anchored, fresh);
        }

        // A resize is explicitly not this question: the surface has to be
        // re-derived from a measurement, so the correction declines and the pass
        // places against its own reading rather than shifting a wrong size.
        assert_eq!(
            host_rect_translation(opened_against, [200, 100, 1500, 900]),
            None
        );
        // And a window that has not moved buys no correction at all.
        assert_eq!(host_rect_translation(opened_against, opened_against), None);
    }

    #[test]
    fn a_reveal_requires_a_sampled_native_background() {
        let source = overlay_source();
        let reveal = function_body(source, "fn reveal_protected_pair(");
        let gate = reveal
            .find("if !native_surface_is_paintable(app, epoch, host_generation)")
            .expect("a reveal must prove there is something to paint");
        let show = reveal.find("window.show()").expect("the single reveal");
        assert!(
            gate < show,
            "a transparent protected window must never be revealed unpainted"
        );
        // Both reveal sites are the gated helper, never a bare show.
        let guard = function_body(source, "fn start_guard(");
        assert_eq!(guard.matches("reveal_protected_pair(").count(), 2);
        assert!(!guard.contains("window.show()"));
        // A session that has no sample yet must take one instead of revealing.
        //
        // Asserted against the shape the guard actually has. This used to look
        // for `|| !native_surface_sampled` inside the resample condition; that
        // arm now lives in `native_surface_resample_required`, which is a pure
        // function with its own runtime coverage below, and the guard's own job
        // is to answer the question once per pass and feed it in. The literal
        // went stale and the assertion has been silently unreachable since --
        // the reason this file prefers behaviour to substrings.
        assert!(guard.contains("if !native_surface_sampled {"));
        assert!(guard.contains("native_surface_sampled =\n                            native_surface_is_paintable(&app, epoch, target.generation);"));
        // And the arm itself, where it now lives.
        assert!(native_surface_resample_required(false, false, false, false));
        assert!(!native_surface_resample_required(true, false, false, false));
    }

    #[test]
    fn a_stopped_guard_can_never_leave_the_pair_visible() {
        let source = overlay_source();
        let guard = function_body(source, "fn start_guard(");
        let ownership = guard
            .find("ProtectedWindowGuardOwnership::claim(&app, epoch)")
            .expect("the guard must own the pair it may reveal");
        // Claimed before the loop, so an early return, a fail-closed exit, or a
        // panic unwinding the guard thread all run the cleanup.
        let loop_start = guard.find("loop {").expect("guard loop");
        assert!(ownership < loop_start);
        let teardown = function_body(source, "impl Drop for ProtectedWindowGuardOwnership {");
        assert!(teardown.contains("release_protected_window_guard(self.epoch)"));
        assert!(teardown.contains("hide_protected_pair_or_destroy(&self.app)"));
        // An abnormal stop also ends the session, so no IPC path keeps working
        // against a session nothing is watching.
        assert!(teardown.contains("clear_and_hide(&self.app)"));
        // The teardown proves the hide landed instead of trusting it, because a
        // best-effort hide that never lands is what stranded a visible window.
        let escalation = function_body(source, "fn hide_protected_pair_or_destroy(");
        let hide = escalation.find("hide_window(app)").expect("hide first");
        let proof = escalation
            .find("window.is_visible()")
            .expect("the hide must be proven");
        let destroy = escalation
            .find(".close()")
            .expect("an unproven hide must escalate");
        let free = escalation
            .find("wait_for_freed_protected_labels(")
            .expect("a destroyed label must be freed");
        assert!(hide < proof);
        assert!(proof < destroy);
        assert!(destroy < free);
    }

    #[test]
    fn pair_ownership_is_released_only_by_the_generation_that_still_holds_it() {
        claim_protected_window_guard(41);
        assert!(release_protected_window_guard(41));
        // A second release must not order a newer owner's windows off screen.
        assert!(!release_protected_window_guard(41));

        claim_protected_window_guard(42);
        claim_protected_window_guard(43);
        assert!(!release_protected_window_guard(42));
        assert!(release_protected_window_guard(43));
        // With the pair unowned, no guard generation may claim responsibility.
        assert!(!release_protected_window_guard(44));
    }

    #[test]
    fn an_aborted_open_leaves_no_session_and_nothing_visible() {
        let show = function_body(overlay_source(), "pub(crate) fn show(");
        assert!(show.contains("show_guarded_overlay("));
        let clear = show
            .find("OverlaySessionState>().clear()")
            .expect("an aborted open must end its session");
        let hide = show
            .find("hide_protected_pair_or_destroy(app)")
            .expect("an aborted open must prove nothing is left visible");
        assert!(clear < hide);
    }

    #[test]
    fn a_window_that_cannot_be_proven_frameless_is_destroyed_not_retained() {
        let gate = function_body(overlay_source(), "fn ensure_retained_protected_window(");
        let contract = gate
            .find("apply_protected_frame_contract(&window, surface)")
            .expect("gated construction strips the frame");
        let destroy = gate
            .find(".close()")
            .expect("a window that keeps its caption must not be retained");
        let free = gate
            .find("wait_for_freed_protected_labels(")
            .expect("the label must be freed for the retry");
        assert!(contract < destroy);
        assert!(destroy < free);
    }

    #[test]
    fn no_session_can_exist_without_a_guard_watching_it() {
        let open = function_body(overlay_source(), "fn show_guarded_overlay(");
        assert!(
            open.contains("start_guard(") && open.contains("    )?;"),
            "a guard that cannot start must fail the open"
        );
    }

    #[test]
    fn retained_windows_survive_session_end_and_closes_free_their_labels() {
        let source = overlay_source();
        // The sub-100 ms toggle depends on the pair being hidden, not destroyed,
        // and on the renderer wipe that makes that retention safe.
        let hide = function_body(source, "fn hide_window(app:");
        assert!(hide.contains(".hide()"));
        assert!(!hide.contains(".close()"));
        let clear = function_body(source, "pub(crate) fn clear_and_hide(");
        assert!(clear.contains("hide_window(app)"));
        assert!(clear.contains("OVERLAY_SESSION_EVENT, false"));
        // The paths that do destroy the pair must leave the labels free, or the
        // next open either collides with a dying label or reuses a dead window.
        // A hide that cannot be proven escalates to a destroy, so the retained
        // pair is only ever given up when keeping it would strand a visible
        // window; the ordinary session end still only hides.
        let teardown = function_body(source, "fn hide_protected_pair_or_destroy(");
        let asked_to_hide = teardown.find("hide_window(app)").expect("hide first");
        let destroyed = teardown.find(".close()").expect("escalation");
        assert!(
            asked_to_hide < destroyed,
            "the retained pair may only be destroyed after a hide could not be proven"
        );
        for signature in [
            "fn close_window(",
            "fn show_guarded_overlay(",
            "fn hide_protected_pair_or_destroy(",
        ] {
            let body = function_body(source, signature);
            let close = body
                .find(".close()")
                .unwrap_or_else(|| panic!("{signature} lost its close"));
            let wait = body
                .find("wait_for_freed_protected_labels(")
                .unwrap_or_else(|| panic!("{signature} must wait for its labels"));
            assert!(close < wait, "{signature} must wait after closing");
        }
    }

    #[test]
    fn the_pre_warm_worker_runs_once_per_main_page_load() {
        // This hook fires on both PageLoadEvent::Started and
        // PageLoadEvent::Finished. Pre-warming on both raced two builders for the
        // same labels and produced two windows titled "OSL private composer".
        let hook = include_str!("main.rs")
            .split(".on_page_load(")
            .nth(1)
            .expect("page load hook")
            .split(".on_window_event(")
            .next()
            .expect("page load hook body");
        assert_eq!(hook.matches("native_discord_overlay::prewarm(").count(), 1);
        let gate = hook
            .find("PageLoadEvent::Finished")
            .expect("pre-warm must run for a single page-load event");
        let call = hook
            .find("native_discord_overlay::prewarm(")
            .expect("pre-warm call");
        assert!(gate < call, "pre-warm must be gated on one page-load event");
    }

    fn stub_previous_window(current: isize) -> isize {
        // A three-window chain, top first: 10 above 20 above 30.
        match current {
            30 => 20,
            20 => 10,
            _ => 0,
        }
    }

    #[test]
    fn a_bounded_z_order_walk_reports_only_a_proven_order() {
        // Proven above: the walk upwards from Discord meets the composer.
        assert_eq!(
            resolve_window_is_above(10, 30, 8, stub_previous_window),
            Some(true)
        );
        // Proven below: the walk reaches the top of the chain without it.
        assert_eq!(
            resolve_window_is_above(30, 10, 8, stub_previous_window),
            Some(false)
        );
        // Undecidable answers must never become a mutation: an exhausted bound,
        // a missing window, or a degenerate comparison are all "no drift".
        assert_eq!(
            resolve_window_is_above(10, 30, 1, stub_previous_window),
            None
        );
        assert_eq!(
            resolve_window_is_above(0, 30, 8, stub_previous_window),
            None
        );
        assert_eq!(
            resolve_window_is_above(10, 0, 8, stub_previous_window),
            None
        );
        assert_eq!(
            resolve_window_is_above(10, 10, 8, stub_previous_window),
            None
        );
    }

    #[test]
    fn the_protected_stack_is_re_asserted_only_when_it_actually_drifted() {
        // Clicking into Discord raises the borrowed sibling above the composer.
        // The correction must be driven by a read, because raising an
        // already-correct window on a timer is what interrupted WebView2
        // keyboard delivery.
        let source = overlay_source();
        let guard = function_body(source, "fn start_guard(");
        let cadence = guard
            .find("last_stack_probe.elapsed() >= PROTECTED_STACK_PROBE_INTERVAL")
            .expect("the z-order read must be rate limited");
        let read = guard
            .find("active_protected_stack_drifted(&app, discord_window)")
            .expect("the guard must read the order before changing it");
        let steady = guard
            .find("&& !stack_drifted")
            .expect("a drifted stack must not take the read-only fast path");
        let assertion = guard
            .find("if !ready || overlay_scale_changed || composer_restored || stack_drifted {")
            .expect("the stack is re-asserted on proven drift");
        assert!(cadence < read, "the cadence must gate the read");
        assert!(read < steady);
        assert!(steady < assertion);
        // And never while the borrowed window is moving. The correction the read
        // can trigger is the most expensive thing this guard does -- a
        // `settle_protected_window` round trip and an `hwnd()` getter, both through
        // the event-loop FIFO that a caption drag's modal move loop is holding --
        // and clicking OSL's header to begin the drag is exactly what reorders
        // these siblings and makes the probe find drift. That is the owner's "click
        // on the header and it is VERY laggy".
        let probe_gate = &guard[..cadence];
        assert!(
            probe_gate.contains("&& host_rect_is_still\n                        &&"),
            "a moving window must not be able to buy a stack correction"
        );
        // One `GetWindowRect` for the tick's cheap decisions, shared by the reclaim
        // gate, this probe and the read-only fast path -- the fast path used to take
        // its own. The two that remain are deliberately fresh reads taken *after*
        // the host reconcile, which is real work a moving window does not wait for.
        assert_eq!(
            guard
                .matches("exact_window_rect_matches(discord_window, last_rect)")
                .count(),
            3,
            "the tick's own read, the post-reconcile read, and the QA host-error fallback"
        );
        assert!(guard.contains(
            "let host_rect_is_still = exact_window_rect_matches(discord_window, last_rect);"
        ));
        // A move or a resize is not one of the reasons. `active_ensure_carrier_stack`
        // is a blocking UI-thread round trip plus a SetWindowPos, a style rewrite
        // and two DWM calls; running it per `WM_MOVE` is the reported drag lag,
        // and it cannot be needed because the placement is issued SWP_NOZORDER.
        let placement = function_body(source, "fn position_window_pair(");
        assert!(
            placement.contains("SWP_NOACTIVATE | SWP_NOZORDER"),
            "the placement must not be able to change the order it would then have to correct"
        );
        assert!(
            !guard.contains("|| geometry_changed || composer_restored"),
            "a geometry event must not re-assert the stack"
        );
        // The probe itself can only read.
        let probe = function_body(source, "fn protected_composer_is_above_discord(");
        assert!(!probe.contains("SetWindowPos"));
        assert!(!probe.contains(".show()"));
        assert!(!probe.contains(".hide()"));
        // Production may reorder the composer inside its band but never join the
        // topmost band, which its own ownership check forbids outright.
        let raise = function_body(source, "fn raise_protected_composer_above_discord(");
        assert!(raise.contains("window_is_topmost(discord_root as isize)"));
        assert!(!raise.contains("HWND_TOPMOST"));
        assert!(!raise.contains("SWP_SHOWWINDOW"));
    }

    #[test]
    fn a_composer_raise_is_only_ever_a_reorder_inside_one_band() {
        // `SetWindowPos(composer, discord, ...)` is a correction only while both
        // windows are in the same band. Across bands the same call is a band
        // change, and it is wrong in both directions: it would demote the QA
        // build's topmost composer out of the band that is the only reason it
        // stays above Discord, and it would promote a production composer into a
        // band its own ownership check fails closed on.
        assert!(composer_raise_is_a_same_band_reorder(false, false));
        assert!(composer_raise_is_a_same_band_reorder(true, true));
        assert!(!composer_raise_is_a_same_band_reorder(true, false));
        assert!(!composer_raise_is_a_same_band_reorder(false, true));
    }

    /// `SetWindowPos` z-order semantics, modelled: `order[0]` is the top of the
    /// stack, and `insert_after` names the window that ends up immediately
    /// ABOVE the positioned one (`0` is `HWND_TOP`). Taken from the behaviour
    /// this file already verifies in `ensure_shield_stack`, which passes the
    /// overlay as `hWndInsertAfter` to put the shield *behind* it and then
    /// proves it with `GW_HWNDNEXT`.
    fn apply_insert_after(order: &mut Vec<isize>, window: isize, insert_after: isize) {
        order.retain(|existing| *existing != window);
        let at = if insert_after == 0 {
            0
        } else {
            order
                .iter()
                .position(|existing| *existing == insert_after)
                .expect("hWndInsertAfter must be in the z-order")
                + 1
        };
        order.insert(at, window);
    }

    #[test]
    fn the_raise_actually_lands_the_composer_above_discord() {
        // The regression this catches is an inverted `hWndInsertAfter`, which no
        // string match can see: it compiles, it returns success from every
        // `SetWindowPos` call, and it reads exactly like the correct code. So the
        // argument is computed by a pure function and the resulting z-order is
        // simulated here.
        const COMPOSER: isize = 11;
        const DISCORD: isize = 22;
        const SHIELD: isize = 33;
        const OSL_MAIN: isize = 44;
        const OTHER_APP: isize = 55;

        // Discord in front of the composer: exactly what engaging the lock
        // produces, because it brings the borrowed window forward first.
        let mut order = vec![OTHER_APP, DISCORD, COMPOSER, SHIELD, OSL_MAIN];
        let above_discord = order[order
            .iter()
            .position(|window| *window == DISCORD)
            .expect("discord")
            - 1];
        let insert_after = composer_insert_after_above_discord(COMPOSER, DISCORD, above_discord)
            .expect("a composer below Discord must be corrected");
        assert_ne!(
            insert_after, DISCORD,
            "naming Discord as hWndInsertAfter puts the composer BELOW it"
        );
        apply_insert_after(&mut order, COMPOSER, insert_after);
        let composer_at = order.iter().position(|w| *w == COMPOSER).expect("composer");
        let discord_at = order.iter().position(|w| *w == DISCORD).expect("discord");
        assert!(
            composer_at < discord_at,
            "the composer must end up above Discord, got {order:?}"
        );

        // Discord at the very top of its band: the composer must go above the
        // whole band, which is HWND_TOP.
        let mut order = vec![DISCORD, COMPOSER, SHIELD, OSL_MAIN];
        let insert_after =
            composer_insert_after_above_discord(COMPOSER, DISCORD, 0).expect("a correction");
        assert_eq!(
            insert_after, 0,
            "HWND_TOP is the only handle above the band"
        );
        apply_insert_after(&mut order, COMPOSER, insert_after);
        assert_eq!(order.first(), Some(&COMPOSER));

        // Already immediately above Discord: no write at all. Re-asserting a
        // correct stack on a cadence is what interrupts WebView2 keyboard
        // delivery, and an inverted argument guarantees that cadence forever.
        assert_eq!(
            composer_insert_after_above_discord(COMPOSER, DISCORD, COMPOSER),
            None
        );
        // Degenerate reads never become a write.
        assert_eq!(
            composer_insert_after_above_discord(0, DISCORD, OTHER_APP),
            None
        );
        assert_eq!(
            composer_insert_after_above_discord(COMPOSER, 0, OTHER_APP),
            None
        );
        assert_eq!(
            composer_insert_after_above_discord(COMPOSER, COMPOSER, OTHER_APP),
            None
        );
    }

    #[test]
    fn the_raise_reads_back_what_it_landed() {
        // A successful `SetWindowPos` proves only that Windows accepted the
        // call. The order it produced has to be measured, exactly as
        // `ensure_shield_stack` measures its own.
        let source = overlay_source();
        let raise = function_body(source, "fn raise_protected_composer_above_discord(");
        let write = raise.find("SetWindowPos(").expect("the correction");
        let verify = raise
            .find("resolve_window_is_above(")
            .expect("the correction must prove the order it produced");
        assert!(write < verify, "the read-back must follow the write");
        // The argument is never Discord's own handle.
        assert!(!raise.contains("            discord_root,\n"));
        assert!(raise.contains("insert_after as windows_sys::Win32::Foundation::HWND"));
        // Only a proved inversion may fail; an exhausted walk never does.
        assert!(raise.contains("== Some(false)"));
    }

    #[test]
    fn production_recovers_the_caret_the_same_way_qa_does() {
        // Being above Discord and holding Discord's keyboard focus are two
        // different contracts. Production asserted only the first, so the
        // operator typed into Discord's own message box in the clear.
        assert!(active_should_reclaim_composer_focus(
            true, true, false, true
        ));
        // Narrow on purpose: it can never pull the caret away from an operator
        // who is doing something else.
        assert!(!active_should_reclaim_composer_focus(
            true, true, false, false
        ));
        assert!(!active_should_reclaim_composer_focus(
            true, false, false, true
        ));
        assert!(!active_should_reclaim_composer_focus(
            true, true, true, true
        ));
        assert!(!active_should_reclaim_composer_focus(
            false, true, false, true
        ));

        // And it may never end the session: a refused foreground change is not
        // an identity question, and taking the composer off screen is the one
        // outcome a presentation-only correction must not have.
        let guard = function_body(overlay_source(), "fn start_guard(");
        assert!(
            guard.contains("if active_focus_overlay(&window).is_ok() {"),
            "the steady-state focus reclaim must not be fatal"
        );
    }

    #[test]
    fn both_builds_own_the_composers_order_against_discord() {
        // The raise used to be compiled out of the QA build entirely, which left
        // that build with no writer relating the composer to Discord at all: its
        // carrier stack raised the composer to the topmost band and assumed that
        // settled it. Both variants must now name the borrowed window.
        let source = overlay_source();
        let raise_at = source
            .find("fn raise_protected_composer_above_discord(")
            .expect("the raise must exist");
        let gate = &source[source[..raise_at]
            .rfind("#[cfg(")
            .expect("the raise must carry a cfg")..raise_at];
        assert!(
            !gate.contains("discord-qa-shell"),
            "the raise must be compiled into both builds, not only production"
        );

        let qa_carrier = function_body(source, "fn active_ensure_carrier_stack(");
        assert!(
            qa_carrier.contains("raise_protected_composer_above_discord(overlay, discord_window)"),
            "the QA carrier stack must state its order against Discord, not assume it"
        );
        assert!(
            !qa_carrier.contains("_discord_window"),
            "the QA carrier stack may no longer ignore the borrowed window"
        );
        let production_carrier = {
            let at = source
                .find(concat!(
                    "#[cfg(not(feature = \"discord-qa-shell\"))]\n",
                    "#[cfg(target_os = \"windows\")]\n",
                    "fn active_ensure_carrier_stack("
                ))
                .expect("the production carrier stack");
            function_body(&source[at..], "fn active_ensure_carrier_stack(")
        };
        assert!(
            production_carrier
                .contains("raise_protected_composer_above_discord(overlay, discord_window)"),
            "the production carrier stack must keep its raise"
        );
    }

    #[test]
    fn a_live_run_states_the_composers_z_order_instead_of_implying_it() {
        // Three-valued and never optimistic: an exhausted walk or a cross-band
        // comparison is "unknown", which must never read as "above".
        assert_eq!(composer_zorder_label(Some(true)), "above-discord");
        assert_eq!(composer_zorder_label(Some(false)), "below-discord");
        assert_eq!(composer_zorder_label(None), "unknown");

        let source = overlay_source();
        let guard = function_body(source, "fn start_guard(");
        // Recorded where the bounded read already runs, so the breadcrumb never
        // buys a walk of its own outside the 200 ms probe cadence, and again
        // right after the only writer of the order.
        let probe = guard
            .find("qa_record_composer_zorder(&app, discord_window, \"probe\")")
            .expect("the probe must state the proved order");
        let after_write = guard
            .find("qa_record_composer_zorder(&app, discord_window, \"carrier_stack\")")
            .expect("the correction must read back what it landed");
        let correction = guard
            .find("active_ensure_carrier_stack(&window, &shield, discord_window, shielded)")
            .expect("the correction must exist");
        assert!(probe < correction);
        assert!(correction < after_write);
        // Visibility travels with the order, because the failure this breadcrumb
        // names was a composer that was above Discord the whole time and simply
        // not on screen.
        let record = function_body(source, "fn qa_record_composer_zorder(");
        assert!(record.contains("WS_VISIBLE"));
        assert!(record.contains("composer_zorder={order} visible={visible}"));
        // A breadcrumb may only read.
        assert!(!record.contains("SetWindowPos"));
        assert!(!record.contains(".show()"));
        assert!(!record.contains(".hide()"));
    }

    #[test]
    fn losing_the_foreground_never_takes_the_protected_composer_off_screen() {
        // The blocker. A ready session used to hide the composer *and* the shield
        // on every tick on which neither OSL nor Discord held the foreground, in
        // both builds, and a second copy of the same question ended the session
        // outright. The window stayed owned, positioned and stacked above
        // Discord, so every z-order reading of the stack looked healthy while the
        // operator's click and keystrokes went into Discord's real message box in
        // the clear.
        let source = overlay_source();
        let guard = function_body(source, "fn start_guard(");
        // The question is not answered-and-ignored any more, it is not asked.
        // Both the predicate and the guard's read of it are gone, so there is no
        // steady-state foreground answer left for a later edit to react to.
        assert!(!source.contains("fn trusted_focus_state("));
        assert!(!source.contains("fn active_trusted_focus_state("));
        assert!(!guard.contains("active_trusted_focus_state("));
        assert!(!guard.contains("foreground_is_trusted"));
        assert!(
            !guard.contains("trusted_foreground("),
            "the second copy of the same presentation question must stay deleted"
        );
        // The foreground reads that remain each have a reason, and none of them
        // is presentation: the eye refresh and the surface backstop gate on
        // `osl_process_is_foreground` to avoid driving Electron from the
        // background, and the focus reclaim asks whether Discord took the caret
        // back. Neither can hide anything.
        // Only the first-open decision may still refuse an untrusted foreground.
        assert_eq!(
            guard
                .matches("The native Discord window is no longer foreground")
                .count(),
            1,
            "an already-open session may no longer be ended by losing the foreground"
        );
        let first_open = guard
            .find("FirstGuardDecision::Close => {")
            .expect("the first-open foreground policy must survive untouched");
        assert!(
            first_open
                < guard
                    .find("The native Discord window is no longer foreground")
                    .expect("the surviving refusal")
        );

        // Only the transitions that are genuinely not "the composer is on screen
        // and the operator is typing into it" may take it off screen: the operator
        // taking the surface away (a minimized owner, or a lock down with nothing
        // painted), a composer measurement that cannot see past the surface itself,
        // the native-background re-sample, and a surrendered composer band with no
        // row left above Discord's message box. Anything else is another way to
        // lose the operator's keystrokes.
        //
        // "Protection being off entirely" was once a separate bail-out and was
        // deleted on the grounds that the lock decides whether OSL *encrypts*,
        // never whether the operator has somewhere to type. Deleting it also
        // deleted the only thing that took the composer off Discord's message box
        // when the operator switched it off, which is the measured defect; it is
        // back, but inside the presence rule -- `protected_surface_presence` --
        // rather than as a geometry answer, so a lock that is *on* still cannot be
        // read as "nothing on screen" anywhere.
        //
        // Two of them assert the flag outright. The presence branch and the
        // surrendered-band branch instead *read back* whether the pair actually
        // left the screen, which is the stronger form: `hide_window` only asks
        // Tauri, on the event loop, and returns before anything has moved, so
        // latching the flag on the strength of that request is what once left a
        // composer on the desktop over a minimized owner with every retry
        // suppressed by bookkeeping that was simply wrong.
        assert_eq!(
            guard.matches("composer_temporarily_hidden = true;").count(),
            2
        );
        // Every read-back site goes through the one helper, so none of them can
        // drift into recording a hide it has not proved. Three: the presence rule,
        // and the two points at which a surrendered composer band turns out to have
        // no painted row left above Discord's message box -- once when the surface
        // is derived and once when the placement re-derives it against the bounds it
        // is actually issued with.
        assert_eq!(
            guard
                .matches("leave_the_screen(&app, composer_temporarily_hidden)")
                .count(),
            3
        );
        assert!(!guard.contains("hide_window(&app);"));
        let leave = strip_line_comments(function_body(overlay_source(), "fn leave_the_screen("));
        let asked = leave
            .find("hide_window(app)")
            .expect("the hide must be issued");
        let proved = leave
            .find("protected_pair_is_off_screen(app)")
            .expect("and it must be read back");
        assert!(
            asked < proved,
            "the pair must actually leave the screen, not merely be recorded as gone"
        );
        // And one of the reasons is keyed on the host being off screen, which is the
        // only reason that is about the window rather than about attention. A
        // composer that is merely unfocused, occluded or moving is never iconic, so
        // this cannot be reached by a drag -- the property measured at zero
        // presence transitions across one.
        let minimize = guard
            .find("owner_window_is_minimized(&main)")
            .expect("the owner's window state must be what decides this");
        let bookkeeping = guard[minimize..]
            .find("leave_the_screen(&app, composer_temporarily_hidden)")
            .expect("the restore path must know whether the pair left the screen");
        // `bookkeeping` is an offset *into `guard[minimize..]`*, so the branch body
        // is that slice's prefix. Slicing `guard[minimize..bookkeeping]` instead
        // mixed a relative index with an absolute one and could only ever panic,
        // which is an assertion that never ran.
        assert!(
            !guard[minimize..][..bookkeeping].contains("composer_temporarily_hidden = true"),
            "the presence branch must never assert a hide it has not read back"
        );
    }

    #[test]
    fn every_win32_reveal_also_strips_the_frame_it_restores() {
        // tao rebuilds the whole window style from its own cached flags, and that
        // style always carries WS_CAPTION|WS_SYSMENU, so a hide leaves the
        // caption waiting in the style bits. Showing a protected window with
        // SWP_SHOWWINDOW therefore puts a captioned surface on screen unless the
        // frame is stripped after it, exactly as after a Tauri reveal.
        let source = overlay_source();
        let mut reveals = 0;
        for (at, _) in source.match_indices("| SWP_SHOWWINDOW,") {
            let after = &source[at..];
            let after = &after[..after.len().min(900)];
            assert!(
                after.contains("enforce_native_frameless_overlay("),
                "a Win32 reveal lost its post-show frameless enforcement"
            );
            reveals += 1;
        }
        // The shield stack and the QA composer raise. A third one would be a
        // third way to reveal a captioned protected window.
        assert_eq!(reveals, 2);
    }

    #[test]
    fn a_verified_session_is_announced_at_its_transition_and_at_every_reveal() {
        // The renderer can only become ready by reading a verified session, and a
        // retained WebView that was not listening at the one transition
        // announcement refused every keystroke for the rest of the session.
        let guard = function_body(overlay_source(), "fn start_guard(");
        assert_eq!(guard.matches("OVERLAY_SESSION_EVENT, true").count(), 2);
        let restore = guard
            .split("let composer_restored = composer_temporarily_hidden;")
            .nth(1)
            .expect("the restore reveal");
        let ready_gate = restore
            .find("if ready {")
            .expect("a reveal only re-announces a session that is already ready");
        let repeat = restore
            .find("OVERLAY_SESSION_EVENT, true")
            .expect("every reveal of a ready session repeats the announcement");
        assert!(ready_gate < repeat);
        let commit = guard
            .find("mark_ready(epoch, &host)?")
            .expect("the phase transition");
        let announce = guard[commit..]
            .find("OVERLAY_SESSION_EVENT, true")
            .expect("the transition must announce the readable session");
        assert!(
            announce > 0,
            "readiness is committed before it is announced"
        );
    }

    #[test]
    fn the_protected_renderer_never_loses_its_readiness_poll() {
        // The announcement above is the fast path, never the only path. The
        // renderer must always have one verification poll pending while it is not
        // ready, because a renderer that refuses every Enter while the composer is
        // on screen is indistinguishable from a broken app.
        let renderer = include_str!("../../osl-hub-ui/src/overlay.ts");
        let discard = function_body(renderer, "function discardProtectedSession(): void {");
        assert!(discard.contains("overlayReady = false;"));
        assert!(
            discard.contains("scheduleOverlayInit();"),
            "a discarded session must re-arm the readiness poll"
        );
        assert!(!discard.contains("cancelOverlayInit"));
        let init = function_body(
            renderer,
            "async function initializeOverlay(): Promise<void> {",
        );
        let ready = init
            .find("overlayReady = true;")
            .expect("initialization is the only thing that can grant readiness");
        let cancel = init
            .find("cancelOverlayInit();")
            .expect("a verified session stops polling");
        assert!(cancel < ready);
        // One re-arm for an unreadable state, one for a rejected read, one for a
        // discarded session. Polling is cancelled only by a verified session.
        assert_eq!(init.matches("scheduleOverlayInit();").count(), 2);
        assert_eq!(renderer.matches("scheduleOverlayInit();").count(), 3);
    }

    /// Every command the protected renderer invokes is actually granted to it.
    ///
    /// THE defect this exists to make impossible, and the one that kept the eye
    /// dark: `rehydrate_native_discord_overlay_history` was written, registered in
    /// `generate_handler!`, and reachable in every source-level sense -- and had
    /// no permission in `permissions/hub.toml` and no grant in
    /// `capabilities/native-discord-overlay.json`. Tauri therefore refused every
    /// invoke at the ACL boundary, BEFORE the command body ran, so not one of the
    /// command's own breadcrumbs could ever be written and the renderer saw only a
    /// rejected promise it journals in memory. The feature could not work, and
    /// could not report that it could not work.
    ///
    /// Nothing else in the build catches this: it compiles, it links, the command
    /// is in the handler list, and every test that reads Rust source passes.
    #[test]
    fn every_command_the_protected_renderer_invokes_is_granted_to_its_webview() {
        let renderer = include_str!("../../osl-hub-ui/src/overlay.ts");
        let capability = include_str!("../capabilities/native-discord-overlay.json");
        let manifest = include_str!("../permissions/hub.toml");

        // The renderer's own direct invokes. Adapters shared with the main window
        // live elsewhere and are granted by that window's capability.
        let mut invoked = Vec::new();
        for fragment in renderer.split("await invoke<unknown>(\"").skip(1) {
            let name = fragment
                .split('"')
                .next()
                .expect("an invoke names its command");
            assert!(
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "a command name is a fixed snake_case literal, never built at runtime"
            );
            invoked.push(name);
        }
        invoked.sort_unstable();
        invoked.dedup();
        // If this ever reaches zero the test has stopped testing anything, most
        // likely because the invoke spelling changed.
        assert!(
            invoked.contains(&"rehydrate_native_discord_overlay_history"),
            "the eye's own read must be among the renderer's invokes"
        );

        for command in invoked {
            let permission = format!("allow-{}", command.replace('_', "-"));
            // Declared: the permission has to exist before anything can grant it.
            assert!(
                manifest.contains(&format!("identifier = \"{permission}\"")),
                "{command} has no permission in permissions/hub.toml, so no capability can grant it and every invoke is refused before the command runs"
            );
            // ...and bound to exactly this command, so a near-miss identifier
            // cannot look like a grant.
            assert!(
                manifest.contains(&format!("commands.allow = [\"{command}\"]")),
                "{permission} must allow exactly {command}"
            );
            // Granted: to this webview, which is the only one that may ask.
            assert!(
                capability.contains(&format!("\"{permission}\"")),
                "{command} is not granted to the native-discord-overlay webview, so Tauri refuses it at the ACL boundary and the command's own breadcrumbs can never be written"
            );
        }

        // The capability stays narrow: it grants the overlay webview and nothing
        // else, and claims no host, window, filesystem, shell or network authority.
        assert!(capability.contains("\"webviews\": [\"native-discord-overlay\"]"));
        for forbidden in ["core:webview:", "core:window:", "fs:", "shell:", "http:"] {
            assert!(
                !capability.contains(forbidden),
                "the protected layer must not gain {forbidden} authority"
            );
        }
    }

    /// Every leg of the eye's read says what it did, and the entry says it was
    /// asked at all.
    ///
    /// This is the test for the defect that made the eye undiagnosable: six
    /// preconditions could refuse the transcript read and each one returned a bare
    /// `Err` to a renderer that journals errors in memory only. The result was a
    /// display feature which, on a live machine with the QA trail enabled and the
    /// eye on, produced no artifact whatsoever -- so "the backend refused" and
    /// "the renderer never asked" were the same observation, and every fix was a
    /// guess.
    #[test]
    fn the_eye_names_every_leg_of_its_read_including_the_ones_that_refuse() {
        let labels = [
            REHYDRATE_ENTERED,
            REHYDRATE_REFUSED_CALLER,
            REHYDRATE_REFUSED_SCOPE,
            REHYDRATE_CONTEXT_UNAVAILABLE,
            REHYDRATE_OWNER_UNAVAILABLE,
            REHYDRATE_SCOPE_BINDING_UNAVAILABLE,
            REHYDRATE_READ_UNAVAILABLE,
            REHYDRATE_CONTEXT_CHANGED,
            REHYDRATE_FRAME_ABSENT,
            REHYDRATE_ROWS_PLACED,
            REHYDRATE_ROWS_UNPLACEABLE,
            REHYDRATE_SHIPPED,
        ];
        let mut unique = labels.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            labels.len(),
            "two legs may never share a label"
        );
        for label in labels {
            assert!(label.starts_with("rehydrate_"));
            assert!(label.is_ascii() && !label.contains(' ') && !label.contains('{'));
        }

        let command = function_body(
            include_str!("main.rs"),
            "async fn rehydrate_native_discord_overlay_history(",
        );
        // The entry is named FIRST, unconditionally, before the caller check --
        // the only way this process can distinguish "refused" from "never asked".
        let entered = command
            .find("qa_discord_rehydrate_stage(native_discord_overlay::REHYDRATE_ENTERED, None);")
            .expect("the command names its own entry");
        let caller = command
            .find("if caller.label() != native_discord_overlay::OVERLAY_LABEL {")
            .expect("the caller check exists");
        assert!(
            entered < caller,
            "entry is named before anything can refuse"
        );

        // Every pre-read precondition carries its own label out. A bare `?` on any
        // of these is exactly the silence this test exists to forbid.
        for leg in [
            "REHYDRATE_CONTEXT_UNAVAILABLE,\n            require_overlay_context_snapshot(&app),",
            "REHYDRATE_OWNER_UNAVAILABLE,\n            active_unlocked_osl_user_id(&core),",
            "REHYDRATE_SCOPE_BINDING_UNAVAILABLE,\n            native_discord_scope_binding(&app),",
            "REHYDRATE_READ_UNAVAILABLE,\n            osl_privacy_hub::native_discord_adapter::read_visible_message_rows(",
        ] {
            assert!(
                command.contains(leg),
                "an unlabelled pre-read refusal is invisible: {leg}"
            );
        }
        // And the refusal itself is unchanged: the helper reports and returns the
        // caller's own error value, never a substitute for it.
        let helper = function_body(include_str!("main.rs"), "fn qa_named_rehydrate_refusal<T>(");
        assert!(helper.contains("qa_discord_rehydrate_stage(stage, None);"));
        assert!(helper.contains("result"));
        // The error VALUE must never reach the trail -- only the fixed label.
        assert!(!helper.contains("Some("));
        assert!(!helper.contains("format!"));

        // Placement is counted separately from decoding, because "OSL opened
        // nothing" and "OSL opened rows the window is not over yet" have different
        // fixes and looked identical from outside.
        assert!(command.contains("REHYDRATE_ROWS_PLACED, Some(placed)"));
        assert!(command.contains("REHYDRATE_ROWS_UNPLACEABLE,\n            Some(unplaceable),"));
        assert!(command.contains("REHYDRATE_FRAME_ABSENT, None"));
        // Nothing on this path may write text. The trail takes a label and a
        // count, and the count is a `usize`.
        assert!(!command.contains("qa_discord_rehydrate_stage(&"));
        assert!(!command.contains("flagtext}"));
    }
}
