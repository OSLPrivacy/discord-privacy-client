# CONTRACT — client integrity (T10-I1)

**Status:** FROZEN 2026-08-01. **Owner:** T10. **Consumers:** T10-I2 through T10-I8.

This contract implements D21's self-check, honest disclosure, build-hash handshake,
transparency record, and signed acknowledgements. OSL is pro-mod and pro-information:
hardware or TPM attestation is not part of this protocol. `09-DECISIONS.md` wins over
this contract if an owner decision conflicts with it. This contract consumes
`features.md` §6 and `transport.md`; it does not alter either contract.

## 1. Signed build-hash manifest

Each promoted `hub-latest` release publishes immutable `build-hashes.json` and its
detached signature `build-hashes.json.sig`. The signature covers the exact manifest
bytes and is verified with the existing OSL Hub updater minisign public key compiled
in `apps/osl-hub/tauri.conf.json` at `plugins.updater.pubkey` (key id
`3B6AE4739858E8D4`). The release workflow signs with the matching
`HUB_TAURI_SIGNING_PRIVATE_KEY`; no second key, endpoint, or trust root exists.

The UTF-8 JSON has this exact format-1 shape. Unknown format versions are invalid;
extra fields are ignored for forward compatibility.

```json
{
  "format": 1,
  "builds": [
    {
      "version": "1.2.3",
      "tag": "v1.2.3",
      "commit": "40-lowercase-hex-git-commit",
      "installer_sha256": "64-lowercase-hex",
      "exe_sha256": "64-lowercase-hex"
    }
  ]
}
```

`builds` contains no duplicate `exe_sha256`. `version` has no leading `v`; `tag` is
the immutable Git tag; `commit` is a full lowercase 40-hex Git commit; and both hash
fields are lowercase, unprefixed 64-hex SHA-256 digests. `installer_sha256` covers the
published installer; `exe_sha256` covers the PE executable inside it.

The client bundles the manifest and signature used to build it. It does not fetch the
manifest while running, and it never queries Rekor or another transparency service at
runtime. The release workflow publishes Sigstore/Rekor provenance for public audit;
the client trusts its bundled, signed list only. This avoids leaking which build a user
runs through a live lookup.

## 2. Local self-check and honest claim

At launch the client SHA-256 hashes `current_exe()`, verifies the bundled manifest,
and compares its digest to `exe_sha256`. Its local result is exactly:

| result | condition | required local disclosure |
| --- | --- | --- |
| `Verified` | valid signature and format; executable hash is listed | this copy matches a published OSL build hash |
| `Mismatch` | valid signature and format; executable hash is absent | this copy does not match a published OSL build hash |
| `Unknown` | manifest/signature absent, malformed, unsupported, or unverifiable | this copy could not be checked |

The check detects accidental corruption, a failed update, and unsophisticated tampering.
It does **not** prove a running process against a determined attacker: one who can modify
the executable can modify the checker or its display. `Verified` is a statement about a
comparison this client made, not a security guarantee about the machine. `Unknown` is
never treated or rendered as `Verified`.

## 3. Handshake declaration

The authenticated control-message declaration is named `BuildIntegrityReport` and has
this fixed payload:

```text
u8 report_version = 1
u8 local_result                 // 0 = Verified, 1 = Mismatch, 2 = Unknown
u8 build_hash_sha256[32]        // SHA-256(current_exe()), even for Unknown
```

`build_hash_sha256` is exactly 32 raw bytes, never hex text or a Git commit hash. The
report uses `MSG_TYPE_BUILD_INTEGRITY = 0x0C` and is permitted only in
`WIRE_VERSION_V6` or later. Existing control-message protection authenticates it; it is
not a new identity, authentication, or attestation mechanism.

The receiver accepts a declaration only when all fixed fields are present and
`report_version == 1`. For an older wire version, missing declaration, unknown report
version, invalid result, or invalid length, its peer state is `NotReported`. No absent
or malformed declaration may decode as `Verified`.

## 4. Peer disclosure, never peer verification

The receiver locally compares a reported 32-byte digest against its validated bundled
manifest. It exposes exactly these states:

| peer state | meaning | messaging policy |
| --- | --- | --- |
| `ReportedPublished` | peer client reported a digest in the local signed list | display report; allow messaging |
| `ReportedUnpublished` | peer client reported a digest absent from the local signed list | display report; allow messaging |
| `NotReported` | no valid v6 declaration was received | display not reported; allow messaging |

The reported `local_result` is display context only; publication comes from the received
hash and the local signed list. Receiving-side manifest failure gives `NotReported` with
local degraded-trust disclosure, never a fabricated published verdict.

All peer wording says **what the peer's client reported**. It never states or implies a
property of the peer's machine, process, operating system, or screenshot protection.
`ReportedPublished` is not “the peer is verified”, and `NotReported` is not evidence a
peer is unsafe. An unpublished hash, modified client, stale list, or no report must
never refuse, block, downgrade, or otherwise prevent messaging. Refusal would make
disclosure a lever to force an official build.

## 5. Transparency boundary

The release workflow attests the installer it built with `actions/attest-build-provenance`;
Sigstore/Rekor supplies a public append-only provenance record. It supports independent
audit that a published artifact came from this repository and workflow. It is never
queried by a client in a conversation or handshake; log unavailability never rejects a peer.

## 6. Acknowledgements — no third class

Integrity introduces no acknowledgement or receipt class. `features.md` §6 is reused
verbatim:

> **6.1 Destruction acks — MANDATORY, signed**
>
> Emitted for burn and expiry instructions. Values: `destroyed` · `already_absent` ·
> `never_held`. Signed by the responding device's identity key.
>
> - **A1.** Never optional, never suppressible by user preference. Refusing to ack **is** the
>   honest disclosure: the sender's UI reads "this device has not confirmed deletion", exactly
>   the shape `revocation_status` already uses for "Not acknowledged".
> - **A2.** Aggregate across the peer's devices. **The sender UI shows the number confirmed,
>   never a boolean or a denominator** — "2 devices confirmed". A denominator exposes the
>   recipient's device count; a boolean rounds away incomplete confirmation.
> - **A3.** Absence of an ack is `Unconfirmed`. It is never rendered as "complied" and never
>   as "refused".
>
> **6.2 Privacy receipts — OPTIONAL, signed, recipient-controlled**
>
> `Delivered` (materialized) and `Opened` (rendered). These are metadata the recipient emits
> about themselves, so the recipient controls them.
>
> - **A4.** When receipts are off, the sender must **not** be able to distinguish "off" from
>   "not yet" — otherwise switching them off is itself a signal, which defeats the point.
> - **A4a.** Privacy receipts default **off**. A recipient opts in explicitly; the sender
>   receives no capability, status bit, or alternate failure that distinguishes this default
>   from an unreported receipt.
> - **A5.** Receipts are **strictly monotone**: `Prepared → RelayAccepted → Delivered → Opened
>   → Destroyed`. A receipt for an earlier state arriving later is **dropped, never displayed,
>   never regresses the row.** Out-of-order arrival is normal, not exceptional.
> - **A6.** **`Opened` is emitted at render START, not at close.** A client that crashes mid-view
>   must still have reported. Under-reporting a privacy-relevant event is strictly worse than
>   over-reporting it.
> - **A7.** Receipts are facts about the past and are **never invalidated by a later lifecycle
>   transition**.

No integrity-flavoured ack may be added.
