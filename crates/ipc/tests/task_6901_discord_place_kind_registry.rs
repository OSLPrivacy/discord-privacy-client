//! TASK 6901 - the shipping Discord kinds command must derive its answer
//! from the single `DiscordWhitelistKind::ALL` place-kind registry, and no
//! other shipping source may answer the same question with a hard-coded
//! array detached from that registry.

use ipc::auto_whitelist_rules::DiscordWhitelistKind;
use ipc::commands::{cmd_osl_get_discord_whitelist_kinds, cmd_osl_list_discord_whitelist_kinds};
use ipc::provider_discovery::ShippingWindowsProvider;
use ipc::state::AppState;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const EXPECTED_IDS: [&str; 5] = [
    "direct_message",
    "group_chat",
    "server",
    "server_channel",
    "thread",
];

const EXPECTED_NAMES: [&str; 5] = [
    "direct message",
    "group chat",
    "server",
    "server channel",
    "thread",
];

#[test]
fn task_6901_discord_kinds_command_answers_exactly_five_named_kinds() {
    let dto_kinds =
        cmd_osl_get_discord_whitelist_kinds().expect("shipping Discord whitelist kinds");
    let ids: Vec<&str> = dto_kinds.iter().map(|kind| kind.id.as_str()).collect();
    let names: Vec<&str> = dto_kinds.iter().map(|kind| kind.name.as_str()).collect();

    println!("TASK6901_DISCORD_KIND_COUNT: {}", dto_kinds.len());
    println!("TASK6901_DISCORD_KIND_IDS: {}", ids.join(", "));
    println!("TASK6901_DISCORD_KIND_NAMES: {}", names.join(", "));

    assert_eq!(
        dto_kinds.len(),
        5,
        "the Discord kinds command must return exactly 5 kinds"
    );
    assert_eq!(
        ids,
        EXPECTED_IDS.to_vec(),
        "Discord kind IDs must stay named and ordered"
    );
    assert_eq!(
        names,
        EXPECTED_NAMES.to_vec(),
        "Discord kind names must stay named and ordered"
    );
    assert!(
        ids.iter().all(|id| !id.is_empty()),
        "no Discord kind ID may be empty"
    );
    assert!(
        names.iter().all(|name| !name.is_empty()),
        "no Discord kind name may be empty"
    );
    assert_eq!(
        ids.iter().collect::<HashSet<_>>().len(),
        ids.len(),
        "Discord kind IDs must be unique"
    );
}

#[test]
fn task_6901_discord_kinds_command_derives_from_the_place_kind_registry() {
    let state = AppState::new();

    let registry_ids: Vec<&'static str> = DiscordWhitelistKind::ALL
        .iter()
        .map(|kind| kind.id())
        .collect();
    let registry_names: Vec<&'static str> = DiscordWhitelistKind::ALL
        .iter()
        .map(|kind| kind.name())
        .collect();

    assert!(
        !registry_ids.is_empty(),
        "DiscordWhitelistKind::ALL registry is starved: it must not be empty"
    );

    let dto_kinds =
        cmd_osl_get_discord_whitelist_kinds().expect("shipping Discord whitelist kinds");
    let command_ids: Vec<&str> = dto_kinds.iter().map(|kind| kind.id.as_str()).collect();
    let command_names: Vec<&str> = dto_kinds.iter().map(|kind| kind.name.as_str()).collect();
    assert_eq!(
        command_ids, registry_ids,
        "cmd_osl_get_discord_whitelist_kinds must equal DiscordWhitelistKind::ALL exactly"
    );
    assert_eq!(
        command_names, registry_names,
        "cmd_osl_get_discord_whitelist_kinds must equal DiscordWhitelistKind::ALL exactly"
    );

    let label_kinds =
        cmd_osl_list_discord_whitelist_kinds(&state).expect("shipping Discord whitelist labels");
    assert_eq!(
        label_kinds, registry_names,
        "cmd_osl_list_discord_whitelist_kinds must answer from the same registry"
    );

    let provider_ids = ShippingWindowsProvider::Discord.supported_place_kinds();
    assert_eq!(
        provider_ids, registry_ids,
        "the shipping Windows provider must enumerate the same registry, not its own copy"
    );
}

/// Recursively collects every `.rs` file under `dir`, skipping test
/// fixtures, examples, throwaway worktrees, and build output so the scan
/// only sees shipping source.
fn collect_shipping_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "target" | "node_modules" | ".git" | "tests" | "examples" | "dist"
            ) || name.starts_with(".task-")
            {
                continue;
            }
            collect_shipping_rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn task_6901_exactly_one_shipping_source_answers_the_discord_kind_list() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .join("../..")
        .canonicalize()
        .expect("resolve repo root from CARGO_MANIFEST_DIR");

    let scan_roots = [
        "crates",
        "apps/osl-hub/src",
        "apps/osl-hub-ui/src",
        "src-tauri/src",
    ];

    // The exact quoted tokens that make up the 5 Discord place kinds. A
    // shipping file that carries all five together is answering "what are
    // the Discord kinds" with a literal list of its own, whether or not it
    // is meant to be the registry.
    let tokens = [
        "\"direct_message\"",
        "\"group_chat\"",
        "\"server_channel\"",
        "\"thread\"",
        "\"server\"",
    ];

    let mut files = Vec::new();
    for root in scan_roots {
        let dir = repo_root.join(root);
        if dir.exists() {
            collect_shipping_rust_files(&dir, &mut files);
        }
    }
    assert!(
        files.len() > 100,
        "the shipping source scan must cover the real tree, found only {}",
        files.len()
    );

    let mut sources: Vec<PathBuf> = files
        .into_iter()
        .filter(|path| {
            let Ok(contents) = std::fs::read_to_string(path) else {
                return false;
            };
            tokens.iter().all(|token| contents.contains(token))
        })
        .collect();
    sources.sort();

    let relative: Vec<String> = sources
        .iter()
        .map(|path| {
            path.strip_prefix(&repo_root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    println!(
        "TASK6901_DISCORD_KIND_SHIPPING_SOURCES count={}",
        relative.len()
    );
    for path in &relative {
        println!("TASK6901_DISCORD_KIND_SHIPPING_SOURCE: {path}");
    }

    assert_eq!(
        relative.len(),
        1,
        "exactly one shipping source may answer the Discord kind list, found: {relative:?}"
    );
    assert!(
        relative[0].ends_with("crates/ipc/src/auto_whitelist_rules.rs"),
        "the one shipping source must be the place-kind registry itself, found: {}",
        relative[0]
    );
}
