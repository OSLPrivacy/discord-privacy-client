use mail_prefilter_boundary::*;
use std::fs;
use std::process::Command;

#[derive(Default)]
struct Lookup;

impl AllowedSenderLookup for Lookup {
    type Error = &'static str;

    fn exact_allowed_senders(
        &mut self,
        _provider_id: &str,
        _mailbox_place: &str,
    ) -> Result<Vec<String>, Self::Error> {
        Ok(vec!["friend@example.test".into()])
    }
}

#[derive(Default)]
struct RecordingMailbox {
    opened_folders: Vec<String>,
}

impl ProviderMailbox for RecordingMailbox {
    type Error = &'static str;

    fn candidate_ids(
        &mut self,
        request: &ProviderPrefilterRequest,
    ) -> Result<Vec<u8>, Self::Error> {
        self.opened_folders.push(match request.folder.as_str() {
            "INBOX" => "Inbox".into(),
            other => other.to_owned(),
        });
        Ok(br#"{"message_ids":[]}"#.to_vec())
    }

    fn header(&mut self, _message_id: &str) -> Result<Vec<u8>, Self::Error> {
        panic!("empty candidate inventory must not fetch a header")
    }

    fn body(&mut self, _message_id: &str) -> Result<Vec<u8>, Self::Error> {
        panic!("empty candidate inventory must not fetch a body")
    }
}

#[derive(Default)]
struct Observer;

impl ProcessEntryObserver for Observer {
    type Error = &'static str;

    fn observe_before_parsing(
        &mut self,
        _provider_id: &str,
        _boundary: &str,
        _kind: RawResponseKind,
        _raw_response: &[u8],
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn provider_folders() -> Vec<ProviderFolderEvidence> {
    vec![
        ProviderFolderEvidence {
            name: "Inbox".into(),
            role: FolderRole::Inbox,
        },
        ProviderFolderEvidence {
            name: "Sent".into(),
            role: FolderRole::Sent,
        },
        ProviderFolderEvidence {
            name: "Drafts".into(),
            role: FolderRole::Drafts,
        },
        ProviderFolderEvidence {
            name: "Archive".into(),
            role: FolderRole::Archive,
        },
        ProviderFolderEvidence {
            name: "Family Private".into(),
            role: FolderRole::UserPrivate,
        },
        ProviderFolderEvidence {
            name: "Receipts".into(),
            role: FolderRole::Other,
        },
    ]
}

fn structural_real_shape() -> FolderAuditEvidence {
    let forbidden = ["Sent", "Drafts", "Archive", "Family Private", "Receipts"];
    FolderAuditEvidence {
        schema: FOLDER_EVIDENCE_SCHEMA.into(),
        origin: EvidenceOrigin::RealProviderExternal,
        provider_id: "gmail".into(),
        real_provider_account: true,
        run_nonce: "structural-test-only-not-live-acceptance".into(),
        shipping_allowlist: SHIPPING_FOLDER_ALLOWLIST
            .iter()
            .map(|folder| (*folder).to_owned())
            .collect(),
        provider_folders: provider_folders(),
        provider_request_log: FolderProviderRequestLog {
            observer: "provider-owned external request audit log".into(),
            externally_observed: true,
            opened_folders: vec!["Inbox".into()],
        },
        shipping_refusals: forbidden
            .into_iter()
            .map(|folder| ShippingFolderRefusal {
                folder: folder.into(),
                error: format!(
                    "provider_id=gmail boundary={} failure=requested folder {:?} refused; shipping allowlist is [Inbox]",
                    FOLDER_ACCESS_BOUNDARY, folder
                ),
                provider_access_count: 0,
            })
            .collect(),
    }
}

#[test]
fn shipping_reader_opens_only_inbox_and_refuses_every_other_folder_by_name() {
    assert_eq!(SHIPPING_FOLDER_ALLOWLIST, ["Inbox"]);
    assert!(SHIPPING_FOLDER_ALLOWLIST.len() <= 2);

    let mut mailbox = RecordingMailbox::default();
    read_allowed_conversations_in_folder(
        "gmail",
        "Inbox",
        lookup_allowed_sender_grant("gmail", &mut Lookup).unwrap(),
        &mut mailbox,
        &mut Observer,
    )
    .unwrap();
    assert_eq!(mailbox.opened_folders, ["Inbox"]);

    for forbidden in ["Sent", "Drafts", "Archive", "Family Private", "Receipts"] {
        let access_count_before = mailbox.opened_folders.len();
        let result = read_allowed_conversations_in_folder(
            "gmail",
            forbidden,
            lookup_allowed_sender_grant("gmail", &mut Lookup).unwrap(),
            &mut mailbox,
            &mut Observer,
        );
        assert!(
            result.is_err(),
            "forbidden provider folder touched: {forbidden}"
        );
        let error = result.unwrap_err().to_string();
        assert!(error.contains(&format!("requested folder \"{forbidden}\" refused")));
        assert!(error.contains(FOLDER_ACCESS_BOUNDARY));
        assert_eq!(
            mailbox.opened_folders.len(),
            access_count_before,
            "provider was accessed for forbidden folder {forbidden}"
        );
        println!("TASK4351_REFUSAL folder={forbidden} provider_access_count=0 error={error}");
    }
    println!(
        "TASK4351_READER allowlist_count={} allowlist={} opened={:?} other_folders_opened=0",
        SHIPPING_FOLDER_ALLOWLIST.len(),
        SHIPPING_FOLDER_ALLOWLIST.join(","),
        mailbox.opened_folders
    );
}

#[test]
fn exact_external_shape_reports_six_folders_and_zero_other_opens() {
    let summary = audit_folder_evidence(&structural_real_shape()).unwrap();
    assert_eq!(summary.allowlist_count, 1);
    assert_eq!(summary.catalog_folders, 6);
    assert_eq!(summary.opened_allowed_requests, 1);
    assert_eq!(summary.other_folders_opened, 0);
    assert_eq!(summary.forbidden_refusals, 5);
    println!(
        "TASK4351_STRUCTURAL provider={} allowlist_count={} catalog_folders={} opened_allowed={} other_folders_opened={} forbidden_refusals={} dev_fixture=true",
        summary.provider_id,
        summary.allowlist_count,
        summary.catalog_folders,
        summary.opened_allowed_requests,
        summary.other_folders_opened,
        summary.forbidden_refusals
    );
}

#[test]
fn fixture_origin_is_dev_only_and_cannot_satisfy_acceptance() {
    let mut fixture = structural_real_shape();
    fixture.origin = EvidenceOrigin::Fixture;
    let error = audit_folder_evidence(&fixture).unwrap_err().to_string();
    assert!(error.contains("fixtures, proxies, and simulated folder evidence are dev-only"));
    assert!(error.contains(FOLDER_ACCESS_BOUNDARY));
}

#[test]
fn first_forbidden_provider_folder_in_request_order_is_named() {
    let mut bypassed = structural_real_shape();
    bypassed.provider_request_log.opened_folders =
        vec!["Inbox".into(), "Drafts".into(), "Archive".into()];
    let error = audit_folder_evidence(&bypassed).unwrap_err().to_string();
    assert!(error.contains("provider_id=gmail"), "{error}");
    assert!(
        error.contains("forbidden provider folder touched: Drafts"),
        "{error}"
    );
    println!("TASK4351_GUARD_BYPASS exit=1 {error}");
}

#[test]
fn acceptance_binary_prints_counts_and_exits_one_on_first_forbidden_open() {
    let directory = std::env::temp_dir().join(format!("task4351-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();

    let green_path = directory.join("structural-green.json");
    fs::write(
        &green_path,
        serde_json::to_vec_pretty(&structural_real_shape()).unwrap(),
    )
    .unwrap();
    let green = Command::new(env!("CARGO_BIN_EXE_audit-4351"))
        .arg(&green_path)
        .output()
        .unwrap();
    assert!(
        green.status.success(),
        "{}",
        String::from_utf8_lossy(&green.stderr)
    );
    let stdout = String::from_utf8_lossy(&green.stdout);
    assert!(stdout.contains("allowlist_count=1"), "{stdout}");
    assert!(stdout.contains("catalog_folders=6"), "{stdout}");
    assert!(stdout.contains("other_folders_opened=0"), "{stdout}");

    let mut bypassed = structural_real_shape();
    bypassed.provider_request_log.opened_folders =
        vec!["Inbox".into(), "Drafts".into(), "Archive".into()];
    let red_path = directory.join("guard-removed.json");
    fs::write(&red_path, serde_json::to_vec_pretty(&bypassed).unwrap()).unwrap();
    let red = Command::new(env!("CARGO_BIN_EXE_audit-4351"))
        .arg(&red_path)
        .output()
        .unwrap();
    assert_eq!(red.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&red.stderr);
    assert!(
        stderr.contains("forbidden provider folder touched: Drafts"),
        "{stderr}"
    );
    println!("TASK4351_BINARY_GREEN {}", stdout.trim());
    println!("TASK4351_BINARY_RED exit=1 {}", stderr.trim());

    fs::remove_dir_all(directory).unwrap();
}
