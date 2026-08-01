# Spaces manifest ceiling and decryption cost

Status: **contract finding for T1; not a Spaces implementation or a product
member limit.**

## Verified constraint

T1's frozen `transport.md` defines the group manifest as an ordinary padded
blob containing one entry per recipient device.  The blob lane is capped at
64 KiB (65,536 bytes); Padme padding does not raise that cap.  The current
checkout does **not** contain the plan's cited
`cipher-store-cf/src/endpoints/blob.ts`; consequently this finding relies on
the frozen transport and storage contracts, not on a nonexistent implementation
line.

The same contract describes an entry as approximately 48--64 bytes.  At the
largest stated entry size, even an imaginary manifest with zero framing bytes
can contain only:

```text
floor(65,536 / 64) = 1,024 recipient-device entries
```

Real framing has a positive size, so the real maximum is strictly below 1,024.
For five devices per member, the absolute upper bound is 204 members, and a
roughly-200-member Space already consumes about 1,000 entries.  This is a
**ceiling**, not an approved membership cap: the actual cap must also leave
headroom for manifest framing, the measured sealed-entry encoding, padding, and
the relay's row, byte, grant, and offline-member budgets from T21-B1/K4.

## Contract vector

```json
{
  "version": 1,
  "blobCapBytes": 65536,
  "maxSealedEntryBytes": 64,
  "minimumFramingBytes": 1,
  "assumedDevicesPerMember": 5,
  "zeroFramingDeviceUpperBound": 1024,
  "positiveFramingDeviceUpperBound": 1023,
  "approximateMembersAtFiveDevices": 200,
  "requiresBoundedEntrySelection": true
}
```

The vector deliberately uses one byte as the lower bound on framing.  It does
not claim a wire format: K4 must replace it with a measured format overhead
and encoded entry size before G9 can set the one enforced join limit.

## The decryption cost

The manifest contract says that every entry is sealed to one recipient, but it
does not provide a recipient-entry selector.  With `N` opaque sealed entries,
a recipient has no way to identify their entry before trying it.  The inbound
path is therefore `O(N)` AEAD opens in the worst case: a valid entry near the
end costs nearly `N` opens, and a malicious or malformed manifest costs `N`
failed opens.

At the blob ceiling this is up to 1,023 trial decryptions in a framed manifest,
not the bounded 7 opens used by `osl-ratchet-next` header decryption.  That is
a receiver-controlled DoS surface of the same kind as T18-B2's `decrypt_from`
finding.  It must not be hidden behind an optimistic average or treated as an
implementation detail.

## Required T1 resolution

T1 must add a manifest-entry selection rule before Spaces fan-out is built:

1. A recipient must select at most a small, explicit constant number of
   candidate entries before attempting AEAD open.  The constant and refusal
   behaviour for a manifest exceeding it belong in the frozen transport
   contract and its tests.
2. The selector must be authenticated as part of the sealed entry and must not
   give the relay a stable account, device, Space, or membership index.  The
   relay stores an opaque blob and must not gain a roster by parsing or indexing
   manifest entries.
3. The entry encoding and manifest framing must be measured at 5, 20, and 100
   members in T21-K4.  That measurement replaces the 48--64-byte planning
   range, derives the actual member ceiling, and feeds the single G9 join cap.

Until that resolution and measurement land, Spaces must not claim a numeric
member limit or accept a design that performs unbounded trial decryption.

## Sources

- `03-CONTRACTS/transport.md` §1 and §1b: 64 KiB blob lane; ordinary padded
  manifest; one sealed entry per recipient device.
- `03-CONTRACTS/storage.md` §2b: Padme preserves the 65,536-byte ceiling.
- `crates/osl-ratchet-next/src/session.rs`: current bounded header trial
  decryption is a useful contrast, not an implementation for manifests.
- Owner decisions D16, D17, D29, and D66: do not evict undelivered messages;
  per-device copies; eager fetch; and Spaces in v1.
