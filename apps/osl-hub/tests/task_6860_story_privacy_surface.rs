//! TASK 6860 — the hub's story privacy surface.
//!
//! Lives as an integration target because it exercises the shipped library the
//! way the Tauri command layer does: through the public surface, with the
//! platform answering for itself.

use osl_privacy_hub::story_privacy_surface::{
    resolve_send_to, story_privacy_surface, story_privacy_surface_for, StorySendToDto,
};
use story_privacy::{
    StoryAudience, StoryLifetime, SHIELD_DISCLOSURE, SHIELD_PRIMITIVE, SHIELD_UNAVAILABLE_COPY,
    VIEW_RECEIPT_OFF_VIEWER_COPY, VIEW_RECEIPT_VIEWER_COPY,
};
#[test]
fn the_surface_offers_three_audiences_and_three_deadlines() {
    let dto = story_privacy_surface_for(
        true,
        StoryAudience::Friends,
        StoryLifetime::TwelveHours,
        true,
        false,
    );
    assert_eq!(
        dto.audiences.iter().map(|row| row.id).collect::<Vec<_>>(),
        vec![
            "story-audience-everyone",
            "story-audience-friends",
            "story-audience-verified"
        ]
    );
    assert_eq!(
        dto.lifetimes.iter().map(|row| row.label).collect::<Vec<_>>(),
        vec!["1H", "12H", "24H"]
    );
    assert_eq!(
        dto.lifetimes.iter().map(|row| row.seconds).collect::<Vec<_>>(),
        vec![3_600, 43_200, 86_400]
    );
    assert_eq!(dto.audiences.iter().filter(|row| row.selected).count(), 1);
    assert_eq!(dto.lifetimes.iter().filter(|row| row.selected).count(), 1);
}

#[test]
fn the_shield_row_claims_nothing_where_no_primitive_exists() {
    let unsupported = story_privacy_surface_for(
        false,
        StoryAudience::Friends,
        StoryLifetime::OneHour,
        true,
        false,
    );
    assert!(!unsupported.shield.available);
    assert!(!unsupported.shield.control_enabled);
    assert!(!unsupported.shield.on);
    assert!(!unsupported.shield.claims_protection);
    assert_eq!(unsupported.shield.primitive, None);
    assert_eq!(unsupported.shield.disclosure, None);
    assert_eq!(
        unsupported.shield.unavailable_copy,
        Some(SHIELD_UNAVAILABLE_COPY)
    );

    let supported = story_privacy_surface_for(
        true,
        StoryAudience::Friends,
        StoryLifetime::OneHour,
        true,
        false,
    );
    assert!(supported.shield.claims_protection);
    assert_eq!(supported.shield.primitive, Some(SHIELD_PRIMITIVE));
    assert_eq!(supported.shield.disclosure, Some(SHIELD_DISCLOSURE));
    assert_eq!(supported.shield.unavailable_copy, None);
}

#[test]
fn the_platform_itself_decides_what_this_build_may_say() {
    let dto = story_privacy_surface(
        StoryAudience::Friends,
        StoryLifetime::TwentyFourHours,
        true,
        false,
    );
    assert_eq!(dto.shield.available, cfg!(windows));
    assert_eq!(dto.shield.claims_protection, cfg!(windows));
}

#[test]
fn send_to_reports_whether_it_inherited_or_overrode() {
    assert_eq!(
        resolve_send_to(StoryAudience::Everyone, None),
        StorySendToDto {
            audience: "story-audience-everyone",
            source: "inherited-default",
        }
    );
    assert_eq!(
        resolve_send_to(StoryAudience::Everyone, Some(StoryAudience::Verified)),
        StorySendToDto {
            audience: "story-audience-verified",
            source: "send-to-override",
        }
    );
}

#[test]
fn the_viewer_sentence_matches_the_receipt_mode() {
    let on = story_privacy_surface_for(
        true,
        StoryAudience::Friends,
        StoryLifetime::OneHour,
        false,
        true,
    );
    assert_eq!(on.viewer_pre_open_copy, VIEW_RECEIPT_VIEWER_COPY);
    let off = story_privacy_surface_for(
        true,
        StoryAudience::Friends,
        StoryLifetime::OneHour,
        false,
        false,
    );
    assert_eq!(off.viewer_pre_open_copy, VIEW_RECEIPT_OFF_VIEWER_COPY);
}
