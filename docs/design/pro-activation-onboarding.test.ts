import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface ActivationContract {
  placement: { before: string; after: string };
  skip: { firstClass: boolean; networkRequest: boolean; persistAttempt: boolean; route: string; access: string };
  success: { statuses: string[]; route: string };
  failure: { staysOnStep: boolean; canRetry: boolean; canSkip: boolean };
  safety: { basicEncryptionRequiresPro: boolean; cloudConsentBundledWithActivation: boolean; codePersistedInWebStorage: boolean };
}

function readContract(): ActivationContract {
  const document = readFileSync(new URL("./pro-activation-onboarding.md", import.meta.url), "utf8");
  const match = document.match(/```json pro-activation-contract\n([\s\S]*?)\n```/u);
  if (!match) throw new Error("Pro activation contract fixture is missing");
  return JSON.parse(match[1]) as ActivationContract;
}

describe("Pro activation onboarding contract", () => {
  it("keeps activation optional and preserves the required privacy step", () => {
    const contract = readContract();

    expect(contract.placement).toEqual({ before: "privacy", after: "recovery" });
    expect(contract.skip).toEqual({
      firstClass: true,
      networkRequest: false,
      persistAttempt: false,
      route: "privacy",
      access: "free",
    });
    expect(contract.failure).toEqual({ staysOnStep: true, canRetry: true, canSkip: true });
  });

  it("keeps Pro activation separate from encryption and cloud consent", () => {
    const contract = readContract();

    expect(contract.success).toEqual({ statuses: ["ACTIVE", "CANCELLED", "GRACE"], route: "privacy" });
    expect(contract.safety).toEqual({
      basicEncryptionRequiresPro: false,
      cloudConsentBundledWithActivation: false,
      codePersistedInWebStorage: false,
    });
  });
});
