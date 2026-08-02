import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

describe("T2-97 shipping offline capability wiring", () => {
  it("uses the fail-closed status projection in the OSL Chat route and gates relay actions before their native calls", () => {
    expect(main).toMatch(/from\s*["']\.\/offline-capability-status["']/u);
    expect(main).toMatch(/function oslChatContent\(\): string[\s\S]*offlineCapabilitiesMarkup\(\)/u);
    for (const capability of ["receiveNewMessages", "sendMessage", "lookUpNewContactKey", "confirmBurnOnServer", "enforceExpiryOnServer", "enforceViewOnceOnServer"]) {
      expect(main).toContain(`"${capability}"`);
    }
    expect(main).toMatch(/function refreshOslChat[\s\S]*refuseOfflineCapability\("receiveNewMessages"\)[\s\S]*openOslChatText\(/u);
    expect(main).toMatch(/function sendOslChat[\s\S]*refuseOfflineCapability\("sendMessage"\)[\s\S]*prepareOslChatText\(/u);
    expect(main).toMatch(/function submitFriendCode[\s\S]*refuseOfflineCapability\("lookUpNewContactKey"\)[\s\S]*addOslFriend\(/u);
  });
});
