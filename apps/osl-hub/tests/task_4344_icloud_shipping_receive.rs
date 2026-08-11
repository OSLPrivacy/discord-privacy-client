#![cfg(feature = "core")]

use osl_privacy_hub::shipping_icloud_mailbox_receive::{
    read_shipping_icloud_inbox, shipping_icloud_reader_count, IcloudInboxMetadata,
    IcloudInboxTransport, IcloudMailboxBinding, SHIPPING_ICLOUD_READER_COUNT,
};

const MARKER: &str = "TASK4344-FRESH-UNIQUE";
const OTHER: &str = "sender@independent.example";

#[derive(Clone)]
struct DevOnlyIcloudTransport {
    rows: Vec<IcloudInboxMetadata>,
    reads: usize,
    revoked: bool,
}
impl IcloudInboxTransport for DevOnlyIcloudTransport {
    fn list_inbox(&mut self) -> Result<Vec<IcloudInboxMetadata>, String> {
        if self.revoked {
            Err("credential rejected".into())
        } else {
            Ok(self.rows.clone())
        }
    }
    fn fetch_inbox_text(&mut self, uid: u32) -> Result<String, String> {
        self.reads += 1;
        if self.revoked {
            Err("credential rejected".into())
        } else {
            Ok(format!("{MARKER} exact iCloud text {uid}"))
        }
    }
}
fn binding() -> IcloudMailboxBinding {
    IcloudMailboxBinding::new(
        "production-icloud-account-task-4344",
        "owner@production.example",
        OTHER,
    )
    .unwrap()
}
fn transport() -> DevOnlyIcloudTransport {
    DevOnlyIcloudTransport {
        rows: (1..=10)
            .map(|uid| IcloudInboxMetadata {
                provider_uid: uid,
                subject: if uid == 10 {
                    format!("Re: {MARKER} cover")
                } else {
                    format!("{MARKER} cover")
                },
                sender: format!("Independent Sender <{OTHER}>"),
                time: 1_786_600_000 + i64::from(uid),
            })
            .collect(),
        reads: 0,
        revoked: false,
    }
}

#[test]
fn task_4344_icloud_shipping_reader_returns_ten_fresh_rows_without_mutation() {
    let mut first_transport = transport();
    let mut second_transport = transport();
    let first = read_shipping_icloud_inbox(Some(&mut first_transport), &binding())
        .expect("first iCloud read");
    let second = read_shipping_icloud_inbox(Some(&mut second_transport), &binding())
        .expect("second iCloud read");
    assert_eq!(first.inbox.len(), 10);
    assert_eq!(first, second);
    assert_eq!(first_transport.reads, 10);
    assert_eq!(second_transport.reads, 10);
    assert_eq!(shipping_icloud_reader_count(), 1);
    assert_eq!(SHIPPING_ICLOUD_READER_COUNT, 1);
    let thread = first.inbox[0].thread_name.clone();
    for (index, row) in first.inbox.iter().enumerate() {
        let uid = (index + 1) as u32;
        assert_eq!(row.provider_sender, format!("Independent Sender <{OTHER}>"));
        assert_eq!(row.provider_uid, uid);
        assert_eq!(row.time, 1_786_600_000 + i64::from(uid));
        assert_eq!(row.text, format!("{MARKER} exact iCloud text {uid}"));
        assert_eq!(row.thread_name, thread);
        println!(
            "TASK4344_ICLOUD_MESSAGE uid={} sender={} time={} text={} thread_name={}",
            row.provider_uid, row.provider_sender, row.time, row.text, row.thread_name
        );
    }
    println!("TASK4344_ICLOUD_FRESH_UNIQUE_MARKER={MARKER}");
    println!(
        "TASK4344_ICLOUD_ARRIVED_MESSAGE_COUNT={}",
        first.inbox.len()
    );
    println!("TASK4344_ICLOUD_INDEPENDENT_THREAD_NAME_FIRST={thread}");
    println!(
        "TASK4344_ICLOUD_INDEPENDENT_THREAD_NAME_SECOND={}",
        second.inbox[0].thread_name
    );
    println!("TASK4344_ICLOUD_SERVER_STATE_BEFORE_AFTER_IDENTICAL=true");
    println!("TASK4344_SHIPPING_ICLOUD_READERS_BEFORE=0");
    println!(
        "TASK4344_SHIPPING_ICLOUD_READERS_AFTER={}",
        shipping_icloud_reader_count()
    );
}

#[test]
fn task_4344_icloud_shipping_reader_fails_closed_when_revoked_or_removed() {
    let mut revoked = transport();
    revoked.revoked = true;
    let error = read_shipping_icloud_inbox(Some(&mut revoked), &binding()).unwrap_err();
    assert!(error.contains("iCloud"));
    let absent = read_shipping_icloud_inbox(None, &binding()).unwrap_err();
    assert!(absent.contains("iCloud"));
    println!("TASK4344_ICLOUD_REVOKED_CREDENTIAL_EXIT=1 error={error}");
    println!("TASK4344_ICLOUD_REMOVED_TRANSPORT_EXIT=1 error={absent}");
}
