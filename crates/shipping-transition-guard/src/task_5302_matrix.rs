//! Executable acceptance matrix for TASK 5302.

use std::collections::BTreeSet;

use ed25519_dalek::SigningKey;

use crate::shipping_transition_guard::{
    Authority, AuthorityClass, ShippingStateKind, ShippingStateSnapshot, ShippingTransitionGuard,
    SignedShippingAction, TransitionRefusal, TRANSITION_GATES,
};

#[derive(Clone, Debug)]
pub struct TransitionRowReport {
    pub gate: u32,
    pub name: &'static str,
    pub external_ids: Vec<String>,
    pub stale_signer: String,
    pub current_signer: String,
    pub before_epoch: u64,
    pub after_epoch: u64,
    pub before_grant: String,
    pub after_grant: String,
    pub hostile_actions: usize,
    pub hostile_state_changes: usize,
    pub control_state_changes: usize,
    pub verifier_calls: usize,
}

#[derive(Clone, Debug)]
pub struct TransitionMatrixReport {
    pub rows: Vec<TransitionRowReport>,
    pub hostile_actions: usize,
    pub hostile_state_changes: usize,
    pub control_state_changes: usize,
    pub verifier_calls: usize,
    pub final_state: ShippingStateSnapshot,
}

#[derive(Clone, Copy)]
struct RowSpec {
    gate: u32,
    name: &'static str,
    class: AuthorityClass,
    state_kind: ShippingStateKind,
    stale_seed: u8,
    current_seed: u8,
    before_epoch: u64,
    after_epoch: u64,
    before_grant: &'static str,
    after_grant: &'static str,
    hostile_kinds: &'static [ShippingStateKind],
    hostile_ids: &'static [&'static str],
}

const MESSAGE_ONLY: &[ShippingStateKind] = &[ShippingStateKind::Message];
const FILE_ONLY: &[ShippingStateKind] = &[ShippingStateKind::File];
const ENTITLEMENT_ONLY: &[ShippingStateKind] = &[ShippingStateKind::Entitlement];
const MAILBOX_ONLY: &[ShippingStateKind] = &[ShippingStateKind::Mailbox];
const ROSTER_ONLY: &[ShippingStateKind] = &[ShippingStateKind::Roster];
const UPDATE_ONLY: &[ShippingStateKind] = &[ShippingStateKind::Update];
const CARRIER_ONLY: &[ShippingStateKind] = &[ShippingStateKind::CarrierTable];
const STATE_AND_MESSAGE: &[ShippingStateKind] =
    &[ShippingStateKind::Roster, ShippingStateKind::Message];

const ROWS: [RowSpec; 10] = [
    RowSpec {
        gate: 3558,
        name: "removed-friend-message-and-pointer",
        class: AuthorityClass::RemovedFriend,
        state_kind: ShippingStateKind::Message,
        stale_seed: 11,
        current_seed: 12,
        before_epoch: 41,
        after_epoch: 42,
        before_grant: "friend/reach/removed/3558",
        after_grant: "friend/reach/accepted-control/3558",
        hostile_kinds: &[ShippingStateKind::Message, ShippingStateKind::Message],
        hostile_ids: &["ext-3558-hostile-fresh", "ptr-3558-before-removal"],
    },
    RowSpec {
        gate: 3728,
        name: "expired-pro-upload",
        class: AuthorityClass::ProGrant,
        state_kind: ShippingStateKind::File,
        stale_seed: 21,
        current_seed: 21,
        before_epoch: 7,
        after_epoch: 8,
        before_grant: "pro/grant/3728/last-valid",
        after_grant: "pro/grant/3728/live-control",
        hostile_kinds: FILE_ONLY,
        hostile_ids: &["upload-3728-hostile-higher-sequence"],
    },
    RowSpec {
        gate: 3730,
        name: "expired-pro-send",
        class: AuthorityClass::ProGrant,
        state_kind: ShippingStateKind::Message,
        stale_seed: 22,
        current_seed: 22,
        before_epoch: 17,
        after_epoch: 18,
        before_grant: "pro/grant/3730/last-valid",
        after_grant: "pro/grant/3730/live-control",
        hostile_kinds: MESSAGE_ONLY,
        hostile_ids: &["message-3730-hostile-higher-sequence"],
    },
    RowSpec {
        gate: 3732,
        name: "expired-pro-autoscrub",
        class: AuthorityClass::ProGrant,
        state_kind: ShippingStateKind::Entitlement,
        stale_seed: 23,
        current_seed: 23,
        before_epoch: 27,
        after_epoch: 28,
        before_grant: "pro/grant/3732/last-valid",
        after_grant: "pro/grant/3732/live-control",
        hostile_kinds: ENTITLEMENT_ONLY,
        hostile_ids: &["autoscrub-3732-hostile-higher-sequence"],
    },
    RowSpec {
        gate: 3789,
        name: "expired-pro-lapsed-file-read",
        class: AuthorityClass::ProGrant,
        state_kind: ShippingStateKind::File,
        stale_seed: 24,
        current_seed: 24,
        before_epoch: 37,
        after_epoch: 38,
        before_grant: "pro/grant/3789/last-valid",
        after_grant: "pro/grant/3789/live-control",
        hostile_kinds: FILE_ONLY,
        hostile_ids: &["file-read-3789-hostile-higher-sequence"],
    },
    RowSpec {
        gate: 3983,
        name: "old-friend-key-state-and-message",
        class: AuthorityClass::RotatedFriendKey,
        state_kind: ShippingStateKind::Roster,
        stale_seed: 31,
        current_seed: 32,
        before_epoch: 51,
        after_epoch: 52,
        before_grant: "friend/key/3983/old",
        after_grant: "friend/key/3983/current",
        hostile_kinds: STATE_AND_MESSAGE,
        hostile_ids: &[
            "state-3983-old-key-version-9000",
            "message-3983-old-key-seq-9001",
        ],
    },
    RowSpec {
        gate: 4354,
        name: "signed-out-mailbox-stale-token",
        class: AuthorityClass::MailboxToken,
        state_kind: ShippingStateKind::Mailbox,
        stale_seed: 41,
        current_seed: 41,
        before_epoch: 61,
        after_epoch: 62,
        before_grant: "mailbox/token/4354/signed-out",
        after_grant: "mailbox/token/4354/refreshed",
        hostile_kinds: MAILBOX_ONLY,
        hostile_ids: &["mailbox-read-4354-stale-token-new-request"],
    },
    RowSpec {
        gate: 5168,
        name: "compromised-account-root-roster",
        class: AuthorityClass::RevokedSigner,
        state_kind: ShippingStateKind::Roster,
        stale_seed: 51,
        current_seed: 52,
        before_epoch: 70,
        after_epoch: 71,
        before_grant: "account/root/5168/compromised",
        after_grant: "account/root/5168/replacement",
        hostile_kinds: ROSTER_ONLY,
        hostile_ids: &["roster-5168-compromised-root-version-9999"],
    },
    RowSpec {
        gate: 5170,
        name: "revoked-release-signer-update",
        class: AuthorityClass::RevokedSigner,
        state_kind: ShippingStateKind::Update,
        stale_seed: 61,
        current_seed: 62,
        before_epoch: 80,
        after_epoch: 81,
        before_grant: "release/signer/5170/revoked",
        after_grant: "release/signer/5170/replacement",
        hostile_kinds: UPDATE_ONLY,
        hostile_ids: &["update-5170-revoked-signer-version-10000"],
    },
    RowSpec {
        gate: 5171,
        name: "revoked-release-signer-carrier-table",
        class: AuthorityClass::RevokedSigner,
        state_kind: ShippingStateKind::CarrierTable,
        stale_seed: 71,
        current_seed: 72,
        before_epoch: 90,
        after_epoch: 91,
        before_grant: "release/signer/5171/revoked",
        after_grant: "release/signer/5171/replacement",
        hostile_kinds: CARRIER_ONLY,
        hostile_ids: &["carrier-table-5171-revoked-signer-version-11000"],
    },
];

pub fn run_matrix() -> Result<TransitionMatrixReport, String> {
    validate_fixed_inventory()?;
    let mut guard = ShippingTransitionGuard::new();

    for row in ROWS {
        let stale_secret = key(row.stale_seed);
        let current_secret = key(row.current_seed);
        guard.install_transition(
            row.gate,
            row.class,
            authority(&stale_secret, row.before_grant, row.before_epoch),
            authority(&current_secret, row.after_grant, row.after_epoch),
            row.state_kind,
            100,
        )?;
        for kind in row.hostile_kinds {
            guard.allow_shipping_entry(row.gate, *kind)?;
        }
    }
    guard.record_pre_transition_external_id("ptr-3558-before-removal");

    let mut reports = Vec::with_capacity(ROWS.len());
    let mut hostile_actions = 0usize;
    let mut hostile_state_changes = 0usize;
    let mut control_state_changes = 0usize;
    let mut hostile_failures = Vec::new();

    for row in ROWS {
        let stale_secret = key(row.stale_seed);
        let current_secret = key(row.current_seed);
        let stale_authority = authority(&stale_secret, row.before_grant, row.before_epoch);
        let current_authority = authority(&current_secret, row.after_grant, row.after_epoch);
        let calls_before = guard.verifier_calls();
        let mut ids = Vec::new();

        for (index, (kind, external_id)) in row
            .hostile_kinds
            .iter()
            .zip(row.hostile_ids.iter())
            .enumerate()
        {
            let before = guard.snapshot();
            let action = SignedShippingAction::sign(
                row.gate,
                *external_id,
                row.before_grant,
                row.before_epoch,
                10_000 + index as u64,
                *kind,
                format!("hostile:{}:{external_id}", row.name).as_bytes(),
                &stale_secret,
            );
            ids.push(action.external_id.clone());
            let refusal = guard.submit(&action);
            let after = guard.snapshot();
            let changed = state_delta(&before, &after);
            if refusal.is_ok() || changed != 0 {
                hostile_failures.push(format!(
                    "TASK5302 accepted stale authority gate={} hostile_signer={} stale_authority={} changed_state={}:{} external_id={}",
                    row.gate,
                    action.signer_fingerprint(),
                    row.before_grant,
                    kind.name(),
                    changed,
                    action.external_id,
                ));
            } else {
                match refusal.expect_err("checked refusal") {
                    TransitionRefusal::StaleAuthority {
                        signer,
                        stale_credential,
                        ..
                    } if signer == action.signer_fingerprint()
                        && stale_credential == row.before_grant => {}
                    other => {
                        hostile_failures.push(format!(
                            "TASK5302 hostile action missed stale-authority verifier gate={} signer={} stale_authority={} refusal={other:?}",
                            row.gate,
                            action.signer_fingerprint(),
                            row.before_grant,
                        ));
                    }
                }
            }
            hostile_actions += 1;
            hostile_state_changes += changed;
        }

        let before_control = guard.snapshot();
        let control_id = format!("control-{}-current-authority", row.gate);
        let control = SignedShippingAction::sign(
            row.gate,
            &control_id,
            row.after_grant,
            row.after_epoch,
            20_000,
            row.state_kind,
            format!("current-control:{}", row.name).as_bytes(),
            &current_secret,
        );
        guard.submit(&control).map_err(|error| {
            format!(
                "TASK5302 current control refused gate={} signer={} authority={} error={error:?}",
                row.gate,
                control.signer_fingerprint(),
                row.after_grant,
            )
        })?;
        let after_control = guard.snapshot();
        let control_delta = state_delta(&before_control, &after_control);
        if control_delta != 1 {
            return Err(format!(
                "TASK5302 current control changed wrong state gate={} expected=1 actual={control_delta}",
                row.gate
            ));
        }
        control_state_changes += control_delta;
        ids.push(control_id);

        reports.push(TransitionRowReport {
            gate: row.gate,
            name: row.name,
            external_ids: ids,
            stale_signer: stale_authority.fingerprint(),
            current_signer: current_authority.fingerprint(),
            before_epoch: row.before_epoch,
            after_epoch: row.after_epoch,
            before_grant: row.before_grant.to_owned(),
            after_grant: row.after_grant.to_owned(),
            hostile_actions: row.hostile_ids.len(),
            hostile_state_changes: 0,
            control_state_changes: control_delta,
            verifier_calls: guard.verifier_calls() - calls_before,
        });
    }

    if !hostile_failures.is_empty() {
        return Err(format!(
            "{} current_controls={} hostile_classes_observed=5",
            hostile_failures.join("; "),
            control_state_changes,
        ));
    }

    if reports.len() != 10 || hostile_actions != 12 || hostile_state_changes != 0 {
        return Err(format!(
            "TASK5302 matrix invariant failed rows={} hostile_actions={} hostile_state_changes={}",
            reports.len(),
            hostile_actions,
            hostile_state_changes
        ));
    }

    Ok(TransitionMatrixReport {
        rows: reports,
        hostile_actions,
        hostile_state_changes,
        control_state_changes,
        verifier_calls: guard.verifier_calls(),
        final_state: guard.snapshot(),
    })
}

fn validate_fixed_inventory() -> Result<(), String> {
    let actual = ROWS.iter().map(|row| row.gate).collect::<Vec<_>>();
    if actual != TRANSITION_GATES {
        return Err(format!(
            "TASK5302 transition inventory mismatch expected={TRANSITION_GATES:?} actual={actual:?}"
        ));
    }
    let unique = actual.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != 10 {
        return Err(format!(
            "TASK5302 transition inventory is not unique: {actual:?}"
        ));
    }
    Ok(())
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn authority(secret: &SigningKey, credential: &str, epoch: u64) -> Authority {
    Authority {
        public_key: secret.verifying_key(),
        credential: credential.to_owned(),
        epoch,
    }
}

fn state_delta(before: &ShippingStateSnapshot, after: &ShippingStateSnapshot) -> usize {
    ShippingStateKind::ALL
        .into_iter()
        .map(|kind| after.count(kind).saturating_sub(before.count(kind)))
        .sum()
}
