//! Client half of the T1 realtime wakeup channel.
//!
//! The Cloudflare endpoint is intentionally simple: every client tick is a
//! 2,048-character text message of spaces and every reply is a JSON wakeup
//! padded with spaces to the same size.  A reply carries discovery data only;
//! the bearer capability stays in the carrier-derived local pointer.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Duration;

use serde_json::Value;

use crate::realtime_decoy::DecoyFetch;

/// Exact text-frame size accepted by `cipher-store-cf/src/realtime/connection.ts`.
pub const FRAME_BYTES: usize = 2_048;
/// The protocol's fixed client cadence. This is not a backoff or work queue delay.
pub const TICK_INTERVAL: Duration = Duration::from_secs(4);

const ID_BYTES: usize = 16;

/// The route selected before the realtime connection is opened.
///
/// Tor is an explicit privacy choice.  It may use a distinct cadence only in a
/// negotiated protocol revision; v1 keeps the frozen four-second schedule for
/// both routes.  Neither route accepts link-quality or work-queue input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeRoute {
    Direct,
    Tor,
}

impl RealtimeRoute {
    pub const fn tick_interval(self) -> Duration {
        match self {
            Self::Direct | Self::Tor => TICK_INTERVAL,
        }
    }
}

/// Build the fixed tick frame required by the server.
///
/// This allocation happens once per scheduled write; it is intentionally
/// independent of local work and prior responses.
pub fn tick_frame() -> String {
    " ".repeat(FRAME_BYTES)
}

/// Opaque 16-byte identifier, encoded as lowercase hex only on the wire.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlobId([u8; ID_BYTES]);

impl BlobId {
    pub const fn from_bytes(bytes: [u8; ID_BYTES]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; ID_BYTES] {
        &self.0
    }

    const fn is_zero(&self) -> bool {
        let mut index = 0;
        while index < ID_BYTES {
            if self.0[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }
}

/// A carrier-provided pointer. It is deliberately not parsed from a wakeup frame.
#[derive(Clone)]
pub struct CarrierPointer {
    bearer_capability: String,
}

impl CarrierPointer {
    /// Construct only from the authenticated carrier path, out of band from realtime.
    pub fn from_carrier(bearer_capability: impl Into<String>) -> Self {
        Self {
            bearer_capability: bearer_capability.into(),
        }
    }

    fn bearer_capability(&self) -> &str {
        &self.bearer_capability
    }
}

/// A fetch that was authorized by a pre-existing carrier pointer, not by a wakeup.
pub struct AuthorizedFetch {
    pub blob_id: BlobId,
    pointer: CarrierPointer,
}

impl AuthorizedFetch {
    /// Invoke the normal bearer-authorized fetch path when its own schedule permits.
    pub fn fetch_with<F, E>(&self, fetch: F) -> Result<(), E>
    where
        F: FnOnce(BlobId, &str) -> Result<(), E>,
    {
        fetch(self.blob_id, self.pointer.bearer_capability())
    }
}

/// Exactly one fetch-shaped action follows every realtime reply.  A matching
/// carrier pointer authorizes the real fetch; all other replies produce the
/// ordinary capability-negative decoy instead.
pub enum ScheduledFetch {
    Authorized(AuthorizedFetch),
    Decoy(DecoyFetch),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct WakeupKey {
    delivery_tag: [u8; ID_BYTES],
    blob_id: BlobId,
}

/// Errors are intentionally fail-closed: malformed wakeups never cause fetch work.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrameError {
    WrongLength,
    NonSpacePadding,
    InvalidJson,
    UnexpectedFields,
    InvalidIdentifier,
}

/// Maintains a cadence independent of server replies and queued local work.
#[derive(Clone, Debug)]
pub struct ConstantRateSchedule {
    next_tick: Duration,
    interval: Duration,
}

impl ConstantRateSchedule {
    pub const fn from_start(start: Duration) -> Self {
        Self {
            next_tick: start,
            interval: TICK_INTERVAL,
        }
    }

    pub const fn for_route(start: Duration, route: RealtimeRoute) -> Self {
        Self {
            next_tick: start,
            interval: route.tick_interval(),
        }
    }

    /// Return the next scheduled instant and advance by exactly one protocol interval.
    pub fn next_frame(&mut self) -> (Duration, String) {
        let when = self.next_tick;
        self.next_tick = self
            .next_tick
            .checked_add(self.interval)
            .expect("tick schedule overflow");
        (when, tick_frame())
    }
}

/// A realtime connection state machine. Call `next_outbound_frame` every turn;
/// handling a response cannot alter its cadence or its emitted size.
pub struct RealtimeClient {
    schedule: ConstantRateSchedule,
    pointers: BTreeMap<BlobId, CarrierPointer>,
    pending: BTreeSet<WakeupKey>,
    scheduled_fetches: VecDeque<ScheduledFetch>,
}

impl RealtimeClient {
    pub fn new(start: Duration) -> Self {
        Self::for_route(start, RealtimeRoute::Direct)
    }

    /// Start a constant-rate stream for the route selected at onboarding.
    ///
    /// The route only selects a frozen protocol policy. It never lets latency,
    /// tunnel state, or queued work vary the tick schedule.
    pub fn for_route(start: Duration, route: RealtimeRoute) -> Self {
        Self {
            schedule: ConstantRateSchedule::for_route(start, route),
            pointers: BTreeMap::new(),
            pending: BTreeSet::new(),
            scheduled_fetches: VecDeque::new(),
        }
    }

    /// Register a pointer received from the carrier before any wakeup is handled.
    pub fn remember_carrier_pointer(&mut self, blob_id: BlobId, pointer: CarrierPointer) {
        self.pointers.insert(blob_id, pointer);
    }

    /// Emits exactly one constant-size tick at the next fixed scheduled instant.
    pub fn next_outbound_frame(&mut self) -> (Duration, String) {
        self.schedule.next_frame()
    }

    /// Decode a server response and queue only fetches justified by local pointers.
    /// This method performs no fetch itself.
    pub fn receive_frame(&mut self, frame: &str) -> Result<(), FrameError> {
        let wakeup = parse_wakeup(frame)?;
        // T1-51's empty response is a zero/zero decoy. It is not a wakeup,
        // even if a caller accidentally retained a pointer with an all-zero id.
        // It still schedules a capability-negative fetch so a fetch after a
        // reply cannot reveal whether that reply matched a local pointer.
        if wakeup.0 == [0; ID_BYTES] && wakeup.1.is_zero() {
            self.scheduled_fetches
                .push_back(ScheduledFetch::Decoy(DecoyFetch::random()));
            return Ok(());
        }
        let key = WakeupKey {
            delivery_tag: wakeup.0,
            blob_id: wakeup.1,
        };
        if !self.pending.insert(key) {
            return Ok(());
        }

        // `blob_id` is an unauthorised hint. Only a pointer already held before
        // this frame turns it into locally scheduled bearer-authenticated work.
        if let Some(pointer) = self.pointers.get(&wakeup.1).cloned() {
            self.scheduled_fetches.push_back(ScheduledFetch::Authorized(AuthorizedFetch {
                blob_id: wakeup.1,
                pointer,
            }));
        } else {
            self.scheduled_fetches
                .push_back(ScheduledFetch::Decoy(DecoyFetch::random()));
        }
        Ok(())
    }

    pub fn take_fetch_work(&mut self) -> Option<ScheduledFetch> {
        self.scheduled_fetches.pop_front()
    }
}

fn parse_wakeup(frame: &str) -> Result<([u8; ID_BYTES], BlobId), FrameError> {
    if frame.len() != FRAME_BYTES || !frame.is_ascii() {
        return Err(FrameError::WrongLength);
    }
    let json = frame.trim_end_matches(' ');
    if json.len() == frame.len() || frame[json.len()..].bytes().any(|byte| byte != b' ') {
        return Err(FrameError::NonSpacePadding);
    }
    let value: Value = serde_json::from_str(json).map_err(|_| FrameError::InvalidJson)?;
    let object = value.as_object().ok_or(FrameError::UnexpectedFields)?;
    if object.len() != 2 || !object.contains_key("delivery_tag") || !object.contains_key("blob_id")
    {
        return Err(FrameError::UnexpectedFields);
    }
    let tag = parse_id(
        object["delivery_tag"]
            .as_str()
            .ok_or(FrameError::InvalidIdentifier)?,
    )?;
    let blob = BlobId(parse_id(
        object["blob_id"]
            .as_str()
            .ok_or(FrameError::InvalidIdentifier)?,
    )?);
    Ok((tag, blob))
}

fn parse_id(value: &str) -> Result<[u8; ID_BYTES], FrameError> {
    if value.len() != ID_BYTES * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(FrameError::InvalidIdentifier);
    }
    let mut result = [0; ID_BYTES];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        result[index] = (hex(chunk[0])? << 4) | hex(chunk[1])?;
    }
    Ok(result)
}

fn hex(byte: u8) -> Result<u8, FrameError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(FrameError::InvalidIdentifier),
    }
}
