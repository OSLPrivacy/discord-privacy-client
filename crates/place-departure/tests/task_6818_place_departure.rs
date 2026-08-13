//! TASK 6818 — the check for leaving a group or an enclave.
//!
//! This is the whole bar, and it is graded end to end rather than in pieces.
//! One run of the check drives four engine stages over the shipped scenario:
//!
//! 1. `model`  — the engine builds the world and hands the interface the live
//!               sidebar model at each point a menu has to be rendered. The
//!               engine also drives its own departures here, which is what
//!               exercises the engine-side ownership boundary.
//! 2. `depart` — a *second, independent* world, driven only by activations
//!               parsed back out of the markup the shipped sidebar module
//!               actually rendered. A step whose leave item is missing or
//!               disabled produces no activation and nothing happens for it.
//! 3. `forged` — a third world, driven by a menu whose disabled leave items
//!               were rewritten to look enabled. The engine must refuse them
//!               anyway and write nothing, because the menu is not the
//!               authority.
//! 4. `relabel`— a fourth world built from the same scenario with every role
//!               label permuted. Every departure verdict has to be identical,
//!               which is what "never decide this from a shipped or renamed
//!               role label" means.
//!
//! The rendering steps are real: they shell out to `vite-node` and run
//! `apps/osl-hub-ui/scripts/render-place-departure-6818.ts`, which imports the
//! shipped `apps/osl-hub-ui/src/place-departure-6818.ts`. If that renderer is
//! missing, or renders no leave item, this check goes red rather than quietly
//! grading the engine on its own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use place_departure::client::PlaceKind;
use place_departure::enclave::Admission;
use place_departure::run::{run, MenuBundle, RunReport};
use place_departure::world::{
    ContentRound, DepartureOutcome, DepartureReport, Fixture, MenuActivation,
};

// ---------------------------------------------------------------------------
// What the scenario is expected to contain. Starving a place out of the
// scenario, or renaming one, fails here before anything else is looked at.
// ---------------------------------------------------------------------------

const LEAVER: &str = "wren";
/// The place the leaver never leaves. Without it, "the leaver receives nothing"
/// would also be satisfied by an engine that is simply broken for that member.
const CONTROL_PLACE: &str = "beacon-group";

/// (place, kind, step, delete_local_history, roster before, roster after)
const DEPARTURES: &[(&str, PlaceKind, &str, bool, &[&str], &[&str])] = &[
    (
        "atlas-group",
        PlaceKind::Group,
        "leave-atlas",
        false,
        &["ash", "brook", "cy", "wren"],
        &["ash", "brook", "cy"],
    ),
    (
        "harbor-enclave",
        PlaceKind::Enclave,
        "leave-harbor",
        false,
        &["ash", "brook", "cy", "wren"],
        &["ash", "brook", "cy"],
    ),
    (
        "foundry-group",
        PlaceKind::Group,
        "leave-foundry",
        true,
        &["dell", "eve", "wren"],
        &["dell", "eve"],
    ),
    (
        "quarry-enclave",
        PlaceKind::Enclave,
        "leave-quarry",
        false,
        &["dell", "eve", "wren"],
        &["dell", "eve"],
    ),
];

/// The two attempts the leaver makes while they are the only member holding a
/// role id that carries the place's required authority.
const BLOCKED_STEPS: &[(&str, &str)] = &[
    ("leave-foundry-blocked", "foundry-group"),
    ("leave-quarry-blocked", "quarry-enclave"),
];

/// (place, channel, the leaver could read it, the remaining readers)
const CHANNEL_REKEYS: &[(&str, &str, bool, &[&str])] = &[
    ("harbor-enclave", "general", true, &["ash", "brook", "cy"]),
    ("harbor-enclave", "logistics", true, &["ash"]),
    ("harbor-enclave", "stewards", false, &[]),
    ("quarry-enclave", "hall", true, &["dell", "eve"]),
    ("quarry-enclave", "survey", false, &[]),
];

/// (client@place, exact roster, exact epoch) every client must hold after a
/// restart that replays the signed ladder.
const RESTART_ROSTERS: &[(&str, &[&str], u64)] = &[
    ("ash@atlas-group", &["ash", "brook", "cy"], 5),
    ("ash@harbor-enclave", &["ash", "brook", "cy"], 5),
    ("brook@atlas-group", &["ash", "brook", "cy"], 5),
    ("brook@harbor-enclave", &["ash", "brook", "cy"], 5),
    ("cy@atlas-group", &["ash", "brook", "cy"], 5),
    ("cy@beacon-group", &["cy", "eve", "wren"], 3),
    ("cy@harbor-enclave", &["ash", "brook", "cy"], 5),
    ("dell@foundry-group", &["dell", "eve"], 4),
    ("dell@quarry-enclave", &["dell", "eve"], 4),
    ("eve@beacon-group", &["cy", "eve", "wren"], 3),
    ("eve@foundry-group", &["dell", "eve"], 4),
    ("eve@quarry-enclave", &["dell", "eve"], 4),
    ("wren@beacon-group", &["cy", "eve", "wren"], 3),
];

/// (place, retained local messages, the separate deletion choice was taken)
const HISTORY_AFTER: &[(&str, usize, bool)] = &[
    ("atlas-group", 2, false),
    ("foundry-group", 0, true),
    ("harbor-enclave", 2, false),
    ("quarry-enclave", 1, false),
];

// ---------------------------------------------------------------------------
// Rendering bridge
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct SkippedStep {
    step: String,
    reason: String,
}

#[derive(serde::Deserialize)]
struct RenderedMenu {
    task: u32,
    source: String,
    activations: Vec<MenuActivation>,
    skipped: Vec<SkippedStep>,
    markup: BTreeMap<String, String>,
}

impl RenderedMenu {
    fn bundle(&self) -> MenuBundle {
        MenuBundle {
            task: self.task,
            source: self.source.clone(),
            activations: self.activations.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
struct RenderedAfter {
    markup: String,
    place_list_in_markup: Vec<String>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/place-departure has a repository root")
        .to_path_buf()
}

/// Runs the shipped renderer. A missing renderer is a failure, never a skip.
fn render(mode: &str, input: &Path, output: &Path, mutation: Option<&str>) {
    let ui = repo_root().join("apps/osl-hub-ui");
    let runner = ui.join("node_modules/.bin/vite-node");
    assert!(
        runner.exists(),
        "TASK6818 the menu renderer needs {}; the check cannot grade a menu it \
         never rendered",
        runner.display()
    );
    let script = ui.join("scripts/render-place-departure-6818.ts");
    assert!(
        script.exists(),
        "TASK6818 missing renderer script {}",
        script.display()
    );

    let mut command = Command::new(&runner);
    command
        .current_dir(&ui)
        .arg("scripts/render-place-departure-6818.ts")
        .arg(mode)
        .arg(input)
        .arg(output);
    match mutation {
        Some(value) => {
            command.env("OSL6818_RENDER_MUTATION", value);
        }
        None => {
            command.env_remove("OSL6818_RENDER_MUTATION");
        }
    }
    let out = command.output().expect("the menu renderer runs");
    assert!(
        out.status.success(),
        "TASK6818 menu renderer failed ({mode}): {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("TASK6818 reading {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("TASK6818 parsing {}: {error}", path.display()))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) {
    std::fs::write(path, serde_json::to_vec_pretty(value).expect("serializable"))
        .unwrap_or_else(|error| panic!("TASK6818 writing {}: {error}", path.display()));
}

// ---------------------------------------------------------------------------
// Harness — four engine stages plus the two rendering passes, built once.
// ---------------------------------------------------------------------------

struct Harness {
    model: RunReport,
    menu: RenderedMenu,
    depart: RunReport,
    after: RenderedAfter,
    forged_menu: RenderedMenu,
    forged: RunReport,
    relabelled: RunReport,
    _dir: tempfile::TempDir,
}

fn fixture() -> Fixture {
    place_departure::shipped_fixture().expect("the shipped scenario parses")
}

/// The same scenario with every role label permuted onto a different role.
///
/// The role *ids* are derived from the place handle and a role key that never
/// reaches the interface, so this changes only display text.
fn relabelled_fixture() -> Fixture {
    let mut fixture = fixture();
    for place in &mut fixture.places {
        let labels: Vec<String> = place.roles.iter().map(|role| role.label.clone()).collect();
        for (index, role) in place.roles.iter_mut().enumerate() {
            // Rotate: every role wears the label of the next one, so the role
            // carrying the required authority is never the one labelled the way
            // it shipped, and in the two blocked places a role with no
            // authority ends up labelled "Warden".
            role.label = labels[(index + 1) % labels.len()].clone();
        }
    }
    fixture
}

fn harness() -> &'static Harness {
    static HARNESS: OnceLock<Harness> = OnceLock::new();
    HARNESS.get_or_init(|| {
        let dir = tempfile::tempdir().expect("temp dir");
        let base = dir.path();

        // 1. model — the engine drives itself and emits the sidebar models.
        let model = run(fixture(), &base.join("state-model"), None, "model")
            .expect("TASK6818 model stage runs");
        let model_path = base.join("model.json");
        write_json(&model_path, &model);

        // The honest menu: real markup, real activation parsing.
        let menu_path = base.join("menu.json");
        render("menu", &model_path, &menu_path, None);
        let menu: RenderedMenu = read_json(&menu_path);

        // 2. depart — a fresh world driven only by what the menu produced.
        let depart = run(
            fixture(),
            &base.join("state-depart"),
            Some(&menu.bundle()),
            "depart",
        )
        .expect("TASK6818 depart stage runs");
        let depart_path = base.join("depart.json");
        write_json(&depart_path, &depart);

        // The leaver's own surface, rendered from that report.
        let after_path = base.join("after.json");
        render("after", &depart_path, &after_path, None);
        let after: RenderedAfter = read_json(&after_path);

        // 3. forged — the menu lies about the disabled items being enabled.
        let forged_menu_path = base.join("forged-menu.json");
        render(
            "menu",
            &model_path,
            &forged_menu_path,
            Some("enable-blocked-item"),
        );
        let forged_menu: RenderedMenu = read_json(&forged_menu_path);
        let forged = run(
            fixture(),
            &base.join("state-forged"),
            Some(&forged_menu.bundle()),
            "forged",
        )
        .expect("TASK6818 forged stage runs");

        // 4. relabel — same scenario, every role label moved.
        let relabelled = run(
            relabelled_fixture(),
            &base.join("state-relabel"),
            None,
            "relabel",
        )
        .expect("TASK6818 relabel stage runs");

        Harness {
            model,
            menu,
            depart,
            after,
            forged_menu,
            forged,
            relabelled,
            _dir: dir,
        }
    })
}

fn completed<'a>(report: &'a RunReport, step: &str) -> &'a DepartureReport {
    report
        .departures
        .iter()
        .find(|departure| departure.step == step)
        .unwrap_or_else(|| panic!("TASK6818 no departure recorded for step {step}"))
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn round<'a>(report: &'a RunReport, place: &str, channel: Option<&str>) -> &'a ContentRound {
    report
        .content
        .iter()
        .find(|round| round.place == place && round.channel.as_deref() == channel)
        .unwrap_or_else(|| {
            panic!("TASK6818 no post-departure content round for {place} channel {channel:?}")
        })
}

fn delivery<'a>(round: &'a ContentRound, client: &str) -> &'a place_departure::world::DeliveryRow {
    round
        .deliveries
        .iter()
        .find(|row| row.client == client)
        .unwrap_or_else(|| panic!("TASK6818 no delivery row for {client} in {}", round.mark))
}

// ---------------------------------------------------------------------------
// 1. The menu is the only way in, and it drives every departure.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_every_departure_is_driven_by_the_rendered_menu() {
    let harness = harness();

    assert_eq!(
        harness.menu.source, "apps/osl-hub-ui/src/place-departure-6818.ts",
        "the activations must come from the shipped sidebar module"
    );
    assert!(
        !harness.menu.markup.is_empty(),
        "TASK6818 the renderer produced no markup at all"
    );
    println!(
        "TASK6818_MENU_POINTS: {}",
        harness.menu.markup.len()
    );

    // Every rendered point carries a real place rail with a real leave item.
    for (point, markup) in &harness.menu.markup {
        assert!(
            markup.contains("<nav class=\"place-rail\""),
            "TASK6818 point {point} rendered no place rail"
        );
        assert!(
            markup.contains("place-menu-item--leave"),
            "TASK6818 point {point} rendered no leave item"
        );
    }

    // Four activations, one per real place, each carrying the exact shipped
    // wording and the action that matches the place kind.
    assert_eq!(
        harness.menu.activations.len(),
        DEPARTURES.len(),
        "TASK6818 the rendered menu produced {} activations, expected {}",
        harness.menu.activations.len(),
        DEPARTURES.len()
    );
    for (place, kind, step, delete_local_history, _, _) in DEPARTURES {
        let activation = harness
            .menu
            .activations
            .iter()
            .find(|activation| activation.step == *step)
            .unwrap_or_else(|| panic!("TASK6818 no menu activation for step {step}"));
        let (expected_action, expected_label) = match kind {
            PlaceKind::Group => ("leave-group", "Leave group"),
            PlaceKind::Enclave => ("leave-enclave", "Leave enclave"),
        };
        assert_eq!(&activation.place, place);
        assert_eq!(activation.menu_action, expected_action);
        assert_eq!(
            activation.rendered_label, expected_label,
            "TASK6818 {place} rendered the wrong leave wording"
        );
        assert!(activation.rendered_enabled);
        assert_eq!(activation.actor, LEAVER);
        assert_eq!(activation.delete_local_history, *delete_local_history);
        println!(
            "TASK6818_MENU_ACTIVATION: step={step} place={place} action={} label=\"{}\" delete_local_history={}",
            activation.menu_action, activation.rendered_label, activation.delete_local_history
        );
    }

    // The two blocked steps are not offered at all by the honest menu.
    assert_eq!(harness.menu.skipped.len(), BLOCKED_STEPS.len());
    for (step, _) in BLOCKED_STEPS {
        let skipped = harness
            .menu
            .skipped
            .iter()
            .find(|skipped| skipped.step == *step)
            .unwrap_or_else(|| panic!("TASK6818 step {step} should not be offered"));
        assert_eq!(skipped.reason, "last-authority-holder");
        println!("TASK6818_MENU_SKIPPED: step={step} reason={}", skipped.reason);
    }

    // And the engine acted on all four, and on nothing else. The only steps it
    // never heard about are exactly the two the honest menu refused to offer —
    // a menu that dropped a leave item it should have rendered would show up
    // here as an extra starved step.
    let mut starved = harness.depart.missing_activations.clone();
    starved.sort();
    let mut expected_starved: Vec<String> =
        BLOCKED_STEPS.iter().map(|(step, _)| step.to_string()).collect();
    expected_starved.sort();
    assert_eq!(
        starved, expected_starved,
        "TASK6818 the engine was starved of activations the menu should have rendered"
    );
    assert_eq!(harness.depart.departures.len(), DEPARTURES.len());
    let kinds: BTreeSet<&str> = harness
        .depart
        .departures
        .iter()
        .map(|departure| match departure.kind {
            PlaceKind::Group => "group",
            PlaceKind::Enclave => "enclave",
        })
        .collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["enclave", "group"]),
        "TASK6818 both a real group and a real enclave must be left"
    );
    println!(
        "TASK6818_DEPARTURES: {}",
        harness
            .depart
            .departures
            .iter()
            .map(|d| d.place.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

// ---------------------------------------------------------------------------
// 2. Group departure: only the leaver goes, and sender state rotates.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_group_departure_removes_only_the_leaver_and_rotates_sender_state() {
    let harness = harness();

    for (place, kind, step, _, before, after) in DEPARTURES {
        if *kind != PlaceKind::Group {
            continue;
        }
        let departure = completed(&harness.depart, step);
        let DepartureOutcome::Completed {
            epoch_before,
            epoch_after,
        } = &departure.outcome
        else {
            panic!("TASK6818 {place} did not complete: {:?}", departure.outcome);
        };
        assert_eq!(*epoch_after, *epoch_before + 1);
        assert_eq!(departure.roster_before, strings(before));
        assert_eq!(departure.roster_after, strings(after));
        assert_eq!(departure.removed, vec![LEAVER.to_string()]);
        assert!(
            departure.other_members_changed.is_empty(),
            "TASK6818 {place} changed members other than the leaver"
        );
        assert!(
            departure.signed_leave_event.is_some(),
            "TASK6818 {place} left without a signed self-removal — a local-list-only leave"
        );

        // Every remaining member rotated their own chain, and handed the new
        // one to the remaining roster only.
        let rotated: Vec<String> = departure
            .rotations
            .iter()
            .map(|row| row.client.clone())
            .collect();
        assert_eq!(
            rotated,
            strings(after),
            "TASK6818 {place} did not rotate every remaining member's sender chain"
        );
        for row in &departure.rotations {
            assert!(
                row.chain_id_before != Some(row.chain_id_after),
                "TASK6818 {place}/{} kept its rotation root", row.client
            );
            assert!(
                !row.distributed_to.contains(&LEAVER.to_string()),
                "TASK6818 {place}/{} handed the new chain to the leaver", row.client
            );
            let mut expected: Vec<String> = strings(after);
            expected.retain(|handle| handle != &row.client);
            assert_eq!(row.distributed_to, expected);
            println!(
                "TASK6818_ROTATION: place={place} client={} chain {:?} -> {} to [{}]",
                row.client,
                row.chain_id_before,
                row.chain_id_after,
                row.distributed_to.join(", ")
            );
        }
        assert_eq!(departure.keys_wrapped_for_leaver, 0);
    }
}

// ---------------------------------------------------------------------------
// 3. Enclave departure: epoch advances, every affected channel is re-keyed,
//    and future content is denied.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_enclave_departure_advances_the_epoch_rekeys_and_denies_future_content() {
    let harness = harness();

    for (place, kind, step, _, before, after) in DEPARTURES {
        if *kind != PlaceKind::Enclave {
            continue;
        }
        let departure = completed(&harness.depart, step);
        let DepartureOutcome::Completed {
            epoch_before,
            epoch_after,
        } = &departure.outcome
        else {
            panic!("TASK6818 {place} did not complete: {:?}", departure.outcome);
        };
        assert_eq!(
            *epoch_after,
            *epoch_before + 1,
            "TASK6818 {place} did not advance the roster epoch"
        );
        assert_eq!(departure.roster_before, strings(before));
        assert_eq!(departure.roster_after, strings(after));
        assert_eq!(departure.removed, vec![LEAVER.to_string()]);
        assert!(departure.other_members_changed.is_empty());
        assert!(
            departure.signed_leave_event.is_some(),
            "TASK6818 {place} left without a signed self-removal"
        );
        println!(
            "TASK6818_ENCLAVE_EPOCH: place={place} {epoch_before} -> {epoch_after}"
        );

        let expected: Vec<&(&str, &str, bool, &[&str])> = CHANNEL_REKEYS
            .iter()
            .filter(|(channel_place, ..)| channel_place == place)
            .collect();
        assert_eq!(
            departure.rekeys.len(),
            expected.len(),
            "TASK6818 {place} did not consider every channel"
        );
        for (_, channel, leaver_was_reader, readers) in expected {
            let row = departure
                .rekeys
                .iter()
                .find(|row| row.channel == *channel)
                .unwrap_or_else(|| panic!("TASK6818 {place} skipped channel {channel}"));
            assert_eq!(row.leaver_was_reader, *leaver_was_reader);
            assert_eq!(row.rekeyed, *leaver_was_reader);
            if *leaver_was_reader {
                assert_ne!(
                    row.key_id_before, row.key_id_after,
                    "TASK6818 {place}/{channel} kept its content key id"
                );
                assert_eq!(row.key_epoch_after, row.key_epoch_before + 1);
                assert_eq!(row.wrapped_for, strings(readers));
                assert!(!row.wrapped_for.contains(&LEAVER.to_string()));
            } else {
                // A channel the leaver could never read is deliberately left
                // alone, so a blanket re-key cannot masquerade as a real one.
                assert_eq!(row.key_id_before, row.key_id_after);
                assert_eq!(row.key_epoch_after, row.key_epoch_before);
                assert!(row.wrapped_for.is_empty());
            }
            println!(
                "TASK6818_REKEY: place={place} channel={channel} leaver_was_reader={} rekeyed={} epoch {} -> {} wrapped_for=[{}]",
                row.leaver_was_reader,
                row.rekeyed,
                row.key_epoch_before,
                row.key_epoch_after,
                row.wrapped_for.join(", ")
            );
        }
        assert_eq!(departure.keys_wrapped_for_leaver, 0);

        // Denial is an admission decision, not merely a missing key.
        for (_, channel, ..) in CHANNEL_REKEYS.iter().filter(|(p, ..)| p == place) {
            let content = round(&harness.depart, place, Some(channel));
            let row = delivery(content, LEAVER);
            assert_eq!(
                row.admission.as_ref(),
                Some(&Admission::DeniedNotOnRoster),
                "TASK6818 {place}/{channel} did not deny the departed member"
            );
            assert!(!row.decrypted);
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Restart: every remaining client converges on the exact roster.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_remaining_clients_converge_on_the_exact_roster_after_restart() {
    let harness = harness();

    let observed: Vec<(String, Vec<String>, u64)> = harness
        .depart
        .restart_rosters
        .iter()
        .map(|view| (view.client.clone(), view.roster.clone(), view.epoch))
        .collect();
    let expected: Vec<(String, Vec<String>, u64)> = RESTART_ROSTERS
        .iter()
        .map(|(client, roster, epoch)| (client.to_string(), strings(roster), *epoch))
        .collect();
    assert_eq!(
        observed, expected,
        "TASK6818 the replayed rosters after restart do not match exactly"
    );
    for (client, roster, epoch) in &observed {
        println!("TASK6818_RESTART_ROSTER: {client} epoch={epoch} roster=[{}]", roster.join(", "));
    }

    // The leaver holds none of the places they left, at any epoch.
    for (place, ..) in DEPARTURES {
        assert!(
            !observed
                .iter()
                .any(|(client, ..)| client == &format!("{LEAVER}@{place}")),
            "TASK6818 the leaver still holds {place} after the restart"
        );
    }
    assert!(
        observed
            .iter()
            .any(|(client, ..)| client == &format!("{LEAVER}@{CONTROL_PLACE}")),
        "TASK6818 the leaver lost the control place they never left"
    );

    // The restart is a real restart: sender chains are session-only, so every
    // group member comes back with no outbound chain and installs a fresh one.
    // A restart that handed back a cached in-memory session would carry the
    // pre-restart chain id here.
    assert!(
        !harness.depart.post_restart_distribution.is_empty(),
        "TASK6818 nothing was redistributed after the restart"
    );
    for row in &harness.depart.post_restart_distribution {
        assert_eq!(
            row.chain_id_before, None,
            "TASK6818 {}/{} survived the restart with a live sender chain — that is not a restart",
            row.place, row.client
        );
    }
    println!(
        "TASK6818_RESTART_FRESH_CHAINS: {}",
        harness.depart.post_restart_distribution.len()
    );
}

// ---------------------------------------------------------------------------
// 5. New marked content reaches the members who stayed, and not the leaver.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_new_content_reaches_the_remaining_members_and_not_the_leaver() {
    let harness = harness();

    let mut leaver_new_content = 0_usize;
    let mut remaining_new_content = 0_usize;
    let mut opened_per_place: BTreeMap<&str, usize> = BTreeMap::new();

    for (place, kind, _, _, _, after) in DEPARTURES {
        let channels: Vec<Option<&str>> = match kind {
            PlaceKind::Group => vec![None],
            PlaceKind::Enclave => CHANNEL_REKEYS
                .iter()
                .filter(|(channel_place, ..)| channel_place == place)
                .map(|(_, channel, ..)| Some(*channel))
                .collect(),
        };
        for channel in channels {
            let content = round(&harness.depart, place, channel);
            let expected_mark = match channel {
                None => format!("after-departure/{place}"),
                Some(channel) => format!("after-departure/{place}/{channel}"),
            };
            assert_eq!(content.mark, expected_mark);
            assert!(
                after.contains(&content.sender.as_str()),
                "TASK6818 {place} new content was sent by someone who left"
            );

            let leaver_row = delivery(content, LEAVER);
            assert!(
                !leaver_row.decrypted,
                "TASK6818 the leaver opened new content in {place} after leaving"
            );
            assert_eq!(leaver_row.mark, None);
            assert_eq!(
                leaver_row.admission.as_ref(),
                Some(&Admission::DeniedNotOnRoster)
            );

            let opened: Vec<String> = content
                .deliveries
                .iter()
                .filter(|row| row.decrypted)
                .map(|row| row.client.clone())
                .collect();
            remaining_new_content += opened.len();
            *opened_per_place.entry(place).or_default() += opened.len();
            println!(
                "TASK6818_NEW_CONTENT: mark={} opened_by=[{}] leaver_opened={}",
                content.mark,
                opened.join(", "),
                leaver_row.decrypted
            );
        }
    }

    // Every place the leaver left produced content that at least one remaining
    // member actually opened. Otherwise "the leaver got nothing" would be true
    // of an engine that delivers nothing to anyone.
    assert!(
        remaining_new_content >= DEPARTURES.len(),
        "TASK6818 the remaining members opened only {remaining_new_content} new messages"
    );
    for (place, ..) in DEPARTURES {
        let opened = opened_per_place.get(place).copied().unwrap_or_default();
        assert!(
            opened >= 1,
            "TASK6818 no remaining member opened new content in {place}; the leaver receiving \
             nothing there proves nothing"
        );
    }

    // Control: the place the leaver never left still reaches them.
    let control = round(&harness.depart, CONTROL_PLACE, None);
    let control_row = delivery(control, LEAVER);
    assert!(
        control_row.decrypted,
        "TASK6818 the leaver stopped receiving content in the place they never left — \
         the engine is broken for that member rather than honouring a departure"
    );
    assert_eq!(control_row.mark.as_deref(), Some(control.mark.as_str()));
    assert_eq!(control_row.admission.as_ref(), Some(&Admission::Allowed));
    leaver_new_content += 1;
    println!(
        "TASK6818_CONTROL_CONTENT: mark={} leaver_opened={}",
        control.mark, control_row.decrypted
    );
    println!("TASK6818_LEAVER_NEW_CONTENT_IN_LEFT_PLACES: 0");
    println!("TASK6818_LEAVER_NEW_CONTENT_IN_CONTROL_PLACE: {leaver_new_content}");
}

// ---------------------------------------------------------------------------
// 6. The leaver receives zero new keys.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_the_leaver_receives_zero_new_keys() {
    let harness = harness();

    let mut wrapped_for_leaver = 0_usize;
    for departure in &harness.depart.departures {
        wrapped_for_leaver += departure.keys_wrapped_for_leaver;
        for row in &departure.rotations {
            assert!(!row.distributed_to.contains(&LEAVER.to_string()));
        }
        for row in &departure.rekeys {
            assert!(!row.wrapped_for.contains(&LEAVER.to_string()));
        }
    }
    assert_eq!(
        wrapped_for_leaver, 0,
        "TASK6818 key material was wrapped for the leaver during departure"
    );

    // And nothing reached them in the post-restart redistribution either.
    let mut control_wraps = 0_usize;
    for row in &harness.depart.post_restart_distribution {
        let to_leaver = row.distributed_to.contains(&LEAVER.to_string());
        if row.place == CONTROL_PLACE {
            if to_leaver {
                control_wraps += 1;
            }
        } else {
            assert!(
                !to_leaver,
                "TASK6818 {} handed a post-restart chain to the leaver",
                row.place
            );
        }
    }
    assert!(
        control_wraps >= 2,
        "TASK6818 the control place stopped distributing to the leaver ({control_wraps} wraps) — \
         the zero above would then mean nothing"
    );
    println!("TASK6818_NEW_KEYS_FOR_LEAVER_IN_LEFT_PLACES: 0");
    println!("TASK6818_NEW_KEYS_FOR_LEAVER_IN_CONTROL_PLACE: {control_wraps}");

    // Honest about what stays: the key material already on the device is not
    // recalled, and the check records it rather than pretending otherwise.
    for probe in &harness.depart.leaver_keys_after {
        println!(
            "TASK6818_LEAVER_RETAINED_KEYS: place={} sender_keys={} channel_keys={} in_place_list={}",
            probe.place, probe.held_sender_keys, probe.held_channel_keys, probe.in_place_list
        );
    }
}

// ---------------------------------------------------------------------------
// 7. The leaver's own surface: the place is gone, the local copy is honest.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_the_leaver_sees_the_place_absent_with_an_honest_history_copy() {
    let harness = harness();
    let view = &harness.depart.leaver_view;

    assert_eq!(view.leaver, LEAVER);
    assert_eq!(
        view.place_list,
        vec![CONTROL_PLACE.to_string()],
        "TASK6818 the leaver's place list is wrong after leaving four places"
    );
    assert_eq!(
        harness.after.place_list_in_markup,
        vec![CONTROL_PLACE.to_string()],
        "TASK6818 the rendered place list still shows a place that was left"
    );
    assert_eq!(view.departed.len(), HISTORY_AFTER.len());

    for (place, retained, deleted) in HISTORY_AFTER {
        let departed = view
            .departed
            .iter()
            .find(|row| row.handle == *place)
            .unwrap_or_else(|| panic!("TASK6818 no departed row for {place}"));
        assert!(!departed.in_place_list);
        assert_eq!(
            departed.retained_messages, *retained,
            "TASK6818 {place} kept the wrong number of local messages"
        );
        assert_eq!(departed.deletion_choice_applied, *deleted);

        // The same facts have to be in the markup the leaver is actually shown.
        let kind = if place.ends_with("-group") {
            "group"
        } else {
            "enclave"
        };
        let expected_row = format!(
            "data-departed-place=\"{place}\" data-departed-kind=\"{kind}\" \
             data-in-place-list=\"false\" data-retained-messages=\"{retained}\" \
             data-deletion-choice=\"{}\"",
            if *deleted { "applied" } else { "not-applied" }
        )
        .replace("             ", "");
        assert!(
            harness.after.markup.contains(&expected_row),
            "TASK6818 the leaver's surface does not honestly report {place}; expected `{expected_row}`"
        );
        println!(
            "TASK6818_LEAVER_HISTORY: place={place} in_place_list={} retained_messages={} deletion_choice_applied={}",
            departed.in_place_list, departed.retained_messages, departed.deletion_choice_applied
        );
    }

    // The wording is the honest one: leaving is not deletion, deletion is a
    // separate choice, and neither reaches anyone else's device.
    assert!(harness.after.markup.contains(
        "It did not delete the messages you already had — they are still on this device."
    ));
    assert!(harness.after.markup.contains(
        "Members who stay keep their own copies. Leaving cannot delete a message from someone else&#039;s device."
    ) || harness.after.markup.contains(
        "Members who stay keep their own copies. Leaving cannot delete a message from someone else's device."
    ));
    // The one place where the separate deletion choice was taken says so, and
    // no other place claims a deletion that did not happen.
    assert_eq!(
        harness.after.markup.matches("data-deletion-choice=\"applied\"").count(),
        1
    );
}

// ---------------------------------------------------------------------------
// 8. The ownership boundary: a last-authority attempt writes nothing, and a
//    second holder releases it.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_a_last_authority_attempt_changes_zero_bytes_and_a_second_holder_permits_departure() {
    let harness = harness();

    for report in [&harness.model, &harness.forged] {
        for (step, place) in BLOCKED_STEPS {
            let attempt = completed(report, step);
            let DepartureOutcome::Refused { code, detail } = &attempt.outcome else {
                panic!(
                    "TASK6818 [{}] {place} allowed the last authority holder to leave",
                    report.stage
                );
            };
            assert_eq!(code, "last-authority-holder");
            assert!(detail.contains("transfer it before leaving"));

            // Nothing moved: no signed event, no key work, no roster change.
            assert!(attempt.signed_leave_event.is_none());
            assert!(attempt.rotations.is_empty());
            assert!(attempt.rekeys.is_empty());
            assert_eq!(attempt.roster_before, attempt.roster_after);
            assert!(attempt.removed.is_empty());
            assert!(!attempt.deletion_choice_applied);
            assert_eq!(
                attempt.leaver_held_keys_digest_before, attempt.leaver_held_keys_digest_after
            );

            // And nothing was written: every client's sealed file is the same
            // length with the same digest, before and after the attempt.
            assert_eq!(
                attempt.state_bytes_before, attempt.state_bytes_after,
                "TASK6818 [{}] {place} wrote to disk during a refused departure",
                report.stage
            );
            let changed: usize = attempt
                .state_bytes_before
                .iter()
                .zip(&attempt.state_bytes_after)
                .filter(|(before, after)| before != after)
                .count();
            let delta: i64 = attempt
                .state_bytes_after
                .iter()
                .map(|(_, length, _)| *length as i64)
                .sum::<i64>()
                - attempt
                    .state_bytes_before
                    .iter()
                    .map(|(_, length, _)| *length as i64)
                    .sum::<i64>();
            assert_eq!(changed, 0);
            assert_eq!(delta, 0);
            println!(
                "TASK6818_LAST_AUTHORITY_REFUSAL: stage={} step={step} place={place} code={code} files_changed={changed} byte_delta={delta}",
                report.stage
            );
        }
    }

    // The forged run is the strong form: the menu claimed the item was enabled
    // and the engine refused anyway.
    for (step, _) in BLOCKED_STEPS {
        let activation = harness
            .forged_menu
            .activations
            .iter()
            .find(|activation| activation.step == *step)
            .unwrap_or_else(|| {
                panic!("TASK6818 the forged menu did not offer {step}; the probe proved nothing")
            });
        assert!(
            activation.rendered_enabled,
            "TASK6818 the forged menu did not actually claim {step} was enabled"
        );
        println!(
            "TASK6818_FORGED_MENU_CLAIM: step={step} rendered_enabled={} engine=refused",
            activation.rendered_enabled
        );
    }

    // The two-authority control: after the required authority is transferred to
    // a second member, the same member leaves the same place.
    for (place, _, step, ..) in DEPARTURES {
        if !BLOCKED_STEPS.iter().any(|(_, blocked)| blocked == place) {
            continue;
        }
        let departure = completed(&harness.depart, step);
        assert!(
            matches!(departure.outcome, DepartureOutcome::Completed { .. }),
            "TASK6818 {place} still refused departure after the authority was transferred"
        );
        assert_eq!(departure.model_at_attempt.authority_verdict, "redundant");
        assert!(departure.model_at_attempt.leave_enabled);
        assert_eq!(departure.model_at_attempt.refusal_code, None);
        assert_eq!(
            departure.model_at_attempt.other_authority_holders,
            vec!["dell".to_string()]
        );
        println!(
            "TASK6818_TWO_AUTHORITY_CONTROL: place={place} verdict={} other_holders=[{}] outcome=completed",
            departure.model_at_attempt.authority_verdict,
            departure.model_at_attempt.other_authority_holders.join(", ")
        );
    }
}

// ---------------------------------------------------------------------------
// 9. The gate never reads a role label — shipped or renamed.
// ---------------------------------------------------------------------------

#[test]
fn task_6818_the_ownership_gate_ignores_role_labels() {
    let harness = harness();

    // The scenario ships a role literally labelled "Owner" that carries no
    // authority at all, held by the member the leaver has to transfer to.
    let initial = harness
        .model
        .models
        .iter()
        .find(|model| model.point == "initial")
        .expect("TASK6818 the initial sidebar model");
    let blocked = initial
        .places
        .iter()
        .find(|place| place.handle == "foundry-group")
        .expect("foundry-group is in the leaver's list");
    assert!(!blocked.leave_enabled);
    assert_eq!(blocked.refusal_code.as_deref(), Some("last-authority-holder"));
    assert_eq!(blocked.authority_verdict, "last_holder");
    assert_eq!(
        blocked.held_authority_role_labels,
        vec!["Warden".to_string()],
        "TASK6818 the scenario no longer ships the label the gate must ignore"
    );
    println!(
        "TASK6818_SHIPPED_LABELS: place=foundry-group leaver_role_label={:?} blocking=true owner_labelled_role_holds_authority=false",
        blocked.held_authority_role_labels
    );

    // Now the same scenario with every label rotated onto a different role.
    // Verdicts must be byte-identical; only the display text may move.
    assert_eq!(
        harness.relabelled.models.len(),
        harness.model.models.len(),
        "TASK6818 the relabelled run rendered a different number of model points"
    );
    let mut compared = 0_usize;
    for (shipped, renamed) in harness.model.models.iter().zip(&harness.relabelled.models) {
        assert_eq!(shipped.point, renamed.point);
        assert_eq!(shipped.places.len(), renamed.places.len());
        for (left, right) in shipped.places.iter().zip(&renamed.places) {
            assert_eq!(left.handle, right.handle);
            assert_eq!(
                (
                    left.leave_enabled,
                    left.refusal_code.clone(),
                    left.authority_verdict.clone(),
                    left.other_authority_holders.clone()
                ),
                (
                    right.leave_enabled,
                    right.refusal_code.clone(),
                    right.authority_verdict.clone(),
                    right.other_authority_holders.clone()
                ),
                "TASK6818 renaming a role changed the departure verdict for {}",
                left.handle
            );
            compared += 1;
        }
    }
    assert!(compared >= 8, "TASK6818 compared only {compared} place rows");
    println!("TASK6818_RELABELLED_VERDICTS_IDENTICAL: {compared}");

    // The relabelled run must still take the same route through the engine:
    // same refusals, same completions, in the same order.
    let shipped: Vec<(String, bool)> = harness
        .model
        .departures
        .iter()
        .map(|d| (d.step.clone(), matches!(d.outcome, DepartureOutcome::Refused { .. })))
        .collect();
    let renamed: Vec<(String, bool)> = harness
        .relabelled
        .departures
        .iter()
        .map(|d| (d.step.clone(), matches!(d.outcome, DepartureOutcome::Refused { .. })))
        .collect();
    assert_eq!(shipped, renamed);
    assert_eq!(
        shipped.iter().filter(|(_, refused)| *refused).count(),
        BLOCKED_STEPS.len()
    );
    println!(
        "TASK6818_RELABELLED_OUTCOMES_IDENTICAL: {} steps, {} refused in both runs",
        shipped.len(),
        BLOCKED_STEPS.len()
    );
}
