//! Minimal public-cover observation seam for X.
//!
//! A public cover is only evidence of public visibility after a separate,
//! signed-out browser profile has received and displayed the exact marked post.
//! The receiving job is injected because it is the boundary that moves public
//! X content from the provider surface into that profile's observed timeline.

/// One marked cover sent to X's public timeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XMarkedPublicCover {
    pub marker: String,
    pub text: String,
}

/// A deliberately separate browser profile used by the public-cover check.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XBrowserProfile {
    profile_id: String,
    signed_in: bool,
    observed_public_covers: Vec<XMarkedPublicCover>,
}

impl XBrowserProfile {
    pub fn signed_in(profile_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            signed_in: true,
            observed_public_covers: Vec::new(),
        }
    }

    pub fn signed_out(profile_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            signed_in: false,
            observed_public_covers: Vec::new(),
        }
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub const fn is_signed_in(&self) -> bool {
        self.signed_in
    }

    /// The only receiver-side mutation: an X receiving job records what this
    /// profile actually observed on the public surface.
    pub fn record_public_observation(&mut self, cover: XMarkedPublicCover) {
        self.observed_public_covers.push(cover);
    }

    fn sees_exactly(&self, cover: &XMarkedPublicCover) -> bool {
        self.observed_public_covers
            .iter()
            .any(|observed| observed == cover)
    }
}

/// Public X surface shared by independent browser profiles.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct XPublicTimeline {
    covers: Vec<XMarkedPublicCover>,
}

impl XPublicTimeline {
    pub fn marked_cover(&self) -> Option<&XMarkedPublicCover> {
        self.covers.last()
    }
}

/// The evidence recorded by one public-cover warning run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XPublicCoverWarningRun {
    pub marked_cover: XMarkedPublicCover,
    pub sender_profile_id: String,
    pub signed_out_profile_id: String,
    pub receive_job_runs: usize,
    pub signed_out_profile_sees_cover: bool,
    pub public_visibility_recorded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XPublicCoverWarningError {
    SenderIsSignedOut,
    InspectionProfileIsSignedIn,
    ProfilesMustBeSeparate,
    EmptyMarker,
    EmptyCover,
    SignedOutProfileDidNotSeeMarkedCover,
}

impl core::fmt::Display for XPublicCoverWarningError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SenderIsSignedOut => f.write_str("X public cover sender is signed out"),
            Self::InspectionProfileIsSignedIn => {
                f.write_str("X public cover inspection profile must be signed out")
            }
            Self::ProfilesMustBeSeparate => {
                f.write_str("X public cover sender and inspection profiles must be separate")
            }
            Self::EmptyMarker => f.write_str("X public cover marker is empty"),
            Self::EmptyCover => f.write_str("X public cover text is empty"),
            Self::SignedOutProfileDidNotSeeMarkedCover => {
                f.write_str("signed-out X profile did not see the marked public cover")
            }
        }
    }
}

impl std::error::Error for XPublicCoverWarningError {}

/// Send one marked public cover, run the X receiver for an independent
/// signed-out profile, and record public visibility only after that profile
/// sees the exact cover. A no-op receiver cannot produce a successful run.
pub fn run_x_public_cover_warning<R>(
    sender: &XBrowserProfile,
    signed_out_profile: &mut XBrowserProfile,
    marker: &str,
    cover_text: &str,
    mut receive_job: R,
) -> Result<XPublicCoverWarningRun, XPublicCoverWarningError>
where
    R: FnMut(&XPublicTimeline, &mut XBrowserProfile),
{
    if !sender.is_signed_in() {
        return Err(XPublicCoverWarningError::SenderIsSignedOut);
    }
    if signed_out_profile.is_signed_in() {
        return Err(XPublicCoverWarningError::InspectionProfileIsSignedIn);
    }
    if sender.profile_id() == signed_out_profile.profile_id() {
        return Err(XPublicCoverWarningError::ProfilesMustBeSeparate);
    }
    if marker.trim().is_empty() {
        return Err(XPublicCoverWarningError::EmptyMarker);
    }
    if cover_text.trim().is_empty() {
        return Err(XPublicCoverWarningError::EmptyCover);
    }

    let marked_cover = XMarkedPublicCover {
        marker: marker.to_owned(),
        text: cover_text.to_owned(),
    };
    let timeline = XPublicTimeline {
        covers: vec![marked_cover.clone()],
    };
    receive_job(&timeline, signed_out_profile);

    let signed_out_profile_sees_cover = signed_out_profile.sees_exactly(&marked_cover);
    let run = XPublicCoverWarningRun {
        marked_cover,
        sender_profile_id: sender.profile_id().to_owned(),
        signed_out_profile_id: signed_out_profile.profile_id().to_owned(),
        receive_job_runs: 1,
        signed_out_profile_sees_cover,
        public_visibility_recorded: signed_out_profile_sees_cover,
    };

    if !run.signed_out_profile_sees_cover {
        return Err(XPublicCoverWarningError::SignedOutProfileDidNotSeeMarkedCover);
    }
    Ok(run)
}
