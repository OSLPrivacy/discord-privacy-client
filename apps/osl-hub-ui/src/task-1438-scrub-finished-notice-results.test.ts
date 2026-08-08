import { describe, expect, it } from "vitest";
import {
  noticeResultsMarkup,
  openNoticeResults,
  runIdFromNoticeId,
  type ScrubFinishedNotice,
  type ScrubRunResults,
} from "./scrub-finished-notice-results";

function fixtureNotice(id: string): ScrubFinishedNotice {
  return {
    id,
    service: "Discord",
    finishedAccount: "First",
    nextAccount: "Second",
    matchCount: 2,
    title: "Discord scan finished",
    detail: "2 matches found in Discord. Next account: Second.",
  };
}

function fixtureRuns(): ScrubRunResults[] {
  return [
    { runId: "task-1438-run-alpha", service: "Discord", account: "First", matchCount: 2 },
    { runId: "task-1438-run-beta", service: "Gmail", account: "Other", matchCount: 0 },
  ];
}

describe("TASK 1438 - connect notice to results", () => {
  it("extracts the run id a finished-scrub notice names", () => {
    expect(runIdFromNoticeId("scrub-finished-task-1438-run-alpha")).toBe("task-1438-run-alpha");
  });

  it("an activated fixture notice opens exactly the run whose id it names", () => {
    const notice = fixtureNotice("scrub-finished-task-1438-run-alpha");
    const opened = openNoticeResults(notice, fixtureRuns());

    expect(opened.opened).toHaveLength(1);
    expect(opened.opened[0].runId).toBe("task-1438-run-alpha");
    expect(opened.opened[0].service).toBe("Discord");
    expect(opened.reason).toBeNull();
  });

  it("does not also open the other run on screen", () => {
    const notice = fixtureNotice("scrub-finished-task-1438-run-alpha");
    const opened = openNoticeResults(notice, fixtureRuns());

    expect(opened.opened.some((run) => run.runId === "task-1438-run-beta")).toBe(false);
  });

  it("a notice with an unknown id opens 0 runs and shows its reason", () => {
    const notice = fixtureNotice("scrub-finished-no-such-run");
    const opened = openNoticeResults(notice, fixtureRuns());

    expect(opened.opened).toHaveLength(0);
    expect(opened.reason).toBe('No results found for run "no-such-run"');
  });

  it("a notice id not shaped like a finished-scrub notice also opens 0 runs and shows its reason", () => {
    const notice = fixtureNotice("not-a-scrub-notice");
    const opened = openNoticeResults(notice, fixtureRuns());

    expect(opened.opened).toHaveLength(0);
    expect(opened.reason).toBe('Notice "not-a-scrub-notice" does not name a run');
  });

  it("renders the opened run's results", () => {
    const opened = openNoticeResults(fixtureNotice("scrub-finished-task-1438-run-alpha"), fixtureRuns());
    const markup = noticeResultsMarkup(opened);
    expect(markup).toContain('data-scrub-notice-results-run="task-1438-run-alpha"');
    expect(markup).toContain("Discord results");
    expect(markup).toContain("First: 2 matches");
  });

  it("renders the reason instead of a result when nothing opened", () => {
    const opened = openNoticeResults(fixtureNotice("scrub-finished-missing"), fixtureRuns());
    const markup = noticeResultsMarkup(opened);
    expect(markup).not.toContain("data-scrub-notice-results-run");
    expect(markup).toContain("No results found for run &quot;missing&quot;");
  });
});
