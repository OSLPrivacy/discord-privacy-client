import { describe, expect, it } from "vitest";
import { senderReceiptStatus } from "./receipt-status";

describe("sender receipt status", () => {
  it("renders an absent receipt as not confirmed, never evidence of unread content", () => {
    const status = senderReceiptStatus(undefined);

    expect(status).toEqual({ label: "Not confirmed", confirmed: false });
    expect(status.label).not.toMatch(/read/i);
  });

  it("keeps pre-receipt states indistinguishable from an absent receipt", () => {
    expect(senderReceiptStatus(null)).toEqual(senderReceiptStatus("Prepared"));
    expect(senderReceiptStatus("RelayAccepted")).toEqual(senderReceiptStatus(undefined));
  });

  it("attributes confirmed delivery and opening to the recipient app", () => {
    expect(senderReceiptStatus("Delivered")).toEqual({
      label: "Their app reported it delivered",
      confirmed: true,
    });
    expect(senderReceiptStatus("Opened")).toEqual({
      label: "Their app reported it opened",
      confirmed: true,
    });
  });
});
