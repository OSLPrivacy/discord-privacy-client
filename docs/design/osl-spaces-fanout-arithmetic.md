# Space fan-out arithmetic against the frozen storage contract

Status: **contract finding for T6 and T1; not a change to the frozen storage
contract and not a shipped Spaces capability.** Verified 2026-08-01 against
`storage.md`, owner decisions D16/D17/D29, and the live cipher-store limits.

## Verdict

The current contract cannot safely carry Space write-time fan-out at community
scale. It fails first on **live row count**, not payload bytes. This is a P0
contract finding: Space and direct-message copies share the one UNDELIVERED
pool, and a full pool rejects every subsequent PUT with `503 storage_capacity`.
It does not evict an undelivered message (D16).

## Inputs and calculation

- D17 requires one independent payload copy per recipient device. The proposed
  storage-contract device cap is five per member.
- A Space message also has one `multi-fetch` manifest blob. It remains until
  burn or TTL; eager fetch does not remove it.
- The live generic-store admission limits are 100,000 rows and 2 GiB. An
  ordinary blob is at most 64 KiB.
- D29 eager fetch clears an online recipient's payload promptly, but an
  offline recipient's payload remains in UNDELIVERED for the fixed seven-day
  TTL.

For `N` members at `D = 5` devices each:

```
rows per message = N × D + 1 manifest
messages before row cap = floor(100,000 / rows per message)
```

| Space members | Recipient devices | Rows / message | Whole messages before global row cap |
| ---: | ---: | ---: | ---: |
| 5 | 25 | 26 | 3,846 |
| 20 | 100 | 101 | 990 |
| 100 | 500 | 501 | 199 |
| 500 | 2,500 | 2,501 | 39 |

Thus a 100-member Space fills the **service-wide** row budget after 199
messages in flight. That is not a per-Space or per-sender allowance.

Bytes disguise the problem. A 200-byte text message becomes about 256 bytes
after Padmé padding; 501 copies are about 128 KiB. The 2 GiB byte budget would
therefore allow roughly 16,000 such messages, so the row budget binds about
80 times earlier. The 64 KiB object limit is also an independent ceiling: at
about 64 bytes per sealed manifest entry, a manifest reaches that ceiling near
1,000 recipient devices (about 200 members at five devices).

## Offline case and blast radius

Consider one 100-member Space with 30 members offline for one day while 200
messages are sent:

```
30 offline members × 5 devices × 200 messages = 30,000 undelivered rows
```

One deliberately modest Space therefore consumes 30% of the global row
budget. Three such Spaces fill it. Eager fetch improves the online case but
does not alter this offline arithmetic.

UNDELIVERED and RETAINED are separate pools, but the frozen contract has no
separate Space and direct-message UNDELIVERED pools. Consequently, a busy
Space can make a direct-message upload return `503 storage_capacity` anywhere
in the service. The client must queue and retry rather than discard the
undelivered message, so the failure is product-wide sending stall until rows
drain, not an acceptable lossy fallback.

Every row also requires an upload grant. A 100-member message therefore mints
501 single-use grants within their 600-second lifetime. The contract must say
whether tier policy is an issuance rate or a total allowance; otherwise even a
first Space message may exceed a Free-tier budget.

## Decision required from T6 and T1

T21 selects no storage design. T6 must decide, and record, one or more of:

1. Raise `MAX_LIVE_BLOB_ROWS`, derived from member count, devices, expected
   offline fraction, and in-flight duration rather than a round number.
2. Give Space fan-out a separate pool so it cannot consume direct-message
   UNDELIVERED capacity. This preserves D16 structurally.
3. Change to per-member read-time fan-out: one stored ciphertext plus a
   manifest. This removes the row multiplication, but breaks the frozen
   per-blob single-fetch and per-member burn/view-once properties.
4. Cap Space size to the measured capacity and refuse new joins or sends above
   it. This is the necessary honest fallback if the first two changes do not
   land.

Recommendation: **2 + 4**. A separate Space pool removes the direct-message
blast radius; an enforced, measured cap keeps the remaining capacity claim
honest. No numeric product cap is set by this note.

## Sources

- Owner decisions D16, D17, and D29 in `plan/09-DECISIONS.md`.
- `plan/03-CONTRACTS/storage.md` §§1, 2, 5, 6, and 7.
- `cipher-store-cf/src/lib/blob-limits.ts` and
  `cipher-store-cf/src/endpoints/blob.ts`.
- T1-37's one-manifest / per-recipient sealed-entry design.
