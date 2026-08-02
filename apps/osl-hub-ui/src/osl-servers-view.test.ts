import { describe, expect, it } from "vitest";
import { oslServersViewMarkup } from "./osl-servers-view";

describe("T14-T4 third-party server destination", () => {
  it("names the provider-only surface instead of claiming OSL-native servers", () => {
    const markup = oslServersViewMarkup((label) => `<span>${label}</span>`);

    expect(markup).toContain(">Third-party servers</h1>");
    expect(markup).not.toContain(">Servers</h1>");
    expect(markup).toContain("Discord servers");
    expect(markup).toContain("Telegram groups and channels");
    expect(markup).toContain("Signal groups");
    expect(markup).toContain("Snapchat groups");
    expect(markup).toContain("OSL does not claim provider-server access or read provider pages. Direct OSL Chats are available now.");
  });
});
