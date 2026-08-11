# TASK 4701 forwarding-audio qualification

This directory pins and qualifies the forwarding server selected for OSL voice:
LiveKit Server `v1.13.5` (`3b9f118327b257301083a7c4aa46076c8012918a`).

The qualification publishes already encrypted, Opus-shaped RTP payloads from
three independent JWT-authenticated speaker processes.  LiveKit is treated as
an SFU: RTP headers may change, but the encrypted RTP payload must not.  Five
frames are sent per second for 600 seconds, producing exactly 3,000 frames per
speaker.

The release server tarball is pinned in `release-lock.json`.  `qualify.py`
downloads and verifies it, builds the pinned Go speaker, runs the ten-minute
trial, records process/configuration/transport/sink artifacts, and calls
`verify.py`.  `mutants.py` proves the same verifier rejects each forbidden
capability.

This is backend qualification only.  It does not enable or advertise voice in
the OSL client.

Run from the repository root:

```sh
python3 tools/task-4701-forwarding-audio/qualify.py
python3 tools/task-4701-forwarding-audio/mutants.py
```

All Cargo commands remain subject to the lane-wide
`CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/i` rule; this task's upstream server
and client harness are Go binaries and do not invoke Cargo.
