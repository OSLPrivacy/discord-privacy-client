//! Accessibility boundary for an OSL-claimed native Discord window.
//!
//! This module never uses a WebView, DevTools, DOM/JavaScript injection,
//! credentials, cookies, private APIs, process memory, or message-history
//! scraping. A snapshot is requested on demand, bounded by node/time/result
//! limits, and contains only hashes and geometry. A separately calibrated
//! composer path may enter one fixed, non-secret OSL carrier after an explicit
//! trusted-overlay gesture. It is Discord-specific and reads only bounded
//! visible accessibility names, runtime ids, focus, bounds, and at most the
//! current composer value to prove it is empty or equals OSL's fixed marker.
//! That value is never retained, returned, or logged. The visible labels are
//! user-confirmed context evidence, not provider-authenticated Discord account
//! proof. The adapter revalidates the signed window generation,
//! visible label, runtime id, bounds, and foreground focus before placement,
//! every compatibility character, and Enter.

#[cfg(any(target_os = "windows", test))]
use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};

#[cfg(any(target_os = "windows", test))]
use std::sync::Mutex;

#[cfg(any(target_os = "windows", test))]
const MAX_CARRIERS: usize = 32;
#[cfg(any(target_os = "windows", test))]
const MAX_ACCESSIBLE_TEXT_BYTES: usize = 16 * 1024;
#[cfg(any(target_os = "windows", test))]
const DPC0_PREFIX: &str = "DPC0::";

const MAX_COVER_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscordCarrierMode {
    Atomic,
    Compatibility,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscordCarrierStatus {
    Sent,
    CalibrationRequired,
    ContextChanged,
    ComposerUnavailable,
    ComposerNotEmpty,
    PlacementRejected,
    EnterRejected,
    PlatformUnsupported,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordComposerCalibrationReceipt {
    pub calibrated: bool,
    pub generation: u64,
    pub conversation_binding_hash: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordCarrierReceipt {
    pub placed: bool,
    pub enter_sent: bool,
    pub status: DiscordCarrierStatus,
    pub mode: DiscordCarrierMode,
    pub compatibility_delay_ms: u16,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone, Eq, PartialEq)]
struct ComposerBinding {
    generation: u64,
    scope_binding_hash: String,
    conversation_binding_hash: String,
    composer_name_hash: String,
    automation_id_hash: String,
    class_name_hash: String,
    runtime_id: Vec<i32>,
    bounds: AccessibilityBounds,
}

/// One-message-only calibration. It contains hashes and geometry, never a
/// Discord title, account value, message, credential, token, or timing trace.
#[derive(Default)]
pub struct NativeDiscordComposerState {
    #[cfg(any(target_os = "windows", test))]
    binding: Mutex<Option<ComposerBinding>>,
}

/// Convert an aggregate characters-per-second estimate into one coarse delay.
/// Exact key cadence is never accepted, retained, logged, or replayed.
pub fn compatibility_delay_ms(chars_per_second: u16) -> u16 {
    let bounded_cps = if chars_per_second == 0 {
        6
    } else {
        chars_per_second.clamp(2, 16)
    };
    (1_000 + bounded_cps / 2) / bounded_cps
}

impl NativeDiscordComposerState {
    /// Whether this overlay session has a verified Discord composer binding.
    /// This is only an availability hint for the UI; `place_carrier` still
    /// performs every generation, context, focus, bounds, runtime-id, and
    /// empty-composer check again before touching Discord.
    pub fn marker_available(&self) -> bool {
        #[cfg(any(target_os = "windows", test))]
        {
            self.binding
                .lock()
                .map(|binding| binding.is_some())
                .unwrap_or(false)
        }
        #[cfg(not(any(target_os = "windows", test)))]
        {
            false
        }
    }

    pub fn clear(&self) {
        #[cfg(any(target_os = "windows", test))]
        if let Ok(mut binding) = self.binding.lock() {
            *binding = None;
        }
    }

    pub fn calibrate(
        &self,
        host: &crate::native_window_host::NativeWindowHostState,
        owner_osl_user_id: &str,
        scope_binding: &str,
    ) -> Result<DiscordComposerCalibrationReceipt, String> {
        #[cfg(target_os = "windows")]
        {
            host.with_current_discord_accessibility_target(owner_osl_user_id, |target, trusted| {
                windows::calibrate(self, target, trusted, scope_binding)
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (host, owner_osl_user_id, scope_binding);
            Err("The native Discord composer is unavailable on this platform".to_owned())
        }
    }

    pub fn place_carrier(
        &self,
        host: &crate::native_window_host::NativeWindowHostState,
        owner_osl_user_id: &str,
        scope_binding: &str,
        mode: DiscordCarrierMode,
        chars_per_second: u16,
        carrier: &str,
    ) -> DiscordCarrierReceipt {
        let delay = compatibility_delay_ms(chars_per_second);
        if !valid_cover(carrier) {
            return carrier_failure(mode, delay, DiscordCarrierStatus::PlacementRejected);
        }
        #[cfg(target_os = "windows")]
        {
            host.with_current_discord_accessibility_target(owner_osl_user_id, |target, trusted| {
                Ok(windows::place(
                    self,
                    target,
                    trusted,
                    scope_binding,
                    mode,
                    delay,
                    carrier,
                ))
            })
            .unwrap_or_else(|_| carrier_failure(mode, delay, DiscordCarrierStatus::ContextChanged))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (host, owner_osl_user_id, scope_binding, carrier);
            carrier_failure(mode, delay, DiscordCarrierStatus::PlatformUnsupported)
        }
    }
}

fn valid_cover(carrier: &str) -> bool {
    !carrier.is_empty()
        && carrier.len() <= MAX_COVER_BYTES
        && !carrier.chars().any(char::is_control)
}

fn carrier_failure(
    mode: DiscordCarrierMode,
    compatibility_delay_ms: u16,
    status: DiscordCarrierStatus,
) -> DiscordCarrierReceipt {
    DiscordCarrierReceipt {
        placed: false,
        enter_sent: false,
        status,
        mode,
        compatibility_delay_ms,
    }
}

#[cfg(any(target_os = "windows", test))]
fn drive_compatibility_carrier(
    carrier: &str,
    mut checkpoint: impl FnMut(&str) -> bool,
    mut send_character: impl FnMut(char) -> bool,
    mut wait: impl FnMut(),
) -> bool {
    let mut placed_prefix = String::new();
    carrier.chars().all(|character| {
        if !checkpoint(&placed_prefix) || !send_character(character) {
            return false;
        }
        placed_prefix.push(character);
        wait();
        true
    })
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DiscordSnapshotReason {
    Ready,
    PlatformUnsupported,
    NotHosted,
    HostIdentityChanged,
    AccessibilityUnavailable,
    NodeOrTimeLimitExceeded,
    SelfAccountUnproven,
    ConversationUnproven,
    ParticipantsUnproven,
    ComposerUnproven,
    CarrierAuthorUnproven,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AccessibilityBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl AccessibilityBounds {
    #[cfg(any(target_os = "windows", test))]
    fn valid(self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DiscordComposerSnapshot {
    pub locator_hash: String,
    pub bounds: AccessibilityBounds,
    pub has_keyboard_focus: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DiscordCarrierSnapshot {
    pub locator_hash: String,
    pub ciphertext_hash: String,
    pub author_identity_hash: String,
    pub bounds: AccessibilityBounds,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NativeDiscordAccessibilitySnapshot {
    pub generation: u64,
    pub available: bool,
    pub reason: DiscordSnapshotReason,
    pub self_account_identity_hash: Option<String>,
    pub conversation_identity_hash: Option<String>,
    pub participant_identity_hashes: Vec<String>,
    pub composer: Option<DiscordComposerSnapshot>,
    pub visible_carriers: Vec<DiscordCarrierSnapshot>,
}

impl NativeDiscordAccessibilitySnapshot {
    pub(crate) fn unavailable(generation: u64, reason: DiscordSnapshotReason) -> Self {
        Self {
            generation,
            available: false,
            reason,
            self_account_identity_hash: None,
            conversation_identity_hash: None,
            participant_identity_hashes: Vec::new(),
            composer: None,
            visible_carriers: Vec::new(),
        }
    }
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone)]
struct ProvenComposer {
    locator: String,
    bounds: AccessibilityBounds,
    has_keyboard_focus: bool,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Clone)]
struct ProvenCarrier {
    locator: String,
    ciphertext: String,
    author_identity: String,
    bounds: AccessibilityBounds,
}

#[cfg(any(target_os = "windows", test))]
#[derive(Default)]
struct ProvenEvidence {
    self_account_identity: Option<String>,
    conversation_identity: Option<String>,
    participant_identities: Vec<String>,
    composer: Option<ProvenComposer>,
    carriers: Vec<ProvenCarrier>,
}

#[cfg(any(target_os = "windows", test))]
fn assemble_snapshot(
    generation: u64,
    evidence: ProvenEvidence,
) -> NativeDiscordAccessibilitySnapshot {
    let self_identity = match bounded_identity(evidence.self_account_identity.as_deref()) {
        Some(value) => value,
        None => {
            return NativeDiscordAccessibilitySnapshot::unavailable(
                generation,
                DiscordSnapshotReason::SelfAccountUnproven,
            )
        }
    };
    let conversation = match bounded_identity(evidence.conversation_identity.as_deref()) {
        Some(value) => value,
        None => {
            return NativeDiscordAccessibilitySnapshot::unavailable(
                generation,
                DiscordSnapshotReason::ConversationUnproven,
            )
        }
    };
    let mut participant_hashes = evidence
        .participant_identities
        .iter()
        .filter_map(|value| bounded_identity(Some(value)))
        .map(|value| stable_hash("discord-participant", value))
        .collect::<Vec<_>>();
    participant_hashes.sort_unstable();
    participant_hashes.dedup();
    if participant_hashes.is_empty() || participant_hashes.len() > 64 {
        return NativeDiscordAccessibilitySnapshot::unavailable(
            generation,
            DiscordSnapshotReason::ParticipantsUnproven,
        );
    }
    let composer = match evidence.composer {
        Some(composer)
            if bounded_locator(&composer.locator).is_some() && composer.bounds.valid() =>
        {
            DiscordComposerSnapshot {
                locator_hash: stable_hash("discord-accessibility-locator", &composer.locator),
                bounds: composer.bounds,
                has_keyboard_focus: composer.has_keyboard_focus,
            }
        }
        _ => {
            return NativeDiscordAccessibilitySnapshot::unavailable(
                generation,
                DiscordSnapshotReason::ComposerUnproven,
            )
        }
    };
    let had_carriers = !evidence.carriers.is_empty();
    let mut carriers = Vec::with_capacity(evidence.carriers.len().min(MAX_CARRIERS));
    for carrier in evidence.carriers.into_iter().take(MAX_CARRIERS) {
        let Some(locator) = bounded_locator(&carrier.locator) else {
            continue;
        };
        let Some(author) = bounded_identity(Some(&carrier.author_identity)) else {
            continue;
        };
        if !carrier.bounds.valid() || !strict_ciphertext_candidate(&carrier.ciphertext) {
            continue;
        }
        carriers.push(DiscordCarrierSnapshot {
            locator_hash: stable_hash("discord-accessibility-locator", locator),
            ciphertext_hash: stable_hash("discord-visible-ciphertext", &carrier.ciphertext),
            author_identity_hash: stable_hash("discord-author", author),
            bounds: carrier.bounds,
        });
    }
    if had_carriers && carriers.is_empty() {
        return NativeDiscordAccessibilitySnapshot::unavailable(
            generation,
            DiscordSnapshotReason::CarrierAuthorUnproven,
        );
    }
    NativeDiscordAccessibilitySnapshot {
        generation,
        available: true,
        reason: DiscordSnapshotReason::Ready,
        self_account_identity_hash: Some(stable_hash("discord-self-account", self_identity)),
        conversation_identity_hash: Some(stable_hash("discord-conversation", conversation)),
        participant_identity_hashes: participant_hashes,
        composer: Some(composer),
        visible_carriers: carriers,
    }
}

#[cfg(any(target_os = "windows", test))]
fn bounded_identity(value: Option<&str>) -> Option<&str> {
    value.filter(|value| {
        !value.is_empty()
            && value.len() <= 512
            && !value.chars().any(char::is_control)
            && value.trim() == *value
    })
}

#[cfg(any(target_os = "windows", test))]
fn bounded_locator(value: &str) -> Option<&str> {
    (!value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control))
        .then_some(value)
}

#[cfg(any(target_os = "windows", test))]
fn normalized_conversation_from_composer(name: &str) -> Option<&str> {
    let value = name.trim();
    let rest = value
        .strip_prefix("Message ")
        .or_else(|| value.strip_prefix("message "))?
        .trim_start_matches(['@', '#'])
        .trim();
    bounded_identity(Some(rest))
}

#[cfg(any(target_os = "windows", test))]
fn conversation_name_variants(conversation: &str) -> Vec<String> {
    let Some(conversation) = bounded_identity(Some(conversation)) else {
        return Vec::new();
    };
    let normalized = conversation.trim_start_matches(['@', '#']).trim();
    if normalized.is_empty() {
        return Vec::new();
    }
    vec![
        normalized.to_owned(),
        format!("@{normalized}"),
        format!("#{normalized}"),
    ]
}

#[cfg(any(target_os = "windows", test))]
fn visible_header_bounds(header: AccessibilityBounds, root: AccessibilityBounds) -> bool {
    let header_limit = root.top + ((root.bottom - root.top) / 4).max(96);
    header.valid()
        && header.left >= root.left
        && header.top >= root.top
        && header.right <= root.right
        && header.bottom <= header_limit
        && header.right > root.left + 240
}

#[cfg(any(target_os = "windows", test))]
fn stable_hash(domain: &str, value: &str) -> String {
    let mut hash = Sha256::new();
    for part in [domain.as_bytes(), value.as_bytes()] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(any(target_os = "windows", test))]
fn strict_ciphertext_candidate(value: &str) -> bool {
    if value.len() < DPC0_PREFIX.len() + 64
        || value.len() > MAX_ACCESSIBLE_TEXT_BYTES
        || !value.starts_with(DPC0_PREFIX)
    {
        return false;
    }
    let body = &value[DPC0_PREFIX.len()..];
    if body.len() % 4 != 0 {
        return false;
    }
    let padding_start = body.find('=').unwrap_or(body.len());
    if body[padding_start..].len() > 2 || !body[padding_start..].bytes().all(|byte| byte == b'=') {
        return false;
    }
    body[..padding_start]
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
}

/// One bounded request against the currently claimed native Discord host.
/// There is intentionally no timer, watcher, event subscription, or polling.
pub fn snapshot_native_discord(
    host: &crate::native_window_host::NativeWindowHostState,
) -> NativeDiscordAccessibilitySnapshot {
    host.discord_accessibility_snapshot()
}

#[cfg(target_os = "windows")]
pub(crate) fn snapshot_claimed_window(
    target: crate::native_window_host::NativeDiscordAccessibilityTarget,
    process_is_trusted: &dyn Fn(u32) -> bool,
) -> NativeDiscordAccessibilitySnapshot {
    windows::snapshot(target, process_is_trusted)
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use ::windows::Win32::Foundation::{HWND, POINT};
    use ::windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use ::windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
        SafeArrayUnaccessData,
    };
    use ::windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
        IUIAutomationTreeWalker, IUIAutomationValuePattern, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId, UIA_TextPatternId, UIA_ValuePatternId,
    };
    use std::ffi::c_void;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
        VK_RETURN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, SetForegroundWindow, GA_ROOT,
    };

    const SNAPSHOT_TIMEOUT: Duration = Duration::from_millis(300);

    struct ComGuard(bool);

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    #[derive(Clone)]
    struct LocatedComposer {
        element: IUIAutomationElement,
        binding: ComposerBinding,
    }

    fn runtime_id(element: &IUIAutomationElement) -> Result<Vec<i32>, String> {
        let array = unsafe { element.GetRuntimeId() }
            .map_err(|_| "Discord did not expose a stable composer runtime id".to_owned())?;
        if array.is_null() {
            return Err("Discord did not expose a stable composer runtime id".to_owned());
        }
        let result = (|| {
            let lower = unsafe { SafeArrayGetLBound(array, 1) }
                .map_err(|_| "Discord composer runtime id is invalid".to_owned())?;
            let upper = unsafe { SafeArrayGetUBound(array, 1) }
                .map_err(|_| "Discord composer runtime id is invalid".to_owned())?;
            if upper < lower || upper - lower > 64 {
                return Err("Discord composer runtime id is invalid".to_owned());
            }
            let mut data: *mut c_void = std::ptr::null_mut();
            unsafe { SafeArrayAccessData(array, &mut data) }
                .map_err(|_| "Discord composer runtime id is unavailable".to_owned())?;
            let length = (upper - lower + 1) as usize;
            let values = if data.is_null() {
                Vec::new()
            } else {
                unsafe { std::slice::from_raw_parts(data.cast::<i32>(), length) }.to_vec()
            };
            let _ = unsafe { SafeArrayUnaccessData(array) };
            if values.is_empty() {
                return Err("Discord composer runtime id is unavailable".to_owned());
            }
            Ok(values)
        })();
        let _ = unsafe { SafeArrayDestroy(array) };
        result
    }

    fn element_bounds(element: &IUIAutomationElement) -> Option<AccessibilityBounds> {
        let rect = unsafe { element.CurrentBoundingRectangle() }.ok()?;
        let bounds = AccessibilityBounds {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        bounds.valid().then_some(bounds)
    }

    fn composer_text(element: &IUIAutomationElement) -> Option<String> {
        if let Ok(pattern) =
            unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        {
            if !unsafe { pattern.CurrentIsReadOnly() }.is_ok_and(|value| !value.as_bool()) {
                return None;
            }
            return unsafe { pattern.CurrentValue() }
                .ok()
                .map(|value| value.to_string())
                .filter(|value| value.len() <= 256 && !value.chars().any(char::is_control));
        }
        if let Ok(pattern) =
            unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }
        {
            return unsafe { pattern.DocumentRange() }
                .and_then(|range| unsafe { range.GetText(257) })
                .ok()
                .map(|value| value.to_string())
                .filter(|value| value.len() <= 256 && !value.chars().any(char::is_control));
        }
        None
    }

    fn composer_empty(element: &IUIAutomationElement) -> bool {
        composer_text(element).is_some_and(|value| value.is_empty())
    }

    fn trusted_visible_element(
        element: &IUIAutomationElement,
        root_bounds: AccessibilityBounds,
        process_is_trusted: &dyn Fn(u32) -> bool,
    ) -> Option<AccessibilityBounds> {
        let process_id = unsafe { element.CurrentProcessId() }.ok()?;
        if process_id <= 0 || !process_is_trusted(process_id as u32) {
            return None;
        }
        if unsafe { element.CurrentIsOffscreen() }.is_ok_and(|value| value.as_bool())
            || !unsafe { element.CurrentIsEnabled() }.is_ok_and(|value| value.as_bool())
        {
            return None;
        }
        let bounds = element_bounds(element)?;
        (bounds.left >= root_bounds.left
            && bounds.top >= root_bounds.top
            && bounds.right <= root_bounds.right
            && bounds.bottom <= root_bounds.bottom)
            .then_some(bounds)
    }

    fn candidate_from_ancestor_chain(
        start: IUIAutomationElement,
        walker: &IUIAutomationTreeWalker,
        root_bounds: AccessibilityBounds,
        process_is_trusted: &dyn Fn(u32) -> bool,
        started: Instant,
        predicate: impl Fn(&IUIAutomationElement, AccessibilityBounds) -> bool,
    ) -> Option<IUIAutomationElement> {
        let mut current = start;
        for _ in 0..12 {
            if started.elapsed() > SNAPSHOT_TIMEOUT {
                return None;
            }
            if trusted_visible_element(&current, root_bounds, process_is_trusted)
                .is_some_and(|bounds| predicate(&current, bounds))
            {
                return Some(current);
            }
            current = unsafe { walker.GetParentElement(&current) }.ok()?;
        }
        None
    }

    fn composer_candidate(
        element: IUIAutomationElement,
        walker: &IUIAutomationTreeWalker,
        root_bounds: AccessibilityBounds,
        process_is_trusted: &dyn Fn(u32) -> bool,
        started: Instant,
    ) -> Option<IUIAutomationElement> {
        candidate_from_ancestor_chain(
            element,
            walker,
            root_bounds,
            process_is_trusted,
            started,
            |candidate, _| {
                let control_type = unsafe { candidate.CurrentControlType() }.ok();
                control_type.is_some_and(|kind| {
                    kind == UIA_EditControlTypeId || kind == UIA_DocumentControlTypeId
                }) && unsafe { candidate.CurrentName() }
                    .ok()
                    .map(|value| value.to_string())
                    .as_deref()
                    .and_then(normalized_conversation_from_composer)
                    .is_some()
            },
        )
    }

    fn matching_visible_header_count(
        automation: &IUIAutomation,
        walker: &IUIAutomationTreeWalker,
        root_bounds: AccessibilityBounds,
        conversation: &str,
        process_is_trusted: &dyn Fn(u32) -> bool,
        started: Instant,
    ) -> Result<usize, String> {
        let names = conversation_name_variants(conversation);
        let width = root_bounds.right - root_bounds.left;
        let mut runtime_ids = Vec::<Vec<i32>>::new();
        for x_percent in [30, 40, 50, 60, 70] {
            for y_offset in [18, 30, 42, 56] {
                if started.elapsed() > SNAPSHOT_TIMEOUT {
                    return Err("Discord accessibility timed out".to_owned());
                }
                let point = POINT {
                    x: root_bounds.left + width * x_percent / 100,
                    y: root_bounds.top + y_offset,
                };
                let Ok(element) = (unsafe { automation.ElementFromPoint(point) }) else {
                    continue;
                };
                let Some(header) = candidate_from_ancestor_chain(
                    element,
                    walker,
                    root_bounds,
                    process_is_trusted,
                    started,
                    |candidate, bounds| {
                        visible_header_bounds(bounds, root_bounds)
                            && unsafe { candidate.CurrentName() }
                                .ok()
                                .map(|value| value.to_string())
                                .is_some_and(|name| names.iter().any(|expected| expected == &name))
                    },
                ) else {
                    continue;
                };
                if let Ok(id) = runtime_id(&header) {
                    if !runtime_ids.contains(&id) {
                        runtime_ids.push(id);
                    }
                }
            }
        }
        Ok(runtime_ids.len())
    }

    fn locate(
        target: crate::native_window_host::NativeDiscordAccessibilityTarget,
        process_is_trusted: &dyn Fn(u32) -> bool,
        scope_binding: &str,
        require_header: bool,
    ) -> Result<LocatedComposer, String> {
        if scope_binding.is_empty()
            || scope_binding.len() > 512
            || scope_binding.chars().any(char::is_control)
        {
            return Err("The OSL friend scope binding is invalid".to_owned());
        }
        let automation: IUIAutomation =
            unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
                .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let root = unsafe { automation.ElementFromHandle(HWND(target.window as _)) }
            .map_err(|_| "The trusted Discord window is unavailable".to_owned())?;
        let root_bounds = element_bounds(&root)
            .ok_or_else(|| "The trusted Discord window bounds are unavailable".to_owned())?;
        let started = Instant::now();
        let walker = unsafe { automation.RawViewWalker() }
            .map_err(|_| "Windows accessibility is unavailable".to_owned())?;
        let _ = unsafe { SetForegroundWindow(target.window as _) };
        std::thread::sleep(Duration::from_millis(20));
        let mut candidates: Vec<(IUIAutomationElement, Vec<i32>)> = Vec::new();
        let mut starts = Vec::new();
        if let Ok(focused) = unsafe { automation.GetFocusedElement() } {
            starts.push(focused);
        }
        let width = root_bounds.right - root_bounds.left;
        let height = root_bounds.bottom - root_bounds.top;
        for x_percent in [45, 60, 75] {
            for bottom_offset in [36, 56, 80, 108] {
                let point = POINT {
                    x: root_bounds.left + width * x_percent / 100,
                    y: root_bounds.bottom - bottom_offset.min(height / 3),
                };
                if let Ok(element) = unsafe { automation.ElementFromPoint(point) } {
                    starts.push(element);
                }
            }
        }
        if starts.len() > 16 {
            return Err("Discord accessibility exceeded its safe limit".to_owned());
        }
        for start in starts {
            if started.elapsed() > SNAPSHOT_TIMEOUT {
                return Err("Discord accessibility timed out".to_owned());
            }
            let Some(element) =
                composer_candidate(start, &walker, root_bounds, process_is_trusted, started)
            else {
                continue;
            };
            let id = runtime_id(&element)?;
            if !candidates.iter().any(|(_, existing)| existing == &id) {
                candidates.push((element, id));
            }
        }
        if candidates.len() != 1 {
            return Err("Discord did not expose one exact visible message composer".to_owned());
        }
        let (element, _) = candidates.pop().unwrap();
        let bounds = trusted_visible_element(&element, root_bounds, process_is_trusted)
            .ok_or_else(|| {
                "Discord did not expose one exact visible message composer".to_owned()
            })?;
        let composer_name = unsafe { element.CurrentName() }
            .ok()
            .map(|value| value.to_string())
            .filter(|value| bounded_identity(Some(value)).is_some())
            .ok_or_else(|| "Discord did not expose the selected conversation".to_owned())?;
        let conversation = normalized_conversation_from_composer(&composer_name)
            .ok_or_else(|| "Discord did not expose the selected conversation".to_owned())?;
        if require_header
            && matching_visible_header_count(
                &automation,
                &walker,
                root_bounds,
                conversation,
                process_is_trusted,
                started,
            )? != 1
        {
            return Err("Discord did not expose a matching visible conversation header".to_owned());
        }
        let automation_id = unsafe { element.CurrentAutomationId() }
            .map(|value| value.to_string())
            .unwrap_or_default();
        let class_name = unsafe { element.CurrentClassName() }
            .map(|value| value.to_string())
            .unwrap_or_default();
        let binding = ComposerBinding {
            generation: target.generation,
            scope_binding_hash: stable_hash("osl-discord-scope", scope_binding),
            conversation_binding_hash: stable_hash("discord-visible-conversation", conversation),
            composer_name_hash: stable_hash("discord-composer-name", &composer_name),
            automation_id_hash: stable_hash("discord-composer-automation-id", &automation_id),
            class_name_hash: stable_hash("discord-composer-class", &class_name),
            runtime_id: runtime_id(&element)?,
            bounds,
        };
        Ok(LocatedComposer { element, binding })
    }

    pub(super) fn calibrate(
        state: &NativeDiscordComposerState,
        target: crate::native_window_host::NativeDiscordAccessibilityTarget,
        process_is_trusted: &dyn Fn(u32) -> bool,
        scope_binding: &str,
    ) -> Result<DiscordComposerCalibrationReceipt, String> {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let located = locate(target, process_is_trusted, scope_binding, true)?;
        if !composer_empty(&located.element) {
            return Err("Clear the visible Discord composer before calibrating".to_owned());
        }
        let receipt = DiscordComposerCalibrationReceipt {
            calibrated: true,
            generation: located.binding.generation,
            conversation_binding_hash: located.binding.conversation_binding_hash.clone(),
        };
        *state
            .binding
            .lock()
            .map_err(|_| "The Discord composer calibration is unavailable".to_owned())? =
            Some(located.binding);
        Ok(receipt)
    }

    fn binding_matches(expected: &ComposerBinding, actual: &ComposerBinding) -> bool {
        expected == actual
    }

    fn foreground_is_exact_discord(target_window: isize) -> bool {
        let target = target_window as windows_sys::Win32::Foundation::HWND;
        let foreground = unsafe { GetForegroundWindow() };
        if target.is_null() || foreground.is_null() {
            return false;
        }
        let target_root = unsafe { GetAncestor(target, GA_ROOT) };
        let foreground_root = unsafe { GetAncestor(foreground, GA_ROOT) };
        !target_root.is_null() && target_root == foreground_root
    }

    fn calibrated_element_is_current(
        element: &IUIAutomationElement,
        expected: &ComposerBinding,
    ) -> bool {
        let name = unsafe { element.CurrentName() }
            .ok()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let automation_id = unsafe { element.CurrentAutomationId() }
            .ok()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let class_name = unsafe { element.CurrentClassName() }
            .ok()
            .map(|value| value.to_string())
            .unwrap_or_default();
        element_bounds(element) == Some(expected.bounds)
            && runtime_id(element).is_ok_and(|runtime_id| runtime_id == expected.runtime_id)
            && stable_hash("discord-composer-name", &name) == expected.composer_name_hash
            && stable_hash("discord-composer-automation-id", &automation_id)
                == expected.automation_id_hash
            && stable_hash("discord-composer-class", &class_name) == expected.class_name_hash
            && unsafe { element.CurrentHasKeyboardFocus() }.is_ok_and(|value| value.as_bool())
    }

    fn may_continue_input(
        target_window: isize,
        element: &IUIAutomationElement,
        expected: &ComposerBinding,
        expected_composer_text: &str,
    ) -> bool {
        foreground_is_exact_discord(target_window)
            && calibrated_element_is_current(element, expected)
            && composer_text(element).as_deref() == Some(expected_composer_text)
    }

    fn keyboard_input(scan: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: 0,
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send_unicode_chunk(units: &[u16]) -> bool {
        let mut inputs = Vec::with_capacity(units.len() * 2);
        for unit in units {
            inputs.push(keyboard_input(*unit, KEYEVENTF_UNICODE));
            inputs.push(keyboard_input(*unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
        }
        (unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<INPUT>() as i32,
            )
        }) == inputs.len() as u32
    }

    fn send_enter(target_window: isize) -> bool {
        if !foreground_is_exact_discord(target_window) {
            return false;
        }
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_RETURN,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VK_RETURN,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];
        (unsafe { SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) }) == 2
    }

    pub(super) fn place(
        state: &NativeDiscordComposerState,
        target: crate::native_window_host::NativeDiscordAccessibilityTarget,
        process_is_trusted: &dyn Fn(u32) -> bool,
        scope_binding: &str,
        mode: DiscordCarrierMode,
        delay_ms: u16,
        carrier: &str,
    ) -> DiscordCarrierReceipt {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let expected = state.binding.lock().ok().and_then(|value| value.clone());
        let Some(expected) = expected else {
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::CalibrationRequired);
        };
        let Ok(before) = locate(target, process_is_trusted, scope_binding, false) else {
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::ComposerUnavailable);
        };
        if !binding_matches(&expected, &before.binding) {
            state.clear();
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::ContextChanged);
        }
        if !composer_empty(&before.element) {
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::ComposerNotEmpty);
        }
        if unsafe { SetForegroundWindow(target.window as _) } == 0
            || unsafe { before.element.SetFocus() }.is_err()
        {
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::PlacementRejected);
        }
        std::thread::sleep(Duration::from_millis(25));
        let Ok(focused) = locate(target, process_is_trusted, scope_binding, false) else {
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::ComposerUnavailable);
        };
        if !binding_matches(&expected, &focused.binding)
            || !unsafe { focused.element.CurrentHasKeyboardFocus() }
                .is_ok_and(|value| value.as_bool())
            || !composer_empty(&focused.element)
            || !foreground_is_exact_discord(target.window)
        {
            state.clear();
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::ContextChanged);
        }
        let placed = match mode {
            DiscordCarrierMode::Atomic => {
                let units = carrier.encode_utf16().collect::<Vec<_>>();
                may_continue_input(target.window, &focused.element, &expected, "")
                    && send_unicode_chunk(&units)
            }
            DiscordCarrierMode::Compatibility => drive_compatibility_carrier(
                carrier,
                |placed_prefix| {
                    may_continue_input(target.window, &focused.element, &expected, placed_prefix)
                },
                |character| {
                    let mut encoded = [0u16; 2];
                    send_unicode_chunk(character.encode_utf16(&mut encoded))
                },
                || {
                    std::thread::sleep(Duration::from_millis(u64::from(delay_ms)));
                },
            ),
        };
        if !placed {
            return carrier_failure(mode, delay_ms, DiscordCarrierStatus::PlacementRejected);
        }
        let Ok(final_check) = locate(target, process_is_trusted, scope_binding, false) else {
            return DiscordCarrierReceipt {
                placed: true,
                enter_sent: false,
                status: DiscordCarrierStatus::ContextChanged,
                mode,
                compatibility_delay_ms: delay_ms,
            };
        };
        if !binding_matches(&expected, &final_check.binding)
            || !unsafe { final_check.element.CurrentHasKeyboardFocus() }
                .is_ok_and(|value| value.as_bool())
            || composer_text(&final_check.element).as_deref() != Some(carrier)
            || !foreground_is_exact_discord(target.window)
        {
            state.clear();
            return DiscordCarrierReceipt {
                placed: true,
                enter_sent: false,
                status: DiscordCarrierStatus::ContextChanged,
                mode,
                compatibility_delay_ms: delay_ms,
            };
        }
        if !may_continue_input(target.window, &final_check.element, &expected, carrier)
            || !send_enter(target.window)
        {
            return DiscordCarrierReceipt {
                placed: true,
                enter_sent: false,
                status: DiscordCarrierStatus::EnterRejected,
                mode,
                compatibility_delay_ms: delay_ms,
            };
        }
        state.clear();
        DiscordCarrierReceipt {
            placed: true,
            enter_sent: true,
            status: DiscordCarrierStatus::Sent,
            mode,
            compatibility_delay_ms: delay_ms,
        }
    }

    pub(super) fn snapshot(
        target: crate::native_window_host::NativeDiscordAccessibilityTarget,
        process_is_trusted: &dyn Fn(u32) -> bool,
    ) -> NativeDiscordAccessibilitySnapshot {
        let started = Instant::now();
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let automation: IUIAutomation =
            match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
                Ok(value) => value,
                Err(_) => {
                    return NativeDiscordAccessibilitySnapshot::unavailable(
                        target.generation,
                        DiscordSnapshotReason::AccessibilityUnavailable,
                    )
                }
            };
        let root = match unsafe { automation.ElementFromHandle(HWND(target.window as _)) } {
            Ok(value) => value,
            Err(_) => {
                return NativeDiscordAccessibilitySnapshot::unavailable(
                    target.generation,
                    DiscordSnapshotReason::AccessibilityUnavailable,
                )
            }
        };
        // Discord's current accessibility semantics do not expose stable,
        // provider-authenticated account/conversation/member identifiers.
        // Electron's descendant enumeration can block inside the provider, so
        // the snapshot path never starts a tree walk merely to rediscover that
        // these identities remain unproven. Composer placement uses separate,
        // bounded focus/point probes and still fails closed on ambiguity.
        let _ = (root, started, target.process_id, process_is_trusted);
        assemble_snapshot(target.generation, ProvenEvidence::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> AccessibilityBounds {
        AccessibilityBounds {
            left: 10,
            top: 20,
            right: 300,
            bottom: 80,
        }
    }

    #[test]
    fn ciphertext_shape_is_strict_and_rejects_plaintext_and_prose_tokens() {
        assert!(strict_ciphertext_candidate(&format!(
            "DPC0::{}",
            "A".repeat(64)
        )));
        assert!(!strict_ciphertext_candidate("ordinary private message"));
        assert!(!strict_ciphertext_candidate(
            "This compact sentence is a marker-free prose token."
        ));
        assert!(!strict_ciphertext_candidate("DPC0::short"));
        assert!(!strict_ciphertext_candidate(&format!(
            "DPC0::{}!",
            "A".repeat(64)
        )));
        assert!(!strict_ciphertext_candidate(&format!(
            "DPC0::{}",
            "A".repeat(MAX_ACCESSIBLE_TEXT_BYTES)
        )));
    }

    #[test]
    fn complete_proven_evidence_returns_hashes_geometry_and_no_plaintext() {
        let ciphertext = format!("DPC0::{}", "A".repeat(64));
        let snapshot = assemble_snapshot(
            9,
            ProvenEvidence {
                self_account_identity: Some("discord-user:100".to_owned()),
                conversation_identity: Some("discord-dm:200".to_owned()),
                participant_identities: vec!["discord-user:200".to_owned()],
                composer: Some(ProvenComposer {
                    locator: "runtime:composer:1".to_owned(),
                    bounds: bounds(),
                    has_keyboard_focus: true,
                }),
                carriers: vec![ProvenCarrier {
                    locator: "runtime:message:7".to_owned(),
                    ciphertext: ciphertext.clone(),
                    author_identity: "discord-user:200".to_owned(),
                    bounds: bounds(),
                }],
            },
        );
        assert!(snapshot.available);
        assert_eq!(snapshot.generation, 9);
        assert_eq!(snapshot.reason, DiscordSnapshotReason::Ready);
        assert_eq!(snapshot.visible_carriers.len(), 1);
        assert_ne!(snapshot.visible_carriers[0].ciphertext_hash, ciphertext);
        assert_eq!(snapshot.composer.unwrap().bounds, bounds());
    }

    #[test]
    fn missing_or_ambiguous_semantics_fail_closed() {
        let empty = assemble_snapshot(3, ProvenEvidence::default());
        assert!(!empty.available);
        assert_eq!(empty.reason, DiscordSnapshotReason::SelfAccountUnproven);

        let missing_composer = assemble_snapshot(
            4,
            ProvenEvidence {
                self_account_identity: Some("self".to_owned()),
                conversation_identity: Some("conversation".to_owned()),
                participant_identities: vec!["peer".to_owned()],
                ..ProvenEvidence::default()
            },
        );
        assert!(!missing_composer.available);
        assert_eq!(
            missing_composer.reason,
            DiscordSnapshotReason::ComposerUnproven
        );
    }

    #[test]
    fn carrier_without_proven_author_never_surfaces() {
        let snapshot = assemble_snapshot(
            5,
            ProvenEvidence {
                self_account_identity: Some("self".to_owned()),
                conversation_identity: Some("conversation".to_owned()),
                participant_identities: vec!["peer".to_owned()],
                composer: Some(ProvenComposer {
                    locator: "composer".to_owned(),
                    bounds: bounds(),
                    has_keyboard_focus: false,
                }),
                carriers: vec![ProvenCarrier {
                    locator: "message".to_owned(),
                    ciphertext: format!("DPC0::{}", "A".repeat(64)),
                    author_identity: String::new(),
                    bounds: bounds(),
                }],
            },
        );
        assert!(!snapshot.available);
        assert_eq!(
            snapshot.reason,
            DiscordSnapshotReason::CarrierAuthorUnproven
        );
        assert!(snapshot.visible_carriers.is_empty());
    }

    #[test]
    fn compatibility_speed_is_coarse_clamped_and_deterministic() {
        assert_eq!(compatibility_delay_ms(0), 167);
        assert_eq!(compatibility_delay_ms(1), 500);
        assert_eq!(compatibility_delay_ms(2), 500);
        assert_eq!(compatibility_delay_ms(4), 250);
        assert_eq!(compatibility_delay_ms(6), 167);
        assert_eq!(compatibility_delay_ms(8), 125);
        assert_eq!(compatibility_delay_ms(10), 100);
        assert_eq!(compatibility_delay_ms(12), 83);
        assert_eq!(compatibility_delay_ms(14), 71);
        assert_eq!(compatibility_delay_ms(16), 63);
        assert_eq!(compatibility_delay_ms(17), 63);
        assert_eq!(compatibility_delay_ms(u16::MAX), 63);
    }

    #[test]
    fn cover_source_is_bounded_to_safe_local_prose() {
        assert!(valid_cover("Sounds good to me."));
        assert!(valid_cover("🔒 OSL private message"));
        assert!(!valid_cover(""));
        assert!(!valid_cover("line one\nline two"));
        assert!(!valid_cover(&"x".repeat(MAX_COVER_BYTES + 1)));
    }

    #[test]
    fn calibration_clear_drops_the_complete_message_binding() {
        let state = NativeDiscordComposerState::default();
        assert!(!state.marker_available());
        *state.binding.lock().unwrap() = Some(ComposerBinding {
            generation: 7,
            scope_binding_hash: "scope".to_owned(),
            conversation_binding_hash: "conversation".to_owned(),
            composer_name_hash: "composer".to_owned(),
            automation_id_hash: "automation".to_owned(),
            class_name_hash: "class".to_owned(),
            runtime_id: vec![1, 2, 3],
            bounds: bounds(),
        });
        assert!(state.marker_available());
        state.clear();
        assert!(!state.marker_available());
        assert!(state.binding.lock().unwrap().is_none());
    }

    #[test]
    fn carrier_receipts_never_claim_delivery_when_enter_was_not_sent() {
        let receipt = carrier_failure(
            DiscordCarrierMode::Atomic,
            compatibility_delay_ms(10),
            DiscordCarrierStatus::ContextChanged,
        );
        assert!(!receipt.placed);
        assert!(!receipt.enter_sent);
        assert_eq!(receipt.status, DiscordCarrierStatus::ContextChanged);
    }

    #[test]
    fn compatibility_driver_revalidates_before_every_character() {
        let mut checkpoints = Vec::new();
        let mut sent = String::new();
        assert!(drive_compatibility_carrier(
            "abc",
            |prefix| {
                checkpoints.push(prefix.to_owned());
                true
            },
            |character| {
                sent.push(character);
                true
            },
            || {},
        ));
        assert_eq!(checkpoints, ["", "a", "ab"]);
        assert_eq!(sent, "abc");
    }

    #[test]
    fn compatibility_driver_aborts_immediately_when_focus_checkpoint_fails() {
        let mut checks = 0;
        let mut sent = String::new();
        assert!(!drive_compatibility_carrier(
            "abcd",
            |_| {
                checks += 1;
                checks < 3
            },
            |character| {
                sent.push(character);
                true
            },
            || {},
        ));
        assert_eq!(checks, 3);
        assert_eq!(sent, "ab");
    }

    #[test]
    fn composer_conversation_normalization_and_header_variants_are_exact() {
        assert_eq!(
            normalized_conversation_from_composer("Message @alice"),
            Some("alice")
        );
        assert_eq!(
            normalized_conversation_from_composer("message #private"),
            Some("private")
        );
        assert_eq!(normalized_conversation_from_composer("Search"), None);
        assert_eq!(
            conversation_name_variants("alice"),
            ["alice", "@alice", "#alice"]
        );
        assert!(conversation_name_variants("\n").is_empty());
    }

    #[test]
    fn matching_header_must_be_visible_inside_the_trusted_top_region() {
        let root = AccessibilityBounds {
            left: 100,
            top: 100,
            right: 1300,
            bottom: 900,
        };
        assert!(visible_header_bounds(
            AccessibilityBounds {
                left: 400,
                top: 130,
                right: 700,
                bottom: 180,
            },
            root
        ));
        assert!(!visible_header_bounds(
            AccessibilityBounds {
                left: 400,
                top: 700,
                right: 700,
                bottom: 760,
            },
            root
        ));
        assert!(!visible_header_bounds(
            AccessibilityBounds {
                left: 0,
                top: 130,
                right: 700,
                bottom: 180,
            },
            root
        ));
    }
}
