import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", async () => {
  const actual = await vi.importActual<typeof import("./preferences")>("./preferences");
  return { ...actual, isTauriRuntime: mocks.isTauriRuntime };
});
import { acceptedFriendPageActionsMarkup } from "./friend-page-actions";
import {
  handleLiveScanPause,
  handleLiveScanResume,
  handleLiveScanStopScrub,
  liveScanControlsMarkup,
  resetLiveScanControlsStateForTest,
} from "./live-scan-controls";
import { formatNarratorJourneyReport, runNarratorJourneys, type NarratorJourneySpec } from "./narrator-journeys";
import { renderScrubRoute } from "./scrub-route";
import { whitelistingSaved, whitelistingScreenMarkup, type WhitelistingScreenState } from "./whitelisting-screen";

const changedWhitelisting: WhitelistingScreenState = {
  conversations: [
    { id: "study-circle", account: "Discord · @ada", name: "Study Circle", kind: "Group" },
    { id: "study-beta", account: "Discord · @ada", name: "Study Group Beta", kind: "Group" },
  ],
  saved: ["study-circle"],
  draft: ["study-circle", "study-beta"],
  search: "",
  busy: false,
};

const oneOpenRun = {
  contract: "autoscrubRunFleet.v1",
  openRunCount: 1,
  globalStopRequested: false,
  stopConfirmation: { required: false, keepScanningLabel: "Keep scanning", stopNowLabel: "Stop now" },
  unattendedExecutionAllowed: false,
  quitGuard: { state: "notRequested", honestRemainingSecondsEstimate: null, reason: "No stop request is active." },
  fleetActions: [],
  runs: [{
    runId: "narrator-run-001", serviceId: "discord", accountId: "account-1", phase: "running",
    reviewedItemCount: 3, remainingItemCount: 0, paceMilliseconds: 1_000, stopRequested: false,
    mutationAllowed: false, lastOutcome: "none", accountActions: [],
  }],
};

async function narratorSpecs(): Promise<readonly NarratorJourneySpec[]> {
  const savedWhitelisting = whitelistingSaved(changedWhitelisting);
  const friend = acceptedFriendPageActionsMarkup({
    personId: "ada-lovelace",
    name: "Ada Lovelace",
    pictureMarkup: '<span role="img" aria-label="Ada Lovelace portrait">A</span>',
    accepted: true,
  });
  const scrubStart = renderScrubRoute({
    accounts: [{ id: "local-export", label: "Local message export", detail: "TXT on this device" }],
    selectedAccountIds: ["local-export"],
    selectedCategories: ["personal"],
    scan: { state: "not-started", findings: 0 },
  }, "scan");

  resetLiveScanControlsStateForTest();
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
  mocks.invoke.mockResolvedValueOnce(oneOpenRun).mockResolvedValueOnce({ ...oneOpenRun, openRunCount: 0, runs: [] });
  const scrubReadyControls = liveScanControlsMarkup();
  await handleLiveScanPause();
  const scrubPausedControls = liveScanControlsMarkup();
  await handleLiveScanResume();
  const scrubResumedControls = liveScanControlsMarkup();
  await handleLiveScanStopScrub();
  const scrubStoppedControls = liveScanControlsMarkup();
  return [
    {
      name: "change one Whitelisting setting",
      steps: [
        { surfaceHtml: whitelistingScreenMarkup(changedWhitelisting), target: "Search conversations", kind: "control" },
        { surfaceHtml: whitelistingScreenMarkup(changedWhitelisting), target: "Allow Study Group Beta in Discord · @ada", kind: "control" },
        { surfaceHtml: whitelistingScreenMarkup(changedWhitelisting), target: "Save", kind: "control" },
        {
          surfaceHtml: whitelistingScreenMarkup(savedWhitelisting),
          target: "Everything on this screen is saved.",
          kind: "text",
          savedChange: true,
          spokenConfirmation: "Everything on this screen is saved.",
        },
      ],
    },
    {
      name: "find one friend",
      steps: [{ surfaceHtml: friend, target: "Ada Lovelace", kind: "text" }],
    },
    {
      name: "start, pause, resume, and stop one Scrub run",
      steps: [
        { surfaceHtml: scrubStart, target: "Start scan", kind: "control" },
        { surfaceHtml: scrubReadyControls, target: "Pause", kind: "control" },
        { surfaceHtml: scrubPausedControls, target: "Resume", kind: "control" },
        { surfaceHtml: scrubResumedControls, target: "Stop Scrub", kind: "control" },
        { surfaceHtml: scrubStoppedControls, target: "Ready to review", kind: "text" },
      ],
    },
  ];
}

describe("TASK 3653 Windows Narrator settings, friends, and Scrub journeys", () => {
  it("saves exactly three completed journeys with no unnamed or blocked controls", async () => {
    const report = runNarratorJourneys(await narratorSpecs());
    const printed = formatNarratorJourneyReport(report);
    console.log(printed);

    expect(report.journeys).toHaveLength(3);
    expect(report.completedJourneys).toBe(3);
    expect(report.blockedControls).toBe(0);
    expect(report.unnamedControls).toBe(0);
    expect(report.savedChanges).toBe(1);
    expect(report.spokenConfirmations).toEqual(["Everything on this screen is saved."]);
    expect(printed).toContain("summary saved_journeys=3 completed_journeys=3 blocked_controls=0 unnamed_controls=0 saved_changes=1 spoken_confirmations=1");
  });

  it("records unnamed and disabled requested controls as incomplete", () => {
    const report = runNarratorJourneys([{
      name: "broken control proof",
      steps: [{ surfaceHtml: '<button type="button" disabled></button>', target: "", kind: "control" }],
    }]);
    expect(report.completedJourneys).toBe(0);
    expect(report.blockedControls).toBe(1);
    expect(report.unnamedControls).toBe(1);
  });
});
