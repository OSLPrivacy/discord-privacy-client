# OSL Spaces voice: feasibility and release gate

Status: design decision, 2026-08-02. This document does not add voice calling or alter a shipped
surface.

## Decision

**Voice is feasible, but it is post-v1 work on its own track.** OSL Spaces v1 does not include
voice. A future voice channel must remain a plainly unavailable state: it cannot request a
microphone, create a call, or imply that someone can join it.

The allowed unavailable-state copy is:

> Voice is coming later. It is not available in OSL Spaces yet.

Do not describe the future feature as private, anonymous, metadata-free, zero-knowledge, or
end-to-end encrypted until the release gates in this document are implemented and independently
proved. This resolves OQ-21-7 as **post-v1**; it is a scope boundary, not a delivery promise.

## Current evidence

The Spaces prototype is a UI mock, not an incomplete calling implementation. Its `callModal()`
function in `docs/prototypes/osl-chats-lab/app.js:437-440` only opens a modal. The prototype has no
`RTCPeerConnection` and no `getUserMedia`, so there is no signaling, capture, NAT traversal, media
path, key distribution, or call admission to harden. Treating it as a launchable voice surface
would turn a visual affordance into a false product claim.

## What the real system requires

WebRTC supplies media transport and ICE connectivity, not the system around a group call. A usable
E2EE Spaces call needs all of these parts:

1. An authenticated signaling service to exchange offers, answers, and ICE candidates, issuing
   short-lived call credentials only to current Space members.
2. STUN/TURN operations for real NAT traversal. Direct peer paths expose network information to
   peers; relay paths still expose traffic to the relay.
3. An SFU for multiparty calls. A mesh makes each participant upload one stream per listener;
   an SFU lets a participant upload once and forwards selected streams to listeners. That is an
   operational media service, not a text-message relay.
4. A media-key protocol that binds every epoch to authenticated Space membership. It must cover
   creation, device add, join, leave, removal, rejoin, rotation, replay protection, and recovery.
   Text sender keys are not automatically a voice-key design.
5. Platform work: desktop capture permission, clear microphone state, mute and device changes,
   reconnect/error behavior, accessibility, and test coverage on every supported platform.

## Privacy and metadata boundary

End-to-end media encryption can prevent the SFU from reading audio. It cannot conceal a live call
from the network path that must route it. At minimum, a media service can learn an opaque
room-scoped participant connection, connection and disconnection times, packet timing and volume,
and network-routing data. It can therefore infer who is participating with whom in real time.

Padding cannot make an interactive call look like no call, and it does not remove the media
operator's need to receive and forward packets. This is an unavoidable privacy cost, not a defect
to obscure with marketing.

The future design must use opaque, per-call participant identifiers at the media boundary. It must
not send usernames, display names, contact graphs, text history, governance data, plaintext media
keys, recordings, transcripts, moderation content, or analytics to that boundary. Any recording,
transcription, abuse pipeline, or retention proposal is a separate plaintext/behavioural-data
decision and requires its own threat-model review.

## Cost model

Voice costs participant-minutes and downstream media, rather than stored-message bytes. Before
implementation, the chosen provider or self-hosted design must publish a dated quote and a
load-tested concurrency budget. Model monthly operating cost as:

```
plan fee
+ billable participant-minutes × per-participant-minute price
+ billable downstream GiB × egress price
+ TURN relay, public IP, observability, support, and on-call costs
```

For self-hosting, replace vendor line items with regional SFU capacity, TURN bandwidth and egress,
monitoring, alerting, incident response, load testing, and an exit/migration plan. Self-hosting may
improve operational control; it does not remove the media-path metadata or make multiparty media
free. Planning must include both sustained participants and burst concurrency, audio/video bitrate,
TURN-only traffic, and failure capacity—not merely average monthly minutes.

## Future-track release gates

No implementation or public availability claim is allowed until the separate voice track has all of
the following, with an owner and executable evidence:

1. Authenticated Space membership and sender authentication are shipped. An unauthenticated or
   removed client cannot obtain a call credential or impersonate a participant.
2. Signaling accepts only an opaque, short-lived membership-bound call capability and never
   receives plaintext media keys.
3. The media-key/epoch protocol is specified and tested for join, removal, device add, rejoin,
   rotation, replay, and recovery. A removal test proves that the removed member cannot decrypt
   media after the applicable rotation point.
4. The SFU and TURN deployment has documented regions, retention and logging settings, rate
   limits, capacity limits, cost alerts, abuse boundary, incident procedure, and exit plan.
5. A multi-party harness proves normal NAT, TURN-only connectivity, packet loss, reconnect,
   membership removal during a call, key rotation, and that the media operator cannot recover
   plaintext audio.
6. Native UI tests cover permission denial, microphone indicators, mute semantics, audio-device
   changes, background behavior, unavailable/error states, and accessibility on each supported
   platform.
7. The UI and claim allowlist are reviewed against the metadata boundary. Failure must be closed
   and honest—never a silent plaintext downgrade or an animation that mimics a working call.

Until every gate passes, Spaces may show only the unavailable copy above. The v1 named-groups
subset remains explicitly **no voice**, consistent with the Spaces plan's scope boundary.

## References

- [WebRTC peer connections](https://webrtc.org/getting-started/peer-connections): signaling is
  outside WebRTC and ICE uses STUN/TURN.
- [WebRTC security architecture](https://webrtc-security.github.io/): transport security does not
  erase application or traffic-analysis considerations.
