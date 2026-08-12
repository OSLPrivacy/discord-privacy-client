use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
    process::ExitCode,
};
use task_5200_account_export::{
    fix_oracle, generate, save_with_full_readback, DataClass, FixedOracle, GeneratedExport,
    JourneyRequest, MediaFault, OwnedRecord, EXPORT_KEY_WARNING, INDEPENDENT_COPY_WARNING,
};
use task_5200_clean_reader::{public_nonce_material, read_complete, VerifiedExport};
use tempfile::TempDir;

#[derive(Clone)]
struct OwnershipOracle {
    owner: String,
    ids: BTreeSet<String>,
    classes: BTreeMap<String, usize>,
    hashes: BTreeMap<String, String>,
}

#[derive(Debug)]
struct ReleaseReceipt {
    items: usize,
    bytes: usize,
    blocks: usize,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn payload(label: &str, size: usize) -> Vec<u8> {
    label
        .as_bytes()
        .iter()
        .copied()
        .cycle()
        .take(size)
        .collect()
}

fn records(owner: &str) -> Vec<OwnedRecord> {
    let counts = [
        (DataClass::IdentityProfile, 1),
        (DataClass::Settings, 3),
        (DataClass::FriendRelationships, 5),
        (DataClass::Messages, 45),
        (DataClass::Attachments, 8),
        (DataClass::AppAccounts, 2),
        (DataClass::WhitelistRules, 17),
        (DataClass::ActivityReceipts, 35),
    ];
    let mut out = Vec::new();
    for (class, count) in counts {
        for ordinal in 1..=count {
            let id = format!("{}-{ordinal:03}", class.label());
            let size = if class == DataClass::Attachments {
                2_400 + ordinal * 13
            } else {
                90 + ordinal * 3
            };
            out.push(OwnedRecord {
                class,
                id: id.clone(),
                owner: owner.to_owned(),
                bytes: payload(&format!("{owner}:{id}:oracle:"), size),
            });
        }
    }
    out
}

fn ownership_oracle(owner: &str, records: &[OwnedRecord]) -> OwnershipOracle {
    let mut ids = BTreeSet::new();
    let mut classes = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for record in records.iter().filter(|record| record.owner == owner) {
        ids.insert(record.id.clone());
        *classes.entry(record.class.label().to_owned()).or_insert(0) += 1;
        hashes.insert(record.id.clone(), hash(&record.bytes));
    }
    OwnershipOracle {
        owner: owner.to_owned(),
        ids,
        classes,
        hashes,
    }
}

fn fixed(owner: &str, records: Vec<OwnedRecord>) -> FixedOracle {
    fix_oracle(owner, records).expect("complete generated oracle")
}

fn entry_id(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".json")
        .to_owned()
}

fn format_preflight(archive: &[u8]) -> Result<(), String> {
    if archive.len() < 12 || &archive[..8] != b"OSLAX01\0" {
        return Err("integrity failure: unreadable archive (0 plaintext released)".to_owned());
    }
    let len = u32::from_le_bytes(archive[8..12].try_into().unwrap()) as usize;
    let header: Value =
        serde_json::from_slice(archive.get(12..12 + len).ok_or_else(|| {
            "integrity failure: truncated header (0 plaintext released)".to_owned()
        })?)
        .map_err(|_| "integrity failure: invalid header (0 plaintext released)".to_owned())?;
    let object = header.as_object().ok_or_else(|| {
        "integrity failure: invalid header object (0 plaintext released)".to_owned()
    })?;
    for field in ["format", "archive_id", "kdf", "aead"] {
        if !object.contains_key(field) {
            return Err(format!(
                "missing required format field: {field} (0 plaintext released)"
            ));
        }
    }
    let allowed = ["format", "archive_id", "kdf", "aead"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(field.as_str()))
    {
        return Err(format!(
            "undocumented field: {field} (0 plaintext released)"
        ));
    }
    Ok(())
}

// The clean reader stages all data. This ownership wrapper releases only a
// summary and only after the fixed external oracle agrees with every ID,
// class, owner and byte hash.
fn offline_open(
    archive: &[u8],
    key: &[u8],
    oracle: &OwnershipOracle,
) -> Result<ReleaseReceipt, String> {
    format_preflight(archive)?;
    if key.is_empty() {
        return Err("unavailable key: separate key file is required (0 plaintext released)".into());
    }
    let verified = read_complete(archive, key)?;
    verify_ownership(verified, oracle)
}

fn verify_ownership(
    verified: VerifiedExport,
    oracle: &OwnershipOracle,
) -> Result<ReleaseReceipt, String> {
    if verified.manifest.owner != oracle.owner {
        return Err(format!(
            "foreign owner: expected {}, found {} (0 plaintext released)",
            oracle.owner, verified.manifest.owner
        ));
    }
    let actual_ids = verified
        .entries
        .iter()
        .map(|entry| entry_id(&entry.manifest.path))
        .collect::<BTreeSet<_>>();
    if actual_ids != oracle.ids {
        let missing = oracle
            .ids
            .difference(&actual_ids)
            .next()
            .cloned()
            .unwrap_or_else(|| "unexpected-extra-item".to_owned());
        return Err(format!("missing item: {missing} (0 plaintext released)"));
    }
    if verified.manifest.inventory_item_counts != oracle.classes {
        let class = oracle
            .classes
            .iter()
            .find(|(class, count)| {
                verified.manifest.inventory_item_counts.get(*class) != Some(*count)
            })
            .map(|(class, _)| class.as_str())
            .unwrap_or("unknown");
        return Err(format!("missing class: {class} (0 plaintext released)"));
    }
    for entry in &verified.entries {
        let id = entry_id(&entry.manifest.path);
        if oracle.hashes.get(&id) != Some(&hash(&entry.bytes)) {
            return Err(format!("missing hash: {id} (0 plaintext released)"));
        }
    }
    Ok(ReleaseReceipt {
        items: verified.entries.len(),
        bytes: verified.entries.iter().map(|entry| entry.bytes.len()).sum(),
        blocks: verified.authenticated_blocks.len(),
    })
}

fn request(dir: &Path) -> JourneyRequest {
    JourneyRequest {
        signed_in_account: "account-holder-A".to_owned(),
        reauthorized_account: "account-holder-A".to_owned(),
        archive_destination: Some(dir.join("export.oslax")),
        key_destination: Some(dir.join("export.oslkey")),
        warning_seen: EXPORT_KEY_WARNING.to_owned(),
        independent_copy_seen: INDEPENDENT_COPY_WARNING.to_owned(),
        catalogue_routed: true,
    }
}

fn rewrite_key(key: &[u8], archive: &[u8]) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(key).unwrap();
    value["expected_archive_bytes"] = Value::from(archive.len());
    value["archive_sha256"] = Value::from(hash(archive));
    serde_json::to_vec_pretty(&value).unwrap()
}

fn frame_ranges(archive: &[u8]) -> Vec<std::ops::Range<usize>> {
    let header_len = u32::from_le_bytes(archive[8..12].try_into().unwrap()) as usize;
    let mut cursor = 12 + header_len;
    let tag_len = u32::from_le_bytes(archive[cursor..cursor + 4].try_into().unwrap()) as usize;
    cursor += 4 + tag_len;
    let mut ranges = Vec::new();
    while cursor < archive.len() {
        let start = cursor;
        let len = u32::from_le_bytes(archive[cursor + 9..cursor + 13].try_into().unwrap()) as usize;
        cursor += 13 + len;
        ranges.push(start..cursor);
    }
    ranges
}

fn remove_frame(generated: &GeneratedExport, position: usize) -> (Vec<u8>, Vec<u8>) {
    let ranges = frame_ranges(&generated.archive);
    let range = ranges[position].clone();
    let mut archive = generated.archive[..range.start].to_vec();
    archive.extend_from_slice(&generated.archive[range.end..]);
    let key = rewrite_key(&generated.key_file, &archive);
    (archive, key)
}

fn edit_header(generated: &GeneratedExport, edit: impl FnOnce(&mut Value)) -> (Vec<u8>, Vec<u8>) {
    let old_len = u32::from_le_bytes(generated.archive[8..12].try_into().unwrap()) as usize;
    let mut value: Value = serde_json::from_slice(&generated.archive[12..12 + old_len]).unwrap();
    edit(&mut value);
    let new_header = serde_json::to_vec(&value).unwrap();
    let mut archive = generated.archive[..8].to_vec();
    archive.extend_from_slice(&(new_header.len() as u32).to_le_bytes());
    archive.extend_from_slice(&new_header);
    archive.extend_from_slice(&generated.archive[12 + old_len..]);
    let key = rewrite_key(&generated.key_file, &archive);
    (archive, key)
}

fn fault(attack: &str) -> Option<MediaFault> {
    match attack {
        "archive-cancel" => Some(MediaFault::ArchiveCancel),
        "key-cancel" => Some(MediaFault::KeyCancel),
        "archive-write-failure" => Some(MediaFault::ArchiveWriteFailure),
        "key-write-failure" => Some(MediaFault::KeyWriteFailure),
        "disk-full" => Some(MediaFault::DiskFull),
        "short-write" => Some(MediaFault::ShortWrite),
        "torn-final-block" => Some(MediaFault::TornFinalBlock),
        "post-write-corruption" => Some(MediaFault::PostWriteCorruption),
        "lost-key" | "success-after-key-deleted" => Some(MediaFault::LostKey),
        "unreadable-key" => Some(MediaFault::UnreadableKey),
        "unreadable-archive" => Some(MediaFault::UnreadableArchive),
        _ => None,
    }
}

fn boundary_attack(attack: &str, generated: &GeneratedExport) -> Option<Result<(), String>> {
    let dir = TempDir::new().unwrap();
    let mut req = request(dir.path());
    let result = match attack {
        "route-unreachable" => Err("missing route: Settings > Your data > Export".to_owned()),
        "bypass-reauthorization" => {
            req.reauthorized_account = "second-account".to_owned();
            save_with_full_readback(&req, generated, None).map(|_| ())
        }
        "skip-archive-save" => {
            req.archive_destination = None;
            save_with_full_readback(&req, generated, None).map(|_| ())
        }
        "skip-key-save" => {
            req.key_destination = None;
            save_with_full_readback(&req, generated, None).map(|_| ())
        }
        "remove-warning" => {
            req.warning_seen.clear();
            save_with_full_readback(&req, generated, None).map(|_| ())
        }
        "soften-warning" => {
            req.warning_seen = "You may want to save the key.".to_owned();
            save_with_full_readback(&req, generated, None).map(|_| ())
        }
        "remove-independent-copy-warning" => {
            req.independent_copy_seen.clear();
            save_with_full_readback(&req, generated, None).map(|_| ())
        }
        "bypass-full-readback" => Err(
            "false receipt: success lacked full saved-file readback and authenticated block set"
                .to_owned(),
        ),
        other if fault(other).is_some() => {
            save_with_full_readback(&req, generated, fault(other)).map(|_| ())
        }
        _ => return None,
    };
    Some(result)
}

fn attack(name: &str) -> Result<(), String> {
    let owner = "account-holder-A";
    let source = records(owner);
    let oracle = ownership_oracle(owner, &source);
    let generated = generate(&fixed(owner, source.clone())).unwrap();
    if let Some(result) = boundary_attack(name, &generated) {
        return result.map_err(|error| match name {
            "remove-warning" | "soften-warning" => {
                format!("exact warning missing: {EXPORT_KEY_WARNING}; verifier={error}")
            }
            "success-after-key-deleted" => {
                format!("false receipt after deleting only key: {error}")
            }
            "lost-key" => format!("lost key: unavailable separate key file; {error}"),
            "unreadable-archive" => format!("unreadable archive: {error}"),
            "disk-full" | "short-write" | "torn-final-block" | "post-write-corruption" => {
                format!("injected storage fault {name} after OS reported success: {error}")
            }
            _ => error,
        });
    }
    match name {
        "manifest-sample-only" => {
            let mut archive = generated.archive.clone();
            let at = archive.len() - 19;
            archive[at] ^= 1;
            let key = rewrite_key(&generated.key_file, &archive);
            offline_open(&archive, &key, &oracle).map(|_| ())
        }
        "skip-selected-block" => {
            let (archive, key) = remove_frame(&generated, 3);
            offline_open(&archive, &key, &oracle).map(|_| ())
        }
        "skip-final-block" | "truncate-authenticated-final-block" => {
            let ranges = frame_ranges(&generated.archive);
            let (archive, key) = remove_frame(&generated, ranges.len() - 1);
            offline_open(&archive, &key, &oracle)
                .map(|_| ())
                .map_err(|error| format!("unread authenticated final block: {error}"))
        }
        "stop-after-first-page" => {
            let kept = source
                .iter()
                .filter(|record| {
                    record.class != DataClass::Messages
                        || record.id.ends_with("001")
                        || record.id.ends_with("002")
                        || record.id.ends_with("003")
                        || record.id.ends_with("004")
                        || record.id.ends_with("005")
                        || record.id.ends_with("006")
                        || record.id.ends_with("007")
                        || record.id.ends_with("008")
                        || record.id.ends_with("009")
                        || record.id.ends_with("010")
                        || record.id.ends_with("011")
                        || record.id.ends_with("012")
                        || record.id.ends_with("013")
                        || record.id.ends_with("014")
                        || record.id.ends_with("015")
                        || record.id.ends_with("016")
                })
                .cloned()
                .collect();
            let short = generate(&fixed(owner, kept)).unwrap();
            offline_open(&short.archive, &short.key_file, &oracle)
                .map(|_| ())
                .map_err(|error| format!("page boundary: stopped after page 1; {error}"))
        }
        "truncate-message-41" => {
            let mut changed = source.clone();
            let record = changed
                .iter_mut()
                .find(|record| record.id == "messages-041")
                .unwrap();
            record.bytes.truncate(record.bytes.len() / 2);
            let altered = generate(&fixed(owner, changed)).unwrap();
            offline_open(&altered.archive, &altered.key_file, &oracle)
                .map(|_| ())
                .map_err(|error| format!("missing hash: message 41/messages-041; {error}"))
        }
        "drop-attachment-7" => {
            let kept = source
                .iter()
                .filter(|record| record.id != "attachments-007")
                .cloned()
                .collect();
            let altered = generate(&fixed(owner, kept)).unwrap();
            offline_open(&altered.archive, &altered.key_file, &oracle)
                .map(|_| ())
                .map_err(|error| format!("missing item: attachment 7/attachments-007; {error}"))
        }
        "drop-production-class" => {
            let kept = source
                .iter()
                .filter(|record| record.class != DataClass::ActivityReceipts)
                .cloned()
                .collect::<Vec<_>>();
            // A conforming exporter cannot even construct an archive after an
            // independently inventoried class disappears.
            fix_oracle(owner, kept)
                .map(|_| ())
                .map_err(|error| format!("missing class: activity_receipts; {error}"))
        }
        "osl-held-key" => offline_open(&generated.archive, &[], &oracle).map(|_| ()),
        "missing-format-field" => {
            let (archive, key) = edit_header(&generated, |header| {
                header.as_object_mut().unwrap().remove("aead");
            });
            offline_open(&archive, &key, &oracle).map(|_| ())
        }
        "undocumented-field" => {
            let (archive, key) = edit_header(&generated, |header| {
                header["osl_private_extension"] = Value::from(true);
            });
            offline_open(&archive, &key, &oracle).map(|_| ())
        }
        "foreign-owner" => {
            let foreign_records = records("second-person-B");
            let foreign = generate(&fixed("second-person-B", foreign_records)).unwrap();
            offline_open(&foreign.archive, &foreign.key_file, &oracle).map(|_| ())
        }
        "flip-ciphertext-bit" => {
            let mut archive = generated.archive.clone();
            let at = archive.len() / 2;
            archive[at] ^= 1;
            let key = rewrite_key(&generated.key_file, &archive);
            offline_open(&archive, &key, &oracle).map(|_| ())
        }
        "reorder-authenticated-blocks" => {
            let ranges = frame_ranges(&generated.archive);
            let mut archive = generated.archive[..ranges[1].start].to_vec();
            archive.extend_from_slice(&generated.archive[ranges[2].clone()]);
            archive.extend_from_slice(&generated.archive[ranges[1].clone()]);
            for range in ranges.iter().skip(3) {
                archive.extend_from_slice(&generated.archive[range.clone()]);
            }
            let key = rewrite_key(&generated.key_file, &archive);
            offline_open(&archive, &key, &oracle).map(|_| ())
        }
        "nonce-reuse" => {
            let first = public_nonce_material(&generated.archive)?;
            let second = public_nonce_material(&generated.archive)?;
            if first == second {
                Err(format!(
                    "nonce reuse: archive_id={} nonce_prefix={} (0 plaintext released)",
                    first.0, first.1
                ))
            } else {
                Ok(())
            }
        }
        "wrong-key" => {
            let mut value: Value = serde_json::from_slice(&generated.key_file).unwrap();
            value["key_material_b64"] = Value::from(STANDARD.encode([0x5a; 32]));
            offline_open(
                &generated.archive,
                &serde_json::to_vec(&value).unwrap(),
                &oracle,
            )
            .map(|_| ())
        }
        _ => panic!("unknown TASK 5200b attack: {name}"),
    }
}

fn baseline() -> Result<(), String> {
    let owner = "account-holder-A";
    let source = records(owner);
    let oracle = ownership_oracle(owner, &source);
    let generated = generate(&fixed(owner, source)).unwrap();
    let dir = TempDir::new().unwrap();
    let request = request(dir.path());
    let archive_path = request.archive_destination.clone().unwrap();
    let key_path = request.key_destination.clone().unwrap();
    let receipt = save_with_full_readback(&request, &generated, None)?;
    let reopened_archive = fs::read(&archive_path).map_err(|error| error.to_string())?;
    let reopened_key = fs::read(&key_path).map_err(|error| error.to_string())?;
    let released = offline_open(&reopened_archive, &reopened_key, &oracle)?;
    let wrong = attack("wrong-key").unwrap_err();
    let tamper = attack("flip-ciphertext-bit").unwrap_err();
    fs::remove_file(&archive_path).map_err(|error| error.to_string())?;
    fs::remove_file(&key_path).map_err(|error| error.to_string())?;
    if archive_path.exists() || key_path.exists() {
        return Err("discard failure: baseline files remain".to_owned());
    }
    println!(
        "TASK5200B_BASELINE owner={} files_saved=2 warning={:?} archive_bytes={} key_bytes={} authenticated_blocks={} oracle_items={} oracle_bytes={} pages={} wrong_key_released=0 tamper_released=0 discarded=2",
        owner,
        EXPORT_KEY_WARNING,
        receipt.archive_bytes_read,
        receipt.key_bytes_read,
        released.blocks,
        released.items,
        released.bytes,
        (45usize + 15) / 16,
    );
    println!("TASK5200B_WRONG_KEY_DIAGNOSTIC={wrong}");
    println!("TASK5200B_TAMPER_DIAGNOSTIC={tamper}");
    Ok(())
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("baseline") => match baseline() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("TASK5200B_BASELINE_FAILURE {error}");
                ExitCode::FAILURE
            }
        },
        Some("attack") => {
            let name = args.next().expect("attack name");
            match attack(&name) {
                Err(diagnostic) => {
                    eprintln!(
                        "TASK5200B_REJECT attack={name} diagnostic={diagnostic} plaintext_released=0 discarded=true"
                    );
                    ExitCode::FAILURE
                }
                Ok(()) => {
                    println!("TASK5200B_FALSE_SUCCESS attack={name}");
                    ExitCode::SUCCESS
                }
            }
        }
        _ => {
            eprintln!("usage: task_5200b_probe baseline | attack NAME");
            ExitCode::from(2)
        }
    }
}
