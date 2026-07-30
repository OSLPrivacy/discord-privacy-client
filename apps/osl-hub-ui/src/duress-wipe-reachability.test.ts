import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("duress wipe production reachability", () => {
  it("duress-wipe-reachability", () => {
    const nativeMain = readFileSync(
      new URL("../../osl-hub/src/main.rs", import.meta.url),
      "utf8",
    );
    const startupGate = readFileSync(
      new URL("../../osl-hub/src/startup_gate.rs", import.meta.url),
      "utf8",
    );
    const uiCore = readFileSync(new URL("./core.ts", import.meta.url), "utf8");
    const uiMain = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

    expect(uiMain).toContain('id="identity-duress-pin"');
    expect(uiMain).toContain("data-duress-pin");
    expect(uiMain).toContain("unlockHubPasswordGate(secret, duressSecret || undefined)");
    expect(uiCore).toContain("duressPin?: string");
    expect(uiCore).toContain("duressPin: hasDuressPin ? duressPin : null");
    expect(nativeMain).toContain("duress_pin: Option<String>");
    expect(nativeMain).toContain("startup_gate::verify_duress_pin");
    expect(nativeMain).toContain(".duress_engine");
    expect(nativeMain).toContain(".execute()");
    expect(startupGate).toContain("pub fn verify_duress_pin(");
    expect(startupGate).toContain("matches!(outcome, ipc::main_password::GateMatch::Burn)");
  });
});
