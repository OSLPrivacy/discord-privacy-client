import { describe, expect, it } from "vitest";
import { AUTOSCRUB_HOME_PAUSED_FIXTURE, AUTOSCRUB_HOME_PAUSE_REASON, autoScrubActivityRecordMarkup, autoScrubHomeActivityMarkup, openAutoScrubHomeActivity } from "./autoscrub-home-activity";

describe("TASK 1474 connect AutoScrub Home activity", () => {
  it("shows the next run, last run, paused account, exact reason, and all controls on Home", () => {
    const html = autoScrubHomeActivityMarkup();
    expect(html).toContain(`Next run: ${AUTOSCRUB_HOME_PAUSED_FIXTURE.nextRun}`);
    expect(html).toContain("Last run");
    expect(html).toContain(AUTOSCRUB_HOME_PAUSED_FIXTURE.lastRun.summary);
    expect(html).toContain(`data-autoscrub-paused-account=\"${AUTOSCRUB_HOME_PAUSED_FIXTURE.accountId}\"`);
    expect(html).toContain(AUTOSCRUB_HOME_PAUSE_REASON);
    for (const control of ["Run now", "Pause", "Resume", "Stop and turn off"]) expect(html).toContain(control);
    expect(html).toContain("View activity");
  });

  it("View activity opens exactly one record whose account id matches the paused account", () => {
    const record = openAutoScrubHomeActivity(AUTOSCRUB_HOME_PAUSED_FIXTURE.lastRun.id);
    const html = autoScrubActivityRecordMarkup(record);
    expect(record?.accountId).toBe(AUTOSCRUB_HOME_PAUSED_FIXTURE.accountId);
    expect(html.match(/data-autoscrub-activity-record-id=/gu)).toHaveLength(1);
    expect(html).toContain(`data-autoscrub-activity-record-id=\"${AUTOSCRUB_HOME_PAUSED_FIXTURE.lastRun.id}\"`);
    expect(html).toContain(`data-autoscrub-activity-account=\"${AUTOSCRUB_HOME_PAUSED_FIXTURE.accountId}\"`);
  });
});
