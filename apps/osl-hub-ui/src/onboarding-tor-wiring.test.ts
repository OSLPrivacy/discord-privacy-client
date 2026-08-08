import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const sequence = readFileSync(new URL("./onboarding-sequence.ts", import.meta.url), "utf8");

describe("Tor onboarding shipping wiring", () => {
  it("requires the explicit connection choice before review defaults and send setup", () => {
    expect(sequence).toMatch(/"privacy",\s*"tor",\s*"defaults",\s*"sending"/u);
    expect(main).toContain('from "./onboarding-tor"');
    expect(main).toContain('if (onboardingRoute === "tor") return onboardingTorMarkup(torOnboarding);');
    expect(main).toContain('document.querySelector("#continue-onboarding-privacy")?.addEventListener("click", () => { onboardingRoute = "defaults"; render(); });');
    expect(main).toContain('document.querySelector("#continue-defaults-review")?.addEventListener("click", () => { onboardingRoute = "tor"; render(); });');
    expect(main).toContain("chooseTorRoute(torOnboarding, input.value)");
    expect(main).toContain('document.querySelector<HTMLButtonElement>("[data-tor-choice-continue]")');
    expect(main).toContain('if (torOnboarding.choice === null) return;');
    expect(main).toContain('invoke("set_tor_preference", { preference: torOnboarding.choice })');
    expect(main).toContain('onboardingRoute = "defaults";');
    expect(main).toContain("beginOnboardingTorBootstrap()");
    expect(main).toContain('document.querySelector<HTMLButtonElement>("[data-tor-connected-continue]")');
    expect(main).toContain('|| onboardingRoute === "tor"');
  });
});
