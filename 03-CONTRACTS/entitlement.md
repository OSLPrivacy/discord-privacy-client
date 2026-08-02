# Entitlement contract

**Status:** frozen by T16-C1.  This is the implementation contract for the
app and keyserver.  It supersedes the legacy subscription-shaped lifecycle for
prepaid Pro codes.  A code is a bearer credential; possession authorizes its
redemption and validation.  No account, email, payment method, device ID, or
payment reference is part of this protocol.

## Model and clocks

A paid Pro purchase issues one prepaid code for exactly one entitlement period
(`grant_seconds = 2,592,000`, 30 days).  Issuance does **not** activate the
period.  The server records two independent clocks:

| Clock | Field | Starts | Meaning |
| --- | --- | --- | --- |
| Shelf life | none in v1 | n/a | An unredeemed code has no expiry in v1.  A future shelf-life field must not change the entitlement clock. |
| Entitlement period | `redeemed_at`, `grant_seconds`, `expires_at` | successful explicit redemption | `expires_at = redeemed_at + grant_seconds`; it is the sole authority for the Pro period. |

The `licenses` row stores `redeemed_at`, `grant_seconds`, `expires_at`, and
`redemption_binding` as nullable columns.  For v1 `redemption_binding` is
always `NULL`: redemption is not install-bound.  Existing issued codes remain
valid under their recorded legacy state until a separate migration policy says
otherwise; this contract does not rewrite them destructively.

## Server state machine

New paid issuance creates `UNREDEEMED`, with `grant_seconds` set and
`redeemed_at`/`expires_at` `NULL`.  The only normal transitions are:

```text
UNREDEEMED --POST /v1/license/redeem--> ACTIVE --server time >= expires_at--> EXPIRED
                                     \--revocation---------------------------> REVOKED
ACTIVE -------------------------------revocation-----------------------------> REVOKED
EXPIRED ------------------------------revocation-----------------------------> REVOKED
```

`REVOKED` is terminal and wins over every other state.  `EXPIRED` is terminal
except that a later revocation changes it to `REVOKED`.  An unredeemed code
never expires merely because time passed.  Repeated redemption is idempotent:
it returns the original period and must never extend or mint a second period.

The hourly expiry sweep changes a redeemed paid entitlement to `EXPIRED` when
server time reaches `expires_at`.  Validation also reports `EXPIRED` whenever
the stored period has elapsed, so correctness does not wait for the sweep.

## HTTP contract

All timestamps are Unix seconds.  The bearer code is sent only in the JSON
request body and the keyserver stores only its SHA-256 hash.

### `POST /v1/license/redeem`

Request:

```json
{ "license_key": "OSL-XXXX-XXXX-XXXX-XXXX" }
```

For a checksum-valid issued code, redemption atomically performs the
`UNREDEEMED` → `ACTIVE` transition, stamps `redeemed_at` from server time, and
sets `expires_at = redeemed_at + grant_seconds`.  The conditional write is the
idempotency boundary: retries and concurrent calls return the same stored
period.  It must not be implemented as a read-then-write sequence.

Success response (both first redemption and a retry):

```json
{
  "status": "ACTIVE",
  "redeemed_at": 1735689600,
  "expires_at": 1738281600,
  "checksum_ok": true
}
```

An unknown or mistyped key returns the existing non-oracular validation shape
(`status: "UNKNOWN"`, `checksum_ok` as appropriate) and does not create or
change entitlement state.  A revoked code returns `REVOKED`; an expired code
returns `EXPIRED`.  The endpoint must be rate limited equivalently to
validation.

### `POST /v1/license/validate`

Request remains `{ "license_key": "…" }`.  **Validate is read-only: it never
starts, extends, or otherwise changes the entitlement clock.** This includes
support checks and the app's periodic refresh.

For a recognized key, the response is:

```json
{
  "status": "UNREDEEMED | ACTIVE | EXPIRED | REVOKED",
  "redeemed_at": 1735689600,
  "expires_at": 1738281600,
  "checksum_ok": true
}
```

`redeemed_at` and `expires_at` are `null` for `UNREDEEMED`; they remain
nullable for legacy rows.  Existing status-only clients must continue to
parse the response.  `current_period_end` is legacy compatibility data only;
new prepaid logic must use `expires_at` and never infer activation from
validation.

## Client cache and classification

`LicenseCacheInner` becomes blob version 2 and stores exactly this entitlement
projection, sealed at rest with the existing sealer:

```text
license_plaintext
last_validated_status
redeemed_at
expires_at
last_validated_at
checksum_ok
```

`current_period_end` is replaced by `expires_at`.  A v1 cache migration must
fail safely to `Free` if it cannot establish a v2 entitlement projection; it
must not invent an expiry or prolong Pro.  A successful, recognized validation
may update the cache and `last_validated_at`; failed, malformed, or unknown
responses must not slide the bounded offline window.

The app classifies `ACTIVE` with a future `expires_at` as `Paid`.  `UNREDEEMED`,
`EXPIRED`, `REVOKED`, `UNKNOWN`, malformed data, missing cache, and every
unrecognized status classify as `Free`.  When validation is unavailable, a
previously validated active entitlement may be `PaidOfflineGrace` for at most
seven days after `last_validated_at`; local time still makes `expires_at`
effective while offline.  The client launches from this sealed cache with no
network dependency, then refreshes periodically.

## Product floor and optional modules

Paid-equivalent means only `Paid` and bounded `PaidOfflineGrace`.  It may
unlock optional Pro features, but entitlement state must never gate protected
text, encryption, decryption, receiving, local history, or the word-bank
carrier.  Lapse, revocation, malformed cache, and unavailable validation
degrade optional features to Free without destroying data or breaking the
encryption path.

Under D44 and D49, the local AI model is an optional Pro component; cloud
generation additionally requires separate explicit consent.  A Free user, or
a Pro user who declined or removed optional AI, retains the fully working
word-bank carrier and encryption path.  Credits are a separate balance, not an
entitlement, and must not renew, extend, or imply Pro.

## AI tier

The word-bank carrier always ships and is the carrier for every Free state,
including a lapsed or revoked former Pro entitlement.  Local AI and cloud
generation are Pro-only optional carriers.  Cloud generation additionally
requires a separate, explicit consent; activating Pro is never cloud consent.

AI selection must have one entitlement seam: when a requested AI carrier is
not entitled, unavailable, or not consented, it selects the word-bank carrier
instead of failing encryption, sending, or delivery.  Lapse never deletes an
already-installed optional AI model; it merely removes its use until Pro is
active again.  The user may remove that optional component, which has the same
word-bank fallback.

## Publish gate

The following `data/pricing.json` conditions are copied verbatim.  The claim
“Your month starts when you enter the code.” remains blocked until all four
are true:

1. the keyserver records a redemption timestamp on an explicit redeem call, not as a side effect of validation
2. the entitlement period is computed from that timestamp
3. /v1/license/validate returns EXPIRED after the period ends
4. the app degrades Pro features to Free at expiry without breaking the encryption path

Per D54, T11 deploys the checkout pause before this contract permits checkout
to be unpaused.  Unpausing additionally requires the implementation and its
CI proof for all four conditions above; a document or operator assertion is
not sufficient.
