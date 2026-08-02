# Retention claims and evidence

This is the public-copy boundary for the transient OSL payload store. It implements the
honesty requirements in the storage contract, §8; it does not broaden the Burn promise in
[`docs/design/osl-public-claim-allowlist.md`](design/osl-public-claim-allowlist.md).

## Current public position

OSL may say that the relay stores opaque end-to-end encrypted payloads without the keys and
that the acknowledgement carries no account identity. It must also say that the relay can see
request IP addresses and timing, unless and until OHTTP is deployed.

There is currently **no public “instant wipe”, “immediate deletion”, or “irreversible deletion”
claim** for relay payloads. The code uses R2 for payload bytes, but the deletion-durability probe
has not yet produced and recorded a receipt against the exact deployed Worker and bucket. A
source change, a unit test, or a local preview is not that evidence.

Once that receipt exists, the narrow allowed wording is: **“After a receiver acknowledges a
payload, OSL deletes that payload from the serving relay bucket.”** It must be accompanied by:

- “This does not remove copies already received by people or providers.”
- “It does not prevent a provider or network from logging future activity.”
- “The relay still observes request timing and IP address unless OHTTP is in use.”

Do not substitute *gone forever*, *unrecoverable*, *instant wipe*, or a promise about backups,
legal process, recipient devices, or connected-service history.

## Orders and retention

Deletion is useful only against a retrospective request such as “give us last month.” It does
nothing against a forward-looking order such as “log everything from tomorrow.” The latter is a
linkage problem: it is reduced by the protocol's identity-free fetch and acknowledgement design,
not by claiming that deletion makes future observation impossible.

No public text may imply that a deletion request defeats a forward-looking order, logging,
provider retention, exports, backups, screenshots, or copies a recipient already opened.

## Website and in-app copy inventory

The scoped source scan on 2026-08-01 found **no active website or in-app retention string that
needs replacement**. The following live copy is retained because it makes the bounded promise
rather than an irreversible one:

| Surface | Exact current string | Disposition |
|---|---|---|
| `apps/osl-hub-ui/src/feature-claims.ts` | “OSL requests cleanup for protected copies it controls and reports whether that cleanup was acknowledged.” | Keep. It reports acknowledgement, not irreversible deletion. |
| `apps/osl-hub-ui/src/feature-claims.ts` | “Burn cannot take back access someone already had, undo screenshots, remove exports, erase backups, or stop a camera.” | Keep. It states the copy limitations explicitly. |
| `README.md` | “Burn cleans up. It does not un-send.” | Keep. It does not promise connected-service or recipient deletion. |

Before any website or app surface adds relay-deletion copy, replace a proposed absolute claim
with the narrow allowed wording above and its three required limitations. The exact proposed
strings below are prohibited and must be changed before publication:

| Do not publish | Replace with |
|---|---|
| “We instantly wipe messages from our servers.” | “After a receiver acknowledges a payload, OSL deletes that payload from the serving relay bucket.” (Only after a recorded production durability-probe receipt.) |
| “Deletion keeps your messages safe from legal orders.” | “Deletion limits what remains for retrospective requests; it does not prevent future logging.” |
| “Burn makes messages unrecoverable everywhere.” | “Burn cleans up the stated OSL scope and reports each confirmed result separately.” |

## Release evidence required before the narrow relay claim

```json retention-claim-policy
{
  "current_relay_deletion_claim": "not-eligible-no-production-probe-receipt",
  "required_evidence": [
    "payload bytes are stored in R2 rather than D1 or a SQLite Durable Object",
    "the exact deployed workers.dev Worker passes deletion-durability-probe",
    "the probe observes a 404 through the PAYLOADS binding after acknowledgement",
    "the serving bucket has no lock rules",
    "the evidence records that R2 versioning is unsupported"
  ],
  "required_limitations": [
    "does not remove copies already received by people or providers",
    "does not prevent a provider or network from logging future activity",
    "relay observes request timing and IP address unless OHTTP is in use"
  ],
  "forward_looking_order": "deletion does not defeat it; unlinkability is the relevant defense"
}
```

The release owner records the probe receipt with the Worker URL, bucket name, deployment
identifier, run time, and output. If the payload backend, bucket configuration, or deletion path
changes, the claim returns to ineligible until the probe is run again.
