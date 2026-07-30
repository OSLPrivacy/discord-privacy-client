import { describe, expect, it } from "vitest";
import { computeScopeFingerprint, encodeScopeFingerprintInput } from "./scrub-scope-fingerprint";

describe("scrub scope fingerprint", () => {
  it("computeScopeFingerprint injective encoding + SHA-256 digest", async () => {
    const one = await computeScopeFingerprint({
      serviceId: "ab",
      accountId: "c",
      scanScope: "visible",
      findingCategories: ["credential"],
    });
    const two = await computeScopeFingerprint({
      serviceId: "a",
      accountId: "bc",
      scanScope: "visible",
      findingCategories: ["credential"],
    });
    const changedFinding = await computeScopeFingerprint({
      serviceId: "ab",
      accountId: "c",
      scanScope: "visible",
      findingCategories: ["credential", "payment_card"],
    });
    const reorderedFindings = await computeScopeFingerprint({
      serviceId: "ab",
      accountId: "c",
      scanScope: "visible",
      findingCategories: ["payment_card", "credential"],
    });

    expect(one).toMatch(/^[a-f0-9]{64}$/u);
    expect(one).not.toBe(two);
    expect(changedFinding).not.toBe(one);
    expect(changedFinding).toBe(reorderedFindings);
    expect([...encodeScopeFingerprintInput({
      serviceId: "ab",
      accountId: "c",
      scanScope: "visible",
      findingCategories: ["credential"],
    }).slice(0, 4)]).toEqual([0, 0, 0, 6]);
    expect(() => encodeScopeFingerprintInput({
      serviceId: "ab",
      accountId: "c",
      scanScope: "visible",
      findingCategories: ["credential", "credential"],
    })).toThrow("invalid scrub scope fingerprint input");
  });
});
