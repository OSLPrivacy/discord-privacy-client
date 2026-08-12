//! TASK 6860 — the story privacy settings the app hands to the renderer.
//!
//! The renderer never decides whether a shield is possible. It receives
//! [`StoryPrivacySurfaceDto`], which is built from the platform's own answer
//! (`story_privacy::shield_state`) and from the same sentences the engine
//! enforces, so a surface cannot claim capture protection that the OS underneath
//! it cannot deliver.

use serde::Serialize;
use story_privacy::{
    shield_state, shield_state_for, StoryAudience, StoryLifetime, AUDIENCE_SOURCE_DEFAULT,
    AUDIENCE_SOURCE_OVERRIDE, SHIELD_DISCLOSURE, SHIELD_PRIMITIVE, SHIELD_UNAVAILABLE_COPY,
    VIEW_RECEIPT_OFF_VIEWER_COPY, VIEW_RECEIPT_VIEWER_COPY,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StoryOptionDto {
    pub id: &'static str,
    pub label: &'static str,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StoryLifetimeOptionDto {
    pub id: &'static str,
    pub label: &'static str,
    pub seconds: i64,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StoryShieldDto {
    pub available: bool,
    pub primitive: Option<&'static str>,
    pub control_enabled: bool,
    pub on: bool,
    pub claims_protection: bool,
    /// Present only alongside a claim.
    pub disclosure: Option<&'static str>,
    /// Present only when there is no primitive to claim.
    pub unavailable_copy: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StoryPrivacySurfaceDto {
    pub audiences: Vec<StoryOptionDto>,
    pub lifetimes: Vec<StoryLifetimeOptionDto>,
    pub shield: StoryShieldDto,
    pub view_receipts_on: bool,
    pub viewer_pre_open_copy: &'static str,
}

fn audience_label(audience: StoryAudience) -> &'static str {
    match audience {
        StoryAudience::Everyone => "EVERYONE",
        StoryAudience::Friends => "FRIENDS",
        StoryAudience::Verified => "VERIFIED",
    }
}

/// Build the surface for this machine, asking the platform about the shield.
pub fn story_privacy_surface(
    default_audience: StoryAudience,
    default_lifetime: StoryLifetime,
    shield_requested_on: bool,
    view_receipts_on: bool,
) -> StoryPrivacySurfaceDto {
    surface_with_shield(
        shield_state(shield_requested_on),
        default_audience,
        default_lifetime,
        view_receipts_on,
    )
}

/// The same surface with capture-protection availability supplied, so both
/// branches stay testable wherever the tests happen to run.
pub fn story_privacy_surface_for(
    shield_supported: bool,
    default_audience: StoryAudience,
    default_lifetime: StoryLifetime,
    shield_requested_on: bool,
    view_receipts_on: bool,
) -> StoryPrivacySurfaceDto {
    surface_with_shield(
        shield_state_for(shield_supported, shield_requested_on),
        default_audience,
        default_lifetime,
        view_receipts_on,
    )
}

fn surface_with_shield(
    shield: story_privacy::ShieldState,
    default_audience: StoryAudience,
    default_lifetime: StoryLifetime,
    view_receipts_on: bool,
) -> StoryPrivacySurfaceDto {
    StoryPrivacySurfaceDto {
        audiences: StoryAudience::ALL
            .iter()
            .map(|audience| StoryOptionDto {
                id: audience.stable_id(),
                label: audience_label(*audience),
                selected: *audience == default_audience,
            })
            .collect(),
        lifetimes: StoryLifetime::ALL
            .iter()
            .map(|lifetime| StoryLifetimeOptionDto {
                id: lifetime.stable_id(),
                label: lifetime.label(),
                seconds: lifetime.seconds(),
                selected: *lifetime == default_lifetime,
            })
            .collect(),
        shield: StoryShieldDto {
            available: shield.supported,
            primitive: shield.primitive,
            control_enabled: shield.control_enabled,
            on: shield.setting_on,
            claims_protection: shield.claims_protection,
            disclosure: shield.disclosure,
            unavailable_copy: shield.unavailable_copy,
        },
        view_receipts_on,
        viewer_pre_open_copy: if view_receipts_on {
            VIEW_RECEIPT_VIEWER_COPY
        } else {
            VIEW_RECEIPT_OFF_VIEWER_COPY
        },
    }
}

/// What the composer's `SEND TO` resolves to, and whether that came from the
/// settings default or from this one story's override.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StorySendToDto {
    pub audience: &'static str,
    pub source: &'static str,
}

pub fn resolve_send_to(
    default_audience: StoryAudience,
    send_to: Option<StoryAudience>,
) -> StorySendToDto {
    match send_to {
        Some(audience) => StorySendToDto {
            audience: audience.stable_id(),
            source: AUDIENCE_SOURCE_OVERRIDE,
        },
        None => StorySendToDto {
            audience: default_audience.stable_id(),
            source: AUDIENCE_SOURCE_DEFAULT,
        },
    }
}
