# Step files

A step file is a JSON array of `{ id, verb, args }`. `vmqa-run.sh run --steps <file>` wraps it in a
strict V2 request envelope (`schemaVersion`, `runId`, `identifier`, `runStartUtc`, executable
SHA-256, build-identity SHA-256, the closed build identity, and `steps`) and drops it on the share.
Unknown fields and every schema version other than 2 are refused. `run` and `selftest` require a
retained `build-identity.json` and the exact local executable path so the host measures the bytes
independently instead of trusting caller-provided digests. They also require the retained
exact-build evidence directory (`source.tar`, dist archive/manifest, raw npm/Cargo logs, and the
closed build log) whose hashes are part of the identity and are copied beside each run. The caller
must also supply the independently selected full expected commit and tree to build, stamp, run,
selftest, and cleanup verification; the embedded identity cannot choose its own trust root.

## Why there is only one self-test step file

`selftest.json` is used by **both halves** of `vmqa-run.sh selftest` — the positive run and the
negative control. The two differ in exactly one field, the `identifier` in the envelope, and in
nothing else.

That is deliberate. A negative control written as a second step file drifts from the positive one,
and once it has drifted you are no longer testing what you think you are: the two runs differ in
several ways and any difference in outcome can be attributed to the wrong one. Holding the steps
byte-identical makes "these runs differ only in the subject" structurally true instead of a claim
in a comment.

## What the self-test actually proves

| Half | `identifier` | Required verdict |
|---|---|---|
| positive | the real bundle identifier | `pass` |
| negative | `org.oslprivacy.doesnotexist` | `blocked` |

Passing the pair requires three assertions, not two:

1. the positive half is `pass`;
2. the negative half is `blocked`;
3. **the positive half measured the launched process's real surface** — `markerWindowsTotal >= 1`
   from `ping`, `distinctColors >= 16` from `shot`, and structured shot facts proving the surface
   PID equals the launch PID, is at least 200×120, remained foreground with a stable rectangle,
   owned the sample grid, and had no overlapping window above it before and after capture.

The third is the one that is easy to leave out and the one that matters. Without it, a rig where
nothing is running at all produces "found no marker window" on *both* halves, the negative control
appears to work, and the whole self-test reports success while measuring nothing. "Correctly denied"
and "the apparatus is broken" are indistinguishable unless you separately prove the apparatus works.
An all-black screenshot clears a pixel-count check but not a distinct-colour floor. A colourful
whole-desktop screenshot is not enough either: one live run passed on Firefox pixels while the only
OSL HWND it had bound was the 6×6 single-instance marker. `shot` therefore captures only the unique
visible non-marker top-level surface owned by the launched PID and emits the structured facts the
host gate requires.

A negative control that comes back `pass` is not a test failure, it is an **invalid harness**, and
`vmqa-run.sh` exits `9` for it rather than `1`. A harness that confirms whatever it happens to find
is worse than no harness, because its greens get believed.

## Verb reference

| Verb | Args | Notes |
|---|---|---|
| `ping` | — | agent liveness; reports session id and `markerWindowsTotal` |
| `stage` | `exeSha256` | copies exe **and `WebView2Loader.dll`** to `C:\OSL-VMQA\<sha>\` |
| `launch` | `exeSha256`, `timeoutSeconds` | waits for the marker window; does not reposition it |
| `shot` | `name` | exact-PID visible-surface capture + pre/post binding and occlusion proof |
| `click` | `winX`, `winY`, `settleMs` | window-relative; bounds-checked against the subject's rect |
| `type` | `text`, `settleMs` | |
| `key` | `key`, `settleMs` | |
| `wait` | `ms` | |
| `kill` | — | stops only the resolved subject |

Every verb that touches the app resolves its target through `Resolve-VmqaSubject` on the marker
window class `<identifier>-sic`, and through nothing else. There is no verb that takes a pid, a
window title or a process name, because selecting by name has already driven the wrong lane's
application through six UI steps.
