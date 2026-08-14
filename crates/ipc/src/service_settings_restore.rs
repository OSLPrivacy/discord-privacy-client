//! Compatibility loader for service-scoped settings snapshots.

use crate::allowed_places::{validate_allowed_place_record, AllowedPlaceRecord};
use serde::Deserialize;
use std::collections::BTreeSet;

pub const RESTORED_SETTINGS_SERVICE_SHORT_NAMES: [&str; 3] = ["discord", "email", "messenger"];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceScopedFriendRecord {
    pub service_id: String,
    pub person_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServiceScopedSavedSetting {
    pub service_id: String,
    pub setting_id: String,
    pub value: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreCutSettingsFile {
    pub schema_version: u32,
    pub friends: Vec<ServiceScopedFriendRecord>,
    pub allowed_places: Vec<AllowedPlaceRecord>,
    pub saved_settings: Vec<ServiceScopedSavedSetting>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct PreCutSettingsLoadReport {
    pub unknown_service_errors: Vec<String>,
    pub friends_found: usize,
    pub allowed_places_found: usize,
    pub saved_settings_found: usize,
}

pub fn load_pre_cut_settings_file(
    bytes: &[u8],
) -> Result<PreCutSettingsLoadReport, serde_json::Error> {
    load_pre_cut_settings_file_with_service_short_names(
        bytes,
        &RESTORED_SETTINGS_SERVICE_SHORT_NAMES,
    )
}

pub fn load_pre_cut_settings_file_with_service_short_names(
    bytes: &[u8],
    service_short_names: &[&str],
) -> Result<PreCutSettingsLoadReport, serde_json::Error> {
    let file: PreCutSettingsFile = serde_json::from_slice(bytes)?;
    let services = service_short_names.iter().copied().collect::<BTreeSet<_>>();
    let mut report = PreCutSettingsLoadReport::default();

    for friend in &file.friends {
        if services.contains(friend.service_id.as_str()) {
            if !friend.person_id.trim().is_empty() && !friend.display_name.trim().is_empty() {
                report.friends_found += 1;
            }
        } else {
            report.unknown_service_errors.push(format!(
                "unknown service in friend record: {}",
                friend.service_id
            ));
        }
    }

    for place in &file.allowed_places {
        if services.contains(place.app.as_str()) {
            if validate_allowed_place_record(place).is_ok() {
                report.allowed_places_found += 1;
            }
        } else {
            report
                .unknown_service_errors
                .push(format!("unknown service in allowed place: {}", place.app));
        }
    }

    for setting in &file.saved_settings {
        if services.contains(setting.service_id.as_str()) {
            if !setting.setting_id.trim().is_empty() && !setting.value.trim().is_empty() {
                report.saved_settings_found += 1;
            }
        } else {
            report.unknown_service_errors.push(format!(
                "unknown service in saved setting: {}",
                setting.service_id
            ));
        }
    }

    let _ = file.schema_version;
    Ok(report)
}
