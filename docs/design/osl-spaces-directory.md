# OSL Spaces directory: discover Spaces, never members

Status: design boundary, 2026-08-02. This document does not create a directory,
publish a Space, or make any Space discoverable.

## Decision

If OSL later offers Space discovery, it uses the same privacy shape as D20
username discovery: a k-anonymity bucketed lookup followed by an exact match.
There is no prefix search, fuzzy search, type-ahead, browse-all view, or
directory page that can be walked to enumerate Spaces.

A successful exact match may return only the Space's published name,
description, and a join capability. A capability is an invitation to request
admission; it is not evidence that admission has completed or that the caller
can see prior content.

Invite codes shared out of band remain the fully private path. They do not
require a directory query and the directory does not learn the inviter/invitee
pair.

## Deliberate non-disclosures

Directory records and lookup responses must never include, derive, or expose:

- a member, moderator, or administrator roster;
- member, online, or total counts;
- join/leave history or invitation history;
- message volume, message rate, last activity, or any activity-derived rank;
- presence, last-seen, typing, read, or speaking state; or
- an endpoint that accepts a name prefix or returns a browsable result set.

These are not harmless presentation details. Roster and count data make a
membership graph queryable; activity and presence data make that graph live.
Ranking by popularity or recency would recreate the same leakage even if the
raw fields were omitted.

## Lookup boundary

The client derives a bucket from the normalized exact Space name and asks for
that bucket. It performs the exact comparison locally among the returned
decoys. The service must not offer an API that reduces the bucket to a
server-side one-record lookup or reveals which candidate was selected.

The bucket is a privacy mitigation, not a guarantee of strong anonymity at a
small deployment. Its effective anonymity set depends on occupancy; it must
not be marketed as making a Space, its creator, or its membership anonymous.
Rate limits and abuse controls are required before a directory implementation,
but they do not turn an enumerable query shape into a safe one.

## Join semantics

Discovery is separate from membership. A directory result does not reveal
whether the requester knows a member, whether the Space is active, or whether
the join capability will be accepted. The join flow must retain the Space
protocol's pending state and rotate-before-admit rule: a newly admitted member
does not receive material sent before admission.

The directory is therefore not a people finder, a member browser, or a
presence service. It discovers a deliberately published Space by its complete
name only.

## Implementation gate

No endpoint or UI ships from this design alone. Before implementation, it must
have a reviewed bucket protocol, an exact-match-only client contract, bounded
responses, rate-limit and abuse controls, and tests proving that responses
contain no roster, count, or activity fields. Any proposal for public browsing,
search suggestions, or visibility into members requires a new owner decision
and threat-model review.

## Sources

- `plan/09-DECISIONS.md` D20 — bucketed exact-match discovery and out-of-band
  invite codes.
- `plan/03-CONTRACTS/identity.md` §6.2 — exact-match only; no prefix or fuzzy
  lookup because the userbase would be enumerable.
- `plan/05-TRACKS/T21-servers.md` T21-J1/J2 — Space discovery must not leak a
  roster, size, or activity signal.
