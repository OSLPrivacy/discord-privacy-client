import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("secure recovery onboarding", () => {
  it("adds optional Mullvad and Android next steps only after recovery protection passes", () => {
    const recovery = functionSource("recoveryContent", "secureRecoveryOnboardingContent");
    // T15-A7: the refusal branch still short-circuits before any secret or
    // next step is emitted; it just no longer dead-ends, and the latch is read
    // through the recovery-kit state rather than inline.
    expect(functionSource("recoveryKitStateNow", "applyRecoveryKitAction"))
      .toContain("captureProven: recoveryCaptureGate.canRender()");
    expect(recovery).toMatch(/if \(view\.mode === "refusal"\) return recoveryProtectionRefusalContent\(view\);[\s\S]*?secureRecoveryOnboardingContent\(\)/);
    expect(recovery).toContain('id="copy-recovery-kit"');
    expect(recovery).toContain('id="recovery-continue"');
  });

  it("keeps next-step copy user-facing and honest about Android readiness", () => {
    const nextSteps = functionSource("secureRecoveryOnboardingContent", "identityPasswordForm");
    expect(nextSteps).toContain('class="secure-recovery-next-steps"');
    expect(nextSteps).toContain("Mullvad");
    expect(nextSteps).toContain("Optional. Use your existing session later for network privacy.");
    expect(nextSteps).toContain("Android device");
    expect(nextSteps).toContain("Coming later. Phone setup stays optional and separate.");
    expect(nextSteps).not.toMatch(/keyserver|ratchet|receipt|browser profile|provider adapter|account number|tunnel state|credential|token/i);
    expect(styles).toMatch(/\.secure-recovery-next-steps\s*\{[\s\S]*?grid-template-columns:\s*repeat\(2, minmax\(0, 1fr\)\);/);
  });
});
