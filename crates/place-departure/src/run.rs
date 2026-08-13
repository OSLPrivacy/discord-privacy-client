//! The full departure sequence, run identically by the binary and the tests.
//!
//! The sequence is driven by menu activations. In the `model` stage the engine
//! supplies its own so the interface has something to render; in the `depart`
//! stage the only activations it will act on are the ones parsed out of the
//! rendered menu. A step with no activation is recorded as missing and nothing
//! happens for it — which is how a starved menu shows up as a starved
//! departure rather than as a departure that quietly happened anyway.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::client::PlaceKind;
use crate::world::{
    ContentRound, DepartureReport, Fixture, FixtureStep, LeaverViewModel, MenuActivation,
    RosterView, RotationRow, SidebarModel, World, WorldError,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MenuBundle {
    pub task: u32,
    pub source: String,
    pub activations: Vec<MenuActivation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LeaverKeyProbe {
    pub place: String,
    pub held_sender_keys: usize,
    pub held_channel_keys: usize,
    pub in_place_list: bool,
}

/// One leave step the interface has to drive, and the model point whose
/// rendered menu it must be activated from.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LeaveStepPlan {
    pub step: String,
    pub place: String,
    pub delete_local_history: bool,
    pub model_point: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunReport {
    pub task: u32,
    pub stage: String,
    pub leaver: String,
    pub models: Vec<SidebarModel>,
    pub leave_plan: Vec<LeaveStepPlan>,
    pub departures: Vec<DepartureReport>,
    pub missing_activations: Vec<String>,
    pub restart_rosters: Vec<RosterView>,
    pub post_restart_distribution: Vec<RotationRow>,
    pub content: Vec<ContentRound>,
    pub leaver_view: LeaverViewModel,
    pub leaver_keys_after: Vec<LeaverKeyProbe>,
}

/// Executes the fixture's step list.
pub fn run(
    fixture: Fixture,
    directory: &Path,
    menu: Option<&MenuBundle>,
    stage: &str,
) -> Result<RunReport, WorldError> {
    let leaver = fixture.leaver.clone();
    let steps = fixture.steps.clone();
    let mut world = World::build(fixture, directory)?;

    let by_step: BTreeMap<String, MenuActivation> = menu
        .map(|bundle| {
            bundle
                .activations
                .iter()
                .map(|activation| (activation.step.clone(), activation.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut models: Vec<SidebarModel> = Vec::new();
    let mut leave_plan: Vec<LeaveStepPlan> = Vec::new();
    let mut departures = Vec::new();
    let mut missing_activations = Vec::new();
    let mut current_point = String::from("initial");

    for step in &steps {
        match step {
            FixtureStep::Model { point, .. } => {
                current_point = point.clone();
                models.push(world.sidebar_model(point)?);
            }
            FixtureStep::TransferAuthority {
                place, role, to, ..
            } => {
                world.transfer_authority(place, role, to)?;
            }
            FixtureStep::Leave {
                step: step_name,
                place,
                delete_local_history,
            } => {
                leave_plan.push(LeaveStepPlan {
                    step: step_name.clone(),
                    place: place.clone(),
                    delete_local_history: *delete_local_history,
                    model_point: current_point.clone(),
                });
                let activation = match menu {
                    Some(_) => match by_step.get(step_name) {
                        Some(activation) => activation.clone(),
                        None => {
                            missing_activations.push(step_name.clone());
                            continue;
                        }
                    },
                    None => {
                        // Model stage: the engine drives itself so the
                        // interface has a live model to render from.
                        let model = world
                            .sidebar_model("driver")?
                            .places
                            .into_iter()
                            .find(|candidate| candidate.handle == *place);
                        let Some(model) = model else {
                            missing_activations.push(step_name.clone());
                            continue;
                        };
                        MenuActivation {
                            step: step_name.clone(),
                            place: place.clone(),
                            menu_action: model.leave_action.clone(),
                            actor: leaver.clone(),
                            delete_local_history: *delete_local_history,
                            rendered_enabled: model.leave_enabled,
                            rendered_label: leave_label(model.kind),
                        }
                    }
                };
                departures.push(world.depart(&activation)?);
            }
        }
    }

    // Restart every client from its sealed file and let the rosters converge
    // from the replayed signed ladder.
    let restart_rosters = world.restart()?;

    // Sender chains are session-only, so after a restart every group member
    // installs a fresh one and distributes it to the roster they now hold.
    let mut post_restart_distribution = Vec::new();
    let group_places: Vec<String> = world
        .places
        .values()
        .filter(|place| place.kind == PlaceKind::Group)
        .map(|place| place.handle.as_str().to_string())
        .collect();
    for place in &group_places {
        post_restart_distribution.extend(world.redistribute_group_public(place)?);
    }

    // New marked content, after the restart, in every place a departure
    // touched plus the control place the leaver never left.
    let mut content = Vec::new();
    let place_order: Vec<(String, PlaceKind, Vec<String>)> = world
        .places
        .values()
        .map(|place| {
            (
                place.handle.as_str().to_string(),
                place.kind,
                place
                    .channels
                    .iter()
                    .map(|channel| channel.handle.clone())
                    .collect(),
            )
        })
        .collect();
    for (place, kind, channels) in place_order {
        let senders = world.remaining_members(&place);
        let Some(sender) = senders.first().cloned() else {
            continue;
        };
        match kind {
            PlaceKind::Group => {
                content.push(world.broadcast(
                    &place,
                    &sender,
                    None,
                    &format!("after-departure/{place}"),
                    "new content sent after the departure",
                )?);
            }
            PlaceKind::Enclave => {
                for channel in channels {
                    let Some(sender) = world.remaining_channel_member(&place, &channel) else {
                        continue;
                    };
                    content.push(world.broadcast(
                        &place,
                        &sender,
                        Some(&channel),
                        &format!("after-departure/{place}/{channel}"),
                        "new content sent after the departure",
                    )?);
                }
            }
        }
    }

    let leaver_view = world.leaver_view()?;
    let leaver_keys_after = world.leaver_key_probes()?;
    world.save_all()?;

    Ok(RunReport {
        task: 6818,
        stage: stage.to_string(),
        leaver,
        models,
        leave_plan,
        departures,
        missing_activations,
        restart_rosters,
        post_restart_distribution,
        content,
        leaver_view,
        leaver_keys_after,
    })
}

fn leave_label(kind: PlaceKind) -> String {
    match kind {
        PlaceKind::Group => "Leave group".to_string(),
        PlaceKind::Enclave => "Leave enclave".to_string(),
    }
}
