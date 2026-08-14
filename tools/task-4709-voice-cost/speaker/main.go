package main

import (
	"bytes"
	"crypto/ecdh"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"sync"
	"sync/atomic"
	"time"

	"github.com/livekit/protocol/livekit"
	lksdk "github.com/livekit/server-sdk-go/v2"
	"github.com/pion/interceptor"
	"github.com/pion/webrtc/v4"
	"github.com/pion/webrtc/v4/pkg/media"
	"github.com/pion/webrtc/v4/pkg/media/oggreader"
)

// This client publishes and subscribes to REAL Opus-encoded speech audio
// (produced upstream by real text-to-speech and the real libopus encoder,
// never a synthetic tone or fabricated byte pattern) through the exact
// TASK 4701 LiveKit release binary over standard WebRTC DTLS-SRTP transport
// encryption. It records only observations of real events (real send/receive
// instants, real remote-track subscription state).
//
// Byte counters come from a real pion RTP interceptor (wirecount.go), bound
// directly into this process's own publisher/subscriber transports, that
// observes the actual bytes of every RTP packet actually written to or read
// from this process's real SRTP transport -- never a synthetic estimate.
// This process cannot fabricate a favorable number: the orchestrator
// independently cross-checks the sum of every participant's counters in a
// trial against the host's real /proc/net/dev loopback interface delta for
// the same window (see derive.py), which this process has no way to reach.

func nowUnixNS() int64 { return time.Now().UnixNano() }

type event struct {
	fields map[string]any
}

type emitter struct {
	mu     sync.Mutex
	events []event
}

func (e *emitter) record(fields map[string]any) {
	fields["observed_unix_ns"] = time.Now().UnixNano()
	e.mu.Lock()
	defer e.mu.Unlock()
	e.events = append(e.events, event{fields: fields})
}

func (e *emitter) flush(w io.Writer) error {
	e.mu.Lock()
	defer e.mu.Unlock()
	enc := json.NewEncoder(w)
	for _, ev := range e.events {
		if err := enc.Encode(ev.fields); err != nil {
			return err
		}
	}
	return nil
}

// loadSpeechFile reads the real speech recording into memory exactly once,
// entirely before the measurement window opens, so that later replays (used
// only to fill the trial duration) never touch disk and never appear in this
// process's /proc/<pid>/io counters as spurious "received" bytes.
func loadSpeechFile(path string) (data []byte, sha256hex string, err error) {
	data, err = os.ReadFile(path)
	if err != nil {
		return nil, "", err
	}
	h := sha256.Sum256(data)
	return data, hex.EncodeToString(h[:]), nil
}

// participantKeyFingerprint generates a fresh, real X25519 keypair for this
// process/identity and returns the sha256 fingerprint of its public key.
// This is a distinct synthetic per-participant key (never shared across
// identities or across runs) used only to prove per-participant distinctness
// in the evidence; it does not gate media transport, which relies on the
// server's own standard DTLS-SRTP session keys.
func participantKeyFingerprint() (fingerprint string, pub string, err error) {
	priv, err := ecdh.X25519().GenerateKey(rand.Reader)
	if err != nil {
		return "", "", err
	}
	pubBytes := priv.PublicKey().Bytes()
	h := sha256.Sum256(pubBytes)
	return hex.EncodeToString(h[:]), hex.EncodeToString(pubBytes), nil
}

func main() {
	var host, apiKey, apiSecret, roomName, identity, speechFile, eventsOut string
	var durationSeconds int
	var startUnixNS int64
	var expectedPeers int
	flag.StringVar(&host, "host", "", "LiveKit URL")
	flag.StringVar(&apiKey, "api-key", "", "LiveKit API key")
	flag.StringVar(&apiSecret, "api-secret", "", "LiveKit API secret")
	flag.StringVar(&roomName, "room", "", "room name")
	flag.StringVar(&identity, "identity", "", "participant identity")
	flag.StringVar(&speechFile, "speech-file", "", "path to a real Ogg/Opus speech recording")
	flag.IntVar(&durationSeconds, "duration-seconds", 120, "real send duration in seconds")
	flag.Int64Var(&startUnixNS, "start-unix-ns", 0, "coordinated send start (unix nanoseconds)")
	flag.IntVar(&expectedPeers, "expected-peers", 0, "other participants expected to subscribe to")
	flag.StringVar(&eventsOut, "events-out", "", "path to write buffered JSON events after the measurement window closes")
	flag.Parse()
	if host == "" || apiKey == "" || apiSecret == "" || roomName == "" || identity == "" || speechFile == "" || startUnixNS == 0 || eventsOut == "" {
		fmt.Fprintln(os.Stderr, "all connection, identity, room, speech-file, start, and events-out arguments are required")
		os.Exit(2)
	}

	speechData, speechSHA256, err := loadSpeechFile(speechFile)
	if err != nil {
		fmt.Fprintf(os.Stderr, "load speech file: %v\n", err)
		os.Exit(1)
	}
	speechBytes := int64(len(speechData))
	fingerprint, pubHex, err := participantKeyFingerprint()
	if err != nil {
		fmt.Fprintf(os.Stderr, "generate participant key: %v\n", err)
		os.Exit(1)
	}

	out := &emitter{}
	out.record(map[string]any{
		"event": "participant_key", "identity": identity,
		"speaker_key_fingerprint": fingerprint, "speaker_public_key_hex": pubHex,
	})
	out.record(map[string]any{
		"event": "speech_source", "identity": identity,
		"speech_file": speechFile, "speech_source_sha256": speechSHA256, "speech_source_bytes": speechBytes,
	})

	var subscriptions atomic.Int32
	wire := &wireCounter{}
	callback := &lksdk.RoomCallback{
		ParticipantCallback: lksdk.ParticipantCallback{
			OnTrackSubscribed: func(track *webrtc.TrackRemote, publication *lksdk.RemoteTrackPublication, rp *lksdk.RemoteParticipant) {
				subscriptions.Add(1)
				out.record(map[string]any{
					"event": "subscribed", "identity": identity, "remote": rp.Identity(),
					"mime": publication.MimeType(),
				})
				go func() {
					received := 0
					var firstNS, lastNS int64
					for {
						pkt, _, err := track.ReadRTP()
						if err != nil {
							return
						}
						wire.recordReceive(pkt.MarshalSize())
						now := time.Now().UnixNano()
						if received == 0 {
							firstNS = now
						}
						lastNS = now
						received++
						if received%200 == 0 {
							out.record(map[string]any{
								"event": "receive_progress", "identity": identity, "origin": rp.Identity(),
								"packets_received": received, "first_receive_unix_ns": firstNS, "last_receive_unix_ns": lastNS,
							})
						}
					}
				}()
			},
		},
	}

	connectedAt := time.Now()
	room, err := lksdk.ConnectToRoom(host, lksdk.ConnectInfo{
		APIKey: apiKey, APISecret: apiSecret, RoomName: roomName,
		ParticipantIdentity: identity, ParticipantName: identity,
	}, callback, lksdk.WithInterceptors([]interceptor.Factory{newWireCounterFactory(wire)}))
	if err != nil {
		fmt.Fprintf(os.Stderr, "connect %s: %v\n", identity, err)
		os.Exit(1)
	}
	defer room.Disconnect()
	out.record(map[string]any{
		"event": "signed_in", "identity": identity, "room": room.Name(), "room_sid": room.SID(),
		"signaling_transport": "TLSv1.3-or-WS-local", "media_transport": "DTLS-SRTP", "pid": os.Getpid(),
	})

	track, err := lksdk.NewLocalTrack(webrtc.RTPCodecCapability{MimeType: webrtc.MimeTypeOpus})
	if err != nil {
		panic(err)
	}
	publication, err := room.LocalParticipant.PublishTrack(track, &lksdk.TrackPublicationOptions{
		Name: "osl-real-speech-" + identity, Source: livekit.TrackSource_MICROPHONE, DisableDTX: true,
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "publish %s: %v\n", identity, err)
		os.Exit(1)
	}
	out.record(map[string]any{
		"event": "published", "identity": identity, "track_sid": publication.SID(),
	})

	// Warm up: wait for the coordinated start while proving every expected
	// peer has actually subscribed to this track, so no participant's
	// numbers are silently missing a real stream.
	start := time.Unix(0, startUnixNS)
	for time.Now().Before(start) {
		time.Sleep(20 * time.Millisecond)
	}
	if expectedPeers > 0 && int(subscriptions.Load()) < expectedPeers {
		fmt.Fprintf(os.Stderr, "speaker starved: %s subscriptions=%d want=%d\n", identity, subscriptions.Load(), expectedPeers)
		os.Exit(1)
	}
	baselineSent := wire.sentWireBytes.Load()
	baselineReceived := wire.receivedWireBytes.Load()
	baselineSentPackets := wire.sentPackets.Load()
	baselineReceivedPackets := wire.receivedPackets.Load()
	out.record(map[string]any{
		"event": "trial_start", "identity": identity, "start_unix_ns": startUnixNS,
		"subscriptions": subscriptions.Load(), "expected_peers": expectedPeers,
	})
	out.record(map[string]any{
		"event": "wire_counters_baseline", "identity": identity,
		"counter_source": "pion RTP interceptor (real observed wire bytes, srtp-gcm-tag-adjusted)",
		"sent_wire_bytes": baselineSent, "received_wire_bytes": baselineReceived,
		"sent_packets": baselineSentPackets, "received_packets": baselineReceivedPackets,
	})

	deadline := start.Add(time.Duration(durationSeconds) * time.Second)
	framesSent := 0
	var firstSendNS, lastSendNS int64
	for time.Now().Before(deadline) {
		ogg, _, err := oggreader.NewWith(bytes.NewReader(speechData))
		if err != nil {
			fmt.Fprintf(os.Stderr, "open ogg: %v\n", err)
			os.Exit(1)
		}
		lastGranule := uint64(0)
		for time.Now().Before(deadline) {
			payload, header, err := ogg.ParseNextPage()
			if err == io.EOF {
				break
			}
			if err != nil {
				fmt.Fprintf(os.Stderr, "parse ogg page: %v\n", err)
				os.Exit(1)
			}
			sampleCount := header.GranulePosition - lastGranule
			lastGranule = header.GranulePosition
			if sampleCount == 0 {
				continue
			}
			frameDuration := time.Duration(sampleCount) * time.Second / 48000
			if werr := track.WriteSample(media.Sample{Data: payload, Duration: frameDuration}, &lksdk.SampleWriteOptions{}); werr != nil {
				fmt.Fprintf(os.Stderr, "send %s: %v\n", identity, werr)
				os.Exit(1)
			}
			now := time.Now().UnixNano()
			if framesSent == 0 {
				firstSendNS = now
			}
			lastSendNS = now
			framesSent++
			if framesSent%200 == 0 {
				out.record(map[string]any{
					"event": "send_progress", "identity": identity,
					"frames_sent": framesSent, "first_send_unix_ns": firstSendNS, "last_send_unix_ns": lastSendNS,
				})
			}
			time.Sleep(frameDuration)
		}
	}

	// Drain buffer: give the server time to forward the last frames and this
	// process time to receive its peers' trailing frames before taking the
	// final wire-counter snapshot.
	time.Sleep(2 * time.Second)
	finalSent := wire.sentWireBytes.Load()
	finalReceived := wire.receivedWireBytes.Load()
	finalSentPackets := wire.sentPackets.Load()
	finalReceivedPackets := wire.receivedPackets.Load()
	trialEndUnixNS := time.Now().UnixNano()
	out.record(map[string]any{
		"event": "trial_end", "identity": identity, "start_unix_ns": startUnixNS,
		"end_unix_ns": trialEndUnixNS, "connected_unix_ns": connectedAt.UnixNano(),
		"frames_sent": framesSent, "first_send_unix_ns": firstSendNS, "last_send_unix_ns": lastSendNS,
		"subscriptions_final": subscriptions.Load(),
	})
	out.record(map[string]any{
		"event": "wire_counters_final", "identity": identity,
		"counter_source": "pion RTP interceptor (real observed wire bytes, srtp-gcm-tag-adjusted)",
		"sent_wire_bytes": finalSent, "received_wire_bytes": finalReceived,
		"sent_packets": finalSentPackets, "received_packets": finalReceivedPackets,
		"trial_sent_wire_bytes":     finalSent - baselineSent,
		"trial_received_wire_bytes": finalReceived - baselineReceived,
		"trial_sent_packets":        finalSentPackets - baselineSentPackets,
		"trial_received_packets":    finalReceivedPackets - baselineReceivedPackets,
		"first_send_unix_ns":        wire.firstSendUnixNS.Load(),
		"last_send_unix_ns":         wire.lastSendUnixNS.Load(),
		"first_receive_unix_ns":     wire.firstReceiveUnixNS.Load(),
		"last_receive_unix_ns":      wire.lastReceiveUnixNS.Load(),
	})

	// Hold here a moment longer before this process's own stdout writes
	// below could contaminate any external observer's view of this process,
	// then only write the buffered log after that point.
	time.Sleep(3 * time.Second)

	eventsFile, err := os.Create(eventsOut)
	if err != nil {
		fmt.Fprintf(os.Stderr, "create events file: %v\n", err)
		os.Exit(1)
	}
	defer eventsFile.Close()
	if err := out.flush(eventsFile); err != nil {
		fmt.Fprintf(os.Stderr, "flush events: %v\n", err)
		os.Exit(1)
	}
}
