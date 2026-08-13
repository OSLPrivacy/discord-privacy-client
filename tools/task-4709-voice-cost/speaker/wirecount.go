package main

import (
	"sync/atomic"

	"github.com/pion/interceptor"
	"github.com/pion/rtp"
)

// srtpGCMTagBytes is the fixed AEAD_AES_128_GCM authentication tag that SRTP
// appends to every encrypted RTP packet on the wire (no other growth for
// GCM, and LiveKit's release server negotiates GCM by default for the media
// path already proven in TASK 4701). On send, the pion interceptor below
// observes the real plaintext RTP packet (header + payload) immediately
// before it is handed to the SRTP writer. On receive, this participant's own
// real track.ReadRTP() call (main.go) observes the real plaintext RTP packet
// immediately after SRTP has decrypted it. Either way the real on-wire UDP
// payload size is that observed size plus this fixed tag.
//
// (The LiveKit Go SDK v2.8.2 only threads a caller-supplied interceptor list
// into the *publisher* PeerConnection -- engine.go always builds the
// subscriber PeerConnection with its own default interceptor set, so a
// custom interceptor's BindRemoteStream is never invoked. Receive-side
// counting is therefore done directly at this process's own ReadRTP() call
// site instead, which observes exactly the same real per-packet bytes.)
const srtpGCMTagBytes = 16

// wireCounter counts only bytes this process's own real WebRTC transport
// actually sent or received: send counts come from a real pion RTP
// interceptor bound to this process's publisher connection; receive counts
// come from this process's own real track.ReadRTP() calls. Neither path can
// produce a nonzero count without an actual RTP packet crossing this
// process's real transport.
type wireCounter struct {
	sentPackets        atomic.Uint64
	sentWireBytes      atomic.Uint64
	receivedPackets    atomic.Uint64
	receivedWireBytes  atomic.Uint64
	firstSendUnixNS    atomic.Int64
	lastSendUnixNS     atomic.Int64
	firstReceiveUnixNS atomic.Int64
	lastReceiveUnixNS  atomic.Int64
}

func (c *wireCounter) recordReceive(rtpPacketWireBytes int) {
	now := nowUnixNS()
	c.receivedWireBytes.Add(uint64(rtpPacketWireBytes + srtpGCMTagBytes))
	c.receivedPackets.Add(1)
	if c.firstReceiveUnixNS.Load() == 0 {
		c.firstReceiveUnixNS.Store(now)
	}
	c.lastReceiveUnixNS.Store(now)
}

type wireCounterFactory struct {
	counter *wireCounter
}

func newWireCounterFactory(counter *wireCounter) *wireCounterFactory {
	return &wireCounterFactory{counter: counter}
}

func (f *wireCounterFactory) NewInterceptor(_ string) (interceptor.Interceptor, error) {
	return &boundWireCounter{counter: f.counter}, nil
}

type boundWireCounter struct {
	interceptor.NoOp
	counter *wireCounter
}

func (b *boundWireCounter) BindLocalStream(_ *interceptor.StreamInfo, writer interceptor.RTPWriter) interceptor.RTPWriter {
	return interceptor.RTPWriterFunc(func(header *rtp.Header, payload []byte, attributes interceptor.Attributes) (int, error) {
		n, err := writer.Write(header, payload, attributes)
		if err == nil {
			now := nowUnixNS()
			wire := uint64(header.MarshalSize() + len(payload) + srtpGCMTagBytes)
			b.counter.sentWireBytes.Add(wire)
			b.counter.sentPackets.Add(1)
			if b.counter.firstSendUnixNS.Load() == 0 {
				b.counter.firstSendUnixNS.Store(now)
			}
			b.counter.lastSendUnixNS.Store(now)
		}
		return n, err
	})
}
