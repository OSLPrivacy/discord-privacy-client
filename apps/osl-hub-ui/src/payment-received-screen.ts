import "./payment-received-screen.css";

/**
 * TASK 3724 - connect the missing-code recovery screen.
 *
 * Builds on TASK 3200 (`payment-taken-code-coming.ts`), which is what a buyer
 * sees while gate 3192's automatic repair is still running. This module is
 * the screen a buyer reaches to recover a code by hand: "Payment received",
 * holding Find my code, Contact support, and Download OSL, plus the
 * recovered code itself once it exists.
 *
 * The recovery this screen triggers is gate 3723's
 * `repairPaidOneTimeCheckoutClaimWithoutCode`, wired to
 * `POST /v1/checkout/claim` -- which only ever returns a code after the
 * caller proves the checkout with its `session_id` and `claim_token`. This
 * screen mirrors that rule: `recoveredCodeForDisplay` returns a code only for
 * a paid order carrying that proof, never for a paid order without it, no
 * matter what the order record otherwise holds.
 */

/** The session_id + claim_token pair gate 3723's endpoint requires as proof of purchase. */
export interface BuyerProof {
  readonly sessionId: string;
  readonly claimToken: string;
}

/** One checkout as this screen knows it. */
export interface PaymentReceivedOrder {
  readonly reference: string;
  readonly paid: boolean;
  readonly code: string | null;
  readonly buyerProof: BuyerProof | null;
}

export interface PaymentReceivedScreenTree {
  readonly title: string;
  readonly controls: readonly string[];
}

export const PAYMENT_RECEIVED_TITLE = "Payment received";

export const PAYMENT_RECEIVED_FIND_CODE_LABEL = "Find my code";
export const PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL = "Contact support";
export const PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL = "Download OSL";

/** The one customer-facing help route this repo has; `payment-taken-code-coming.ts` and `cancel.html` already point here. */
export const PAYMENT_RECEIVED_SUPPORT_HREF = "https://oslprivacy.com/payment-help.html";
export const PAYMENT_RECEIVED_DOWNLOAD_HREF = "https://oslprivacy.com/download.html";

/**
 * The code this screen is allowed to show. A paid order with no buyer proof
 * -- no proof object, or a proof missing either half -- never shows a code,
 * even if `order.code` already holds one; an unpaid or blank-code order
 * has nothing to show either.
 */
export function recoveredCodeForDisplay(order: PaymentReceivedOrder): string | null {
  if (!order.paid) return null;
  if (!order.buyerProof) return null;
  if (!order.buyerProof.sessionId.trim() || !order.buyerProof.claimToken.trim()) return null;
  const code = order.code;
  return code && code.trim().length > 0 ? code : null;
}

/**
 * The screen tree the finish line checks: the three controls, always in this
 * order, plus the recovered code as a fourth item when (and only when) there
 * is one to show.
 */
export function paymentReceivedScreenTree(order: PaymentReceivedOrder): PaymentReceivedScreenTree {
  const controls: string[] = [
    PAYMENT_RECEIVED_FIND_CODE_LABEL,
    PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL,
    PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL,
  ];
  const code = recoveredCodeForDisplay(order);
  if (code) controls.push(code);
  return { title: PAYMENT_RECEIVED_TITLE, controls };
}

export type PaymentReceivedEvent =
  | { kind: "find-code" }
  | { kind: "contact-support" }
  | { kind: "download-osl" };

/** Maps a clicked control (`data-payment-received-action`) to an event; the caller owns the actual IPC/network call. */
export function paymentReceivedEventForAction(
  action: string | undefined | null,
): PaymentReceivedEvent | null {
  switch (action) {
    case "find-code":
      return { kind: "find-code" };
    case "contact-support":
      return { kind: "contact-support" };
    case "download-osl":
      return { kind: "download-osl" };
    default:
      return null;
  }
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;")
    .replace(/'/gu, "&#39;");
}

function codeMarkup(order: PaymentReceivedOrder): string {
  const code = recoveredCodeForDisplay(order);
  if (code) {
    const safeCode = escapeHtml(code);
    return `<div class="pr-code-row"><span class="pr-code-label">Your code</span><strong class="pr-code-value" data-payment-received-code="${safeCode}">${safeCode}</strong></div>`;
  }
  return `<p class="pr-code-empty" data-payment-received-code="">No code to show yet. Use Find my code once you have your payment receipt to hand.</p>`;
}

export function paymentReceivedScreenMarkup(order: PaymentReceivedOrder): string {
  const title = escapeHtml(PAYMENT_RECEIVED_TITLE);
  const reference = escapeHtml(order.reference);
  return `<section class="pr-screen" aria-labelledby="pr-heading">
    <h1 id="pr-heading" tabindex="-1" class="pr-title">${title}</h1>
    <p class="pr-reference">Order ${reference}</p>
    ${codeMarkup(order)}
    <div class="pr-actions">
      <button class="button primary pr-find" type="button" data-payment-received-action="find-code">${PAYMENT_RECEIVED_FIND_CODE_LABEL}</button>
      <a class="button ghost pr-contact" data-payment-received-action="contact-support" href="${PAYMENT_RECEIVED_SUPPORT_HREF}">${PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL}</a>
      <a class="button ghost pr-download" data-payment-received-action="download-osl" href="${PAYMENT_RECEIVED_DOWNLOAD_HREF}">${PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL}</a>
    </div>
  </section>`;
}
