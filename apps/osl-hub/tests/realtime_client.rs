use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use osl_privacy_hub::realtime_client::{
    BlobId, CarrierPointer, FrameError, RealtimeClient, RealtimeRoute, ScheduledFetch, FRAME_BYTES,
    TICK_INTERVAL,
};
use osl_privacy_hub::realtime_pipe::{
    run_realtime_pipe_ticks, RealtimeEndpoint, RealtimePipeClock, RealtimePipeEntropy,
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
fn t1_t52_empty_idle_response_spends_no_pretend_fetch() {
    let mut client = RealtimeClient::new(Duration::ZERO);
    client
        .receive_frame(&response(0x00, 0x00))
        .expect("valid idle response");
    assert!(
        client.take_fetch_work().is_none(),
        "the all-zero idle response must not spend a pretend fetch"
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

#[test]
fn task_4405_realtime_pipe_opens_ticks_reconnects_and_idle_hour_spends_no_pretend_fetches() {
    const IDLE_HOUR_TICKS: usize = 60 * 60 / 4;

    let fixture = RealtimeServiceFixture::start(IDLE_HOUR_TICKS);
    let endpoint =
        RealtimeEndpoint::parse(&format!("ws://{}/v1/realtime", fixture.address())).unwrap();
    let mut client = RealtimeClient::new(Duration::ZERO);
    let mut clock = RecordingClock::default();
    let mut entropy = FixedEntropy::new([u64::MAX, u64::MAX]);

    let report = run_realtime_pipe_ticks(
        &endpoint,
        &mut client,
        IDLE_HOUR_TICKS,
        &mut clock,
        &mut entropy,
    )
    .expect("pipe drives realtime ticks through the fixture");

    let stats = fixture.join();
    let unique_intervals: std::collections::BTreeSet<_> = report
        .frame_offsets
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    let unique_frame_sizes: std::collections::BTreeSet<_> =
        report.frame_sizes.iter().copied().collect();
    let reconnect_wait_ms: Vec<_> = report
        .reconnect_waits
        .iter()
        .map(|delay| delay.as_millis())
        .collect();

    assert_eq!(report.opened_connections, 3);
    assert_eq!(stats.handshakes, 3);
    assert_eq!(stats.frames_seen, IDLE_HOUR_TICKS);
    for (index, size) in report.frame_sizes.iter().copied().enumerate() {
        let frame_number = index + 1;
        assert_eq!(
            size, FRAME_BYTES,
            "TASK4405_FRAME_{frame_number}_SIZE_GAVE_SOMETHING_AWAY expected {FRAME_BYTES} got {size}"
        );
    }
    assert!(stats.all_frames_were_spaces);
    assert_eq!(unique_intervals, [TICK_INTERVAL].into_iter().collect());
    assert_eq!(unique_frame_sizes, [FRAME_BYTES].into_iter().collect());
    assert_eq!(report.reconnect_waits.len(), 2);
    assert!(report.reconnect_waits[1] > report.reconnect_waits[0]);
    assert_eq!(report.pretend_fetches, 0);
    assert_eq!(report.authorized_fetches, 0);

    println!("TASK4405_CONNECTIONS_OPENED={}", report.opened_connections);
    println!("TASK4405_SERVICE_HANDSHAKES={}", stats.handshakes);
    println!("TASK4405_FRAMES_SENT={}", report.frame_sizes.len());
    println!(
        "TASK4405_FRAME_INTERVAL_SECONDS={}",
        TICK_INTERVAL.as_secs()
    );
    println!("TASK4405_UNIQUE_FRAME_SIZE_BYTES={unique_frame_sizes:?}");
    println!("TASK4405_RECONNECT_WAITS_MS={reconnect_wait_ms:?}");
    println!(
        "TASK4405_IDLE_HOUR_PRETEND_FETCHES={}",
        report.pretend_fetches
    );
}

#[derive(Default)]
struct RecordingClock {
    waited_until: Vec<Duration>,
    reconnect_waits: Vec<Duration>,
}

impl RealtimePipeClock for RecordingClock {
    fn wait_until(&mut self, scheduled_at: Duration) {
        self.waited_until.push(scheduled_at);
    }

    fn wait_for_reconnect(&mut self, delay: Duration) {
        self.reconnect_waits.push(delay);
    }
}

struct FixedEntropy {
    values: Vec<u64>,
    index: usize,
}

impl FixedEntropy {
    fn new(values: impl IntoIterator<Item = u64>) -> Self {
        Self {
            values: values.into_iter().collect(),
            index: 0,
        }
    }
}

impl RealtimePipeEntropy for FixedEntropy {
    fn next_u64(&mut self) -> u64 {
        let value = self
            .values
            .get(self.index)
            .copied()
            .unwrap_or_else(|| *self.values.last().unwrap_or(&0));
        self.index += 1;
        value
    }
}

#[derive(Debug, Default)]
struct FixtureStats {
    handshakes: usize,
    frames_seen: usize,
    all_frames_were_spaces: bool,
}

struct RealtimeServiceFixture {
    address: SocketAddr,
    stats: Arc<Mutex<FixtureStats>>,
    handle: thread::JoinHandle<()>,
}

impl RealtimeServiceFixture {
    fn start(expected_frames: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind realtime fixture");
        let address = listener.local_addr().expect("fixture address");
        let stats = Arc::new(Mutex::new(FixtureStats {
            all_frames_were_spaces: true,
            ..FixtureStats::default()
        }));
        let thread_stats = Arc::clone(&stats);
        let handle = thread::spawn(move || {
            let mut total_frames = 0;
            for connection_index in 0..3 {
                let (mut stream, _) = listener.accept().expect("accept realtime websocket");
                accept_websocket(&mut stream, &thread_stats);
                match connection_index {
                    0 => {
                        read_idle_tick(&mut stream, &thread_stats, &mut total_frames)
                            .expect("first idle tick");
                        write_server_text(&mut stream, &response(0, 0)).expect("first idle reply");
                        read_idle_tick(&mut stream, &thread_stats, &mut total_frames)
                            .expect("second idle tick before cut");
                    }
                    1 => {
                        read_idle_tick(&mut stream, &thread_stats, &mut total_frames)
                            .expect("third idle tick before cut");
                    }
                    _ => {
                        while total_frames < expected_frames {
                            read_idle_tick(&mut stream, &thread_stats, &mut total_frames)
                                .expect("idle tick after reconnect");
                            write_server_text(&mut stream, &response(0, 0))
                                .expect("idle reply after reconnect");
                        }
                    }
                }
            }
        });
        Self {
            address,
            stats,
            handle,
        }
    }

    fn address(&self) -> SocketAddr {
        self.address
    }

    fn join(self) -> FixtureStats {
        self.handle.join().expect("fixture thread completes");
        Arc::try_unwrap(self.stats)
            .expect("fixture stats owner")
            .into_inner()
            .expect("fixture stats mutex")
    }
}

fn accept_websocket(stream: &mut TcpStream, stats: &Arc<Mutex<FixtureStats>>) {
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("read upgrade request");
        request.push(byte[0]);
    }
    let request = String::from_utf8(request).expect("ascii websocket request");
    assert!(request.starts_with("GET /v1/realtime HTTP/1.1\r\n"));
    assert!(request
        .lines()
        .any(|line| line.eq_ignore_ascii_case("Upgrade: websocket")));
    stats.lock().unwrap().handshakes += 1;
    stream
        .write_all(
            b"HTTP/1.1 101 Switching Protocols\r\n\
              Upgrade: websocket\r\n\
              Connection: Upgrade\r\n\
              Sec-WebSocket-Accept: task-4405-fixture\r\n\
              \r\n",
        )
        .expect("write upgrade response");
}

fn read_idle_tick(
    stream: &mut TcpStream,
    stats: &Arc<Mutex<FixtureStats>>,
    total_frames: &mut usize,
) -> Option<()> {
    let frame = read_client_text(stream)?;
    let mut stats = stats.lock().unwrap();
    stats.frames_seen += 1;
    stats.all_frames_were_spaces &= frame.len() == FRAME_BYTES;
    stats.all_frames_were_spaces &= frame.bytes().all(|byte| byte == b' ');
    *total_frames += 1;
    Some(())
}

fn read_client_text(stream: &mut TcpStream) -> Option<String> {
    let mut header = [0_u8; 2];
    stream.read_exact(&mut header).ok()?;
    assert_eq!(header[0] & 0x0f, 0x1);
    assert_ne!(header[1] & 0x80, 0, "client frames must be masked");
    let mut len = usize::from(header[1] & 0x7f);
    if len == 126 {
        let mut extended = [0_u8; 2];
        stream.read_exact(&mut extended).ok()?;
        len = usize::from(u16::from_be_bytes(extended));
    }
    let mut mask = [0_u8; 4];
    stream.read_exact(&mut mask).ok()?;
    let mut payload = vec![0_u8; len];
    stream.read_exact(&mut payload).ok()?;
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % mask.len()];
    }
    String::from_utf8(payload).ok()
}

fn write_server_text(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    assert_eq!(text.len(), FRAME_BYTES);
    let len = u16::try_from(text.len()).expect("fixture frame length fits u16");
    stream.write_all(&[0x81, 126])?;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(text.as_bytes())
}
