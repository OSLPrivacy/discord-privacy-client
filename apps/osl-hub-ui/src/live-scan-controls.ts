import { withNativeDeadline } from "./native-deadline";
import {
  keepScanningAfterAutoScrubStopRequest,
  pauseAutoScrubLiveUpdates,
  requestAutoScrubGlobalStop,
  resumeAutoScrubLiveUpdates,
  stopAutoScrubNowAfterStopRequest,
} from "./autoscrub-unattended-run";
import { projectAutoScrubFleetStatus, type AutoScrubFleetStatus } from "./autoscrub-contract";

export interface LiveScanControlsState {
  readonly status: AutoScrubFleetStatus | null;
  readonly paused: boolean;
  readonly label: string;
  readonly pending: null | "pause" | "resume" | "stopScrub" | "keepScanning" | "stopNow";
}

const INITIAL_STATE: LiveScanControlsState = {
  status: null,
  paused: false,
  label: "Unavailable in this build",
  pending: null,
};

let state: LiveScanControlsState = INITIAL_STATE;
const listeners = new Set<() => void>();

function setState(next: Partial<LiveScanControlsState>): void {
  state = { ...state, ...next };
  listeners.forEach((listener) => listener());
}

export function getLiveScanControlsState(): LiveScanControlsState {
  return state;
}

export function onLiveScanControlsChange(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function resetLiveScanControlsStateForTest(): void {
  state = INITIAL_STATE;
}

function labelForStatus(status: AutoScrubFleetStatus | null, paused: boolean): string {
  if (paused) return "Paused";
  return projectAutoScrubFleetStatus(status).label;
}

async function runCommand(
  pending: NonNullable<LiveScanControlsState["pending"]>,
  label: string,
  action: () => Promise<AutoScrubFleetStatus | null>,
): Promise<void> {
  setState({ pending });
  const status = await withNativeDeadline(action(), label, 2_000).catch(() => null);
  setState({ status, pending: null, label: labelForStatus(status, state.paused) });
}

export async function handleLiveScanPause(): Promise<void> {
  setState({ pending: "pause" });
  await pauseAutoScrubLiveUpdates();
  setState({ paused: true, pending: null, label: "Paused" });
}

export async function handleLiveScanResume(): Promise<void> {
  setState({ pending: "resume", paused: false });
  const status = await withNativeDeadline(resumeAutoScrubLiveUpdates(), "Resume AutoScrub", 2_000).catch(() => null);
  setState({ status, pending: null, label: labelForStatus(status, false) });
}

export async function handleLiveScanStopScrub(): Promise<void> {
  await runCommand("stopScrub", "Stop Scrub", requestAutoScrubGlobalStop);
}

export async function handleLiveScanKeepScanning(): Promise<void> {
  await runCommand("keepScanning", "Keep scanning", keepScanningAfterAutoScrubStopRequest);
}

export async function handleLiveScanStopNow(): Promise<void> {
  await runCommand("stopNow", "Stop now", stopAutoScrubNowAfterStopRequest);
}

const BUTTONS: ReadonlyArray<{
  readonly id: string;
  readonly label: string;
  readonly pending: NonNullable<LiveScanControlsState["pending"]>;
  readonly disabledWhen: (current: LiveScanControlsState) => boolean;
  readonly handler: () => Promise<void>;
}> = [
  {
    id: "live-scan-pause",
    label: "Pause",
    pending: "pause",
    disabledWhen: (current) => current.paused,
    handler: handleLiveScanPause,
  },
  {
    id: "live-scan-resume",
    label: "Resume",
    pending: "resume",
    disabledWhen: (current) => !current.paused,
    handler: handleLiveScanResume,
  },
  {
    id: "live-scan-stop-scrub",
    label: "Stop Scrub",
    pending: "stopScrub",
    disabledWhen: () => false,
    handler: handleLiveScanStopScrub,
  },
  {
    id: "live-scan-keep-scanning",
    label: "Keep scanning",
    pending: "keepScanning",
    disabledWhen: () => false,
    handler: handleLiveScanKeepScanning,
  },
  {
    id: "live-scan-stop-now",
    label: "Stop now",
    pending: "stopNow",
    disabledWhen: () => false,
    handler: handleLiveScanStopNow,
  },
];

export function liveScanControlsMarkup(): string {
  const buttons = BUTTONS.map((button) => {
    const disabled = button.disabledWhen(state) || state.pending === button.pending;
    return `<button class="button compact" id="${button.id}" type="button" ${disabled ? "disabled" : ""}>${button.label}</button>`;
  }).join("");
  return `<div class="live-scan-controls">${buttons}<p class="live-scan-controls-state" data-live-scan-label>${state.label}</p></div>`;
}

export function bindLiveScanControls(root: ParentNode = document): void {
  for (const button of BUTTONS) {
    root
      .querySelector<HTMLButtonElement>(`#${button.id}`)
      ?.addEventListener("click", () => void button.handler());
  }
}
