//! The ownership boundary on departure.
//!
//! A place can be left freely right up to the point where leaving would strip
//! the place of the authority that keeps it governable. At that point departure
//! is refused until the authority is transferred.
//!
//! The decision is taken from role *ids* and the capability set attached to
//! them. `RoleGrant::label` is display text: it is not read here, and the
//! module's public API gives no way to make it matter. That is deliberate — a
//! place that ships a role called "Owner", or a member who renames their role
//! to "Owner", must not thereby acquire or lose the ability to leave.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::ids::{MemberId, RoleId};

/// A governance capability a role can carry.
///
/// These are capabilities, never read grants: holding one does not hand a
/// member a channel key. The departure gate is concerned with exactly one of
/// them.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    /// The authority that keeps a place governable: roster changes, role
    /// changes and channel changes all ultimately require a holder.
    GovernPlace,
    ManageChannels,
    ModerateMembers,
}

/// The authority a departure must not orphan.
pub const REQUIRED_DEPARTURE_AUTHORITY: Authority = Authority::GovernPlace;

/// One role as the place defines it.
///
/// `label` exists only so a member list can show something readable. Nothing
/// in this module branches on it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RoleGrant {
    pub id: RoleId,
    /// Display text. Renaming it does not move authority; see the module note.
    pub label: String,
    pub authorities: BTreeSet<Authority>,
}

impl RoleGrant {
    pub fn new(id: RoleId, label: impl Into<String>, authorities: BTreeSet<Authority>) -> Self {
        Self {
            id,
            label: label.into(),
            authorities,
        }
    }

    /// The only place a role's authority is read from.
    fn grants(&self, authority: Authority) -> bool {
        self.authorities.contains(&authority)
    }
}

/// The place's role table, keyed by the stable role id.
pub type RoleTable = BTreeMap<RoleId, RoleGrant>;

/// Which members hold which roles.
pub type RoleHolders = BTreeMap<MemberId, BTreeSet<RoleId>>;

/// The verdict for one member's departure.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum DepartureAuthority {
    /// The member holds no role id carrying the required authority.
    Clear,
    /// The member holds it, and so does at least one other member.
    Redundant {
        held_role_ids: Vec<RoleId>,
        other_holders: Vec<MemberId>,
    },
    /// The member is the last holder. Departure is refused until the authority
    /// is transferred.
    LastHolder { held_role_ids: Vec<RoleId> },
}

impl DepartureAuthority {
    pub fn permits_departure(&self) -> bool {
        !matches!(self, Self::LastHolder { .. })
    }

    /// Machine-stable reason string used in refusals and in the interface.
    pub fn refusal_code(&self) -> Option<&'static str> {
        match self {
            Self::LastHolder { .. } => Some("last-authority-holder"),
            _ => None,
        }
    }
}

/// Role ids held by `member` that carry `authority`.
fn authority_role_ids(
    roles: &RoleTable,
    holders: &RoleHolders,
    member: MemberId,
    authority: Authority,
) -> Vec<RoleId> {
    let mut held: Vec<RoleId> = holders
        .get(&member)
        .map(|ids| {
            ids.iter()
                .copied()
                .filter(|id| roles.get(id).is_some_and(|role| role.grants(authority)))
                .collect()
        })
        .unwrap_or_default();
    held.sort();
    held
}

/// Evaluates whether `leaver` may depart without orphaning the place.
///
/// `roles` supplies the capability set per role id; `holders` supplies the
/// membership of each role. Neither the role label nor any display name
/// reaches the decision.
pub fn evaluate_departure(
    roles: &RoleTable,
    holders: &RoleHolders,
    leaver: MemberId,
) -> DepartureAuthority {
    let held_role_ids = authority_role_ids(roles, holders, leaver, REQUIRED_DEPARTURE_AUTHORITY);
    if held_role_ids.is_empty() {
        return DepartureAuthority::Clear;
    }

    let mut other_holders: Vec<MemberId> = holders
        .keys()
        .copied()
        .filter(|member| *member != leaver)
        .filter(|member| {
            !authority_role_ids(roles, holders, *member, REQUIRED_DEPARTURE_AUTHORITY).is_empty()
        })
        .collect();
    other_holders.sort();

    if other_holders.is_empty() {
        DepartureAuthority::LastHolder { held_role_ids }
    } else {
        DepartureAuthority::Redundant {
            held_role_ids,
            other_holders,
        }
    }
}

/// Grants `role_id` to `member`: the transfer that unblocks a refused
/// departure. Refused if the role id is not one the place defines, because a
/// place must not acquire authority from a role it never declared.
pub fn grant_role(
    roles: &RoleTable,
    holders: &mut RoleHolders,
    member: MemberId,
    role_id: RoleId,
) -> Result<(), AuthorityError> {
    if !roles.contains_key(&role_id) {
        return Err(AuthorityError::UnknownRole);
    }
    holders.entry(member).or_default().insert(role_id);
    Ok(())
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum AuthorityError {
    #[error("this place does not define that role id")]
    UnknownRole,
}

/// Returns a copy of the role table with every label replaced.
///
/// Used by the check to prove label-blindness: the verdict computed over the
/// relabelled table has to be byte-identical to the shipped one.
pub fn relabel(roles: &RoleTable, labels: &BTreeMap<RoleId, String>) -> RoleTable {
    roles
        .iter()
        .map(|(id, role)| {
            let mut renamed = role.clone();
            if let Some(label) = labels.get(id) {
                renamed.label = label.clone();
            }
            (*id, renamed)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(seed: u8) -> MemberId {
        let secret = crypto::ed25519::SecretKey::from_bytes([seed; 32]);
        MemberId::from_identity_key(&crypto::ed25519::derive_public(&secret))
    }

    fn table() -> (RoleTable, RoleHolders) {
        let warden = RoleId::derive("quarry-enclave", "warden");
        let titled = RoleId::derive("quarry-enclave", "titled");
        let plain = RoleId::derive("quarry-enclave", "member");
        let mut roles = RoleTable::new();
        roles.insert(
            warden,
            RoleGrant::new(warden, "Warden", BTreeSet::from([Authority::GovernPlace])),
        );
        // A role that ships with the most authoritative-sounding label in the
        // product and carries no authority at all.
        roles.insert(titled, RoleGrant::new(titled, "Owner", BTreeSet::new()));
        roles.insert(plain, RoleGrant::new(plain, "Member", BTreeSet::new()));

        let mut holders = RoleHolders::new();
        holders.insert(member(1), BTreeSet::from([warden]));
        holders.insert(member(2), BTreeSet::from([titled]));
        holders.insert(member(3), BTreeSet::from([plain]));
        (roles, holders)
    }

    #[test]
    fn the_last_authority_holder_is_refused_and_a_transfer_releases_them() {
        let (roles, mut holders) = table();
        let warden = RoleId::derive("quarry-enclave", "warden");

        assert_eq!(
            evaluate_departure(&roles, &holders, member(1)),
            DepartureAuthority::LastHolder {
                held_role_ids: vec![warden]
            }
        );
        assert_eq!(
            evaluate_departure(&roles, &holders, member(2)),
            DepartureAuthority::Clear,
            "a role labelled Owner with no authority is not an authority holder"
        );

        grant_role(&roles, &mut holders, member(2), warden).unwrap();
        assert_eq!(
            evaluate_departure(&roles, &holders, member(1)),
            DepartureAuthority::Redundant {
                held_role_ids: vec![warden],
                other_holders: vec![member(2)],
            }
        );
    }

    #[test]
    fn renaming_every_label_changes_no_verdict() {
        let (roles, holders) = table();
        let warden = RoleId::derive("quarry-enclave", "warden");
        let titled = RoleId::derive("quarry-enclave", "titled");
        let plain = RoleId::derive("quarry-enclave", "member");
        let renamed = relabel(
            &roles,
            &BTreeMap::from([
                (warden, "Member".to_string()),
                (titled, "Steward".to_string()),
                (plain, "Owner".to_string()),
            ]),
        );

        for who in [member(1), member(2), member(3)] {
            assert_eq!(
                evaluate_departure(&roles, &holders, who),
                evaluate_departure(&renamed, &holders, who),
                "the departure gate must not read a role label"
            );
        }
    }

    #[test]
    fn an_undeclared_role_id_cannot_be_granted() {
        let (roles, mut holders) = table();
        assert_eq!(
            grant_role(
                &roles,
                &mut holders,
                member(2),
                RoleId::derive("elsewhere", "warden")
            ),
            Err(AuthorityError::UnknownRole)
        );
    }
}
