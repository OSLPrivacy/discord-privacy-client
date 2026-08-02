import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const sequence = readFileSync(new URL("./onboarding-sequence.ts", import.meta.url), "utf8");

describe("Tor onboarding shipping wiring", () => {
  it("requires the explicit no-default connection choice before send setup", () => {
    expect(sequence).toMatch(/"defaults",\s*"tor",\s*"sending"/u);
    expect(main).toContain('from "./onboarding-tor"');
    expect(main).toContain('if (onboardingRoute === "tor") return onboardingTorMarkup(torOnboarding);');
    expect(main).toContain('onboardingRoute = "tor"; render();');
    expect(main).toContain("chooseTorRoute(torOnboarding, input.value)");
    expect(main).toContain('document.querySelector<HTMLButtonElement>("[data-tor-choice-continue]")');
    expect(main).toContain('if (torOnboarding.choice === null) return;');
    expect(main).toContain('onboardingRoute = "sending";');
  });
});
