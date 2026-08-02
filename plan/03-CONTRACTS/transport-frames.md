# CONTRACT — realtime frames (T1-50)

**Status:** FROZEN 2026-08-01. **Owner:** T1. **Consumers:** T1-51 and T1-52.

`09-DECISIONS.md` overrides this contract. This is the concrete wire format for
`transport.md` §6/§6b: a client-ticked, constant-rate WebSocket wakeup channel.
It is deliberately ordinary socket protocol; neither its format nor its semantics
depend on Cloudflare or Durable Objects.

## 1. Schedule and connection lifetime

The client opens one WebSocket and sends one `TICK` every `TICK_MS = 4_000` ms,
independent of input, focus, carrier arrival, a prior response, or whether work is
pending. The server sends exactly one `RESPONSE` for each accepted `TICK`.
Both directions send a frame on every tick, including when their entry count is
zero. A client selects a fresh random `session_id` for each new WebSocket; it is a
per-connection nonce, not an account, device identifier, or credential.

On any close or failed write, the client reconnects with full jitter in the range
`[0, min(30_000, 500 * 2^attempt)]` ms, increments `attempt` only for consecutive
failures, and resets it after one accepted response. It starts a new session and
continues the cadence from the next scheduled tick (it does not burst missed ticks).
It sends its complete current subscription window on every `TICK`, so reconnect
requires no durable server-side association.

`TICK_MS` may be changed only as a protocol revision. A direct and Tor connection
may select different negotiated revisions in a future revision; neither may become
event driven.

## 2. Canonical frame encoding

`FRAME_RAW_BYTES = 1536`; the WebSocket payload is the unpadded base64url encoding
of those raw bytes, so `FRAME_TEXT_CHARS = 2048` exactly. Frames are text messages.
An endpoint rejects a non-text message, a frame not exactly 2048 ASCII base64url
characters, a decoding error, or a decoded length other than 1536. This is an OSL
protocol constant, not a server limit.

All integer fields below are unsigned, big-endian. A decoder rejects an unknown
version, direction, non-zero reserved field, count above its direction's maximum,
or non-zero unused slot bytes. That canonical padding makes an accepted frame have
one encoding and prevents the padding from becoming a side channel.

| raw offset | length | field |
| --- | ---: | --- |
| 0 | 4 | ASCII magic `OSLF` |
| 4 | 1 | `version = 1` |
| 5 | 1 | direction: `0 = TICK`, `1 = RESPONSE` |
| 6 | 2 | reserved, all zero |
| 8 | 16 | `session_id` — 16 CSPRNG bytes chosen by the client |
| 24 | 8 | `frame_id` — sender sequence number, starting at 1 in each direction/session |
| 32 | 8 | `ack_id` — largest contiguous peer `frame_id` accepted in this session; 0 means none |
| 40 | 1 | entry count |
| 41 | 23 | reserved, all zero |
| 64 | 1472 | direction-specific fixed slot area |

The receiver checks the echoed `session_id` before interpreting a response. It
rejects `frame_id = 0`, an `ack_id` greater than the largest sent id, and a frame id
that skips a required earlier replay. IDs are connection-local and are not uploaded,
stored, or used to associate sessions.

### 2.1 `TICK` slots — subscriptions

Each of the 92 slots is 16 bytes: one raw `delivery_tag`. Slots after `entry_count`
are zero. A `TICK` therefore contains `0..92` tags; tags are unique within a tick
and all-zero is invalid as an active tag. They are the complete current subscription
set for that connection, replacing the prior set atomically after the tick is
accepted. The server retains it only for the live socket.

`delivery_tag` is the 16-byte value derived by `transport.md` §6b. It is opaque to
this protocol; the frame never contains a conversation id, account, identity,
pointer `P`, or any fetch/ack/manage capability.

### 2.2 `RESPONSE` slots — wakeups

Each of the 46 slots is 32 bytes:

```text
[delivery_tag: 16 raw bytes][blob_id: 16 raw bytes]
```

Slots after `entry_count` are zero. A `RESPONSE` therefore contains `0..46` wakeups.
Tags and `(delivery_tag, blob_id)` pairs must not repeat in one response. `blob_id`
is the 16-byte identifier from `storage.md` §2 (normally rendered as 32 hex only on
the HTTP path).

A response is a **wakeup, not a delivery**. Possession of `blob_id` cannot fetch an
object: a client must already have the carrier pointer `P` to derive `fetch_cap`.
It contains no payload or authorization and receipt of one does not authorize a
network action outside the normal fixed-rate work schedule.

## 3. Acknowledgement and replay

The sender assigns a new non-zero `frame_id` only after its previous frame has been
acknowledged. Until then it retransmits the byte-identical prior frame on each next
turn; its `frame_id`, slots, and zero padding do not change. Thus a server response
is replayed until a later client tick acknowledges it, and a client tick is replayed
until a response acknowledges it. Receiving the same already-accepted frame id is
idempotent: validate that its bytes equal the recorded frame, re-emit the required
answer, and do not apply its subscription update or wakeups twice.

`ack_id` acknowledges every peer frame through that id. An endpoint retains at most
its single unacknowledged frame in memory for the live socket. After a reconnect,
the client sends the entire subscription set again; T6's fresh match query supplies
any still-live blobs. No replay state or tag-to-client association survives a socket
close.

The client deduplicates a wakeup by `(delivery_tag, blob_id)` while it is pending
local processing. It must tolerate a replay and a later storage `404`: fetch is
still authorized only by the carrier-derived capability, and `404` is deliberately
ambiguous.

## 4. Server matching boundary and portability

For each accepted tick, the server asks T6 for live objects matching the ephemeral
subscription set and packs up to 46 results into the response. If more results are
available, it emits them over later acknowledged responses. T6 must not create a
durable tag/client, tag/session, or account association. The frame server holds only
the live socket's subscription set and one unacknowledged response.

An implementation may use an automatic WebSocket response facility as an optimisation,
but it must have identical observable behaviour to a plain server that receives a
tick, makes this match query, and writes one response. There are no alarms,
server-originated ticks, provider headers, or provider-specific control frames in
this contract.

Constant length and cadence hide whether a response has wakeups from a wire observer;
they do not hide that a client is connected or its IP from the server. The persistent
connection versus polling choice is owned by the onboarding decision (D64).

## 5. T1-T50 frame vectors

The hashes below are SHA-256 of the 1536 raw bytes, before base64url encoding. Hex
strings in the vectors are raw field bytes. They fix the byte order, header layout,
slot layout, zero padding, and fixed capacity without making prose a parser API.

```json
{
  "format": 1,
  "rawBytes": 1536,
  "textChars": 2048,
  "tickSlots": 92,
  "responseSlots": 46,
  "tick": {
    "sessionId": "000102030405060708090a0b0c0d0e0f",
    "frameId": "1",
    "ackId": "0",
    "subscriptions": [
      "101112131415161718191a1b1c1d1e1f",
      "202122232425262728292a2b2c2d2e2f"
    ],
    "sha256": "d51874c45f22903603668e299953373aaef43bcc390c396685383649e1289d7f"
  },
  "response": {
    "sessionId": "000102030405060708090a0b0c0d0e0f",
    "frameId": "1",
    "ackId": "1",
    "wakeups": [
      {
        "deliveryTag": "101112131415161718191a1b1c1d1e1f",
        "blobId": "303132333435363738393a3b3c3d3e3f"
      }
    ],
    "sha256": "f019b9b548ef6b9bfd027d8b068923b56dd836bc3223ad6ba11aa299666c0fce"
  }
}
```
