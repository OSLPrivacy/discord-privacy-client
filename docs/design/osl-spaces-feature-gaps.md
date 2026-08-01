# Spaces community-scale feature gaps — handoff to T18-D1

Status: **contract input for T18-D1 and T2; not an implementation and not a
change to `features.md`.** This is the additional ready-to-paste wording T21
requires before T2-01 freezes.

T18-D1 already carries the four group-scale gaps: destruction-ack aggregation,
burn while a member is offline, view-once per recipient device, and expiry
under clock skew.  A Space adds three cases that must be resolved in the same
handoff.  The wording below deliberately retains D17's per-device copies and
D28's three separate burn effects; it does not turn either into an account or
group boolean.

## Add to §6.1a — departed recipients and destruction acknowledgements

For every message, record the recipient-device snapshot and the membership
epoch used for fan-out.  The aggregate must report three disjoint counts:
`confirmed`, `unconfirmed-active`, and `departed-unconfirmed`.  A device moves
to `departed-unconfirmed` only after a signed, ordered membership removal for
its member has been applied; it must not be silently removed from the message
snapshot or counted as confirmed.

The aggregate is therefore an honest terminal display even when a former
member's acknowledgement can never arrive.  It must say that the delivery is
unconfirmed for the departed recipient, not remain a generic `Unconfirmed`
state forever and not claim universal deletion.  A later acknowledgement from
that device may move it to `confirmed`, but no inferred acknowledgement may do
so.  Removal affects future fan-out; it cannot prove deletion of a copy that a
former member already received.

## Add to §8.9 — expiry policy precedence

Resolve a message's expiry exactly once, at send time, and serialize the
resulting absolute expiry in the protected message metadata.  The precedence
is: an explicit per-message expiry, then the channel default, then the Space
default.  A change to either default affects only later messages.  Each chosen
duration must remain within D14's 1-second to 30-day range; an absent expiry
is distinct from a zero duration.

All recipient devices use the serialized absolute expiry and the existing R2
severity order when rendering a terminal state.  They must not re-resolve a
default after receipt, which would let one Space member interpret the same
message under a different channel or Space policy.

## Add to §8.11a — removal tombstones

A signed removal event creates a durable, non-secret tombstone for the removed
member's recipient-device entries and for messages they authored.  The
tombstone carries the Space, channel, membership epoch, member identity, and
event ordering reference, but no plaintext or content-derived metadata.  It
is distributed through the same authenticated membership-event path as the
removal.

On receipt, clients stop future fan-out to the removed member, preserve the
historical message's original sender attribution, and retain the tombstone so
replayed pre-removal delivery, acknowledgement, or membership events cannot
restore active membership or overwrite the departed-recipient aggregate.
The tombstone is not a retroactive content wipe and must not be presented as
one.  Any burn of an already-delivered message still reports D28's per-member
queued and confirmed states independently.

## Contract vectors

```json
{
  "version": 1,
  "expiryPrecedence": ["message", "channel", "space"],
  "departureAggregate": {
    "confirmed": 2,
    "unconfirmedActive": 1,
    "departedUnconfirmed": 1,
    "display": "2 confirmed; 1 active recipient unconfirmed; 1 departed recipient unconfirmed"
  },
  "tombstone": {
    "stopsFutureFanout": true,
    "preservesSenderAttribution": true,
    "rejectsPreRemovalReplay": true,
    "retroactiveContentWipe": false
  }
}
```

## Non-changes and routing

T18-D1 should append these three inserts to its ready-to-paste T2 diff; T21
does not edit `03-CONTRACTS/features.md`.  T2 retains ownership of the frozen
contract.  The existing §7 R1–R6 ordering and §4.3 CLAIM model already
generalise to N recipients and are out of scope for this handoff.

## Sources

- T21-B4 in `/home/liamw/osl-plan/plan/05-TRACKS/T21-servers.md`:
  departed-member acknowledgements, channel/Space expiry precedence, and
  removed-member tombstones.
- T18-D1 in `/home/liamw/osl-plan/plan/05-TRACKS/T18-groups.md`: the four
  group-scale feature gaps and the T2 handoff boundary.
- Owner decisions D14, D17, D28, and D66 in
  `/home/liamw/osl-plan/plan/09-DECISIONS.md`.
