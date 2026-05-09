# Design: Discord OAuth authentication flow

Status: **Draft.**

This doc covers Discord OAuth handling for key-server registration
and re-registration. Cross-referenced from
[`unlock-and-duress.md`](unlock-and-duress.md) and
[`key-server-api.md`](key-server-api.md).

## Purpose

Discord OAuth is used by the key server to verify ownership of a
Discord `user_id` at the **moment of registration or
re-registration**. After registration, the user's `IK_X25519` self-
signature binds them to the `user_id`; subsequent operations rely on
`IK_X25519`-signed requests, **not** on Discord OAuth.

Why OAuth at registration: prevents random spoofing of arbitrary
`user_id`s in the key server's user table. Without OAuth, an
attacker could register fake keys for any Discord user_id. With
OAuth, the attacker must compromise the actual Discord account.

OAuth is **not** the primary defence against MITM after re-
registration — that role belongs to contact-side fingerprint
verification (Signal safety-numbers pattern). OAuth raises the bar
to "must compromise the Discord account first," but the contact-side
verification is what keeps the construction safe even when that
happens.

## OAuth flow during onboarding

Standard Discord OAuth2 authorization-code flow:

1. App opens the user's default browser to:
   `https://discord.com/api/oauth2/authorize?client_id=<APP_ID>&response_type=code&scope=identify&redirect_uri=http://localhost:<RANDOM_PORT>/callback&state=<RANDOM_NONCE>`

   Scope is `identify` only — we do **not** request `messages.read`,
   `guilds`, or any data scope. Identity verification is the only
   purpose.
2. User authorizes in browser.
3. Discord redirects to
   `http://localhost:<RANDOM_PORT>/callback?code=<CODE>&state=<RANDOM_NONCE>`.
4. App's local listener:
    - Verifies the `state` parameter matches what was sent (CSRF
      protection).
    - Captures the `code`.
    - Closes the listener immediately.
5. App exchanges the code for an access token via Discord's token
   endpoint (`POST /api/oauth2/token`).
6. Access token presented to key server in the `Authorization:
   Bearer <token>` header on `POST /v1/register`.

## Token verification at server

On receiving a registration request:

1. Server calls Discord's `GET /users/@me` with the supplied access
   token.
2. Discord returns the canonical `user_id` and `bot` flag.
3. Server verifies:
    - `bot == false` (reject bot accounts; see "Bot vs user
      accounts" below).
    - The `user_id` claim returned by Discord matches the
      `user_id` in the registration request body.
4. If verification passes, server proceeds with registration.
5. Server caches the verification result for **10 minutes**, keyed
   by `(user_id, sha256(token))`. Replays within 10 min skip the
   Discord call but **still apply** the rate limit.

Cache TTL of 10 min is a tradeoff between Discord API load and
replay window. See open question 1.

## Token expiry and refresh

Discord access tokens are valid for ~1 week. The server does **not**
store refresh tokens; it does not need to. After registration, the
user's `IK_X25519` self-signature is the binding for all subsequent
operations.

If a user re-registers more than 1 week after initial registration
(or after the access token's actual expiry), the client re-runs the
OAuth flow to obtain a fresh access token. The user goes through
Discord's OAuth consent screen again — typically auto-confirmed if
Discord still has a valid session for the user.

## Behaviour during Discord outages

| Operation | Behaviour |
| --- | --- |
| Initial registration (`POST /v1/register`, new `user_id`) | **Fail closed.** Server cannot verify ownership without Discord. Client surfaces: *"Discord verification unavailable, retry shortly."* |
| Re-registration (post-duress) | **Fail closed.** Same reason. |
| All other operations (key fetch, prekey bundle, wrapped-key upload, burn, sessions/rotate, alerts) | **Fail open** — these are authenticated by `IK_X25519` signature, not OAuth. Continue normally. |

Rationale: a user who has already registered does not depend on
Discord availability for ongoing operation. Only registration
requires OAuth, which is acceptable because registration is a
once-per-install event.

A Discord outage long enough to block a user's post-duress
re-registration is a real availability hit but does not affect
existing-installation use. Documented honestly; no silent fallback.

## Bot vs user accounts

User accounts only. Bot accounts have a different OAuth flow (bot
token, no user identity), and this app is for human users. Reject
`bot == true` at registration. User-facing copy if attempted:
*"This app does not support Discord bot accounts."*

## Re-registration semantics (post-duress)

Re-registration is identical to initial registration in OAuth terms:
fresh access token, server verifies via `/users/@me`, server
overwrites `IK_X25519` and `IK_MLKEM768` public keys, bumps
`last_rotated_at`, applies the rate limit below.

### Rate limit (resolved)

**3 re-registrations per 24-hour rolling window**, then exponential
backoff:

| Attempt within 24 h | Required delay before next attempt |
| ---                 | ---                                |
| 1st, 2nd, 3rd       | None (allowed)                     |
| 4th                 | 1 hour                             |
| 5th                 | 4 hours                            |
| 6th and subsequent  | 24 hours each                      |

Rationale: handles legitimate repeated-duress scenarios (escalating
threats where a user is forced to duress, reinstalls, gets duressed
again) while preventing automation abuse where an attacker bombards
the server with registration attempts.

The actual defence against malicious re-registration is the contact-
side fingerprint verification documented in
[`key-server-api.md`](key-server-api.md). Rate limit is belt-and-
suspenders.

## Privacy considerations

- Server learns the Discord `user_id` at registration. This is by
  necessity — without it, anyone could register for anyone's
  Discord identity.
- Server does **not** store the OAuth access token beyond the
  10-min verification cache (and only the SHA-256 hash, not the
  token itself).
- **Pseudonymous mode** (v2+ per original spec) replaces `user_id`
  with an opaque ID; the OAuth flow is replaced by out-of-band
  pseudonym exchange (QR code, short code). Doesn't apply in v1.

## Open questions

1. **Token-verification cache TTL.** 10 minutes is a tradeoff guess.
   Tradeoff: shorter TTL = more Discord API load but smaller replay
   window; longer TTL = less load but larger window. Confirm against
   Discord's API rate-limit posture and observed traffic.
2. **Localhost listener security.** State parameter required; bind
   listener to a single random port per flow; close immediately
   after callback. Verify these defaults are sufficient against
   local attackers (other apps on the same machine).
3. **2FA-required Discord accounts.** Discord's OAuth flow handles
   2FA transparently when the user has it enabled. Confirm no
   special handling needed in our client.
4. **Rate-limit behaviour under genuine DoS.** A targeted user under
   coordinated coercion campaigns could legitimately burn through
   the 3/24h allowance. After 24 h delay enforced, should the user
   have a manual override channel (e.g. contact support, identity
   verification)? Out of scope for v1; document and revisit if
   reports emerge.

## Review gate

- [ ] Review of OAuth flow against Discord developer docs (best
      practices may shift).
- [ ] CSRF / state-parameter validation review.
- [ ] Failure-mode testing during Discord outages.
- [ ] Localhost-listener security review (port range, bind address,
      TLS not used because localhost — confirm acceptable).
- [ ] Rate-limit tuning based on observed traffic and incident
      reports.

## References

- Discord OAuth2 documentation (developer portal).
- RFC 6749 (OAuth 2.0).
- RFC 7636 (PKCE — applicable extension for native apps; consider
  in implementation).
