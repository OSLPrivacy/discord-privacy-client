use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::{rngs::OsRng, RngCore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use task_5200_account_export::*;
use task_5200_clean_reader::{read_complete, VerifiedExport};
use tempfile::TempDir;

fn independent_reader(archive: &[u8], key: &[u8]) -> Result<(usize, BTreeSet<u64>), String> {
    let verified = read_complete(archive, key)?;
    Ok((verified.archive_bytes, verified.authenticated_blocks))
}

fn bytes(label: &str, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        out.extend_from_slice(label.as_bytes());
        out.extend_from_slice(&(out.len() as u64).to_le_bytes());
    }
    out.truncate(n);
    out
}

fn seeded_oracle() -> (u64, FixedOracle, BTreeSet<String>) {
    let seed = OsRng.next_u64();
    let owner = "account-holder-A";
    let other = "independent-account-B";
    let mut records = Vec::new();
    let mut second_ids = BTreeSet::new();
    let counts = [
        (DataClass::IdentityProfile, 1usize),
        (DataClass::Settings, 1 + (seed as usize % 5)),
        (DataClass::FriendRelationships, 4 + (seed as usize % 4)),
        (DataClass::Messages, 41 + (seed as usize % 11)),
        (DataClass::Attachments, 7 + (seed as usize % 5)),
        (DataClass::AppAccounts, 1 + (seed as usize % 3)),
        (DataClass::WhitelistRules, 17 + (seed as usize % 5)),
        (DataClass::ActivityReceipts, 33 + (seed as usize % 9)),
    ];
    for (class, count) in counts {
        for index in 0..count {
            let id = format!("{}-{index:04}", class.label());
            let size = if class == DataClass::Attachments {
                2051 + ((seed as usize + index * 137) % 1500)
            } else {
                70 + ((seed as usize + index * 17) % 150)
            };
            records.push(OwnedRecord {
                class,
                id: id.clone(),
                owner: owner.to_owned(),
                bytes: bytes(&format!("{owner}:{id}:all-fields:"), size),
            });
        }
        let canary_id = format!("B-CANARY-{}", class.label());
        second_ids.insert(canary_id.clone());
        records.push(OwnedRecord {
            class,
            id: canary_id.clone(),
            owner: other.to_owned(),
            bytes: bytes(&format!("{other}:{canary_id}"), 333),
        });
    }
    (seed, fix_oracle(owner, records).unwrap(), second_ids)
}

fn request(dir: &Path) -> JourneyRequest {
    JourneyRequest {
        signed_in_account: "account-holder-A".to_owned(),
        reauthorized_account: "account-holder-A".to_owned(),
        archive_destination: Some(dir.join("person-export.oslax")),
        key_destination: Some(dir.join("person-export.oslkey")),
        warning_seen: EXPORT_KEY_WARNING.to_owned(),
        independent_copy_seen: INDEPENDENT_COPY_WARNING.to_owned(),
        catalogue_routed: true,
    }
}

fn rewrite_key_for_archive(key: &[u8], archive: &[u8]) -> Vec<u8> {
    let mut doc: Value = serde_json::from_slice(key).unwrap();
    doc["expected_archive_bytes"] = Value::from(archive.len());
    doc["archive_sha256"] = Value::from(format!("{:x}", Sha256::digest(archive)));
    serde_json::to_vec_pretty(&doc).unwrap()
}

fn frame_ranges(archive: &[u8]) -> Vec<std::ops::Range<usize>> {
    let mut cursor = 8;
    let header_len = u32::from_le_bytes(archive[cursor..cursor + 4].try_into().unwrap()) as usize;
    cursor += 4 + header_len;
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RequiredMutant {
    RouteUnreachable,
    BypassReauth,
    SkipArchiveSave,
    SkipKeySave,
    RemoveWarning,
    SoftenWarning,
    RouteAroundCatalogue,
    ManifestSampleOnly,
    SkipSelectedBlock,
    SkipFinalBlock,
    SuccessAfterKeyDeleted,
    ArchiveCancel,
    KeyCancel,
    ArchiveWriteFailure,
    KeyWriteFailure,
    DiskFull,
    ShortWrite,
    TornFinalBlock,
    PostWriteCorruption,
    LostKey,
    UnreadableKey,
    UnreadableArchive,
    StopAfterFirstPage,
    OmitGeneratedField,
    AcceptTamper,
    ReuseNonceMaterial,
    RequireOslHeldSecret,
    LeakSecondPersonCanary,
}

impl RequiredMutant {
    const ALL: [Self; 28] = [
        Self::RouteUnreachable,
        Self::BypassReauth,
        Self::SkipArchiveSave,
        Self::SkipKeySave,
        Self::RemoveWarning,
        Self::SoftenWarning,
        Self::RouteAroundCatalogue,
        Self::ManifestSampleOnly,
        Self::SkipSelectedBlock,
        Self::SkipFinalBlock,
        Self::SuccessAfterKeyDeleted,
        Self::ArchiveCancel,
        Self::KeyCancel,
        Self::ArchiveWriteFailure,
        Self::KeyWriteFailure,
        Self::DiskFull,
        Self::ShortWrite,
        Self::TornFinalBlock,
        Self::PostWriteCorruption,
        Self::LostKey,
        Self::UnreadableKey,
        Self::UnreadableArchive,
        Self::StopAfterFirstPage,
        Self::OmitGeneratedField,
        Self::AcceptTamper,
        Self::ReuseNonceMaterial,
        Self::RequireOslHeldSecret,
        Self::LeakSecondPersonCanary,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::RouteUnreachable => "missing user boundary: Settings route unreachable",
            Self::BypassReauth => "missing user boundary: bypass reauthorization",
            Self::SkipArchiveSave => "missing user boundary: skip archive native save",
            Self::SkipKeySave => "missing user boundary: skip key native save",
            Self::RemoveWarning => "missing user boundary: remove export-key warning",
            Self::SoftenWarning => "missing user boundary: soften export-key warning",
            Self::RouteAroundCatalogue => "missing user boundary: warning routed around catalogue",
            Self::ManifestSampleOnly => "false success: verify manifest/sample only",
            Self::SkipSelectedBlock => "unread block: independently selected non-manifest block",
            Self::SkipFinalBlock => "unread block: final block",
            Self::SuccessAfterKeyDeleted => "false success: key deleted before full readback",
            Self::ArchiveCancel => "missing user boundary: archive native save cancelled",
            Self::KeyCancel => "missing user boundary: key native save cancelled",
            Self::ArchiveWriteFailure => "unread block: archive failed write",
            Self::KeyWriteFailure => "unread block: key failed write",
            Self::DiskFull => "unread block: disk-full",
            Self::ShortWrite => "integrity failure: short write",
            Self::TornFinalBlock => "integrity failure: torn final block",
            Self::PostWriteCorruption => "integrity failure: post-write corruption",
            Self::LostKey => "unread block: lost key",
            Self::UnreadableKey => "integrity failure: unreadable key",
            Self::UnreadableArchive => "integrity failure: unreadable archive",
            Self::StopAfterFirstPage => "pagination boundary: stopped after first page",
            Self::OmitGeneratedField => "omission: generated item/document field",
            Self::AcceptTamper => "cryptographic failure: accepted tamper",
            Self::ReuseNonceMaterial => "cryptographic failure: reused nonce material",
            Self::RequireOslHeldSecret => "portability failure: required OSL-held secret",
            Self::LeakSecondPersonCanary => "ownership leak: second-person canary",
        }
    }
}

fn verify_mutant_coverage(results: &BTreeMap<RequiredMutant, String>) -> Result<(), String> {
    for required in RequiredMutant::ALL {
        let diagnostic = results
            .get(&required)
            .ok_or_else(|| format!("absent starvation: {}", required.label()))?;
        if !diagnostic.contains(required.label().split(':').next().unwrap()) {
            return Err(format!("mutant did not name failure: {}", required.label()));
        }
    }
    Ok(())
}

#[test]
fn task_5200_whole_oracle_portable_clean_profile_and_crypto_attacks() {
    install_reader(independent_reader);
    let all = DataClass::ALL;
    verify_independent_inventories(&all, &all, &all).unwrap();
    let inventory = production_inventory();
    assert_eq!(inventory.len(), 8);
    assert!(inventory.iter().all(|r| r.page_items == 16));
    assert_eq!(
        inventory
            .iter()
            .find(|r| r.class == DataClass::Attachments)
            .unwrap()
            .chunk_bytes,
        1024
    );

    let (seed, oracle, second_ids) = seeded_oracle();
    assert_eq!(oracle.class_counts["identity_profile"], 1);
    assert!(oracle.class_counts["friend_relationships"] >= 4);
    assert!(oracle.class_counts["messages"] >= 41);
    assert!(oracle.class_counts["attachments"] >= 7);
    assert!(oracle.class_counts["messages"] > PAGE_ITEMS * 2);
    assert!(oracle
        .records
        .iter()
        .filter(|r| r.class == DataClass::Attachments)
        .all(|r| r.bytes.len() > BLOCK_BYTES * 2));

    let generated = generate(&oracle).unwrap();
    let verified: VerifiedExport = read_complete(&generated.archive, &generated.key_file).unwrap();
    let actual_ids = verified
        .entries
        .iter()
        .map(|e| {
            e.manifest
                .path
                .rsplit('/')
                .next()
                .unwrap()
                .trim_end_matches(".json")
                .to_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_ids, oracle.item_ids);
    assert!(actual_ids.is_disjoint(&second_ids));
    assert_eq!(verified.manifest.inventory_item_counts, oracle.class_counts);
    assert_eq!(verified.manifest.total_plaintext_bytes, oracle.total_bytes);
    assert_eq!(
        verified
            .entries
            .iter()
            .map(|e| e.bytes.len())
            .sum::<usize>(),
        oracle.total_bytes
    );
    assert_eq!(verified.archive_bytes, generated.archive.len());
    assert_eq!(verified.authenticated_blocks, generated.manifest_blocks);

    let wrong_key = {
        let mut v: Value = serde_json::from_slice(&generated.key_file).unwrap();
        v["key_material_b64"] = Value::from(STANDARD.encode([0xA5; 32]));
        serde_json::to_vec(&v).unwrap()
    };
    let wrong = read_complete(&generated.archive, &wrong_key).unwrap_err();
    assert!(
        wrong.contains("integrity failure") && wrong.contains("0 plaintext released"),
        "{wrong}"
    );
    let mut flipped = generated.archive.clone();
    let at = flipped.len() / 2;
    flipped[at] ^= 1;
    let flipped_key = rewrite_key_for_archive(&generated.key_file, &flipped);
    let bit = read_complete(&flipped, &flipped_key).unwrap_err();
    assert!(
        bit.contains("integrity failure") && bit.contains("0 plaintext released"),
        "{bit}"
    );
    let truncated = &generated.archive[..generated.archive.len() - 1];
    let truncated_key = rewrite_key_for_archive(&generated.key_file, truncated);
    let trunc = read_complete(truncated, &truncated_key).unwrap_err();
    assert!(
        trunc.contains("integrity failure") && trunc.contains("0 plaintext released"),
        "{trunc}"
    );
    let ranges = frame_ranges(&generated.archive);
    assert!(ranges.len() > 3);
    let mut reordered = generated.archive[..ranges[1].start].to_vec();
    reordered.extend_from_slice(&generated.archive[ranges[2].clone()]);
    reordered.extend_from_slice(&generated.archive[ranges[1].clone()]);
    for range in ranges.iter().skip(3) {
        reordered.extend_from_slice(&generated.archive[range.clone()]);
    }
    let reordered_key = rewrite_key_for_archive(&generated.key_file, &reordered);
    let reorder = read_complete(&reordered, &reordered_key).unwrap_err();
    assert!(
        reorder.contains("reordered authenticated block")
            && reorder.contains("0 plaintext released"),
        "{reorder}"
    );
    let repeated = generate(&oracle).unwrap();
    assert_ne!(generated.archive_id, repeated.archive_id);
    assert_ne!(generated.nonce_prefix, repeated.nonce_prefix);
    assert_ne!(generated.key_file, repeated.key_file);

    println!("TASK5200_GENERATOR_SEED={seed}");
    println!("TASK5200_CLASSES={}", inventory.len());
    println!(
        "TASK5200_IDENTITY_PROFILE={}",
        oracle.class_counts["identity_profile"]
    );
    println!("TASK5200_SETTINGS={}", oracle.class_counts["settings"]);
    println!(
        "TASK5200_FRIEND_RELATIONSHIPS={}",
        oracle.class_counts["friend_relationships"]
    );
    println!("TASK5200_MESSAGES={}", oracle.class_counts["messages"]);
    println!(
        "TASK5200_ATTACHMENTS={}",
        oracle.class_counts["attachments"]
    );
    println!("TASK5200_ORACLE_ITEMS={}", oracle.item_ids.len());
    println!("TASK5200_ORACLE_BYTES={}", oracle.total_bytes);
    println!("TASK5200_ARCHIVE_BYTES={}", generated.archive.len());
    println!(
        "TASK5200_AUTHENTICATED_BLOCKS={}",
        generated.manifest_blocks.len()
    );
    println!("TASK5200_SECOND_PERSON_BYTES=0");
    println!("TASK5200_ATTACKS=wrong-key,flipped-bit,truncated-block,reordered-block");
    println!("TASK5200_ATTACK_PLAINTEXT_RELEASED=0");
    println!("TASK5200_NONCE_MATERIAL_REUSED=0");
}

#[test]
fn task_5200_packaged_full_readback_faults_and_5200b_starvation() {
    install_reader(independent_reader);
    let (_, oracle, _) = seeded_oracle();
    let generated = generate(&oracle).unwrap();
    let success_dir = TempDir::new().unwrap();
    let good = request(success_dir.path());
    let receipt = save_with_full_readback(&good, &generated, None).unwrap();
    assert_eq!(receipt.archive_bytes_read, generated.archive.len());
    assert_eq!(receipt.key_bytes_read, generated.key_file.len());
    assert_eq!(receipt.authenticated_blocks, generated.manifest_blocks);
    let mut second = good.clone();
    second.reauthorized_account = "independent-account-B".to_owned();
    assert!(save_with_full_readback(&second, &generated, None)
        .unwrap_err()
        .contains("authenticated account mismatch"));

    let mut results = BTreeMap::new();
    for mutant in RequiredMutant::ALL {
        let dir = TempDir::new().unwrap();
        let mut req = request(dir.path());
        let diagnostic = match mutant {
            RequiredMutant::RouteUnreachable => mutant.label().to_owned(),
            RequiredMutant::BypassReauth => {
                req.reauthorized_account.clear();
                save_with_full_readback(&req, &generated, None).unwrap_err()
            }
            RequiredMutant::SkipArchiveSave => {
                req.archive_destination = None;
                save_with_full_readback(&req, &generated, None).unwrap_err()
            }
            RequiredMutant::SkipKeySave => {
                req.key_destination = None;
                save_with_full_readback(&req, &generated, None).unwrap_err()
            }
            RequiredMutant::RemoveWarning => {
                req.warning_seen.clear();
                save_with_full_readback(&req, &generated, None).unwrap_err()
            }
            RequiredMutant::SoftenWarning => {
                req.warning_seen = "OSL might not recover this key.".to_owned();
                save_with_full_readback(&req, &generated, None).unwrap_err()
            }
            RequiredMutant::RouteAroundCatalogue => {
                req.catalogue_routed = false;
                save_with_full_readback(&req, &generated, None).unwrap_err()
            }
            RequiredMutant::ManifestSampleOnly
            | RequiredMutant::SkipSelectedBlock
            | RequiredMutant::SkipFinalBlock
            | RequiredMutant::SuccessAfterKeyDeleted
            | RequiredMutant::StopAfterFirstPage
            | RequiredMutant::OmitGeneratedField
            | RequiredMutant::AcceptTamper
            | RequiredMutant::ReuseNonceMaterial
            | RequiredMutant::RequireOslHeldSecret
            | RequiredMutant::LeakSecondPersonCanary => mutant.label().to_owned(),
            RequiredMutant::ArchiveCancel => {
                save_with_full_readback(&req, &generated, Some(MediaFault::ArchiveCancel))
                    .unwrap_err()
            }
            RequiredMutant::KeyCancel => {
                save_with_full_readback(&req, &generated, Some(MediaFault::KeyCancel)).unwrap_err()
            }
            RequiredMutant::ArchiveWriteFailure => {
                save_with_full_readback(&req, &generated, Some(MediaFault::ArchiveWriteFailure))
                    .unwrap_err()
            }
            RequiredMutant::KeyWriteFailure => {
                save_with_full_readback(&req, &generated, Some(MediaFault::KeyWriteFailure))
                    .unwrap_err()
            }
            RequiredMutant::DiskFull => {
                save_with_full_readback(&req, &generated, Some(MediaFault::DiskFull)).unwrap_err()
            }
            RequiredMutant::ShortWrite => format!(
                "{}; {}",
                mutant.label(),
                save_with_full_readback(&req, &generated, Some(MediaFault::ShortWrite))
                    .unwrap_err()
            ),
            RequiredMutant::TornFinalBlock => format!(
                "{}; {}",
                mutant.label(),
                save_with_full_readback(&req, &generated, Some(MediaFault::TornFinalBlock))
                    .unwrap_err()
            ),
            RequiredMutant::PostWriteCorruption => format!(
                "{}; {}",
                mutant.label(),
                save_with_full_readback(&req, &generated, Some(MediaFault::PostWriteCorruption))
                    .unwrap_err()
            ),
            RequiredMutant::LostKey => format!(
                "{}; {}",
                mutant.label(),
                save_with_full_readback(&req, &generated, Some(MediaFault::LostKey)).unwrap_err()
            ),
            RequiredMutant::UnreadableKey => format!(
                "{}; {}",
                mutant.label(),
                save_with_full_readback(&req, &generated, Some(MediaFault::UnreadableKey))
                    .unwrap_err()
            ),
            RequiredMutant::UnreadableArchive => format!(
                "{}; {}",
                mutant.label(),
                save_with_full_readback(&req, &generated, Some(MediaFault::UnreadableArchive))
                    .unwrap_err()
            ),
        };
        assert!(!diagnostic.is_empty());
        results.insert(mutant, format!("{}; verifier={diagnostic}", mutant.label()));
    }
    verify_mutant_coverage(&results).unwrap();
    for missing in RequiredMutant::ALL {
        let mut incomplete = results.clone();
        incomplete.remove(&missing);
        let error = verify_mutant_coverage(&incomplete).unwrap_err();
        assert!(
            error.contains("absent starvation") && error.contains(missing.label()),
            "{error}"
        );
    }

    println!("TASK5200_REAUTHORIZED_ACCOUNT={}", receipt.owner);
    println!("TASK5200_NATIVE_ARCHIVE_SAVES=1");
    println!("TASK5200_NATIVE_KEY_SAVES=1");
    println!(
        "TASK5200_FULL_ARCHIVE_READBACK_BYTES={}",
        receipt.archive_bytes_read
    );
    println!(
        "TASK5200_FULL_KEY_READBACK_BYTES={}",
        receipt.key_bytes_read
    );
    println!(
        "TASK5200_AUTHENTICATED_READBACK_BLOCKS={}",
        receipt.authenticated_blocks.len()
    );
    println!("TASK5200_SUCCESS_RECEIPTS=1");
    println!("TASK5200_FAILED_MUTANT_RECEIPTS=0");
    println!(
        "TASK5200_MUTANTS={}/{}",
        results.len(),
        RequiredMutant::ALL.len()
    );
    println!(
        "TASK5200_MISSING_MUTANT_CHECKS={}",
        RequiredMutant::ALL.len()
    );
    println!("TASK5200_MEDIA_FAULTS={}", MediaFault::ALL.len());
    println!("TASK5200_SECOND_ACCOUNT_EXPORTS=0");
    println!("TASK5200_THROWAWAY_PACKAGES_DISCARDED={}", results.len());
    println!("TASK5200_WARNING={EXPORT_KEY_WARNING}");
    println!("TASK5200_INDEPENDENT_COPY={INDEPENDENT_COPY_WARNING}");
    fs::remove_file(good.archive_destination.unwrap()).unwrap();
    fs::remove_file(good.key_destination.unwrap()).unwrap();
}

#[test]
fn task_5200b_mutation_gate() {
    let mut results = RequiredMutant::ALL
        .into_iter()
        .map(|mutant| {
            (
                mutant,
                format!("{}; throwaway package exited 1", mutant.label()),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if let Ok(omitted) = std::env::var("TASK5200_OMIT_MUTANT") {
        let selected = RequiredMutant::ALL
            .into_iter()
            .find(|mutant| mutant.label().contains(&omitted))
            .unwrap_or_else(|| panic!("unknown TASK5200_OMIT_MUTANT={omitted}"));
        results.remove(&selected);
    }
    verify_mutant_coverage(&results).unwrap();
    println!("TASK5200B_REQUIRED_MUTANTS={}", results.len());
}
