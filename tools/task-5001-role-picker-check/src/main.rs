extern crate self as ipc;

pub mod space_roster {
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
    pub struct SpaceChannelId([u8; Self::LENGTH]);
    impl SpaceChannelId {
        pub const LENGTH: usize = 16;
        pub const fn from_bytes(bytes: [u8; Self::LENGTH]) -> Self { Self(bytes) }
    }
}

pub mod atomic_file {
    use std::path::Path;
    pub(crate) fn read_recoverable_bounded(path: &Path, max_bytes: u64, label: &str) -> Result<Option<Vec<u8>>, String> {
        match std::fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() || metadata.len() > max_bytes => Err(format!("{label} is not a bounded regular file")),
            Ok(_) => std::fs::read(path).map(Some).map_err(|_| format!("{label} could not be read")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(format!("{label} metadata could not be read")),
        }
    }
    pub(crate) fn write_recoverable(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
        let parent = path.parent().ok_or_else(|| format!("{label} path is invalid"))?;
        std::fs::create_dir_all(parent).map_err(|_| format!("{label} directory could not be created"))?;
        std::fs::write(path, bytes).map_err(|_| format!("{label} could not be written"))
    }
}

pub mod burn_authorize {
    use crate::burn_contract::{BurnSignatureVerifier, RemoteFriendBurnPlan, RemoteFriendBurnRequest};
    #[derive(Clone, Copy, Debug, Eq, PartialEq)] pub struct BurnAuthorizationError;
    #[derive(Clone, Debug, Eq, PartialEq)] pub struct BurnScopeBindings<'a> { _marker: std::marker::PhantomData<&'a ()> }
    pub fn authorize_remote_friend_burn(_bindings: BurnScopeBindings<'_>, _local_identity_commitment: [u8; 32], _request: &RemoteFriendBurnRequest, _revoked_grants: &std::collections::BTreeMap<[u8; 16], u64>, _verifier: &impl BurnSignatureVerifier) -> Result<RemoteFriendBurnPlan, BurnAuthorizationError> { Ok(RemoteFriendBurnPlan { burn_id: [0; 32], notices: Vec::new() }) }
}
pub mod burn_contract {
    #[derive(Clone, Debug, Eq, PartialEq)] pub struct RemoteFriendBurnPlan { pub burn_id: [u8; 32], pub notices: Vec<()> }
    #[derive(Clone, Debug, Eq, PartialEq)] pub struct RemoteFriendBurnRequest { pub affected_identity_commitments: Vec<[u8; 32]> }
    pub trait BurnSignatureVerifier {}
}

#[path = "../../../apps/osl-hub/src/spaces.rs"]
mod spaces;

use spaces::{CustomRoleProperties, CustomRoleStore, RoleMentionPolicy};

fn properties(name: &str, self_assignable: bool, order: i64, colour: &str, icon: &str) -> CustomRoleProperties {
    CustomRoleProperties {
        name: name.to_owned(), colour: colour.to_owned(), icon: icon.to_owned(), order,
        hoist: false, mention_policy: RoleMentionPolicy::Everyone, slow_mode_seconds: 0,
        longest_mute_seconds: 0, actions_per_hour_budget: 0, self_assignable,
        auto_grant_on_join: false, expires_at_unix_seconds: u64::MAX,
        duplicate_source_role_id: "fixture".to_owned(), template_name: "fixture".to_owned(),
    }
}

#[test]
fn task_5001_only_lists_and_toggles_self_assignable_roles() {
    let path = std::env::temp_dir().join(format!("task-5001-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut store = CustomRoleStore::load(&path).expect("fixture store");
    let owner = store.create_role(properties("Owner", false, 0, "#ffffff", "★")).unwrap();
    let announcements = store.create_role(properties("Announcements", true, 10, "#f2b84b", "◉")).unwrap();
    let moderator = store.create_role(properties("Moderator", false, 20, "#ef626b", "◆")).unwrap();
    let _events = store.create_role(properties("Event host", true, 30, "#8b5cf6", "✦")).unwrap();
    let member = store.create_role(properties("Member", false, 40, "#99aab5", "●")).unwrap();

    let screen_roles = store.self_assignable_roles();
    assert_eq!(screen_roles.len(), 2);
    assert_eq!(screen_roles.iter().map(|role| role.properties.name.as_str()).collect::<Vec<_>>(), ["Announcements", "Event host"]);
    assert_eq!(screen_roles[0].properties.colour, "#f2b84b");
    assert_eq!(screen_roles[0].properties.icon, "◉");
    assert_eq!(screen_roles[1].properties.colour, "#8b5cf6");
    assert_eq!(screen_roles[1].properties.icon, "✦");
    assert!(!screen_roles.iter().any(|role| role.id == moderator.id || role.id == owner.id || role.id == member.id));
    println!("TASK5001_BACKEND fixture_roles=5 shown_rows={} hidden_non_self_assignable=3", screen_roles.len());

    let before = store.role_ids_for_member("member-1").len();
    assert!(store.take_self_assignable_role("member-1", &announcements.id));
    let after_take = store.role_ids_for_member("member-1").len();
    assert!(store.drop_self_assignable_role("member-1", &announcements.id));
    let after_drop = store.role_ids_for_member("member-1").len();
    assert_eq!(after_take - before, 1);
    assert_eq!(after_take - after_drop, 1);
    println!("TASK5001_BACKEND take_delta={} drop_delta={} final_member_roles={}", after_take - before, after_take - after_drop, after_drop);

    assert!(!store.take_self_assignable_role("member-1", &moderator.id));
    assert!(!store.drop_self_assignable_role("member-1", &moderator.id));
    assert_eq!(store.role_ids_for_member("member-1"), Vec::<String>::new());
    println!("TASK5001_BACKEND non_self_assignable=Moderator appears=false take_allowed=false");
    let _ = std::fs::remove_file(path);
}
