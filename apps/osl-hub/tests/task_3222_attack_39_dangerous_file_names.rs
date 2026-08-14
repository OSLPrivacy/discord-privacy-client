//! TASK 3222 / attack 39: authenticated hostile logical filenames survive the
//! attachment wire unchanged, while receiver output is confined to an explicit
//! folder under a deterministic portable leaf and never overwrites a file.
//!
//! `TASK3222_FIXTURE` selects the manifest consumed by this exact check. A
//! manifest missing even one required hostile-name class must therefore turn
//! the same test red before filesystem side effects begin.

#![cfg(all(feature = "core", target_os = "linux"))]

use crypto::aead;
use osl_privacy_hub::osl_chat_attachment_filename::{
    safe_attachment_output_name, save_received_attachment_bytes, MAX_SAFE_ATTACHMENT_FILENAME_BYTES,
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/task_3222/all_dangerous_names.json"
);
const OUTSIDE_CHOSEN_FOLDER_RESULT: &str = "DANGEROUS-NAME-3222-A";
const REQUIRED_KINDS: [DangerousNameKind; 6] = [
    DangerousNameKind::PathParts,
    DangerousNameKind::ReservedWindows,
    DangerousNameKind::VeryLong,
    DangerousNameKind::RightToLeft,
    DangerousNameKind::LookalikeA,
    DangerousNameKind::LookalikeB,
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum DangerousNameKind {
    PathParts,
    ReservedWindows,
    VeryLong,
    RightToLeft,
    LookalikeA,
    LookalikeB,
}

impl DangerousNameKind {
    fn label(self) -> &'static str {
        match self {
            Self::PathParts => "path_parts",
            Self::ReservedWindows => "reserved_windows",
            Self::VeryLong => "very_long",
            Self::RightToLeft => "right_to_left",
            Self::LookalikeA => "lookalike_a",
            Self::LookalikeB => "lookalike_b",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    files: Vec<FixtureName>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureName {
    kind: DangerousNameKind,
    name: String,
}

#[test]
fn task_3222_hostile_names_stay_inside_chosen_folder_without_overwrite_or_display_drift() {
    let fixture_path = std::env::var_os("TASK3222_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FIXTURE));
    let fixtures =
        load_and_validate_fixture(&fixture_path).unwrap_or_else(|error| panic!("{error}"));

    let root = tempfile::Builder::new()
        .prefix("osl-task-3222-")
        .tempdir()
        .expect("create isolated dangerous-name root");
    let transport_folder = root.path().join("transport-not-downloads");
    let parent_folder = root.path().join("download-parent");
    let chosen_folder = parent_folder.join("chosen");
    fs::create_dir(&transport_folder).expect("create transport folder");
    fs::create_dir(&parent_folder).expect("create download parent");
    fs::create_dir(&chosen_folder).expect("create chosen download folder");

    let parent_sentinel = parent_folder.join("parent-existing.txt");
    let chosen_sentinel = chosen_folder.join("chosen-existing.txt");
    fs::write(&parent_sentinel, b"TASK3222 parent sentinel").expect("write parent sentinel");
    fs::write(&chosen_sentinel, b"TASK3222 chosen sentinel").expect("write chosen sentinel");
    let parent_entries_before = direct_entry_count(&parent_folder);
    let parent_files_before = direct_regular_file_count(&parent_folder);

    let mut expected_files = BTreeSet::from([parent_sentinel.clone(), chosen_sentinel.clone()]);
    let mut output_names = BTreeSet::new();
    let mut sender_receiver_same_name_count = 0usize;
    let mut collision_refusal_count = 0usize;

    for (index, fixture) in fixtures.iter().enumerate() {
        let payload = format!(
            "TASK3222 valid attachment bytes category={} ordinal={index}",
            fixture.kind.label()
        )
        .into_bytes();
        let key_bytes = [u8::try_from(index + 1).expect("six fixtures fit in u8"); 32];

        let sealed = ipc::attachment_wire::seal_attachment(
            aead::Key::from_bytes(key_bytes),
            &payload,
            &fixture.name,
        )
        .expect("sender seals a valid file carrying its hostile logical name");
        let (received_bytes, received_name) =
            ipc::attachment_wire::open_attachment(aead::Key::from_bytes(key_bytes), &sealed)
                .expect("receiver authenticates the sent file and logical name");

        assert_eq!(received_bytes, payload);
        assert_eq!(
            received_name.as_bytes(),
            fixture.name.as_bytes(),
            "sender and receiver filename bytes differ for {}",
            fixture.kind.label()
        );
        sender_receiver_same_name_count += 1;

        let safe_output_name = safe_attachment_output_name(&received_name)
            .expect("hostile logical name maps to a portable output leaf");
        assert!(safe_output_name.is_ascii());
        assert!(safe_output_name.len() <= MAX_SAFE_ATTACHMENT_FILENAME_BYTES);
        assert_eq!(Path::new(&safe_output_name).components().count(), 1);
        assert!(output_names.insert(safe_output_name.clone()));

        let saved = save_received_attachment_bytes(&chosen_folder, &received_name, &received_bytes)
            .expect("receiver saves authenticated bytes in the chosen folder");
        assert_eq!(saved.display_name.as_bytes(), fixture.name.as_bytes());
        assert_eq!(
            saved.path.parent(),
            Some(chosen_folder.as_path()),
            "{OUTSIDE_CHOSEN_FOLDER_RESULT} is the result that should have been refused: attachment landed outside chosen folder kind={} name={:?} path={}",
            fixture.kind.label(),
            fixture.name,
            saved.path.display()
        );
        assert_eq!(
            saved.path.file_name().and_then(|name| name.to_str()),
            Some(safe_output_name.as_str())
        );
        assert_eq!(fs::read(&saved.path).expect("read saved output"), payload);
        expected_files.insert(saved.path.clone());

        let replacement = format!("TASK3222 forbidden replacement {index}");
        let error =
            save_received_attachment_bytes(&chosen_folder, &received_name, replacement.as_bytes())
                .expect_err("an existing output must never be overwritten");
        assert!(error.contains("could not be reserved"));
        assert_eq!(
            fs::read(&saved.path).expect("read collision-protected output"),
            payload
        );
        collision_refusal_count += 1;

        println!(
            "TASK3222_NAME kind={} sender={:?} receiver={:?} output={:?}",
            fixture.kind.label(),
            fixture.name,
            received_name,
            safe_output_name
        );
    }

    let actual_files = recursive_regular_files(&parent_folder);
    let parent_entries_after = direct_entry_count(&parent_folder);
    let parent_files_after = direct_regular_file_count(&parent_folder);
    let landed_file_count = actual_files.len() - 2;
    let outside_chosen_file_count = actual_files
        .iter()
        .filter(|path| {
            **path != parent_sentinel
                && **path != chosen_sentinel
                && path.parent() != Some(chosen_folder.as_path())
        })
        .count();

    assert_eq!(actual_files, expected_files);
    assert_eq!(landed_file_count, REQUIRED_KINDS.len());
    assert_eq!(
        outside_chosen_file_count, 0,
        "{OUTSIDE_CHOSEN_FOLDER_RESULT} is the result that should have been refused: files outside chosen folder expected=0 actual={outside_chosen_file_count}"
    );
    assert_eq!(parent_entries_after, parent_entries_before);
    assert_eq!(parent_files_after, parent_files_before);
    assert_eq!(
        fs::read(&parent_sentinel).unwrap(),
        b"TASK3222 parent sentinel"
    );
    assert_eq!(
        fs::read(&chosen_sentinel).unwrap(),
        b"TASK3222 chosen sentinel"
    );
    assert_eq!(collision_refusal_count, REQUIRED_KINDS.len());
    assert_eq!(sender_receiver_same_name_count, REQUIRED_KINDS.len());

    println!("TASK3222_FIXTURE={}", fixture_path.display());
    println!(
        "TASK3222_REQUIRED_DANGEROUS_NAME_COUNT={}",
        REQUIRED_KINDS.len()
    );
    println!("TASK3222_LANDED_FILE_COUNT={landed_file_count}");
    println!("TASK3222_OUTSIDE_CHOSEN_FILE_COUNT={outside_chosen_file_count}");
    println!("TASK3222_PARENT_ENTRY_COUNT_BEFORE={parent_entries_before}");
    println!("TASK3222_PARENT_ENTRY_COUNT_AFTER={parent_entries_after}");
    println!("TASK3222_PARENT_FILE_COUNT_BEFORE={parent_files_before}");
    println!("TASK3222_PARENT_FILE_COUNT_AFTER={parent_files_after}");
    println!("TASK3222_EXISTING_FILE_OVERWRITE_COUNT=0");
    println!("TASK3222_COLLISION_REFUSAL_COUNT={collision_refusal_count}");
    println!("TASK3222_SENDER_RECEIVER_SAME_NAME_COUNT={sender_receiver_same_name_count}");
}

fn load_and_validate_fixture(path: &Path) -> Result<Vec<FixtureName>, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "TASK3222 could not read fixture {}: {error}",
            path.display()
        )
    })?;
    let manifest: FixtureManifest = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "TASK3222 could not parse fixture {}: {error}",
            path.display()
        )
    })?;

    for required in REQUIRED_KINDS {
        let count = manifest
            .files
            .iter()
            .filter(|fixture| fixture.kind == required)
            .count();
        if count != 1 {
            return Err(format!(
                "TASK3222 dangerous filename kind={} expected=1 actual={count}",
                required.label()
            ));
        }
    }
    if manifest.files.len() != REQUIRED_KINDS.len() {
        return Err(format!(
            "TASK3222 dangerous filename count expected={} actual={}",
            REQUIRED_KINDS.len(),
            manifest.files.len()
        ));
    }

    let named = |kind| {
        manifest
            .files
            .iter()
            .find(|fixture| fixture.kind == kind)
            .expect("required category counted")
            .name
            .as_str()
    };
    if !named(DangerousNameKind::PathParts).contains(['/', '\\'])
        || !named(DangerousNameKind::PathParts)
            .split(['/', '\\'])
            .any(|part| part == "..")
    {
        return Err("TASK3222 path_parts filename has no traversal folder part".to_owned());
    }
    let reserved = named(DangerousNameKind::ReservedWindows)
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    if !matches!(
        reserved.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "LPT1"
    ) {
        return Err("TASK3222 reserved_windows filename is not a Windows device name".to_owned());
    }
    if named(DangerousNameKind::VeryLong).len() <= 255 {
        return Err("TASK3222 very_long filename is not longer than 255 bytes".to_owned());
    }
    if !named(DangerousNameKind::RightToLeft)
        .chars()
        .any(|character| matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    {
        return Err("TASK3222 right_to_left filename has no bidi direction mark".to_owned());
    }
    let lookalike_a = named(DangerousNameKind::LookalikeA);
    let lookalike_b = named(DangerousNameKind::LookalikeB);
    if lookalike_a == lookalike_b
        || lookalike_skeleton(lookalike_a) != lookalike_skeleton(lookalike_b)
    {
        return Err(
            "TASK3222 lookalike filenames are not distinct encodings of the same visible name"
                .to_owned(),
        );
    }
    for fixture in &manifest.files {
        if fixture.name.len() > ipc::attachment_wire::MAX_FILENAME_LEN {
            return Err(format!(
                "TASK3222 {} filename exceeds the authenticated wire bound",
                fixture.kind.label()
            ));
        }
        if ipc::attachment_wire::mime_for_filename(&fixture.name).is_none() {
            return Err(format!(
                "TASK3222 {} filename has no valid attachment ending",
                fixture.kind.label()
            ));
        }
    }
    Ok(manifest.files)
}

fn lookalike_skeleton(name: &str) -> String {
    name.chars()
        .filter_map(|character| match character {
            'é' => Some('e'),
            '\u{0301}' => None,
            other => Some(other),
        })
        .collect()
}

fn direct_entry_count(path: &Path) -> usize {
    fs::read_dir(path).expect("read direct entries").count()
}

fn direct_regular_file_count(path: &Path) -> usize {
    fs::read_dir(path)
        .expect("read direct files")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .count()
}

fn recursive_regular_files(path: &Path) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    let mut pending = vec![path.to_owned()];
    while let Some(folder) = pending.pop() {
        for entry in fs::read_dir(folder).expect("walk download parent") {
            let entry = entry.expect("read download entry");
            let kind = entry.file_type().expect("read download entry type");
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.insert(entry.path());
            } else {
                panic!(
                    "TASK3222 unexpected non-file output: {}",
                    entry.path().display()
                );
            }
        }
    }
    files
}
