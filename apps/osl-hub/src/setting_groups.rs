//! The fail-closed Settings-screen ownership inventory.
//!
//! A setting can be reset only through the screen which owns it.  Keeping the
//! mapping here makes an omitted persisted setting visible instead of silently
//! assigning it to a guessed screen.

/// One persisted setting and the Settings screen that owns it.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SavedSettingGroup {
    pub setting: &'static str,
    pub group: &'static str,
}

/// Every setting covered by the safe-settings reset surface.
pub const SAVED_SETTING_GROUPS: [SavedSettingGroup; 29] = [
    SavedSettingGroup {
        setting: "auto_whitelist_rule",
        group: "Whitelisting",
    },
    SavedSettingGroup {
        setting: "profile_picture",
        group: "Account",
    },
    SavedSettingGroup {
        setting: "friend_account_reach",
        group: "Whitelisting",
    },
    SavedSettingGroup {
        setting: "friend_auto_whitelist",
        group: "Whitelisting",
    },
    SavedSettingGroup {
        setting: "friend_verification_warnings",
        group: "Whitelisting",
    },
    SavedSettingGroup {
        setting: "privacy_level",
        group: "Privacy",
    },
    SavedSettingGroup {
        setting: "chat_approval_suggestions",
        group: "Notifications",
    },
    SavedSettingGroup {
        setting: "app_notifications",
        group: "Notifications",
    },
    SavedSettingGroup {
        setting: "next_generation_messages",
        group: "Apps and sending",
    },
    SavedSettingGroup {
        setting: "verification_warning",
        group: "Privacy",
    },
    SavedSettingGroup {
        setting: "message_burn_scope",
        group: "Apps and sending",
    },
    SavedSettingGroup {
        setting: "message_timer",
        group: "Apps and sending",
    },
    SavedSettingGroup {
        setting: "message_view_once_duration",
        group: "Apps and sending",
    },
    SavedSettingGroup {
        setting: "message_writer",
        group: "Apps and sending",
    },
    SavedSettingGroup {
        setting: "look_theme",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_named_look",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_accent",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_corners",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_glow",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_text",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_spacing",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "look_see_through",
        group: "Look",
    },
    SavedSettingGroup {
        setting: "behaviour_position",
        group: "Behaviour",
    },
    SavedSettingGroup {
        setting: "behaviour_remember_place",
        group: "Behaviour",
    },
    SavedSettingGroup {
        setting: "behaviour_movement",
        group: "Behaviour",
    },
    SavedSettingGroup {
        setting: "behaviour_tray_picture",
        group: "Behaviour",
    },
    SavedSettingGroup {
        setting: "behaviour_sound",
        group: "Behaviour",
    },
    SavedSettingGroup {
        setting: "behaviour_mute",
        group: "Behaviour",
    },
    SavedSettingGroup {
        setting: "behaviour_quiet_hours",
        group: "Behaviour",
    },
];

/// Returns the owning Settings screen for a saved setting.
pub fn saved_setting_group(setting: &str) -> Result<&'static str, String> {
    SAVED_SETTING_GROUPS
        .iter()
        .find(|entry| entry.setting == setting)
        .map(|entry| entry.group)
        .ok_or_else(|| format!("OSL saved setting '{setting}' has no Settings group"))
}

/// Refuses unknown screen labels, including spelling and case variants.
pub fn validate_saved_settings_group(group: &str) -> Result<(), String> {
    const GROUPS: [&str; 7] = [
        "Whitelisting",
        "Account",
        "Privacy",
        "Notifications",
        "Apps and sending",
        "Look",
        "Behaviour",
    ];
    if !GROUPS.contains(&group) {
        return Err(format!("OSL Settings group '{group}' is unknown"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{saved_setting_group, validate_saved_settings_group, SAVED_SETTING_GROUPS};

    #[test]
    fn every_saved_setting_has_one_named_settings_group() {
        for entry in SAVED_SETTING_GROUPS {
            assert_eq!(saved_setting_group(entry.setting).unwrap(), entry.group);
            validate_saved_settings_group(entry.group).unwrap();
        }
    }

    #[test]
    fn unknown_settings_group_is_refused() {
        let error = validate_saved_settings_group("unknown-0861")
            .expect_err("a reset must never guess an unknown Settings screen");
        assert!(error.contains("unknown-0861"));
    }
}
