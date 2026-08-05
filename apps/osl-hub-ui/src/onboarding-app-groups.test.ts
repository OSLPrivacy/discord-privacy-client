import { describe, expect, it } from "vitest";
import { groupOnboardingApps } from "./onboarding-app-groups";

describe("onboarding app groups", () => {
  it("places a browser-imported app without a linked account under browser history", () => {
    const groups = groupOnboardingApps({
      apps: [
        { id: "gmail", linked: false },
        { id: "discord", linked: true },
      ],
      nativeApps: [],
      savedAccountsReady: true,
      importedBrowserAppIds: new Set(["gmail"]),
    });

    expect(groups.connected.map((app) => app.id)).toEqual(["discord"]);
    expect(groups.browserHistory.map((app) => app.id)).toEqual(["gmail"]);
    expect(groups.other).toEqual([]);
  });
});
