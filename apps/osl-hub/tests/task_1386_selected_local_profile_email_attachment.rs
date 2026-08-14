use osl_privacy_hub::local_profile_email_driver::{
    attach_selected_local_profile, credential_field_count, local_profile_attachment_policy_is_safe,
    LocalProfileEmailAttachmentRequest, SelectedLocalEmailProfile,
    LOCAL_PROFILE_EMAIL_ATTACHMENT_COMMAND,
};

const SELECTED_LABEL: &str = "Liam work mail";
const MAILBOX_IDENTITY: &str = "liam.work@example.invalid";
const UNSELECTED_LABEL: &str = "Guest mail";
const SIGNED_OUT_LABEL: &str = "Liam signed-out mail";
const TEMPORARY_LABEL: &str = "OSL temporary mail";

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
        LocalProfileEmailAttachmentRequest::selected_existing_profile(UNSELECTED_LABEL),
    )
    .expect_err("a profile Liam did not select must not attach");
    assert_eq!(
        unselected.to_string(),
        "Refused unselected browser profile: Guest mail"
    );

    let signed_out = SelectedLocalEmailProfile::owner_selected(SIGNED_OUT_LABEL, None::<String>);
    let signed_out_refusal = attach_selected_local_profile(
        &signed_out,
        LocalProfileEmailAttachmentRequest::selected_existing_profile(SIGNED_OUT_LABEL),
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
        SelectedLocalEmailProfile::owner_selected(TEMPORARY_LABEL, Some(MAILBOX_IDENTITY));
    let temp_refusal = attach_selected_local_profile(
        &selected,
        LocalProfileEmailAttachmentRequest::temporary_profile_for_test(TEMPORARY_LABEL),
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
