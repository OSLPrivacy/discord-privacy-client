import { describe, expect, it } from "vitest";
import { onboardingPasswordRoleContent } from "./password-roles";

const stealth = () => onboardingPasswordRoleContent({
  role: "stealth",
  configured: false,
  passwordEyeIcon: () => "<svg></svg>",
  statusTag: (label) => `<span>${label}</span>`,
});

describe("onboarding password roles", () => {
  // 2026-08-06 restyle. One paragraph became a picture plus one line. The rule
  // it carried is unchanged and is what is checked here: the screen says what
  // the mode DOES, says the two things it does NOT do, and never claims OSL
  // becomes invisible.
  it("states the stealth workspace limit without claiming invisibility", () => {
    const content = stealth();

    // What it does -- now the caption under the animation.
    expect(content).toContain("Opens an empty workspace");
    // The two limits. These are the reason the sentence existed; losing either
    // would leave someone believing stealth mode hides the app itself.
    expect(content).toContain("does not hide that OSL is installed");
    expect(content).toContain("anyone viewing hidden files can still find it");
    expect(content).not.toMatch(/invisible|undetectable|untraceable/iu);
  });

  it("puts the limits where the mode is armed, not above the fold", () => {
    // The caveat used to sit in a paragraph under the title, which is the part
    // of a setup screen people skip. It now sits immediately before the button
    // that turns the mode on.
    const content = stealth();
    const caveat = content.indexOf("does not hide that OSL is installed");
    const submit = content.indexOf("stealth-submit");
    expect(caveat).toBeGreaterThanOrEqual(0);
    expect(submit).toBeGreaterThan(caveat);
  });

  it("keeps the picture describable for anyone who cannot see it", () => {
    // An animation with no text alternative is worse than the paragraph it
    // replaced, because a screen reader gets nothing at all.
    expect(stealth()).toContain('aria-label="a workspace empties of its messages when stealth mode turns on"');
  });
});
