# OSL Enclaves: feasibility and size

Status: **planning estimate for the D66 v1 commitment; not an implemented
capability or a shipping claim.**

## Decision-sized answer

OSL Enclaves — OSL-native, Discord-style communities — are a build from zero. The current size is **73 scoped
tasks, 5,290 minutes (about 88 task-hours), and a realistic delivery band of
110--140 engineering hours**. The delivery band includes integration, contract
freezes, measurement-driven rework, and the real-machine proof that task sums
do not capture. Four proof tasks need Windows VMs.

This is not a reason to defer the work: D66 makes Discord-like OSL communities
a v1 commitment. It is a reason to schedule it as its own, post-first-usable
track (T21), rather than presenting it as a small extension of 1:1 OSL Chat.

The estimate is for the privacy-preserving text-community foundation:
membership, channels, invitations and join/leave/removal, the fixed
member/moderator/admin role set, local moderation controls, encrypted group
delivery, offline behaviour, and end-to-end proof. Group voice is a separate
E2EE media/SFU effort and is not included in this number.

## Verified starting point

There is no OSL-native community primitive in the shipping app or relay. The
prototype contributes interaction vocabulary, not a backend, a membership
model, moderation, or an encrypted delivery design.

The sender-key premise needs the current wording: sender keys are **enabled in
the IPC core**, but the shipping app constructs only DM conversations, so it
cannot reach the group route. Communities therefore require a conversation and
membership surface; they are not enabled by flipping a feature flag. T18 owns
the sender-key construction and its security fixes. T21 consumes that work and
must not duplicate it.

## The cost that prevents a casual rollout

D17 and the frozen features contract require one independently acknowledged
payload blob per recipient *device*. The group manifest adds one multi-fetch
object per message. Consequently, at the proposed five-device ceiling, a
100-member community message creates **501 relay rows**, not one shared
ciphertext. Sender keys avoid repeated payload encryption; they do not remove
per-device relay copies, per-device acknowledgement, burn, view-once, or
expiry work.

| Worked case | Arithmetic | Result |
| --- | --- | ---: |
| Recipient-device payload copies | `100 members × 5 devices` | 500 |
| Manifest object | `1` | 1 |
| Relay rows per message | `500 + 1` | 501 |
| 100,000-row global pool / one message | `floor(100,000 / 501)` | 199 messages in flight |
| One community, 30 offline members, 200 messages/day | `30 × 5 × 200` | 30,000 undelivered rows |

The last case consumes 30% of the shared undelivered pool in one day. Copies
for offline devices remain for the fixed seven-day TTL, and D16 forbids
evicting them to make room for retained data. Without a separate community
fan-out pool and a measured join limit, a busy community can exhaust the same
pool that direct messages need. This is a release gate, not a later capacity
optimization.

The manifest also has a hard planning ceiling: its 64 KiB blob can hold fewer
than 1,024 recipient-device entries at the contract's worst-case 64-byte entry
size. Actual framing, entry encoding, grants, row capacity, and offline-member
measurements must set the enforced member limit; 100 is an example, not an
approved limit.

## Dependencies before implementation can start

1. T5 must repair the safety-number ceremony. The current group sender check
   relies on a carrier identity and a TOFU pin; an OSL-native community has no
   third-party carrier, so it also needs T21's replacement sender-authentication
   design.
2. T18 must land the sender-key rotation and group-lifecycle work. Membership
   removal must rotate before a new member is admitted, and a removed member
   cannot be represented as having lost content already held.
3. T1 and T6 must provide the frozen transport, per-device roster, manifests,
   capacity decision, and a community-safe delivery tag. A relay must not learn
   a community roster or turn a group-derived tag into a member-tracking feed.
4. T2's destructive lifecycle contract and the first-usable chat work must be
   integrated rather than reimplemented.

## Recommendation

Commit to T21 as the implementation track, with **110--140 engineering hours
after the listed gates**, and do not attach a calendar promise until the row
pool decision and the 5/20/100-member measurements exist. The first shippable
community scope needs an enforced, measured member cap and an explicit refusal
path; it must never silently fall back to shared copies or first-fetch-wins
delivery.

## Contract vector

```json
{
  "version": 1,
  "product": "osl-enclaves",
  "estimate": {
    "tasks": 73,
    "taskMinutes": 5290,
    "taskHours": 88,
    "deliveryHoursLower": 110,
    "deliveryHoursUpper": 140,
    "windowsVmProofTasks": 4
  },
  "fanout": {
    "members": 100,
    "devicesPerMember": 5,
    "manifestRows": 1,
    "payloadRows": 500,
    "totalRows": 501,
    "globalLiveRowBudget": 100000,
    "messagesBeforeBudgetExhaustion": 199
  },
  "offlineWorkedCase": {
    "offlineMembers": 30,
    "devicesPerMember": 5,
    "messagesPerDay": 200,
    "undeliveredRows": 30000
  },
  "requires": [
    "separate-community-fanout-pool-or-equivalent-capacity-decision",
    "measured-and-enforced-member-limit",
    "replacement-native-sender-authentication",
    "per-device-roster"
  ]
}
```

## Sources

- Owner decision D66 in `plan/09-DECISIONS.md`: communities are in v1 and
  depend on sender keys, T5, and a storage-scale review.
- `plan/05-TRACKS/T21-servers.md` §§0, 4, and 8: task inventory, delivery
  band, fan-out arithmetic, and gates.
- `docs/design/group-sender-keys-shipping-state.md`: sender keys are enabled
  in core but unreachable from the shipping app.
- `03-CONTRACTS/features.md` §4.4 and `03-CONTRACTS/transport.md` §1b:
  per-device copies and the group manifest.
