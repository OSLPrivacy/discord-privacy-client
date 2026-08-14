use osl_privacy_hub::x_public_cover::{
    run_x_public_cover_warning, XBrowserProfile, XPublicTimeline,
};

const MARKER: &str = "TASK1111-X-PUBLIC-COVER";
const COVER: &str = "A marked public cover visible to a signed-out X profile.";

fn receive_marked_public_cover(timeline: &XPublicTimeline, profile: &mut XBrowserProfile) {
    let cover = timeline
        .marked_cover()
        .expect("the X sending job placed one public cover")
        .clone();
    profile.record_public_observation(cover);
}

#[test]
fn task_1111_signed_out_x_profile_sees_marked_public_cover_and_records_visibility() {
    let sender = XBrowserProfile::signed_in("x-sender-profile-1111");
    let mut signed_out = XBrowserProfile::signed_out("x-signed-out-profile-1111");
    let stubbed_receive_job = std::env::var_os("TASK1111_STUB_X_RECEIVE_JOB").is_some();

    let result = run_x_public_cover_warning(
        &sender,
        &mut signed_out,
        MARKER,
        COVER,
        |timeline, profile| {
            if !stubbed_receive_job {
                receive_marked_public_cover(timeline, profile);
            }
        },
    );

    match result {
        Ok(run) => {
            assert!(sender.is_signed_in());
            assert!(!signed_out.is_signed_in());
            assert_ne!(run.sender_profile_id, run.signed_out_profile_id);
            assert_eq!(run.marked_cover.marker, MARKER);
            assert_eq!(run.marked_cover.text, COVER);
            assert_eq!(run.receive_job_runs, 1);
            assert!(run.signed_out_profile_sees_cover);
            assert!(run.public_visibility_recorded);
            println!("TASK1111_MARKER={}", run.marked_cover.marker);
            println!(
                "TASK1111_SIGNED_OUT_PROFILE_SEES_COVER={}",
                run.signed_out_profile_sees_cover
            );
            println!(
                "TASK1111_PUBLIC_VISIBILITY_RECORDED={}",
                run.public_visibility_recorded
            );
            println!("TASK1111_X_RECEIVE_JOB_RUNS={}", run.receive_job_runs);
        }
        Err(error) => {
            println!("TASK1111_STUBBED_X_RECEIVE_JOB={stubbed_receive_job}");
            println!("TASK1111_CHECK_FAILURE={error}");
            panic!("TASK1111 signed-out public-cover check failed: {error}");
        }
    }
}
