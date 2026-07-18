//! Fail-closed contracts for OSL-owned windows drawn over an official app.
//!
//! This module intentionally does not inspect or modify another application's
//! DOM, accessibility tree, process memory, credentials, or network traffic.
//! Geometry alone is never treated as proof of a conversation. A service
//! adapter must bind an exact account, conversation, native window generation,
//! process fingerprint, and non-secret message field before either overlay can
//! be shown. Until an adapter can produce that evidence, the correct state is
//! hidden.

use std::collections::VecDeque;
use std::time::Duration;

use zeroize::Zeroizing;

const MAX_RECT_EDGE: u32 = 16_384;
const MAX_VISIBLE_MESSAGES_HARD: usize = 2_048;
const MAX_VISIBLE_BYTES_HARD: usize = 32 * 1024 * 1024;
const MAX_PLAINTEXT_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_VISIBLE_TTL: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRect {
    fn valid(self) -> bool {
        self.width > 0
            && self.height > 0
            && self.width <= MAX_RECT_EDGE
            && self.height <= MAX_RECT_EDGE
            && self.x.checked_add_unsigned(self.width).is_some()
            && self.y.checked_add_unsigned(self.height).is_some()
    }

    fn contains(self, child: Self) -> bool {
        if !self.valid() || !child.valid() {
            return false;
        }
        let Some(right) = self.x.checked_add_unsigned(self.width) else {
            return false;
        };
        let Some(bottom) = self.y.checked_add_unsigned(self.height) else {
            return false;
        };
        let Some(child_right) = child.x.checked_add_unsigned(child.width) else {
            return false;
        };
        let Some(child_bottom) = child.y.checked_add_unsigned(child.height) else {
            return false;
        };
        child.x >= self.x && child.y >= self.y && child_right <= right && child_bottom <= bottom
    }

    pub fn contains_point(self, x: i32, y: i32) -> bool {
        self.valid()
            && x >= self.x
            && y >= self.y
            && self
                .x
                .checked_add_unsigned(self.width)
                .is_some_and(|right| x < right)
            && self
                .y
                .checked_add_unsigned(self.height)
                .is_some_and(|bottom| y < bottom)
    }
}

/// An opaque identity emitted by a trusted, service-specific adapter.
/// None of these strings may contain message text, usernames, window titles,
/// or service credentials.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExternalContextBinding {
    pub service_id: String,
    pub account_id: String,
    pub context_id: String,
    pub native_window_id: u64,
    pub native_window_generation: u64,
    pub process_fingerprint_sha256: [u8; 32],
}

impl ExternalContextBinding {
    fn valid(&self) -> bool {
        fn opaque(value: &str, max: usize) -> bool {
            !value.is_empty()
                && value.len() <= max
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }
        opaque(&self.service_id, 32)
            && opaque(&self.account_id, 64)
            && opaque(&self.context_id, 160)
            && self.native_window_id != 0
            && self.native_window_generation != 0
            && self
                .process_fingerprint_sha256
                .iter()
                .any(|byte| *byte != 0)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VerifiedFieldKind {
    MessageComposer,
    MessageBody,
    PasswordOrLogin,
    Unknown,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ComposerCalibration {
    pub binding: ExternalContextBinding,
    pub target_window: ScreenRect,
    pub composer: ScreenRect,
    pub field_kind: VerifiedFieldKind,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WindowObservation {
    pub binding: ExternalContextBinding,
    pub target_window: ScreenRect,
    pub foreground: bool,
    pub minimized: bool,
    pub geometry_certain: bool,
    pub focused_field: VerifiedFieldKind,
    pub focused_field_bounds: Option<ScreenRect>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OverlayHiddenReason {
    NotCalibrated,
    InvalidCalibration,
    ContextChanged,
    WindowMovedOrResized,
    WindowNotForeground,
    WindowUnavailable,
    GeometryUncertain,
    ComposerNotFocused,
    PasswordOrLoginField,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ComposerOverlayDecision {
    Show(ScreenRect),
    Hide(OverlayHiddenReason),
}

#[derive(Default)]
pub struct ComposerOverlayGuard {
    calibration: Option<ComposerCalibration>,
}

impl ComposerOverlayGuard {
    pub fn calibrate(
        &mut self,
        calibration: ComposerCalibration,
    ) -> Result<(), OverlayHiddenReason> {
        if !calibration.binding.valid()
            || calibration.field_kind != VerifiedFieldKind::MessageComposer
            || calibration.composer.width < 240
            || calibration.composer.height < 36
            || !calibration.target_window.contains(calibration.composer)
        {
            self.calibration = None;
            return Err(OverlayHiddenReason::InvalidCalibration);
        }
        self.calibration = Some(calibration);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.calibration = None;
    }

    /// The overlay must be hidden immediately for every uncertain observation.
    /// Repositioning requires an explicit new calibration; it is never guessed.
    pub fn observe(&self, observed: &WindowObservation) -> ComposerOverlayDecision {
        let Some(calibration) = self.calibration.as_ref() else {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::NotCalibrated);
        };
        if calibration.binding != observed.binding {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::ContextChanged);
        }
        if observed.minimized {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::WindowUnavailable);
        }
        if !observed.foreground {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::WindowNotForeground);
        }
        if !observed.geometry_certain {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::GeometryUncertain);
        }
        if calibration.target_window != observed.target_window {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::WindowMovedOrResized);
        }
        if observed.focused_field == VerifiedFieldKind::PasswordOrLogin {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::PasswordOrLoginField);
        }
        if observed.focused_field != VerifiedFieldKind::MessageComposer
            || observed.focused_field_bounds != Some(calibration.composer)
        {
            return ComposerOverlayDecision::Hide(OverlayHiddenReason::ComposerNotFocused);
        }
        ComposerOverlayDecision::Show(calibration.composer)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedCarrierBinding {
    pub context: ExternalContextBinding,
    pub native_message_id_sha256: [u8; 32],
    pub ciphertext_sha256: [u8; 32],
    pub pixel_bounds: ScreenRect,
}

impl EncryptedCarrierBinding {
    fn valid(&self, window: ScreenRect) -> bool {
        self.context.valid()
            && self.native_message_id_sha256.iter().any(|byte| *byte != 0)
            && self.ciphertext_sha256.iter().any(|byte| *byte != 0)
            && window.contains(self.pixel_bounds)
            && self.pixel_bounds.height >= 12
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DecryptedHitTarget {
    Plaintext([u8; 32]),
    PassThrough,
}

/// Pure geometry plan for the receive overlay. Plaintext is deliberately kept
/// in `VisiblePlaintextCache`, not in this clonable/debuggable plan.
#[derive(Default)]
pub struct DecryptionOverlayGuard {
    context: Option<ExternalContextBinding>,
    target_window: Option<ScreenRect>,
    visible: Vec<EncryptedCarrierBinding>,
}

impl DecryptionOverlayGuard {
    pub fn bind_context(
        &mut self,
        context: ExternalContextBinding,
        target_window: ScreenRect,
    ) -> Result<(), OverlayHiddenReason> {
        if !context.valid() || !target_window.valid() {
            self.clear();
            return Err(OverlayHiddenReason::InvalidCalibration);
        }
        self.context = Some(context);
        self.target_window = Some(target_window);
        self.visible.clear();
        Ok(())
    }

    pub fn replace_visible(
        &mut self,
        observed: &WindowObservation,
        carriers: Vec<EncryptedCarrierBinding>,
    ) -> Result<(), OverlayHiddenReason> {
        let Some(context) = self.context.as_ref() else {
            return Err(OverlayHiddenReason::NotCalibrated);
        };
        let Some(window) = self.target_window else {
            return Err(OverlayHiddenReason::NotCalibrated);
        };
        let safe = context == &observed.binding
            && observed.foreground
            && !observed.minimized
            && observed.geometry_certain
            && observed.target_window == window
            && observed.focused_field != VerifiedFieldKind::PasswordOrLogin;
        if !safe
            || carriers
                .iter()
                .any(|carrier| &carrier.context != context || !carrier.valid(window))
        {
            self.visible.clear();
            return Err(
                if observed.focused_field == VerifiedFieldKind::PasswordOrLogin {
                    OverlayHiddenReason::PasswordOrLoginField
                } else if context != &observed.binding {
                    OverlayHiddenReason::ContextChanged
                } else {
                    OverlayHiddenReason::GeometryUncertain
                },
            );
        }
        self.visible = carriers;
        Ok(())
    }

    pub fn hit_test(&self, x: i32, y: i32) -> DecryptedHitTarget {
        self.visible
            .iter()
            .find(|carrier| carrier.pixel_bounds.contains_point(x, y))
            .map_or(DecryptedHitTarget::PassThrough, |carrier| {
                DecryptedHitTarget::Plaintext(carrier.native_message_id_sha256)
            })
    }

    pub fn visible_bindings(&self) -> &[EncryptedCarrierBinding] {
        &self.visible
    }

    pub fn clear(&mut self) {
        self.visible.clear();
        self.context = None;
        self.target_window = None;
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OverlayCacheTier {
    Free,
    Pro,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct VisibleCacheLimits {
    pub max_messages: usize,
    pub max_bytes: usize,
    pub ttl: Duration,
}

impl VisibleCacheLimits {
    pub fn for_tier(tier: OverlayCacheTier) -> Self {
        match tier {
            OverlayCacheTier::Free => Self {
                max_messages: 250,
                max_bytes: 4 * 1024 * 1024,
                ttl: Duration::from_secs(30 * 60),
            },
            OverlayCacheTier::Pro => Self {
                max_messages: 1_000,
                max_bytes: 16 * 1024 * 1024,
                ttl: Duration::from_secs(2 * 60 * 60),
            },
        }
    }

    pub fn bounded(self) -> Self {
        Self {
            max_messages: self.max_messages.clamp(1, MAX_VISIBLE_MESSAGES_HARD),
            max_bytes: self.max_bytes.clamp(1, MAX_VISIBLE_BYTES_HARD),
            ttl: self.ttl.min(MAX_VISIBLE_TTL),
        }
    }
}

struct PlaintextEntry {
    message_id_sha256: [u8; 32],
    plaintext: Zeroizing<String>,
    touched_at_ms: u64,
}

/// Bounded RAM cache for the visible viewport plus a small adjacent buffer.
/// Entries zeroize when evicted, locked, or dropped. It never serializes.
pub struct VisiblePlaintextCache {
    limits: VisibleCacheLimits,
    entries: VecDeque<PlaintextEntry>,
    bytes: usize,
}

impl VisiblePlaintextCache {
    pub fn new(limits: VisibleCacheLimits) -> Self {
        Self {
            limits: limits.bounded(),
            entries: VecDeque::new(),
            bytes: 0,
        }
    }

    pub fn insert(&mut self, message_id_sha256: [u8; 32], plaintext: String, now_ms: u64) -> bool {
        let bytes = plaintext.len();
        if message_id_sha256.iter().all(|byte| *byte == 0)
            || bytes == 0
            || bytes > MAX_PLAINTEXT_MESSAGE_BYTES
            || bytes > self.limits.max_bytes
        {
            return false;
        }
        self.evict_expired(now_ms);
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.message_id_sha256 == message_id_sha256)
        {
            if let Some(old) = self.entries.remove(index) {
                self.bytes = self.bytes.saturating_sub(old.plaintext.len());
            }
        }
        while self.entries.len() >= self.limits.max_messages
            || self.bytes.saturating_add(bytes) > self.limits.max_bytes
        {
            let Some(old) = self.entries.pop_front() else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(old.plaintext.len());
        }
        self.bytes = self.bytes.saturating_add(bytes);
        self.entries.push_back(PlaintextEntry {
            message_id_sha256,
            plaintext: Zeroizing::new(plaintext),
            touched_at_ms: now_ms,
        });
        true
    }

    pub fn get(&mut self, message_id_sha256: [u8; 32], now_ms: u64) -> Option<&str> {
        self.evict_expired(now_ms);
        let index = self
            .entries
            .iter()
            .position(|entry| entry.message_id_sha256 == message_id_sha256)?;
        let mut entry = self.entries.remove(index)?;
        entry.touched_at_ms = now_ms;
        self.entries.push_back(entry);
        self.entries.back().map(|entry| entry.plaintext.as_str())
    }

    pub fn retain_visible(&mut self, visible_and_adjacent: &[[u8; 32]], now_ms: u64) {
        self.evict_expired(now_ms);
        let mut retained = VecDeque::with_capacity(self.entries.len());
        while let Some(entry) = self.entries.pop_front() {
            if visible_and_adjacent.contains(&entry.message_id_sha256) {
                retained.push_back(entry);
            } else {
                self.bytes = self.bytes.saturating_sub(entry.plaintext.len());
            }
        }
        self.entries = retained;
    }

    pub fn lock(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    fn evict_expired(&mut self, now_ms: u64) {
        let ttl_ms = self.limits.ttl.as_millis().min(u128::from(u64::MAX)) as u64;
        while self
            .entries
            .front()
            .is_some_and(|entry| now_ms.saturating_sub(entry.touched_at_ms) >= ttl_ms)
        {
            if let Some(old) = self.entries.pop_front() {
                self.bytes = self.bytes.saturating_sub(old.plaintext.len());
            }
        }
    }
}

impl Drop for VisiblePlaintextCache {
    fn drop(&mut self) {
        self.lock();
    }
}

/// Format contract for a possible future SSD cache. It is not wired to storage
/// today. Before that can change, the caller must authenticate and decrypt each
/// record with an OSL-identity-scoped local key. Plaintext or a remote cache is
/// not an allowed fallback; unavailable secure storage means cache miss.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EncryptedLocalOverlayCacheRecord {
    pub version: u8,
    pub identity_key_id_sha256: [u8; 32],
    pub nonce: [u8; 24],
    pub authenticated_ciphertext: Vec<u8>,
    pub expires_at_ms: u64,
}

/// Hard bounds for a future encrypted-at-rest SSD cache. This cache is an
/// optional performance aid, not message history: oldest records are evicted
/// first and the user may disable it or choose a smaller limit. It must never
/// be mirrored to OSL infrastructure.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct EncryptedLocalCacheLimits {
    pub max_records: usize,
    pub max_bytes: usize,
    pub ttl: Duration,
}

impl EncryptedLocalCacheLimits {
    pub fn for_tier(tier: OverlayCacheTier) -> Self {
        match tier {
            OverlayCacheTier::Free => Self {
                max_records: 25_000,
                max_bytes: 50 * 1024 * 1024,
                ttl: Duration::from_secs(30 * 24 * 60 * 60),
            },
            OverlayCacheTier::Pro => Self {
                max_records: 250_000,
                max_bytes: 512 * 1024 * 1024,
                ttl: Duration::from_secs(90 * 24 * 60 * 60),
            },
        }
    }
}

impl EncryptedLocalOverlayCacheRecord {
    pub fn structurally_valid(&self, now_ms: u64) -> bool {
        self.version == 1
            && self.identity_key_id_sha256.iter().any(|byte| *byte != 0)
            && self.nonce.iter().any(|byte| *byte != 0)
            && self.authenticated_ciphertext.len() >= 16
            && self.authenticated_ciphertext.len() <= MAX_PLAINTEXT_MESSAGE_BYTES + 16
            && self.expires_at_ms > now_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(generation: u64) -> ExternalContextBinding {
        ExternalContextBinding {
            service_id: "discord".to_owned(),
            account_id: "test-account-1".to_owned(),
            context_id: "dm-42".to_owned(),
            native_window_id: 41,
            native_window_generation: generation,
            process_fingerprint_sha256: [7; 32],
        }
    }

    fn observation() -> WindowObservation {
        WindowObservation {
            binding: binding(3),
            target_window: ScreenRect {
                x: 100,
                y: 50,
                width: 1000,
                height: 700,
            },
            foreground: true,
            minimized: false,
            geometry_certain: true,
            focused_field: VerifiedFieldKind::MessageComposer,
            focused_field_bounds: Some(ScreenRect {
                x: 240,
                y: 670,
                width: 720,
                height: 56,
            }),
        }
    }

    #[test]
    fn composer_requires_exact_context_focus_and_unchanged_geometry() {
        let mut guard = ComposerOverlayGuard::default();
        let observed = observation();
        guard
            .calibrate(ComposerCalibration {
                binding: observed.binding.clone(),
                target_window: observed.target_window,
                composer: observed.focused_field_bounds.unwrap(),
                field_kind: VerifiedFieldKind::MessageComposer,
            })
            .unwrap();
        assert_eq!(
            guard.observe(&observed),
            ComposerOverlayDecision::Show(observed.focused_field_bounds.unwrap())
        );

        let mut moved = observed.clone();
        moved.target_window.x += 1;
        assert_eq!(
            guard.observe(&moved),
            ComposerOverlayDecision::Hide(OverlayHiddenReason::WindowMovedOrResized)
        );
        let mut other_chat = observed.clone();
        other_chat.binding.context_id = "dm-43".to_owned();
        assert_eq!(
            guard.observe(&other_chat),
            ComposerOverlayDecision::Hide(OverlayHiddenReason::ContextChanged)
        );
    }

    #[test]
    fn composer_never_covers_login_or_unverified_fields() {
        let observed = observation();
        let mut guard = ComposerOverlayGuard::default();
        let invalid = guard.calibrate(ComposerCalibration {
            binding: observed.binding.clone(),
            target_window: observed.target_window,
            composer: observed.focused_field_bounds.unwrap(),
            field_kind: VerifiedFieldKind::PasswordOrLogin,
        });
        assert_eq!(invalid, Err(OverlayHiddenReason::InvalidCalibration));

        guard
            .calibrate(ComposerCalibration {
                binding: observed.binding.clone(),
                target_window: observed.target_window,
                composer: observed.focused_field_bounds.unwrap(),
                field_kind: VerifiedFieldKind::MessageComposer,
            })
            .unwrap();
        let mut password = observed;
        password.focused_field = VerifiedFieldKind::PasswordOrLogin;
        assert_eq!(
            guard.observe(&password),
            ComposerOverlayDecision::Hide(OverlayHiddenReason::PasswordOrLoginField)
        );
    }

    #[test]
    fn receive_overlay_passes_through_everywhere_except_verified_plaintext() {
        let observed = observation();
        let mut guard = DecryptionOverlayGuard::default();
        guard
            .bind_context(observed.binding.clone(), observed.target_window)
            .unwrap();
        let carrier = EncryptedCarrierBinding {
            context: observed.binding.clone(),
            native_message_id_sha256: [2; 32],
            ciphertext_sha256: [3; 32],
            pixel_bounds: ScreenRect {
                x: 300,
                y: 240,
                width: 260,
                height: 54,
            },
        };
        guard.replace_visible(&observed, vec![carrier]).unwrap();
        assert_eq!(
            guard.hit_test(330, 260),
            DecryptedHitTarget::Plaintext([2; 32])
        );
        assert_eq!(guard.hit_test(200, 200), DecryptedHitTarget::PassThrough);
    }

    #[test]
    fn receive_overlay_hides_all_regions_on_focus_or_context_uncertainty() {
        let observed = observation();
        let mut guard = DecryptionOverlayGuard::default();
        guard
            .bind_context(observed.binding.clone(), observed.target_window)
            .unwrap();
        let carrier = EncryptedCarrierBinding {
            context: observed.binding.clone(),
            native_message_id_sha256: [2; 32],
            ciphertext_sha256: [3; 32],
            pixel_bounds: ScreenRect {
                x: 300,
                y: 240,
                width: 260,
                height: 54,
            },
        };
        guard.replace_visible(&observed, vec![carrier]).unwrap();
        let mut uncertain = observed;
        uncertain.geometry_certain = false;
        assert!(guard.replace_visible(&uncertain, vec![]).is_err());
        assert!(guard.visible_bindings().is_empty());
    }

    #[test]
    fn visible_plaintext_cache_is_bounded_expires_and_retains_only_viewport_buffer() {
        let mut cache = VisiblePlaintextCache::new(VisibleCacheLimits {
            max_messages: 2,
            max_bytes: 8,
            ttl: Duration::from_millis(20),
        });
        assert!(cache.insert([1; 32], "one".to_owned(), 0));
        assert!(cache.insert([2; 32], "two".to_owned(), 1));
        assert!(cache.insert([3; 32], "tri".to_owned(), 2));
        assert_eq!(cache.len(), 2);
        assert!(cache.get([1; 32], 2).is_none());
        cache.retain_visible(&[[3; 32]], 3);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get([3; 32], 3), Some("tri"));
        assert!(cache.get([3; 32], 23).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_tiers_are_small_and_custom_limits_cannot_exceed_hard_caps() {
        let free = VisibleCacheLimits::for_tier(OverlayCacheTier::Free);
        let pro = VisibleCacheLimits::for_tier(OverlayCacheTier::Pro);
        assert_eq!(free.max_messages, 250);
        assert_eq!(free.max_bytes, 4 * 1024 * 1024);
        assert!(free.max_messages < pro.max_messages);
        assert!(free.max_bytes < pro.max_bytes);
        let bounded = VisibleCacheLimits {
            max_messages: usize::MAX,
            max_bytes: usize::MAX,
            ttl: Duration::from_secs(u64::MAX),
        }
        .bounded();
        assert_eq!(bounded.max_messages, MAX_VISIBLE_MESSAGES_HARD);
        assert_eq!(bounded.max_bytes, MAX_VISIBLE_BYTES_HARD);
        assert_eq!(bounded.ttl, MAX_VISIBLE_TTL);

        let free_ssd = EncryptedLocalCacheLimits::for_tier(OverlayCacheTier::Free);
        let pro_ssd = EncryptedLocalCacheLimits::for_tier(OverlayCacheTier::Pro);
        assert_eq!(free_ssd.max_bytes, 50 * 1024 * 1024);
        assert!(free_ssd.max_records < pro_ssd.max_records);
        assert!(free_ssd.max_bytes < pro_ssd.max_bytes);
        assert!(free_ssd.ttl < pro_ssd.ttl);
    }

    #[test]
    fn encrypted_ssd_cache_rejects_plain_or_expired_shapes() {
        let valid = EncryptedLocalOverlayCacheRecord {
            version: 1,
            identity_key_id_sha256: [1; 32],
            nonce: [2; 24],
            authenticated_ciphertext: vec![4; 32],
            expires_at_ms: 100,
        };
        assert!(valid.structurally_valid(99));
        assert!(!valid.structurally_valid(100));
        let mut plain = valid;
        plain.authenticated_ciphertext = b"hello".to_vec();
        assert!(!plain.structurally_valid(0));
    }
}
