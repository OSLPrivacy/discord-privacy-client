import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import {
  parseAutoScrubFleetStatus,
  parseAutoScrubReviewedRunRequest,
  type AutoScrubFleetStatus,
  type AutoScrubReviewedRunRequest,
} from "./autoscrub-contract";

export async function loadAutoScrubRunFleetStatus(): Promise<AutoScrubFleetStatus | null> {
  if (!isTauriRuntime()) return null;
  return parseAutoScrubFleetStatus(await invoke<unknown>("get_autoscrub_run_fl"));
}

export async function requestAutoScrubGlobalStop(): Promise<AutoScrubFleetStatus | null> {
  if (!isTauriRuntime()) return null;
  return parseAutoScrubFleetStatus(await invoke<unknown>("request_autoscrub_global_stop"));
}

export async function startAutoScrubReviewedRun(request: AutoScrubReviewedRunRequest): Promise<AutoScrubFleetStatus | null> {
  const reviewed = parseAutoScrubReviewedRunRequest(request);
  if (!isTauriRuntime()) return null;
  return parseAutoScrubFleetStatus(await invoke<unknown>("start_autoscrub_reviewed_run", { request: reviewed }));
}
