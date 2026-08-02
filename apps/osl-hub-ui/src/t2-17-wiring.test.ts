import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("T2-17 shipping receipt wiring", () => {
  it("renders the sender receipt projection from the production OSL Chat route", () => {
    expect(main).toMatch(/import\s*\{\s*senderReceiptStatus\s*\}\s*from\s*["']\.\/receipt-status["']/u);
    expect(main).toMatch(/function oslChatContent\(\): string[\s\S]*oslChatSenderReceiptMarkup/u);
    expect(main).toMatch(/function oslChatSenderReceiptMarkup[\s\S]*senderReceiptStatus\(/u);
  });
});
