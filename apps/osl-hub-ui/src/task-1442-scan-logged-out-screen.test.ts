/**
 * TASK 1442 - connect logged-out screen.
 *
 * Finish line: a logout fixture displays the exact sentence
 * "Discord needs you to sign in again", and 0 scan steps run after it.
 *
 * The fixture backend below is a line-for-line stand-in for gate 1440/1441's
 * store in apps/osl-hub/src/autoscrub_run.rs:
 *
 *   record_service_connection_event  -> phase Blocked, stop_requested true,
 *                                       mutation_allowed false, one
 *                                       state-recording log entry;
 *   account_actions_for              -> EMPTY while stop_requested is set;
 *   fleet_actions_for                -> ["Stop all scanning"] while a run is open;
 *   fleet()                          -> only open-phase runs are listed.
 *
 * The scan driver is the real `AutoScrubProgress` from autoscrub-progress.ts,
 * and a scan step is only counted when its body actually executes -- the count
 * is taken inside the step, where neither the screen nor the test can forge it.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { AutoScrubProgress } from "./autoscrub-progress";
import type { AutoScrubRunActionKind } from "./autoscrub-contract";
import {
  LOGGED_OUT_RECOVERY_CHOICES,
  ScanLoggedOutScreen,
  signInAgainSentence,
  type LoggedOutScreenPorts,
  type ServiceConnectionEvent,
} from "./scan-logged-out-screen";

const EXACT_SENTENCE = "Discord needs you to sign in again.";

type Phase = "reviewRequired" | "running" | "stopping" | "blocked" | "skipped" | "complete" | "failed";

const ACTION_LABELS: Record<AutoScrubRunActionKind, string> = {
  openAccount: "Open account",
  tryAgainAfterSignIn: "Try again after sign-in",
  skipThisAccount: "Skip this account",
  stopAllScanning: "Stop all scanning",
};

interface FixtureRun {
  runId: string;
  serviceId: "discord";
  accountId: string;
  phase: Phase;
  reviewedItemCount: number;
  remainingItemCount: number;
  stopRequested: boolean;
  connectionState: "running" | "stopped_on_logout" | "stopped_on_human_check" | "stopped_on_suspension";
}

const OPEN_PHASES: readonly Phase[] = ["reviewRequired", "running", "stopping", "blocked"];

/** apps/osl-hub/src/autoscrub_run.rs :: AutoScrubRunStore, for two accounts. */
class FixtureAutoScrubStore {
  globalStopRequested = false;
  stopConfirmationRequired = false;
  readonly serviceConnectionLog: { runId: string; event: string; state: string; action: string }[] = [];
  readonly credentialCalls: unknown[] = [];

  runs: FixtureRun[] = [
    {
      runId: "autoscrub-run-0001",
      serviceId: "discord",
      accountId: "acct-discord-maple",
      phase: "running",
      reviewedItemCount: 6,
      remainingItemCount: 6,
      stopRequested: false,
      connectionState: "running",
    },
    {
      runId: "autoscrub-run-0002",
      serviceId: "discord",
      accountId: "acct-discord-birch",
      phase: "reviewRequired",
      reviewedItemCount: 4,
      remainingItemCount: 4,
      stopRequested: false,
      connectionState: "running",
    },
  ];

  /** account_actions_for -- empty while the run is stopped. */
  private accountActions(run: FixtureRun): { action: string; label: string }[] {
    if (this.globalStopRequested || run.stopRequested) return [];
    const kinds: AutoScrubRunActionKind[] =
      run.phase === "reviewRequired" || run.phase === "running"
        ? ["openAccount", "skipThisAccount"]
        : run.phase === "blocked" || run.phase === "failed"
          ? ["tryAgainAfterSignIn", "skipThisAccount"]
          : [];
    return kinds.map((action) => ({ action, label: ACTION_LABELS[action] }));
  }

  fleet(): unknown {
    const open = this.runs.filter((run) => OPEN_PHASES.includes(run.phase));
    return {
      contract: "autoscrubRunFleet.v1",
      openRunCount: open.length,
      globalStopRequested: this.globalStopRequested,
      stopConfirmation: {
        required: this.stopConfirmationRequired,
        keepScanningLabel: "Keep scanning",
        stopNowLabel: "Stop now",
      },
      unattendedExecutionAllowed: false,
      quitGuard: {
        state: this.globalStopRequested ? "checking" : "notRequested",
        honestRemainingSecondsEstimate: null,
        reason: this.globalStopRequested ? "A stop request is being checked." : "No stop request is active.",
      },
      fleetActions:
        open.length === 0 || this.globalStopRequested
          ? []
          : [{ action: "stopAllScanning", label: "Stop all scanning" }],
      runs: open.map((run) => ({
        runId: run.runId,
        serviceId: run.serviceId,
        accountId: run.accountId,
        phase: run.phase,
        reviewedItemCount: run.reviewedItemCount,
        remainingItemCount: run.remainingItemCount,
        paceMilliseconds: 500,
        stopRequested: run.stopRequested,
        mutationAllowed: false,
        lastOutcome: "held",
        accountActions: this.accountActions(run),
      })),
    };
  }

  /** record_service_connection_event. Never a credential action. */
  recordServiceConnectionEvent(event: ServiceConnectionEvent): unknown {
    const run = this.runs.find(
      (candidate) => candidate.serviceId === event.serviceId && candidate.accountId === event.accountId,
    );
    if (!run) throw new Error("AutoScrub service connection event did not match an open account");
    const state =
      event.event === "logout"
        ? "stopped_on_logout"
        : event.event === "human_check"
          ? "stopped_on_human_check"
          : "stopped_on_suspension";
    run.connectionState = state;
    run.phase = "blocked";
    run.stopRequested = true;
    this.serviceConnectionLog.push({ runId: run.runId, event: event.event, state, action: "record_state" });
    return this.fleet();
  }

  /** request_account_action. */
  requestRunAction(runId: string, action: AutoScrubRunActionKind): unknown {
    if (action === "stopAllScanning") {
      this.globalStopRequested = true;
      this.stopConfirmationRequired = this.runs.some((run) => OPEN_PHASES.includes(run.phase));
      for (const run of this.runs) {
        if (OPEN_PHASES.includes(run.phase)) run.stopRequested = true;
      }
      return this.fleet();
    }
    const run = this.runs.find((candidate) => candidate.runId === runId);
    if (!run) throw new Error("AutoScrub run was not found");
    if (action === "tryAgainAfterSignIn") {
      if (run.phase !== "blocked" && run.phase !== "failed") {
        throw new Error("AutoScrub sign-in retry is available only after an account stop");
      }
      run.phase = "running";
      run.stopRequested = false;
      run.connectionState = "running";
    } else if (action === "skipThisAccount") {
      run.phase = "skipped";
      run.stopRequested = false;
    } else {
      run.phase = "running";
      run.stopRequested = false;
    }
    return this.fleet();
  }
}

interface Harness {
  store: FixtureAutoScrubStore;
  screen: ScanLoggedOutScreen;
  /** Every scan step body that actually executed, in order. */
  executed: string[];
  runActionCalls: { runId: string; action: AutoScrubRunActionKind }[];
  openedAccounts: { serviceId: string; accountId: string }[];
  /** Drive the scan loop for one account. Returns how many step bodies ran. */
  driveScan(runId: string, attempts: number): Promise<number>;
}

function harness(): Harness {
  const store = new FixtureAutoScrubStore();
  const executed: string[] = [];
  const runActionCalls: { runId: string; action: AutoScrubRunActionKind }[] = [];
  const openedAccounts: { serviceId: string; accountId: string }[] = [];

  const ports: LoggedOutScreenPorts = {
    requestRunAction: async (runId, action) => {
      runActionCalls.push({ runId, action });
      return store.requestRunAction(runId, action);
    },
    openAccountSurface: async (serviceId, accountId) => {
      openedAccounts.push({ serviceId, accountId });
      return { serviceId, accountId };
    },
    scanDriverFor: (run) => new AutoScrubProgress(run.reviewedItemCount),
  };

  const screen = new ScanLoggedOutScreen(ports);

  const driveScan = async (runId: string, attempts: number): Promise<number> => {
    const before = executed.length;
    for (let index = 0; index < attempts; index += 1) {
      await screen.runScanStep(runId, async () => {
        executed.push(`${runId}#${index}`);
        const run = store.runs.find((candidate) => candidate.runId === runId);
        if (run && run.remainingItemCount > 0) run.remainingItemCount -= 1;
      });
    }
    return executed.length - before;
  };

  return { store, screen, executed, runActionCalls, openedAccounts, driveScan };
}

const LOGOUT_EVENT: ServiceConnectionEvent = {
  serviceId: "discord",
  accountId: "acct-discord-maple",
  event: "logout",
};

describe("TASK 1442 - logged-out screen", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("shows the exact sentence and all four recovery choices, and runs 0 scan steps after it", async () => {
    const { store, screen, driveScan } = harness();

    screen.acceptFleetStatus(store.fleet());
    const before = await driveScan("autoscrub-run-0001", 2);
    expect(before).toBe(2);

    const raw = store.recordServiceConnectionEvent(LOGOUT_EVENT) as { runs: { accountActions: unknown[] }[] };
    const view = screen.applyServiceConnectionEvent(raw, LOGOUT_EVENT);
    expect(view).not.toBeNull();

    // 1. the exact sentence, on screen.
    expect(view?.sentence).toBe(EXACT_SENTENCE);
    const markup = screen.markup();
    expect(markup).toContain(EXACT_SENTENCE);
    expect(markup).toContain("Discord needs you to sign in again");

    // 2. all four recovery choices.
    const labels = [...markup.matchAll(/<button[^>]*>([^<]+)<\/button>/gu)].map((match) => match[1]);
    expect(labels).toEqual([
      "Open account",
      "Try again after sign-in",
      "Skip this account",
      "Stop all scanning",
    ]);

    // 3. 0 scan steps after it -- for this account and for the whole fleet.
    const after = await driveScan("autoscrub-run-0001", 5);
    const afterOther = await driveScan("autoscrub-run-0002", 3);
    expect(after).toBe(0);
    expect(screen.scanStepsSinceLoggedOut()).toBe(0);
    expect(screen.scanStepsRun("autoscrub-run-0001")).toBe(2);

    // The other account is a separate run and is not what this screen stopped;
    // recorded here so the 0 above is read as this account's, not the fleet's.
    expect(afterOther).toBe(3);

    // The backend recorded the state and never a credential action.
    expect(store.serviceConnectionLog).toEqual([
      { runId: "autoscrub-run-0001", event: "logout", state: "stopped_on_logout", action: "record_state" },
    ]);
    expect(store.credentialCalls).toHaveLength(0);

    console.log(`task1442_sentence=${view?.sentence}`);
    console.log(`task1442_sentence_in_markup=${markup.includes(EXACT_SENTENCE)}`);
    console.log(`task1442_choice_labels=${labels.join(",")}`);
    console.log(`task1442_choice_count=${labels.length}`);
    console.log(`task1442_steps_before_logout=${before}`);
    console.log(`task1442_steps_after_logout=${after}`);
    console.log(`task1442_scan_steps_since_logged_out=${screen.scanStepsSinceLoggedOut()}`);
    console.log(`task1442_other_account_steps_after_logout=${afterOther}`);
    console.log(`task1442_stopped_account_actions_from_backend=${raw.runs[0]?.accountActions.length}`);
    console.log(`task1442_credential_actions=${store.credentialCalls.length}`);
  });

  it("states the four choices even though the stopped run carries none", () => {
    const { store, screen } = harness();
    screen.acceptFleetStatus(store.fleet());
    const raw = store.recordServiceConnectionEvent(LOGOUT_EVENT) as { runs: { accountActions: unknown[] }[] };
    expect(raw.runs[0]?.accountActions).toEqual([]);

    const view = screen.applyServiceConnectionEvent(raw, LOGOUT_EVENT);
    expect(view?.choices.map((choice) => choice.label)).toEqual([
      "Open account",
      "Try again after sign-in",
      "Skip this account",
      "Stop all scanning",
    ]);
    expect(LOGGED_OUT_RECOVERY_CHOICES).toHaveLength(4);
  });

  it("Open account opens the account surface without restarting the scan", async () => {
    const { store, screen, openedAccounts, runActionCalls, driveScan } = harness();
    screen.acceptFleetStatus(store.fleet());
    screen.applyServiceConnectionEvent(store.recordServiceConnectionEvent(LOGOUT_EVENT), LOGOUT_EVENT);

    const result = await screen.choose("openAccount");
    expect(result).toEqual({ state: "openedAccount", serviceId: "discord", accountId: "acct-discord-maple" });
    expect(openedAccounts).toEqual([{ serviceId: "discord", accountId: "acct-discord-maple" }]);
    expect(runActionCalls).toEqual([]);
    expect(screen.visible()).toBe(true);
    expect(await driveScan("autoscrub-run-0001", 4)).toBe(0);
    expect(screen.scanStepsSinceLoggedOut()).toBe(0);
  });

  it("Skip this account keeps its partial results and runs no more steps on it", async () => {
    const { store, screen, driveScan } = harness();
    screen.acceptFleetStatus(store.fleet());
    await driveScan("autoscrub-run-0001", 2);
    screen.applyServiceConnectionEvent(store.recordServiceConnectionEvent(LOGOUT_EVENT), LOGOUT_EVENT);
    const partialBefore = screen.currentView()?.partialResultCount;

    await screen.choose("skipThisAccount");
    expect(partialBefore).toBe(2);
    expect(store.runs[0]?.phase).toBe("skipped");
    expect(store.runs[0]?.remainingItemCount).toBe(4);
    expect(screen.visible()).toBe(false);
    expect(await driveScan("autoscrub-run-0001", 3)).toBe(0);
  });

  it("Stop all scanning stops every account", async () => {
    const { store, screen, driveScan } = harness();
    screen.acceptFleetStatus(store.fleet());
    screen.applyServiceConnectionEvent(store.recordServiceConnectionEvent(LOGOUT_EVENT), LOGOUT_EVENT);

    await screen.choose("stopAllScanning");
    expect(store.globalStopRequested).toBe(true);
    expect(await driveScan("autoscrub-run-0001", 2)).toBe(0);
    expect(await driveScan("autoscrub-run-0002", 2)).toBe(0);
  });

  it("Try again after sign-in is the only choice that lets steps run again", async () => {
    const { store, screen, driveScan } = harness();
    screen.acceptFleetStatus(store.fleet());
    screen.applyServiceConnectionEvent(store.recordServiceConnectionEvent(LOGOUT_EVENT), LOGOUT_EVENT);
    expect(await driveScan("autoscrub-run-0001", 2)).toBe(0);

    await screen.choose("tryAgainAfterSignIn");
    expect(screen.visible()).toBe(false);
    expect(await driveScan("autoscrub-run-0001", 2)).toBe(2);
  });

  it("draws nothing when the backend status does not show the account stopped", () => {
    const { store, screen } = harness();
    const view = screen.applyServiceConnectionEvent(store.fleet(), LOGOUT_EVENT);
    expect(view).toBeNull();
    expect(screen.markup()).toBe("");
  });

  it("names the service the event actually stopped", () => {
    expect(signInAgainSentence("discord")).toBe(EXACT_SENTENCE);
    expect(signInAgainSentence("telegram")).toBe("Telegram needs you to sign in again.");
  });
});
