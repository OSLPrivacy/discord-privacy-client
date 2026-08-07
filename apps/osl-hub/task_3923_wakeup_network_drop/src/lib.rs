#[path = "../../src/realtime_client.rs"]
pub mod realtime_client;
#[path = "../../src/realtime_decoy.rs"]
pub mod realtime_decoy;
#[path = "../../src/realtime_pipe.rs"]
pub mod realtime_pipe;
#[path = "../../src/realtime_resume.rs"]
pub mod realtime_resume;
#[path = "../../src/realtime_subscription.rs"]
pub mod realtime_subscription;

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use std::thread;
    use std::time::Duration;

    use crate::realtime_client::{
        BlobId, CarrierPointer, RealtimeClient, ScheduledFetch, FRAME_BYTES,
    };
    use crate::realtime_pipe::{open_realtime_connection, RealtimeEndpoint, RealtimePipeError};
    use crate::realtime_resume::ReconnectSchedule;

    const OUTAGE_SECONDS: u64 = 60;
    const WAITING_MESSAGE: &str = "task-3923 waiting message";

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
    fn task_3923_wakeup_connection_recovers_after_sixty_second_network_drop() {
        let elapsed_ms = Arc::new(AtomicU64::new(0));
        let fixture = NetworkDropFixture::start(Arc::clone(&elapsed_ms));
        let endpoint =
            RealtimeEndpoint::parse(&format!("ws://{}/v1/realtime", fixture.address())).unwrap();
        let mut connection_states = Vec::new();

        let mut client = RealtimeClient::new(Duration::ZERO);
        client.remember_carrier_pointer(id(0x23), CarrierPointer::from_carrier(WAITING_MESSAGE));

        let mut socket = open_realtime_connection(&endpoint).expect("initial connection opens");
        connection_states.push("connected");

        let first_tick = client.next_outbound_tick();
        socket
            .send_text(&first_tick.frame)
            .expect("first realtime tick sends while connected");
        client
            .receive_frame(
                &socket
                    .read_text()
                    .expect("idle wakeup arrives before network drop"),
            )
            .expect("idle wakeup frame is valid");

        let stranded_tick = client.next_outbound_tick();
        socket
            .send_text(&stranded_tick.frame)
            .expect("the waiting message tick is written as the network is cut");
        let drop_error = socket
            .read_text()
            .expect_err("network drop must make the live connection report not connected");
        assert!(is_reconnectable(&drop_error), "{drop_error:?}");
        connection_states.push("not_connected");

        let (mut socket, attempts_during_outage, returned_after_ms) =
            reconnect_after_outage(&endpoint, &elapsed_ms, OUTAGE_SECONDS);
        connection_states.push("connected");

        let after_reconnect_tick = client.next_outbound_tick();
        socket
            .send_text(&after_reconnect_tick.frame)
            .expect("tick sends after reconnect");
        client
            .receive_frame(
                &socket
                    .read_text()
                    .expect("waiting wakeup arrives after reconnect"),
            )
            .expect("waiting wakeup frame is valid");

        let mut shown_message = None;
        while let Some(fetch) = client.take_fetch_work() {
            if let ScheduledFetch::Authorized(fetch) = fetch {
                fetch
                    .fetch_with(|blob_id, bearer_capability| {
                        assert_eq!(blob_id, id(0x23));
                        shown_message = Some(bearer_capability.to_owned());
                        Ok::<(), ()>(())
                    })
                    .expect("authorized waiting-message fetch succeeds");
            }
        }

        let shown_after_return_seconds =
            (returned_after_ms.saturating_sub(OUTAGE_SECONDS * 1_000)) as f64 / 1_000.0;
        assert_eq!(
            connection_states,
            ["connected", "not_connected", "connected"]
        );
        assert_eq!(shown_message.as_deref(), Some(WAITING_MESSAGE));
        assert!(
            shown_after_return_seconds <= 15.0,
            "waiting message was shown after {shown_after_return_seconds:.3}s"
        );
        assert!(
            attempts_during_outage > 1 && attempts_during_outage < 60,
            "reconnect attempts during outage must be >1 and <60, got {attempts_during_outage}"
        );

        println!(
            "TASK3923_CONNECTION_SEQUENCE={}",
            connection_states.join(",")
        );
        println!("TASK3923_NETWORK_DROP_SECONDS={OUTAGE_SECONDS}");
        println!("TASK3923_SENT_WHILE_NETWORK_CUT=\"{WAITING_MESSAGE}\"");
        println!("TASK3923_WAITING_MESSAGE=\"{}\"", shown_message.unwrap());
        println!("TASK3923_WAITING_MESSAGE_AFTER_RETURN_SECONDS={shown_after_return_seconds:.3}");
        println!("TASK3923_RECONNECT_ATTEMPTS_DURING_OUTAGE={attempts_during_outage}");

        fixture.join();
    }

    fn reconnect_after_outage(
        endpoint: &RealtimeEndpoint,
        elapsed_ms: &Arc<AtomicU64>,
        outage_seconds: u64,
    ) -> (crate::realtime_pipe::RealtimeSocket, usize, u64) {
        let mut reconnect = ReconnectSchedule::new();
        let mut attempts_during_outage = 0;
        let outage_ms = outage_seconds * 1_000;
        let max_delay_randoms = [250, 750, 1_750, 3_750, 7_750, 15_750, 29_750];
        let mut randoms = max_delay_randoms
            .into_iter()
            .chain(std::iter::repeat(29_750));

        loop {
            let delay = reconnect.next_delay(randoms.next().expect("repeat is infinite"));
            let delay_ms = u64::try_from(delay.as_millis()).expect("delay fits u64");
            let after_wait = elapsed_ms.fetch_add(delay_ms, Ordering::SeqCst) + delay_ms;
            if after_wait < outage_ms {
                attempts_during_outage += 1;
            }

            match open_realtime_connection(endpoint) {
                Ok(socket) => return (socket, attempts_during_outage, after_wait),
                Err(error) if is_reconnectable(&error) => {}
                Err(error) => panic!("non-reconnectable error during outage: {error:?}"),
            }
        }
    }

    fn is_reconnectable(error: &RealtimePipeError) -> bool {
        matches!(
            error,
            RealtimePipeError::Io(_) | RealtimePipeError::UnexpectedClose
        )
    }

    struct NetworkDropFixture {
        address: SocketAddr,
        handle: thread::JoinHandle<()>,
    }

    impl NetworkDropFixture {
        fn start(elapsed_ms: Arc<AtomicU64>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind network-drop fixture");
            let address = listener.local_addr().expect("fixture address");
            let handle = thread::spawn(move || {
                let mut accepted = 0usize;
                loop {
                    let (mut stream, _) = listener.accept().expect("accept realtime connection");
                    if accepted == 0 {
                        accepted += 1;
                        accept_websocket(&mut stream);
                        read_client_text(&mut stream).expect("first tick before outage");
                        write_server_text(&mut stream, &response(0, 0)).expect("idle response");
                        read_client_text(&mut stream).expect("tick stranded by outage");
                        drop(stream);
                        continue;
                    }

                    if elapsed_ms.load(Ordering::SeqCst) < OUTAGE_SECONDS * 1_000 {
                        accepted += 1;
                        drop(stream);
                        continue;
                    }

                    accept_websocket(&mut stream);
                    read_client_text(&mut stream).expect("tick after reconnect");
                    write_server_text(&mut stream, &response(0x42, 0x23))
                        .expect("waiting-message wakeup after reconnect");
                    break;
                }
            });
            Self { address, handle }
        }

        fn address(&self) -> SocketAddr {
            self.address
        }

        fn join(self) {
            self.handle.join().expect("fixture thread completes");
        }
    }

    fn accept_websocket(stream: &mut TcpStream) {
        let mut request = Vec::new();
        let mut byte = [0_u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            stream
                .read_exact(&mut byte)
                .expect("read websocket upgrade request");
            request.push(byte[0]);
        }
        let request = String::from_utf8(request).expect("ascii websocket request");
        assert!(request.starts_with("GET /v1/realtime HTTP/1.1\r\n"));
        assert!(request
            .lines()
            .any(|line| line.eq_ignore_ascii_case("Upgrade: websocket")));
        stream
            .write_all(
                b"HTTP/1.1 101 Switching Protocols\r\n\
                  Upgrade: websocket\r\n\
                  Connection: Upgrade\r\n\
                  Sec-WebSocket-Accept: task-3923-fixture\r\n\
                  \r\n",
            )
            .expect("write websocket upgrade response");
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
}
