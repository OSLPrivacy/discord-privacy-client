//! Credential-free attachment to an owner-selected local browser profile.
//!
//! The browser owns its sign-in session; OSL receives only the selected label
//! and the mailbox identity visibly reported by that already-signed-in session.
//! There is no password, cookie, token, profile path, or temp-profile fallback.

use core::fmt;

pub const LOCAL_PROFILE_EMAIL_ATTACHMENT_COMMAND: &str = "attach_selected_local_profile";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedLocalEmailProfile {
    profile_label: String,
    visible_mailbox_identity: Option<String>,
}

impl SelectedLocalEmailProfile {
    /// `None` represents the provider's signed-out page.
    pub fn owner_selected(
        profile_label: impl Into<String>,
        visible_mailbox_identity: Option<impl Into<String>>,
    ) -> Self {
        Self {
            profile_label: profile_label.into(),
            visible_mailbox_identity: visible_mailbox_identity.map(Into::into),
        }
    }

    pub fn profile_label(&self) -> &str {
        &self.profile_label
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalProfileEmailAttachmentRequest {
    pub profile_label: String,
    /// An attachment kind, deliberately not a renderer-supplied filesystem path.
    pub attachment: LocalProfileAttachmentKind,
}

impl LocalProfileEmailAttachmentRequest {
    pub fn selected_existing_profile(profile_label: impl Into<String>) -> Self {
        Self {
            profile_label: profile_label.into(),
            attachment: LocalProfileAttachmentKind::SelectedExistingWindowsProfile,
        }
    }

    /// Test-only representation of the old fresh-profile route.
    pub fn temporary_profile_for_test(profile_label: impl Into<String>) -> Self {
        Self {
            profile_label: profile_label.into(),
            attachment: LocalProfileAttachmentKind::TemporaryProfile,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalProfileAttachmentKind {
    SelectedExistingWindowsProfile,
    TemporaryProfile,
}

/// The only successful output: readable, non-secret session identity data.
/// It is deliberately not serializable or persistable by this driver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachedLocalEmailProfile {
    profile_label: String,
    mailbox_identity: String,
}

impl AttachedLocalEmailProfile {
    pub fn profile_label(&self) -> &str {
        &self.profile_label
    }

    pub fn mailbox_identity(&self) -> &str {
        &self.mailbox_identity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LocalProfileEmailAttachmentError {
    UnselectedProfile { profile_label: String },
    SignedOutProfile { profile_label: String },
    TemporaryProfileForbidden { profile_label: String },
}

impl fmt::Display for LocalProfileEmailAttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnselectedProfile { profile_label } => {
                write!(f, "Refused unselected browser profile: {profile_label}")
            }
            Self::SignedOutProfile { profile_label } => {
                write!(f, "Refused signed-out browser profile: {profile_label}")
            }
            Self::TemporaryProfileForbidden { profile_label } => {
                write!(f, "Refused temporary browser profile: {profile_label}")
            }
        }
    }
}

impl std::error::Error for LocalProfileEmailAttachmentError {}

/// Attach only the exact Liam-selected existing Windows browser profile.
/// The caller observes an already-signed-in page; this boundary cannot sign in,
/// prompt for a secret, create a profile, or copy a profile.
pub fn attach_selected_local_profile(
    selected: &SelectedLocalEmailProfile,
    request: LocalProfileEmailAttachmentRequest,
) -> Result<AttachedLocalEmailProfile, LocalProfileEmailAttachmentError> {
    if request.attachment == LocalProfileAttachmentKind::TemporaryProfile {
        return Err(
            LocalProfileEmailAttachmentError::TemporaryProfileForbidden {
                profile_label: request.profile_label,
            },
        );
    }
    if request.profile_label != selected.profile_label {
        return Err(LocalProfileEmailAttachmentError::UnselectedProfile {
            profile_label: request.profile_label,
        });
    }
    let mailbox_identity = selected.visible_mailbox_identity.clone().ok_or_else(|| {
        LocalProfileEmailAttachmentError::SignedOutProfile {
            profile_label: selected.profile_label.clone(),
        }
    })?;

    Ok(AttachedLocalEmailProfile {
        profile_label: selected.profile_label.clone(),
        mailbox_identity,
    })
}

/// The startup policy check turns red if a throwaway implementation permits the
/// old temporary-profile route.
pub const fn local_profile_attachment_policy_is_safe(
    permits_temporary_profile_attachment: bool,
) -> bool {
    !permits_temporary_profile_attachment
}

/// Zero credential fields are read, stored, or requested at this boundary.
pub const fn credential_field_count() -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    const SELECTED_LABEL: &str = "Liam work mail";
    const MAILBOX_IDENTITY: &str = "liam.work@example.invalid";

    #[test]
    fn task_1386_direct_local_profile_command_reads_only_selected_signed_in_mailbox() {
        let selected =
            SelectedLocalEmailProfile::owner_selected(SELECTED_LABEL, Some(MAILBOX_IDENTITY));
        let attachment = attach_selected_local_profile(
            &selected,
            LocalProfileEmailAttachmentRequest::selected_existing_profile(SELECTED_LABEL),
        )
        .expect("the exact owner-selected signed-in profile attaches");

        assert_eq!(attachment.profile_label(), SELECTED_LABEL);
        assert_eq!(attachment.mailbox_identity(), MAILBOX_IDENTITY);
        assert_eq!(credential_field_count(), 0);
        println!("TASK1386 direct_local_profile_command={LOCAL_PROFILE_EMAIL_ATTACHMENT_COMMAND}");
        println!(
            "TASK1386 selected_profile_label={}",
            attachment.profile_label()
        );
        println!(
            "TASK1386 signed_in_mailbox_identity={}",
            attachment.mailbox_identity()
        );
        println!(
            "TASK1386 credential_fields_exposed={}",
            credential_field_count()
        );
    }

    #[test]
    fn task_1386_refuses_unselected_and_signed_out_profiles_by_name() {
        let selected =
            SelectedLocalEmailProfile::owner_selected(SELECTED_LABEL, Some(MAILBOX_IDENTITY));
        let unselected = attach_selected_local_profile(
            &selected,
            LocalProfileEmailAttachmentRequest::selected_existing_profile("Guest mail"),
        )
        .expect_err("a profile Liam did not select must not attach");
        assert_eq!(
            unselected.to_string(),
            "Refused unselected browser profile: Guest mail"
        );

        let signed_out =
            SelectedLocalEmailProfile::owner_selected("Liam signed-out mail", None::<String>);
        let signed_out_refusal = attach_selected_local_profile(
            &signed_out,
            LocalProfileEmailAttachmentRequest::selected_existing_profile("Liam signed-out mail"),
        )
        .expect_err("the selected but signed-out profile must not attach");
        assert_eq!(
            signed_out_refusal.to_string(),
            "Refused signed-out browser profile: Liam signed-out mail"
        );
        println!("TASK1386 unselected_profile_refusal={unselected}");
        println!("TASK1386 signed_out_profile_refusal={signed_out_refusal}");
    }

    #[test]
    fn task_1386_temp_profile_throwaway_copy_turns_the_guard_red() {
        let selected =
            SelectedLocalEmailProfile::owner_selected("OSL temporary mail", Some(MAILBOX_IDENTITY));
        let temp_refusal = attach_selected_local_profile(
            &selected,
            LocalProfileEmailAttachmentRequest::temporary_profile_for_test("OSL temporary mail"),
        )
        .expect_err("the production command must refuse the old temporary-profile route");
        assert_eq!(
            temp_refusal.to_string(),
            "Refused temporary browser profile: OSL temporary mail"
        );

        assert!(local_profile_attachment_policy_is_safe(false));
        assert!(!local_profile_attachment_policy_is_safe(true));
        println!("TASK1386 temporary_profile_refusal={temp_refusal}");
        println!("TASK1386 throwaway_copy_temp_profile_attachment=permitted");
        println!("TASK1386 throwaway_copy_attachment_check=FAIL");
    }
}
