/**
 * TASK 3200 - the payment-taken, code-coming screen.
 *
 * The finish line is four things on the screen -- the paid amount, the date, a
 * plain sentence saying the code is coming, and a contact line -- plus the
 * rule that the screen appears only for a paid record with no code. The last
 * one is the one that can quietly pass while broken, so it is checked as a
 * table: every record shape the app can hold, and what the screen does with
 * it. A screen that always drew would fail four of those five rows.
 */
import { describe, expect, it } from "vitest";

import {
  CODE_COMING_NEXT_STEPS,
  CODE_COMING_SENTENCE,
  PAYMENT_TAKEN_TITLE,
  type PaidCheckoutRecord,
  contactLine,
  formatPaidAmount,
  formatPaidDate,
  paymentTakenCodeComingView,
  renderPaymentTakenCodeComing,
  renderPaymentTakenCodeComingForRecord,
  shouldShowPaymentTakenCodeComing,
} from "./payment-taken-code-coming";
import {
  OSL_PAYMENT_CONTACT,
  PAID_NO_CODE_AMOUNT_TEXT,
  PAID_NO_CODE_DATE_TEXT,
  PAID_NO_CODE_RECORD,
  PAID_WITH_CODE_RECORD,
} from "./payment-taken-code-coming-data";

const screen = (record: PaidCheckoutRecord | null = PAID_NO_CODE_RECORD): string =>
  renderPaymentTakenCodeComingForRecord(record, OSL_PAYMENT_CONTACT);

function visibleText(html: string): string {
  return html
    .replace(/<[^>]+>/gu, " ")
    .replace(/&amp;/gu, "&")
    .replace(/&#39;/gu, "'")
    .replace(/&quot;/gu, '"')
    .replace(/\s+/gu, " ")
    // A tag boundary is not a space: "help</a>." must read back as "help.".
    .replace(/\s+([.,;:?!])/gu, "$1")
    .trim();
}

describe("what the screen shows", () => {
  it("shows the paid amount", () => {
    const text = visibleText(screen());
    console.log(`TASK3200 amount_text=${PAID_NO_CODE_AMOUNT_TEXT}`);
    expect(formatPaidAmount(PAID_NO_CODE_RECORD.amountCents, PAID_NO_CODE_RECORD.currency)).toBe(
      PAID_NO_CODE_AMOUNT_TEXT,
    );
    expect(text).toContain(PAID_NO_CODE_AMOUNT_TEXT);
    expect(screen()).toContain(`data-payment-taken-amount="${PAID_NO_CODE_AMOUNT_TEXT}"`);
  });

  it("shows the date the money was taken", () => {
    const text = visibleText(screen());
    console.log(`TASK3200 date_text=${PAID_NO_CODE_DATE_TEXT}`);
    expect(formatPaidDate(PAID_NO_CODE_RECORD.paidAt)).toBe(PAID_NO_CODE_DATE_TEXT);
    expect(text).toContain(PAID_NO_CODE_DATE_TEXT);
    expect(screen()).toContain(`data-payment-taken-date="${PAID_NO_CODE_DATE_TEXT}"`);
  });

  it("says the code is coming, in one plain sentence", () => {
    const text = visibleText(screen());
    console.log(`TASK3200 sentence="${CODE_COMING_SENTENCE}"`);
    console.log(`TASK3200 sentence_words=${CODE_COMING_SENTENCE.split(/\s+/u).length}`);
    expect(text).toContain(CODE_COMING_SENTENCE);
    // One sentence: one full stop, and it is the last character.
    expect(CODE_COMING_SENTENCE.match(/[.!?]/gu)).toHaveLength(1);
    expect(CODE_COMING_SENTENCE.endsWith(".")).toBe(true);
    // It has to say the code is coming, not merely mention a code.
    expect(CODE_COMING_SENTENCE).toMatch(/\bcode is coming\b/u);
    // Plain: short, and none of the words a person did not buy.
    expect(CODE_COMING_SENTENCE.split(/\s+/u).length).toBeLessThanOrEqual(14);
    expect(CODE_COMING_SENTENCE).not.toMatch(
      /licen[cs]e|entitlement|webhook|session|claim|provision|asynchronous|reconcil/iu,
    );
  });

  it("shows a contact line that leads somewhere", () => {
    const html = screen();
    const view = paymentTakenCodeComingView(PAID_NO_CODE_RECORD, OSL_PAYMENT_CONTACT);
    expect(view).not.toBeNull();
    const line = contactLine(view!);
    console.log(`TASK3200 contact_line="${line}"`);
    console.log(`TASK3200 contact_href=${OSL_PAYMENT_CONTACT.href}`);
    expect(visibleText(html)).toContain(line);
    expect(html).toContain(`href="${OSL_PAYMENT_CONTACT.href}"`);
    expect(line).toContain(OSL_PAYMENT_CONTACT.label);
    expect(line).toContain(PAID_NO_CODE_RECORD.reference);
    expect(line).toMatch(/get in touch/iu);
  });

  it("tells the person what to do next, starting with not paying twice", () => {
    const text = visibleText(screen());
    for (const step of CODE_COMING_NEXT_STEPS) expect(text).toContain(step);
    console.log(`TASK3200 next_steps=${CODE_COMING_NEXT_STEPS.length}`);
    expect(text).toContain("What to do next");
    expect(text).toMatch(/Do not pay again/u);
    expect(screen()).toContain('data-payment-taken-action="check-again"');
    expect(text).toContain(PAYMENT_TAKEN_TITLE);
  });
});

describe("when the screen appears", () => {
  const cases: readonly {
    readonly name: string;
    readonly record: PaidCheckoutRecord | null;
    readonly shown: boolean;
  }[] = [
    { name: "paid, no code", record: PAID_NO_CODE_RECORD, shown: true },
    // A blank code string is no code: it is what a half-written row holds.
    { name: "paid, blank code", record: { ...PAID_NO_CODE_RECORD, code: "   " }, shown: true },
    { name: "paid, code arrived", record: PAID_WITH_CODE_RECORD, shown: false },
    { name: "no money taken", record: { ...PAID_NO_CODE_RECORD, amountCents: 0 }, shown: false },
    { name: "no paid record", record: null, shown: false },
  ];

  it("appears only for a paid record with no code", () => {
    for (const item of cases) {
      const html = screen(item.record);
      const shown = html.includes('data-payment-taken-screen="shown"');
      console.log(`TASK3200 case="${item.name}" shown=${shown ? "yes" : "no"} html_length=${html.length}`);
      expect(shown).toBe(item.shown);
      expect(shouldShowPaymentTakenCodeComing(item.record, OSL_PAYMENT_CONTACT)).toBe(item.shown);
      if (!item.shown) expect(html).toBe("");
    }
    const shownCount = cases.filter((item) => item.shown).length;
    console.log(`TASK3200 cases=${cases.length} shown=${shownCount} hidden=${cases.length - shownCount}`);
    expect(shownCount).toBe(2);
    // Every hidden case is hidden for one of the two reasons that matter:
    // a code is already in hand, or no money was taken.
    for (const item of cases.filter((entry) => !entry.shown)) {
      const hasCode = (item.record?.code ?? "").trim().length > 0;
      const tookMoney = (item.record?.amountCents ?? 0) > 0;
      expect(hasCode || !tookMoney).toBe(true);
    }
  });

  it("draws nothing at all when there is no view", () => {
    expect(renderPaymentTakenCodeComing(null)).toBe("");
    expect(paymentTakenCodeComingView(PAID_WITH_CODE_RECORD, OSL_PAYMENT_CONTACT)).toBeNull();
  });

  it("refuses a screen with no way to get in touch", () => {
    expect(() =>
      paymentTakenCodeComingView(PAID_NO_CODE_RECORD, { label: "  ", href: "" }),
    ).toThrow("A payment-taken screen needs a way to get in touch");
  });
});

describe("amount and date formatting", () => {
  it("prints cents, symbols, and unknown currencies without guessing", () => {
    expect(formatPaidAmount(500, "usd")).toBe("$5.00");
    expect(formatPaidAmount(1234, "eur")).toBe("€12.34");
    expect(formatPaidAmount(1005, "gbp")).toBe("£10.05");
    expect(formatPaidAmount(750, "cad")).toBe("7.50 CAD");
    expect(() => formatPaidAmount(0, "usd")).toThrow("Not a paid amount: 0");
    expect(() => formatPaidAmount(500, "dollars")).toThrow("Not a currency: dollars");
  });

  it("reads the payment day in UTC, whatever the machine thinks", () => {
    expect(formatPaidDate(1786113129)).toBe("7 August 2026");
    expect(formatPaidDate(1)).toBe("1 January 1970");
    expect(() => formatPaidDate(0)).toThrow("Not a payment time: 0");
  });
});
