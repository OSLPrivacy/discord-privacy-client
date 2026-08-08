import type { PaidCheckoutRecord, PaymentContact } from "./payment-taken-code-coming";

/**
 * Fixed data for the TASK 3200 screen.
 *
 * The amount and currency are the ones gate 3192 repairs on the server: a
 * five-dollar one-time Pro checkout, `amount_cents = 500`, `currency = 'usd'`.
 * `paidAt` is a fixed unix second (2026-08-07T14:32:09Z) so the date the check
 * reads is the same date on every machine.
 *
 * `PAID_WITH_CODE_RECORD` is the same payment after the code landed, and it is
 * the fixture that must draw nothing: a screen that appeared for it would be
 * telling someone holding a working code that their code has not arrived.
 */
export const PAID_NO_CODE_RECORD: PaidCheckoutRecord = {
  reference: "pi_3RQ8f2LdPro00042",
  amountCents: 500,
  currency: "usd",
  paidAt: 1786113129,
  code: null,
} as const;

export const PAID_WITH_CODE_RECORD: PaidCheckoutRecord = {
  ...PAID_NO_CODE_RECORD,
  code: "OSL-7QK4-2M9X-5RTB-8WZC",
} as const;

/** What the fixture's amount and date must read as on screen. */
export const PAID_NO_CODE_AMOUNT_TEXT = "$5.00";
export const PAID_NO_CODE_DATE_TEXT = "7 August 2026";

/**
 * Where "get in touch" goes.
 *
 * This is the one customer-facing help route the repository actually has --
 * `payment-help.html` at the site root, the page `cancel.html` already sends
 * people to. The screen takes the contact as data rather than holding an
 * address of its own, so shipping a real support address later is a change to
 * this constant and nothing else, and no address is invented in the meantime.
 */
export const OSL_PAYMENT_CONTACT: PaymentContact = {
  label: "OSL payment help",
  href: "https://oslprivacy.com/payment-help.html",
} as const;
