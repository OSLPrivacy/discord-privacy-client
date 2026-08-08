//! Fail-closed publish policy for protected Instagram stories.
//!
//! Instagram removes a story after 24 hours. OSL may choose a shorter timer,
//! but it must never promise a longer lifetime than the provider. Publishing
//! is also audience-bound: every selected member must have an exact persisted
//! Instagram public-post allowance for the publishing account.

use serde::Serialize;
use std::collections::BTreeSet;

pub const INSTAGRAM_STORY_LIFETIME_HOURS: i64 = 24;
pub const INSTAGRAM_STORY_LIFETIME_SECONDS: i64 = INSTAGRAM_STORY_LIFETIME_HOURS * 60 * 60;
pub const INSTAGRAM_STORY_ALLOWED_PLACE_KIND: &str = "public_post";
pub const MAX_INSTAGRAM_STORY_AUDIENCE_MEMBERS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramStoryPublishInput {
    pub account: String,
    pub selected_audience: Vec<String>,
    pub published_at: i64,
    pub osl_expires_at: i64,
    /// The effective expiry presented by the composer. A missing or different
    /// value keeps the publish control unavailable and cannot be bypassed by
    /// invoking publish directly.
    pub presented_effective_expires_at: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramStoryPublishControl {
    pub publish_control: &'static str,
    pub available: bool,
    pub allowed_audience: bool,
    pub earlier_expiry_matches: bool,
    pub earlier_expires_at: i64,
    pub expiry_source: &'static str,
    pub instagram_expires_at: i64,
    pub audience_count: usize,
    pub unallowed_audience: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramStoryPublishReceipt {
    pub publish_control: &'static str,
    pub effective_expires_at: i64,
    pub expiry_source: &'static str,
    pub instagram_expires_at: i64,
    pub maximum_visible_story_life_hours: i64,
    pub audience_count: usize,
}

/// The complete availability condition. Keeping the conjunction here makes it
/// impossible for composer details unrelated to audience authority or the
/// effective expiry to enable publishing.
pub const fn instagram_story_publish_control_available(
    allowed_audience: bool,
    earlier_expiry_matches: bool,
) -> bool {
    allowed_audience && earlier_expiry_matches
}

pub fn inspect_instagram_story_publish_control(
    input: &InstagramStoryPublishInput,
    mut audience_member_is_allowed: impl FnMut(&str, &str) -> Result<bool, String>,
) -> Result<InstagramStoryPublishControl, String> {
    validate_input(input)?;
    let instagram_expires_at = input
        .published_at
        .checked_add(INSTAGRAM_STORY_LIFETIME_SECONDS)
        .ok_or_else(|| "Instagram story 24-hour expiry is out of range".to_owned())?;
    let earlier_expires_at = input.osl_expires_at.min(instagram_expires_at);
    let expiry_source = if input.osl_expires_at <= instagram_expires_at {
        "osl_timer"
    } else {
        "instagram_24_hours"
    };

    let mut unallowed_audience = Vec::new();
    for member in &input.selected_audience {
        if !audience_member_is_allowed(&input.account, member)? {
            unallowed_audience.push(member.clone());
        }
    }
    let allowed_audience = unallowed_audience.is_empty();
    let earlier_expiry_matches = input.presented_effective_expires_at == Some(earlier_expires_at);
    let available =
        instagram_story_publish_control_available(allowed_audience, earlier_expiry_matches);

    Ok(InstagramStoryPublishControl {
        publish_control: if available {
            "available"
        } else {
            "unavailable"
        },
        available,
        allowed_audience,
        earlier_expiry_matches,
        earlier_expires_at,
        expiry_source,
        instagram_expires_at,
        audience_count: input.selected_audience.len(),
        unallowed_audience,
    })
}

/// Re-check the complete condition at the direct backend invocation boundary.
pub fn invoke_instagram_story_publish(
    input: &InstagramStoryPublishInput,
    audience_member_is_allowed: impl FnMut(&str, &str) -> Result<bool, String>,
) -> Result<InstagramStoryPublishReceipt, String> {
    let control = inspect_instagram_story_publish_control(input, audience_member_is_allowed)?;
    if let Some(member) = control.unallowed_audience.first() {
        return Err(format!(
            "Instagram story audience member is not allowed: {member}"
        ));
    }
    if !control.earlier_expiry_matches {
        return Err(format!(
            "Instagram story effective expiry must equal the earlier expiry: {}",
            control.earlier_expires_at
        ));
    }
    if !control.available {
        return Err("Instagram story publish control is unavailable".to_owned());
    }

    Ok(InstagramStoryPublishReceipt {
        publish_control: "available",
        effective_expires_at: control.earlier_expires_at,
        expiry_source: control.expiry_source,
        instagram_expires_at: control.instagram_expires_at,
        maximum_visible_story_life_hours: INSTAGRAM_STORY_LIFETIME_HOURS,
        audience_count: control.audience_count,
    })
}

pub fn instagram_story_audience_stable_id(account: &str, member: &str) -> String {
    format!("instagram:{account}:{INSTAGRAM_STORY_ALLOWED_PLACE_KIND}:{member}")
}

fn validate_input(input: &InstagramStoryPublishInput) -> Result<(), String> {
    validate_component(&input.account, "Instagram story account is invalid")?;
    if input.selected_audience.is_empty()
        || input.selected_audience.len() > MAX_INSTAGRAM_STORY_AUDIENCE_MEMBERS
    {
        return Err("Instagram story selected audience is invalid".to_owned());
    }
    let mut unique = BTreeSet::new();
    for member in &input.selected_audience {
        validate_component(member, "Instagram story audience member is invalid")?;
        if member == &input.account || !unique.insert(member) {
            return Err("Instagram story selected audience is invalid".to_owned());
        }
    }
    if input.published_at < 0 || input.osl_expires_at <= input.published_at {
        return Err("Instagram story OSL timer expiry is invalid".to_owned());
    }
    Ok(())
}

fn validate_component(value: &str, message: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 512
        || value.contains('\0')
        || value.contains(',')
        || value.contains(':')
        || value.chars().any(char::is_whitespace)
    {
        return Err(message.to_owned());
    }
    Ok(())
}
