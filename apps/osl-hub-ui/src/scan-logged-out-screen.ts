import { openEmbeddedServiceAccount, type ServiceId } from "./services";
import { requestAutoScrubRunAction } from "./autoscrub-unattended-run";
import {
  deepFreeze,
  parseAutoScrubFleetStatus,
  type AutoScrubFleetStatus,
  type AutoScrubRunAction,
  type AutoScrubRunActionKind,
  type AutoScrubRunSummary,
} from "./autoscrub-contract";

/**
 * TASK 1442 - connect the logged-out screen.
 *
 * Gate 1440 (apps/osl-hub/src/autoscrub_run.rs, `record_service_connection_event`)
 * made a logout stop that one account: it sets the run to `Blocked`, sets
 * `stop_requested`, keeps `mutation_allowed` false, and writes a
 * state-recording log entry that is never a credential action. Gate 1441 added
 * the four bounded run actions and their exact labels
 * (`AutoScrubRunActionKind::label`). This module is the screen a person sees
 * when that happens, bound to those two shapes.
 *
 * Two facts about the backend decided the design here, and both were read out
 * of `autoscrub_run.rs` rather than assumed:
 *
 *   1. `account_actions_for` returns an EMPTY action list when the run has
 *      `stop_requested` set -- and gate 1440's logout path sets exactly that.
 *      So the fleet DTO for a logged-out account carries zero account actions
 *      and (via `fleet_actions_for`) only "Stop all scanning". The screen
 *      therefore STATES all four recovery choices from the contract's own
 *      action kinds instead of mirroring `accountActions`, which would draw a
 *      logged-out screen with one button on it.
 *
 *   2. `AutoScrubRunActionKind::OpenAccount` sets the run back to `Running`.
 *      On a logged-out account that would resume scanning against a signed-out
 *      session -- the one thing this task's bar forbids. The product wording
 *      for this choice is "opens the account so you can sign in yourself", so
 *      "Open account" is dispatched to `open_service_host` (the embedded
 *      account surface) and NOT to `request_autoscrub_run_action`. It never
 *      restarts the run and never touches a password or a code. "Try again
 *      after sign-in" stays the only choice that puts the run back to running,
 *      which is what gate 1441's `TryAgainAfterSignIn` arm is for.
 *
 * `record_service_connection_event` has no `#[tauri::command]` registration of
 * its own (it is a library function inside gate 1440's scope; the only
 * registered AutoScrub action command is `request_autoscrub_run_action`,
 * main.rs:12749). So this connector CONSUMES the fleet status that function
 * returns rather than inventing a command name for it: the caller hands over
 * the raw status plus the event kind that produced it, and the screen is drawn
 * from a strict re-parse of that status, never from what the caller claimed.
 *
 * The scanning side of the bar is enforced, not described. The screen owns the
 * per-account scan driver and stops it the moment the logged-out state is
 * accepted, so every later `runScanStep` executes nothing and
 * `scanStepsSinceLoggedOut` stays 0.
 */

/** The service-connection events gate 1440 accepts, in its own wire spelling. */
export type ServiceConnectionEventKind = "logout" | "human_check" | "suspension";

/** `AutoScrubServiceConnectionEvent` from gate 1440, camelCase as serde emits it. */
export interface ServiceConnectionEvent {
  readonly serviceId: ServiceId;
  readonly accountId: string;
  readonly event: ServiceConnectionEventKind;
}

/**
 * One scan driver for one account. `AutoScrubProgress` (autoscrub-progress.ts)
 * satisfies this structurally; the fixture in the test does too.
 */
export interface ScanStepDriver {
  requestStop(): unknown;
  runNext(step: () => Promise<void>): Promise<unknown>;
}

export interface LoggedOutRecoveryChoice {
  readonly action: AutoScrubRunActionKind;
  readonly label: AutoScrubRunAction["label"];
  /** Where the press goes: the account surface, or gate 1441's run action. */
  readonly dispatch: "openAccountSurface" | "runAction";
}

/**
 * All four recovery choices, in the order the product states them. The labels
 * are typed as `AutoScrubRunAction["label"]`, so they cannot drift from the
 * strings gate 1441's `label()` produces without failing to compile.
 */
export const LOGGED_OUT_RECOVERY_CHOICES: readonly LoggedOutRecoveryChoice[] = deepFreeze([
  { action: "openAccount", label: "Open account", dispatch: "openAccountSurface" },
  { action: "tryAgainAfterSignIn", label: "Try again after sign-in", dispatch: "runAction" },
  { action: "skipThisAccount", label: "Skip this account", dispatch: "runAction" },
  { action: "stopAllScanning", label: "Stop all scanning", dispatch: "runAction" },
] satisfies LoggedOutRecoveryChoice[]);

const SERVICE_NAMES: Readonly<Record<ServiceId, string>> = Object.freeze({
  discord: "Discord",
  telegram: "Telegram",
  email: "Email",
  signal: "Signal",
  whatsapp: "WhatsApp",
});

/** The exact sentence this screen exists to say. */
export function signInAgainSentence(serviceId: ServiceId): string {
  return `${SERVICE_NAMES[serviceId]} needs you to sign in again.`;
}

/** What the other two stop reasons say. Same screen, same four choices. */
export function serviceConnectionSentence(serviceId: ServiceId, event: ServiceConnectionEventKind): string {
  const name = SERVICE_NAMES[serviceId];
  if (event === "logout") return signInAgainSentence(serviceId);
  if (event === "human_check") return `${name} is asking you to finish a check yourself.`;
  return `${name} has suspended this account, so OSL stopped it.`;
}

export interface LoggedOutScreenView {
  readonly runId: string;
  readonly serviceId: ServiceId;
  readonly accountId: string;
  readonly event: ServiceConnectionEventKind;
  readonly sentence: string;
  readonly choices: readonly LoggedOutRecoveryChoice[];
  /** Restated from the parsed status, not from what the caller assumed. */
  readonly partialResultCount: number;
}

export const LOGGED_OUT_CHOICE_SELECTOR = "[data-logged-out-choice]";

function escapeAttr(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/"/gu, "&quot;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
}

function escapeText(value: string): string {
  return value.replace(/&/gu, "&amp;").replace(/</gu, "&lt;").replace(/>/gu, "&gt;");
}

/** The screen itself. Nothing is drawn when no account is stopped. */
export function loggedOutScreenMarkup(view: LoggedOutScreenView | null): string {
  if (!view) return "";
  const runId = escapeAttr(view.runId);
  const buttons = view.choices
    .map((choice) =>
      `<button type="button" data-logged-out-choice="${escapeAttr(choice.action)}"` +
      ` data-logged-out-run="${runId}">${escapeText(choice.label)}</button>`)
    .join("");
  return (
    `<section class="scan-logged-out" data-logged-out-run="${runId}"` +
    ` data-logged-out-event="${escapeAttr(view.event)}">` +
    `<p class="scan-logged-out-message">${escapeText(view.sentence)}</p>` +
    `<p class="scan-logged-out-partial">OSL kept the ${view.partialResultCount} ` +
    `${view.partialResultCount === 1 ? "result" : "results"} it already found on this account.</p>` +
    `<p class="scan-logged-out-credentials">OSL never enters a password or a sign-in code for you.</p>` +
    `<div class="scan-logged-out-choices">${buttons}</div>` +
    `</section>`
  );
}

export interface LoggedOutScreenPorts {
  /** Gate 1441's registered command, through the checked wrapper. */
  requestRunAction(runId: string, action: AutoScrubRunActionKind): Promise<unknown>;
  /** `open_service_host` -- opens the account so the person signs in themselves. */
  openAccountSurface(serviceId: ServiceId, accountId: string): Promise<unknown>;
  /** One scan driver per run. Stopped the moment the account is logged out. */
  scanDriverFor(run: AutoScrubRunSummary): ScanStepDriver;
}

class StoppedScanDriver implements ScanStepDriver {
  requestStop(): unknown {
    return undefined;
  }

  async runNext(): Promise<unknown> {
    return { state: "stopped" };
  }
}

export const LOGGED_OUT_SCREEN_PORTS: LoggedOutScreenPorts = {
  requestRunAction: (runId, action) => requestAutoScrubRunAction(runId, action),
  openAccountSurface: (serviceId, accountId) => openEmbeddedServiceAccount(serviceId, accountId),
  scanDriverFor: () => new StoppedScanDriver(),
};

export type LoggedOutChoiceResult =
  | { readonly state: "refused"; readonly reason: string }
  | { readonly state: "openedAccount"; readonly serviceId: ServiceId; readonly accountId: string }
  | { readonly state: "sent"; readonly action: AutoScrubRunActionKind };

export class ScanLoggedOutScreen {
  private status: AutoScrubFleetStatus | null = null;
  private view: LoggedOutScreenView | null = null;
  private readonly drivers = new Map<string, ScanStepDriver>();
  private readonly stepsByRun = new Map<string, number>();
  private stepsAfterScreen = 0;

  constructor(private readonly ports: LoggedOutScreenPorts = LOGGED_OUT_SCREEN_PORTS) {}

  /** Take a fleet status from the backend and give every open run a driver. */
  acceptFleetStatus(raw: unknown): AutoScrubFleetStatus {
    const status = parseAutoScrubFleetStatus(raw);
    this.status = status;
    for (const run of status.runs) {
      if (!this.drivers.has(run.runId)) this.drivers.set(run.runId, this.ports.scanDriverFor(run));
    }
    return status;
  }

  /**
   * The logged-out entry point: the raw fleet status gate 1440 returned after
   * it recorded the event, plus the event kind that produced it.
   *
   * The state is re-read from the parsed status. If that status does not
   * actually show the account stopped, nothing is drawn and no driver is
   * stopped -- a screen that appears on a claim rather than on the recorded
   * state would be worse than no screen.
   */
  applyServiceConnectionEvent(raw: unknown, event: ServiceConnectionEvent): LoggedOutScreenView | null {
    const status = this.acceptFleetStatus(raw);
    const run = status.runs.find(
      (candidate) => candidate.serviceId === event.serviceId && candidate.accountId === event.accountId,
    );
    if (!run) return null;
    if (run.phase !== "blocked" || !run.stopRequested || run.mutationAllowed !== false) return null;

    this.drivers.get(run.runId)?.requestStop();
    this.stepsAfterScreen = 0;
    this.view = Object.freeze({
      runId: run.runId,
      serviceId: run.serviceId,
      accountId: run.accountId,
      event: event.event,
      sentence: serviceConnectionSentence(run.serviceId, event.event),
      choices: LOGGED_OUT_RECOVERY_CHOICES,
      partialResultCount: run.reviewedItemCount - run.remainingItemCount,
    });
    return this.view;
  }

  currentView(): LoggedOutScreenView | null {
    return this.view;
  }

  markup(): string {
    return loggedOutScreenMarkup(this.view);
  }

  visible(): boolean {
    return this.view !== null;
  }

  /** Scan steps that actually executed for one account, over its whole life. */
  scanStepsRun(runId: string): number {
    return this.stepsByRun.get(runId) ?? 0;
  }

  /**
   * Scan steps that executed on the LOGGED-OUT account since its screen went
   * up. This is the bar. It is deliberately that account's count and not the
   * fleet's: a logout stops one account and the run moves on to the next
   * approved one, so a fleet-wide zero would be a stronger claim than the
   * product makes. Ending the whole run is what "Stop all scanning" is for,
   * and that path stops every driver.
   */
  scanStepsSinceLoggedOut(): number {
    return this.stepsAfterScreen;
  }

  /**
   * Run one scan step for one account. The step body only runs if that
   * account's driver still allows it, and every execution is counted where the
   * caller cannot forge it.
   */
  async runScanStep(runId: string, step: () => Promise<void>): Promise<unknown> {
    const driver = this.drivers.get(runId);
    if (!driver) return { state: "stopped" };
    let ran = 0;
    const result = await driver.runNext(async () => {
      ran += 1;
      await step();
    });
    if (ran > 0) {
      this.stepsByRun.set(runId, (this.stepsByRun.get(runId) ?? 0) + ran);
      if (this.view && this.view.runId === runId) this.stepsAfterScreen += ran;
    }
    return result;
  }

  /** Press one of the four choices. */
  async choose(action: AutoScrubRunActionKind): Promise<LoggedOutChoiceResult> {
    const view = this.view;
    if (!view) return { state: "refused", reason: "no logged-out account is on screen" };
    const choice = LOGGED_OUT_RECOVERY_CHOICES.find((candidate) => candidate.action === action);
    if (!choice) return { state: "refused", reason: `unknown recovery choice ${String(action)}` };

    if (choice.dispatch === "openAccountSurface") {
      await this.ports.openAccountSurface(view.serviceId, view.accountId);
      // Opening the account is not a retry: the screen stays up and the run
      // stays stopped until the person chooses "Try again after sign-in".
      return { state: "openedAccount", serviceId: view.serviceId, accountId: view.accountId };
    }

    const next = await this.ports.requestRunAction(view.runId, action);
    if (next !== null && next !== undefined) {
      const status = this.acceptFleetStatus(next);
      const run = status.runs.find((candidate) => candidate.runId === view.runId);
      if (action === "tryAgainAfterSignIn" && run?.phase === "running" && !run.stopRequested) {
        // Signed back in: a fresh driver, and the screen comes down.
        this.drivers.set(run.runId, this.ports.scanDriverFor(run));
        this.stepsAfterScreen = 0;
        this.view = null;
      } else if (!run || run.phase === "skipped" || run.phase === "complete") {
        // Skipped or finished: gate 1441 keeps that account's partial results
        // and drops it out of the open fleet. Its driver never runs again.
        this.drivers.get(view.runId)?.requestStop();
        this.view = null;
      } else if (action === "stopAllScanning" || status.globalStopRequested) {
        for (const driver of this.drivers.values()) driver.requestStop();
        this.view = null;
      }
    }
    return { state: "sent", action };
  }

  fleetStatus(): AutoScrubFleetStatus | null {
    return this.status;
  }
}
