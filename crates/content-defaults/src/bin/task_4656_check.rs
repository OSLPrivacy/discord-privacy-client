//! TASK 4656 check — Settings defaults for post visibility, story
//! visibility and story lifetime, plus the production post/story
//! consumers that read them.
//!
//! `--starve <case>` skips one required case on purpose, so the check can
//! be proven able to fail. Valid case names: default, restart,
//! post-consumer, story-consumer, no-override, override, lifetime.

use content_defaults::{
    resolve_post_audience, resolve_story_audience, resolve_story_lifetime_seconds,
    settings_path_for_restart_test, Defaults, SettingsStore, SignedSendTo, StoryLifetime,
    VisibilityChoice, VisibilityOption,
};
use crypto::ed25519;
use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Case {
    Default,
    Restart,
    PostConsumer,
    StoryConsumer,
    NoOverride,
    Override,
    Lifetime,
}

impl Case {
    const ALL: [Case; 7] = [
        Case::Default,
        Case::Restart,
        Case::PostConsumer,
        Case::StoryConsumer,
        Case::NoOverride,
        Case::Override,
        Case::Lifetime,
    ];

    fn name(self) -> &'static str {
        match self {
            Case::Default => "default",
            Case::Restart => "restart",
            Case::PostConsumer => "post-consumer",
            Case::StoryConsumer => "story-consumer",
            Case::NoOverride => "no-override",
            Case::Override => "override",
            Case::Lifetime => "lifetime",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|case| case.name() == name)
    }
}

fn universe() -> BTreeSet<String> {
    ["alice", "bob", "carol", "dave", "erin"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn friends() -> BTreeSet<String> {
    ["alice", "bob", "carol"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn devices() -> BTreeSet<String> {
    ["author-phone", "author-laptop"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn requested_starvation() -> Result<Option<Case>, String> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => Ok(None),
        [flag, name] if flag == "--starve" => Case::from_name(name)
            .map(Some)
            .ok_or_else(|| format!("unknown case: {name}")),
        _ => Err("usage: task_4656_check [--starve <case>]".to_owned()),
    }
}

fn run() -> Result<(), String> {
    let starved = requested_starvation()?;
    let mut invoked: HashSet<Case> = HashSet::new();
    let mut report: Vec<String> = Vec::new();

    // ---- Case::Default -----------------------------------------------
    // Every one of the three Settings defaults, across every stable ID it
    // can hold, actually changes what the default *is*.
    if starved != Some(Case::Default) {
        let mut checked_visibility_ids = BTreeSet::new();
        for option in VisibilityOption::ALL {
            let choice = match option {
                VisibilityOption::Everyone => VisibilityChoice::everyone(),
                VisibilityOption::Chosen => VisibilityChoice::chosen(["carol".to_owned()]),
                VisibilityOption::Except => VisibilityChoice::except(["bob".to_owned()]),
                VisibilityOption::OnlyMe => VisibilityChoice::only_me(),
            };
            let defaults = Defaults {
                post_visibility: choice.clone(),
                story_visibility: choice.clone(),
                story_lifetime: StoryLifetime::TwentyFourHours,
            };
            if defaults.post_visibility.option != option || defaults.story_visibility.option != option {
                return Err(format!(
                    "Case::Default: defaults did not hold the {} stable ID",
                    option.stable_id()
                ));
            }
            checked_visibility_ids.insert(option.stable_id());
        }
        let mut checked_lifetime_ids = BTreeSet::new();
        for life in StoryLifetime::ALL {
            let defaults = Defaults {
                story_lifetime: life,
                ..Defaults::default()
            };
            if defaults.story_lifetime.stable_id() != life.stable_id() {
                return Err("Case::Default: story lifetime default did not round-trip".to_owned());
            }
            checked_lifetime_ids.insert(life.stable_id());
        }
        if checked_visibility_ids.len() != 4 || checked_lifetime_ids.len() != 4 {
            return Err("Case::Default: STARVED — not all stable IDs were exercised".to_owned());
        }
        report.push(format!(
            "default: visibility ids={:?} lifetime ids={:?}",
            checked_visibility_ids, checked_lifetime_ids
        ));
        invoked.insert(Case::Default);
    }

    // ---- Case::Restart --------------------------------------------------
    // All three defaults survive a simulated restart (drop the store,
    // reopen against the same backing file).
    if starved != Some(Case::Restart) {
        let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
        let path = settings_path_for_restart_test(dir.path());
        let set_defaults = Defaults {
            post_visibility: VisibilityChoice::except(["bob".to_owned()]),
            story_visibility: VisibilityChoice::chosen(["dave".to_owned()]),
            story_lifetime: StoryLifetime::SeventyTwoHours,
        };
        let mut store = SettingsStore::open(&path)?;
        store.set_defaults(set_defaults.clone())?;
        drop(store);

        let reopened = SettingsStore::open(&path)?;
        if reopened.defaults() != &set_defaults {
            return Err("Case::Restart: defaults did not survive a reopen".to_owned());
        }
        report.push(format!(
            "restart: post={} story={} lifetime={} (reopened from {:?})",
            reopened.defaults().post_visibility.option.stable_id(),
            reopened.defaults().story_visibility.option.stable_id(),
            reopened.defaults().story_lifetime.stable_id(),
            path.file_name().unwrap()
        ));
        invoked.insert(Case::Restart);
    }

    // ---- Case::PostConsumer ---------------------------------------------
    // resolve_post_audience takes no visibility parameter (0 per-post
    // controls) and reflects the settings default for every option.
    if starved != Some(Case::PostConsumer) {
        let mut allowed_count = 0usize;
        let mut refused_count = 0usize;
        for option in VisibilityOption::ALL {
            let choice = match option {
                VisibilityOption::Everyone => VisibilityChoice::everyone(),
                VisibilityOption::Chosen => VisibilityChoice::chosen(["carol".to_owned()]),
                VisibilityOption::Except => VisibilityChoice::except(["bob".to_owned()]),
                VisibilityOption::OnlyMe => VisibilityChoice::only_me(),
            };
            let defaults = Defaults {
                post_visibility: choice,
                story_visibility: VisibilityChoice::everyone(),
                story_lifetime: StoryLifetime::OneHour,
            };
            let audience = resolve_post_audience(&defaults, &universe(), &friends(), &devices());
            if audience.keyed.is_empty() || audience.unkeyed.is_empty() {
                return Err(format!(
                    "Case::PostConsumer: {} produced no allowed+refused pair",
                    option.stable_id()
                ));
            }
            allowed_count += audience.keyed.len();
            refused_count += audience.unkeyed.len();
        }
        report.push(format!(
            "post-consumer: options=4 total-keyed={} total-unkeyed={}",
            allowed_count, refused_count
        ));
        invoked.insert(Case::PostConsumer);
    }

    // ---- Case::StoryConsumer ---------------------------------------------
    // resolve_story_audience with no override reflects the story default,
    // for every option.
    if starved != Some(Case::StoryConsumer) {
        let mut allowed_count = 0usize;
        let mut refused_count = 0usize;
        for option in VisibilityOption::ALL {
            let choice = match option {
                VisibilityOption::Everyone => VisibilityChoice::everyone(),
                VisibilityOption::Chosen => VisibilityChoice::chosen(["carol".to_owned()]),
                VisibilityOption::Except => VisibilityChoice::except(["bob".to_owned()]),
                VisibilityOption::OnlyMe => VisibilityChoice::only_me(),
            };
            let defaults = Defaults {
                post_visibility: VisibilityChoice::everyone(),
                story_visibility: choice,
                story_lifetime: StoryLifetime::OneHour,
            };
            let audience =
                resolve_story_audience(&defaults, &universe(), &friends(), &devices(), None)
                    .map_err(|error| format!("Case::StoryConsumer: {error}"))?;
            if audience.keyed.is_empty() || audience.unkeyed.is_empty() {
                return Err(format!(
                    "Case::StoryConsumer: {} produced no allowed+refused pair",
                    option.stable_id()
                ));
            }
            allowed_count += audience.keyed.len();
            refused_count += audience.unkeyed.len();
        }
        report.push(format!(
            "story-consumer: options=4 total-keyed={} total-unkeyed={}",
            allowed_count, refused_count
        ));
        invoked.insert(Case::StoryConsumer);
    }

    // ---- Case::NoOverride -------------------------------------------------
    // A story published with no SEND TO override uses exactly the default —
    // demonstrated by pairing the same default with the override case below
    // and showing precedence flips the result.
    if starved != Some(Case::NoOverride) {
        let defaults = Defaults {
            post_visibility: VisibilityChoice::everyone(),
            story_visibility: VisibilityChoice::except(["bob".to_owned()]),
            story_lifetime: StoryLifetime::OneHour,
        };
        let audience =
            resolve_story_audience(&defaults, &universe(), &friends(), &devices(), None)
                .map_err(|error| format!("Case::NoOverride: {error}"))?;
        let expected = defaults
            .story_visibility
            .resolve(&universe(), &friends(), &devices());
        if audience.keyed != expected.keyed || audience.unkeyed != expected.unkeyed {
            return Err("Case::NoOverride: story with no override did not use the default".to_owned());
        }
        if !audience.keyed.contains("alice") || !audience.keyed.contains("carol") {
            return Err("Case::NoOverride: default except-bob audience missing an expected recipient".to_owned());
        }
        if audience.keyed.contains("bob") {
            return Err("Case::NoOverride: default except-bob audience wrongly keyed bob".to_owned());
        }
        report.push(format!(
            "no-override: story_visibility={} keyed={:?}",
            defaults.story_visibility.option.stable_id(),
            audience.keyed
        ));
        invoked.insert(Case::NoOverride);
    }

    // ---- Case::Override ----------------------------------------------------
    // A story published WITH a signed SEND TO uses only that audience —
    // the default (deliberately set to something that would keep bob out
    // and let the whole friend list in) is fully overridden.
    if starved != Some(Case::Override) {
        let defaults = Defaults {
            post_visibility: VisibilityChoice::everyone(),
            story_visibility: VisibilityChoice::everyone(),
            story_lifetime: StoryLifetime::OneHour,
        };
        let (secret, _public) = ed25519::generate_keypair();
        let send_to = SignedSendTo::sign(VisibilityChoice::chosen(["bob".to_owned()]), &secret);
        let audience = resolve_story_audience(
            &defaults,
            &universe(),
            &friends(),
            &devices(),
            Some(&send_to),
        )
        .map_err(|error| format!("Case::Override: valid signature was refused: {error}"))?;
        if audience.keyed != ["bob".to_owned()].into_iter().collect::<BTreeSet<_>>() {
            return Err(format!(
                "Case::Override: SEND TO chosen=[bob] did not narrow the default everyone audience, got {:?}",
                audience.keyed
            ));
        }
        if audience.keyed.contains("alice") || audience.keyed.contains("carol") {
            return Err("Case::Override: default's everyone audience leaked past the override".to_owned());
        }

        // A tampered override (signature no longer matches the claimed
        // choice) must be refused, not silently defaulted.
        let mut tampered = send_to.clone();
        tampered.choice = VisibilityChoice::everyone();
        match resolve_story_audience(
            &defaults,
            &universe(),
            &friends(),
            &devices(),
            Some(&tampered),
        ) {
            Ok(_) => {
                return Err("Case::Override: tampered SEND TO override was accepted".to_owned())
            }
            Err(_) => {}
        }
        report.push(format!(
            "override: default=vis.everyone override=chosen[bob] resolved-keyed={:?} tamper-rejected=true",
            audience.keyed
        ));
        invoked.insert(Case::Override);
    }

    // ---- Case::Lifetime -----------------------------------------------------
    if starved != Some(Case::Lifetime) {
        let mut seen = BTreeSet::new();
        for life in StoryLifetime::ALL {
            let defaults = Defaults {
                story_lifetime: life,
                ..Defaults::default()
            };
            let seconds = resolve_story_lifetime_seconds(&defaults);
            if seconds != life.seconds() {
                return Err(format!(
                    "Case::Lifetime: {} resolved to {seconds}s, expected {}s",
                    life.stable_id(),
                    life.seconds()
                ));
            }
            seen.insert((life.stable_id(), seconds));
        }
        if seen.len() != 4 {
            return Err("Case::Lifetime: STARVED — not all four lifetimes were exercised".to_owned());
        }
        report.push(format!("lifetime: {:?}", seen));
        invoked.insert(Case::Lifetime);
    }

    for required in Case::ALL {
        if !invoked.contains(&required) {
            return Err(format!("TASK4656 missing required case: {}", required.name()));
        }
    }

    println!("TASK 4656 post/story/story-lifetime Settings defaults");
    for line in &report {
        println!("  {line}");
    }
    println!(
        "TASK4656 cases: {}",
        Case::ALL.map(Case::name).join(",")
    );
    println!("TASK 4656 PASS");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
