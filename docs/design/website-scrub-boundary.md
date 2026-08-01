# Website Scrub boundary

Status: T11-F1 decision — no live public username check.

## Decision

`oslprivacy.com` must not offer a live username-availability check. The
website will not call the username directory, maintain a copied list of
claimed names, or present an availability result. A visitor who wants an OSL
username chooses one in the client and learns the authoritative outcome when
their signed claim is submitted.

This is a deliberate **skip**, not an unavailable implementation. Do not
build T11-F4's live check. The website's Scrub surface remains a rendered,
username-only demo until a later, separately approved product decision
changes this boundary.

## Why

The current keyserver has an unauthenticated exact lookup at
`POST /v1/usernames/lookup`. It keeps a handle out of the URL and pads hit and
miss responses, but it is still a direct request for one handle. In
particular, its rate-limit key is the caller IP. That makes the endpoint an
existing server-side lookup capability, not a privacy-preserving public
availability feature and not client wiring for the website.

OWNER DECISION D20 controls this choice: username discovery must use a
k-anonymity bucketed lookup, with exact matching only and no prefix search.
The current exact endpoint does not meet that requirement. D19 also requires
confusables detection and permanently retired usernames; neither property can
be truthfully represented by a static website list.

## Consequences and re-entry criteria

- No website code may call `/v1/usernames/lookup` or `/v1/usernames/claim`,
  and no new availability endpoint is authorized by this decision.
- Do not turn the website's mock into a static-directory check. It would be
  stale and would incorrectly imply that a positive result reserves a name.
- The signed client claim is the only availability authority for now; a
  conflict is an ordinary claim outcome, not a preflight result.
- Reconsider a website discovery feature only after T5 delivers the D20
  bucketed directory protocol and a privacy review approves a browser caller.
  That work must do exact matching locally from the returned bucket and must
  not add prefix search or a direct per-handle request.

This decision does not approve the existing direct lookup for new consumers.
It identifies it as a pre-existing route that must be replaced or retired as
the D20 directory work lands.
