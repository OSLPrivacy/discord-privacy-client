extern crate self as ipc;

pub mod space_roster {
    #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
    pub struct SpaceChannelId([u8; Self::LENGTH]);

    impl SpaceChannelId {
        pub const LENGTH: usize = 16;

        pub const fn from_bytes(bytes: [u8; Self::LENGTH]) -> Self {
            Self(bytes)
        }
    }
}

pub mod atomic_file {
    use std::path::Path;

    pub(crate) fn read_recoverable_bounded(
        path: &Path,
        max_bytes: u64,
        label: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        match std::fs::metadata(path) {
            Ok(metadata) if !metadata.is_file() || metadata.len() > max_bytes => {
                Err(format!("{label} is not a bounded regular file"))
            }
            Ok(_) => std::fs::read(path)
                .map(Some)
                .map_err(|_| format!("{label} could not be read")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(format!("{label} metadata could not be read")),
        }
    }

    pub(crate) fn write_recoverable(path: &Path, bytes: &[u8], label: &str) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| format!("{label} path is invalid"))?;
        std::fs::create_dir_all(parent)
            .map_err(|_| format!("{label} directory could not be created"))?;
        std::fs::write(path, bytes).map_err(|_| format!("{label} could not be written"))
    }
}

pub mod burn_authorize {
    use crate::burn_contract::{
        BurnSignatureVerifier, RemoteFriendBurnPlan, RemoteFriendBurnRequest,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct BurnAuthorizationError;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct BurnScopeBindings<'a> {
        _marker: std::marker::PhantomData<&'a ()>,
    }

    pub fn authorize_remote_friend_burn(
        _bindings: BurnScopeBindings<'_>,
        _local_identity_commitment: [u8; 32],
        _request: &RemoteFriendBurnRequest,
        _revoked_grants: &std::collections::BTreeMap<[u8; 16], u64>,
        _verifier: &impl BurnSignatureVerifier,
    ) -> Result<RemoteFriendBurnPlan, BurnAuthorizationError> {
        Ok(RemoteFriendBurnPlan {
            burn_id: [0; 32],
            notices: Vec::new(),
        })
    }
}

pub mod burn_contract {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct RemoteFriendBurnPlan {
        pub burn_id: [u8; 32],
        pub notices: Vec<()>,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct RemoteFriendBurnRequest {
        pub affected_identity_commitments: Vec<[u8; 32]>,
    }

    pub trait BurnSignatureVerifier {}
}

#[path = "../../../apps/osl-hub/src/spaces.rs"]
mod spaces;

use spaces::{
    CustomRoleProperties, CustomRoleRecord, CustomRoleStore, RoleMentionPolicy,
    CUSTOM_ROLE_RECORD_FIELDS,
};

fn filled_role_properties() -> CustomRoleProperties {
    CustomRoleProperties {
        name: "Signal Watch Captain".to_owned(),
        colour: "#14b8a6".to_owned(),
        icon: "shield-check".to_owned(),
        order: 42,
        hoist: true,
        mention_policy: RoleMentionPolicy::OwnerAndModerators,
        slow_mode_seconds: 17,
        longest_mute_seconds: 3_600,
        actions_per_hour_budget: 24,
        self_assignable: true,
        auto_grant_on_join: true,
        expires_at_unix_seconds: 4_000_000_000,
        duplicate_source_role_id: "seed-role-template".to_owned(),
        template_name: "watch-captain-template".to_owned(),
    }
}

fn role_store_path(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "osl-space-role-{label}-{}-{nonce}.json",
        std::process::id()
    ))
}

fn remove_role_store_file(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

fn missing_field_store_json(field: &str) -> Vec<u8> {
    let role = CustomRoleRecord {
        id: "role-missing-check".to_owned(),
        properties: filled_role_properties(),
    };
    let mut role_value = serde_json::to_value(role).expect("role serializes");
    role_value
        .as_object_mut()
        .expect("role is an object")
        .remove(field);
    let document = serde_json::json!({
        "next_role_number": 1,
        "roles": {
            "role-missing-check": role_value
        },
        "member_roles": {},
        "templates": {}
    });
    serde_json::to_vec_pretty(&document).expect("document serializes")
}

fn main() {
    let path = role_store_path("task-4857-roundtrip");
    let mut store = CustomRoleStore::load(&path).expect("empty store loads");
    let properties = filled_role_properties();
    let original = store
        .create_role(properties.clone())
        .expect("15-field role is accepted");
    store.save().expect("custom role store saves");

    let restarted = CustomRoleStore::load(&path).expect("role store reloads after restart");
    let reloaded = restarted
        .role(&original.id)
        .expect("created role survives restart");
    let restart_matches = reloaded == &original;
    println!(
        "TASK4857_RESTART role_id={} stored_property_count={} restart_matches={} name={} colour={} icon={} order={} hoist={} mention_policy={:?} slow_mode_seconds={} longest_mute_seconds={} actions_per_hour_budget={} self_assignable={} auto_grant_on_join={} expires_at_unix_seconds={} duplicate_source_role_id={} template_name={}",
        reloaded.id,
        CustomRoleRecord::stored_field_count(),
        restart_matches,
        reloaded.properties.name,
        reloaded.properties.colour,
        reloaded.properties.icon,
        reloaded.properties.order,
        reloaded.properties.hoist,
        reloaded.properties.mention_policy,
        reloaded.properties.slow_mode_seconds,
        reloaded.properties.longest_mute_seconds,
        reloaded.properties.actions_per_hour_budget,
        reloaded.properties.self_assignable,
        reloaded.properties.auto_grant_on_join,
        reloaded.properties.expires_at_unix_seconds,
        reloaded.properties.duplicate_source_role_id,
        reloaded.properties.template_name
    );
    assert!(restart_matches);
    assert_eq!(CustomRoleRecord::stored_field_count(), 15);

    let duplicate_path = role_store_path("task-4857-duplicate");
    let mut duplicate_store = CustomRoleStore::load(&duplicate_path).expect("duplicate store");
    let duplicate_source = duplicate_store
        .create_role(properties.clone())
        .expect("source role");
    let duplicate = duplicate_store
        .duplicate_role(&duplicate_source.id)
        .expect("duplicate role");
    let copied_value_count = usize::from(duplicate.properties == duplicate_source.properties)
        * CustomRoleProperties::filled_property_count();
    println!(
        "TASK4857_DUPLICATE source_id={} duplicate_id={} ids_differ={} copied_value_count={}",
        duplicate_source.id,
        duplicate.id,
        duplicate_source.id != duplicate.id,
        copied_value_count
    );
    assert_ne!(duplicate_source.id, duplicate.id);
    assert_eq!(copied_value_count, 14);

    let expiry_path = role_store_path("task-4857-expiry");
    let mut expiry_store = CustomRoleStore::load(&expiry_path).expect("expiry store");
    let mut expiring_properties = properties.clone();
    expiring_properties.name = "Temporary watch".to_owned();
    expiring_properties.template_name = "temporary-watch-template".to_owned();
    expiring_properties.expires_at_unix_seconds = 99;
    expiry_store
        .add_template(
            expiring_properties.template_name.clone(),
            expiring_properties.clone(),
        )
        .expect("template saves");
    let template_before = expiry_store
        .template(&expiring_properties.template_name)
        .expect("template exists before prune")
        .clone();
    let expiring = expiry_store
        .create_role(expiring_properties.clone())
        .expect("expiring role");
    for member_id in ["member-a", "member-b", "member-c"] {
        assert!(expiry_store.grant_role_to_member(member_id, &expiring.id));
    }
    let report = expiry_store.prune_expired_roles(100);
    let template_after = expiry_store
        .template(&expiring_properties.template_name)
        .expect("template remains after prune")
        .clone();
    let remaining_member_grants: usize = ["member-a", "member-b", "member-c"]
        .into_iter()
        .map(|member_id| expiry_store.role_ids_for_member(member_id).len())
        .sum();
    println!(
        "TASK4857_EXPIRY expired_role_count={} member_role_removal_count={} remaining_member_grants={} template_untouched={}",
        report.expired_role_count,
        report.member_role_removal_count,
        remaining_member_grants,
        template_before == template_after
    );
    assert_eq!(report.expired_role_count, 1);
    assert_eq!(report.member_role_removal_count, 3);
    assert_eq!(remaining_member_grants, 0);
    assert_eq!(template_before, template_after);

    let mut missing_refusals = Vec::new();
    for field in CUSTOM_ROLE_RECORD_FIELDS {
        let missing_path = role_store_path(&format!("task-4857-missing-{field}"));
        std::fs::write(&missing_path, missing_field_store_json(field))
            .expect("missing-field fixture writes");
        let error = CustomRoleStore::load(&missing_path)
            .expect_err("missing role property is refused")
            .to_string();
        println!("TASK4857_MISSING_REFUSAL field={field} error=\"{error}\"");
        assert!(
            error.contains(field),
            "refusal must name missing field {field}, got {error}"
        );
        missing_refusals.push(field.to_owned());
        remove_role_store_file(&missing_path);
    }
    println!(
        "TASK4857_MISSING_REFUSAL_COUNT={} fields={}",
        missing_refusals.len(),
        missing_refusals.join(",")
    );
    assert_eq!(missing_refusals.len(), 15);

    remove_role_store_file(&path);
    remove_role_store_file(&duplicate_path);
    remove_role_store_file(&expiry_path);
}
