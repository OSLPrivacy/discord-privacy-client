//! TASK 6856 — customisable Enclave categories, channels, modes and overrides.
//!
//! This is the acceptance check for the finish line. It is written as one test
//! so the whole story runs in order: three clients edit concurrently, reconnect,
//! converge, restart, and are then asked every mode and override cell as signed
//! read/post/manage actions.
//!
//! The check carries a **coverage ledger**. Every dimension the finish line
//! names — each structural edit, each concurrent client, each collapse profile,
//! each shipped mode, each custom role, every permission cell, every deletion
//! case, the restart and the resource boundary — is recorded only after the
//! check has actually verified it. The final assertion fails, and the process
//! exits 1, if any required entry is absent. Starving the check therefore makes
//! it red rather than quietly shrinking it.
//!
//! `OSL_TASK_6856_STARVE=<tag>` removes exactly one of those dimensions, which
//! is how the starvation claim itself is demonstrated rather than asserted.
//! `OSL_TASK_6856_STARVE=visual-reorder` moves the rendered list without
//! emitting a signed reorder operation; the convergence assertion then fails,
//! which is what "visual-only reordering makes it red" means here.

use crypto::ed25519::SecretKey;
use crate::enclave_layout::{
    load_collapse_profile, measure_layout_limit, save_collapse_profile, AccessSource, Category,
    CategoryId, Channel, ChannelAction, ChannelDisposition, ChannelId, ChannelMode, CollapseProfile,
    EnclaveClient, EnclaveLayout, EnclaveLayoutId, LayoutBudget, LayoutError, LayoutOp, MessageId,
    OverrideBit, RejectionReason, RoleId, RoleOverride,
};
use crate::space_roster::SpaceMemberId;
use std::collections::{BTreeMap, BTreeSet};
use tempfile::TempDir;

// ---------------------------------------------------------------- ledger ---

#[derive(Default)]
struct Ledger {
    seen: BTreeSet<String>,
}

impl Ledger {
    fn record(&mut self, dimension: &str, item: impl AsRef<str>) {
        self.seen.insert(format!("{dimension}:{}", item.as_ref()));
    }

    fn missing(&self, required: &[String]) -> Vec<String> {
        required
            .iter()
            .filter(|tag| !self.seen.contains(*tag))
            .cloned()
            .collect()
    }
}

fn starve_tag() -> String {
    std::env::var("OSL_TASK_6856_STARVE").unwrap_or_default()
}

fn starved(tag: &str) -> bool {
    starve_tag() == tag
}

// ------------------------------------------------------------ the spec -----

/// The acceptance table, written here independently of the implementation.
/// If the resolver disagrees with this, the check is red.
fn spec_base(mode: ChannelMode, action: ChannelAction, authority: bool) -> bool {
    match (mode, action) {
        (ChannelMode::Open, ChannelAction::Read | ChannelAction::Post) => true,
        (ChannelMode::ReadOnly, ChannelAction::Read) => true,
        (ChannelMode::ReadOnly, ChannelAction::Post) => authority,
        (ChannelMode::Stewards, ChannelAction::Read | ChannelAction::Post) => authority,
        (_, ChannelAction::Manage) => authority,
    }
}

fn spec_cell(
    mode: ChannelMode,
    action: ChannelAction,
    bit: Option<OverrideBit>,
    authority: bool,
) -> bool {
    match bit {
        Some(OverrideBit::Allow) => true,
        Some(OverrideBit::Deny) => false,
        None => spec_base(mode, action, authority),
    }
}

fn bit_token(bit: Option<OverrideBit>) -> &'static str {
    match bit {
        None => "inherit",
        Some(OverrideBit::Allow) => "allow",
        Some(OverrideBit::Deny) => "deny",
    }
}

// ------------------------------------------------------------- helpers -----

fn key(seed: u8) -> SecretKey {
    SecretKey::from_bytes([seed; 32])
}

struct Ids {
    next: u64,
}

impl Ids {
    fn bytes(&mut self) -> [u8; 16] {
        let n = self.next;
        self.next += 1;
        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&n.to_be_bytes());
        bytes[8..].copy_from_slice(&(n ^ 0x5555_5555_5555_5555).to_be_bytes());
        bytes
    }

    fn category(&mut self) -> CategoryId {
        CategoryId::from_bytes(self.bytes())
    }

    fn channel(&mut self) -> ChannelId {
        ChannelId::from_bytes(self.bytes())
    }

    fn role(&mut self) -> RoleId {
        RoleId::from_bytes(self.bytes())
    }

    fn message(&mut self) -> MessageId {
        MessageId::from_bytes(self.bytes())
    }
}

/// Every replica exchanges logs with every other, which is what "reconnect"
/// means here: no client is privileged and no order is assumed.
fn reconnect(clients: &mut [&mut EnclaveClient]) {
    let logs: Vec<_> = clients.iter().map(|client| client.log().clone()).collect();
    for client in clients.iter_mut() {
        for log in &logs {
            client.sync_from(log).expect("same enclave, same founder");
        }
    }
    // Second pass so operations learned in the first pass propagate onward.
    let logs: Vec<_> = clients.iter().map(|client| client.log().clone()).collect();
    for client in clients.iter_mut() {
        for log in &logs {
            client.sync_from(log).expect("same enclave, same founder");
        }
    }
}

fn tree_names(layout: &EnclaveLayout) -> Vec<(String, Vec<String>)> {
    layout
        .ordered_categories()
        .into_iter()
        .map(|category: &Category| {
            (
                category.name.clone(),
                layout
                    .ordered_channels(category.id)
                    .into_iter()
                    .map(|channel: &Channel| channel.name.clone())
                    .collect(),
            )
        })
        .collect()
}

fn position_snapshot(layout: &EnclaveLayout) -> BTreeMap<String, i64> {
    let mut snapshot = BTreeMap::new();
    for category in layout.categories().values() {
        snapshot.insert(format!("category:{}", category.id.hex()), category.position);
    }
    for channel in layout.channels().values() {
        snapshot.insert(format!("channel:{}", channel.id.hex()), channel.position);
    }
    snapshot
}

fn member_hex(member: SpaceMemberId) -> String {
    hex::encode(member.as_bytes())[..12].to_owned()
}

/// One cell of the permission grid, kept so the whole grid can be re-asked
/// after the restart.
struct Cell {
    channel: ChannelId,
    mode: ChannelMode,
    action: ChannelAction,
    bit: Option<OverrideBit>,
    expected: bool,
    seed_message: MessageId,
}

#[test]
#[allow(clippy::too_many_lines)]
fn enclave_layout_is_customisable_converges_and_enforces() {
    let mut ledger = Ledger::default();
    let mut required: Vec<String> = Vec::new();
    let mut ids = Ids { next: 1 };
    let enclave = EnclaveLayoutId::from_bytes([0x6a; EnclaveLayoutId::LENGTH]);

    eprintln!("TASK 6856 check — starve tag: {:?}", starve_tag());

    // ---------------------------------------------------------------- cast --
    let mut ada = EnclaveClient::founder(enclave, key(0xA1));
    let founder_key = *ada.public_key();
    let mut bo = EnclaveClient::joining(enclave, founder_key, key(0xB2));
    let mut cy = EnclaveClient::joining(enclave, founder_key, key(0xC3));
    let mut dee = EnclaveClient::joining(enclave, founder_key, key(0xD4));
    let mut eve = EnclaveClient::joining(enclave, founder_key, key(0xE5));

    for member in [
        *bo.public_key(),
        *cy.public_key(),
        *dee.public_key(),
        *eve.public_key(),
    ] {
        ada.submit(LayoutOp::AdmitMember { key: member })
            .expect("the founder admits members");
    }

    // ------------------------------------------------------- custom roles --
    let steward = ids.role();
    let council = ids.role();
    let guest = ids.role();
    let archivist = ids.role();
    // Starving `custom-role` removes the Archivist outright: the role is never
    // defined and never granted, so the second role Eve holds — the one that
    // makes deny-beats-allow across a member's roles observable — is gone.
    let mut role_roster: Vec<(RoleId, &str, bool)> = vec![
        (steward, "Stewards", true),
        (council, "Council", true),
        (guest, "Guest", false),
    ];
    let mut grants: Vec<(SpaceMemberId, RoleId)> = vec![
        (bo.member(), steward),
        (cy.member(), steward),
        (dee.member(), guest),
        (eve.member(), guest),
    ];
    if !starved("custom-role") {
        role_roster.push((archivist, "Archivist", false));
        grants.push((eve.member(), archivist));
    }
    let custom_role_count = role_roster.len();
    for (role, name, authority) in role_roster {
        ada.submit(LayoutOp::DefineRole {
            role,
            name: name.to_owned(),
            authority,
        })
        .expect("the founder defines roles");
    }
    for (member, role) in grants {
        ada.submit(LayoutOp::GrantRole { member, role })
            .expect("the founder grants roles");
    }

    // ------------------------------- a roster with no 2/5 shape in sight ---
    // Three categories and six channels at rest: already past the ceiling the
    // former fixed roster imposed, before any measured limit is consulted.
    let commons = ids.category();
    let field = ids.category();
    let archive = ids.category();
    for (category, name, position) in [
        (commons, "Common", 0),
        (field, "Field", 1),
        (archive, "Archive", 2),
    ] {
        ada.submit(LayoutOp::CreateCategory {
            category,
            name: name.to_owned(),
            position,
        })
        .expect("the founder creates categories");
    }

    let mut shipped_channels: Vec<ChannelId> = Vec::new();
    for (index, (category, name, mode)) in [
        (commons, "welcome", ChannelMode::Open),
        (commons, "notices", ChannelMode::ReadOnly),
        (commons, "stewardship", ChannelMode::Stewards),
        (field, "field-log", ChannelMode::Open),
        (field, "dispatch", ChannelMode::ReadOnly),
        (archive, "cold-store", ChannelMode::Open),
    ]
    .into_iter()
    .enumerate()
    {
        let channel = ids.channel();
        ada.submit(LayoutOp::CreateChannel {
            channel,
            category,
            name: name.to_owned(),
            mode,
            position: index as i64,
        })
        .expect("the founder creates channels");
        shipped_channels.push(channel);
    }
    let welcome = shipped_channels[0];
    let notices = shipped_channels[1];
    let stewardship = shipped_channels[2];
    let field_log = shipped_channels[3];
    let cold_store = shipped_channels[5];

    {
        let layout = ada.layout();
        assert_eq!(layout.categories().len(), 3, "three categories exist at rest");
        assert_eq!(layout.channels().len(), 6, "six channels exist at rest");
        assert!(layout.rejections().is_empty(), "setup was fully admitted");
    }

    reconnect(&mut [&mut ada, &mut bo, &mut cy, &mut dee, &mut eve]);

    // ------------------------------------- three clients edit concurrently --
    // None of the three syncs until every edit below is signed, so these are
    // genuinely concurrent: each client's Lamport counter advances against a
    // log that does not contain the others' work.
    let new_channel = ids.channel();
    let signals = ids.category();

    if !starved("edit:create") {
        ada.submit(LayoutOp::CreateChannel {
            channel: new_channel,
            category: archive,
            name: "ledger".to_owned(),
            mode: ChannelMode::Stewards,
            position: 7,
        })
        .expect("Ada creates concurrently");
        cy.submit(LayoutOp::CreateCategory {
            category: signals,
            name: "Signals".to_owned(),
            position: 3,
        })
        .expect("Cy creates concurrently");
    }

    if !starved("edit:rename") {
        ada.submit(LayoutOp::RenameCategory {
            category: commons,
            name: "Commons".to_owned(),
        })
        .expect("Ada renames concurrently");
        cy.submit(LayoutOp::RenameChannel {
            channel: field_log,
            name: "field-notes".to_owned(),
        })
        .expect("Cy renames concurrently");
        // A deliberate conflict on one field, to prove last-writer-wins is a
        // property of the operation set and not of who reconnected first.
        ada.submit(LayoutOp::RenameCategory {
            category: field,
            name: "Field Ops".to_owned(),
        })
        .expect("Ada renames the contested category");
        cy.submit(LayoutOp::RenameCategory {
            category: field,
            name: "Fieldwork".to_owned(),
        })
        .expect("Cy renames the contested category");
    }

    if !starved("edit:move") {
        bo.submit(LayoutOp::MoveChannel {
            channel: notices,
            category: archive,
            position: 5,
        })
        .expect("Bo moves concurrently");
    }

    if !starved("edit:delete") {
        cy.submit(LayoutOp::DeleteChannel {
            channel: cold_store,
            disposition: None,
        })
        .expect("Cy deletes an empty channel concurrently");
    }

    if !starved("client") {
        bo.submit(LayoutOp::RenameChannel {
            channel: welcome,
            name: "front-door".to_owned(),
        })
        .expect("Bo renames concurrently");
    }

    // --------------------------------------------- reconnect and converge --
    reconnect(&mut [&mut ada, &mut bo, &mut cy]);
    reconnect(&mut [&mut ada, &mut bo, &mut cy, &mut dee, &mut eve]);

    let layouts = [ada.layout(), bo.layout(), cy.layout()];
    let trees: Vec<_> = layouts.iter().map(EnclaveLayout::ordered_tree).collect();
    let names: Vec<_> = layouts.iter().map(tree_names).collect();
    let memberships: Vec<_> = layouts
        .iter()
        .map(|layout| layout.members().clone())
        .collect();
    let role_sets: Vec<_> = layouts.iter().map(|layout| layout.roles().clone()).collect();

    assert_eq!(trees[0], trees[1], "Ada and Bo converge on order");
    assert_eq!(trees[1], trees[2], "Bo and Cy converge on order");
    assert_eq!(names[0], names[1], "Ada and Bo converge on names");
    assert_eq!(names[1], names[2], "Bo and Cy converge on names");
    assert_eq!(
        memberships[0], memberships[1],
        "Ada and Bo converge on membership"
    );
    assert_eq!(
        memberships[1], memberships[2],
        "Bo and Cy converge on membership"
    );
    assert_eq!(role_sets[0], role_sets[1], "Ada and Bo converge on roles");
    assert_eq!(role_sets[1], role_sets[2], "Bo and Cy converge on roles");
    assert_eq!(
        ada.log().canonical_bytes(),
        bo.log().canonical_bytes(),
        "the signed logs are byte-identical after reconnect"
    );
    assert_eq!(
        bo.log().canonical_bytes(),
        cy.log().canonical_bytes(),
        "the signed logs are byte-identical after reconnect"
    );

    for (name, member) in [
        ("Ada", ada.member()),
        ("Bo", bo.member()),
        ("Cy", cy.member()),
    ] {
        if starved("client") && name == "Bo" {
            continue;
        }
        assert!(
            layouts[0].is_member(member),
            "{name} is in the converged membership"
        );
        ledger.record("client", member_hex(member));
    }
    for member in [&ada, &bo, &cy] {
        required.push(format!("client:{}", member_hex(member.member())));
    }

    // Every concurrent edit is recorded only once all three replicas show it.
    if !starved("edit:create") {
        for layout in &layouts {
            assert!(layout.channel(new_channel).is_some(), "created channel converged");
            assert!(layout.category(signals).is_some(), "created category converged");
        }
        ledger.record("edit", "create");
    }
    if !starved("edit:rename") {
        for layout in &layouts {
            assert_eq!(layout.category(commons).expect("commons").name, "Commons");
            assert_eq!(
                layout.channel(field_log).expect("field log").name,
                "field-notes"
            );
        }
        let contested: BTreeSet<&str> = layouts
            .iter()
            .map(|layout| layout.category(field).expect("field").name.as_str())
            .collect();
        assert_eq!(
            contested.len(),
            1,
            "the contested rename converged on one winner: {contested:?}"
        );
        let winner = *contested.iter().next().expect("one winner");
        assert!(
            winner == "Field Ops" || winner == "Fieldwork",
            "the winner is one of the two submitted names, not a third value"
        );
        ledger.record("edit", "rename");
    }
    if !starved("edit:move") {
        for layout in &layouts {
            assert_eq!(
                layout.channel(notices).expect("notices").category,
                archive,
                "the moved channel converged into its new category"
            );
        }
        ledger.record("edit", "move");
    }
    if !starved("edit:delete") {
        for layout in &layouts {
            assert!(
                layout.channel(cold_store).is_none(),
                "the deleted channel converged as absent"
            );
        }
        ledger.record("edit", "delete");
    }
    for kind in ["create", "rename", "move", "delete", "reorder"] {
        required.push(format!("edit:{kind}"));
    }

    // ------------------------------------ reordering is authoritative only --
    let before_positions = position_snapshot(&ada.layout());
    let before_ops = ada.log().len();
    let reordered_view: Vec<ChannelId> = if starved("visual-reorder") {
        // A visual-only reorder: the rendered list is permuted, no signed
        // operation is produced. Nothing else in this branch changes.
        let mut order: Vec<ChannelId> = ada
            .layout()
            .ordered_channels(commons)
            .into_iter()
            .map(|channel| channel.id)
            .collect();
        assert!(order.len() >= 2, "the category has something to reorder");
        order.swap(0, 1);
        order
    } else {
        let target = ada
            .layout()
            .ordered_channels(commons)
            .first()
            .expect("commons holds a channel")
            .id;
        ada.submit(LayoutOp::ReorderChannel {
            channel: target,
            position: 900,
        })
        .expect("Ada reorders");
        ada.layout()
            .ordered_channels(commons)
            .into_iter()
            .map(|channel| channel.id)
            .collect()
    };

    if !starved("visual-reorder") {
        let after_positions = position_snapshot(&ada.layout());
        let changed: Vec<_> = after_positions
            .iter()
            .filter(|(key, value)| before_positions.get(*key) != Some(*value))
            .map(|(key, _)| key.clone())
            .collect();
        assert_eq!(
            changed.len(),
            1,
            "one reorder changes exactly one authoritative position: {changed:?}"
        );
        assert_eq!(
            ada.log().len(),
            before_ops + 1,
            "one reorder emits exactly one signed operation"
        );
        ledger.record("reorder", "one-signed-position");
    }
    required.push("reorder:one-signed-position".to_owned());

    reconnect(&mut [&mut ada, &mut bo, &mut cy]);
    let replicated: Vec<ChannelId> = bo
        .layout()
        .ordered_channels(commons)
        .into_iter()
        .map(|channel| channel.id)
        .collect();
    assert_eq!(
        replicated, reordered_view,
        "another client must see the same order; a reorder that only moved the \
         rendered list never reaches this replica"
    );
    ledger.record("edit", "reorder");
    ledger.record("reorder", "replicated");
    required.push("reorder:replicated".to_owned());

    // ------------------------------------------ collapse state stays local --
    ada.collapse_mut().collapse(commons);
    bo.collapse_mut().collapse(field);
    bo.collapse_mut().collapse(archive);
    if !starved("collapse-profile") {
        cy.collapse_mut().collapse(archive);
        cy.collapse_mut().expand(archive);
    }

    assert!(ada.collapse().is_collapsed(commons));
    assert!(!ada.collapse().is_collapsed(field));
    assert!(bo.collapse().is_collapsed(field) && bo.collapse().is_collapsed(archive));
    assert!(!bo.collapse().is_collapsed(commons));
    assert!(cy.collapse().collapsed.is_empty(), "Cy folds nothing shut");
    assert_ne!(
        ada.collapse().collapsed,
        bo.collapse().collapsed,
        "collapse state is per member"
    );
    assert_ne!(bo.collapse().collapsed, cy.collapse().collapsed);
    let log_text = String::from_utf8_lossy(&ada.log().canonical_bytes()).into_owned();
    assert!(
        !log_text.contains("collapse"),
        "collapse state never enters the signed log"
    );

    for (name, client) in [("Ada", &ada), ("Bo", &bo), ("Cy", &cy)] {
        if starved("collapse-profile") && name == "Cy" {
            continue;
        }
        ledger.record("collapse-profile", member_hex(client.member()));
    }
    for client in [&ada, &bo, &cy] {
        required.push(format!("collapse-profile:{}", member_hex(client.member())));
    }

    // ------------------------------------ STEWARDS survives a role rename ---
    {
        let layout = ada.layout();
        assert!(
            layout.resolve(stewardship, bo.member(), ChannelAction::Read).allowed,
            "a steward reads a STEWARDS channel"
        );
        assert!(
            !layout.resolve(stewardship, dee.member(), ChannelAction::Read).allowed,
            "a guest does not read a STEWARDS channel"
        );
        assert_eq!(
            layout
                .channel(stewardship)
                .expect("stewardship")
                .mode
                .display_label(),
            "STEWARDS"
        );
        assert!(layout.authority_role_ids().contains(&steward));
        assert!(!layout.authority_role_ids().contains(&guest));
    }
    // Swap the two role names outright. If STEWARDS consulted a name anywhere,
    // this inverts it.
    ada.submit(LayoutOp::RenameRole {
        role: steward,
        name: "Guest".to_owned(),
    })
    .expect("the founder renames a role");
    ada.submit(LayoutOp::RenameRole {
        role: guest,
        name: "Stewards".to_owned(),
    })
    .expect("the founder renames a role");
    {
        let layout = ada.layout();
        assert_eq!(layout.role(steward).expect("steward").name, "Guest");
        assert_eq!(layout.role(guest).expect("guest").name, "Stewards");
        assert!(
            layout.resolve(stewardship, bo.member(), ChannelAction::Read).allowed,
            "STEWARDS still admits the authority-bearing role id after the rename"
        );
        assert!(
            !layout.resolve(stewardship, dee.member(), ChannelAction::Read).allowed,
            "STEWARDS still refuses the non-authority role id after the rename"
        );
        assert_eq!(layout.authority_role_ids(), BTreeSet::from([steward, council]));
        ledger.record("custom-role", "steward-authority-after-rename");
        ledger.record("custom-role", "council-authority");
    }
    required.push("custom-role:steward-authority-after-rename".to_owned());
    required.push("custom-role:council-authority".to_owned());

    // ------------------------------------------- the whole permission grid --
    let grid = ids.category();
    ada.submit(LayoutOp::CreateCategory {
        category: grid,
        name: "Grid".to_owned(),
        position: 50,
    })
    .expect("the founder creates the grid category");

    let mut cells: Vec<Cell> = Vec::new();
    for mode in ChannelMode::SHIPPED {
        if starved(&format!("mode:{}", mode.token())) {
            continue;
        }
        for action in ChannelAction::ALL {
            if starved("permission-cell") && action == ChannelAction::Manage {
                continue;
            }
            for bit in [None, Some(OverrideBit::Allow), Some(OverrideBit::Deny)] {
                let channel = ids.channel();
                ada.submit(LayoutOp::CreateChannel {
                    channel,
                    category: grid,
                    name: format!("{}-{}-{}", mode.token(), action.token(), bit_token(bit)),
                    mode,
                    position: cells.len() as i64,
                })
                .expect("the founder creates a grid channel");
                let seed = ids.message();
                ada.submit(LayoutOp::PostMessage {
                    channel,
                    message: seed,
                    body: "seed".to_owned(),
                })
                .expect("the founder seeds the channel");
                if let Some(bit) = bit {
                    ada.submit(LayoutOp::SetChannelOverride {
                        channel,
                        role: guest,
                        over: RoleOverride::INHERIT.with(action, bit),
                    })
                    .expect("the founder sets one override cell");
                }

                let expected = spec_cell(mode, action, bit, false);
                let layout = ada.layout();
                let decision = layout.resolve(channel, dee.member(), action);
                assert_eq!(
                    decision.allowed,
                    expected,
                    "{} / {} / {} resolved wrongly",
                    mode.display_label(),
                    action.token(),
                    bit_token(bit)
                );
                // Inherited versus overridden must be visible, not inferred.
                if bit.is_some() {
                    assert!(
                        matches!(decision.source, AccessSource::ChannelOverride { role } if role == guest),
                        "an override cell reports the role that overrode it"
                    );
                    assert_eq!(decision.source.label(), "overridden");
                    ledger.record("access-source", "overridden");
                } else {
                    assert!(
                        matches!(decision.source, AccessSource::InheritedMode { mode: m } if m == mode),
                        "an inherited cell reports the mode it inherited"
                    );
                    assert_eq!(decision.source.label(), "inherited");
                    ledger.record("access-source", "inherited");
                }
                // The two actions this cell did not touch still inherit.
                for other in ChannelAction::ALL.into_iter().filter(|other| *other != action) {
                    let untouched = layout.resolve(channel, dee.member(), other);
                    assert_eq!(
                        untouched.allowed,
                        spec_base(mode, other, false),
                        "setting one cell must not move another"
                    );
                    assert!(untouched.source.is_inherited());
                }

                // The same answer, as a signed action.
                signed_action_matches(
                    &mut ada,
                    &mut dee,
                    channel,
                    action,
                    expected,
                    seed,
                    &mut ids,
                    &format!("{}/{}/{}", mode.token(), action.token(), bit_token(bit)),
                );

                cells.push(Cell {
                    channel,
                    mode,
                    action,
                    bit,
                    expected,
                    seed_message: seed,
                });
                ledger.record(
                    "permission-cell",
                    format!("{}/{}/{}", mode.token(), action.token(), bit_token(bit)),
                );
            }
        }
        ledger.record("mode", mode.token());
    }
    for mode in ChannelMode::SHIPPED {
        required.push(format!("mode:{}", mode.token()));
        for action in ChannelAction::ALL {
            for bit in [None, Some(OverrideBit::Allow), Some(OverrideBit::Deny)] {
                required.push(format!(
                    "permission-cell:{}/{}/{}",
                    mode.token(),
                    action.token(),
                    bit_token(bit)
                ));
            }
        }
    }
    required.push("access-source:inherited".to_owned());
    required.push("access-source:overridden".to_owned());

    // ------------------------------- category inheritance and deny-wins -----
    {
        // A category of its own: a category override reaches every channel
        // beneath it, so it must not be set on the grid the cells above use.
        let nest = ids.category();
        ada.submit(LayoutOp::CreateCategory {
            category: nest,
            name: "Nest".to_owned(),
            position: 60,
        })
        .expect("the founder creates the nested category");
        let inherited = ids.channel();
        ada.submit(LayoutOp::CreateChannel {
            channel: inherited,
            category: nest,
            name: "category-inherited".to_owned(),
            mode: ChannelMode::Open,
            position: 500,
        })
        .expect("the founder creates a channel");
        ada.submit(LayoutOp::SetCategoryOverride {
            category: nest,
            role: guest,
            over: RoleOverride::INHERIT.with(ChannelAction::Post, OverrideBit::Deny),
        })
        .expect("the founder sets a category override");
        let layout = ada.layout();
        let decision = layout.resolve(inherited, dee.member(), ChannelAction::Post);
        assert!(!decision.allowed, "a category deny reaches its channels");
        assert!(
            matches!(decision.source, AccessSource::CategoryOverride { role } if role == guest),
            "the category is named as the source"
        );
        assert_eq!(decision.source.label(), "overridden");
        ledger.record("override-scope", "category");

        // A channel-level allow beats the category deny.
        ada.submit(LayoutOp::SetChannelOverride {
            channel: inherited,
            role: guest,
            over: RoleOverride::INHERIT.with(ChannelAction::Post, OverrideBit::Allow),
        })
        .expect("the founder sets a channel override");
        let layout = ada.layout();
        let decision = layout.resolve(inherited, dee.member(), ChannelAction::Post);
        assert!(decision.allowed, "the channel override beats the category");
        assert!(matches!(decision.source, AccessSource::ChannelOverride { .. }));
        ledger.record("override-scope", "channel-beats-category");

        // Clearing it falls back to the category, not to the mode.
        ada.submit(LayoutOp::ClearChannelOverride {
            channel: inherited,
            role: guest,
        })
        .expect("the founder clears a channel override");
        let decision = ada
            .layout()
            .resolve(inherited, dee.member(), ChannelAction::Post);
        assert!(!decision.allowed);
        assert!(matches!(decision.source, AccessSource::CategoryOverride { .. }));
        ledger.record("override-scope", "cleared-falls-back");

        // Eve holds Guest and Archivist. One allow, one deny: deny wins, and
        // every replica names the same role.
        if !starved("custom-role") {
            ada.submit(LayoutOp::SetChannelOverride {
                channel: inherited,
                role: archivist,
                over: RoleOverride::INHERIT.with(ChannelAction::Post, OverrideBit::Allow),
            })
            .expect("the founder sets an archivist override");
        }
        ada.submit(LayoutOp::SetChannelOverride {
            channel: inherited,
            role: guest,
            over: RoleOverride::INHERIT.with(ChannelAction::Post, OverrideBit::Deny),
        })
        .expect("the founder sets a guest override");
        reconnect(&mut [&mut ada, &mut bo, &mut cy, &mut eve]);
        for layout in [ada.layout(), bo.layout(), cy.layout()] {
            let decision = layout.resolve(inherited, eve.member(), ChannelAction::Post);
            assert!(!decision.allowed, "deny beats allow across a member's roles");
            assert!(
                matches!(decision.source, AccessSource::ChannelOverride { role } if role == guest || role == archivist)
            );
        }
        signed_action_matches(
            &mut ada,
            &mut eve,
            inherited,
            ChannelAction::Post,
            false,
            MessageId::from_bytes([0; 16]),
            &mut ids,
            "deny-beats-allow",
        );
        ledger.record("custom-role", "guest-override");
        if !starved("custom-role") {
            ledger.record("custom-role", "archivist-deny-beats-allow");
        }

        // The permission grid the product renders covers every custom role.
        let rows = ada.layout().permission_grid(inherited);
        assert_eq!(
            rows.len(),
            custom_role_count,
            "the grid shows every custom role"
        );
        for row in &rows {
            assert_eq!(row.cells.len(), 3, "every role has read, post and manage");
            for cell in &row.cells {
                assert!(cell.source.is_inherited() || cell.source.is_overridden());
            }
        }
        ledger.record("custom-role", "grid-lists-every-role");
    }
    for tag in [
        "override-scope:category",
        "override-scope:channel-beats-category",
        "override-scope:cleared-falls-back",
        "custom-role:guest-override",
        "custom-role:archivist-deny-beats-allow",
        "custom-role:grid-lists-every-role",
    ] {
        required.push(tag.to_owned());
    }

    // -------------------------------------------------- deletion is safe ----
    {
        let doomed = ids.channel();
        let haven = ids.channel();
        for (channel, name) in [(doomed, "doomed"), (haven, "haven")] {
            ada.submit(LayoutOp::CreateChannel {
                channel,
                category: grid,
                name: name.to_owned(),
                mode: ChannelMode::Open,
                position: 600,
            })
            .expect("the founder creates a channel");
        }
        let kept: Vec<MessageId> = (0..3)
            .map(|index| {
                let message = ids.message();
                ada.submit(LayoutOp::PostMessage {
                    channel: doomed,
                    message,
                    body: format!("keep {index}"),
                })
                .expect("the founder posts");
                message
            })
            .collect();

        if !starved("deletion-case:refuse-plain") {
            let refusal = ada.delete_channel(doomed, None).expect_err("refused");
            match &refusal {
                LayoutError::DestinationRequired {
                    channel_name,
                    messages,
                } => {
                    assert_eq!(channel_name, "doomed");
                    assert_eq!(*messages, 3);
                }
                other => panic!("unexpected refusal: {other}"),
            }
            assert!(
                refusal.to_string().contains("Choose a channel to move them to, or burn them on purpose"),
                "the refusal says what the member must choose: {refusal}"
            );
            // The same refusal must hold when the operation is signed directly,
            // so the safety is not a UI check a client can skip.
            let before = ada.log().len();
            ada.submit(LayoutOp::DeleteChannel {
                channel: doomed,
                disposition: None,
            })
            .expect("a client may always sign");
            assert_eq!(ada.log().len(), before + 1);
            let layout = ada.layout();
            assert!(
                layout.channel(doomed).is_some(),
                "the projection refused the unsafe delete"
            );
            assert!(layout.rejections().iter().any(|rejected| matches!(
                rejected.reason,
                RejectionReason::ChannelNotEmpty { messages: 3 }
            )));
            assert_eq!(layout.messages_in(doomed).len(), 3);
            ledger.record("deletion-case", "refuse-plain");
        }

        if !starved("deletion-case:unsafe-destination") {
            let missing = ChannelId::from_bytes([0xFE; 16]);
            assert_eq!(
                ada.delete_channel(
                    doomed,
                    Some(ChannelDisposition::MoveContentTo { channel: missing })
                )
                .expect_err("refused"),
                LayoutError::DestinationNotSafe
            );
            assert_eq!(
                ada.delete_channel(
                    doomed,
                    Some(ChannelDisposition::MoveContentTo { channel: doomed })
                )
                .expect_err("refused"),
                LayoutError::DestinationNotSafe
            );
            assert!(ada.layout().channel(doomed).is_some());
            ledger.record("deletion-case", "unsafe-destination");
        }

        if !starved("deletion-case:move") {
            let before = ada.layout().messages().len();
            ada.delete_channel(
                doomed,
                Some(ChannelDisposition::MoveContentTo { channel: haven }),
            )
            .expect("a safe destination is accepted");
            let layout = ada.layout();
            assert!(layout.channel(doomed).is_none(), "the channel is gone");
            assert_eq!(
                layout.messages().len(),
                before,
                "no message was lost in the move"
            );
            assert_eq!(layout.messages_in(haven).len(), 3);
            for message in kept {
                assert!(
                    layout
                        .messages_in(haven)
                        .iter()
                        .any(|held| held.id == message && held.moved_from.is_some()),
                    "each message arrived, marked with where it came from"
                );
            }
            assert!(
                layout.orphaned_messages().is_empty(),
                "deletion never orphans content"
            );
            ledger.record("deletion-case", "move-to-safe-destination");
        }

        if !starved("deletion-case:burn") {
            let burned_channel = ids.channel();
            ada.submit(LayoutOp::CreateChannel {
                channel: burned_channel,
                category: grid,
                name: "burn-me".to_owned(),
                mode: ChannelMode::Open,
                position: 700,
            })
            .expect("the founder creates a channel");
            let doomed_messages: Vec<MessageId> = (0..2)
                .map(|index| {
                    let message = ids.message();
                    ada.submit(LayoutOp::PostMessage {
                        channel: burned_channel,
                        message,
                        body: format!("burn {index}"),
                    })
                    .expect("the founder posts");
                    message
                })
                .collect();
            let before = ada.layout().messages().len();
            ada.delete_channel(burned_channel, Some(ChannelDisposition::BurnContent))
                .expect("an explicit burn is accepted");
            let layout = ada.layout();
            assert!(layout.channel(burned_channel).is_none());
            assert_eq!(
                layout.messages().len(),
                before - 2,
                "the burn destroyed exactly what it named"
            );
            let burn = layout
                .burns()
                .iter()
                .find(|burn| burn.channel == burned_channel)
                .expect("a burn receipt exists");
            assert_eq!(burn.channel_name, "burn-me");
            assert_eq!(burn.messages, doomed_messages);
            assert!(layout.orphaned_messages().is_empty());
            ledger.record("deletion-case", "explicit-burn");
        }

        if !starved("deletion-case:category") {
            let occupied = ids.category();
            let refuge = ids.category();
            for (category, name, position) in [(occupied, "Occupied", 80), (refuge, "Refuge", 81)] {
                ada.submit(LayoutOp::CreateCategory {
                    category,
                    name: name.to_owned(),
                    position,
                })
                .expect("the founder creates a category");
            }
            let resident = ids.channel();
            ada.submit(LayoutOp::CreateChannel {
                channel: resident,
                category: occupied,
                name: "resident".to_owned(),
                mode: ChannelMode::Open,
                position: 0,
            })
            .expect("the founder creates a channel");

            ada.submit(LayoutOp::DeleteCategory {
                category: occupied,
                channels_to: None,
            })
            .expect("a client may always sign");
            let layout = ada.layout();
            assert!(
                layout.category(occupied).is_some(),
                "a category holding channels is not deleted by default"
            );
            assert!(layout.rejections().iter().any(|rejected| matches!(
                rejected.reason,
                RejectionReason::CategoryNotEmpty { channels: 1 }
            )));
            ledger.record("deletion-case", "category-refuses-orphaning");

            ada.submit(LayoutOp::DeleteCategory {
                category: occupied,
                channels_to: Some(refuge),
            })
            .expect("the founder deletes with a destination");
            let layout = ada.layout();
            assert!(layout.category(occupied).is_none());
            assert_eq!(
                layout.channel(resident).expect("resident survived").category,
                refuge,
                "the channel moved rather than vanishing"
            );
            for channel in layout.channels().values() {
                assert!(
                    layout.category(channel.category).is_some(),
                    "no channel is left in a category that no longer exists"
                );
            }
            ledger.record("deletion-case", "category-move-to-destination");
        }
    }
    for tag in [
        "deletion-case:refuse-plain",
        "deletion-case:unsafe-destination",
        "deletion-case:move-to-safe-destination",
        "deletion-case:explicit-burn",
        "deletion-case:category-refuses-orphaning",
        "deletion-case:category-move-to-destination",
    ] {
        required.push(tag.to_owned());
    }

    // ------------------------------------------------------------ restart ---
    reconnect(&mut [&mut ada, &mut bo, &mut cy, &mut dee, &mut eve]);
    let before_tree = ada.layout().ordered_tree();
    let before_names = tree_names(&ada.layout());
    let before_members = ada.layout().members().clone();

    let device_a = TempDir::new().expect("temp dir");
    let device_b = TempDir::new().expect("temp dir");
    let device_c = TempDir::new().expect("temp dir");
    let device_d = TempDir::new().expect("temp dir");
    ada.save(device_a.path()).expect("save");
    bo.save(device_b.path()).expect("save");
    cy.save(device_c.path()).expect("save");
    dee.save(device_d.path()).expect("save");

    if !starved("restart") {
        let ada_after = EnclaveClient::reopen(device_a.path(), key(0xA1)).expect("reopen");
        let bo_after = EnclaveClient::reopen(device_b.path(), key(0xB2)).expect("reopen");
        let cy_after = EnclaveClient::reopen(device_c.path(), key(0xC3)).expect("reopen");
        let mut dee_after = EnclaveClient::reopen(device_d.path(), key(0xD4)).expect("reopen");

        let restarted = ada_after.layout();
        assert_eq!(restarted.ordered_tree(), before_tree, "order survives restart");
        assert_eq!(tree_names(&restarted), before_names, "names survive restart");
        assert_eq!(
            *restarted.members(),
            before_members,
            "membership survives restart"
        );
        assert_eq!(
            bo_after.layout().ordered_tree(),
            before_tree,
            "every client's order survives restart"
        );
        assert_eq!(cy_after.layout().ordered_tree(), before_tree);
        ledger.record("restart", "log-and-order");

        assert!(ada_after.collapse().is_collapsed(commons));
        assert!(!ada_after.collapse().is_collapsed(field));
        assert!(bo_after.collapse().is_collapsed(field));
        assert!(!bo_after.collapse().is_collapsed(commons));
        assert!(cy_after.collapse().collapsed.is_empty());
        assert_ne!(ada_after.collapse().collapsed, bo_after.collapse().collapsed);
        // A device that reads another member's directory finds nothing of its
        // own, because collapse state is filed per member.
        let cross = load_collapse_profile(device_a.path(), bo.member()).expect("load");
        assert_eq!(
            cross,
            CollapseProfile::default(),
            "one member's device holds no other member's collapse state"
        );
        ledger.record("restart", "collapse-independent");

        // Every cell of the grid answers identically after the restart, and
        // still answers as a signed action rather than only as a query.
        for cell in &cells {
            let decision = restarted.resolve(cell.channel, dee.member(), cell.action);
            assert_eq!(
                decision.allowed,
                cell.expected,
                "{} / {} / {} changed across the restart",
                cell.mode.display_label(),
                cell.action.token(),
                bit_token(cell.bit)
            );
            assert_eq!(decision.source.is_overridden(), cell.bit.is_some());
        }
        ledger.record("restart", "permission-grid");

        let mut ada_after = ada_after;
        let mut checked_signed = 0_usize;
        for cell in cells.iter().filter(|cell| cell.action == ChannelAction::Post) {
            signed_action_matches(
                &mut ada_after,
                &mut dee_after,
                cell.channel,
                ChannelAction::Post,
                cell.expected,
                cell.seed_message,
                &mut ids,
                "after-restart",
            );
            checked_signed += 1;
        }
        assert!(
            checked_signed >= 3,
            "the restart re-ran signed post actions across the grid"
        );
        ledger.record("restart", "signed-actions");
    }
    for tag in [
        "restart:log-and-order",
        "restart:collapse-independent",
        "restart:permission-grid",
        "restart:signed-actions",
    ] {
        required.push(tag.to_owned());
    }

    // ------------------------------------- the signature is load-bearing ---
    if !starved("signature") {
        // A forged operation is refused at the door.
        let forged = ipc::enclave_layout::SignedLayoutOp::sign(
            enclave,
            &key(0xF0),
            9_999,
            LayoutOp::CreateCategory {
                category: ids.category(),
                name: "forged".to_owned(),
                position: 0,
            },
        );
        let mut tampered = forged.clone();
        tampered.op = LayoutOp::CreateCategory {
            category: ids.category(),
            name: "forged".to_owned(),
            position: 1,
        };
        let mut log = ada.log().clone();
        assert_eq!(log.append(tampered).expect_err("refused"), LayoutError::BadSignature);

        // And a layout file edited on disk is refused when it is read back,
        // rather than partially trusted.
        let tamper_dir = TempDir::new().expect("temp dir");
        ada.save(tamper_dir.path()).expect("save");
        let path = ipc::enclave_layout::layout_log_path(tamper_dir.path(), ada.member());
        let raw = std::fs::read_to_string(&path).expect("read");
        // "Grid" is created unconditionally, so this tamper does not depend on
        // any edit a starved run might have skipped.
        let edited = raw.replacen("\"Grid\"", "\"Grab\"", 1);
        assert_ne!(raw, edited, "the tamper actually changed the file");
        std::fs::write(&path, edited).expect("write");
        assert!(
            EnclaveClient::reopen(tamper_dir.path(), key(0xA1)).is_err(),
            "a tampered layout file is refused on reopen"
        );
        ledger.record("signature", "forged-op-refused");
        ledger.record("signature", "tampered-file-refused");
    }
    required.push("signature:forged-op-refused".to_owned());
    required.push("signature:tampered-file-refused".to_owned());

    // -------------------------------------------------- resource boundary ---
    if !starved("resource-boundary") {
        let small = measure_layout_limit(48 * 1024);
        let large = measure_layout_limit(144 * 1024);
        eprintln!("  measured (48 KiB): {}", small.disclosure());
        eprintln!("  measured (144 KiB): {}", large.disclosure());

        assert!(
            small.channels > 5,
            "the measured channel limit is not the old five: {}",
            small.channels
        );
        assert!(
            small.categories > 2,
            "the measured category limit is not the old two: {}",
            small.categories
        );
        assert!(
            large.channels > small.channels,
            "the limit tracks the budget, so it is measured and not a constant \
             ({} at 48 KiB vs {} at 144 KiB)",
            small.channels,
            large.channels
        );
        let ratio = large.channels as f64 / small.channels as f64;
        assert!(
            (2.0..=4.0).contains(&ratio),
            "tripling the budget roughly triples the measurement: ratio {ratio:.2}"
        );
        assert!(small.bytes_per_channel > 0);
        assert!(small.disclosure().contains("measured limit, not a fixed ceiling"));
        assert!(small.disclosure().contains(&small.channels.to_string()));
        ledger.record("resource-boundary", "measured-scales-with-budget");

        // The refusal a real client gives names the number it measured.
        let mut tight = EnclaveClient::founder(
            EnclaveLayoutId::from_bytes([0x77; EnclaveLayoutId::LENGTH]),
            key(0x77),
        )
        .with_budget(LayoutBudget::new(16 * 1024));
        let tight_category = CategoryId::from_bytes([0x01; 16]);
        tight
            .submit(LayoutOp::CreateCategory {
                category: tight_category,
                name: "tight".to_owned(),
                position: 0,
            })
            .expect("the first category fits");
        let mut made = 0_usize;
        let refusal = loop {
            let mut bytes = [0_u8; 16];
            bytes[..8].copy_from_slice(&(made as u64 + 1).to_be_bytes());
            match tight.submit(LayoutOp::CreateChannel {
                channel: ChannelId::from_bytes(bytes),
                category: tight_category,
                name: format!("t{made}"),
                mode: ChannelMode::Open,
                position: made as i64,
            }) {
                Ok(_) => made += 1,
                Err(error) => break error,
            }
        };
        match refusal {
            LayoutError::LogBudgetExhausted {
                measured_channels,
                log_bytes,
                budget_bytes,
            } => {
                assert_eq!(measured_channels, made, "the refusal counts what it built");
                assert!(measured_channels > 5, "measured {measured_channels} channels");
                assert!(log_bytes <= budget_bytes);
                assert_eq!(budget_bytes, 16 * 1024);
                eprintln!(
                    "  tight-budget refusal: {}",
                    LayoutError::LogBudgetExhausted {
                        measured_channels,
                        log_bytes,
                        budget_bytes
                    }
                );
            }
            other => panic!("unexpected refusal: {other}"),
        }
        ledger.record("resource-boundary", "refusal-names-measured-count");

        // And nothing in the ordinary path stops at two categories or five
        // channels: this is the ceiling the redesign removed.
        let mut roomy = EnclaveClient::founder(
            EnclaveLayoutId::from_bytes([0x78; EnclaveLayoutId::LENGTH]),
            key(0x78),
        );
        let mut roomy_ids = Ids { next: 9_000 };
        let mut roomy_categories = Vec::new();
        for index in 0..7 {
            let category = roomy_ids.category();
            roomy
                .submit(LayoutOp::CreateCategory {
                    category,
                    name: format!("cat{index}"),
                    position: index,
                })
                .expect("categories are not capped at two");
            roomy_categories.push(category);
        }
        for index in 0..40 {
            roomy
                .submit(LayoutOp::CreateChannel {
                    channel: roomy_ids.channel(),
                    category: roomy_categories[index % roomy_categories.len()],
                    name: format!("ch{index}"),
                    mode: ChannelMode::SHIPPED[index % 3],
                    position: index as i64,
                })
                .expect("channels are not capped at five");
        }
        let roomy_layout = roomy.layout();
        assert_eq!(roomy_layout.categories().len(), 7);
        assert_eq!(roomy_layout.channels().len(), 40);
        assert!(roomy_layout.rejections().is_empty());
        ledger.record("resource-boundary", "no-two-five-ceiling");
    }
    for tag in [
        "resource-boundary:measured-scales-with-budget",
        "resource-boundary:refusal-names-measured-count",
        "resource-boundary:no-two-five-ceiling",
    ] {
        required.push(tag.to_owned());
    }

    // A collapse profile written by hand is still per member on disk.
    {
        let dir = TempDir::new().expect("temp dir");
        let mut profile = CollapseProfile::default();
        profile.collapse(commons);
        save_collapse_profile(dir.path(), ada.member(), &profile).expect("save");
        assert_eq!(
            load_collapse_profile(dir.path(), ada.member()).expect("load"),
            profile
        );
        assert_eq!(
            load_collapse_profile(dir.path(), bo.member()).expect("load"),
            CollapseProfile::default()
        );
    }

    // ------------------------------------------------------------- ledger ---
    required.sort();
    required.dedup();
    let missing = ledger.missing(&required);
    eprintln!(
        "  covered {}/{} required dimensions",
        required.len() - missing.len(),
        required.len()
    );
    assert!(
        missing.is_empty(),
        "TASK 6856 check is starved — {} of {} required dimensions were never \
         verified: {missing:?}",
        missing.len(),
        required.len()
    );
    eprintln!("TASK 6856 check: {} dimensions verified", required.len());
}

/// Runs `action` as a real signed operation by `actor` and asserts the outcome
/// matches what the resolver said. A permission that only shows in a query is
/// not a permission.
#[allow(clippy::too_many_arguments)]
fn signed_action_matches(
    owner: &mut EnclaveClient,
    actor: &mut EnclaveClient,
    channel: ChannelId,
    action: ChannelAction,
    expected: bool,
    seed: MessageId,
    ids: &mut Ids,
    label: &str,
) {
    actor
        .sync_from(owner.log())
        .expect("the actor catches up first");
    match action {
        ChannelAction::Read => {
            let readable = actor
                .layout()
                .readable_messages(actor.member())
                .iter()
                .any(|message| message.id == seed);
            assert_eq!(
                readable, expected,
                "{label}: reading the seeded message disagreed with the resolver"
            );
        }
        ChannelAction::Post => {
            let message = ids.message();
            actor
                .submit(LayoutOp::PostMessage {
                    channel,
                    message,
                    body: format!("signed {label}"),
                })
                .expect("a client may always sign");
            owner.sync_from(actor.log()).expect("merge");
            let present = owner
                .layout()
                .messages()
                .iter()
                .any(|held| held.id == message);
            assert_eq!(
                present, expected,
                "{label}: the signed post disagreed with the resolver"
            );
        }
        ChannelAction::Manage => {
            let name = format!("managed-{}", ids.next);
            let before = owner
                .layout()
                .channel(channel)
                .map(|existing| existing.name.clone());
            actor
                .submit(LayoutOp::RenameChannel {
                    channel,
                    name: name.clone(),
                })
                .expect("a client may always sign");
            owner.sync_from(actor.log()).expect("merge");
            let after = owner
                .layout()
                .channel(channel)
                .map(|existing| existing.name.clone());
            assert_eq!(
                after == Some(name),
                expected,
                "{label}: the signed rename disagreed with the resolver (was {before:?})"
            );
        }
    }
    owner.sync_from(actor.log()).expect("merge");
}
