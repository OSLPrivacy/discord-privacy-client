# Spaces scale honesty

Status: **design constraint; not public copy and not a shipped Spaces capability.**

## The public sentence, when Spaces ships

> Spaces have a membership limit because every message is delivered separately to
> each recipient device. When members are offline, their copies remain in the
> relay until they reconnect, so the limit protects reliable delivery for Spaces
> and direct messages instead of silently degrading either.

This sentence may ship only with a measured member limit and the refusal that
names that limit and this reason. Until then, D68 prohibits a present-tense
Spaces capability claim.

## Why a limit is necessary

The current fan-out model is one relay blob for every recipient device plus one
manifest blob. The frozen-storage analysis used the following deliberately
modest example; **100 members is an example, not the member cap**:

| Input | Value |
|---|---:|
| Members | 100 |
| Device cap per member | 5 |
| Relay blobs per message | `100 × 5 + 1 = 501` |
| Global live-blob-row budget | 100,000 |
| Messages in flight before that global budget fills | `floor(100,000 / 501) = 199` |

Rows, rather than bytes, bind first in this model. More importantly, eager
fetch clears copies promptly only for members who are online. An offline
member's copies remain undelivered for the seven-day TTL. For one 100-member
Space with 30 members offline for one day and 200 messages that day, the relay
holds `30 × 5 × 200 = 30,000` undelivered rows. That is 30% of the global
100,000-row budget. Three such Spaces would exhaust it and make direct messages
queue and retry; undelivered messages must not be evicted to make room.

## What this does and does not set

This note sets **no numeric member cap**. A numeric cap chosen from a product
wish would be dishonest: it must come from T21-B1's storage arithmetic, T21-G9's
single refusal path, and T21-K4's measurements of rows, bytes, PUTs, grants,
and manifest size at 5, 20, and 100 members. If those measurements differ from
the arithmetic above, the measurements win and this note and the cap must
change together.

Before a cap can ship, the storage owner must also decide the row budget and
whether Space fan-out has its own pool. Without a separate pool, a busy Space
can fill the shared undelivered pool and prevent all direct messages from
sending. The product must refuse the next join over the measured limit; it must
never deliver a degraded Space message or call one protected.

## Sources

- T21-B1 / track §4.1–§4.5: fan-out arithmetic, row budget, and the shared-pool
  failure mode.
- T21-G9: one cap shared by the join path and sender-key consumers, with an
  honest refusal.
- Owner decisions D16, D17, D29, and D68: do not evict undelivered messages;
  per-device copies; eager fetch; and no capability claim before it exists.
