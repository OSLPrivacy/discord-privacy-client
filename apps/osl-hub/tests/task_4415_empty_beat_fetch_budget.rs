use std::time::Duration;

use osl_privacy_hub::realtime_client::{
    BlobId, CarrierPointer, RealtimeClient, ScheduledFetch, FRAME_BYTES, TICK_INTERVAL,
};

const BEAT_SECONDS: u64 = 4;
const BEATS_PER_IDLE_HOUR: usize = 60 * 60 / BEAT_SECONDS as usize;
const STORE_FETCH_LIMIT_PER_HOUR: usize = 120;
const MIXED_REAL_BEATS: usize = 59;

#[derive(Default)]
struct FetchCounts {
    authorized: usize,
    pretend: usize,
}

impl FetchCounts {
    fn total(&self) -> usize {
        self.authorized + self.pretend
    }
}

fn id_from_u16(value: u16) -> [u8; 16] {
    let [high, low] = value.to_be_bytes();
    [
        high, low, high, low, high, low, high, low, high, low, high, low, high, low, high, low,
    ]
}

fn blob_from_u16(value: u16) -> BlobId {
    BlobId::from_bytes(id_from_u16(value))
}

fn response(tag: [u8; 16], blob: [u8; 16]) -> String {
    let body = format!(
        r#"{{"delivery_tag":"{}","blob_id":"{}"}}"#,
        hex(tag),
        hex(blob),
    );
    assert!(
        body.len() < FRAME_BYTES,
        "fixture response must fit inside one fixed-size frame"
    );
    format!("{body:<FRAME_BYTES$}")
}

fn empty_response() -> String {
    response([0; 16], [0; 16])
}

fn hex(bytes: [u8; 16]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn drain_fetches(client: &mut RealtimeClient) -> FetchCounts {
    let mut counts = FetchCounts::default();
    while let Some(fetch) = client.take_fetch_work() {
        match fetch {
            ScheduledFetch::Authorized(_) => counts.authorized += 1,
            ScheduledFetch::Decoy(_) => counts.pretend += 1,
        }
    }
    counts
}

#[test]
fn task_4415_empty_beat_does_not_spend_fetch_budget() {
    let old_idle_fetches_per_hour = BEATS_PER_IDLE_HOUR;
    let mut client = RealtimeClient::new(Duration::ZERO);
    let empty = empty_response();
    let mut idle_fetches = 0;

    for beat in 0..BEATS_PER_IDLE_HOUR {
        let (scheduled_at, frame) = client.next_outbound_frame();
        assert_eq!(
            scheduled_at,
            Duration::from_secs(BEAT_SECONDS * beat as u64)
        );
        assert_eq!(frame.len(), FRAME_BYTES);
        client
            .receive_frame(&empty)
            .expect("the store's empty response is a valid fixed-size frame");
        idle_fetches += drain_fetches(&mut client).total();
    }

    assert_eq!(TICK_INTERVAL, Duration::from_secs(BEAT_SECONDS));
    assert_eq!(old_idle_fetches_per_hour, 900);
    assert_eq!(
        idle_fetches, 0,
        "{idle_fetches} fetches in an idle hour where 0 were expected"
    );

    let mut real = RealtimeClient::new(Duration::ZERO);
    let real_blob = blob_from_u16(0x4415);
    real.remember_carrier_pointer(real_blob, CarrierPointer::from_carrier("carrier-cap-4415"));
    real.receive_frame(&response(id_from_u16(1), *real_blob.as_bytes()))
        .expect("real wakeup frame is valid");
    let real_counts = drain_fetches(&mut real);
    assert_eq!(real_counts.authorized, 1);
    assert_eq!(real_counts.pretend, 1);
    assert_eq!(real_counts.total(), 2);

    let mut mixed = RealtimeClient::new(Duration::ZERO);
    let mut mixed_counts = FetchCounts::default();
    for beat in 0..BEATS_PER_IDLE_HOUR {
        if beat < MIXED_REAL_BEATS {
            let value = u16::try_from(beat + 1).expect("mixed hour fixture fits in u16");
            let blob = blob_from_u16(0x5000 + value);
            mixed.remember_carrier_pointer(
                blob,
                CarrierPointer::from_carrier(format!("mixed-carrier-cap-{beat}")),
            );
            mixed
                .receive_frame(&response(id_from_u16(value), *blob.as_bytes()))
                .expect("mixed real wakeup frame is valid");
        } else {
            mixed
                .receive_frame(&empty)
                .expect("mixed idle response is valid");
        }
        let beat_counts = drain_fetches(&mut mixed);
        mixed_counts.authorized += beat_counts.authorized;
        mixed_counts.pretend += beat_counts.pretend;
    }

    assert_eq!(mixed_counts.authorized, MIXED_REAL_BEATS);
    assert_eq!(mixed_counts.pretend, MIXED_REAL_BEATS);
    assert!(
        mixed_counts.total() < STORE_FETCH_LIMIT_PER_HOUR,
        "mixed traffic spent {} fetches against a {} per-hour limit",
        mixed_counts.total(),
        STORE_FETCH_LIMIT_PER_HOUR
    );

    println!("TASK4415_IDLE_BEATS_PER_HOUR={BEATS_PER_IDLE_HOUR}");
    println!("TASK4415_IDLE_FETCHES_BEFORE={old_idle_fetches_per_hour}");
    println!("TASK4415_IDLE_FETCHES_AFTER={idle_fetches}");
    println!(
        "TASK4415_REAL_BEAT_AUTHORIZED_FETCHES={}",
        real_counts.authorized
    );
    println!("TASK4415_REAL_BEAT_PRETEND_FETCHES={}", real_counts.pretend);
    println!("TASK4415_TICK_INTERVAL_SECONDS={}", TICK_INTERVAL.as_secs());
    println!("TASK4415_MIXED_REAL_BEATS={MIXED_REAL_BEATS}");
    println!(
        "TASK4415_MIXED_AUTHORIZED_FETCHES={}",
        mixed_counts.authorized
    );
    println!("TASK4415_MIXED_PRETEND_FETCHES={}", mixed_counts.pretend);
    println!("TASK4415_MIXED_TOTAL_FETCHES={}", mixed_counts.total());
    println!("TASK4415_STORE_FETCH_LIMIT_PER_HOUR={STORE_FETCH_LIMIT_PER_HOUR}");
}
