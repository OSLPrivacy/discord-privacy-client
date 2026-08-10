# Carrier reference harness (Task 5102)

This isolated tool implements the shared fail-closed state machine for Windows
carrier reference capture. A platform backend must return two matching UIA and
window observations for one expected HWND, then accept capture requests only
for the exact UIA composer/message-row bounds plus a four-physical-pixel seam
ring. The backend returns two matching RGB/RGBA frames bound to the same HWND
generation. Only then does the harness encode one lossless bounded PNG and its
non-content manifest.

There is deliberately no desktop-capture or arbitrary-crop API. Raw frame
storage is zeroized before output files are opened and again on every drop path.
The executable is a deterministic synthetic-account fixture used to exercise
success and refusal paths on any build host. A Windows collector can implement
`CarrierBackend` with UI Automation plus Windows Graphics Capture (preferred)
or tightly bounded visible-screen capture, while signature, foreground,
occlusion, generation, and geometry checks remain mandatory inputs to the
shared state machine.

Release baselines use a second, deliberately separate state machine. Captures
first enter a content-addressed `candidates/` set. Only an Ed25519 signature by
a distinct reviewer authorized in the configured release-review trust root can
copy that exact set into `baselines/objects/` and atomically advance its named
carrier/channel/state pointer. Version is signed into each manifest so a new
carrier version remains in the same parent-linked history. The signed release
record binds the parent hash, old/new content identity, capture author,
reviewer, and trust-root key id. Runtime adaptation, capture authors, and
passing diffs have no baseline write path; a stale parent also makes replayed
acceptance fail closed.
