# OSL Circles: product scope

## Decision

**OSL Circles means OSL-native servers and communities.** A Circle is a
multi-member space with membership, channels, roles, moderation, and join/leave
rules. It is the Discord-capability product the owner put in v1 in D66, not a
synonym for a post's recipient list.

This resolves the collision between two earlier designs that used the same
word:

| Earlier design | Product name going forward | Relationship to Circles |
| --- | --- | --- |
| The `osl-chats-lab` prototype's spaces with channels, voice, roles, and a public directory | **OSL Circles** | The product definition. The prototype is design input only, not shipping implementation or a capability claim. |
| `osl-gui-final-plan.md`'s Close Friends / Family / Work post recipients | **private audiences** | A separate private-social feature. A private audience is not a Circle, server, community, or channel. |

“Circle” in product, design, and claim copy therefore always refers to the
server/community product. Use “private audience” when describing a selected
set of recipients for a post. Do not describe either product as currently
available: no OSL-native server/community backend or shipping surface exists.

## Scope boundary

D66 makes the server/community product a v1 commitment. It does not convert
the prototype into a specification or waive its dependencies. The delivery
track must define the initial capability set and prove it before any present-
tense claim. At minimum, an OSL Circle needs a membership model, channel model,
roles/permissions, moderation, and explicit join/leave behavior; otherwise it
is not the product D66 selected.

The work remains gated by sender-key group delivery (T18), membership identity
and trust (T5), and the storage-scale review required by D66. T21 owns the new
server/community track; this document records its product boundary and does
not claim implementation progress.

## Contract test vectors

```json
{
  "version": 1,
  "circles": {
    "product": "osl-native-server-community",
    "requiredCapabilities": [
      "membership",
      "channels",
      "roles-permissions",
      "moderation",
      "join-leave"
    ],
    "shippingStatus": "not-implemented"
  },
  "privateAudiences": {
    "product": "named-post-recipients",
    "isCircle": false,
    "shippingStatus": "not-implemented"
  }
}
```

## Decision sources

- `plan/09-DECISIONS.md` D66 — owner decision that OSL servers/communities are
  in v1 and should have Discord-like capabilities.
- `docs/prototypes/osl-chats-lab/` — the Discord-style prototype that supplies
  the community design direction.
- `docs/design/osl-gui-final-plan.md` — the distinct named-post-audiences
  private-social design.
