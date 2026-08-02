# Group feature composition — proposed T2 contract wording

This is the ready-to-paste extension for `plan/03-CONTRACTS/features.md`.
T18 supplies the group reasoning; T2 owns the contract and must decide where
these paragraphs are inserted.  This document deliberately does **not** edit
the contract.

## Add after §6.1: §6.1a Group destruction-ack aggregation

For a group, a destruction acknowledgement is a fact about one recipient
**device**, not a fact about the group.  The aggregate denominator is every
recipient device to which the message was delivered or queued for delivery;
the numerator is the subset whose clients have confirmed the applicable
terminal state.  The UI presents the count (for example, “4 of 6 recipient
devices confirmed”), never a green boolean such as “all deleted” unless the
numerator equals the complete denominator.

`Unconfirmed` is a real, durable state: a member that is offline, has retired
a device, or never receives the instruction keeps that device out of the
confirmed numerator.  Absence of an acknowledgement is never rendered as
“complied”, “refused”, or an error.  A permanently unconfirmed member is not
a reason to round the group result into either success or failure.

## Add after §8.11: §8.11a Group burn while members are offline

A group burn has independently observable effects:

1. the server deletes each blob it still holds; and
2. the sender queues one peer-device destruction instruction for every
   recipient device.

The first effect is a server event and can be confirmed as such.  The second
effect is an independent queue per recipient device and remains `Unconfirmed`
until that device acknowledges it; one offline member must not prevent the
server-deletion result from being shown, but neither may it produce a combined
green tick.  The UI states the two facts separately, using the §6.1a count for
device confirmation.  It must never say that all group copies were deleted
while an instruction queue is undrained.

## Add to §8.3 and §10.3: view-once fan-out

In a group, “exactly one fetch, ever” is an invariant of each blob, not of a
logical message.  Per §4.4, the sender creates one single-fetch blob and one
capability for each `(message, recipient device)` pair.  A reservation or
failed download for member A’s device neither consumes nor destroys member
B’s blob; retries within A’s own reservation window remain governed by §4.3.

Accordingly, sender status is per recipient device and is aggregated as a
count.  “Reserved”, “downloaded”, and “opened” remain distinct facts for each
blob.  A group must not use one shared capability, one shared reservation, or
one group-wide “viewed” state.  The honest boundary in §10.3 applies to each
recipient device: a successful fetch proves only that that blob was served,
not that a human saw it.

## Add to §8.9: group expiry under skew

Each recipient device enforces the same per-message expiry inputs under the
two-clock design in §8.9.  Devices may observe expiry, burn, view-once
consumption, or eviction in different wall-clock and delivery orders.  That
does not change the terminal effect: it is the join in §7.  The displayed
reason is always the fixed R2 severity order — `Burn > ViewOnceConsumed >
Expired > Evicted` — rather than arrival order or a member-specific clock.

Thus all members that have observed the same terminal facts render the same
reason even when their clocks are skewed.  An unobserved instruction remains
an acknowledgement-state issue under §6.1a; it does not authorize a different
precedence rule.

## Explicit non-changes

No revision to §7 R1–R6 is proposed: effect-as-join and the fixed explanation
order already generalise from two devices to any number of recipient devices.
No revision to the §4.3 CLAIM model is proposed: its object-and-clock
reservation remains correct.  Group support must preserve §4.4’s one blob per
recipient device model; replacing it with a shared capability would break
per-member acknowledgements, burns, and receipts.
