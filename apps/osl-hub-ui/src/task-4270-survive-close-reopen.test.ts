import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";
import { homeAppsFromServices } from "./services";

const THE_THREE = ["x", "instagram", "messenger"] as const;

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function pickerAndHomeAppIds(): { picker: string[]; home: string[] } {
  // No linked accounts -- this is the catalog state a freshly (re)opened OSL
  // renders before any account is connected, so it exercises exactly the
  // static definitions the picker and home screen are both built from.
  const apps = homeAppsFromServices([]);
  const picker = apps.filter((app) => app.visibility === "launch").map((app) => app.id);
  // Mirrors main.ts's homeDestinationContent(): apps not yet "available" are
  // always included on the home screen as roadmap tiles, independent of any
  // persisted onboarding-app-selection preference.
  const home = apps
    .filter((app) => app.visibility === "launch" && app.launchState !== "available")
    .map((app) => app.id);
  return { picker, home };
}

describe("TASK 4270 -- the three survive closing and reopening OSL", () => {
  it("X, Instagram and Messenger are on the picker and the home screen, both before and after a simulated restart", async () => {
    const before = pickerAndHomeAppIds();
    for (const id of THE_THREE) {
      expect(before.picker, `${id} on picker before restart`).toContain(id);
      expect(before.home, `${id} on home screen before restart`).toContain(id);
    }

    // Simulate closing and reopening OSL: drop the module cache and reload
    // the catalog fresh, the same way a new process re-imports it on launch.
    vi.resetModules();
    const reloaded = await import("./services");
    const apps = reloaded.homeAppsFromServices([]);
    const after = {
      picker: apps.filter((app) => app.visibility === "launch").map((app) => app.id),
      home: apps
        .filter((app) => app.visibility === "launch" && app.launchState !== "available")
        .map((app) => app.id),
    };

    for (const id of THE_THREE) {
      expect(after.picker, `${id} on picker after restart`).toContain(id);
      expect(after.home, `${id} on home screen after restart`).toContain(id);
    }

    console.log(
      `TASK4270_UI picker_before=${JSON.stringify(before.picker.filter((id) => (THE_THREE as readonly string[]).includes(id)))} ` +
      `home_before=${JSON.stringify(before.home.filter((id) => (THE_THREE as readonly string[]).includes(id)))} ` +
      `picker_after=${JSON.stringify(after.picker.filter((id) => (THE_THREE as readonly string[]).includes(id)))} ` +
      `home_after=${JSON.stringify(after.home.filter((id) => (THE_THREE as readonly string[]).includes(id)))}`,
    );
  });

  it("main.ts wires the picker to the full catalog and the home screen to always show non-available apps as roadmap tiles", () => {
    // The picker (chooseAppsOnboardingContent) renders every "launch"
    // visibility app from the catalog, comingSoon included.
    expect(mainSource).toContain('const apps = homeAppsFromServices(services)\n    .filter((app) => app.visibility === "launch");');
    // The home screen always folds in apps that are not yet "available" as
    // roadmap tiles, regardless of persisted selection state.
    expect(mainSource).toContain('const roadmapHomeApps = launchableHomeApps.filter((app) => app.launchState !== "available");');
  });
});
