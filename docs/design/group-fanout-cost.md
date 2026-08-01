# Group fan-out cost at scale

Status: measured from the current storage contract and implementation on 2026-08-01.

## Result

Sender keys make one content-encryption operation serve a group, but they do
not change transport fan-out.  With the proposed maximum of five recipient
devices per member, a group message creates **five independent payload blobs
per member**, each with its own capability and delivery tag.  It also creates
one shared, `multi-fetch` manifest.  The numbers below include every recipient
device, as required by D17; they do not subtract the sending member.  This is
the convention used by the 100-member / 500-blob planning example.

| members (N) | recipient devices (5N) | payload blobs | total PUTs (payload + manifest) | delivery tags written | maximum uploaded object bytes* |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 5 | 25 | 25 | 26 | 25 | 1,703,936 B (1.625 MiB) |
| 20 | 100 | 100 | 101 | 100 | 6,619,136 B (6.3125 MiB) |
| 100 | 500 | 500 | 501 | 500 | 32,833,536 B (31.3125 MiB) |
| 500 | 2,500 | 2,500 | 2,501 | 2,500 | 163,905,536 B (156.3125 MiB) |

\*This is the current 64 KiB maximum for every payload object and the
manifest: `(5N + 1) * 65,536`.  It is a capacity bound, not a claim that a
normal text message uses 64 KiB.  For a Padme-valid payload length `L` and a
manifest length `M`, the actual upload body bytes are `5N * L + M`, where
`M <= 65,536`.  HTTP headers, the upload grant, and responses are deliberately
not included: the grant is variable-length and no final request wire profile
exists yet, so reporting a made-up fixed header budget would not be a real
measurement.

The sender issues `5N + 1` authenticated PUTs.  Eager recipients issue `5N`
payload GETs.  Each target device also needs its sealed entry from the shared
manifest, so the conservative end-to-end download cost is `5N * (L + M)` when
every device fetches the manifest itself.  If device synchronization later
allows one account-level manifest fetch, that optimization must be specified
and measured separately; it cannot be assumed by the per-device delivery
contract.

## Delivery-tag window

Each payload PUT carries one distinct 16-byte `delivery_tag`; the shared
manifest does not need a delivery tag because it is reached from the carrier
pointer.  Thus each group message writes `5N` tags, shown in the table above
(400 B, 1,600 B, 8,000 B, and 40,000 B of raw tag values respectively).

The subscription window has two separate dimensions.  Let `W` be the number
of future tags a receiver can honestly derive per sender-device ratchet.  `W`
is not set in the contract and is explicitly awaiting T19-E6.  For one
recipient device in a group of N members, the required upper-bound subscription
set is `5 * (N - 1) * W` tags: up to five sending devices for every other
member.  The corresponding raw tag bytes are that count times 16.

| members (N) | tags per recipient device (`5(N-1)W`) | raw bytes per recipient device | tags across one member's 5 devices (`25(N-1)W`) |
| ---: | ---: | ---: | ---: |
| 5 | 20W | 320W B | 100W |
| 20 | 95W | 1,520W B | 475W |
| 100 | 495W | 7,920W B | 2,475W |
| 500 | 2,495W | 39,920W B | 12,475W |

The transport contract currently says only “a window of upcoming tags,” not
its width or serialized subscription frame.  A numeric window byte budget
must therefore wait for T19-E6 and T1-55; choosing one here would turn an open
protocol parameter into a false measurement.  At `W = 1`, the last row is
2,495 tags / 39,920 raw bytes per receiving device before framing.

## Periodic sender-key distribution

The sender-key distribution message is separate from blob storage.  It is
emitted over the v=3 bundle path at most once per five-minute self-heal interval
on a sender's next outbound group message (and on rotation).  It scales with
the recipient list in that bundle, not with the `5N` blob PUTs above.  It must
be budgeted as a periodic control-plane cost, not hidden in the payload-blob
number.  The current wire includes the scope key, a chain id, a 32-byte rotation
root, a 32-byte physical-device id, and a timestamp; its final encrypted byte
size depends on the scope identifier and recipient prekey bundle and is not a
constant in the implementation.

## Decision implication

The dominant cost is storage and transport fan-out, not group content
encryption.  A 100-member room at the proposed five-device maximum reaches 501
PUTs and up to 31.3125 MiB of upload bodies for one maximum-sized message;
500 members reaches 2,501 PUTs and up to 156.3125 MiB.  Any group-member cap
must be selected against these figures and the resolved delivery-tag window,
then enforced before a send starts.  Sender keys are not a reason to remove
that cap.

## Inputs verified

- `features.md` §4.4 establishes one blob per `(message, recipient device)`.
- `storage.md` §5 makes copies independent per recipient device and records
  the five-device proposal in OPEN-3.
- `storage.md` §2 and §7b define a 16-byte delivery tag on each payload PUT;
  its subscription window intentionally has no fixed width yet.
- `storage.md` §2 marks the shared manifest `multi-fetch`; T1-37 defines one
  cover pointer to that manifest with a sealed entry per recipient.
- `cipher-store-cf/src/endpoints/blob.ts` and
  `crates/ipc/src/cipher_store_client.rs` currently cap an object at 64 KiB.
- `crates/ipc/src/commands.rs` defines the five-minute SKDM periodic emission
  interval and sends it through `send_skdm_via_v3_bundle`.
