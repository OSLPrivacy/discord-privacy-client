import { describe, expect, it } from "vitest";
import { parseUpdateCheck } from "./updates";

describe("trusted OSL Privacy update contract", () => {
  it("accepts bounded plain update metadata", () => {
    expect(parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "Security and reliability fixes." })).toEqual({
      state: "available", current: "0.1.0", next: "0.2.0", notes: "Security and reliability fixes.",
    });
  });

  it("rejects remote HTML fields, arbitrary URLs, and unknown states", () => {
    expect(parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "ok", html: "<b>remote</b>" }).state).toBe("error");
    expect(parseUpdateCheck({ status: "update_available", current: "0.1.0", next: "0.2.0", notes: "ok", url: "https://evil.invalid" }).state).toBe("error");
    expect(parseUpdateCheck({ status: "install_now" }).state).toBe("error");
  });

  it("fails closed on malformed versions and oversized notes", () => {
    expect(parseUpdateCheck({ status: "up_to_date", current: "<script>" }).state).toBe("error");
    expect(parseUpdateCheck({ status: "update_available", current: "1.0.0", next: "2.0.0", notes: "x".repeat(2_001) }).state).toBe("error");
  });
});
