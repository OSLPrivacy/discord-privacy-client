import { describe, expect, it } from "vitest";
import { breakingChangeNoticeMarkup } from "./breaking-change-notice";

describe("T15-C7 verification breaking-change notice", () => {
  it("renders a non-dismissable reason for people.version below 3", () => {
    const notice = breakingChangeNoticeMarkup(2);

    expect(notice).toContain('data-people-reverification-notice');
    expect(notice).toContain("Verification codes changed");
    expect(notice).toContain("previous check compared a number OSL generated against itself");
    expect(notice).toContain("If a friend is offline");
    expect(notice).not.toMatch(/dismiss|close|button/iu);
  });
});
