# OSL Spaces voice: feasibility and release gate

Status: design decision, 2026-08-01. This document does not add voice calling or change a
shipped surface.

## Decision

**Voice is feasible, but it is post-v1 work on a separate track.** The first usable Spaces
release ships without voice. A voice channel may exist as a future channel kind, but it must not
start capture, create a call, or imply that a call can be joined.

The honest unavailable-state copy is:

> Voice is coming later. It is not available in OSL Spaces yet.

Do not call the future feature private, anonymous, zero-knowledge, metadata-free, or
end-to-end encrypted until the release gate below has been passed and those specific properties
have been independently tested.

```json
{
  "decision": "post-v1-separate-track",
  "availability": "coming-later-not-implemented",
  "requiredBeforeBuild": [
    "authenticated-space-membership",
    "authenticated-voice-signaling",
    "end-to-end-media-key-distribution-and-rotation",
    "sfu-and-turn-operations",
    "cross-platform-call-security-and-reliability-tests"
  ],
  "prohibitedClaims": [
    "voice-is-available",
    "voice-hides-real-time-participation-metadata",
    "voice-is-zero-knowledge-to-the-media-operator"
  ],
  "unavoidableMetadata": [
    "a-participant-connected-to-a-room",
    "connection-and-disconnection-times",
    "media-flow-timing-and-volume",
    "network-address-information-without-an-additional-relay"
  ]
}
```

## Why this is a separate system

WebRTC supplies peer connections and ICE, but deliberately does not supply signaling; clients
still need a signaling service to exchange offers, answers, and ICE candidates. Connectivity also
requires STUN/TURN infrastructure for real-world NAT traversal. A many-party call should use an
SFU rather than a full mesh: every sender uploads one encoded stream and the SFU forwards selected
streams to listeners. That avoids a quadratic client bandwidth cost, but introduces a live media
operator.

Transport encryption alone is insufficient. The SFU must forward encrypted media without access to
its plaintext, while members receive authenticated media keys that rotate when a participant joins,
leaves, is removed, or changes device. The existing text sender-key work cannot simply be named as
the solution: voice needs a distinct media-key and epoch design, membership/authentication binding,
late-join behaviour, mute and device-change rules, and real-call recovery tests.

## Privacy boundary

End-to-end media encryption can prevent the SFU from reading audio, but cannot hide the existence
and shape of a live connection from that SFU. At minimum, the media path can observe a room-scoped
pseudonymous connection, start/end times, packet timing and volume, and network-routing data. A
TURN relay can also handle media. Padding cannot make a real-time interactive call look like no
call, and it does not remove the operator's need to route media.

Consequences for OSL Spaces:

- Do not expose presence, a member roster, speaking indicators, or a public activity count merely
  because the media backend has connection state.
- Use opaque, per-call participant identifiers at the media boundary; do not send usernames,
  display names, contact graphs, text history, or Space governance data there.
- No recording, transcription, moderation pipeline, analytics, or retained media logs in v1.
  Those features would create a new plaintext or behavioural-data disclosure and require an
  explicit future decision.
- Direct peer-to-peer paths do not meet OSL's metadata posture either: they reveal network
  information to peers. Any proposed relay or anonymity design needs a separate threat-model
  review; it is not implied by the text transport's Tor option.

## Cost model (not a quote)

The cost driver is participant-minutes and downstream media, not stored message bytes. For a
managed SFU, estimate monthly spend as:

```
max(0, participant_minutes - included_participant_minutes) * participant_minute_price
+ max(0, downstream_GiB - included_downstream_GiB) * downstream_GiB_price
+ plan_fee + TURN/egress/observability costs not included by the provider
```

For a self-hosted SFU, replace the provider line items with regional VM capacity, TURN relay
bandwidth, public-IP and egress charges, monitoring, on-call coverage, and load-test capacity.
Self-hosting preserves D11's portability goal but does not make media transport free.

As a reproducible planning reference only, LiveKit's public pricing page accessed 2026-08-01 lists
its Ship plan at $50/month, 150,000 included WebRTC participant-minutes, then $0.0005 per minute,
250 GB included downstream transfer, then $0.12/GB. Those figures are vendor pricing, may change,
and are not an OSL commitment. Before a build starts, select a provider or self-hosted capacity,
record its region and retention terms, and replace this reference with a dated quote and a
load-tested concurrency budget.

Example workload: 1,000 people using voice for 20 minutes/day for 30 days is 600,000
participant-minutes/month. At the quoted rate it leaves 450,000 billable participant-minutes
($225 before the plan fee and data-transfer/TURN costs). This is intentionally a participant-minute
example, not a promise about concurrency, audio bitrate, or total bill.

## Build gate for the future voice track

The future track may begin only after all of the following have an owner and an executable proof:

1. Authenticated Space membership and sender authentication are shipped; no unauthenticated client
   can obtain a call admission credential or impersonate a participant.
2. The call-control/signaling service authenticates an opaque Space membership capability, binds it
   to a short-lived call credential, and never receives plaintext media keys.
3. The media-key protocol documents creation, distribution, authentication, removal, rejoin,
   device addition, and rotation. Its tests prove a removed participant cannot decrypt media after
   the applicable rotation point.
4. An SFU and TURN deployment is chosen with a data-retention/logging configuration, rate limits,
   abuse boundary, capacity limits, cost alerting, and a self-hosting/exit plan consistent with D11
   and D31b.
5. Native desktop capture permission, microphone indicators, mute semantics, audio-device changes,
   background behaviour, failure/reconnect states, and accessibility are implemented and tested on
   every supported platform.
6. A multi-party test harness covers normal NAT, TURN-only connectivity, packet loss, reconnect,
   member removal during a call, key rotation, and a media-operator plaintext-negative test.
7. The UI and claim allowlist have been reviewed against the privacy boundary above. A call must
   fail closed with an honest unavailable/error state, never silently downgrade to plaintext.

Until all seven gates pass, the only allowed product state is the unavailable copy above. This is a
gate, not a promise of delivery date.

## Sources

- [WebRTC: getting started with peer connections](https://webrtc.org/getting-started/peer-connections)
  — signaling is outside WebRTC; ICE uses STUN/TURN.
- [LiveKit: camera and microphone](https://docs.livekit.io/transport/media/publish/) — capture
  permissions and track publication examples.
- [LiveKit pricing](https://livekit.com/pricing) — planning reference for participant-minute and
  downstream-transfer pricing, accessed 2026-08-01.
