/**
 * TASK 3200 - the "payment taken, code coming" screen.
 *
 * Gate 3192 is the server side of the same fault: a completed Stripe checkout
 * whose `licenses` row never appeared, found and repaired by
 * `repairPaidStripeCheckoutsMissingCodes`. That repair runs on the keyserver
 * and takes some seconds to some minutes to land. This module is what the
 * person staring at the app during those minutes sees -- the money has left
 * their account and no code has arrived, which is the exact moment someone
 * pays a second time.
 *
 * So the screen holds four things, and the check next door requires all four:
 * the amount that was taken, the day it was taken, one plain sentence saying
 * the code is coming, and a line telling them how to get in touch. Everything
 * else on it is the "what to do next" list, whose first job is to say plainly
 * that paying again is the wrong move.
 *
 * The screen is never drawn from a guess. `paymentTakenCodeComingView` is the
 * only way to make one, and it returns `null` for every record except a paid
 * one with no code -- no record, a zero-amount record, or a record whose code
 * has arrived all produce nothing to draw. Drawing is a pure function of that
 * view, same shape as `renderAccountScreen` and `renderFriendPicturesScreen`.
 */

/**
 * One paid checkout as the app knows it.
 *
 * The field names follow what gate 3192 reads on the server:
 * `commerce_events.amount_cents`, `.currency` (lowercase ISO 4217) and
 * `.occurred_at` (unix seconds), plus whether a code exists for it yet.
 * `reference` is the payment identifier a person can quote when they get in
 * touch; it is a receipt number, not a claim token, and is safe on screen.
 */
export interface PaidCheckoutRecord {
  readonly reference: string;
  readonly amountCents: number;
  readonly currency: string;
  readonly paidAt: number;
  readonly code: string | null;
}

/** How to get in touch, supplied by the caller rather than invented here. */
export interface PaymentContact {
  readonly label: string;
  readonly href: string;
}

export interface PaymentTakenCodeComingView {
  readonly amount: string;
  readonly date: string;
  readonly reference: string;
  readonly contact: PaymentContact;
}

export const PAYMENT_TAKEN_TITLE = "Payment taken, code coming";

/** The one plain sentence the finish line asks for. */
export const CODE_COMING_SENTENCE = "Your payment went through and your code is coming.";

/**
 * What to do next, in the order the screen lists it. The second step is the
 * point of the whole screen: the failure this covers costs money only if the
 * person reads a missing code as a failed payment and pays again.
 */
export const CODE_COMING_NEXT_STEPS: readonly string[] = [
  "Leave OSL open. The code appears here on its own as soon as it arrives.",
  "Do not pay again. A second payment takes more money and does not make this code arrive any sooner.",
  "If the code has not arrived within one day, use the line below to get in touch.",
] as const;

const MONTHS: readonly string[] = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/**
 * Currency symbols for the currencies OSL actually charges in. Anything else
 * is printed as "12.34 CAD" rather than guessed at, and no `Intl` locale data
 * is consulted: the string on this screen has to be the same string in a test,
 * on a Linux desktop and in a screenshot, whatever the machine's locale is.
 */
const CURRENCY_SYMBOLS: Readonly<Record<string, string>> = {
  usd: "$",
  eur: "€",
  gbp: "£",
};

export function formatPaidAmount(amountCents: number, currency: string): string {
  if (!Number.isSafeInteger(amountCents) || amountCents <= 0) {
    throw new Error(`Not a paid amount: ${amountCents}`);
  }
  const code = currency.trim().toLowerCase();
  if (!/^[a-z]{3}$/u.test(code)) throw new Error(`Not a currency: ${currency}`);
  const units = Math.floor(amountCents / 100);
  const cents = String(amountCents % 100).padStart(2, "0");
  const symbol = CURRENCY_SYMBOLS[code];
  return symbol ? `${symbol}${units}.${cents}` : `${units}.${cents} ${code.toUpperCase()}`;
}

/**
 * The day the payment landed, as "7 August 2026".
 *
 * `paidAt` is stamped by the payment company in UTC, and the date is read back
 * in UTC too. A day boundary crossed by a timezone would move the date under a
 * person who reopened the app in another country, and this line exists so they
 * can match it against their bank statement, which is dated the same way.
 */
export function formatPaidDate(paidAt: number): string {
  if (!Number.isSafeInteger(paidAt) || paidAt <= 0) {
    throw new Error(`Not a payment time: ${paidAt}`);
  }
  const when = new Date(paidAt * 1000);
  return `${when.getUTCDate()} ${MONTHS[when.getUTCMonth()]} ${when.getUTCFullYear()}`;
}

function isCodeMissing(code: string | null): boolean {
  return code === null || code.trim().length === 0;
}

/**
 * The screen's whole visibility rule.
 *
 * `null` means "draw nothing", and that is the answer for every record except
 * a paid one whose code has not arrived: no record at all, a record that took
 * no money, or a record that already carries a code.
 */
export function paymentTakenCodeComingView(
  record: PaidCheckoutRecord | null | undefined,
  contact: PaymentContact,
): PaymentTakenCodeComingView | null {
  if (!record) return null;
  if (!Number.isSafeInteger(record.amountCents) || record.amountCents <= 0) return null;
  if (!isCodeMissing(record.code)) return null;
  if (contact.label.trim().length === 0 || contact.href.trim().length === 0) {
    throw new Error("A payment-taken screen needs a way to get in touch");
  }
  return {
    amount: formatPaidAmount(record.amountCents, record.currency),
    date: formatPaidDate(record.paidAt),
    reference: record.reference.trim(),
    contact,
  };
}

/** True when the app should show this screen for `record`. */
export function shouldShowPaymentTakenCodeComing(
  record: PaidCheckoutRecord | null | undefined,
  contact: PaymentContact,
): boolean {
  return paymentTakenCodeComingView(record, contact) !== null;
}

export function contactLine(view: PaymentTakenCodeComingView): string {
  const reference = view.reference.length > 0 ? ` Quote payment reference ${view.reference}.` : "";
  return `Still no code? Get in touch through ${view.contact.label}.${reference}`;
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function factsMarkup(view: PaymentTakenCodeComingView): string {
  return [
    `<dl class="payment-taken-facts">`,
    `<div class="payment-taken-fact">`,
    `<dt class="payment-taken-fact-name">Amount taken</dt>`,
    `<dd class="payment-taken-fact-value" data-payment-taken-amount="${escapeHtml(view.amount)}">${escapeHtml(view.amount)}</dd>`,
    `</div>`,
    `<div class="payment-taken-fact">`,
    `<dt class="payment-taken-fact-name">Date</dt>`,
    `<dd class="payment-taken-fact-value" data-payment-taken-date="${escapeHtml(view.date)}">${escapeHtml(view.date)}</dd>`,
    `</div>`,
    `<div class="payment-taken-fact">`,
    `<dt class="payment-taken-fact-name">Payment reference</dt>`,
    `<dd class="payment-taken-fact-value payment-taken-reference" data-payment-taken-reference="${escapeHtml(view.reference)}">${escapeHtml(view.reference)}</dd>`,
    `</div>`,
    `</dl>`,
  ].join("");
}

function nextStepsMarkup(): string {
  const steps = CODE_COMING_NEXT_STEPS.map(
    (step, index) =>
      `<li class="payment-taken-step" data-payment-taken-step="${index + 1}">${escapeHtml(step)}</li>`,
  ).join("");
  return [
    `<section class="payment-taken-next" aria-labelledby="payment-taken-next-heading">`,
    `<h3 class="payment-taken-next-heading" id="payment-taken-next-heading">What to do next</h3>`,
    `<ol class="payment-taken-steps">${steps}</ol>`,
    `<button type="button" class="payment-taken-action" data-payment-taken-action="check-again">Check again</button>`,
    `</section>`,
  ].join("");
}

function contactMarkup(view: PaymentTakenCodeComingView): string {
  const line = contactLine(view);
  const [before, ...rest] = line.split(view.contact.label);
  const after = rest.join(view.contact.label);
  const link = `<a class="payment-taken-contact-link" data-payment-taken-contact-href="${escapeHtml(view.contact.href)}" href="${escapeHtml(view.contact.href)}">${escapeHtml(view.contact.label)}</a>`;
  return [
    `<p class="payment-taken-contact" data-payment-taken-contact="${escapeHtml(line)}">`,
    escapeHtml(before),
    link,
    escapeHtml(after),
    `</p>`,
  ].join("");
}

/** Draw the screen for a view. `null` draws nothing at all. */
export function renderPaymentTakenCodeComing(view: PaymentTakenCodeComingView | null): string {
  if (view === null) return "";
  return [
    `<section class="payment-taken-screen" data-payment-taken-screen="shown" aria-label="${PAYMENT_TAKEN_TITLE}">`,
    `<header class="payment-taken-header">`,
    `<h2 class="payment-taken-heading">${PAYMENT_TAKEN_TITLE}</h2>`,
    `<p class="payment-taken-sentence" data-payment-taken-sentence="${escapeHtml(CODE_COMING_SENTENCE)}">${escapeHtml(CODE_COMING_SENTENCE)}</p>`,
    `</header>`,
    factsMarkup(view),
    nextStepsMarkup(),
    contactMarkup(view),
    `</section>`,
  ].join("");
}

/**
 * Draw the screen for a record, or nothing when the record is not a paid one
 * missing its code. This is the entry point the app routes through, so the
 * visibility rule cannot be walked around by rendering a view directly.
 */
export function renderPaymentTakenCodeComingForRecord(
  record: PaidCheckoutRecord | null | undefined,
  contact: PaymentContact,
): string {
  return renderPaymentTakenCodeComing(paymentTakenCodeComingView(record, contact));
}

/**
 * Mount the screen and redraw it whenever the record changes. Same shape as
 * `attachFriendPicturesScreen`: one mount, one redraw, no state of its own.
 * A record whose code has arrived empties the mount, so the screen leaves as
 * soon as the thing it is about is over.
 */
export function attachPaymentTakenCodeComing(
  mount: HTMLElement,
  record: PaidCheckoutRecord | null,
  contact: PaymentContact,
  handlers: { onCheckAgain?: () => void } = {},
): (next: PaidCheckoutRecord | null) => void {
  let current = record;
  const draw = (): void => {
    mount.innerHTML = renderPaymentTakenCodeComingForRecord(current, contact);
  };
  mount.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    const button = target?.closest?.("[data-payment-taken-action]") as HTMLElement | null;
    if (button?.dataset.paymentTakenAction === "check-again") handlers.onCheckAgain?.();
  });
  draw();
  return (next: PaidCheckoutRecord | null): void => {
    current = next;
    draw();
  };
}
