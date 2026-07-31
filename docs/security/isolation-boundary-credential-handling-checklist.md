# Isolation Boundary And Credential Handling Review Checklist

Status: required before any hosted AutoScrub, optional proprietary module, or
native account-control feature may graduate from assisted local operation to
delete-capable or unattended operation.

This checklist is the security-review intake for boundaries that can observe
user content, drive another application, delete user data, or handle account
authorization. Passing this checklist is not itself permission to ship. The
central acceptance record must name the exact commit, reviewed files, test
evidence, reviewer, and open residual risks.

## Review Scope

The review covers four boundary families:

1. Local native companion and overlay boundaries.
2. Browser and hosted AutoScrub boundaries.
3. Optional proprietary module boundaries.
4. Credential, consent, binding, authority, and deletion pathways.

Out of scope for this checklist: generic UI polish, marketing claims, pricing,
and future roadmap promises. Those may block release through other gates, but
they do not satisfy this review.

## Required Evidence

Every reviewed boundary must have evidence for each item below.

| Gate | Evidence Required | Refusal Condition |
| --- | --- | --- |
| Explicit user consent | A durable user action or receipt scoped to the exact account, service, operation, and time window. | Missing, expired, revoked, implied, inherited, or ambiguous consent refuses the action. |
| Account binding | A verified account or local identity binding for the exact surface being acted on. | Missing, stale, cross-account, guessed, copied, or UI-only account binding refuses the action. |
| Operation authority | A production source of read, send, delete, or review authority for the exact requested operation. | Missing, synthetic, test-only, comments-only, or broader-than-requested authority refuses the action. |
| Isolation boundary | Proof that the boundary does not inherit ambient filesystem, network, process, browser-session, credential-store, token, cookie, or destructive authority. | Ambient authority or unreviewed bridge capability refuses the action. |
| Credential handling | Proof credentials and account identifiers stay inside the component authorized to hold them and do not enter logs, diagnostics, Debug, Display, screenshots, fixtures, or review artifacts. | Secret-bearing diagnostics, account handles, credentials, or stable private identifiers refuse the action. |
| Plaintext minimization | A list of every plaintext ingress and egress path, with size limits and redaction behavior. | Unbounded plaintext, hidden transcript capture, or default plaintext sharing refuses the action. |
| Deletion semantics | A scoped proof of what local state, remote blob, or service item can actually be deleted. | Missing readback, inability to bind the target, or claims that exceed verified deletion refuses the action. |
| Challenge handling | A stop/restart matrix for authentication challenges, service challenges, missing permissions, interrupted sessions, and user cancellation. | Automatic bypass, retry-after-challenge, anti-detection behavior, or silent fallback refuses the action. |
| Auditability | A credential-free receipt or event record that says what was attempted, what was refused, what succeeded, and what remains unknown. | Receipts containing secrets, account handles, native message text, or unverifiable success claims refuse the action. |

## Credential Handling Requirements

- No credential, account identifier, user handle, access token, refresh token,
  cookie, session secret, recovery phrase, private key, bearer token, or native
  window handle may appear in Debug or Display output.
- Logs, panic messages, fixture names, screenshots, audit records, and support
  exports must use redacted digests, fixed labels, bounded counters, or opaque
  review identifiers.
- Test fixtures must use obviously fake values and must not resemble live
  secrets. A fixture that needs stable identity must use deterministic fake
  digests, not real handles or account IDs.
- A component that only needs a digest must never request the underlying secret
  or account identifier.
- A component that receives explicit plaintext must document the exact user
  action that created that plaintext path, the maximum byte count, and the
  redaction behavior.

## Boundary Requirements

- The open side owns the final decision. Closed, hosted, browser, or native
  helper components may return advice or bounded observations only.
- Advice cannot widen permissions, create consent, synthesize binding, mint
  authority, lower a security floor, or convert assisted operation into
  unattended operation.
- Absence of consent, binding, authority, permission, entitlement, reviewer
  approval, or verified capability means refusal or Unavailable, never
  permission.
- Challenge, cancellation, timeout, account mismatch, target mismatch, stale
  window binding, stale profile binding, and failed readback are stop states.
  They must preserve local drafts and require a new user action before retry.
- Hosted isolation must prove per-run disposal, least-privilege credentials,
  credential revocation, bounded retention, and a credential-free receipt before
  it can handle delete-capable work.
- Browser import has exactly two user choices: bring in selected local data, or
  open the service in protected Firefox. Silent import is refused.
- OSL-RN remains disabled for product traffic until external cryptographic
  review explicitly accepts the wire-in. `RN_WIRE_IN_ENABLED` must stay `false`.

## User-Facing Language Requirements

User-facing copy must translate implementation details into product concepts.
The following terms are review-only and must not appear as navigation labels,
onboarding choices, warning labels, settings names, status labels, or ordinary
decision copy:

- keyserver
- ratchet
- receipt
- browser profile
- provider adapter

For Burn-related copy, use the five-part guarantee model:

1. Local OSL copy.
2. OSL server copy.
3. Other person's app.
4. Connected service message.
5. Already opened copies.

Burn copy must not use these banned claims:

- cryptographic burn
- destroys keys, not messages
- permanent ciphertext
- disappears forever
- permanently undecryptable
- gone for good

Do not fabricate ban-risk percentages or probability estimates for connected
services.

## Acceptance Checklist

A release reviewer must mark each item before approval:

- [ ] The reviewed commit and file list are named.
- [ ] All required evidence rows above are satisfied.
- [ ] Every absence case above refuses instead of permitting.
- [ ] Debug and Display output are checked for credentials, account identifiers,
  handles, tokens, secrets, and stable private identifiers.
- [ ] Plaintext paths are explicit, size bounded, and redacted in diagnostics.
- [ ] Challenge, cancellation, timeout, stop, and restart behavior are tested.
- [ ] Deletion claims are scoped to verified local, server, service, and peer
  effects.
- [ ] User-facing copy avoids implementation terms and banned Burn claims.
- [ ] OSL-RN wire-in remains disabled.
- [ ] Residual risks are listed with owner decisions or release blockers.

Any unchecked item is a release blocker for delete-capable, unattended, hosted,
or proprietary-boundary operation.
