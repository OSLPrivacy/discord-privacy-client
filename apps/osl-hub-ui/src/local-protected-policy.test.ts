import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function functionSource(name: string, nextName: string): string {
  const start = source.indexOf(`function ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("local protected context policy", () => {
  it("loads the exact context policy after activation", () => {
    const activation = functionSource("startLocalProtectedContext", "prepareLocalProtectedDraft");
    expect(activation).toContain("loadActiveContextSecurity(context.contextToken)");
    expect(activation).toContain("localProtectedSheet.decryptDisplayEnabled = security.decryptDisplayEnabled");
  });

  it("preserves the context decrypt-display setting while preparing", () => {
    const prepare = functionSource("prepareLocalProtectedDraft", "openLocalProtectedCapsule");
    expect(prepare).toContain("saveActiveContextSecurity(contextToken, ttlSeconds, localProtectedSheet.decryptDisplayEnabled)");
    expect(prepare).not.toContain("saveActiveContextSecurity(contextToken, ttlSeconds, true)");
    expect(prepare).toContain("!policy || !isLocalTtlSeconds(policy.ttlSeconds)");
    expect(prepare).toContain("localProtectedSheet.ttlSeconds = policy.ttlSeconds");
    expect(prepare.indexOf("localProtectedSheet.ttlSeconds = policy.ttlSeconds"))
      .toBeLessThan(prepare.indexOf("prepareLocalProtectedText(contextToken"));
    expect(prepare).not.toContain("localProtectedSheet.ttlSeconds = ttlSeconds");
    expect(prepare).toContain("localProtectedSheet.draft = plaintext");
    expect(prepare).toContain('setup.sendMode === "manual"');
    expect(prepare.indexOf('setup.sendMode === "manual"'))
      .toBeLessThan(prepare.indexOf("navigator.clipboard.writeText(prepared.capsule)"));
    expect(prepare.indexOf("hasExperimentalSendConsent")).toBeLessThan(prepare.indexOf("prepareLocalProtectedText(contextToken"));
    expect(prepare).toContain("Copied safely; nothing was sent.");
  });

  it("fails closed before decrypt and no longer carries a control of its own", () => {
    const opening = functionSource("openLocalProtectedCapsule", "copyLocalProtectedCapsule");
    expect(opening.indexOf("!localProtectedSheet.decryptDisplayEnabled")).toBeLessThan(opening.indexOf("decryptLocalProtectedText(contextToken, capsule)"));
    // EXPECTED VALUE CHANGED (TASK 4501). This sheet used to own one of four
    // controls over a single per-scope setting, with its own handler writing
    // `input.checked` straight into it. Four controls is four answers to "is
    // this protected right now"; there is one now, and it is the eye. The sheet
    // still READS the setting and still fails closed on it, asserted above --
    // it just no longer decides it.
    expect(source).not.toContain("changeLocalDecryptDisplay");
    expect(source).not.toContain('id="local-decrypt-display"');
    expect(source).not.toContain("saveActiveContextSecurity(contextToken, localProtectedSheet.ttlSeconds, input.checked)");
  });
});
