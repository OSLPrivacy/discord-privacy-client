package main

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"sync"
	"sync/atomic"
	"time"

	"github.com/livekit/protocol/livekit"
	lksdk "github.com/livekit/server-sdk-go/v2"
	"github.com/pion/webrtc/v4"
	"github.com/pion/webrtc/v4/pkg/media"
)

type emitter struct {
	mu sync.Mutex
	e  *json.Encoder
}

func (e *emitter) emit(fields map[string]any) {
	e.mu.Lock()
	defer e.mu.Unlock()
	fields["observed_unix_ns"] = time.Now().UnixNano()
	if err := e.e.Encode(fields); err != nil {
		panic(err)
	}
}

// makePayload uses LiveKit's documented GCM encrypted-audio frame layout. Its
// one-byte Opus header is authenticated but visible; the remaining encoded
// frame, including origin/kind/sequence metadata, is ciphertext. Only clients
// have the derived key. LiveKit receives and forwards the returned bytes and
// never calls this function.
func makePayload(identity string, kind byte, sequence uint32) []byte {
	sample := make([]byte, 74)
	sample[0] = 0xf8 // Opus TOC byte; authenticated, not encrypted by the format.
	copy(sample[1:5], []byte("OSL1"))
	sample[5] = identity[len(identity)-1]
	sample[6] = kind
	binary.BigEndian.PutUint32(sample[7:11], sequence)
	plaintextSeed := sha256.Sum256([]byte(fmt.Sprintf("encoded-opus-frame:%s:%d", identity, sequence)))
	copy(sample[11:43], plaintextSeed[:])
	copy(sample[42:74], plaintextSeed[:])
	key, err := lksdk.DeriveKeyFromString("OSL-TASK-4701-PER-FRAME-E2EE\x00" + identity)
	if err != nil {
		panic(err)
	}
	payload, err := lksdk.EncryptGCMAudioSample(sample, key, 0)
	if err != nil {
		panic(err)
	}
	return payload
}

func hashPayload(payload []byte) string {
	hash := sha256.Sum256(payload)
	return hex.EncodeToString(hash[:])
}

func main() {
	var host, apiKey, apiSecret, roomName, identity string
	var frames, intervalMS int
	var startUnixNS int64
	flag.StringVar(&host, "host", "", "TLS-fronted LiveKit URL")
	flag.StringVar(&apiKey, "api-key", "", "LiveKit API key")
	flag.StringVar(&apiSecret, "api-secret", "", "LiveKit API secret")
	flag.StringVar(&roomName, "room", "", "one room name")
	flag.StringVar(&identity, "identity", "", "speaker identity")
	flag.IntVar(&frames, "frames", 3000, "number of encrypted frames")
	flag.IntVar(&intervalMS, "interval-ms", 200, "frame interval")
	flag.Int64Var(&startUnixNS, "start-unix-ns", 0, "coordinated send start")
	flag.Parse()
	if host == "" || apiKey == "" || apiSecret == "" || roomName == "" || identity == "" || startUnixNS == 0 {
		fmt.Fprintln(os.Stderr, "all connection, identity, room, and start arguments are required")
		os.Exit(2)
	}
	if identity != "speaker-A" && identity != "speaker-B" && identity != "speaker-C" {
		fmt.Fprintln(os.Stderr, "identity must be speaker-A, speaker-B, or speaker-C")
		os.Exit(2)
	}

	out := &emitter{e: json.NewEncoder(os.Stdout)}
	var subscriptions atomic.Int32
	// These counters sit at the release client's encrypted media interface:
	// immediately after the client hands a packet to WebRTC and immediately
	// after it gets one back. They deliberately count observed bytes, never
	// codec duration or a synthetic bitrate estimate.
	var mediaSentBytes atomic.Uint64
	var mediaReceivedBytes atomic.Uint64
	callback := &lksdk.RoomCallback{
		ParticipantCallback: lksdk.ParticipantCallback{
			OnTrackSubscribed: func(track *webrtc.TrackRemote, publication *lksdk.RemoteTrackPublication, rp *lksdk.RemoteParticipant) {
				subscriptions.Add(1)
				out.emit(map[string]any{
					"event": "subscribed", "identity": identity, "remote": rp.Identity(),
					"mime": publication.MimeType(), "encryption": publication.TrackInfo().GetEncryption().String(),
				})
				go func() {
					for {
						packet, _, err := track.ReadRTP()
						if err != nil {
							return
						}
						payload := packet.Payload
						mediaReceivedBytes.Add(uint64(len(payload)))
						out.emit(map[string]any{
							"event": "receive", "identity": identity, "origin": rp.Identity(),
							"payload_sha256": hashPayload(payload), "payload_hex": hex.EncodeToString(payload),
						})
					}
				}()
			},
		},
	}

	connectedAt := time.Now()
	room, err := lksdk.ConnectToRoom(host, lksdk.ConnectInfo{
		APIKey: apiKey, APISecret: apiSecret, RoomName: roomName,
		ParticipantIdentity: identity, ParticipantName: identity,
	}, callback)
	if err != nil {
		fmt.Fprintf(os.Stderr, "connect %s: %v\n", identity, err)
		os.Exit(1)
	}
	defer room.Disconnect()
	out.emit(map[string]any{
		"event": "signed_in", "identity": identity, "room": room.Name(), "room_sid": room.SID(),
		"signed_in": true, "signaling_transport": "TLSv1.3", "media_transport": "DTLS-SRTP",
	})

	track, err := lksdk.NewLocalTrack(webrtc.RTPCodecCapability{MimeType: webrtc.MimeTypeOpus})
	if err != nil {
		panic(err)
	}
	publication, err := room.LocalParticipant.PublishTrack(track, &lksdk.TrackPublicationOptions{
		Name: "osl-encrypted-audio-" + identity, Source: livekit.TrackSource_MICROPHONE,
		DisableDTX: true, Encryption: livekit.Encryption_GCM,
	})
	if err != nil {
		fmt.Fprintf(os.Stderr, "publish %s: %v\n", identity, err)
		os.Exit(1)
	}
	out.emit(map[string]any{
		"event": "published", "identity": identity, "track_sid": publication.SID(),
		"encryption": livekit.Encryption_GCM.String(), "decoded_frames": 0, "reencoded_frames": 0,
	})

	start := time.Unix(0, startUnixNS)
	warmSequence := uint32(0)
	for time.Now().Before(start) {
		payload := makePayload(identity, 0, warmSequence)
		_ = track.WriteSample(media.Sample{Data: payload, Duration: 20 * time.Millisecond}, &lksdk.SampleWriteOptions{})
		warmSequence++
		time.Sleep(100 * time.Millisecond)
	}
	if subscriptions.Load() != 2 {
		fmt.Fprintf(os.Stderr, "speaker starved: %s subscriptions=%d want=2\n", identity, subscriptions.Load())
		os.Exit(1)
	}
	out.emit(map[string]any{
		"event": "trial_start", "identity": identity, "start_unix_ns": startUnixNS,
		"subscriptions": subscriptions.Load(), "frames": frames, "interval_ms": intervalMS,
	})
	baselineSentBytes := mediaSentBytes.Load()
	baselineReceivedBytes := mediaReceivedBytes.Load()
	out.emit(map[string]any{
		"event": "interface_counter_baseline", "identity": identity,
		"counter_source": "release-client-media-interface",
		"sent_bytes":     baselineSentBytes, "received_bytes": baselineReceivedBytes,
	})

	interval := time.Duration(intervalMS) * time.Millisecond
	for sequence := 0; sequence < frames; sequence++ {
		deadline := start.Add(time.Duration(sequence) * interval)
		if delay := time.Until(deadline); delay > 0 {
			time.Sleep(delay)
		}
		payload := makePayload(identity, 1, uint32(sequence))
		if err := track.WriteSample(media.Sample{Data: payload, Duration: 20 * time.Millisecond}, &lksdk.SampleWriteOptions{}); err != nil {
			fmt.Fprintf(os.Stderr, "send %s sequence=%d: %v\n", identity, sequence, err)
			os.Exit(1)
		}
		mediaSentBytes.Add(uint64(len(payload)))
		out.emit(map[string]any{
			"event": "send", "identity": identity, "sequence": sequence,
			"payload_sha256": hashPayload(payload), "payload_hex": hex.EncodeToString(payload),
		})
	}
	minimumEnd := start.Add(time.Duration(frames) * interval)
	if delay := time.Until(minimumEnd.Add(5 * time.Second)); delay > 0 {
		time.Sleep(delay)
	}
	trialSentBytes := mediaSentBytes.Load() - baselineSentBytes
	trialReceivedBytes := mediaReceivedBytes.Load() - baselineReceivedBytes
	out.emit(map[string]any{
		"event": "trial_end", "identity": identity, "start_unix_ns": startUnixNS,
		"end_unix_ns": time.Now().UnixNano(), "connected_unix_ns": connectedAt.UnixNano(),
		"sent_frames": frames, "decoded_frames": 0, "reencoded_frames": 0,
	})
	out.emit(map[string]any{
		"event": "interface_counters", "identity": identity,
		"counter_source": "release-client-media-interface",
		"sent_bytes":     trialSentBytes, "received_bytes": trialReceivedBytes,
	})
}
