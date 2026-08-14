use osl_privacy_hub::instagram_story_tools::{
    instagram_story_controls_availability, require_instagram_story_controls,
    InstagramStoryControlsAvailability, DESKTOP_PLAIN_UPLOADED_FILE,
    INSTAGRAM_STORY_TOOL_UNAVAILABLE,
};

const NAMED_PHONE_ONLY_TOOLS: [&str; 5] = ["polls", "music", "camera", "drawing", "filters"];

#[test]
fn task_1156_every_named_phone_only_story_tool_returns_unavailable() {
    let mut unavailable = Vec::new();

    for tool in NAMED_PHONE_ONLY_TOOLS {
        let direct = instagram_story_controls_availability(tool);
        assert_eq!(
            direct,
            InstagramStoryControlsAvailability::Unavailable,
            "{tool} must never receive OSL story controls"
        );
        assert_eq!(direct.as_str(), INSTAGRAM_STORY_TOOL_UNAVAILABLE);

        let refusal = require_instagram_story_controls(tool)
            .expect_err("a phone-only Instagram story tool must be refused");
        assert_eq!(refusal.tool(), tool);
        assert_eq!(refusal.availability(), direct);
        assert_eq!(refusal.to_string(), INSTAGRAM_STORY_TOOL_UNAVAILABLE);
        unavailable.push(format!("{tool}={}", direct.as_str()));
    }

    println!(
        "TASK1156_NAMED_PHONE_ONLY_TOOLS {} count={}",
        unavailable.join(" "),
        unavailable.len()
    );
}

#[test]
fn task_1156_other_and_future_phone_only_tools_fail_closed() {
    for tool in [
        "stickers",
        "text",
        "layout",
        "boomerang",
        "hands_free",
        "dual",
        "green_screen",
        "a_future_instagram_phone_tool",
        "Plain_Uploaded_File",
        "plain_uploaded_file ",
        "",
    ] {
        assert_eq!(
            instagram_story_controls_availability(tool),
            InstagramStoryControlsAvailability::Unavailable,
            "unknown, aliased, or other phone-only tool {tool:?} must fail closed"
        );
        assert!(require_instagram_story_controls(tool).is_err());
    }

    assert_eq!(
        instagram_story_controls_availability(DESKTOP_PLAIN_UPLOADED_FILE),
        InstagramStoryControlsAvailability::Available
    );
    assert_eq!(
        require_instagram_story_controls(DESKTOP_PLAIN_UPLOADED_FILE),
        Ok(())
    );

    println!(
        "TASK1156_OTHER_PHONE_ONLY_TOOLS=11 status={} supported_desktop_tool={}=available",
        INSTAGRAM_STORY_TOOL_UNAVAILABLE, DESKTOP_PLAIN_UPLOADED_FILE
    );
}
