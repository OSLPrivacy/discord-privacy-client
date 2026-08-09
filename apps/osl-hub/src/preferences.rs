use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::claim_state::{self, Surface};
use crate::models::{
    default_home_tile_order, is_default_home_tile, HomeTileArrangementInput,
    HomeTileArrangementRead, HomeTileCapabilityFacts, HomeTileData, OnboardingPreferences,
    DEFAULT_HOME_TILE_ORDER,
};

const PREVIEW_STATE_VERSION: u8 = 1;
const MAX_PREFERENCES_BYTES: u64 = 16 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PreferencesDocument {
    version: u8,
    onboarding: OnboardingPreferences,
    #[serde(default)]
    home_tiles_by_user: BTreeMap<String, StoredHomeTileArrangement>,
}

impl Default for PreferencesDocument {
    fn default() -> Self {
        Self {
            version: PREVIEW_STATE_VERSION,
            onboarding: OnboardingPreferences::default(),
            home_tiles_by_user: BTreeMap::new(),
        }
    }
}

pub struct PreviewState {
    path: PathBuf,
    onboarding: Mutex<OnboardingPreferences>,
    home_tiles_by_user: Mutex<BTreeMap<String, StoredHomeTileArrangement>>,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredHomeTileArrangement {
    order: Vec<String>,
    hidden: Vec<String>,
}

impl PreviewState {
    pub fn load(path: PathBuf) -> Self {
        let document = read_preferences(&path).unwrap_or_default();
        let onboarding = document.onboarding.fail_closed();
        let home_tiles_by_user = document
            .home_tiles_by_user
            .into_iter()
            .filter_map(|(owner, arrangement)| {
                validate_owner_key(&owner)
                    .ok()
                    .map(|owner| (owner, sanitize_arrangement(arrangement)))
            })
            .collect();

        Self {
            path,
            onboarding: Mutex::new(onboarding),
            home_tiles_by_user: Mutex::new(home_tiles_by_user),
        }
    }

    pub fn get(&self) -> Result<OnboardingPreferences, String> {
        self.onboarding
            .lock()
            .map(|preferences| preferences.clone())
            .map_err(|_| "preview preferences lock is unavailable".to_owned())
    }

    pub fn save(
        &self,
        preferences: OnboardingPreferences,
    ) -> Result<OnboardingPreferences, String> {
        let preferences = preferences.fail_closed();
        let home_tiles_by_user = self
            .home_tiles_by_user
            .lock()
            .map_err(|_| "home tile preferences lock is unavailable".to_owned())?
            .clone();
        write_preferences(&self.path, &preferences, &home_tiles_by_user)
            .map_err(|error| format!("could not save preview preferences: {error}"))?;

        let mut current = self
            .onboarding
            .lock()
            .map_err(|_| "preview preferences lock is unavailable".to_owned())?;
        *current = preferences.clone();
        Ok(preferences)
    }

    pub fn reset(&self) -> Result<OnboardingPreferences, String> {
        self.save(OnboardingPreferences::default())
    }

    pub fn get_home_tile_arrangement(
        &self,
        owner_user_id: &str,
    ) -> Result<HomeTileArrangementRead, String> {
        let owner_user_id = validate_owner_key(owner_user_id)?;
        let home_tiles_by_user = self
            .home_tiles_by_user
            .lock()
            .map_err(|_| "home tile preferences lock is unavailable".to_owned())?;
        Ok(read_arrangement(
            home_tiles_by_user.get(&owner_user_id).cloned(),
        ))
    }

    pub fn save_home_tile_arrangement(
        &self,
        owner_user_id: &str,
        arrangement: HomeTileArrangementInput,
    ) -> Result<HomeTileArrangementRead, String> {
        let owner_user_id = validate_owner_key(owner_user_id)?;
        let stored = sanitize_arrangement(StoredHomeTileArrangement {
            order: arrangement.order,
            hidden: arrangement.hidden,
        });

        let onboarding = self
            .onboarding
            .lock()
            .map_err(|_| "preview preferences lock is unavailable".to_owned())?
            .clone();
        let mut home_tiles_by_user = self
            .home_tiles_by_user
            .lock()
            .map_err(|_| "home tile preferences lock is unavailable".to_owned())?
            .clone();
        home_tiles_by_user.insert(owner_user_id.clone(), stored.clone());

        write_preferences(&self.path, &onboarding, &home_tiles_by_user)
            .map_err(|error| format!("could not save home tile preferences: {error}"))?;

        let mut current = self
            .home_tiles_by_user
            .lock()
            .map_err(|_| "home tile preferences lock is unavailable".to_owned())?;
        *current = home_tiles_by_user;
        Ok(read_arrangement(Some(stored)))
    }
}

fn read_preferences(path: &Path) -> Option<PreferencesDocument> {
    let bytes = crate::atomic_file::read_recoverable_bounded(
        path,
        MAX_PREFERENCES_BYTES,
        "preview preferences",
    )
    .ok()
    .flatten()?;
    let document = serde_json::from_slice::<PreferencesDocument>(&bytes).ok()?;
    (document.version == PREVIEW_STATE_VERSION).then_some(document)
}

fn write_preferences(
    path: &Path,
    preferences: &OnboardingPreferences,
    home_tiles_by_user: &BTreeMap<String, StoredHomeTileArrangement>,
) -> Result<(), String> {
    let document = PreferencesDocument {
        version: PREVIEW_STATE_VERSION,
        onboarding: preferences.clone(),
        home_tiles_by_user: home_tiles_by_user.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&document)
        .map_err(|_| "preferences could not be encoded".to_owned())?;
    if bytes.len() as u64 > MAX_PREFERENCES_BYTES {
        return Err("preview preferences exceed the size limit".to_owned());
    }
    crate::atomic_file::write_recoverable(path, &bytes, "preview preferences")
}

fn validate_owner_key(owner_user_id: &str) -> Result<String, String> {
    let owner_user_id = owner_user_id.trim();
    if owner_user_id.is_empty()
        || owner_user_id.len() > 128
        || !owner_user_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'@'))
    {
        return Err("home tile owner identity is invalid".to_owned());
    }
    Ok(owner_user_id.to_owned())
}

fn sanitize_arrangement(arrangement: StoredHomeTileArrangement) -> StoredHomeTileArrangement {
    let mut seen = HashSet::<String>::new();
    let mut order = Vec::<String>::with_capacity(DEFAULT_HOME_TILE_ORDER.len());
    for tile in arrangement.order {
        if is_default_home_tile(&tile) && seen.insert(tile.clone()) {
            order.push(tile);
        }
    }
    for tile in DEFAULT_HOME_TILE_ORDER {
        if seen.insert((*tile).to_owned()) {
            order.push((*tile).to_owned());
        }
    }

    let hidden_set = arrangement
        .hidden
        .into_iter()
        .filter(|tile| is_default_home_tile(tile))
        .collect::<HashSet<_>>();
    let hidden = order
        .iter()
        .filter(|tile| hidden_set.contains(*tile))
        .cloned()
        .collect();

    StoredHomeTileArrangement { order, hidden }
}

fn read_arrangement(arrangement: Option<StoredHomeTileArrangement>) -> HomeTileArrangementRead {
    let arrangement =
        arrangement
            .map(sanitize_arrangement)
            .unwrap_or_else(|| StoredHomeTileArrangement {
                order: default_home_tile_order(),
                hidden: Vec::new(),
            });
    let hidden = arrangement.hidden.into_iter().collect::<HashSet<_>>();
    let mut visible_tiles = Vec::new();
    let mut hidden_tiles = Vec::new();
    for tile in arrangement.order {
        if hidden.contains(&tile) {
            hidden_tiles.push(tile);
        } else {
            visible_tiles.push(tile);
        }
    }
    let visible_tile_data = tile_data(&visible_tiles);
    let hidden_tile_data = tile_data(&hidden_tiles);
    HomeTileArrangementRead {
        visible_tiles,
        hidden_tiles,
        visible_tile_data,
        hidden_tile_data,
    }
}

fn tile_data(tiles: &[String]) -> Vec<HomeTileData> {
    tiles
        .iter()
        .map(|tile| HomeTileData {
            id: tile.clone(),
            capability: capability_facts_for_home_tile(tile),
        })
        .collect()
}

fn capability_facts_for_home_tile(tile: &str) -> Option<HomeTileCapabilityFacts> {
    let surface = match tile {
        "discord" => Surface::Discord,
        "telegram" => Surface::Telegram,
        "signal" => Surface::Signal,
        "whatsapp" => Surface::Whatsapp,
        "gmail" => Surface::Gmail,
        "outlook" => Surface::OutlookWeb,
        "proton" => Surface::Proton,
        "yahoo" => Surface::Yahoo,
        "aol" => Surface::Aol,
        "gmx" => Surface::Gmx,
        "maildotcom" => Surface::MailDotCom,
        "icloud" => Surface::ICloud,
        "tuta" => Surface::Tuta,
        "osl-chats" => Surface::OslChats,
        "osl-mail" => Surface::OslMail,
        _ => return None,
    };
    let row = claim_state::claim_of(surface);
    let public_claim = claim_state::claim_for(row);
    Some(HomeTileCapabilityFacts {
        surface: surface.ruling_slug(),
        public_claim: public_claim.slug(),
        carrier_evidence: row.carrier.slug(),
        delivery_evidence: row.delivery.slug(),
        claim_blockers: row.blockers.iter().map(|blocker| blocker.slug()).collect(),
        matrix_position: row.matrix.slug(),
        first_party: claim_state::is_first_party(surface),
        capability_claim: public_claim.is_capability_claim(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ForwardSecrecyMode, PlacementMode, SendMode};
    use serde_json::Value;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_file() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("osl-hub-preview-{}-{nonce}", std::process::id()))
            .join("preferences.json")
    }

    #[test]
    fn preferences_round_trip_without_platform_data() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let expected = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::Enter,
            placement_mode: PlacementMode::Compatibility,
            cover_insertion: Some(crate::models::CoverInsertion::InsertOnSend),
            show_plaintext_preview: false,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: false,
            forward_secrecy_mode: ForwardSecrecyMode::default(),
        };

        state.save(expected.clone()).expect("save preferences");
        assert_eq!(PreviewState::load(path.clone()).get().unwrap(), expected);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_or_oversized_files_fail_closed_to_defaults() {
        let path = temporary_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; MAX_PREFERENCES_BYTES as usize + 1]).unwrap();

        assert_eq!(
            PreviewState::load(path.clone()).get().unwrap(),
            OnboardingPreferences::default()
        );
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn oversized_recovery_backup_is_never_restored_or_read() {
        let path = temporary_file();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let backup = path.with_extension("bak");
        fs::write(&backup, vec![b'x'; MAX_PREFERENCES_BYTES as usize + 1]).unwrap();

        assert_eq!(
            PreviewState::load(path.clone()).get().unwrap(),
            OnboardingPreferences::default()
        );
        assert!(!path.exists());
        assert!(backup.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn persisted_send_trigger_discards_retired_risk_acknowledgement() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let preferences = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::EnterX2,
            placement_mode: PlacementMode::Compatibility,
            cover_insertion: Some(crate::models::CoverInsertion::TypeNaturally),
            show_plaintext_preview: true,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: true,
            forward_secrecy_mode: ForwardSecrecyMode::default(),
        };

        let saved = state.save(preferences).expect("save preferences");
        assert!(saved.onboarding_complete);
        assert!(!saved.acknowledge_experimental_send_risk);
        assert!(
            PreviewState::load(path.clone())
                .get()
                .unwrap()
                .onboarding_complete
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_primary_recovers_last_committed_preferences() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let expected = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::Enter,
            placement_mode: PlacementMode::Atomic,
            cover_insertion: None,
            show_plaintext_preview: false,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: false,
            forward_secrecy_mode: ForwardSecrecyMode::default(),
        };
        state.save(expected.clone()).unwrap();
        fs::rename(&path, path.with_extension("bak")).unwrap();

        let recovered = PreviewState::load(path.clone());
        assert_eq!(recovered.get().unwrap(), expected);
        assert!(path.exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn task_0812_per_user_tile_arrangement_direct_reads_return_ordered_visible_and_hidden_lists() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());

        let default = state
            .get_home_tile_arrangement("user-a")
            .expect("default arrangement");
        println!(
            "TASK0812 default_visible_tiles={}",
            default.visible_tiles.join(",")
        );
        println!(
            "TASK0812 default_hidden_tiles={}",
            if default.hidden_tiles.is_empty() {
                "(none)".to_owned()
            } else {
                default.hidden_tiles.join(",")
            }
        );
        assert_eq!(default.visible_tiles, default_home_tile_order());
        assert!(default.hidden_tiles.is_empty());

        let saved = state
            .save_home_tile_arrangement(
                "user-a",
                HomeTileArrangementInput {
                    order: vec![
                        "scrub".to_owned(),
                        "discord".to_owned(),
                        "unknown-tile".to_owned(),
                        "gmail".to_owned(),
                        "discord".to_owned(),
                    ],
                    hidden: vec![
                        "discord".to_owned(),
                        "unknown-tile".to_owned(),
                        "osl-mail".to_owned(),
                        "discord".to_owned(),
                    ],
                },
            )
            .expect("save user-a arrangement");
        println!(
            "TASK0812 user_a_saved_visible_tiles={}",
            saved.visible_tiles.join(",")
        );
        println!(
            "TASK0812 user_a_saved_hidden_tiles={}",
            saved.hidden_tiles.join(",")
        );

        let user_b = state
            .save_home_tile_arrangement(
                "user-b",
                HomeTileArrangementInput {
                    order: vec!["telegram".to_owned(), "discord".to_owned()],
                    hidden: vec!["telegram".to_owned()],
                },
            )
            .expect("save user-b arrangement");
        println!(
            "TASK0812 user_b_visible_tiles={}",
            user_b.visible_tiles.join(",")
        );
        println!(
            "TASK0812 user_b_hidden_tiles={}",
            user_b.hidden_tiles.join(",")
        );

        let direct = state
            .get_home_tile_arrangement("user-a")
            .expect("direct read user-a");
        println!(
            "TASK0812 user_a_direct_visible_tiles={}",
            direct.visible_tiles.join(",")
        );
        println!(
            "TASK0812 user_a_direct_hidden_tiles={}",
            direct.hidden_tiles.join(",")
        );

        let reloaded = PreviewState::load(path.clone())
            .get_home_tile_arrangement("user-a")
            .expect("reload user-a arrangement");
        println!(
            "TASK0812 user_a_reloaded_visible_tiles={}",
            reloaded.visible_tiles.join(",")
        );
        println!(
            "TASK0812 user_a_reloaded_hidden_tiles={}",
            reloaded.hidden_tiles.join(",")
        );

        assert_eq!(
            direct.visible_tiles,
            vec![
                "scrub",
                "gmail",
                "telegram",
                "signal",
                "whatsapp",
                "outlook",
                "proton",
                "yahoo",
                "aol",
                "gmx",
                "maildotcom",
                "icloud",
                "tuta",
                "osl-chats",
                "osl-notes"
            ]
        );
        assert_eq!(direct.hidden_tiles, vec!["discord", "osl-mail"]);
        assert_eq!(reloaded, direct);
        assert_eq!(user_b.hidden_tiles, vec!["telegram"]);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn task_0801_direct_tile_data_includes_capability_facts_and_saves_no_label_text() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());

        let direct = state
            .save_home_tile_arrangement(
                "user-a",
                HomeTileArrangementInput {
                    order: vec![
                        "telegram".to_owned(),
                        "discord".to_owned(),
                        "outlook".to_owned(),
                        "scrub".to_owned(),
                    ],
                    hidden: vec!["discord".to_owned()],
                },
            )
            .expect("save arrangement");

        let direct_tile_count = direct.visible_tile_data.len() + direct.hidden_tile_data.len();
        let direct_fact_count = direct
            .visible_tile_data
            .iter()
            .chain(direct.hidden_tile_data.iter())
            .filter(|tile| tile.capability.is_some())
            .count();
        let direct_missing_fact_ids = direct
            .visible_tile_data
            .iter()
            .chain(direct.hidden_tile_data.iter())
            .filter(|tile| tile.capability.is_none())
            .map(|tile| tile.id.as_str())
            .collect::<Vec<_>>();
        let telegram = direct
            .visible_tile_data
            .iter()
            .find(|tile| tile.id == "telegram")
            .and_then(|tile| tile.capability.as_ref())
            .expect("telegram capability facts");
        let discord = direct
            .hidden_tile_data
            .iter()
            .find(|tile| tile.id == "discord")
            .and_then(|tile| tile.capability.as_ref())
            .expect("discord capability facts");
        let outlook = direct
            .visible_tile_data
            .iter()
            .find(|tile| tile.id == "outlook")
            .and_then(|tile| tile.capability.as_ref())
            .expect("outlook capability facts");
        let saved_json = fs::read_to_string(&path).expect("saved preferences file");
        let saved: Value = serde_json::from_str(&saved_json).expect("saved preferences json");
        let saved_label_text_values = count_saved_label_text_values(&saved);
        let saved_label_text_fields = count_saved_label_text_fields(&saved);
        let saved_capability_fact_fields = count_saved_capability_fact_fields(&saved);

        println!("TASK0801 direct_tile_data_count={direct_tile_count}");
        println!("TASK0801 direct_capability_fact_count={direct_fact_count}");
        println!(
            "TASK0801 direct_missing_capability_fact_ids={}",
            if direct_missing_fact_ids.is_empty() {
                "(none)".to_owned()
            } else {
                direct_missing_fact_ids.join(",")
            }
        );
        println!(
            "TASK0801 direct_telegram_capability_facts={}/{}/{}/{}",
            telegram.surface,
            telegram.public_claim,
            telegram.carrier_evidence,
            telegram.delivery_evidence
        );
        println!(
            "TASK0801 direct_discord_blockers={}",
            discord.claim_blockers.join(",")
        );
        println!("TASK0801 direct_outlook_surface={}", outlook.surface);
        println!("TASK0801 saved_label_text_values={saved_label_text_values}");
        println!("TASK0801 saved_label_text_fields={saved_label_text_fields}");
        println!("TASK0801 saved_capability_fact_fields={saved_capability_fact_fields}");

        assert_eq!(direct_tile_count, DEFAULT_HOME_TILE_ORDER.len());
        assert_eq!(direct_fact_count, DEFAULT_HOME_TILE_ORDER.len() - 2);
        assert_eq!(direct_missing_fact_ids, vec!["scrub", "osl-notes"]);
        assert_eq!(
            (
                telegram.surface,
                telegram.public_claim,
                telegram.carrier_evidence,
                telegram.delivery_evidence,
            ),
            (
                "telegram",
                "noClaim",
                "provenLiveWithReceipt",
                "neverProvenLive",
            )
        );
        assert_eq!(
            discord.claim_blockers,
            vec!["open-security-finding", "unknown-recheck-required"]
        );
        assert_eq!(outlook.surface, "outlook");
        assert_eq!(outlook.matrix_position, "noRow");
        assert_eq!(saved_label_text_values, 0);
        assert_eq!(saved_label_text_fields, 0);
        assert_eq!(saved_capability_fact_fields, 0);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    fn count_saved_label_text_values(value: &Value) -> usize {
        const FORBIDDEN_LABEL_TEXT: &[&str] = &[
            "Discord",
            "Telegram",
            "Signal",
            "WhatsApp",
            "Outlook",
            "Gmail",
            "Coming soon",
            "Experimental",
            "No claim",
            "Beta",
            "Available",
            "Externally blocked",
        ];
        match value {
            Value::String(text) => FORBIDDEN_LABEL_TEXT
                .iter()
                .filter(|label| text.contains(**label))
                .count(),
            Value::Array(items) => items.iter().map(count_saved_label_text_values).sum(),
            Value::Object(map) => map.values().map(count_saved_label_text_values).sum(),
            _ => 0,
        }
    }

    fn count_saved_label_text_fields(value: &Value) -> usize {
        match value {
            Value::Array(items) => items.iter().map(count_saved_label_text_fields).sum(),
            Value::Object(map) => {
                map.keys()
                    .filter(|key| key.contains("label") || key.contains("displayName"))
                    .count()
                    + map
                        .values()
                        .map(count_saved_label_text_fields)
                        .sum::<usize>()
            }
            _ => 0,
        }
    }

    fn count_saved_capability_fact_fields(value: &Value) -> usize {
        const FACT_FIELDS: &[&str] = &[
            "capability",
            "publicClaim",
            "carrierEvidence",
            "deliveryEvidence",
            "claimBlockers",
            "matrixPosition",
            "firstParty",
            "capabilityClaim",
        ];
        match value {
            Value::Array(items) => items.iter().map(count_saved_capability_fact_fields).sum(),
            Value::Object(map) => {
                map.keys()
                    .filter(|key| FACT_FIELDS.contains(&key.as_str()))
                    .count()
                    + map
                        .values()
                        .map(count_saved_capability_fact_fields)
                        .sum::<usize>()
            }
            _ => 0,
        }
    }
}
