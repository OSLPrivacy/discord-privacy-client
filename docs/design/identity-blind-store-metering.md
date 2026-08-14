# Identity-blind store metering

Status: design note for owner decision. This does not choose an implementation.

## Existing boundary

The cipher-store is deliberately identity-blind. `PUT /v1/blob` accepts a
short-lived signed upload grant before it reads the body or writes blob state,
but the grant is not an account credential.
The store upload grant fields are exactly aud, exp and jti.

That boundary is the hard problem for metering. A normal server-enforced
allowance debits an account after each byte is accepted. If the store performs
that debit directly, the store learns who owns the blob. If the store does not
perform that debit, some other party must either trust the client-reported size
or receive enough redemption data to attach the store's byte count to an
account.

## Design 1: anonymous prepaid vouchers

The keyserver sells denominated credit, then issues unlinkable prepaid vouchers.
The client spends enough vouchers with an upload, and the store redeems the
vouchers without learning who bought them. The voucher can be a blind-signed
serial with a denomination and expiry, or an equivalent unlinkable e-cash note.
The store verifies the issuer signature, checks the voucher serial against a
spent set, enforces that accepted bytes do not exceed the presented
denominations, and consumes the voucher serials.

Keyserver knows:
Payment account or checkout holder when the purchase is account-based; payment
status; denomination schedule sold; issuance time; expiry policy; and aggregate
outstanding liability. With blind issuance, it does not know the unblinded
voucher serials later presented to the store.

Store knows:
The anonymous upload grant with aud, exp and jti; the voucher denomination,
expiry, issuer signature, and spend serial; the observed upload byte count; the
blob metadata it already stores; and whether a voucher serial has already been
spent. It does not learn account id, buyer identity, Stripe customer, crypto
invoice claimant, or payment account.

Stripe or the crypto invoice path knows:
Stripe knows the card-side customer/payment/session data and the purchased
credit SKU or denomination, but not which blob receives the bytes. The crypto
invoice path can avoid account identity in the same style as the existing
anonymous crypto invoice precedent in `keyserver-cf/src/lib/anonymous-crypto.ts`,
`keyserver-cf/src/endpoints/crypto-checkout.ts`,
`keyserver-cf/src/endpoints/crypto-status.ts`,
`keyserver-cf/src/endpoints/crypto-settlement.ts`,
`keyserver-cf/migrations/0008_anonymous_crypto_invoices.sql`,
`keyserver-cf/migrations/0013_crypto_payment_reference_single_use.sql`, and
`keyserver-cf/migrations/0023_durable_crypto_commerce.sql`: invoice id, claim
hash, asset, amount, settlement event, delivery key/ciphertext, and aggregate
commerce records exist without retaining email, txid, service account, or blob
identity.

Client meter computes:
The sealed byte count before upload; the voucher denominations needed to cover
that count; local remaining prepaid balance; change handling when denominations
do not exactly match; and a predicted spend display. The store's observed byte
count remains authoritative for acceptance.

Tradeoff:
This preserves the store's identity blindness and avoids a per-account usage
profile at the store. It adds e-cash complexity: blind issuance, denomination
choice, serial-spend storage, expiry, lost-voucher handling, refund handling,
and a decision about whether partially spent value gets change or burns whole
denominations.

## Design 2: account-tied ledger

The keyserver owns a per-account byte ledger. For each upload, the client asks
the keyserver for a normal anonymous store grant. The grant still carries only
aud, exp and jti, but the keyserver records jti against the account and expected
spend. The store accepts the upload with the anonymous grant, then reports jti
and observed byte count to the keyserver. The keyserver debits the account using
its jti-to-account issuance record.

Keyserver knows:
Account id; payment state; byte allowance; grant issuance time; jti; expected
upload size if supplied before issuance; store-reported byte count; debit time;
remaining balance; and failed or replayed redemption attempts that reach the
ledger. This creates the per-account byte-spend profile.

Store knows:
The anonymous upload grant with aud, exp and jti; the observed upload byte
count; the blob metadata it already stores; replay status for jti; and whether
the keyserver accepted the redemption callback if callbacks are synchronous. It
does not need account id in the grant, but it participates in producing an
account-linkable jti and byte-count record at the keyserver.

Stripe or the crypto invoice path knows:
Stripe knows the card-side customer/payment/session data and the product that
funds the ledger. The crypto invoice path knows the invoice and settlement
facts needed to create ledger credit. Once credit is assigned to an account,
the payment path and keyserver account state can be joined by the keyserver,
even if the store never sees that join.

Client meter computes:
The sealed byte count before upload; whether local cached allowance appears
sufficient; the requested debit size; retry/idempotency state for a grant whose
upload may have reached the store; and user-visible remaining allowance after
the keyserver confirms redemption. The keyserver/store redemption result is
authoritative.

Tradeoff:
This is mechanically simpler and gives straightforward server enforcement,
refund, chargeback, and abuse controls. It does not teach the store who owns a
blob, but it does teach the keyserver which account spent each byte whenever
the store redeems a jti.

## Decision boundary

Both designs can keep account identifiers out of the store grant and out of the
store's blob rows. They differ on who can answer the byte-spend question after
the fact. Anonymous prepaid vouchers make the answer "no single server can join
buyer to blob spend without extra correlation." Account-tied ledger makes the
answer "the keyserver can join account, jti, and store-observed byte count."

DECISION NEEDED: WHO MAY KNOW WHICH ACCOUNT SPENT A BYTE.
