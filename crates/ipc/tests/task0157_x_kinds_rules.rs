use ipc::allowed_places::{count_allowed_place_records_for_kind, get_allowed_place_record};
use ipc::auto_whitelist_rules::{X_DIRECT_MESSAGE_PLACE_KIND, X_PUBLIC_POST_PLACE_KIND};
use ipc::commands::{
    cmd_osl_direct_new_place, cmd_osl_get_auto_whitelist_rule_for_place,
    cmd_osl_save_auto_whitelist_rule_for_place,
};
use ipc::AppState;
use tempfile::tempdir;

#[test]
fn x_place_kinds_have_independent_rule_lookups_and_allowed_records() {
    let state = AppState::new();
    let dirs = tempdir().expect("temp dirs");
    let app_data_dir = dirs.path().join("app-data");
    let prefs_dir = dirs.path().join("prefs");

    let direct_saved = cmd_osl_save_auto_whitelist_rule_for_place(
        &state,
        "x".to_string(),
        X_DIRECT_MESSAGE_PLACE_KIND.to_string(),
        "always".to_string(),
        Some(prefs_dir.clone()),
    )
    .expect("save X direct-message rule");
    assert_eq!(direct_saved, "always");

    let direct_decision = cmd_osl_direct_new_place(
        &state,
        app_data_dir.clone(),
        "x".to_string(),
        X_DIRECT_MESSAGE_PLACE_KIND.to_string(),
        "x-dm-0157".to_string(),
        Some("task 0157 X direct message".to_string()),
    )
    .expect("direct X direct-message place");
    assert_eq!(direct_decision.result, "allowed");

    let post_saved = cmd_osl_save_auto_whitelist_rule_for_place(
        &state,
        "x".to_string(),
        X_PUBLIC_POST_PLACE_KIND.to_string(),
        "always".to_string(),
        Some(prefs_dir.clone()),
    )
    .expect("save X public-post rule");
    assert_eq!(post_saved, "always");

    let post_decision = cmd_osl_direct_new_place(
        &state,
        app_data_dir.clone(),
        "x".to_string(),
        X_PUBLIC_POST_PLACE_KIND.to_string(),
        "x-post-0157".to_string(),
        Some("task 0157 X public post".to_string()),
    )
    .expect("direct X public-post place");
    assert_eq!(post_decision.result, "allowed");

    let direct_changed = cmd_osl_save_auto_whitelist_rule_for_place(
        &state,
        "x".to_string(),
        X_DIRECT_MESSAGE_PLACE_KIND.to_string(),
        "only if a friend".to_string(),
        Some(prefs_dir),
    )
    .expect("change only X direct-message rule");
    assert_eq!(direct_changed, "only if a friend");

    let direct_lookup = cmd_osl_get_auto_whitelist_rule_for_place(
        &state,
        "x".to_string(),
        X_DIRECT_MESSAGE_PLACE_KIND.to_string(),
    )
    .expect("lookup X direct-message rule");
    let post_lookup = cmd_osl_get_auto_whitelist_rule_for_place(
        &state,
        "x".to_string(),
        X_PUBLIC_POST_PLACE_KIND.to_string(),
    )
    .expect("lookup X public-post rule");

    let direct_record =
        get_allowed_place_record(&app_data_dir, "x", X_DIRECT_MESSAGE_PLACE_KIND, "x-dm-0157")
            .expect("read X direct-message allowed place")
            .expect("X direct-message allowed place exists");
    let post_record =
        get_allowed_place_record(&app_data_dir, "x", X_PUBLIC_POST_PLACE_KIND, "x-post-0157")
            .expect("read X public-post allowed place")
            .expect("X public-post allowed place exists");

    let direct_count =
        count_allowed_place_records_for_kind(&app_data_dir, "x", X_DIRECT_MESSAGE_PLACE_KIND)
            .expect("count X direct-message allowed places");
    let post_count =
        count_allowed_place_records_for_kind(&app_data_dir, "x", X_PUBLIC_POST_PLACE_KIND)
            .expect("count X public-post allowed places");

    println!("TASK0157 direct_lookup.x_direct_message={direct_lookup}");
    println!("TASK0157 direct_lookup.x_public_post={post_lookup}");
    println!("TASK0157 allowed_place.x_direct_message.count={direct_count}");
    println!("TASK0157 allowed_place.x_public_post.count={post_count}");
    println!(
        "TASK0157 allowed_place.x_direct_message.record={}/{}",
        direct_record.app_kind, direct_record.place_kind
    );
    println!(
        "TASK0157 allowed_place.x_public_post.record={}/{}",
        post_record.app_kind, post_record.place_kind
    );

    assert_eq!(direct_lookup, "only if a friend");
    assert_eq!(post_lookup, "always");
    assert_eq!(direct_count, 1);
    assert_eq!(post_count, 1);
    assert_eq!(direct_record.app_kind, "x");
    assert_eq!(direct_record.place_kind, X_DIRECT_MESSAGE_PLACE_KIND);
    assert_eq!(post_record.app_kind, "x");
    assert_eq!(post_record.place_kind, X_PUBLIC_POST_PLACE_KIND);
}
