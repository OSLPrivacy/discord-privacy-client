# OSL Simple Spec

This file is the short user-facing product contract. It deliberately avoids
internal machinery and names only the promises OSL can make honestly.

## Sending

Sending has three honest outcomes: sent, not sent, or delivery uncertain. OSL
never treats delivery uncertain as sent, never auto-retries it, and never asks
the user to resend as if the first attempt certainly failed.

Double Enter, when selected, is still a two-step user send. The first distinct
user Enter encrypts and places the protected capsule only after OSL verifies the
destination. A second separate user Enter after key-up, after the same
destination is verified again, is the only action that can send. Key repeat, a
held key, a synthetic event or OSL's own placement action cannot satisfy the
second Enter. Timeout, focus loss, destination mismatch or restart cancels the
armed state and preserves the local draft; it never sends and never retries
automatically.

Behavioral acceptance test:
`docs/design/osl-simple-spec.md'`.

Drive the send controller with one verified success, one verified refusal and one
ambiguous post-handoff outcome. The UI and receipt model must report those as
sent, not sent and delivery uncertain respectively; the uncertain case must not
be counted as sent, must not enqueue an automatic retry and must not instruct the
user to resend as though the first attempt certainly failed.

With Double Enter selected, send authority is earned only by two distinct trusted
user Enter presses separated by key-up and by fresh destination verification
before both placement and final Send. The test must fail if key repeat, a held
key, a synthetic event, OSL's own placement action, timeout, focus loss,
destination mismatch or restart can complete Send or auto-retry. In every
cancellation path the local draft must remain available.

## Burn

Burn cleans up. Burn does not un-send.

Burn has five guarantees:

1. **Local cleanup:** OSL shreds its local stored ciphertext and nonce for the
   selected scope, marks the rows burned so sync cannot resurrect them, and drops
   cached attachments for those rows.
2. **OSL server cleanup:** OSL deletes server-side state that belongs to the
   selected OSL scope where that state exists.
3. **Cooperative peer request:** OSL may ask affected peers to clean up their OSL
   copies, but absence of consent, binding, authority, transport delivery or
   verification means the peer cleanup is refused or reported as unavailable.
4. **Exact scope:** A burn applies only to the reviewed and confirmed scope:
   current conversation, linked service account or active OSL identity. Changing
   the scope or options requires confirmation again.
5. **Honest result:** OSL reports what it actually verified. A cleanup request
   is never displayed as deletion, and unsupported or unverified work stays
   visibly unsupported or unverified.

Burn does not delete carrier messages from a native service, does not erase
provider retention, exports, backups, screenshots or already-read plaintext, and
does not destroy anyone's ability to decrypt retained carrier data. The shipping
message format seals to recipient long-term keys, so retained carrier content can
remain readable to anyone who has that key material.

Do not describe Burn as "cryptographic burn", "destroys keys, not messages",
"permanent ciphertext", "permanent gibberish", "mathematically opaque",
"disappears forever", "permanently undecryptable" or "gone for good". Those
phrases claim a deletion or undecryptability guarantee OSL does not currently
earn.
