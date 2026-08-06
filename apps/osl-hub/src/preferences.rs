use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::models::{
    default_home_tile_order, is_default_home_tile, HomeTileArrangementAction,
    HomeTileArrangementInput, HomeTileArrangementRead, OnboardingPreferences,
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

    pub fn apply_home_tile_arrangement_action(
        &self,
        owner_user_id: &str,
        action: HomeTileArrangementAction,
    ) -> Result<HomeTileArrangementRead, String> {
        let owner_user_id = validate_owner_key(owner_user_id)?;
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
        let current = home_tiles_by_user.get(&owner_user_id).cloned();
        let mut stored = current
            .map(sanitize_arrangement)
            .unwrap_or_else(default_arrangement);

        apply_arrangement_action(&mut stored, action)?;

        home_tiles_by_user.insert(owner_user_id.clone(), sanitize_arrangement(stored.clone()));
        write_preferences(&self.path, &onboarding, &home_tiles_by_user)
            .map_err(|error| format!("could not save home tile preferences: {error}"))?;

        let stored = home_tiles_by_user
            .get(&owner_user_id)
            .cloned()
            .ok_or_else(|| "home tile preferences were not saved".to_owned())?;
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

fn default_arrangement() -> StoredHomeTileArrangement {
    StoredHomeTileArrangement {
        order: default_home_tile_order(),
        hidden: Vec::new(),
    }
}

fn read_arrangement(arrangement: Option<StoredHomeTileArrangement>) -> HomeTileArrangementRead {
    let arrangement = arrangement
        .map(sanitize_arrangement)
        .unwrap_or_else(default_arrangement);
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
    HomeTileArrangementRead {
        visible_tiles,
        hidden_tiles,
    }
}

fn apply_arrangement_action(
    arrangement: &mut StoredHomeTileArrangement,
    action: HomeTileArrangementAction,
) -> Result<(), String> {
    match action {
        HomeTileArrangementAction::Move { tile_id, delta } => {
            validate_tile_id(&tile_id)?;
            if delta != -1 && delta != 1 {
                return Err("home tile move delta must be -1 or 1".to_owned());
            }
            let index = arrangement
                .order
                .iter()
                .position(|tile| tile == &tile_id)
                .ok_or_else(|| "home tile move target is unknown".to_owned())?;
            let target = index as isize + delta as isize;
            if target < 0 || target >= arrangement.order.len() as isize {
                return Err("home tile move target is outside the arrangement".to_owned());
            }
            arrangement.order.swap(index, target as usize);
        }
        HomeTileArrangementAction::Drag {
            tile_id,
            before_tile_id,
        } => {
            validate_tile_id(&tile_id)?;
            validate_tile_id(&before_tile_id)?;
            if tile_id == before_tile_id {
                return Ok(());
            }
            let source = arrangement
                .order
                .iter()
                .position(|tile| tile == &tile_id)
                .ok_or_else(|| "home tile drag source is unknown".to_owned())?;
            let tile = arrangement.order.remove(source);
            let target = arrangement
                .order
                .iter()
                .position(|candidate| candidate == &before_tile_id)
                .ok_or_else(|| "home tile drag target is unknown".to_owned())?;
            arrangement.order.insert(target, tile);
        }
        HomeTileArrangementAction::Hide { tile_id } => {
            validate_tile_id(&tile_id)?;
            if !arrangement.hidden.iter().any(|tile| tile == &tile_id) {
                arrangement.hidden.push(tile_id);
            }
        }
        HomeTileArrangementAction::Show { tile_id } => {
            validate_tile_id(&tile_id)?;
            arrangement.hidden.retain(|tile| tile != &tile_id);
        }
        HomeTileArrangementAction::Done => {}
    }
    *arrangement = sanitize_arrangement(arrangement.clone());
    Ok(())
}

fn validate_tile_id(tile_id: &str) -> Result<(), String> {
    if is_default_home_tile(tile_id) {
        Ok(())
    } else {
        Err("home tile action target is unknown".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ForwardSecrecyMode, HomeTileArrangementAction, PlacementMode, SendMode};
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
            send_mode: SendMode::SingleEnter,
            placement_mode: PlacementMode::Compatibility,
            show_plaintext_preview: false,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: true,
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
    fn persisted_experimental_mode_without_acknowledgement_reopens_setup() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let unsafe_preferences = OnboardingPreferences {
            onboarding_complete: true,
            send_mode: SendMode::DoubleEnter,
            placement_mode: PlacementMode::Compatibility,
            show_plaintext_preview: true,
            window_capture_enabled: true,
            acknowledge_experimental_send_risk: false,
            forward_secrecy_mode: ForwardSecrecyMode::default(),
        };

        let saved = state.save(unsafe_preferences).expect("save preferences");
        assert!(!saved.onboarding_complete);
        assert!(
            !PreviewState::load(path.clone())
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
            send_mode: SendMode::Manual,
            placement_mode: PlacementMode::Atomic,
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
    fn task_0813_direct_tile_actions_change_saved_arrangement_and_survive_restart() {
        let path = temporary_file();
        let state = PreviewState::load(path.clone());
        let owner = "user-0813";

        let moved = state
            .apply_home_tile_arrangement_action(
                owner,
                HomeTileArrangementAction::Move {
                    tile_id: "gmail".to_owned(),
                    delta: -1,
                },
            )
            .expect("move tile");
        println!(
            "TASK0813 move_action=move move_order_prefix={}",
            moved
                .visible_tiles
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        );

        let dragged = state
            .apply_home_tile_arrangement_action(
                owner,
                HomeTileArrangementAction::Drag {
                    tile_id: "scrub".to_owned(),
                    before_tile_id: "discord".to_owned(),
                },
            )
            .expect("drag tile");
        println!(
            "TASK0813 drag_action=drag drag_order_prefix={}",
            dragged
                .visible_tiles
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        );

        let hidden = state
            .apply_home_tile_arrangement_action(
                owner,
                HomeTileArrangementAction::Hide {
                    tile_id: "telegram".to_owned(),
                },
            )
            .expect("hide tile");
        println!(
            "TASK0813 hide_action=hide hidden_tiles={}",
            hidden.hidden_tiles.join(",")
        );

        let shown = state
            .apply_home_tile_arrangement_action(
                owner,
                HomeTileArrangementAction::Show {
                    tile_id: "telegram".to_owned(),
                },
            )
            .expect("show tile");
        println!(
            "TASK0813 show_action=show hidden_tiles={}",
            if shown.hidden_tiles.is_empty() {
                "(none)".to_owned()
            } else {
                shown.hidden_tiles.join(",")
            }
        );

        let final_hidden = state
            .apply_home_tile_arrangement_action(
                owner,
                HomeTileArrangementAction::Hide {
                    tile_id: "osl-mail".to_owned(),
                },
            )
            .expect("hide final tile");
        assert_eq!(final_hidden.hidden_tiles, vec!["osl-mail"]);

        let done = state
            .apply_home_tile_arrangement_action(owner, HomeTileArrangementAction::Done)
            .expect("done tile arrangement");
        println!(
            "TASK0813 done_action=done final_visible_tiles={}",
            done.visible_tiles.join(",")
        );
        println!(
            "TASK0813 done_action=done final_hidden_tiles={}",
            done.hidden_tiles.join(",")
        );

        let restarted = PreviewState::load(path.clone())
            .get_home_tile_arrangement(owner)
            .expect("restart reads saved arrangement");
        println!(
            "TASK0813 restart_visible_tiles={}",
            restarted.visible_tiles.join(",")
        );
        println!(
            "TASK0813 restart_hidden_tiles={}",
            restarted.hidden_tiles.join(",")
        );

        let expected_visible = vec![
            "scrub",
            "discord",
            "telegram",
            "signal",
            "gmail",
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
            "osl-notes",
        ];
        assert_eq!(
            moved
                .visible_tiles
                .iter()
                .take(5)
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["discord", "telegram", "signal", "gmail", "whatsapp"]
        );
        assert_eq!(
            dragged
                .visible_tiles
                .iter()
                .take(6)
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["scrub", "discord", "telegram", "signal", "gmail", "whatsapp"]
        );
        assert_eq!(hidden.hidden_tiles, vec!["telegram"]);
        assert!(shown.hidden_tiles.is_empty());
        assert_eq!(done.visible_tiles, expected_visible);
        assert_eq!(done.hidden_tiles, vec!["osl-mail"]);
        assert_eq!(restarted, done);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
