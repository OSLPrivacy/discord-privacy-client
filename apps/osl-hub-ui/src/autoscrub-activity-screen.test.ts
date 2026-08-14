import { describe, expect, it } from "vitest";

import {
  autoScrubActivityRowActions,
  openAccountTarget,
  renderAutoScrubActivityScreen,
  type AutoScrubActivityRun,
} from "./autoscrub-activity-screen";
import {
  AUTOSCRUB_ACTIVITY_SCREEN_LOGGED_OUT_RUN,
  AUTOSCRUB_ACTIVITY_SCREEN_RUNS,
} from "./autoscrub-activity-screen-data";

const signedInRun: AutoScrubActivityRun = {
  runId: "run-a",
  accountId: "acct-a",
  serviceId: "discord",
  text: "acct-a: matched 4, deleted 4, failed 0 (ran on this device)",
  location: "local",
  signedIn: true,
};

describe("TASK 1477 connect AutoScrub activity screen", () => {
  it("shows every run", () => {
    const html = renderAutoScrubActivityScreen(AUTOSCRUB_ACTIVITY_SCREEN_RUNS);

    expect(html.match(/class="autoscrub-activity-row"/gu)).toHaveLength(AUTOSCRUB_ACTIVITY_SCREEN_RUNS.length);
    for (const run of AUTOSCRUB_ACTIVITY_SCREEN_RUNS) {
      expect(html).toContain(`data-run-id="${run.runId}"`);
    }
  });

  it("shows each run's match text and location", () => {
    const html = renderAutoScrubActivityScreen(AUTOSCRUB_ACTIVITY_SCREEN_RUNS);

    for (const run of AUTOSCRUB_ACTIVITY_SCREEN_RUNS) {
      expect(html).toContain(run.text);
    }
    expect(html).toContain("Ran on this device");
    expect(html).toContain("Ran in the cloud");
  });

  it("never renders a jump link, on any row", () => {
    const html = renderAutoScrubActivityScreen(AUTOSCRUB_ACTIVITY_SCREEN_RUNS);

    expect(html).not.toMatch(/<a[\s>]/u);
    expect(html).not.toMatch(/href=/u);
    expect(html).not.toMatch(/go to message|view message|jump to/iu);
  });

  it("offers Open account, Skip account and Turn off on every row", () => {
    const actions = autoScrubActivityRowActions(signedInRun);

    expect(actions.map((action) => action.label)).toEqual(["Open account", "Skip account", "Turn off"]);
    expect(actions.find((action) => action.action === "openAccount")?.accountId).toBe("acct-a");
    expect(actions.find((action) => action.action === "skipAccount")?.accountId).toBe("acct-a");
    expect(actions.find((action) => action.action === "turnOff")?.accountId).toBeNull();
  });

  it("renders one Open account, one Skip account and one Turn off button per row", () => {
    const html = renderAutoScrubActivityScreen(AUTOSCRUB_ACTIVITY_SCREEN_RUNS);
    const rowCount = AUTOSCRUB_ACTIVITY_SCREEN_RUNS.length;

    expect(html.match(/data-autoscrub-activity-action="openAccount"/gu)).toHaveLength(rowCount);
    expect(html.match(/data-autoscrub-activity-action="skipAccount"/gu)).toHaveLength(rowCount);
    expect(html.match(/data-autoscrub-activity-action="turnOff"/gu)).toHaveLength(rowCount);
  });

  it("a logged-out fixture row opens its account and renders no message link", () => {
    const loggedOut = AUTOSCRUB_ACTIVITY_SCREEN_LOGGED_OUT_RUN;
    expect(loggedOut.signedIn).toBe(false);

    // "opens its account": the row's Open account action targets exactly its own account id.
    const openTarget = openAccountTarget(loggedOut, "openAccount");
    expect(openTarget).toBe(loggedOut.accountId);

    const html = renderAutoScrubActivityScreen([loggedOut]);
    const rowMarkup = html.match(/<li class="autoscrub-activity-row"[\s\S]*<\/li>/u)?.[0] ?? "";
    expect(rowMarkup).toContain(`data-account-id="${loggedOut.accountId}"`);
    expect(rowMarkup).toContain(`data-autoscrub-activity-account="${loggedOut.accountId}"`);
    expect(rowMarkup).toContain("Signed out");

    // "no message link is rendered": no anchor, no href, anywhere in that row.
    expect(rowMarkup).not.toMatch(/<a[\s>]/u);
    expect(rowMarkup).not.toMatch(/href=/u);
  });

  it("says so plainly when there is no activity yet", () => {
    const html = renderAutoScrubActivityScreen([]);

    expect(html).toContain("No AutoScrub runs yet");
    expect(html).toContain("Nothing has run yet.");
    expect(html).not.toContain("autoscrub-activity-row");
  });

  it("escapes hostile run text", () => {
    const html = renderAutoScrubActivityScreen([
      { ...signedInRun, text: '<img src=x onerror="alert(1)">' },
    ]);

    expect(html).not.toContain("<img src=x");
    expect(html).toContain("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
  });
});
