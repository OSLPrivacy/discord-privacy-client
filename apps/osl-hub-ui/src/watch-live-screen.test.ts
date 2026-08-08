import { describe, expect, it } from "vitest";
import { WatchLiveScreen, watchLiveScreenMarkup, type WatchLiveScreenSnapshot } from "./watch-live-screen";
import type { WebsiteLiveRunProgress } from "./adapters";

function fakeStorage(): Storage {
  const map = new Map<string, string>();
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => { map.set(key, value); },
    removeItem: (key: string) => { map.delete(key); },
    clear: () => { map.clear(); },
    key: () => null,
    get length() { return map.size; },
  } as Storage;
}

function progress(overrides: Partial<WebsiteLiveRunProgress>): WebsiteLiveRunProgress {
  return {
    activeAccount: "fixture-account-1427@example.invalid",
    currentPlace: "Inbox",
    messagesChecked: 0,
    matches: 0,
    scrolls: 0,
    waits: 0,
    changes: 0,
    ...overrides,
  };
}

// Seven distinct backend snapshots, mirroring the shape of a real fixture run
// (open, check a message, match, scroll, wait, a second check, a second
// match) -- each one is a genuine change to the underlying run, so each must
// advance the visible count by exactly one.
const SEVEN_DISTINCT_STEPS: WebsiteLiveRunProgress[] = [
  progress({ currentPlace: "Inbox", changes: 0 }),
  progress({ currentPlace: "Read pane", messagesChecked: 1, waits: 1, changes: 1 }),
  progress({ currentPlace: "Review matches", messagesChecked: 1, matches: 1, waits: 1, changes: 2 }),
  progress({ currentPlace: "Older messages", messagesChecked: 1, matches: 1, scrolls: 1, waits: 1, changes: 3 }),
  progress({ currentPlace: "Settled wait", messagesChecked: 1, matches: 1, scrolls: 1, waits: 2, changes: 4 }),
  progress({ currentPlace: "Read pane", messagesChecked: 2, matches: 1, scrolls: 1, waits: 2, changes: 5 }),
  progress({ currentPlace: "Review matches", messagesChecked: 2, matches: 2, scrolls: 1, waits: 2, changes: 6 }),
];

describe("WatchLiveScreen", () => {
  it("stays quiet on repeated polls of the same snapshot and advances only on real change", () => {
    const storage = fakeStorage();
    const screen = new WatchLiveScreen("run-1427-a", storage);
    expect(screen.snapshot().visibleProgressCount).toBe(0);

    let last: WatchLiveScreenSnapshot = screen.snapshot();
    for (const step of SEVEN_DISTINCT_STEPS) {
      // Simulate the poller seeing the same snapshot twice in a row before the
      // run actually moves on -- this must not double-count.
      screen.applyProgress(step);
      last = screen.applyProgress(step);
    }

    expect(last.visibleProgressCount).toBe(7);
    expect(last.activeAccount).toBe("fixture-account-1427@example.invalid");
    expect(last.currentConversation).toBe("Review matches");
  });

  it("reopening the same run shows the count it left off at, not zero", () => {
    const storage = fakeStorage();
    const first = new WatchLiveScreen("run-1427-b", storage);
    for (const step of SEVEN_DISTINCT_STEPS) first.applyProgress(step);
    expect(first.snapshot().visibleProgressCount).toBe(7);

    // A fresh instance stands in for the screen being closed and reopened;
    // only the storage (and the run id) carries state across it.
    const reopened = new WatchLiveScreen("run-1427-b", storage);
    expect(reopened.snapshot().visibleProgressCount).toBe(7);
  });

  it("a fresh run (new run id) starts at zero even with prior state in storage", () => {
    const storage = fakeStorage();
    const priorRun = new WatchLiveScreen("run-1427-c", storage);
    for (const step of SEVEN_DISTINCT_STEPS) priorRun.applyProgress(step);
    expect(priorRun.snapshot().visibleProgressCount).toBe(7);

    const freshRun = new WatchLiveScreen("run-1427-d", storage);
    expect(freshRun.snapshot().visibleProgressCount).toBe(0);
  });

  it("renders the progress line, active account, and current conversation", () => {
    const storage = fakeStorage();
    const screen = new WatchLiveScreen("run-1427-e", storage);
    for (const step of SEVEN_DISTINCT_STEPS) screen.applyProgress(step);
    const markup = watchLiveScreenMarkup(screen.snapshot());
    expect(markup).toContain("Progress: 7");
    expect(markup).toContain("fixture-account-1427@example.invalid");
    expect(markup).toContain("Review matches");
  });
});
