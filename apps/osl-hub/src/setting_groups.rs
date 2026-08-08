//! A single, fail-closed inventory of every user setting saved by the Hub.
//!
//! Names are stable backend identifiers rather than UI labels.  The group is
//! the exact Settings screen title that owns the setting, so a direct read can
//! explain where the owner can change it.

/// One saved setting and the Settings screen that owns it.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SavedSettingGroup {
    pub setting: &'static str,
    pub group: &'static str,
}

/// The complete set of saved settings covered by TASK 0796's direct reads.
///
/// Keep this inventory deliberately explicit. A new persisted setting must be
/// added here before it can claim a Settings group; unknown names are refused
/// below instead of being silently assigned to a guessed screen.
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

/// Read the named Settings screen for a saved setting.
///
/// This is intentionally fail-closed: a persisted setting without an explicit
/// inventory entry is refused with its supplied name, making an omitted group
/// visible to both callers and tests.
pub fn saved_setting_group(setting: &str) -> Result<&'static str, String> {
    SAVED_SETTING_GROUPS
        .iter()
        .find(|entry| entry.setting == setting)
        .map(|entry| entry.group)
        .ok_or_else(|| format!("OSL saved setting '{setting}' has no Settings group"))
}

#[cfg(test)]
mod tests {
    use super::{saved_setting_group, SAVED_SETTING_GROUPS};

    #[test]
    fn direct_read_names_a_group_for_every_saved_setting() {
        let direct_reads = SAVED_SETTING_GROUPS
            .iter()
            .map(|entry| {
                let group = saved_setting_group(entry.setting).expect("saved setting has a group");
                assert_eq!(group, entry.group);
                format!("{}={group}", entry.setting)
            })
            .collect::<Vec<_>>();

        println!(
            "TASK0860 direct_read_count={} {}",
            direct_reads.len(),
            direct_reads.join(" | ")
        );
    }

    #[test]
    fn setting_without_a_group_is_refused_by_name() {
        let missing = "unassigned-setting-0860";
        let error = saved_setting_group(missing).expect_err("unassigned setting must be refused");
        assert!(error.contains(missing));
        println!("TASK0860 refusal={error}");
    }
}
