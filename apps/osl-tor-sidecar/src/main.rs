//! osl-tor-sidecar: the OSL-owned Tor egress process. OSL never borrows
//! another application's Tor: this sidecar embeds its own Arti client,
//! owns its own loopback SOCKS listener bound to port 0 by default (the
//! OS picks the ephemeral port), and reports everything — including that
//! port — as one JSON object per line on stdout. Exit codes: 0 clean
//! shutdown, 2 configuration error, 3 listener failure.

mod args;
mod dial;
mod socks;
mod status;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};
use tor_socksproto::SocksStatus;

use crate::args::Config;
use crate::dial::Dialer;
use crate::status::{StatusEvent, StatusSink};

/// Report a failure on the status channel; errors are events too.
fn error_event(sink: &StatusSink, scope: &str, detail: String) {
    sink.emit(&StatusEvent::Error {
        scope: scope.to_string(),
        detail,
    });
}

fn main() {
    let sink = Arc::new(StatusSink::new());
    let config = match args::parse(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(detail) => {
            error_event(&sink, "config", detail);
            std::process::exit(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            error_event(
                &sink,
                "runtime",
                format!("could not build tokio runtime: {error}"),
            );
            std::process::exit(3);
        }
    };
    let code = runtime.block_on(run(config, Arc::clone(&sink)));
    // Dropping the runtime aborts any connection tasks still relaying.
    drop(runtime);
    std::process::exit(code);
}

async fn run(config: Config, sink: Arc<StatusSink>) -> i32 {
    sink.emit(&StatusEvent::Start {
        pid: std::process::id(),
        dial_mode: config.dial_mode.as_str(),
        requested_listen: config.listen.to_string(),
        bridge_in_use: config.bridge_config.is_some() || config.bridge_fixture.is_some(),
    });

    let listener = match TcpListener::bind(config.listen).await {
        Ok(listener) => listener,
        Err(error) => {
            error_event(
                &sink,
                "listener",
                format!("bind of {} failed: {error}", config.listen),
            );
            return 3;
        }
    };
    let local = match listener.local_addr() {
        Ok(local) => local,
        Err(error) => {
            error_event(&sink, "listener", format!("local_addr failed: {error}"));
            return 3;
        }
    };
    // The supervisor's cue that the SOCKS port is ready; the only place
    // the OS-chosen port number is published.
    sink.emit(&StatusEvent::Listening {
        addr: local.to_string(),
        ip: local.ip().to_string(),
        port: local.port(),
    });
    if config.dial_mode == crate::args::DialMode::Direct {
        sink.emit(&StatusEvent::Ready {
            bridge_in_use: false,
        });
    }

    let dialer = Arc::new(Dialer::new(&config));
    let conn_ids = AtomicU64::new(0);

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, peer)) => {
                        let conn = conn_ids.fetch_add(1, Ordering::Relaxed);
                        let sink = Arc::clone(&sink);
                        let dialer = Arc::clone(&dialer);
                        tokio::spawn(async move {
                            handle_connection(stream, peer.to_string(), conn, sink, dialer).await;
                        });
                    }
                    // Transient accept errors (EMFILE and friends) do not
                    // kill the listener.
                    Err(error) => error_event(&sink, "accept", error.to_string()),
                }
            }
            _ = shutdown_signal() => {
                sink.emit(&StatusEvent::Shutdown { reason: "signal".to_string() });
                return 0;
            }
        }
    }
}

/// Resolves when the process is asked to stop (SIGINT or SIGTERM).
#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(term) => term,
        Err(_) => {
            // No SIGTERM handler: fall back to ctrl-c alone.
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

/// Windows has no Unix SIGTERM stream. Ctrl-C is still the normal console
/// shutdown signal and lets the packaged sidecar exit cleanly there.
#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Serve one accepted SOCKS connection start to finish. Every exit path
/// emits a status event, so silence never has to be interpreted.
async fn handle_connection(
    mut stream: TcpStream,
    peer: String,
    conn: u64,
    sink: Arc<StatusSink>,
    dialer: Arc<Dialer>,
) {
    sink.emit(&StatusEvent::Accepted { conn, peer });

    let completed = match socks::run_handshake(&mut stream).await {
        Ok(completed) => completed,
        Err(reason) => {
            sink.emit(&StatusEvent::Refused { conn, reason });
            return;
        }
    };
    let request = &completed.request;

    if !socks::is_supported(request) {
        let status = SocksStatus::COMMAND_NOT_SUPPORTED;
        let _ = socks::send_reply(&mut stream, request, status).await;
        sink.emit(&StatusEvent::Refused {
            conn,
            reason: "only CONNECT is supported".to_string(),
        });
        return;
    }

    let target = socks::Target::from_request(request);
    sink.emit(&StatusEvent::Request {
        conn,
        target: target.describe(),
    });

    let upstream = match dialer.dial(&sink, &target).await {
        Ok(upstream) => upstream,
        Err(reason) => {
            let status = SocksStatus::HOST_UNREACHABLE;
            let _ = socks::send_reply(&mut stream, request, status).await;
            sink.emit(&StatusEvent::ConnectFailed {
                conn,
                target: target.describe(),
                reason,
            });
            return;
        }
    };

    if let Err(reason) = socks::send_reply(&mut stream, request, SocksStatus::SUCCEEDED).await {
        error_event(&sink, &format!("conn:{conn}"), reason);
        return;
    }
    sink.emit(&StatusEvent::ConnectOk {
        conn,
        target: target.describe(),
    });

    match dial::relay(&mut stream, upstream, &completed.readahead).await {
        Ok((bytes_to_target, bytes_from_target)) => sink.emit(&StatusEvent::Closed {
            conn,
            bytes_to_target,
            bytes_from_target,
        }),
        Err(detail) => error_event(&sink, &format!("conn:{conn}"), detail),
    }
}
