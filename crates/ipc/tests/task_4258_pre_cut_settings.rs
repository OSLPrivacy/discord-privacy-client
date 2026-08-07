use ipc::service_settings_restore::{
    load_pre_cut_settings_file, load_pre_cut_settings_file_with_service_short_names,
};

const PRE_CUT_SETTINGS: &[u8] = br#"{
  "schemaVersion": 1,
  "friends": [
    {
      "serviceId": "discord",
      "personId": "friend-discord-4258",
      "displayName": "Discord Friend 4258"
    },
    {
      "serviceId": "email",
      "personId": "friend-email-4258",
      "displayName": "Email Friend 4258"
    },
    {
      "serviceId": "messenger",
      "personId": "friend-messenger-4258",
      "displayName": "Messenger Friend 4258"
    }
  ],
  "allowedPlaces": [
    {
      "app": "discord",
      "account": "account-discord-4258",
      "kind": "direct_message",
      "stable_id": "discord:account-discord-4258:direct_message:place-discord-4258",
      "place_name": "Discord place 4258",
      "person_name": "friend-discord-4258"
    },
    {
      "app": "email",
      "account": "account-email-4258",
      "kind": "email_address",
      "stable_id": "email:account-email-4258:email_address:place-email-4258",
      "place_name": "Email place 4258",
      "person_name": "friend-email-4258"
    },
    {
      "app": "messenger",
      "account": "account-messenger-4258",
      "kind": "direct_message",
      "stable_id": "messenger:account-messenger-4258:direct_message:place-messenger-4258",
      "place_name": "Messenger place 4258",
      "person_name": "friend-messenger-4258"
    }
  ],
  "savedSettings": [
    {
      "serviceId": "discord",
      "settingId": "new-place-rule",
      "value": "ask_me"
    },
    {
      "serviceId": "email",
      "settingId": "new-place-rule",
      "value": "only_if_a_friend"
    },
    {
      "serviceId": "messenger",
      "settingId": "new-place-rule",
      "value": "ask_me"
    }
  ]
}"#;

#[test]
fn task_4258_pre_cut_settings_load_only_with_exact_restored_service_short_names() {
    let restored = load_pre_cut_settings_file(PRE_CUT_SETTINGS).expect("pre-cut settings parse");
    println!(
        "TASK4258_LOAD unknown_service_errors={} friends_found={} allowed_places_found={} saved_settings_found={}",
        restored.unknown_service_errors.len(),
        restored.friends_found,
        restored.allowed_places_found,
        restored.saved_settings_found
    );

    assert_eq!(restored.unknown_service_errors.len(), 0);
    assert_eq!(restored.friends_found, 3);
    assert_eq!(restored.allowed_places_found, 3);
    assert_eq!(restored.saved_settings_found, 3);

    let changed = load_pre_cut_settings_file_with_service_short_names(
        PRE_CUT_SETTINGS,
        &["discord", "email", "messengerx"],
    )
    .expect("same pre-cut settings parse with changed restored short name");
    println!(
        "TASK4258_ONE_CHAR_CHANGE changed_short_name=messengerx unknown_service_errors={} friends_found={} allowed_places_found={} saved_settings_found={}",
        changed.unknown_service_errors.len(),
        changed.friends_found,
        changed.allowed_places_found,
        changed.saved_settings_found
    );

    assert!(changed.unknown_service_errors.len() >= 1);
    assert_eq!(changed.friends_found, 2);
    assert_eq!(changed.allowed_places_found, 2);
    assert_eq!(changed.saved_settings_found, 2);
}
