use std::time::Duration;

use osl_privacy_hub::realtime_client::{
    BlobId, CarrierPointer, FrameError, RealtimeClient, RealtimeRoute, ScheduledFetch, FRAME_BYTES,
    TICK_INTERVAL,
};
use osl_privacy_hub::realtime_resume::{AcknowledgementCursor, ReconnectSchedule, SessionId};
use osl_privacy_hub::realtime_subscription::DeliveryTag;

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

fn fetch_counts(client: &mut RealtimeClient) -> (usize, usize) {
    let mut real = 0;
    let mut pretend = 0;
    while let Some(fetch) = client.take_fetch_work() {
        match fetch {
            ScheduledFetch::Authorized(_) => real += 1,
            ScheduledFetch::Decoy(_) => pretend += 1,
        }
    }
    (real, pretend)
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
        assert!(busy.take_fetch_work().is_some());
    }
}

#[test]
fn task_4415_empty_hour_spends_no_fetches_real_beat_keeps_one_decoy_and_mixed_hour_stays_under_limit(
) {
    assert_eq!(TICK_INTERVAL, Duration::from_secs(4));
    let beats_per_hour =
        usize::try_from(Duration::from_secs(60 * 60).as_secs() / TICK_INTERVAL.as_secs()).unwrap();
    let previous_idle_fetches = beats_per_hour;
    assert_eq!(beats_per_hour, 900);
    assert_eq!(previous_idle_fetches, 900);

    let mut idle = RealtimeClient::new(Duration::ZERO);
    let mut idle_fetches = 0;
    for _ in 0..beats_per_hour {
        idle.receive_frame(&response(0x00, 0x00))
            .expect("empty beat is a valid fixed-size frame");
        let (real, pretend) = fetch_counts(&mut idle);
        idle_fetches += real + pretend;
    }
    println!("TASK4415_BEAT_SECONDS={}", TICK_INTERVAL.as_secs());
    println!("TASK4415_IDLE_BEATS_PER_HOUR={beats_per_hour}");
    println!("TASK4415_IDLE_FETCHES_BEFORE={previous_idle_fetches}");
    println!("TASK4415_IDLE_FETCHES_AFTER={idle_fetches}");
    assert_eq!(
        idle_fetches, 0,
        "4415 expected 0 fetches in an idle hour; saw {idle_fetches} where {previous_idle_fetches} were produced before"
    );

    let mut real_beat = RealtimeClient::new(Duration::ZERO);
    real_beat.remember_carrier_pointer(id(0xbb), CarrierPointer::from_carrier("carrier-only-P"));
    real_beat
        .receive_frame(&response(0xaa, 0xbb))
        .expect("real beat is a valid wakeup");
    let (real_fetches, pretend_fetches) = fetch_counts(&mut real_beat);
    println!("TASK4415_REAL_BEAT_REAL_FETCHES={real_fetches}");
    println!("TASK4415_REAL_BEAT_PRETEND_FETCHES={pretend_fetches}");
    assert_eq!(real_fetches, 1);
    assert_eq!(pretend_fetches, 1);

    let real_beats_in_mixed_hour = 59_usize;
    let mut mixed = RealtimeClient::new(Duration::ZERO);
    let mut mixed_fetches = 0;
    for beat in 0..beats_per_hour {
        if beat < real_beats_in_mixed_hour {
            let byte = u8::try_from(beat + 1).unwrap();
            mixed
                .remember_carrier_pointer(id(byte), CarrierPointer::from_carrier("carrier-only-P"));
            mixed
                .receive_frame(&response(byte, byte))
                .expect("mixed real beat is valid");
        } else {
            mixed
                .receive_frame(&response(0x00, 0x00))
                .expect("mixed idle beat is valid");
        }
        let (real, pretend) = fetch_counts(&mut mixed);
        mixed_fetches += real + pretend;
    }
    println!("TASK4415_MIXED_REAL_BEATS={real_beats_in_mixed_hour}");
    println!("TASK4415_MIXED_HOUR_FETCHES={mixed_fetches}");
    println!("TASK4415_FETCH_LIMIT_PER_HOUR=120");
    assert_eq!(mixed_fetches, 118);
    assert!(mixed_fetches < 120);
}

#[test]
fn t1_t75_direct_and_tor_streams_each_hold_their_fixed_cadence() {
    for route in [RealtimeRoute::Direct, RealtimeRoute::Tor] {
        let mut client = RealtimeClient::for_route(Duration::ZERO, route);
        let mut previous = None;

        for _ in 0..50 {
            let (at, frame) = client.next_outbound_frame();
            if let Some(previous) = previous {
                assert_eq!(
                    at - previous,
                    route.tick_interval(),
                    "a route's tick must not adapt between frames"
                );
            }
            assert_eq!(frame.len(), FRAME_BYTES);
            previous = Some(at);
        }
    }
}

#[test]
fn t1_t55_shipping_tick_rotates_the_opaque_subscription_window() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client
        .replace_subscription_tags((1..=33).map(|byte| {
            DeliveryTag::try_from_bytes([byte; 16]).expect("non-padding delivery tag")
        }));

    let first = client.next_outbound_tick();
    let second = client.next_outbound_tick();

    assert_eq!(first.frame.len(), FRAME_BYTES);
    assert_eq!(first.delivery_tags.len(), 32);
    assert_eq!(second.delivery_tags.len(), 32);
    assert_eq!(first.delivery_tags[0].as_bytes(), [1; 16]);
    assert_eq!(second.delivery_tags[0].as_bytes(), [33; 16]);
    assert!(second
        .delivery_tags
        .iter()
        .all(|tag| tag.as_bytes() != [0; 16]));
}

#[test]
fn t1_t52_unknown_wakeup_causes_no_fetch() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client
        .receive_frame(&response(0xaa, 0xbb))
        .expect("valid unauthorised wakeup");
    // An unknown wakeup must schedule a DECOY, not nothing. Scheduling nothing
    // would leak the match: an observer could tell a matched reply from an
    // unmatched one by whether a fetch followed. The property under test is
    // that a wakeup is never fetch AUTHORITY - never that no work is queued.
    match client.take_fetch_work() {
        Some(ScheduledFetch::Decoy(_)) => {}
        Some(ScheduledFetch::Authorized(_)) => {
            panic!("a wakeup is never fetch authority: unknown blob got an AUTHORIZED fetch")
        }
        None => panic!("an unknown wakeup must still schedule a decoy, or the match leaks"),
    }
}

#[test]
fn t1_t52_matching_wakeup_uses_the_preexisting_carrier_pointer() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client.remember_carrier_pointer(id(0xbb), CarrierPointer::from_carrier("carrier-only-P"));
    client
        .receive_frame(&response(0xaa, 0xbb))
        .expect("valid wakeup");
    let fetch = client
        .take_fetch_work()
        .expect("local pointer authorizes scheduling");
    // ScheduledFetch is an enum: only the Authorized variant carries a bearer
    // capability and can fetch. Matching here also asserts we did NOT get a
    // Decoy - a decoy performing a real fetch would defeat the cover traffic.
    let ScheduledFetch::Authorized(authorized) = fetch else {
        panic!("a locally held pointer must schedule an AUTHORIZED fetch, not a decoy");
    };
    authorized
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
    assert!(client.take_fetch_work().is_none());
}

#[test]
fn t1_t53_reconnect_restores_tags_but_not_the_old_session_cursor() {
    let tags = [
        DeliveryTag::try_from_bytes([1; 16]).unwrap(),
        DeliveryTag::try_from_bytes([2; 16]).unwrap(),
    ];
    let mut before_close = RealtimeClient::new(Duration::ZERO);
    before_close.replace_subscription_tags(tags);
    let mut reconnect = ReconnectSchedule::new();
    reconnect.set_contract(before_close.reconnect_contract(
        SessionId::from_bytes([9; 16]),
        AcknowledgementCursor {
            last_sent: 12,
            last_accepted_peer_frame: 11,
        },
    ));
    let restored = reconnect
        .restore_for_new_session(SessionId::from_bytes([10; 16]))
        .unwrap();

    let mut after_reconnect = RealtimeClient::new(Duration::ZERO);
    after_reconnect.restore_subscriptions(&restored);
    let tick = after_reconnect.next_outbound_tick();
    assert_eq!(tick.delivery_tags, tags);
    assert_eq!(restored.cursor, AcknowledgementCursor::default());
    assert_eq!(restored.session_id.as_bytes(), [10; 16]);
}
