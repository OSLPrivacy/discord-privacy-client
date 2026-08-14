import { describe, expect, it } from "vitest";

import { oslGroupChatHeaderMarkup, oslGroupChatMemberCountLabel } from "./group-chat-header";

describe("TASK 1315 group chat header", () => {
  it("renders the group name, exact member count, and named chat actions", () => {
    const markup = oslGroupChatHeaderMarkup({
      groupId: "group-weekend-plans",
      name: "Weekend plans",
      memberCount: 3,
    });

    expect(markup).toContain("<h1>Weekend plans</h1>");
    expect(markup).toContain('data-osl-group-member-count="3">3 members</p>');
    expect(markup).toContain('aria-label="Chat actions"');
    expect(markup).toContain("Search");
    expect(markup).toContain("Settings");
  });

  it("does not turn an unknown or zero value into a fictional count", () => {
    expect(() => oslGroupChatMemberCountLabel(0)).toThrow("positive, exact member count");
    expect(() => oslGroupChatMemberCountLabel(1.5)).toThrow("positive, exact member count");
  });
});
