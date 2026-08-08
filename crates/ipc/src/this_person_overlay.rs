//! TASK 5031 — the THIS PERSON block: "rename them, just for me" and
//! "give them a colour, just for me".
//!
//! Both are **local overlays**. They are stored in their own file, keyed by
//! person id, and are never written into the record OSL holds for the other
//! account. That record ([`crate::friend_request::StoredFriendRecord`]) is the
//! other person's own profile as they published it; a local rename must leave
//! it byte-for-byte unchanged, because the moment a rename edits the profile
//! record it stops being local and starts being a claim about who someone is.
//!
//! Two rules the rest of the module exists to keep:
//!
//! 1. **One resolved name, three places.** The sidebar row, the thread header
//!    and the notification all read [`resolve_this_person`]. A rename that
//!    reached only two of the three would be worse than no rename at all — the
//!    third place would still be showing a name the operator believes they have
//!    replaced.
//! 2. **The safety-number screen never substitutes.** A rename is a label the
//!    operator chose; the safety-number screen is where identity is compared out
//!    loud. [`safety_number_person_identity`] always carries the profile name
//!    and puts the local rename *beside* it, so no rename can disguise who the
//!    number belongs to.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The overlay file. Deliberately its own file, not a field on the friend
/// record: nothing that walks the profile records can pick these up by accident.
pub const THIS_PERSON_OVERLAY_FILE: &str = "this_person_overlays.json";

pub const THIS_PERSON_OVERLAY_SCHEMA_VERSION: u32 = 1;

pub const MAX_LOCAL_RENAME_BYTES: usize = 64;
pub const MAX_LOCAL_RENAME_CHARS: usize = 48;

/// Said plainly on the setting, because "just for me" is a promise about where
/// the value goes and the operator cannot read the storage layer.
pub const THIS_PERSON_LOCAL_ONLY_SENTENCE: &str =
    "This name and colour stay on this device. The other person is never told either, \
     and their profile name still shows on the safety-number screen.";

/// The colours offered for "give them a colour, just for me", and the pool the
/// derived colour is drawn from. Same eight values the Hub's picture fallback
/// uses, so an overlay colour and a fallback avatar cannot disagree about what
/// the palette is.
pub const THIS_PERSON_COLOUR_CHOICES: [&str; 8] = [
    "#3b82f6", "#14b8a6", "#f97316", "#a855f7", "#ef4444", "#22c55e", "#eab308", "#64748b",
];

/// The three places a person's name and colour are drawn.
pub const PLACE_SIDEBAR: &str = "sidebar";
pub const PLACE_THREAD: &str = "thread";
pub const PLACE_NOTIFICATION: &str = "notification";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThisPersonOverlayError {
    UnknownPerson,
    InvalidRename,
    InvalidColour,
    StorageUnavailable,
}

impl std::fmt::Display for ThisPersonOverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let words = match self {
            Self::UnknownPerson => "OSL: this person is unknown",
            Self::InvalidRename => "OSL: that local name cannot be used",
            Self::InvalidColour => "OSL: that colour is not one OSL offers",
            Self::StorageUnavailable => "OSL: local person overlays are unavailable",
        };
        f.write_str(words)
    }
}

impl std::error::Error for ThisPersonOverlayError {}

type Result<T> = std::result::Result<T, ThisPersonOverlayError>;

/// What the operator chose for one person, and nothing else. No profile field
/// is mirrored in here: a stale copy of someone's published name would be a
/// second source of truth for the one thing that must have exactly one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThisPersonOverlay {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_rename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_colour: Option<String>,
}

impl ThisPersonOverlay {
    pub fn is_empty(&self) -> bool {
        self.local_rename.is_none() && self.local_colour.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThisPersonOverlayFile {
    pub schema_version: u32,
    #[serde(default)]
    pub overlays: BTreeMap<String, ThisPersonOverlay>,
}

impl Default for ThisPersonOverlayFile {
    fn default() -> Self {
        Self {
            schema_version: THIS_PERSON_OVERLAY_SCHEMA_VERSION,
            overlays: BTreeMap::new(),
        }
    }
}

/// Trim, then refuse anything that could make one person's row impersonate
/// another's: control characters, zero-width joiners, bidi overrides, markup
/// angle brackets. Empty (or whitespace-only) is not an error — it is how the
/// operator clears the rename.
pub fn normalise_local_rename(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let forbidden = |character: char| {
        character.is_control()
            || matches!(character,
                '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{2060}'
                    | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
    };
    if value.len() > MAX_LOCAL_RENAME_BYTES
        || value.chars().count() > MAX_LOCAL_RENAME_CHARS
        || value.contains('<')
        || value.contains('>')
        || value.chars().any(forbidden)
    {
        return Err(ThisPersonOverlayError::InvalidRename);
    }
    Ok(Some(value.to_owned()))
}

/// Only a colour OSL itself offers. Free-form hex would let a saved value carry
/// data (and let one person's row be painted the background colour, which is
/// hiding, not colouring).
pub fn normalise_local_colour(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else { return Ok(None) };
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        return Ok(None);
    }
    if THIS_PERSON_COLOUR_CHOICES.contains(&value.as_str()) {
        Ok(Some(value))
    } else {
        Err(ThisPersonOverlayError::InvalidColour)
    }
}

/// The colour a person has when the operator has chosen none: stable per person
/// id, so a row does not change colour between launches.
pub fn derived_person_colour(person_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"OSL-THIS-PERSON-COLOUR-v1");
    hash.update(person_id.as_bytes());
    let digest = hash.finalize();
    let index = usize::from(digest[0]) % THIS_PERSON_COLOUR_CHOICES.len();
    THIS_PERSON_COLOUR_CHOICES[index].to_owned()
}

fn overlay_path(dir: &Path) -> std::path::PathBuf {
    dir.join(THIS_PERSON_OVERLAY_FILE)
}

pub fn load_this_person_overlays(dir: &Path) -> Result<ThisPersonOverlayFile> {
    let path = overlay_path(dir);
    if !path.exists() {
        return Ok(ThisPersonOverlayFile::default());
    }
    let blob = fs::read(&path).map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    let plain = crate::main_password::maybe_decrypt_in_dir(dir, &blob)
        .map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    let file: ThisPersonOverlayFile =
        serde_json::from_slice(&plain).map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    if file.schema_version != THIS_PERSON_OVERLAY_SCHEMA_VERSION {
        return Err(ThisPersonOverlayError::StorageUnavailable);
    }
    Ok(file)
}

pub fn save_this_person_overlays(dir: &Path, file: &ThisPersonOverlayFile) -> Result<()> {
    fs::create_dir_all(dir).map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    let body = serde_json::to_vec(file).map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    let sealed = crate::main_password::maybe_encrypt(&body)
        .map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    crate::recoverable_file::write_recoverable(&overlay_path(dir), &sealed)
        .map_err(|_| ThisPersonOverlayError::StorageUnavailable)?;
    Ok(())
}

pub fn read_this_person_overlay(dir: &Path, person_id: &str) -> Result<ThisPersonOverlay> {
    Ok(load_this_person_overlays(dir)?
        .overlays
        .get(person_id)
        .cloned()
        .unwrap_or_default())
}

fn write_one(dir: &Path, person_id: &str, overlay: ThisPersonOverlay) -> Result<ThisPersonOverlay> {
    if person_id.trim().is_empty() {
        return Err(ThisPersonOverlayError::UnknownPerson);
    }
    let mut file = load_this_person_overlays(dir)?;
    if overlay.is_empty() {
        file.overlays.remove(person_id);
    } else {
        file.overlays.insert(person_id.to_owned(), overlay.clone());
    }
    save_this_person_overlays(dir, &file)?;
    Ok(overlay)
}

/// Save (or, with `None`/blank, clear) the local rename. The colour is left
/// exactly as it was: they are two separate choices in the THIS PERSON block
/// and clearing one must not silently discard the other.
pub fn set_this_person_local_rename(
    dir: &Path,
    person_id: &str,
    rename: Option<&str>,
) -> Result<ThisPersonOverlay> {
    let rename = normalise_local_rename(rename)?;
    let mut overlay = read_this_person_overlay(dir, person_id)?;
    overlay.local_rename = rename;
    write_one(dir, person_id, overlay)
}

pub fn clear_this_person_local_rename(dir: &Path, person_id: &str) -> Result<ThisPersonOverlay> {
    set_this_person_local_rename(dir, person_id, None)
}

pub fn set_this_person_local_colour(
    dir: &Path,
    person_id: &str,
    colour: Option<&str>,
) -> Result<ThisPersonOverlay> {
    let colour = normalise_local_colour(colour)?;
    let mut overlay = read_this_person_overlay(dir, person_id)?;
    overlay.local_colour = colour;
    write_one(dir, person_id, overlay)
}

pub fn clear_this_person_local_colour(dir: &Path, person_id: &str) -> Result<ThisPersonOverlay> {
    set_this_person_local_colour(dir, person_id, None)
}

/// The single answer to "what is this person called and what colour are they,
/// here, on this device". Every rendered place goes through this.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedThisPerson {
    pub person_id: String,
    /// The other account's own profile name, always kept.
    pub profile_name: String,
    /// What this device shows: the local rename when there is one, otherwise
    /// the profile name unchanged.
    pub name: String,
    pub colour: String,
    pub locally_renamed: bool,
    pub locally_coloured: bool,
}

pub fn resolve_this_person(
    person_id: &str,
    profile_name: &str,
    overlay: &ThisPersonOverlay,
) -> ResolvedThisPerson {
    let locally_renamed = overlay.local_rename.is_some();
    let locally_coloured = overlay.local_colour.is_some();
    ResolvedThisPerson {
        person_id: person_id.to_owned(),
        profile_name: profile_name.to_owned(),
        name: overlay
            .local_rename
            .clone()
            .unwrap_or_else(|| profile_name.to_owned()),
        colour: overlay
            .local_colour
            .clone()
            .unwrap_or_else(|| derived_person_colour(person_id)),
        locally_renamed,
        locally_coloured,
    }
}

/// One drawn place: what the sidebar row, the thread header or the notification
/// puts on screen for this person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonPlaceLabel {
    pub place: &'static str,
    pub person_id: String,
    pub name: String,
    pub colour: String,
    pub locally_renamed: bool,
    pub locally_coloured: bool,
}

fn place_label(place: &'static str, resolved: &ResolvedThisPerson) -> PersonPlaceLabel {
    PersonPlaceLabel {
        place,
        person_id: resolved.person_id.clone(),
        name: resolved.name.clone(),
        colour: resolved.colour.clone(),
        locally_renamed: resolved.locally_renamed,
        locally_coloured: resolved.locally_coloured,
    }
}

pub fn sidebar_person_row(
    person_id: &str,
    profile_name: &str,
    overlay: &ThisPersonOverlay,
) -> PersonPlaceLabel {
    place_label(
        PLACE_SIDEBAR,
        &resolve_this_person(person_id, profile_name, overlay),
    )
}

pub fn thread_person_header(
    person_id: &str,
    profile_name: &str,
    overlay: &ThisPersonOverlay,
) -> PersonPlaceLabel {
    place_label(
        PLACE_THREAD,
        &resolve_this_person(person_id, profile_name, overlay),
    )
}

pub fn notification_person_alert(
    person_id: &str,
    profile_name: &str,
    overlay: &ThisPersonOverlay,
) -> PersonPlaceLabel {
    place_label(
        PLACE_NOTIFICATION,
        &resolve_this_person(person_id, profile_name, overlay),
    )
}

/// The three drawn places, in the order the checks report them.
pub fn this_person_places(
    person_id: &str,
    profile_name: &str,
    overlay: &ThisPersonOverlay,
) -> Vec<PersonPlaceLabel> {
    vec![
        sidebar_person_row(person_id, profile_name, overlay),
        thread_person_header(person_id, profile_name, overlay),
        notification_person_alert(person_id, profile_name, overlay),
    ]
}

/// What the safety-number screen prints above the number.
///
/// `profile_name` is not optional and is never replaced. `line` is the rendered
/// string, and when a local rename exists the profile name comes first and the
/// rename follows it, marked as the operator's own label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SafetyNumberPersonIdentity {
    pub person_id: String,
    pub profile_name: String,
    pub local_rename: Option<String>,
    pub line: String,
    /// Always false. Present so a check can assert on it rather than on the
    /// absence of a substitution.
    pub rename_replaces_profile_name: bool,
}

pub fn safety_number_person_identity(
    person_id: &str,
    profile_name: &str,
    overlay: &ThisPersonOverlay,
) -> SafetyNumberPersonIdentity {
    let line = match overlay.local_rename.as_deref() {
        Some(rename) => format!("{profile_name} (you call them {rename})"),
        None => profile_name.to_owned(),
    };
    SafetyNumberPersonIdentity {
        person_id: person_id.to_owned(),
        profile_name: profile_name.to_owned(),
        local_rename: overlay.local_rename.clone(),
        line,
        rename_replaces_profile_name: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_rename_clears_rather_than_saving_whitespace() {
        assert_eq!(
            normalise_local_rename(Some("  Bug  ")).unwrap().as_deref(),
            Some("Bug")
        );
        assert_eq!(normalise_local_rename(Some("   ")).unwrap(), None);
        assert_eq!(normalise_local_rename(None).unwrap(), None);
    }

    #[test]
    fn rename_refuses_disguise_characters_and_overlong_values() {
        assert!(normalise_local_rename(Some("Bug\u{202e}gnisiuqsid")).is_err());
        assert!(normalise_local_rename(Some("Bug\nOther")).is_err());
        assert!(normalise_local_rename(Some("<b>Bug</b>")).is_err());
        assert!(normalise_local_rename(Some(&"a".repeat(MAX_LOCAL_RENAME_BYTES + 1))).is_err());
    }

    #[test]
    fn colour_must_be_one_osl_offers() {
        assert_eq!(
            normalise_local_colour(Some("#A855F7")).unwrap().as_deref(),
            Some("#a855f7")
        );
        assert!(normalise_local_colour(Some("#000000")).is_err());
        assert_eq!(normalise_local_colour(Some("")).unwrap(), None);
    }

    #[test]
    fn derived_colour_is_stable_and_from_the_palette() {
        let first = derived_person_colour("person-1");
        assert_eq!(first, derived_person_colour("person-1"));
        assert!(THIS_PERSON_COLOUR_CHOICES.contains(&first.as_str()));
    }
}
