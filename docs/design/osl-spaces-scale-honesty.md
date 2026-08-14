# Spaces scale honesty

Status: **historical capacity analysis, superseded for membership policy by owner ruling T4.**

## Current product sentence

> Large Enclaves can take time to update. Removing someone is not immediate
> while OSL gives every remaining member new authority.

The delay warning begins at measured N. N is a warning/progress threshold, not
an admission limit or refusal boundary.

## Why progress is necessary

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

This note sets **no numeric member cap**. Measurements determine only when the
plain delay warning and truthful re-key progress become mandatory. If later
measurements differ, N changes; admission does not.

The storage owner must still decide the row budget and
whether Space fan-out has its own pool. Without a separate pool, a busy Space
can fill the shared undelivered pool and prevent all direct messages from
sending. Backpressure may delay sends, but it must not reject Enclave
membership or silently deliver a degraded protected message.

## Sources

- T21-B1 / track §4.1–§4.5: fan-out arithmetic, row budget, and the shared-pool
  failure mode.
- T21-G9: one cap shared by the join path and sender-key consumers, with an
  honest refusal.
- Owner decisions D16, D17, D29, and D68: do not evict undelivered messages;
  per-device copies; eager fetch; and no capability claim before it exists.
