# OSL Simple Spec

This file is the short user-facing product contract. It deliberately avoids
internal machinery and names only the promises OSL can make honestly.

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
5. **Honest receipt:** OSL reports what it actually verified. A cleanup request
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
