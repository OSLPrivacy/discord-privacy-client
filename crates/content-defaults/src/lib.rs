//! TASK 4656 — Settings defaults for post visibility, story visibility and
//! story lifetime, and the production post/story consumers that read them.
//!
//! Visibility options and their per-option rules are the frozen authority
//! from TASK 4651 (`/home/liamw/osl-plan/OSL-AUDITS/evidence/4651.md`):
//! `vis.everyone`, `vis.chosen`, `vis.except`, `vis.onlyme`. Posts keep the
//! default-only model (`resolve_post_audience` takes no visibility
//! parameter at all — there is no per-post override to pass). Stories may
//! override the default through a signed per-story SEND TO choice
//! (TASK 4656's remit); when no override is presented the story falls back
//! to the same settings default a post would use.

use crypto::ed25519::{self, PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub type PersonId = String;
pub type DeviceId = String;

/// The four canonical visibility options, frozen by TASK 4651.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VisibilityOption {
    Everyone,
    Chosen,
    Except,
    OnlyMe,
}

impl VisibilityOption {
    pub const ALL: [VisibilityOption; 4] = [
        VisibilityOption::Everyone,
        VisibilityOption::Chosen,
        VisibilityOption::Except,
        VisibilityOption::OnlyMe,
    ];

    pub fn stable_id(self) -> &'static str {
        match self {
            VisibilityOption::Everyone => "vis.everyone",
            VisibilityOption::Chosen => "vis.chosen",
            VisibilityOption::Except => "vis.except",
            VisibilityOption::OnlyMe => "vis.onlyme",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VisibilityOption::Everyone => "Everyone",
            VisibilityOption::Chosen => "Chosen people",
            VisibilityOption::Except => "Everyone except",
            VisibilityOption::OnlyMe => "Only me",
        }
    }

    pub fn from_stable_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|option| option.stable_id() == id)
    }
}

/// A resolved visibility choice: an option plus the explicit person set the
/// `Chosen`/`Except` options need. `Everyone`/`OnlyMe` ignore `set`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisibilityChoice {
    pub option: VisibilityOption,
    pub set: BTreeSet<PersonId>,
}

impl VisibilityChoice {
    pub fn everyone() -> Self {
        Self {
            option: VisibilityOption::Everyone,
            set: BTreeSet::new(),
        }
    }

    pub fn only_me() -> Self {
        Self {
            option: VisibilityOption::OnlyMe,
            set: BTreeSet::new(),
        }
    }

    pub fn chosen(people: impl IntoIterator<Item = PersonId>) -> Self {
        Self {
            option: VisibilityOption::Chosen,
            set: people.into_iter().collect(),
        }
    }

    pub fn except(people: impl IntoIterator<Item = PersonId>) -> Self {
        Self {
            option: VisibilityOption::Except,
            set: people.into_iter().collect(),
        }
    }

    /// Applies the TASK 4651 per-option rule against a snapshot of the
    /// world at post/story time. `universe` is every known person (friends
    /// and non-friends); `friends_at_time` is the friend list at that same
    /// moment; `author_devices` is the author's own device set (only
    /// `OnlyMe` keys them).
    pub fn resolve(
        &self,
        universe: &BTreeSet<PersonId>,
        friends_at_time: &BTreeSet<PersonId>,
        author_devices: &BTreeSet<DeviceId>,
    ) -> Audience {
        match self.option {
            VisibilityOption::Everyone => {
                let keyed = friends_at_time.clone();
                let unkeyed = universe.difference(&keyed).cloned().collect();
                Audience { keyed, unkeyed }
            }
            VisibilityOption::Chosen => {
                let keyed = self.set.clone();
                let unkeyed = universe.difference(&keyed).cloned().collect();
                Audience { keyed, unkeyed }
            }
            VisibilityOption::Except => {
                let keyed: BTreeSet<_> = friends_at_time.difference(&self.set).cloned().collect();
                let unkeyed = universe.difference(&keyed).cloned().collect();
                Audience { keyed, unkeyed }
            }
            VisibilityOption::OnlyMe => Audience {
                keyed: author_devices.clone(),
                unkeyed: universe.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Audience {
    pub keyed: BTreeSet<PersonId>,
    pub unkeyed: BTreeSet<PersonId>,
}

/// Story lifetime options, frozen by this task's `do:` line: 1 hour, 24
/// hours, 72 hours and 7 days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoryLifetime {
    OneHour,
    TwentyFourHours,
    SeventyTwoHours,
    SevenDays,
}

impl StoryLifetime {
    pub const ALL: [StoryLifetime; 4] = [
        StoryLifetime::OneHour,
        StoryLifetime::TwentyFourHours,
        StoryLifetime::SeventyTwoHours,
        StoryLifetime::SevenDays,
    ];

    pub fn stable_id(self) -> &'static str {
        match self {
            StoryLifetime::OneHour => "life.1h",
            StoryLifetime::TwentyFourHours => "life.24h",
            StoryLifetime::SeventyTwoHours => "life.72h",
            StoryLifetime::SevenDays => "life.7d",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            StoryLifetime::OneHour => "1 hour",
            StoryLifetime::TwentyFourHours => "24 hours",
            StoryLifetime::SeventyTwoHours => "72 hours",
            StoryLifetime::SevenDays => "7 days",
        }
    }

    pub fn seconds(self) -> u64 {
        match self {
            StoryLifetime::OneHour => 3_600,
            StoryLifetime::TwentyFourHours => 86_400,
            StoryLifetime::SeventyTwoHours => 259_200,
            StoryLifetime::SevenDays => 604_800,
        }
    }

    pub fn from_stable_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|life| life.stable_id() == id)
    }
}

/// The three Settings defaults this task adds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    pub post_visibility: VisibilityChoice,
    pub story_visibility: VisibilityChoice,
    pub story_lifetime: StoryLifetime,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            post_visibility: VisibilityChoice::everyone(),
            story_visibility: VisibilityChoice::everyone(),
            story_lifetime: StoryLifetime::TwentyFourHours,
        }
    }
}

/// Persists the three defaults to disk so they survive a restart. `open`
/// reads whatever is already on disk (or falls back to `Defaults::default`
/// on first run); `set_defaults` writes synchronously. A caller that wants
/// to prove "survives restart" drops one `SettingsStore` and `open`s a new
/// one against the same path.
pub struct SettingsStore {
    path: PathBuf,
    defaults: Defaults,
}

impl SettingsStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let defaults = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("corrupt settings defaults at {path:?}: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Defaults::default(),
            Err(error) => return Err(format!("could not read {path:?}: {error}")),
        };
        Ok(Self { path, defaults })
    }

    pub fn defaults(&self) -> &Defaults {
        &self.defaults
    }

    pub fn set_defaults(&mut self, defaults: Defaults) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&defaults)
            .map_err(|error| format!("could not serialize defaults: {error}"))?;
        std::fs::write(&self.path, bytes)
            .map_err(|error| format!("could not write {:?}: {error}", self.path))?;
        self.defaults = defaults;
        Ok(())
    }
}

/// A per-story SEND TO override: an author-signed visibility choice that,
/// when present, is the *only* audience a story uses — the settings
/// default is not consulted at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedSendTo {
    pub choice: VisibilityChoice,
    pub author_public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl SignedSendTo {
    pub fn sign(choice: VisibilityChoice, secret: &SecretKey) -> Self {
        let public = ed25519::derive_public(secret);
        let signature = ed25519::sign(secret, &Self::canonical_bytes(&choice));
        Self {
            choice,
            author_public_key: public.as_bytes().to_vec(),
            signature: signature.as_bytes().to_vec(),
        }
    }

    pub fn verify(&self) -> bool {
        let (Ok(public_bytes), Ok(signature_bytes)) = (
            <[u8; 32]>::try_from(self.author_public_key.as_slice()),
            <[u8; 64]>::try_from(self.signature.as_slice()),
        ) else {
            return false;
        };
        let public = PublicKey::from_bytes(public_bytes);
        let signature = Signature::from_bytes(signature_bytes);
        ed25519::verify(&public, &Self::canonical_bytes(&self.choice), &signature)
            .unwrap_or(false)
    }

    fn canonical_bytes(choice: &VisibilityChoice) -> Vec<u8> {
        serde_json::to_vec(choice).expect("VisibilityChoice always serializes")
    }
}

/// The shipping post consumer. Deliberately takes **no** visibility
/// parameter: posts expose zero per-post visibility controls, so the only
/// input that can determine the audience is the Settings default.
pub fn resolve_post_audience(
    defaults: &Defaults,
    universe: &BTreeSet<PersonId>,
    friends_at_time: &BTreeSet<PersonId>,
    author_devices: &BTreeSet<DeviceId>,
) -> Audience {
    defaults
        .post_visibility
        .resolve(universe, friends_at_time, author_devices)
}

/// The shipping story consumer. With no `override_send_to`, the story uses
/// the Settings default exactly as a post would. With a `SignedSendTo`
/// override, the default is ignored entirely and the story uses only the
/// signed per-story audience; an override whose signature does not verify
/// is refused outright rather than silently falling back to the default.
pub fn resolve_story_audience(
    defaults: &Defaults,
    universe: &BTreeSet<PersonId>,
    friends_at_time: &BTreeSet<PersonId>,
    author_devices: &BTreeSet<DeviceId>,
    override_send_to: Option<&SignedSendTo>,
) -> Result<Audience, String> {
    match override_send_to {
        Some(send_to) => {
            if !send_to.verify() {
                return Err("story SEND TO override does not verify".to_owned());
            }
            Ok(send_to
                .choice
                .resolve(universe, friends_at_time, author_devices))
        }
        None => Ok(defaults
            .story_visibility
            .resolve(universe, friends_at_time, author_devices)),
    }
}

/// The shipping story-lifetime consumer.
pub fn resolve_story_lifetime_seconds(defaults: &Defaults) -> u64 {
    defaults.story_lifetime.seconds()
}

pub fn settings_path_for_restart_test(dir: &Path) -> PathBuf {
    dir.join("content-defaults-settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn universe() -> BTreeSet<PersonId> {
        ["alice", "bob", "carol", "dave"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    fn friends() -> BTreeSet<PersonId> {
        ["alice", "bob", "carol"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    fn devices() -> BTreeSet<DeviceId> {
        ["author-phone"].into_iter().map(String::from).collect()
    }

    #[test]
    fn each_visibility_option_has_an_allowed_and_a_refused_person() {
        let choices = [
            VisibilityChoice::everyone(),
            VisibilityChoice::chosen(["carol".to_owned()]),
            VisibilityChoice::except(["bob".to_owned()]),
            VisibilityChoice::only_me(),
        ];
        for choice in choices {
            let audience = choice.resolve(&universe(), &friends(), &devices());
            assert!(
                !audience.keyed.is_empty(),
                "{:?} produced no allowed person",
                choice.option
            );
            assert!(
                !audience.unkeyed.is_empty(),
                "{:?} produced no refused person",
                choice.option
            );
        }
    }

    #[test]
    fn story_override_ignores_the_default_entirely() {
        let defaults = Defaults {
            post_visibility: VisibilityChoice::everyone(),
            story_visibility: VisibilityChoice::everyone(),
            story_lifetime: StoryLifetime::OneHour,
        };
        let (secret, _public) = ed25519::generate_keypair();
        let send_to = SignedSendTo::sign(VisibilityChoice::chosen(["carol".to_owned()]), &secret);
        let audience = resolve_story_audience(
            &defaults,
            &universe(),
            &friends(),
            &devices(),
            Some(&send_to),
        )
        .expect("valid signature verifies");
        assert_eq!(audience.keyed, ["carol".to_owned()].into());
        assert!(!audience.keyed.contains("alice"));
    }

    #[test]
    fn tampered_override_is_refused_not_defaulted() {
        let defaults = Defaults::default();
        let (secret, _public) = ed25519::generate_keypair();
        let mut send_to =
            SignedSendTo::sign(VisibilityChoice::chosen(["carol".to_owned()]), &secret);
        send_to.choice = VisibilityChoice::everyone();
        let result = resolve_story_audience(
            &defaults,
            &universe(),
            &friends(),
            &devices(),
            Some(&send_to),
        );
        assert!(result.is_err());
    }

    #[test]
    fn defaults_survive_a_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_path_for_restart_test(dir.path());
        let mut store = SettingsStore::open(&path).unwrap();
        let defaults = Defaults {
            post_visibility: VisibilityChoice::except(["bob".to_owned()]),
            story_visibility: VisibilityChoice::only_me(),
            story_lifetime: StoryLifetime::SevenDays,
        };
        store.set_defaults(defaults.clone()).unwrap();
        drop(store);

        let reopened = SettingsStore::open(&path).unwrap();
        assert_eq!(reopened.defaults(), &defaults);
    }
}
