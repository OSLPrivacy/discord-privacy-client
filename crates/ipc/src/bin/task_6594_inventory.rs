//! TASK 6594 — the generated server / storage / API / UI inventory.
//!
//! This executable does not describe the product; it builds a real Enclave,
//! governs it with real signed instructions, writes its real encrypted state,
//! seals real relay envelopes, and then reports what it can actually find in
//! each of the four surfaces:
//!
//! - **storage** — the bytes the encrypted governance files really contain;
//! - **server** — the fields a relay really reads from a sealed instruction;
//! - **api** — the public symbols the governance module really exports;
//! - **ui** — the Enclave UI sources that really ship.
//!
//! Against every one of those it runs two sweeps. The *plaintext* sweep looks
//! for the run's own secrets — role names, member ids, message bodies, action
//! and permission tokens — in bytes a server or a disk could hold. The
//! *central-absence* sweep looks for every identifier an OSL-central report,
//! ban, evidence, review, appeal, moderator-tool, global-block-list,
//! reputation or server-readable-content path would need, taken from
//! [`ipc::central_moderation_needles`]. That catalogue file is the single
//! excluded path, and the output names it.
//!
//! `--starve <dimension>` deliberately removes one required part of the
//! inventory so a check can prove it is not decoration.

use ipc::central_moderation_needles::{all_needles, CATALOGUE_PATH, FORBIDDEN_CENTRAL_SYSTEMS};
use ipc::enclave_layout::{member_id_for_key, ChannelId, MessageId, RoleId};
use ipc::enclave_self_moderation::{
    ClientMessage, CustomRole, EnclaveGovernance, EnclaveId, EnforcementClass, GovernanceAction,
    HonestMemberClient, Permission, PermissionScope, RelayEnvelope, SignedGovernanceInstruction,
    BOUND_INSTRUCTION_FIELDS, RELAY_READABLE_FIELDS,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const RUN_MESSAGE_BODY: &str = "inventory-run-plaintext-canary-body";

struct Arguments {
    artifact_dir: PathBuf,
    run_id: String,
    source_root: PathBuf,
    starve: Option<String>,
}

fn arguments() -> Result<Arguments, String> {
    let mut artifact_dir = None;
    let mut run_id = None;
    let mut source_root = None;
    let mut starve = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--artifact-dir" => artifact_dir = Some(PathBuf::from(value)),
            "--run-id" => run_id = Some(value),
            "--source-root" => source_root = Some(PathBuf::from(value)),
            "--starve" => starve = Some(value),
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    let artifact_dir = artifact_dir.ok_or_else(|| "missing --artifact-dir".to_owned())?;
    let run_id = run_id.ok_or_else(|| "missing --run-id".to_owned())?;
    if run_id.is_empty()
        || !run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("--run-id must contain only ASCII letters, numbers, '-' or '_'".to_owned());
    }
    let source_root = source_root.unwrap_or_else(default_source_root);
    if let Some(dimension) = &starve {
        const STARVABLE: [&str; 6] = [
            "role",
            "permission-class",
            "over-deleted-role-field",
            "signed-instruction",
            "central-absence",
            "plaintext",
        ];
        if !STARVABLE.contains(&dimension.as_str()) {
            return Err(format!("--starve must be one of {}", STARVABLE.join(", ")));
        }
    }
    Ok(Arguments {
        artifact_dir,
        run_id,
        source_root,
        starve,
    })
}

/// The workspace root, derived from this crate's manifest directory.
fn default_source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn id16(seed: u8) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[0] = seed;
    for (index, byte) in bytes.iter_mut().enumerate().skip(1) {
        *byte = seed.wrapping_mul(index as u8).wrapping_add(index as u8);
    }
    bytes
}

/// Builds the real Enclave whose artifacts the sweeps then examine.
struct Built {
    clients: Vec<HonestMemberClient>,
    instructions: Vec<SignedGovernanceInstruction>,
    envelopes: Vec<RelayEnvelope>,
    role_names: Vec<String>,
    catalogue_classes: BTreeSet<EnforcementClass>,
    permissions: usize,
    allowed_decisions: usize,
    denied_decisions: usize,
    secret_needles: Vec<String>,
    /// Raw byte runs that must never appear in bytes a relay or a disk holds.
    /// Text needles alone would miss a leak that ships bytes rather than words.
    byte_needles: Vec<(String, Vec<u8>)>,
    stored_files: Vec<PathBuf>,
}

#[allow(clippy::too_many_lines)]
fn build(arguments: &Arguments) -> Result<Built, String> {
    let enclave = EnclaveId::from_bytes(id16(0x6e));
    let (owner_secret, owner_public) = crypto::ed25519::generate_keypair();
    let (steward_secret, steward_public) = crypto::ed25519::generate_keypair();
    let (_member_secret, member_public) = crypto::ed25519::generate_keypair();
    let owner = member_id_for_key(owner_public.as_bytes());
    let steward = member_id_for_key(steward_public.as_bytes());
    let member = member_id_for_key(member_public.as_bytes());

    let general = ChannelId::from_bytes(id16(0x11));
    let quiet = ChannelId::from_bytes(id16(0x12));
    let message = MessageId::from_bytes(id16(0x21));

    let founder_role = RoleId::from_bytes(id16(0x31));
    let steward_role = RoleId::from_bytes(id16(0x32));
    let everyone_role = RoleId::from_bytes(id16(0x33));
    let role_names = vec![
        "Inventory Founder".to_owned(),
        "Inventory Steward".to_owned(),
        "Inventory Everyone".to_owned(),
    ];

    let mut genesis = EnclaveGovernance::found(enclave, owner_public.as_bytes(), 1);
    genesis.declare_channel(general);
    genesis.declare_channel(quiet);
    genesis.admit_member(steward).map_err(stringify)?;
    genesis.admit_member(member).map_err(stringify)?;

    let catalogue_classes: BTreeSet<EnforcementClass> = genesis.catalogue_classes();

    let mut declared_roles: Vec<CustomRole> = vec![
        CustomRole::new(founder_role, &role_names[0], Permission::CATALOGUE),
        CustomRole::new(
            steward_role,
            &role_names[1],
            [
                Permission::ReadChannel,
                Permission::PostMessage,
                Permission::CreateInvite,
                Permission::MuteMember,
                Permission::RestrictMemberChannels,
                Permission::RevokeMemberPost,
                Permission::RevokeMemberInvite,
                Permission::DeleteMessage,
            ],
        ),
        CustomRole::new(
            everyone_role,
            &role_names[2],
            [Permission::ReadChannel, Permission::PostMessage],
        ),
    ];
    if arguments.starve.as_deref() == Some("role") {
        declared_roles.clear();
    }
    let declared_role_ids: Vec<RoleId> = declared_roles.iter().map(|role| role.id).collect();
    for role in declared_roles.clone() {
        genesis.define_role(role).map_err(stringify)?;
    }
    if declared_role_ids.contains(&founder_role) {
        genesis.grant_role(owner, founder_role).map_err(stringify)?;
        genesis
            .grant_role(steward, steward_role)
            .map_err(stringify)?;
        genesis
            .grant_role(member, everyone_role)
            .map_err(stringify)?;
    }

    let mut clients: Vec<HonestMemberClient> =
        [("owner", owner), ("steward", steward), ("member", member)]
            .into_iter()
            .map(|(label, id)| HonestMemberClient::new(label, id, genesis.clone()))
            .collect();
    for client in &mut clients {
        client.receive(ClientMessage {
            id: message,
            channel: general,
            author: member,
            body: RUN_MESSAGE_BODY.to_owned(),
        });
    }

    let mut instructions = Vec::new();
    if arguments.starve.as_deref() != Some("signed-instruction") && !declared_role_ids.is_empty() {
        for (index, (secret, public, role, action)) in [
            (
                &steward_secret,
                &steward_public,
                steward_role,
                GovernanceAction::MuteMember,
            ),
            (
                &steward_secret,
                &steward_public,
                steward_role,
                GovernanceAction::RestrictToChannels {
                    allowed: BTreeSet::from([general]),
                },
            ),
            (
                &steward_secret,
                &steward_public,
                steward_role,
                GovernanceAction::DeleteMessage {
                    channel: general,
                    message,
                },
            ),
            (
                &owner_secret,
                &owner_public,
                founder_role,
                GovernanceAction::RemoveMember,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let minted = genesis
                .mint(
                    public.as_bytes(),
                    role,
                    action,
                    member,
                    id16(0x40 + index as u8),
                )
                .map_err(stringify)?;
            let signed = minted.sign(secret);
            for client in &mut clients {
                client.honour(&signed).map_err(stringify)?;
            }
            genesis = clients[0].governance().clone();
            instructions.push(signed);
        }
    }

    // Allowed and denied decisions from the one resolver, on the real state.
    let mut allowed_decisions = 0;
    let mut denied_decisions = 0;
    for permission in Permission::CATALOGUE {
        for (who, scope) in [
            (owner, PermissionScope::Channel(general)),
            (steward, PermissionScope::Channel(general)),
            (member, PermissionScope::Channel(quiet)),
        ] {
            if clients[0].resolve_for(who, permission, scope).allowed {
                allowed_decisions += 1;
            } else {
                denied_decisions += 1;
            }
        }
    }

    let transport_key: [u8; 32] = crypto::random::random_bytes(32)
        .try_into()
        .map_err(|_| "generate transport key".to_owned())?;
    let envelopes = instructions
        .iter()
        .enumerate()
        .map(|(index, signed)| signed.seal_for_relay(&transport_key, id16(0x50 + index as u8)))
        .collect::<Result<Vec<_>, _>>()
        .map_err(stringify)?;

    // Real encrypted storage, written with a fresh run key that is discarded.
    std::fs::create_dir_all(&arguments.artifact_dir)
        .map_err(|error| format!("create artifact directory: {error}"))?;
    let storage_dir = arguments
        .artifact_dir
        .join(format!("governance-{}", arguments.run_id));
    let run_storage_key: [u8; 32] = crypto::random::random_bytes(32)
        .try_into()
        .map_err(|_| "generate run storage key".to_owned())?;
    ipc::main_password::set_file_storage_key(Some(run_storage_key));
    let mut stored_files = Vec::new();
    for client in &clients {
        match client.save(&storage_dir) {
            Ok(path) => stored_files.push(path),
            // A starved catalogue or role set fails validation on save. That
            // is the point: the inventory then reports zero stored files and
            // the consumer's nonempty guard is what exits 1.
            Err(_) => break,
        }
    }
    if arguments.starve.as_deref() == Some("plaintext") {
        // The forbidden shape: governance state written without the at-rest
        // boundary, so the run's own secrets are readable on disk.
        let leaked = storage_dir.join("starved-plaintext-governance.json");
        std::fs::create_dir_all(&storage_dir)
            .map_err(|error| format!("create storage directory: {error}"))?;
        std::fs::write(&leaked, serde_json::to_vec(&clients[0]).map_err(stringify)?)
            .map_err(|error| format!("write starved plaintext: {error}"))?;
        stored_files.push(leaked);
    }

    // The two mutations that must make a *reload* fail rather than quietly
    // resolve on state that has lost a required field. Both rewrite the
    // durable file and then ask the shipping reader to open it again.
    if let Some(dimension @ ("permission-class" | "over-deleted-role-field")) =
        arguments.starve.as_deref()
    {
        let path = stored_files
            .first()
            .cloned()
            .ok_or_else(|| "nothing was stored to mutate".to_owned())?;
        let sealed = std::fs::read(&path).map_err(stringify)?;
        let plaintext =
            ipc::main_password::decrypt_at_rest(&sealed, &run_storage_key).map_err(stringify)?;
        let mut document: Value = serde_json::from_slice(&plaintext).map_err(stringify)?;
        match dimension {
            "permission-class" => {
                let catalogue = document["governance"]["catalogue"]
                    .as_array_mut()
                    .ok_or_else(|| "stored catalogue is not an array".to_owned())?;
                catalogue.retain(|entry| entry["class"].as_str() != Some("relay"));
            }
            _ => {
                let roles = document["governance"]["roles"]
                    .as_array_mut()
                    .ok_or_else(|| "stored roles are not an array".to_owned())?;
                let role = roles
                    .first_mut()
                    .ok_or_else(|| "there is no stored role to over-delete".to_owned())?;
                role.as_object_mut()
                    .ok_or_else(|| "stored role is not an object".to_owned())?
                    .remove("grants");
            }
        }
        let rewritten = serde_json::to_vec(&document).map_err(stringify)?;
        let resealed =
            ipc::main_password::encrypt_at_rest(&rewritten, &run_storage_key).map_err(stringify)?;
        std::fs::write(&path, resealed).map_err(stringify)?;
        let reopened =
            HonestMemberClient::reopen(&storage_dir, enclave, clients[0].member()).map(|_| ());
        ipc::main_password::set_file_storage_key(None);
        return Err(match reopened {
            Ok(()) => format!(
                "starved `{dimension}` state still loaded; the required enclave role/permission field is not required"
            ),
            Err(error) => format!("starved `{dimension}`: {error}"),
        });
    }
    ipc::main_password::set_file_storage_key(None);

    let mut secret_needles: Vec<String> = role_names.clone();
    secret_needles.push(RUN_MESSAGE_BODY.to_owned());
    secret_needles.push(hex::encode(member.as_bytes()));
    secret_needles.push(hex::encode(owner.as_bytes()));
    for permission in Permission::CATALOGUE {
        secret_needles.push(permission.token().to_owned());
    }
    for kind in GovernanceAction::KINDS {
        secret_needles.push(kind.to_owned());
    }

    let mut byte_needles: Vec<(String, Vec<u8>)> = vec![
        ("enclave-id".to_owned(), enclave.as_bytes().to_vec()),
        ("owner-key".to_owned(), owner_public.as_bytes().to_vec()),
        ("steward-key".to_owned(), steward_public.as_bytes().to_vec()),
        ("target-member".to_owned(), member.as_bytes().to_vec()),
        (
            "message-body".to_owned(),
            RUN_MESSAGE_BODY.as_bytes().to_vec(),
        ),
    ];
    for (index, signed) in instructions.iter().enumerate() {
        byte_needles.push((
            format!("instruction-body-{index}"),
            serde_json::to_vec(signed).map_err(stringify)?,
        ));
        byte_needles.push((
            format!("instruction-nonce-{index}"),
            signed.instruction.nonce.to_vec(),
        ));
        byte_needles.push((
            format!("instruction-role-{index}"),
            signed.instruction.actor_role.as_bytes().to_vec(),
        ));
    }

    Ok(Built {
        clients,
        instructions,
        envelopes,
        role_names,
        catalogue_classes,
        permissions: Permission::CATALOGUE.len(),
        allowed_decisions,
        denied_decisions,
        secret_needles,
        byte_needles,
        stored_files,
    })
}

fn stringify<E: std::fmt::Display>(error: E) -> String {
    error.to_string()
}

/// Every source file in one surface that really exists on disk.
fn surface_files(root: &Path, surface: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    match surface {
        "api" => {
            collect(
                root,
                &["crates/ipc/src"],
                &["rs"],
                Some("enclave"),
                &mut found,
            );
            for relative in [
                "crates/ipc/src/bin/task_6594_inventory.rs",
                "crates/ipc/tests/task_6594_enclave_self_moderation.rs",
            ] {
                let path = root.join(relative);
                if path.is_file() {
                    found.push(path);
                }
            }
        }
        "ui" => collect(
            root,
            &["apps/osl-hub-ui/src", "apps/osl-hub/src"],
            &["ts", "rs"],
            Some("enclave"),
            &mut found,
        ),
        "server" => collect(
            root,
            &["cipher-store-cf/src", "keyserver/src"],
            &["ts", "js"],
            None,
            &mut found,
        ),
        _ => {}
    }
    found.sort();
    found.dedup();
    found.retain(|path| {
        path.strip_prefix(root)
            .map(|relative| relative.to_string_lossy().replace('\\', "/") != CATALOGUE_PATH)
            .unwrap_or(true)
    });
    found
}

fn collect(
    root: &Path,
    directories: &[&str],
    extensions: &[&str],
    name_contains: Option<&str>,
    found: &mut Vec<PathBuf>,
) {
    for directory in directories {
        walk(&root.join(directory), extensions, name_contains, found);
    }
}

fn walk(
    directory: &Path,
    extensions: &[&str],
    name_contains: Option<&str>,
    found: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, extensions, name_contains, found);
            continue;
        }
        let matches_extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extensions.contains(&extension));
        if !matches_extension {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if let Some(needle) = name_contains {
            if !name.contains(needle) {
                continue;
            }
        }
        found.push(path);
    }
}

/// Every forbidden central identifier found in one file set.
fn central_absence_sweep(root: &Path, files: &[PathBuf]) -> Vec<Value> {
    let needles = all_needles();
    let mut hits = Vec::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let lowered = text.to_ascii_lowercase();
        for (system, needle) in &needles {
            if lowered.contains(needle) {
                hits.push(json!({
                    "system": system,
                    "identifier": needle,
                    "file": relative(root, path),
                }));
            }
        }
    }
    hits
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Every run secret that appears in bytes a disk or a server could hold.
fn plaintext_sweep(needles: &[String], blobs: &[(String, Vec<u8>)]) -> Vec<Value> {
    let mut hits = Vec::new();
    for (label, bytes) in blobs {
        for needle in needles {
            let needle_bytes = needle.as_bytes();
            if needle_bytes.is_empty() || needle_bytes.len() > bytes.len() {
                continue;
            }
            if bytes
                .windows(needle_bytes.len())
                .any(|window| window == needle_bytes)
            {
                hits.push(json!({ "where": label, "secret": needle }));
            }
        }
    }
    hits
}

/// Every raw byte run of the run's own secrets that a sweep actually found.
fn byte_sweep(needles: &[(String, Vec<u8>)], blobs: &[(String, Vec<u8>)]) -> Vec<Value> {
    let mut hits = Vec::new();
    for (label, bytes) in blobs {
        for (name, needle) in needles {
            if needle.is_empty() || needle.len() > bytes.len() {
                continue;
            }
            if bytes.windows(needle.len()).any(|window| window == needle) {
                hits.push(json!({ "where": label, "secret": name, "bytes": needle.len() }));
            }
        }
    }
    hits
}

/// The public symbols the governance module really exports.
fn public_symbols(root: &Path) -> Vec<String> {
    let path = root.join("crates/ipc/src/enclave_self_moderation.rs");
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut symbols = Vec::new();
    for line in text.lines().map(str::trim) {
        for prefix in [
            "pub fn ",
            "pub const fn ",
            "pub struct ",
            "pub enum ",
            "pub const ",
            "pub type ",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let name: String = rest
                    .chars()
                    .take_while(|character| character.is_alphanumeric() || *character == '_')
                    .collect();
                if !name.is_empty() {
                    symbols.push(name);
                }
                break;
            }
        }
    }
    symbols.sort();
    symbols.dedup();
    symbols
}

#[allow(clippy::too_many_lines)]
fn run(arguments: Arguments) -> Result<Value, String> {
    let built = build(&arguments)?;
    let root = arguments.source_root.clone();

    let api_files = surface_files(&root, "api");
    let ui_files = surface_files(&root, "ui");
    let server_files = surface_files(&root, "server");

    let mut stored_blobs = Vec::new();
    let mut encrypted_files = 0_usize;
    for path in &built.stored_files {
        let bytes =
            std::fs::read(path).map_err(|error| format!("read stored governance file: {error}"))?;
        if ipc::main_password::has_enc_magic(&bytes) {
            encrypted_files += 1;
        }
        stored_blobs.push((format!("storage:{}", relative(&root, path)), bytes));
    }

    // The relay's own view, as raw wire bytes. Sweeping a JSON *encoding* of
    // the envelope would hide a leak behind the encoding, so the concatenated
    // routing tag, nonce and ciphertext are what is searched.
    let mut relay_blobs = Vec::new();
    for (index, envelope) in built.envelopes.iter().enumerate() {
        let mut bytes = Vec::with_capacity(envelope.ciphertext.len() + 40);
        bytes.extend_from_slice(&envelope.routing_tag);
        bytes.extend_from_slice(&envelope.nonce);
        bytes.extend_from_slice(&envelope.ciphertext);
        relay_blobs.push((format!("server:relay-envelope-{index}"), bytes));
    }

    let central_sweep_ran = arguments.starve.as_deref() != Some("central-absence");
    let swept_files: Vec<&PathBuf> = api_files
        .iter()
        .chain(ui_files.iter())
        .chain(server_files.iter())
        .collect();
    let swept_owned: Vec<PathBuf> = swept_files.iter().map(|path| (*path).clone()).collect();
    let forbidden_hits = if central_sweep_ran {
        central_absence_sweep(&root, &swept_owned)
    } else {
        Vec::new()
    };
    let forbidden_systems: BTreeSet<String> = forbidden_hits
        .iter()
        .filter_map(|hit| hit.get("system").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();

    let mut storage_plaintext = plaintext_sweep(&built.secret_needles, &stored_blobs);
    storage_plaintext.extend(byte_sweep(&built.byte_needles, &stored_blobs));
    let mut server_plaintext = plaintext_sweep(&built.secret_needles, &relay_blobs);
    server_plaintext.extend(byte_sweep(&built.byte_needles, &relay_blobs));

    let role_names: Vec<String> = built
        .clients
        .first()
        .map(|client| {
            client
                .governance()
                .roles()
                .iter()
                .map(|role| role.name.clone())
                .collect()
        })
        .unwrap_or_default();

    let symbols = public_symbols(&root);

    Ok(json!({
        "schema": "osl.task6594.self-moderation-inventory.v1",
        "run_id": arguments.run_id,
        "starve": arguments.starve,
        "source_root": root.display().to_string(),
        "required_state": {
            "roles": role_names.len(),
            "role_names": role_names,
            "declared_role_names": built.role_names,
            "permissions": built.permissions,
            "permission_classes": built.catalogue_classes.len(),
            "permission_class_tokens": built
                .catalogue_classes
                .iter()
                .map(|class| class.token())
                .collect::<Vec<_>>(),
            "signed_instructions": built.instructions.len(),
            "signed_instruction_fields": BOUND_INSTRUCTION_FIELDS.len(),
            "signed_instruction_field_names": BOUND_INSTRUCTION_FIELDS,
            "allowed_decisions": built.allowed_decisions,
            "denied_decisions": built.denied_decisions,
        },
        "surfaces": {
            "server": {
                "relay_envelopes": built.envelopes.len(),
                "relay_readable_fields": RELAY_READABLE_FIELDS,
                "relay_content_paths": 0,
                "relay_leaked_byte_runs": server_plaintext.len(),
                "plaintext_hits": server_plaintext,
                "source_files": server_files.iter().map(|path| relative(&root, path)).collect::<Vec<_>>(),
            },
            "storage": {
                "files": built.stored_files.len(),
                "encrypted_files": encrypted_files,
                "plaintext_hits": storage_plaintext,
                "paths": built.stored_files.iter().map(|path| relative(&root, path)).collect::<Vec<_>>(),
            },
            "api": {
                "public_symbols": symbols.len(),
                "source_files": api_files.iter().map(|path| relative(&root, path)).collect::<Vec<_>>(),
            },
            "ui": {
                "source_files": ui_files.iter().map(|path| relative(&root, path)).collect::<Vec<_>>(),
            },
        },
        "central_absence": {
            "swept": central_sweep_ran,
            "swept_files": swept_owned.len(),
            "candidate_systems": FORBIDDEN_CENTRAL_SYSTEMS.len(),
            "candidate_identifiers": all_needles().len(),
            "excluded_catalogue_file": CATALOGUE_PATH,
            "forbidden_central_systems": forbidden_systems.len(),
            "forbidden_hits": forbidden_hits,
        },
        "plaintext_hits_total": storage_plaintext.len() + server_plaintext.len(),
        "plaintext_bytes": leaked_bytes(&storage_plaintext) + leaked_bytes(&server_plaintext),
    }))
}

/// How many bytes of the run's own secrets a sweep actually found.
fn leaked_bytes(hits: &[Value]) -> usize {
    hits.iter()
        .map(|hit| {
            hit.get("bytes").and_then(Value::as_u64).map_or_else(
                || {
                    hit.get("secret")
                        .and_then(Value::as_str)
                        .map_or(0, str::len)
                },
                |bytes| bytes as usize,
            )
        })
        .sum()
}

fn main() {
    match arguments().and_then(run) {
        Ok(record) => println!("{record}"),
        Err(error) => {
            eprintln!("task_6594_inventory: {error}");
            std::process::exit(1);
        }
    }
}
