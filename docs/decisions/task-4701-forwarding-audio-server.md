# TASK 4701 — forwarding audio server selection

Status: accepted for backend qualification; this does not put voice back in the
shipped client.  The direct TASK 4701 dispatch is treated as newer authority
for backend work despite the older voice-plan note that voice was out of scope.

## Decision

Choose **LiveKit Server v1.13.5**, source revision
`3b9f118327b257301083a7c4aa46076c8012918a`, release tar SHA-256
`c020fac437b7cc9b776eef1ad5ea8af77be9acfa07602eca20a3a44930dfbc70`.
The deployed signaling listener is behind OSL's TLS 1.3-only front door; media
uses LiveKit's mandatory encrypted WebRTC transports.  Only `livekit-server` is
packaged.  LiveKit Egress, Ingress, SIP, Redis, webhooks, storage, and recording
workers are absent and unreachable.

The decisive distinction is payload versus packet.  An SFU may rewrite RTP
headers for each subscriber.  It must not decode, decrypt, transcode, or
re-encode OSL's already encrypted application payload.  Qualification compares
the RTP `Payload` bytes before and after the SFU.

## Five-requirement comparison

All citations below are reproduced by
`tools/task-4701-forwarding-audio/candidate-source-proof.sh`; it checks out the
exact revision before printing numbered source lines.

| Candidate and source revision | Forward packets; no re-encode | Encrypted per-frame payload untouched | Ordinary transport encryption mandatory | Whole room on one machine | No recording output configured/reachable in OSL deployment |
|---|---|---|---|---|---|
| **LiveKit Server v1.13.5** `3b9f118327b257301083a7c4aa46076c8012918a` | **PASS.** `pkg/sfu/downtrack.go:1011-1044` copies the incoming payload into the outgoing packet; trial proves 3,000/3,000 equality and zero decode/re-encode. | **PASS.** `pkg/rtc/wrappedreceiver.go:93-131` refuses Opus/RED translation for an encrypted source. | **PASS in this deployment.** `config-sample.yaml:53-63` says WebRTC transports are encrypted; lines 15-17 require production TLS fronting. The packaged front door is TLS 1.3-only and has no clear fallback. | **PASS.** `pkg/service/roomallocator.go:134-168` keeps an existing room on its assigned node or selects exactly one node. | **PASS in this deployment.** `README.md:55-60` identifies recording as the separate Egress service. Egress and every other output service are omitted; runtime sink scan must remain zero. |
| **mediasoup v3.19.9** `f8b20b9de5831cb9cd5e2f51c2138129fd61b94f` | **PASS.** `worker/src/RTC/Router.cpp:652-688` gives the same producer packet to consumers. | **PASS for audio.** `worker/src/RTC/Codecs/Opus.cpp:88-107` only inspects the descriptor. | **FAIL strict requirement.** `node/src/Router.ts:517-535` exposes `createPlainTransport` with `enableSrtp=false`. | **CONDITIONAL.** `node/src/WorkerTypes.ts:263-276` creates routers; the application must implement room-to-router/worker placement. | **FAIL strict requirement.** `README.md:30-43` advertises plain RTP input/output and multimedia-tool integration, leaving a reachable sink path. |
| **ion-sfu v1.11.0** `a970af33ddc3bf8782bf49d1de4006180e3e1c08` | **PASS.** `pkg/sfu/downtrack.go:346-384` rewrites the header and writes the original payload. | **PASS for the simple audio path.** The same source line passes `extPkt.Packet.Payload` unchanged. | **FAIL strict requirement.** `cmd/signal/json-rpc/main.go:184-191` falls back to `http.ListenAndServe` when certificate/key are absent. | **PASS.** `pkg/sfu/session.go:220-236` fans a session receiver to its other peers and `pkg/sfu/sfu.go:76-83,190-238` owns sessions in one process. | **FAIL strict requirement.** `README.md:88-93` exposes real-time media-processing and save-to-WebM hooks. |
| **Janus VideoRoom v1.3.3** `07c61050038c7d745013fae8bc8e99d7365c31f1` | **PASS for VideoRoom.** `src/plugins/janus_videoroom.c:13333-13352,13501-13513` relays the incoming buffer while restoring header changes. | **PASS when E2EE is required.** `conf/janus.plugin.videoroom.jcfg.sample:35-46` provides `require_e2ee`. | **FAIL strict requirement.** `src/options.c:56-61` exposes `--no-webrtc-encryption`, disabling DTLS/SRTP. | **PASS.** `src/plugins/janus_videoroom.c:2327-2370` keeps the room and participant table in the process-global room table. | **FAIL strict requirement.** `src/plugins/janus_videoroom.c:2353-2358,7070-7105` contains recorder/file fields and a remotely reachable `enable_recording` request. |

LiveKit is the only candidate whose forwarding and encrypted-source paths fit
without retaining a clear-media switch and whose recording implementation is a
separate service that can be omitted entirely.  The OSL deployment still owns
two non-negotiable controls: TLS-front the signaling endpoint, and fail closed
with the exact verdict `transport encryption required` for a clear offer.
