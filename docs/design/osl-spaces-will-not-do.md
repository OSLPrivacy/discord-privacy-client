# OSL Spaces: deliberate omissions

**Planning input and binding build constraints — never public claim copy.** Each omission below
names the task or tasks that must make it true. A task may not replace an omission with a weaker
implementation without an owner decision.

## 1. No admin who can read content

There is no owner, moderation, or recovery key. Roles decide who receives future keys; they never
give an admin access to content a member already holds. An admin who wants to read a channel joins
it with the same keys as everyone else.

**Enforced by:** T21-F1, T21-F2, T21-F3.

## 2. No server-side content moderation

Spaces have no automod, keyword filter, or server-side message classifier. The relay cannot read
content, and the local sensitive-content classifier can only warn the author; it cannot gate a
room.

**Enforced by:** T21-F2, T21-F5.

## 3. No history on join

A new member receives nothing sent before admission. Rotation completes before admission, rather
than relying on a history-visibility setting.

**Enforced by:** T18-C4, T21-K3.

## 4. No server-held member list

The relay must not learn a Space roster. This deliberately requires per-recipient-device fan-out:
the cost is O(members × devices), not a shared group blob. That cost is retained as a privacy
constraint so later capacity work cannot optimise it into a server-side roster.

**Enforced by:** T21-G2, T21-G3.

## 5. No public directory that enumerates people

Spaces may be discoverable, but their members may not be enumerated: no roster API, member-count
scraping, or online-member broadcast.

**Enforced by:** T21-J1, T21-J2.

## 6. No presence, typing indicators, or default Space read receipts

The Space UI must not reveal online state, last-seen state, typing activity, or read activity by
default. Where receipts are offered, off remains indistinguishable from not-yet.

**Enforced by:** T21-H2, T21-H6.

## 7. No message history that survives OSL

The local encrypted store is the source of truth; the relay is transient. OSL therefore keeps no
server-side Space archive to hand over if the service is acquired, subpoenaed, or disappears.

**Enforced by:** T21-G5, T21-G8.

## 8. No lock-in

A member can export their Space history with their own keys, and the relay remains simple enough to
self-host. Those two properties prevent a service outage from becoming a loss of already-held
history.

**Enforced by:** T6-E1, T21-G5.

## 9. No engagement machinery

Spaces contain no algorithmic ordering, ranked feed, notification optimisation, last-seen signal,
boost, or server-level mechanic. Messages remain a chronological, member-controlled surface.

**Enforced by:** T21-H1, T21-H2.

## 10. No ads, analytics, or telemetry on Space activity

Space activity must not feed advertising, analytics, or telemetry. The relay's necessary delivery
state cannot be repurposed into an activity record.

**Enforced by:** T21-G3, T21-H1.

## 11. No admin-visible log of member behaviour

The signed governance log records only governance events—key rotations, membership changes, and
role changes. Every member sees it; it contains neither message content nor read activity, and
device labels remain local.

**Enforced by:** T21-F4, T21-H5.

## 12. No pretending 10,000-member E2EE rooms work

Space size is capped by measured fan-out and manifest arithmetic. A join above the cap is refused
with the limit and reason named; the product must not silently degrade delivery or protection.

**Enforced by:** T18-E3, T21-G9.

## 13. No phone number or legal identity

Space membership is based on an OSL username and invitation capability, never a phone number or
legal identity.

**Enforced by:** T21-C6, T21-C7.

## 14. No claim before the capability

No Spaces capability is claimed in the product or website before it ships and earns an allowlist
row. This document constrains architecture, not public copy.

**Enforced by:** T21-A6, T21-A7.
