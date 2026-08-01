# Space delivery tags must not use group-shared state

Status: contract finding for T1 transport and T6 storage. This is an analysis
contract, not an implementation or a choice between the alternatives below.

## Finding

`transport.md` section 6b defines a delivery tag from `K_conv_n`, conversation
secret state. That construction has the intended separation property for a
two-party conversation only while the secret is known to those two parties.

In a Space, the group secret is shared with every member. If a recipient's
push-feed tag is derived only from that shared state, any Space member can
derive the tags for every other member. A member can then query or observe a
peer's feed and learn when that peer receives a message. This is exactly the
observation D-SEP forbids; the other Space members are part of the adversary
model here, not trusted observers.

The derivation input for a Space recipient must therefore include secret state
that is unavailable to all other Space members. Group-shared secret material
alone is prohibited.

## Required decision, jointly owned by T1 and T6

The eligible design direction is either:

1. derive each tag from pairwise or recipient-device state, alongside D17's
   independent per-device copies; or
2. give each member a distinct subscription secret that no other member has.

T1 and T6 co-own the resulting section 6b contract. T21 records the violation
and must not select either direction, define its key lifecycle, or introduce a
Space-specific delivery endpoint. The selected design must show that a member
who knows the Space group secret but does not hold another recipient's
private derivation input cannot compute that recipient's feed tag.

## Gate

No Space fan-out or push-feed implementation may reuse the section 6b
group-shared derivation until T1 and T6 publish and test a recipient-isolated
derivation and lifecycle. This gate is independent of sender authentication;
it prevents receipt-timing observation, not sender forgery.

## Contract test vector

The vector uses symbolic secrets rather than production cryptography. The test
models a tag as a deterministic function of its listed inputs, which is enough
to demonstrate the isolation requirement without prescribing an algorithm.

```json
{
  "version": 1,
  "prohibited": {
    "derivationInputs": ["space-group-secret"],
    "observerKnownInputs": ["space-group-secret"],
    "recipientKnownInputs": ["space-group-secret"],
    "observerCanComputeRecipientTag": true
  },
  "eligibleDirections": [
    "pairwise-or-recipient-device-state",
    "per-member-subscription-secret"
  ],
  "requiredProperties": {
    "groupSharedStateAloneProhibited": true,
    "recipientPrivateInputRequired": true,
    "otherSpaceMemberCannotComputeRecipientTag": true,
    "t1AndT6JointDecisionRequired": true,
    "t21DoesNotSelectDesign": true
  },
  "recipientIsolatedExample": {
    "derivationInputs": ["space-group-secret", "recipient-device-secret"],
    "observerKnownInputs": ["space-group-secret"],
    "recipientKnownInputs": ["space-group-secret", "recipient-device-secret"],
    "observerCanComputeRecipientTag": false
  }
}
```
