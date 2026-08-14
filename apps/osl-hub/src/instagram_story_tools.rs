//! Availability boundary for OSL controls on Instagram story tools.
//!
//! OSL can protect the desktop composer introduced for one plain uploaded
//! file. Instagram's poll, music, camera, drawing, and filter editors are
//! phone-only surfaces that OSL cannot inspect or control. The boundary is
//! deliberately fail-closed: every tool identifier except the one supported
//! desktop composer returns `unavailable`, including future tools that this
//! build does not know by name.

use std::fmt;

pub const DESKTOP_PLAIN_UPLOADED_FILE: &str = "plain_uploaded_file";
pub const INSTAGRAM_STORY_TOOL_UNAVAILABLE: &str = "unavailable";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstagramStoryControlsAvailability {
    Available,
    Unavailable,
}

impl InstagramStoryControlsAvailability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => INSTAGRAM_STORY_TOOL_UNAVAILABLE,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstagramStoryToolUnavailable {
    tool: String,
}

impl InstagramStoryToolUnavailable {
    pub fn tool(&self) -> &str {
        &self.tool
    }

    pub const fn availability(&self) -> InstagramStoryControlsAvailability {
        InstagramStoryControlsAvailability::Unavailable
    }
}

impl fmt::Display for InstagramStoryToolUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(INSTAGRAM_STORY_TOOL_UNAVAILABLE)
    }
}

impl std::error::Error for InstagramStoryToolUnavailable {}

/// Report whether OSL story controls may be shown for this exact tool.
///
/// Do not trim, fold case, or accept aliases here. A changed or newly added
/// Instagram tool must stay unavailable until its surface has its own review.
pub fn instagram_story_controls_availability(tool: &str) -> InstagramStoryControlsAvailability {
    if tool == DESKTOP_PLAIN_UPLOADED_FILE {
        InstagramStoryControlsAvailability::Available
    } else {
        InstagramStoryControlsAvailability::Unavailable
    }
}

/// Direct action gate used before attaching OSL controls to a story editor.
pub fn require_instagram_story_controls(tool: &str) -> Result<(), InstagramStoryToolUnavailable> {
    match instagram_story_controls_availability(tool) {
        InstagramStoryControlsAvailability::Available => Ok(()),
        InstagramStoryControlsAvailability::Unavailable => Err(InstagramStoryToolUnavailable {
            tool: tool.to_owned(),
        }),
    }
}
