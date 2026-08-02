use std::time::Duration;

use osl_privacy_hub::realtime_client::{
    BlobId, CarrierPointer, FrameError, RealtimeClient, FRAME_BYTES, TICK_INTERVAL,
};

fn id(byte: u8) -> BlobId {
    BlobId::from_bytes([byte; 16])
}

fn response(tag: u8, blob: u8) -> String {
    let body = format!(
        r#"{{"delivery_tag":"{}","blob_id":"{}"}}"#,
        format!("{tag:02x}").repeat(16),
        format!("{blob:02x}").repeat(16),
    );
    format!("{body:<FRAME_BYTES$}")
}

#[test]
fn t1_t52_outbound_frames_are_constant_for_50_idle_and_busy_turns() {
    let mut idle = RealtimeClient::new(Duration::ZERO);
    let mut busy = RealtimeClient::new(Duration::ZERO);
    busy.remember_carrier_pointer(id(0xbb), CarrierPointer::from_carrier("carrier-only-P"));

    for turn in 0..50_u8 {
        let (idle_at, idle_frame) = idle.next_outbound_frame();
        let (busy_at, busy_frame) = busy.next_outbound_frame();
        assert_eq!(idle_at, Duration::from_secs(4 * u64::from(turn)));
        assert_eq!(busy_at, idle_at, "traffic must not change the tick cadence");
        assert_eq!(idle_frame.len(), FRAME_BYTES);
        assert_eq!(busy_frame.len(), FRAME_BYTES);
        assert_eq!(
            idle_frame, busy_frame,
            "traffic must not change tick contents or size"
        );
        assert!(idle_frame.bytes().all(|byte| byte == b' '));
        assert_eq!(TICK_INTERVAL, Duration::from_secs(4));

        busy.receive_frame(&response(turn, 0xbb))
            .expect("valid wakeup");
        assert!(busy.take_scheduled_fetch().is_some());
    }
}

#[test]
fn t1_t52_unknown_wakeup_causes_no_fetch() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client
        .receive_frame(&response(0xaa, 0xbb))
        .expect("valid unauthorised wakeup");
    assert!(
        client.take_scheduled_fetch().is_none(),
        "a wakeup is never fetch authority"
    );
}

#[test]
fn t1_t52_matching_wakeup_uses_the_preexisting_carrier_pointer() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client.remember_carrier_pointer(id(0xbb), CarrierPointer::from_carrier("carrier-only-P"));
    client
        .receive_frame(&response(0xaa, 0xbb))
        .expect("valid wakeup");
    let fetch = client
        .take_scheduled_fetch()
        .expect("local pointer authorizes scheduling");
    fetch
        .fetch_with(|blob_id, bearer_capability| {
            assert_eq!(blob_id, id(0xbb));
            assert_eq!(bearer_capability, "carrier-only-P");
            Ok::<(), ()>(())
        })
        .expect("fetch closure succeeds");
}

#[test]
fn t1_t52_frames_cannot_supply_a_capability_or_secret() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    let body = r#"{"delivery_tag":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","blob_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","P":"stolen"}"#;
    let frame = format!("{body:<FRAME_BYTES$}");
    assert_eq!(
        client.receive_frame(&frame),
        Err(FrameError::UnexpectedFields)
    );
    assert!(client.take_scheduled_fetch().is_none());
}
