import { describe, expect, it, vi } from "vitest";
import {
  initialAccountRecoveryFlow,
  recoveryScreenMarkup,
  submitRecoveredPassword,
  submitRecoveryPhrase,
  type AccountRecoveryDependencies,
} from "./account-recovery";

const lockoutStatus = {
  passwordLockedUntil: null,
  passwordAttemptsUsed: 0,
  phraseLockedUntil: 1_060,
  phraseAttemptsUsed: 3,
  now: 1_000,
};

describe("account recovery", () => {
  it("fails closed for a wrong phrase and shows the backend attempt and lockout status", async () => {
    const dependencies: AccountRecoveryDependencies = {
      verifyPhrase: vi.fn().mockResolvedValue({ ok: false, lockoutStatus }),
      setPassword: vi.fn(),
    };

    const flow = await submitRecoveryPhrase(initialAccountRecoveryFlow, "wrong words", dependencies);

    expect(flow).toMatchObject({ step: "phrase", recoveryToken: null, lockoutStatus });
    expect(recoveryScreenMarkup(flow)).toContain("3 recovery phrase attempts recorded. Try again in 60 seconds.");
    expect(recoveryScreenMarkup(flow)).not.toContain('name="newPassword"');
    expect(dependencies.setPassword).not.toHaveBeenCalled();
  });

  it("only offers a new password after a verified phrase", async () => {
    const dependencies: AccountRecoveryDependencies = {
      verifyPhrase: vi.fn().mockResolvedValue({
        ok: true,
        recoveryToken: "verified-token",
        lockoutStatus: { ...lockoutStatus, phraseLockedUntil: null },
      }),
      setPassword: vi.fn(),
    };

    const flow = await submitRecoveryPhrase(initialAccountRecoveryFlow, "right words", dependencies);

    expect(flow).toMatchObject({ step: "password", recoveryToken: "verified-token" });
    expect(recoveryScreenMarkup(flow)).toContain('name="newPassword"');
  });

  it("fails closed when setting the replacement password is rejected", async () => {
    const dependencies: AccountRecoveryDependencies = {
      verifyPhrase: vi.fn(),
      setPassword: vi.fn().mockRejectedValue(new Error("expired token")),
    };
    const verifiedFlow = {
      ...initialAccountRecoveryFlow,
      step: "password" as const,
      recoveryToken: "verified-token",
    };

    const flow = await submitRecoveredPassword(verifiedFlow, "new-password", "new-password", dependencies);

    expect(flow).toMatchObject({ step: "password", recoveryToken: "verified-token" });
    expect(flow.error).toContain("could not reset");
  });
});
