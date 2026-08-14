import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  canContinuePastForwardSecrecyChoice,
  chooseForwardSecrecyMode,
  initialForwardSecrecyOnboardingState,
  onboardingForwardSecrecyMarkup,
} from "./onboarding-forward-secrecy";

const styles = readFileSync(new URL("./onboarding-forward-secrecy.css", import.meta.url), "utf8");

describe("D111 forward-secrecy onboarding choice", () => {
  it("starts with no saved choice and holds Continue", () => {
    const state = initialForwardSecrecyOnboardingState();
    expect(state.choice).toBeNull();
    expect(canContinuePastForwardSecrecyChoice(state)).toBe(false);
    const markup = onboardingForwardSecrecyMarkup(state);
    expect(markup).not.toContain('value="protect-past" checked');
    expect(markup).not.toContain('value="keep-group-delivery" checked');
    expect(markup).toContain("data-forward-secrecy-continue disabled");
  });

  it("states both irreversible costs and records an explicit choice", () => {
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    // Neither option may appear cost-free. The cost sits in the same place on
    // both cards, in the same size, so one is not easier to skip than the other.
    expect(markup).toContain("A badly timed restart can lose a message in transit.");
    expect(markup).toContain("A copy of your data reads what you sent.");
    expect(markup.match(/class="fs-tradeoff"/gu)).toHaveLength(2);
    expect(chooseForwardSecrecyMode(initialForwardSecrecyOnboardingState(), "protect-past").choice).toBe("protect-past");
    expect(chooseForwardSecrecyMode(initialForwardSecrecyOnboardingState(), "keep-group-delivery").choice).toBe("keep-group-delivery");
  });

  it("refuses values outside the two choices", () => {
    const saved = chooseForwardSecrecyMode(initialForwardSecrecyOnboardingState(), "protect-past");
    expect(chooseForwardSecrecyMode(saved, "archive-forever").choice).toBe("protect-past");
  });

  it("does not call this a phone", () => {
    // OSL ships as a Windows desktop app. The heading asked what happens "if
    // someone steals your phone", which describes a product that does not exist.
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    expect(markup).not.toMatch(/phone/iu);
    expect(markup).toContain("Forward secrecy");
    expect(markup).toContain("If someone gets into this computer");
  });

  it("drops the cryptographer copy the animations replaced", () => {
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    expect(markup).not.toContain("message keys");
    expect(markup).not.toContain("Both choices keep new messages private");
    expect(markup).not.toContain("You can change it later in Settings");
    expect(markup).not.toContain("STILL LOCKED");
  });

  it("keeps the wiring the Continue handler and the radio listener bind to", () => {
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    expect(markup).toContain('type="radio" name="forward-secrecy-mode"');
    expect(markup).toContain("data-forward-secrecy-continue");
    expect(markup).toContain('class="setup-footer onboarding-actions"');
  });

  it("draws three locks per card from one shared icon", () => {
    const markup = onboardingForwardSecrecyMarkup(initialForwardSecrecyOnboardingState());
    expect(markup.match(/class="fs-lock"/gu)).toHaveLength(6);
    expect(markup).toContain('class="fs-locks fs-locks-held"');
    expect(markup).toContain('class="fs-locks fs-locks-open"');
    // The base icon is identical on both cards, so the only difference a person
    // sees is the colour and whether the shackle lifts.
    expect(markup.match(/d="M8 11 V8 a4 4 0 0 1 8 0 v3"/gu)).toHaveLength(6);
  });

  it("rattles the held locks without ever opening one", () => {
    // The whole point of the left card: something tries, and gets nothing. If
    // the shackle animated there too, both cards would say the same thing.
    expect(styles).toContain(".fs-locks-held .fs-lock {\n  animation: fs-rattle");
    const rattle = styles.slice(styles.indexOf("@keyframes fs-rattle"), styles.indexOf("@keyframes fs-open"));
    expect(rattle).not.toContain("rotate");
    expect(styles).toContain("transform-origin: 16px 11px");
  });

  it("keeps the two cards distinguishable when motion is turned off", () => {
    const reduced = styles.slice(styles.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(reduced).toContain("animation: none");
    // Right card holds its shackle open, so the comparison survives without
    // motion rather than collapsing into two identical rows of shut locks.
    expect(reduced).toContain("transform: rotate(35deg) translateY(-1px)");
  });
});
