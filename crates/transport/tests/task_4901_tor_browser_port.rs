use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use transport::tor::{ArtiProxyConfig, TorTransport, DEFAULT_SOCKS_ADDR};

#[test]
#[ignore = "TASK 4901 red check: proves OSL currently accepts a foreign listener on Tor Browser's 9150 port"]
fn task_4901_osl_must_not_borrow_tor_browser_socks_port() {
    assert_eq!(
        DEFAULT_SOCKS_ADDR.to_string(),
        "127.0.0.1:9150",
        "TASK 4901 is pinned to crates/transport/src/tor.rs:25 and the hard-coded 127.0.0.1:9150"
    );
    assert!(
        Command::new("/bin/sleep").arg("0").status().is_ok(),
        "TASK 4901 needs /bin/sleep to model a live OSL-owned Arti process with no listener"
    );

    let fake_tor_browser = TcpListener::bind(DEFAULT_SOCKS_ADDR).unwrap_or_else(|error| {
        panic!(
            "TASK 4901 could not bind fake Tor Browser SOCKS listener on hard-coded 127.0.0.1:9150: {error}"
        )
    });
    let recorder = thread::spawn(move || record_fake_tor_browser_greetings(fake_tor_browser));

    let route_marked_ready = start_osl_with_tor_selected_and_no_osl_owned_tunnel()
        .map(|transport| {
            make_readiness_result_use_the_borrowed_route(&transport);
            transport.stop();
            true
        })
        .unwrap_or(false);

    let greetings = recorder.join().expect("join fake SOCKS recorder");
    let greeting_count = greetings.len();
    let greeting_hex = greetings
        .iter()
        .map(|greeting| {
            greeting
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join("")
        })
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "TOR-4901-OBSERVED source=crates/transport/src/tor.rs:25 hard-coded=127.0.0.1:9150 route_marked_ready={route_marked_ready} fake_socks_greetings={greeting_count} greeting_hex={greeting_hex}"
    );

    if route_marked_ready && greeting_count >= 1 {
        panic!(
            "TOR-4901-RED crates/transport/src/tor.rs:25 hard-coded 127.0.0.1:9150 route incorrectly marked ready and fake listener recorded {greeting_count} SOCKS greeting(s)"
        );
    }
}

fn start_osl_with_tor_selected_and_no_osl_owned_tunnel() -> Result<TorTransport, String> {
    let mut config = ArtiProxyConfig::new("/bin/sleep");
    config.args = vec!["5".to_owned()];
    config.bootstrap_timeout = Duration::from_secs(2);
    config.readiness_poll_interval = Duration::from_millis(50);
    TorTransport::start(config).map_err(|error| error.to_string())
}

fn make_readiness_result_use_the_borrowed_route(transport: &TorTransport) {
    let Ok(client) = transport.store_client() else {
        return;
    };
    let _ = client.get("https://store.invalid/task-4901").send();
}

fn record_fake_tor_browser_greetings(listener: TcpListener) -> Vec<Vec<u8>> {
    listener
        .set_nonblocking(true)
        .expect("put fake SOCKS listener in nonblocking mode");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut greetings = Vec::new();

    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_millis(500)))
                    .expect("set fake SOCKS stream read timeout");
                let mut buffer = [0_u8; 260];
                match stream.read(&mut buffer) {
                    Ok(read) if read > 0 && buffer[0] == 5 => {
                        greetings.push(buffer[..read].to_vec());
                        let _ = stream.write_all(&[5, 0]);
                        let _ = stream.read(&mut buffer);
                    }
                    _ => {}
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("fake SOCKS listener failed: {error}"),
        }
    }

    greetings
}
