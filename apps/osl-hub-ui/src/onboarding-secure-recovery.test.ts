import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("secure recovery onboarding", () => {
  it("still emits no secret and no onward step on the refusal branch", () => {
    // The rule this file has always protected: the capture latch decides whether
    // anything sensitive paints, and the refusal branch short-circuits BEFORE a
    // phrase or a Continue is emitted. The restyle changed what the ready branch
    // looks like; it must not have changed that ordering.
    const recovery = functionSource("recoveryContent", "identityPasswordForm");
    expect(functionSource("recoveryKitStateNow", "applyRecoveryKitAction"))
      .toContain("captureProven: recoveryCaptureGate.canRender()");
    expect(recovery).toMatch(/if \(view\.mode === "refusal"\) return recoveryProtectionRefusalContent\(view\);[\s\S]*?visibleRecoverySecrets\(state\)/);
    // Both branches above the secrets return before anything below them runs.
    const refusalAt = recovery.indexOf('view.mode === "refusal"');
    const copyAt = recovery.indexOf('id="copy-recovery-kit"');
    expect(refusalAt).toBeGreaterThanOrEqual(0);
    expect(copyAt).toBeGreaterThan(refusalAt);
    expect(recovery).toContain('id="recovery-continue"');
  });

  it("no longer promises Android, and does not lose the Mullvad offer", () => {
    // 2026-08-06 restyle removed the two "next steps" cards from the recovery
    // screen. The Android one was a coming-soon, which the owner banned outright,
    // so it must stay gone. Mullvad was a real offer, so what matters is that it
    // survived somewhere reachable rather than being deleted with the card.
    expect(source).not.toContain("secureRecoveryOnboardingContent");
    expect(source).not.toContain("secure-recovery-next-steps");
    expect(source).not.toContain("Coming later. Phone setup stays optional and separate.");
    expect(source).not.toMatch(/Android device/);
    // Mullvad is still its own onboarding step with its own screen.
    expect(source).toContain('if (onboardingRoute === "mullvad") return mullvadSetupContent();');
    expect(functionSource("renderOnboarding", "onboardingContent")).toContain('"mullvad"');
  });
});
