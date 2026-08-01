import { describe, expect, it } from "vitest";
import { onboardingPasswordRoleContent } from "./password-roles";

describe("onboarding password roles", () => {
  it("states the stealth workspace limit without claiming invisibility", () => {
    const content = onboardingPasswordRoleContent({
      role: "stealth",
      configured: false,
      passwordEyeIcon: () => "<svg></svg>",
      statusTag: (label) => `<span>${label}</span>`,
    });

    expect(content).toContain("Opens an empty workspace without loading your private data.");
    expect(content).toContain("does not hide that OSL is installed");
    expect(content).toContain("anyone viewing hidden files can still find it");
    expect(content).not.toContain("invisible");
  });
});
