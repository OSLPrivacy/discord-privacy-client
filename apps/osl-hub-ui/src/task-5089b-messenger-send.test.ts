import { describe, expect, it } from "vitest";
import { homeLauncherBody } from "./home-launcher-body";
import { homeAppsFromServices } from "./services";

describe("TASK 5089b Messenger protected-send refusal", () => {
  it("shows the refusal reason and exposes no send/start action", () => {
    const markup = homeLauncherBody({
      services: [],
      nativeApps: [],
      savedNativeApps: new Set(),
      selectedOnboardingApps: new Set(),
      hasExplicitOnboardingAppSelection: false,
      homeTileOrder: [],
      hiddenHomeTiles: new Set(),
      homeEditMode: false,
      appLaunchPendingId: null,
      hubIdentities: [],
      homeDestinationContent: "",
    }, {
      homeModuleIcon: () => "",
      homeAppLogo: () => "",
      homeCommandIcon: () => "",
      nativeClaimLabel: () => "",
    });

    const messengerTile = markup.match(/<article[^>]*data-tile-id="messenger"[\s\S]*?<\/article>/u)?.[0];
    expect(messengerTile).toBeTruthy();
    expect(messengerTile).toContain("Messenger");
    expect(messengerTile).toContain("Cannot send yet");
    expect(messengerTile).toContain("<small>Cannot send yet</small>");
    expect(messengerTile).toContain('aria-disabled="true"');
    expect(messengerTile).toContain("disabled");
    expect(messengerTile).not.toContain('data-home-app="messenger"');

    const messenger = homeAppsFromServices([]).find((app) => app.id === "messenger");
    expect(messenger).toMatchObject({ launchState: "comingSoon", serviceId: null, setupEligible: false });
  });
});
