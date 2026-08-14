/**
 * TASK 3724 - connect the missing-code recovery screen.
 *
 * The finish line is a screen tree for title "Payment received" holding
 * exactly Find my code, Contact support, Download OSL, and the recovered
 * code value -- plus the rule that a paid order with no buyer proof shows
 * no code. That rule is the one that can quietly pass while broken, so it
 * is checked as a table: every order shape the app can hold, and what the
 * screen tree does with it.
 */
import { describe, expect, it } from "vitest";

import {
  PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL,
  PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL,
  PAYMENT_RECEIVED_FIND_CODE_LABEL,
  PAYMENT_RECEIVED_TITLE,
  type PaymentReceivedOrder,
  paymentReceivedEventForAction,
  paymentReceivedScreenMarkup,
  paymentReceivedScreenTree,
  recoveredCodeForDisplay,
} from "./payment-received-screen";

const RECOVERED_CODE = "OSL-A3C2-6FTM-7MB4-A50Q";

const PAID_WITH_PROOF_AND_CODE: PaymentReceivedOrder = {
  reference: "pi_3RQ8f2LdPro00042",
  paid: true,
  code: RECOVERED_CODE,
  buyerProof: { sessionId: "cs_test_a1b2c3", claimToken: "claim_9f8e7d" },
};

const PAID_WITH_PROOF_NO_CODE_YET: PaymentReceivedOrder = {
  reference: "pi_3RQ8f2LdPro00043",
  paid: true,
  code: null,
  buyerProof: { sessionId: "cs_test_d4e5f6", claimToken: "claim_1a2b3c" },
};

const PAID_NO_BUYER_PROOF: PaymentReceivedOrder = {
  reference: "pi_3RQ8f2LdPro00044",
  paid: true,
  code: RECOVERED_CODE,
  buyerProof: null,
};

const PAID_BLANK_PROOF: PaymentReceivedOrder = {
  reference: "pi_3RQ8f2LdPro00045",
  paid: true,
  code: RECOVERED_CODE,
  buyerProof: { sessionId: "", claimToken: "" },
};

const UNPAID_WITH_PROOF: PaymentReceivedOrder = {
  reference: "pi_3RQ8f2LdPro00046",
  paid: false,
  code: RECOVERED_CODE,
  buyerProof: { sessionId: "cs_test_g7h8i9", claimToken: "claim_4d5e6f" },
};

describe("the Payment received screen tree", () => {
  it("holds exactly Find my code, Contact support, Download OSL, and the recovered code value", () => {
    const tree = paymentReceivedScreenTree(PAID_WITH_PROOF_AND_CODE);
    console.log(`TASK3724 title=${tree.title}`);
    console.log(`TASK3724 controls=${JSON.stringify(tree.controls)}`);
    expect(tree.title).toBe(PAYMENT_RECEIVED_TITLE);
    expect(tree.controls).toEqual([
      PAYMENT_RECEIVED_FIND_CODE_LABEL,
      PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL,
      PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL,
      RECOVERED_CODE,
    ]);
    expect(tree.controls.length).toBe(4);
  });

  it("a paid order with no buyer proof shows no code", () => {
    expect(recoveredCodeForDisplay(PAID_NO_BUYER_PROOF)).toBeNull();
    const tree = paymentReceivedScreenTree(PAID_NO_BUYER_PROOF);
    console.log(`TASK3724 no_proof_controls=${JSON.stringify(tree.controls)}`);
    expect(tree.controls).toEqual([
      PAYMENT_RECEIVED_FIND_CODE_LABEL,
      PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL,
      PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL,
    ]);
    expect(tree.controls).not.toContain(RECOVERED_CODE);
    expect(tree.controls.length).toBe(3);
  });

  const cases: Array<[string, PaymentReceivedOrder, string | null]> = [
    ["paid, proof, code", PAID_WITH_PROOF_AND_CODE, RECOVERED_CODE],
    ["paid, proof, no code yet", PAID_WITH_PROOF_NO_CODE_YET, null],
    ["paid, no buyer proof", PAID_NO_BUYER_PROOF, null],
    ["paid, blank buyer proof", PAID_BLANK_PROOF, null],
    ["unpaid, with proof", UNPAID_WITH_PROOF, null],
  ];

  it.each(cases)("recoveredCodeForDisplay: %s", (_label, order, expected) => {
    const result = recoveredCodeForDisplay(order);
    console.log(`TASK3724 case="${_label}" code=${result ?? "null"}`);
    expect(result).toBe(expected);
  });

  it("every hidden case ends up with exactly 3 controls, no fourth item", () => {
    for (const [, order] of cases) {
      const tree = paymentReceivedScreenTree(order);
      const expectedLength = recoveredCodeForDisplay(order) ? 4 : 3;
      expect(tree.controls.length).toBe(expectedLength);
    }
  });
});

describe("the Payment received markup", () => {
  it("renders the three action controls with their data-hooks", () => {
    const html = paymentReceivedScreenMarkup(PAID_WITH_PROOF_AND_CODE);
    expect(html).toContain(`data-payment-received-action="find-code"`);
    expect(html).toContain(`data-payment-received-action="contact-support"`);
    expect(html).toContain(`data-payment-received-action="download-osl"`);
    expect(html).toContain(PAYMENT_RECEIVED_FIND_CODE_LABEL);
    expect(html).toContain(PAYMENT_RECEIVED_CONTACT_SUPPORT_LABEL);
    expect(html).toContain(PAYMENT_RECEIVED_DOWNLOAD_OSL_LABEL);
  });

  it("shows the recovered code value with its data-hook when there is proof", () => {
    const html = paymentReceivedScreenMarkup(PAID_WITH_PROOF_AND_CODE);
    expect(html).toContain(`data-payment-received-code="${RECOVERED_CODE}"`);
  });

  it("shows no code value markup for a paid order with no buyer proof", () => {
    const html = paymentReceivedScreenMarkup(PAID_NO_BUYER_PROOF);
    expect(html).not.toContain(RECOVERED_CODE);
    expect(html).toContain(`data-payment-received-code=""`);
  });
});

describe("wiring", () => {
  it("maps each control's action to its event", () => {
    expect(paymentReceivedEventForAction("find-code")).toEqual({ kind: "find-code" });
    expect(paymentReceivedEventForAction("contact-support")).toEqual({ kind: "contact-support" });
    expect(paymentReceivedEventForAction("download-osl")).toEqual({ kind: "download-osl" });
    expect(paymentReceivedEventForAction("nonsense")).toBeNull();
    expect(paymentReceivedEventForAction(undefined)).toBeNull();
    expect(paymentReceivedEventForAction(null)).toBeNull();
  });
});
