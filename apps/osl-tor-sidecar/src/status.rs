//! The stdout status channel: one complete JSON object per line, flushed
//! immediately, and nothing else — no prose, no tracing, no panic text.

use serde::Serialize;
use std::io::Write;
use std::sync::Mutex;

/// One status event. The `event` tag and field names are the sidecar's
/// wire contract; renaming them breaks every status consumer.
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StatusEvent {
    /// Emitted exactly once, before anything else.
    Start {
        pid: u32,
        dial_mode: &'static str,
        requested_listen: String,
        bridge_in_use: bool,
    },
    /// The SOCKS listener is bound; `addr` is what the OS assigned, and
    /// `port` repeats the port for consumers that do not parse `addr`.
    Listening { addr: String, ip: String, port: u16 },
    /// Progress of the embedded Arti client, tor dial mode only.
    Bootstrap {
        state: &'static str,
        percent: u8,
        bridge_in_use: bool,
    },
    /// The Tor client, rather than only its loopback listener, is ready.
    Ready { bridge_in_use: bool },
    /// A client connected to the SOCKS listener.
    Accepted { conn: u64, peer: String },
    /// A SOCKS handshake completed and asked us to reach `target`.
    Request { conn: u64, target: String },
    /// The upstream dial succeeded and relaying is about to begin.
    ConnectOk { conn: u64, target: String },
    /// The upstream dial failed; the SOCKS client got a failure reply.
    ConnectFailed {
        conn: u64,
        target: String,
        reason: String,
    },
    /// The request was refused before any dial was attempted.
    Refused { conn: u64, reason: String },
    /// A relayed connection finished, with per-direction byte counts.
    Closed {
        conn: u64,
        bytes_to_target: u64,
        bytes_from_target: u64,
    },
    /// A recoverable error tied to one connection or subsystem.
    Error { scope: String, detail: String },
    /// The sidecar is exiting.
    Shutdown { reason: String },
}

/// Serializes events onto stdout, one JSON line each. The mutex keeps
/// concurrent connection tasks from interleaving partial lines; flushing
/// per event keeps the stream usable as a live feed (a buffered
/// `Listening` line would stall the supervisor's startup).
pub struct StatusSink {
    out: Mutex<std::io::Stdout>,
}

impl StatusSink {
    pub fn new() -> Self {
        StatusSink {
            out: Mutex::new(std::io::stdout()),
        }
    }

    /// Emit one event as one line. Both error paths fall through: if
    /// stdout is gone the supervisor has abandoned us, and prose
    /// anywhere would break the channel contract.
    pub fn emit(&self, event: &StatusEvent) {
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        let Ok(mut out) = self.out.lock() else {
            return;
        };
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    }
}
