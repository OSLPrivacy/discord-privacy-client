/// TASK 0001 — the single switch that decides whether OSL accepts money.
///
/// This is a PRE-LAUNCH CLOSE, not a removal. Every payment route, handler,
/// price ID, product config and migration stays exactly where it is. Nothing
/// here deletes a capability; it only refuses to start one. Re-opening paid
/// plans, crypto payments and donations at launch is flipping this one switch
/// back on and deploying -- not restoring code from git history.
///
/// Semantics: fail-closed. Only the exact string "true" opens payments.
/// Absent, empty, "TRUE", "1", "yes" and every other value keep them closed.
/// This mirrors the existing house idiom for reversible operational gates
/// (`LINK_GRANT_ENABLED`, `CRYPTO_BTC_ENABLED`, `CRYPTO_DONATION_XMR_ENABLED`).
///
/// Why the refusal lives here and not at the payment provider:
///
/// Deactivating Stripe products and prices is a useful second layer, but it is
/// NOT the boundary. Stripe Checkout accepts inline `line_items[0][price_data]`,
/// which references no product or price object at all -- the donation route
/// uses exactly that shape -- so a handler that merely calls Stripe would still
/// be able to charge a card with every product deactivated. The crypto routes
/// do not involve Stripe in any way: they ask the watcher for a fresh BTC/XMR
/// address, and money sent to that address is gone regardless of what Stripe is
/// configured to do. A shutdown that depends on the provider staying configured
/// a certain way is not a shutdown. So the refusal happens in OSL's own code,
/// before any session, charge, invoice, payment intent or crypto address is
/// created.
///
/// Callers MUST place `paymentsOpen()` as the first statement of the handler --
/// ahead of rate limiting, body parsing and configuration checks -- so a closed
/// deployment never reads a payment body or reaches an outbound call.
///
/// Deliberately NOT gated by this switch, because none of them start a payment:
///   - POST /v1/stripe/webhook        settles money already taken
///   - POST /v1/internal/crypto/settle  settles coins already sent
///   - POST /v1/crypto/status, /v1/donations/crypto/status   read-only
///   - POST /v1/checkout/claim        delivers an already-paid entitlement
/// Closing those would stop OSL hearing about an in-flight payment without
/// stopping the payment, which orphans the customer's money. Wrong order.

/** The one operator-facing name. Set in `wrangler.toml` under `[vars]`. */
export const PAYMENTS_OPEN_VAR = "PAYMENTS_OPEN";

/** Minimal structural type so callers need no extra import gymnastics. */
export interface PaymentsOpenEnv {
  /** Exact "true" opens payments; absent or anything else fails closed. */
  PAYMENTS_OPEN?: string;
}

/**
 * The single gate. Returns true only when the deployment has explicitly
 * opted in to accepting money.
 */
export function paymentsOpen(env: PaymentsOpenEnv): boolean {
  return env.PAYMENTS_OPEN === "true";
}

/**
 * Honest refusal text. Not a 404 (the route exists), not a 500 (nothing broke),
 * and not a provider error (Stripe was never contacted). It states plainly that
 * OSL is not taking payments and that nothing chargeable was created.
 */
export const PAYMENTS_CLOSED_MESSAGE =
  "OSL is not accepting payments at this time; no charge, invoice, or payment address was created";
